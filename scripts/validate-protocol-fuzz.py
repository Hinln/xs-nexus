#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import shlex
import sys
import tomllib
from pathlib import Path


MINIMUM_SECONDS_PER_TARGET = 180
REQUIRED_TARGETS = frozenset(
    {"credential", "data", "discovery", "handshake", "relay", "session_state"}
)
SESSION_STATE_TOKENS = (
    "ClientHelloSent::start",
    "ServerHelloSent::accept",
    ".accept_server_hello",
    ".accept_client_finish",
    ".accept_server_finish",
    ".into_data_plane",
    ".seal_ipv4",
    ".seal_control",
    ".open(",
    ".rotate_epoch",
    ".install_next_epoch",
    ".retire_previous_epoch",
    "key_update_payload",
    "verify_key_update_payload",
)
SESSION_RETRY_TOKENS = (
    "assert_ne!(client_update_retry_frame, client_update_frame)",
    "assert_ne!(server_update_retry_frame, server_update_frame)",
    "open(&client_update_retry_frame)",
    "open(&server_update_retry_frame)",
)


def read_text(path: Path, failures: list[str]) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        failures.append(f"{path}: unable to read: {error}")
        return ""


def manifest_targets(path: Path, failures: list[str]) -> dict[str, str]:
    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        failures.append(f"{path}: unable to parse: {error}")
        return {}

    targets: dict[str, str] = {}
    for entry in document.get("bin", []):
        name = entry.get("name")
        source = entry.get("path")
        if not isinstance(name, str) or not isinstance(source, str):
            failures.append(f"{path}: every fuzz bin needs string name and path")
            continue
        if name in targets:
            failures.append(f"{path}: duplicate fuzz target {name}")
            continue
        targets[name] = source
    return targets


def protocol_job(workflow: str) -> str | None:
    match = re.search(
        r"(?ms)^  protocol-fuzz:\s*\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\s*\n|\Z)",
        workflow,
    )
    return None if match is None else match.group("body")


