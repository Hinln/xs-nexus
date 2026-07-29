#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import tempfile
from pathlib import Path


def load_scanner():
    module_path = Path(__file__).with_name("check-secrets.py")
    spec = importlib.util.spec_from_file_location("xs_secret_scanner", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load secret scanner")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def main() -> int:
    scanner = load_scanner()
    sensitive_name = b"to" + b"ken"

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        dynamic = root / "dynamic.rs"
        dynamic.write_bytes(
            b"let " + sensitive_name + b" = response[\"token\"].as_str();\n"
        )
        if scanner.scan(root, {}):
            raise RuntimeError("Rust non-literal assignment produced a false positive")

        dynamic.write_bytes(b"let key = EphemeralPrivateKey::from_bytes(value);\n")
        if scanner.scan(root, {}):
            raise RuntimeError("Rust private-key type path produced a false positive")

        dynamic.write_bytes(
            b"let " + sensitive_name + b" = \"not-a-production-value\";\n"
        )
        findings = scanner.scan(root, {})
        if [finding.rule for finding in findings] != ["secret-assignment"]:
            raise RuntimeError("Rust literal assignment was not detected")

        dynamic.unlink()
        config = root / "config.yml"
        config.write_bytes(b"api_" + b"sec" + b"ret: not-a-production-value\n")
        findings = scanner.scan(root, {})
        if [finding.rule for finding in findings] != ["secret-assignment"]:
            raise RuntimeError("generic literal assignment was not detected")

        reference_value = b"reference-value-only-for-scanner-test"
        config.write_bytes(b"safe: " + reference_value + b"\n")
        findings = scanner.scan(root, {"TEST_REFERENCE": reference_value})
        if [finding.rule for finding in findings] != ["reference:TEST_REFERENCE"]:
            raise RuntimeError("exact reference value was not detected")

    print("secret scanner regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
