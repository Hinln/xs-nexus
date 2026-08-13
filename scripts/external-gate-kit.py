#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, NamedTuple


REPOSITORY = Path(__file__).resolve().parent.parent
SCHEMA_VERSION = 1
REVISION = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
SAFE_NAME = re.compile(r"^[a-z0-9][a-z0-9-]{1,63}$")
FORBIDDEN_KEY_NAMES = {
    "authorization",
    "cookie",
    "credential_value",
    "passwd",
    "password",
    "private_key",
    "secret",
    "token",
}
FORBIDDEN_KEY_SUFFIXES = tuple(f"_{name}" for name in FORBIDDEN_KEY_NAMES)
FORBIDDEN_BYTES = (
    re.compile(rb"-----BEGIN (?:OPENSSH |RSA |EC |DSA )?PRIVATE KEY-----"),
    re.compile(rb"(?i)(?:postgres(?:ql)?|mysql|redis)://[^\s/@:]+:[^\s/@]+@"),
    re.compile(rb"(?i)authorization\s*:\s*bearer\s+\S+"),
    re.compile(rb"(?i)(?:password|passwd|secret|token|private[_-]?key)\s*[=:]\s*[^\s\"']{8,}"),
)
PROHIBITED_OOB_CHANNELS = {"ssh", "tofu", "same-session", "chat", "email-only"}
SSH_FINGERPRINT = re.compile(r"^SHA256:[A-Za-z0-9+/]{43}$")


class ValidationError(RuntimeError):
    pass


class GateSpec(NamedTuple):
    checks: tuple[str, ...]
    evidence_kinds: tuple[str, ...]
    public_fields: tuple[str, ...] = ()
    metrics: tuple[str, ...] = ()