def validate_repository(root: Path) -> list[str]:
    failures: list[str] = []
    manifest_path = root / "fuzz" / "Cargo.toml"
    script_path = root / "scripts" / "run-protocol-fuzz.sh"
    workflow_path = root / ".github" / "workflows" / "ci.yml"
    targets = manifest_targets(manifest_path, failures)

    missing_targets = sorted(REQUIRED_TARGETS - targets.keys())
    if missing_targets:
        failures.append(
            f"{manifest_path}: missing required targets: {' '.join(missing_targets)}"
        )

    for name, relative_source in sorted(targets.items()):
        source = root / "fuzz" / relative_source
        if not source.is_file():
            failures.append(f"{source}: fuzz target source is missing")
        corpus = root / "fuzz" / "corpus" / name
        if not corpus.is_dir() or not any(path.is_file() for path in corpus.rglob("*")):
            failures.append(f"{corpus}: non-empty seed corpus is required")

    session_source = read_text(
        root / "fuzz" / "fuzz_targets" / "session_state.rs", failures
    )
    for token in SESSION_STATE_TOKENS:
        if token not in session_source:
            failures.append(
                "fuzz/fuzz_targets/session_state.rs: "
                f"missing lifecycle operation {token}"
            )
    for token in SESSION_RETRY_TOKENS:
        if token not in session_source:
            failures.append(
                "fuzz/fuzz_targets/session_state.rs: "
                f"missing fresh-sequence retry invariant {token}"
            )
    if "unsafe" in session_source:
        failures.append("fuzz/fuzz_targets/session_state.rs: unsafe is forbidden")

    script = read_text(script_path, failures)
    target_match = re.search(r"(?m)^readonly TARGETS=\((?P<targets>[^)]*)\)$", script)
    if target_match is None:
        failures.append(f"{script_path}: unable to parse TARGETS")
        script_targets: list[str] = []
    else:
        try:
            script_targets = shlex.split(target_match.group("targets"))
        except ValueError as error:
            failures.append(f"{script_path}: invalid TARGETS: {error}")
            script_targets = []
    if set(script_targets) != set(targets):
        failures.append(f"{script_path}: TARGETS must exactly match fuzz/Cargo.toml")
    if len(script_targets) != len(set(script_targets)):
        failures.append(f"{script_path}: TARGETS contains duplicates")

    seconds_match = re.search(
        r'(?m)^readonly FUZZ_SECONDS="\$\{FUZZ_SECONDS:-(?P<seconds>[0-9]+)\}"$',
        script,
    )
    script_seconds = 0
    if seconds_match is None:
        failures.append(f"{script_path}: unable to parse FUZZ_SECONDS default")
    else:
        script_seconds = int(seconds_match.group("seconds"))
        if script_seconds < MINIMUM_SECONDS_PER_TARGET:
            failures.append(
                f"{script_path}: FUZZ_SECONDS default must be at least "
                f"{MINIMUM_SECONDS_PER_TARGET}"
            )

    required_script_fragments = (
        'readonly FUZZ_SANITIZER="${FUZZ_SANITIZER:-address}"',
        'readonly CARGO_FUZZ_VERSION="0.13.2"',
        'if [[ "${FUZZ_SANITIZER}" != "address" ]]',
        'if [[ "${CARGO_FUZZ_ACTUAL_VERSION}" != "cargo-fuzz ${CARGO_FUZZ_VERSION}" ]]',
        'if [[ -e "${EVIDENCE_DIR}" ]]',
        "git rev-parse HEAD",
        "git status --porcelain=v1 --untracked-files=no",
        'rustc "+${NIGHTLY_TOOLCHAIN}" -vV',
        'cargo "+${NIGHTLY_TOOLCHAIN}" fmt \\\n    --manifest-path fuzz/Cargo.toml',
        'cargo "+${NIGHTLY_TOOLCHAIN}" clippy \\\n    --locked',
        'fuzz build \\\n    --sanitizer "${FUZZ_SANITIZER}"',
        'fuzz run \\\n        --sanitizer "${FUZZ_SANITIZER}"',
        "crash artifacts detected",
        "SHA256SUMS",
        "sha256sum",
    )
    for fragment in required_script_fragments:
        if fragment not in script:
            failures.append(f"{script_path}: missing fail-closed evidence control {fragment!r}")

    workflow = read_text(workflow_path, failures)
    job = protocol_job(workflow)
    if job is None:
        failures.append(f"{workflow_path}: protocol-fuzz job is missing")
    else:
        timeout_match = re.search(r"(?m)^    timeout-minutes:\s*([0-9]+)\s*$", job)
        if timeout_match is None or int(timeout_match.group(1)) < 45:
            failures.append(
                f"{workflow_path}: protocol-fuzz timeout must be at least 45 minutes"
            )
        run_match = re.search(
            r"FUZZ_SECONDS=([0-9]+)\s+FUZZ_SANITIZER=([a-z0-9_-]+)\s+"
            r"make test-protocol-fuzz",
            job,
        )
        if run_match is None:
            failures.append(f"{workflow_path}: explicit fuzz duration/sanitizer is missing")
        else:
            ci_seconds = int(run_match.group(1))
            if ci_seconds < MINIMUM_SECONDS_PER_TARGET or ci_seconds < script_seconds:
                failures.append(
                    f"{workflow_path}: CI fuzz duration cannot weaken the script default"
                )
            if run_match.group(2) != "address":
                failures.append(f"{workflow_path}: protocol fuzz must use address sanitizer")
        required_job_fragments = (
            "toolchain: nightly-2026-08-01",
            "components: rustfmt, clippy",
            "cargo install --locked cargo-fuzz --version 0.13.2",
        )
        for fragment in required_job_fragments:
            if fragment not in job:
                failures.append(
                    f"{workflow_path}: protocol fuzz job must retain {fragment!r}"
                )
        if not re.search(
            r"(?ms)name:\s*protocol-fuzz-evidence\s*\n.*?"
            r"if-no-files-found:\s*error\s*$",
            job,
        ):
            failures.append(
                f"{workflow_path}: protocol fuzz evidence upload must fail on missing files"
            )

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    arguments = parser.parse_args()
    failures = validate_repository(arguments.root.resolve())
    if failures:
        print("Protocol fuzz configuration validation failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("Protocol fuzz configuration validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
