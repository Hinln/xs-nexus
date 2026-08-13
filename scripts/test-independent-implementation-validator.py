#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-independent-implementation.py")
SPEC = importlib.util.spec_from_file_location("validate_independent_implementation", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load independent implementation validator")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def main() -> int:
    assert MODULE.is_approved_reference(
        "scripts/windows/test-native-agent.ps1", "wintun"
    )
    assert not MODULE.is_approved_reference(
        "scripts/windows/runtime-agent.ps1", "wintun"
    )
    assert not MODULE.is_approved_reference(
        "scripts/windows/test-native-agent.ps1", "openvpn"
    )
    assert MODULE.is_approved_reference(
        "crates/windows-wintun/src/lib.rs", "wintun"
    )
    assert not MODULE.is_approved_reference(
        "crates/runtime/src/lib.rs", "wintun"
    )
    assert MODULE.is_approved_reference(
        "installers/windows/xs-nexus-one-click.ps1", "wireguard"
    )
    assert not MODULE.is_approved_reference(
        "scripts/windows/test-native-agent.ps1", "wireguard"
    )
    print("independent implementation validator regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