GATES: dict[str, GateSpec] = {
    "host-identity": GateSpec(
        checks=(
            "control_plane_observation_recorded",
            "local_observation_recorded",
            "fingerprints_match",
            "strict_host_verification_preserved",
            "authentication_performed_only_after_confirmation",
        ),
        evidence_kinds=(
            "control-plane-fingerprint",
            "local-observed-fingerprint",
            "strict-connection-result",
        ),
        public_fields=(
            "approved_fingerprint",
            "observed_fingerprint",
            "out_of_band_channel",
        ),
    ),
    "credential-rotation": GateSpec(
        checks=(
            "owner_inventory_complete",
            "all_replacements_activated",
            "all_old_values_rejected",
            "dependent_clients_migrated",
            "post_rotation_regression_passed",
            "all_surfaces_scanned_without_values",
        ),
        evidence_kinds=(
            "rotation-register",
            "old-value-rejection",
            "post-rotation-regression",
            "surface-scan",
        ),
    ),
    "key-ceremony": GateSpec(
        checks=(
            "offline_environment_verified",
            "roles_separated",
            "dual_control_applied",
            "public_keys_authenticated",
            "rotation_and_revocation_tested",
            "backup_recovery_tested",
            "private_material_absent_from_evidence",
        ),
        evidence_kinds=(
            "ceremony-minutes",
            "public-fingerprints",
            "rotation-revocation",
            "recovery-result",
        ),
        public_fields=("root_fingerprint", "update_fingerprint", "recovery_fingerprint"),
    ),
    "independent-security-audit": GateSpec(
        checks=(
            "reviewer_independent",
            "protocol_and_crypto_reviewed",
            "application_and_infrastructure_reviewed",
            "findings_received",
            "critical_and_high_fixed",
            "independent_retest_passed",
        ),
        evidence_kinds=("scope", "report", "finding-register", "independent-retest"),
        public_fields=("review_organization", "report_identifier"),
    ),
    "windows-real": GateSpec(
        checks=(
            "restorable_vm_snapshot_created",
            "package_signature_verified",
            "install_enrollment_and_traffic_passed",
            "direct_and_relay_passed",
            "reboot_sleep_network_switch_passed",
            "crash_route_and_dad_recovery_passed",
            "upgrade_and_rollback_passed",
            "driver_verifier_completed",
            "uninstall_reinstall_cleanup_passed",
            "no_bsod_or_residue",
        ),
        evidence_kinds=(
            "environment",
            "snapshot",
            "stage-logs",
            "network-before-after",
            "verifier-dump-summary",
            "cleanup",
        ),
        public_fields=("windows_build", "snapshot_identifier", "package_digest"),
    ),
    "nas-real": GateSpec(
        checks=(
            "package_signature_verified",
            "ordinary_node_lifecycle_passed",
            "direct_and_relay_passed",
            "subnet_route_approved",
            "allowed_and_denied_acl_passed",
            "route_withdrawal_passed",
            "upgrade_rollback_and_cleanup_passed",
        ),
        evidence_kinds=(
            "system-inventory",
            "package-verification",
            "ordinary-node",
            "subnet-acl",
            "lifecycle-cleanup",
        ),
        public_fields=("nas_model", "nas_os", "package_digest"),
    ),
    "wan-relay-subnet": GateSpec(
        checks=(
            "different_public_networks_used",
            "direct_path_passed",
            "udp_block_relay_fallback_passed",
            "path_migration_and_recovery_passed",
            "real_subnet_router_passed",
            "acl_bypass_rejected",
            "capacity_and_loss_boundaries_passed",
            "cleanup_passed",
        ),
        evidence_kinds=(
            "network-environment",
            "direct-relay",
            "path-migration",
            "subnet-acl",
            "capacity-loss",
            "cleanup",
        ),
    ),
    "planned-domain-tls": GateSpec(
        checks=(
            "baseline_and_rollback_captured",
            "origin_certificate_and_sni_passed",
            "cdn_strict_verification_enabled",
            "public_http_and_websocket_passed",
            "browser_matrix_passed",
            "unrelated_sites_and_onepanel_preserved",
            "independent_external_retest_passed",
        ),
        evidence_kinds=(
            "baseline",
            "origin-strict-tls",
            "cdn-configuration",
            "external-strict-audit",
            "browser-matrix",
            "post-change-regression",
        ),
        public_fields=("planned_domain", "certificate_fingerprint"),
    ),
    "external-alerting": GateSpec(
        checks=(
            "destination_independent",
            "warning_delivered",
            "critical_delivered",
            "on_call_acknowledged",
            "escalation_verified",
            "resolved_event_closed",
            "notification_failure_detected",
        ),
        evidence_kinds=(
            "provider-destination",
            "warning-delivery",
            "critical-delivery",
            "ack-escalation",
            "resolved-closure",
        ),
        public_fields=("provider", "destination_identifier", "exercise_identifier"),
    ),
    "offsite-recovery": GateSpec(
        checks=(
            "independent_failure_domain_used",
            "private_identity_absent_from_database_host",
            "offsite_receipt_verified",
            "deep_integrity_verified",
            "clean_server_restore_passed",
            "application_validation_passed",
            "rpo_and_rto_recorded",
            "temporary_environment_destroyed",
        ),
        evidence_kinds=(
            "offsite-receipt",
            "deep-verification",
            "clean-server-restore",
            "application-validation",
            "rpo-rto",
            "destruction-receipt",
        ),
        metrics=("rpo_seconds", "rto_seconds"),
    ),
    "formal-soak": GateSpec(
        checks=(
            "approved_identity_verified_host_used",
            "exact_revision_and_images_held_constant",
            "minimum_duration_completed",
            "all_fault_events_passed",
            "resource_growth_within_bounds",
            "host_and_onepanel_invariants_preserved",
            "cleanup_and_secret_scan_passed",
        ),
        evidence_kinds=(
            "summary",
            "resource-samples",
            "event-log",
            "host-invariants",
            "manifest",
            "secret-scan",
        ),
        metrics=("duration_seconds",),
    ),
    "formal-release-rehearsal": GateSpec(
        checks=(
            "owner_signed_rc_verified",
            "main_contains_exact_revision",
            "independent_operator_used",
            "fresh_non_hosted_server_used",
            "formal_secrets_used_without_disclosure",
            "fresh_install_and_runtime_matrix_passed",
            "production_upgrade_passed",
            "production_rollback_passed",
            "approved_release_restored_and_verified",
            "unrelated_resources_and_onepanel_preserved",
        ),
        evidence_kinds=(
            "signed-tag-and-bundle",
            "operator-receipt",
            "fresh-host-baseline",
            "fresh-install-runtime",
            "production-upgrade",
            "production-rollback",
            "restored-release",
            "final-cleanup",
        ),
        public_fields=("signed_tag", "bundle_digest", "operator_identifier"),
    ),
}


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare and validate no-secret XS Nexus external gate evidence kits"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init", help="create a new external gate kit")
    init_parser.add_argument("--output", type=Path, required=True)
    init_parser.add_argument("--revision", required=True)

    status_parser = subparsers.add_parser("status", help="validate and summarize a kit")
    status_parser.add_argument("--root", type=Path, required=True)
    status_parser.add_argument("--expected-revision")
    status_parser.add_argument("--require-complete", action="store_true")

    seal_parser = subparsers.add_parser("seal", help="seal completed gate receipts")
    seal_parser.add_argument("--root", type=Path, required=True)
    seal_parser.add_argument("--expected-revision", required=True)
    seal_parser.add_argument("--gate", choices=tuple(GATES))
    seal_parser.add_argument("--require-complete", action="store_true")

    verify_parser = subparsers.add_parser("verify", help="verify a sealed kit")
    verify_parser.add_argument("--root", type=Path, required=True)
    verify_parser.add_argument("--expected-revision", required=True)
    verify_parser.add_argument("--require-complete", action="store_true")

    return parser.parse_args()


