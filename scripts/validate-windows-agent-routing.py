#!/usr/bin/env python3

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(path: Path, values: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in values:
        if value not in text:
            raise AssertionError(f"{path.relative_to(ROOT)} missing required value: {value}")
    return text


require(ROOT / "Cargo.toml", ['"crates/windows-route-manager"'])
require(
    ROOT / "apps/agent/Cargo.toml",
    ['xs-windows-route-manager = { path = "../../crates/windows-route-manager" }'],
)
require(
    ROOT / "apps/agent/src/lib.rs",
    ['#[cfg(windows)]', "pub mod windows_network;"],
)
source = require(
    ROOT / "apps/agent/src/windows_network.rs",
    [
        "pub struct WindowsNetworkPreparation",
        "pub fn recover_stale",
        "pub fn prepare",
        "pub fn shutdown",
        "write_network_manifest_atomic",
        "ManifestState::Preparing",
        "ManifestState::Active",
        "provision_network",
        "plan_recovery",
        "execute_recovery",
        "remove_network_manifest",
    ],
)
for forbidden in ("unsafe {", "tokio::spawn", "thread::spawn", "std::process::Command"):
    assert forbidden not in source, f"forbidden routing preparation behavior: {forbidden}"

runtime = (ROOT / "apps/agent/src/runtime.rs").read_text(encoding="utf-8")
assert "WindowsNetworkPreparation" not in runtime
assert "windows_network" not in runtime

platform = require(
    ROOT / "crates/windows-route-manager/src/platform.rs",
    [
        "GetIpForwardTable2",
        "FreeMibTable",
        "CreateIpForwardEntry2",
        "DeleteIpForwardEntry2",
        "CreateUnicastIpAddressEntry",
        "DeleteUnicastIpAddressEntry",
        "GetUnicastIpAddressEntry",
        "ERROR_NOT_FOUND",
        "read_private",
        "write_private_atomic",
        "remove_private",
    ],
)
assert platform.count("unsafe {") == 19

print("Windows Agent routing preparation source validation passed")
