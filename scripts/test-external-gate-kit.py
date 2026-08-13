#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("external-gate-kit.py")
SPEC = importlib.util.spec_from_file_location("external_gate_kit", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load external gate kit module")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
REVISION = "a" * 40


def read_receipt(root: Path, gate: str) -> dict:
    return json.loads((root / "receipts" / f"{gate}.json").read_text(encoding="utf-8"))


def write_receipt(root: Path, gate: str, receipt: dict) -> None:
    (root / "receipts" / f"{gate}.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def add_complete_evidence(root: Path, gate: str, receipt: dict) -> None:
    receipt["status"] = "COMPLETE"
    receipt["operator"] = {"identifier": "operator-01", "independent": True}
    receipt["started_at_utc"] = "2026-08-13T00:00:00Z"
    receipt["completed_at_utc"] = "2026-08-13T01:00:00Z"
    receipt["checks"] = {name: True for name in receipt["checks"]}
    receipt["public"] = {name: f"public-{name}" for name in receipt["public"]}
    receipt["metrics"] = {name: 1 for name in receipt["metrics"]}
    receipt["evidence"] = []
    for kind in MODULE.GATES[gate].evidence_kinds:
        relative = f"evidence/{gate}/{kind}.txt"
        (root / relative).write_text(f"redacted evidence for {kind}\n", encoding="utf-8")
        receipt["evidence"].append({"kind": kind, "path": relative, "sha256": None})


def expect_failure(function, fragment: str) -> None:
    try:
        function()
    except MODULE.ValidationError as error:
        assert fragment in str(error), error
    else:
        raise AssertionError("fixture unexpectedly passed")


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary) / "kit"
        initialized = MODULE.init_kit(root, REVISION)
        assert initialized["gates"] == 12
        status = MODULE.status_kit(root, REVISION, False)
        assert status["counts"] == {
            "BLOCKED_EXTERNAL": 12,
            "IN_PROGRESS": 0,
            "COMPLETE": 0,
        }
        assert status["production_gate_result"] == "NO_GO"
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, True),
            "gate is not COMPLETE",
        )

        host = read_receipt(root, "host-identity")
        add_complete_evidence(root, "host-identity", host)
        host["public"] = {
            "approved_fingerprint": "SHA256:" + "A" * 43,
            "observed_fingerprint": "SHA256:" + "A" * 43,
            "out_of_band_channel": "cloud-console",
        }
        write_receipt(root, "host-identity", host)
        sealed = MODULE.seal_kit(root, REVISION, "host-identity", False)
        assert sealed["sealed_gates"] == ["host-identity"]
        assert MODULE.status_kit(root, REVISION, False)["counts"]["COMPLETE"] == 1
        verified = MODULE.verify_kit(root, REVISION, False)
        assert verified["manifest_verified"] is True
        persisted_summary = json.loads((root / "summary.json").read_text(encoding="utf-8"))
        assert "root" not in persisted_summary

        host = read_receipt(root, "host-identity")
        host["evidence"][0]["sha256"] = "0" * 64
        write_receipt(root, "host-identity", host)
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, False),
            "evidence digest mismatch",
        )
        MODULE.seal_kit(root, REVISION, "host-identity", False)

        soak = read_receipt(root, "formal-soak")
        add_complete_evidence(root, "formal-soak", soak)
        soak["metrics"]["duration_seconds"] = 86399
        write_receipt(root, "formal-soak", soak)
        expect_failure(
            lambda: MODULE.seal_kit(root, REVISION, "formal-soak", False),
            "at least 86400",
        )
        soak["metrics"]["duration_seconds"] = 86400
        write_receipt(root, "formal-soak", soak)
        MODULE.seal_kit(root, REVISION, "formal-soak", False)
        MODULE.verify_kit(root, REVISION, False)

        credentials = read_receipt(root, "credential-rotation")
        credentials["password"] = "must-not-be-accepted"
        write_receipt(root, "credential-rotation", credentials)
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, False),
            "forbidden secret field",
        )
        del credentials["password"]
        write_receipt(root, "credential-rotation", credentials)

        host = read_receipt(root, "host-identity")
        host["public"]["out_of_band_channel"] = "tofu"
        write_receipt(root, "host-identity", host)
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, False),
            "not independently out of band",
        )

        host["public"]["out_of_band_channel"] = "cloud-console"
        write_receipt(root, "host-identity", host)
        MODULE.seal_kit(root, REVISION, "host-identity", False)
        (root / "README.md").write_text("tampered\n", encoding="utf-8")
        expect_failure(
            lambda: MODULE.verify_kit(root, REVISION, False),
            "manifest digest mismatch",
        )

        (root / "README.md").write_text("safe\n", encoding="utf-8")
        MODULE.seal_kit(root, REVISION, "host-identity", False)
        unlisted = root / "evidence" / "credential-rotation" / "unlisted.txt"
        unlisted.write_text("password=not-allowed-value\n", encoding="utf-8")
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, False),
            "secret-like material detected",
        )
        unlisted.unlink()

        (root / "unexpected.txt").write_text("unexpected\n", encoding="utf-8")
        expect_failure(
            lambda: MODULE.status_kit(root, REVISION, False),
            "root file set mismatch",
        )

    expect_failure(
        lambda: MODULE.init_kit(MODULE.REPOSITORY / "artifacts" / "invalid-kit", REVISION),
        "outside the repository",
    )
    print("external gate kit regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