def resolved(path: Path) -> Path:
    return path.expanduser().resolve()


def ensure_outside_repository(path: Path) -> None:
    try:
        path.relative_to(REPOSITORY)
    except ValueError:
        return
    raise ValidationError("external gate kits must be created outside the repository")


def validate_revision(value: str) -> str:
    normalized = value.strip().lower()
    if not REVISION.fullmatch(normalized):
        raise ValidationError("candidate revision must be a full 40-character lowercase SHA")
    return normalized


def receipt_template(gate: str, revision: str) -> dict[str, Any]:
    spec = GATES[gate]
    return {
        "schema": SCHEMA_VERSION,
        "gate": gate,
        "candidate_revision": revision,
        "status": "BLOCKED_EXTERNAL",
        "operator": {"identifier": "", "independent": False},
        "started_at_utc": None,
        "completed_at_utc": None,
        "checks": {name: False for name in spec.checks},
        "public": {name: "" for name in spec.public_fields},
        "metrics": {name: None for name in spec.metrics},
        "evidence": [],
        "notes": [],
    }


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def init_kit(output: Path, revision: str) -> dict[str, Any]:
    output = resolved(output)
    ensure_outside_repository(output)
    revision = validate_revision(revision)
    if output.exists() and any(output.iterdir()):
        raise ValidationError("output directory already exists and is not empty")
    output.mkdir(parents=True, exist_ok=True)
    receipts = output / "receipts"
    evidence = output / "evidence"
    receipts.mkdir()
    evidence.mkdir()
    for gate in GATES:
        write_json(receipts / f"{gate}.json", receipt_template(gate, revision))
        (evidence / gate).mkdir()
    write_json(
        output / "kit.json",
        {
            "schema": SCHEMA_VERSION,
            "candidate_revision": revision,
            "created_at_utc": utc_now(),
            "gate_count": len(GATES),
            "repository": "https://github.com/Hinln/xs-nexus",
            "contains_secret_values": False,
        },
    )
    (output / "README.md").write_text(
        "# XS Nexus External Gate Evidence Kit\n\n"
        "This directory must remain outside Git. Never place passwords, tokens, private keys, "
        "cookies, complete credential URIs, or unredacted authorization headers here.\n\n"
        "For each gate, add redacted evidence below `evidence/<gate>/`, update the matching "
        "receipt, set `status` to `COMPLETE`, and run the repository's `external-gate-kit.py "
        "seal` command. A completed check is an attestation backed by listed evidence, not a "
        "request to skip manual execution.\n",
        encoding="utf-8",
    )
    return {"root": str(output), "revision": revision, "gates": len(GATES)}


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"unable to read valid JSON from {path}: {error}") from error


def validate_layout(root: Path) -> None:
    if not root.is_dir() or root.is_symlink():
        raise ValidationError("kit root is missing, not a directory, or a symlink")
    sealed_files = {"SHA256SUMS", "summary.json"}
    actual_root = frozenset(path.name for path in root.iterdir())
    base_root = {"README.md", "evidence", "kit.json", "receipts"}
    if actual_root not in {frozenset(base_root), frozenset(base_root | sealed_files)}:
        raise ValidationError(
            f"kit root file set mismatch: expected base files with optional sealed pair, got {sorted(actual_root)}"
        )
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValidationError(f"kit contains a symlink: {path.relative_to(root)}")
    receipt_directory = root / "receipts"
    evidence_directory = root / "evidence"
    if not receipt_directory.is_dir() or not evidence_directory.is_dir():
        raise ValidationError("receipts and evidence must be directories")
    expected_receipts = {f"{gate}.json" for gate in GATES}
    actual_receipts = {path.name for path in receipt_directory.iterdir()}
    if actual_receipts != expected_receipts or any(
        not path.is_file() for path in receipt_directory.iterdir()
    ):
        raise ValidationError("receipt file set differs from the required gate set")
    expected_evidence = set(GATES)
    actual_evidence = {path.name for path in evidence_directory.iterdir()}
    if actual_evidence != expected_evidence or any(
        not path.is_dir() for path in evidence_directory.iterdir()
    ):
        raise ValidationError("evidence directory set differs from the required gate set")
    for path in evidence_directory.rglob("*"):
        if path.is_file():
            scan_evidence(path)
        elif not path.is_dir():
            raise ValidationError(f"unsupported evidence object: {path.relative_to(root)}")


def validate_no_secret_json(value: Any, location: str = "receipt") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized_key = str(key).lower()
            if normalized_key in FORBIDDEN_KEY_NAMES or normalized_key.endswith(
                FORBIDDEN_KEY_SUFFIXES
            ):
                raise ValidationError(f"{location} contains forbidden secret field {key!r}")
            validate_no_secret_json(item, f"{location}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            validate_no_secret_json(item, f"{location}[{index}]")
    elif isinstance(value, str):
        encoded = value.encode("utf-8", errors="ignore")
        if any(pattern.search(encoded) for pattern in FORBIDDEN_BYTES):
            raise ValidationError(f"{location} contains secret-like material")


def require_exact_keys(value: dict[str, Any], expected: set[str], location: str) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise ValidationError(f"{location} key mismatch: missing={missing}, extra={extra}")


def validate_iso8601(value: Any, location: str, required: bool) -> None:
    if value is None and not required:
        return
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ValidationError(f"{location} must be an ISO-8601 UTC timestamp ending in Z")
    try:
        datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ValidationError(f"{location} is not a valid timestamp") from error


def safe_evidence_path(root: Path, gate: str, value: Any) -> Path:
    if not isinstance(value, str) or not value:
        raise ValidationError(f"{gate}: evidence path must be a non-empty relative string")
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValidationError(f"{gate}: unsafe evidence path {value!r}")
    target = (root / relative).resolve()
    allowed = (root / "evidence" / gate).resolve()
    try:
        target.relative_to(allowed)
    except ValueError as error:
        raise ValidationError(f"{gate}: evidence must stay below evidence/{gate}") from error
    if target.is_symlink() or not target.is_file():
        raise ValidationError(f"{gate}: evidence is missing, not regular, or a symlink: {value}")
    return target


def scan_evidence(path: Path) -> None:
    try:
        with path.open("rb") as evidence_file:
            overlap = b""
            while chunk := evidence_file.read(1024 * 1024):
                data = overlap + chunk
                if any(pattern.search(data) for pattern in FORBIDDEN_BYTES):
                    raise ValidationError(f"secret-like material detected in {path}")
                overlap = data[-4096:]
    except OSError as error:
        raise ValidationError(f"unable to scan evidence file {path}: {error}") from error


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def manifest_inputs(root: Path) -> list[Path]:
    paths: list[Path] = [root / "kit.json", root / "README.md", root / "summary.json"]
    paths.extend(sorted((root / "receipts").glob("*.json")))
    for gate in GATES:
        paths.extend(
            sorted(path for path in (root / "evidence" / gate).rglob("*") if path.is_file())
        )
    return paths


def validate_receipt(
    root: Path,
    gate: str,
    receipt: Any,
    expected_revision: str,
    require_complete: bool,
    verify_hashes: bool,
) -> str:
    if not isinstance(receipt, dict):
        raise ValidationError(f"{gate}: receipt must be an object")
    validate_no_secret_json(receipt, gate)
    expected_keys = {
        "schema",
        "gate",
        "candidate_revision",
        "status",
        "operator",
        "started_at_utc",
        "completed_at_utc",
        "checks",
        "public",
        "metrics",
        "evidence",
        "notes",
    }
    require_exact_keys(receipt, expected_keys, gate)
    if receipt["schema"] != SCHEMA_VERSION or receipt["gate"] != gate:
        raise ValidationError(f"{gate}: schema or gate identity mismatch")
    if receipt["candidate_revision"] != expected_revision:
        raise ValidationError(f"{gate}: candidate revision mismatch")
    status = receipt["status"]
    if status not in {"BLOCKED_EXTERNAL", "IN_PROGRESS", "COMPLETE"}:
        raise ValidationError(f"{gate}: invalid status {status!r}")
    if require_complete and status != "COMPLETE":
        raise ValidationError(f"{gate}: gate is not COMPLETE")

    operator = receipt["operator"]
    if not isinstance(operator, dict):
        raise ValidationError(f"{gate}: operator must be an object")
    require_exact_keys(operator, {"identifier", "independent"}, f"{gate}.operator")
    if not isinstance(operator["identifier"], str) or not isinstance(operator["independent"], bool):
        raise ValidationError(f"{gate}: invalid operator fields")

    spec = GATES[gate]
    checks = receipt["checks"]
    if not isinstance(checks, dict):
        raise ValidationError(f"{gate}: checks must be an object")
    require_exact_keys(checks, set(spec.checks), f"{gate}.checks")
    if any(not isinstance(value, bool) for value in checks.values()):
        raise ValidationError(f"{gate}: all checks must be booleans")

    public = receipt["public"]
    metrics = receipt["metrics"]
    if not isinstance(public, dict) or not isinstance(metrics, dict):
        raise ValidationError(f"{gate}: public and metrics must be objects")
    require_exact_keys(public, set(spec.public_fields), f"{gate}.public")
    require_exact_keys(metrics, set(spec.metrics), f"{gate}.metrics")

    evidence = receipt["evidence"]
    notes = receipt["notes"]
    if not isinstance(evidence, list) or not isinstance(notes, list):
        raise ValidationError(f"{gate}: evidence and notes must be arrays")
    if any(not isinstance(note, str) for note in notes):
        raise ValidationError(f"{gate}: notes must contain strings")
    observed_kinds: set[str] = set()
    observed_paths: set[str] = set()
    for index, entry in enumerate(evidence):
        if not isinstance(entry, dict):
            raise ValidationError(f"{gate}: evidence entry {index} must be an object")
        require_exact_keys(entry, {"kind", "path", "sha256"}, f"{gate}.evidence[{index}]")
        kind = entry["kind"]
        path_value = entry["path"]
        if not isinstance(kind, str) or not SAFE_NAME.fullmatch(kind):
            raise ValidationError(f"{gate}: invalid evidence kind {kind!r}")
        if kind in observed_kinds or path_value in observed_paths:
            raise ValidationError(f"{gate}: duplicate evidence kind or path")
        observed_kinds.add(kind)
        observed_paths.add(path_value)
        target = safe_evidence_path(root, gate, path_value)
        scan_evidence(target)
        digest = entry["sha256"]
        if digest is not None and (not isinstance(digest, str) or not SHA256.fullmatch(digest)):
            raise ValidationError(f"{gate}: invalid evidence SHA-256")
        if verify_hashes and digest != file_sha256(target):
            raise ValidationError(f"{gate}: evidence digest mismatch for {path_value}")

    validate_iso8601(receipt["started_at_utc"], f"{gate}.started_at_utc", status == "COMPLETE")
    validate_iso8601(receipt["completed_at_utc"], f"{gate}.completed_at_utc", status == "COMPLETE")

    if status == "COMPLETE":
        if not operator["identifier"].strip():
            raise ValidationError(f"{gate}: completed receipt requires an operator identifier")
        if not all(checks.values()):
            raise ValidationError(f"{gate}: completed receipt contains false checks")
        missing_kinds = set(spec.evidence_kinds) - observed_kinds
        if missing_kinds:
            raise ValidationError(f"{gate}: missing evidence kinds {sorted(missing_kinds)}")
        if any(not isinstance(public[name], str) or not public[name].strip() for name in spec.public_fields):
            raise ValidationError(f"{gate}: completed receipt has empty public identifiers")
        if gate in {"independent-security-audit", "formal-release-rehearsal"} and not operator["independent"]:
            raise ValidationError(f"{gate}: completed gate requires an independent operator")
        if gate == "host-identity":
            if not SSH_FINGERPRINT.fullmatch(public["approved_fingerprint"]) or not SSH_FINGERPRINT.fullmatch(
                public["observed_fingerprint"]
            ):
                raise ValidationError("host-identity: fingerprints must be OpenSSH SHA256 values")
            if public["approved_fingerprint"] != public["observed_fingerprint"]:
                raise ValidationError("host-identity: approved and observed fingerprints differ")
            if public["out_of_band_channel"].strip().lower() in PROHIBITED_OOB_CHANNELS:
                raise ValidationError("host-identity: channel is not independently out of band")
        if gate == "formal-soak":
            duration = metrics["duration_seconds"]
            if not isinstance(duration, int) or isinstance(duration, bool) or duration < 86400:
                raise ValidationError("formal-soak: duration_seconds must be at least 86400")
        if gate == "offsite-recovery":
            for name in spec.metrics:
                value = metrics[name]
                if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                    raise ValidationError(f"offsite-recovery: {name} must be a non-negative integer")
    return status


def load_kit(root: Path, expected_revision: str | None) -> tuple[str, dict[str, dict[str, Any]]]:
    root = resolved(root)
    ensure_outside_repository(root)
    validate_layout(root)
    kit = load_json(root / "kit.json")
    if not isinstance(kit, dict):
        raise ValidationError("invalid kit metadata")
    require_exact_keys(
        kit,
        {
            "schema",
            "candidate_revision",
            "created_at_utc",
            "gate_count",
            "repository",
            "contains_secret_values",
        },
        "kit",
    )
    if kit["schema"] != SCHEMA_VERSION:
        raise ValidationError("invalid kit schema")
    validate_iso8601(kit["created_at_utc"], "kit.created_at_utc", True)
    revision = validate_revision(str(kit.get("candidate_revision", "")))
    if expected_revision is not None and revision != validate_revision(expected_revision):
        raise ValidationError("kit revision does not match --expected-revision")
    if (
        kit.get("gate_count") != len(GATES)
        or kit.get("contains_secret_values") is not False
        or kit.get("repository") != "https://github.com/Hinln/xs-nexus"
    ):
        raise ValidationError("kit metadata boundary mismatch")
    receipts: dict[str, dict[str, Any]] = {}
    receipt_directory = root / "receipts"
    for gate in GATES:
        receipts[gate] = load_json(receipt_directory / f"{gate}.json")
    return revision, receipts


def status_kit(root: Path, expected_revision: str | None, require_complete: bool) -> dict[str, Any]:
    root = resolved(root)
    revision, receipts = load_kit(root, expected_revision)
    counts = {"BLOCKED_EXTERNAL": 0, "IN_PROGRESS": 0, "COMPLETE": 0}
    for gate, receipt in receipts.items():
        status = validate_receipt(
            root,
            gate,
            receipt,
            revision,
            require_complete,
            verify_hashes=receipt.get("status") == "COMPLETE",
        )
        counts[status] += 1
    return {
        "root": str(root),
        "candidate_revision": revision,
        "counts": counts,
        "all_complete": counts["COMPLETE"] == len(GATES),
        "production_gate_result": "READY_FOR_FRESH_AUDIT" if counts["COMPLETE"] == len(GATES) else "NO_GO",
    }


def seal_kit(
    root: Path,
    expected_revision: str,
    selected_gate: str | None,
    require_complete: bool,
) -> dict[str, Any]:
    root = resolved(root)
    revision, receipts = load_kit(root, expected_revision)
    selected = [selected_gate] if selected_gate else list(GATES)
    sealed: list[str] = []
    for gate in selected:
        receipt = receipts[gate]
        if receipt.get("status") != "COMPLETE":
            if selected_gate or require_complete:
                raise ValidationError(f"{gate}: only COMPLETE receipts can be sealed")
            continue
        validate_receipt(root, gate, receipt, revision, True, verify_hashes=False)
        for entry in receipt["evidence"]:
            entry["sha256"] = file_sha256(safe_evidence_path(root, gate, entry["path"]))
        write_json(root / "receipts" / f"{gate}.json", receipt)
        validate_receipt(root, gate, receipt, revision, True, verify_hashes=True)
        sealed.append(gate)
    if not sealed:
        raise ValidationError("no completed gate receipts were available to seal")

    summary = status_kit(root, revision, require_complete)
    summary["sealed_at_utc"] = utc_now()
    summary["sealed_gates"] = sealed
    persisted_summary = {key: value for key, value in summary.items() if key != "root"}
    write_json(root / "summary.json", persisted_summary)

    lines = []
    for path in manifest_inputs(root):
        if path.is_symlink():
            raise ValidationError(f"manifest input is a symlink: {path}")
        lines.append(f"{file_sha256(path)}  {path.relative_to(root).as_posix()}")
    (root / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return summary


def verify_kit(root: Path, expected_revision: str, require_complete: bool) -> dict[str, Any]:
    root = resolved(root)
    summary = status_kit(root, expected_revision, require_complete)
    manifest = root / "SHA256SUMS"
    if manifest.is_symlink() or not manifest.is_file():
        raise ValidationError("sealed kit SHA256SUMS is missing, not regular, or a symlink")
    try:
        lines = manifest.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise ValidationError(f"unable to read SHA256SUMS: {error}") from error
    expected_paths = {path.relative_to(root).as_posix(): path for path in manifest_inputs(root)}
    observed_paths: set[str] = set()
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  ([^\r\n]+)", line)
        if match is None:
            raise ValidationError("SHA256SUMS contains an invalid line")
        digest, relative_value = match.groups()
        if relative_value in observed_paths:
            raise ValidationError("SHA256SUMS contains a duplicate path")
        path = Path(relative_value)
        if path.is_absolute() or ".." in path.parts or relative_value == "SHA256SUMS":
            raise ValidationError("SHA256SUMS contains an unsafe path")
        expected_path = expected_paths.get(relative_value)
        if expected_path is None:
            raise ValidationError(f"SHA256SUMS contains an unexpected path: {relative_value}")
        if expected_path.is_symlink() or not expected_path.is_file():
            raise ValidationError(f"manifest target is missing or unsafe: {relative_value}")
        if file_sha256(expected_path) != digest:
            raise ValidationError(f"manifest digest mismatch: {relative_value}")
        observed_paths.add(relative_value)
    if observed_paths != set(expected_paths):
        missing = sorted(set(expected_paths) - observed_paths)
        raise ValidationError(f"SHA256SUMS file set differs from the kit: missing={missing}")
    stored_summary = load_json(root / "summary.json")
    if not isinstance(stored_summary, dict):
        raise ValidationError("summary.json must contain an object")
    validate_no_secret_json(stored_summary, "summary")
    require_exact_keys(
        stored_summary,
        {
            "candidate_revision",
            "counts",
            "all_complete",
            "production_gate_result",
            "sealed_at_utc",
            "sealed_gates",
        },
        "summary",
    )
    if stored_summary.get("candidate_revision") != summary["candidate_revision"]:
        raise ValidationError("summary revision mismatch")
    if stored_summary.get("counts") != summary["counts"]:
        raise ValidationError("summary status counts differ from the receipts")
    if stored_summary.get("all_complete") != summary["all_complete"]:
        raise ValidationError("summary completion status differs from the receipts")
    if stored_summary.get("production_gate_result") != summary["production_gate_result"]:
        raise ValidationError("summary gate result differs from the receipts")
    validate_iso8601(stored_summary.get("sealed_at_utc"), "summary.sealed_at_utc", True)
    sealed_gates = stored_summary.get("sealed_gates")
    if not isinstance(sealed_gates, list) or any(gate not in GATES for gate in sealed_gates):
        raise ValidationError("summary sealed_gates is invalid")
    return {
        **summary,
        "manifest_entries": len(observed_paths),
        "manifest_verified": True,
    }


def main() -> int:
    arguments = parse_arguments()
    try:
        if arguments.command == "init":
            result = init_kit(arguments.output, arguments.revision)
        elif arguments.command == "status":
            result = status_kit(
                arguments.root, arguments.expected_revision, arguments.require_complete
            )
        elif arguments.command == "seal":
            result = seal_kit(
                arguments.root,
                arguments.expected_revision,
                arguments.gate,
                arguments.require_complete,
            )
        else:
            result = verify_kit(
                arguments.root, arguments.expected_revision, arguments.require_complete
            )
    except ValidationError as error:
        print(f"external gate kit validation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
