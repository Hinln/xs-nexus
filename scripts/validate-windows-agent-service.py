#!/usr/bin/env python3

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def require(path: Path, values: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in values:
        if value not in text:
            raise AssertionError(f"{path.relative_to(ROOT)} missing required value: {value}")
    return text


workspace = require(
    ROOT / "Cargo.toml",
    ['"crates/windows-service"', '"Win32_System_Services"'],
)
assert workspace.count('"crates/windows-service"') == 1

manifest = require(
    ROOT / "crates/windows-service/Cargo.toml",
    [
        'name = "xs-windows-service"',
        "[target.'cfg(windows)'.dependencies]",
        "tokio.workspace = true",
        "windows-sys.workspace = true",
        'unsafe_code = "deny"',
        'unsafe_op_in_unsafe_fn = "deny"',
    ],
)
assert "[dependencies]" not in manifest

library = require(
    ROOT / "crates/windows-service/src/lib.rs",
    [
        "#![deny(unsafe_code)]",
        "MAX_SERVICE_NAME_UNITS",
        "validate_service_name",
        '#[cfg(windows)]',
        "#[allow(unsafe_code)]",
        "mod platform;",
        "pub use platform::{ServiceShutdown, run_service};",
    ],
)
assert "unsafe {" not in library

platform = require(
    ROOT / "crates/windows-service/src/platform.rs",
    [
        "StartServiceCtrlDispatcherW",
        "RegisterServiceCtrlHandlerExW",
        "SetServiceStatus",
        "SERVICE_START_PENDING",
        "SERVICE_RUNNING",
        "SERVICE_STOP_PENDING",
        "SERVICE_STOPPED",
        "SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN",
        "SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN",
        "SERVICE_CONTROL_INTERROGATE",
        "STOP_REQUESTED.swap(true, Ordering::AcqRel)",
        "STOP_NOTIFY.notify_one()",
        "STATUS_LOCK",
        "catch_unwind",
        "ERROR_SERVICE_SPECIFIC_ERROR",
    ],
)
assert platform.count("unsafe {") == 4
for forbidden in (
    "CreateServiceW",
    "DeleteService",
    "ChangeServiceConfig",
    "std::thread",
    "tokio::time::sleep",
):
    assert forbidden not in platform, f"forbidden SCM runtime behavior: {forbidden}"

agent_manifest = require(
    ROOT / "apps/agent/Cargo.toml",
    ['xs-windows-service = { path = "../../crates/windows-service" }'],
)
assert agent_manifest.count("xs-windows-service") == 1

agent = require(
    ROOT / "apps/agent/src/main.rs",
    [
        'const WINDOWS_SERVICE_NAME: &str = "XsNexusAgent";',
        "Command::Service { config } => run_windows_service(config)",
        'command == "service"',
        "cfg!(windows)",
        "tokio::runtime::Handle::current()",
        "xs_windows_service::run_service(WINDOWS_SERVICE_NAME",
        "runtime.block_on(async move",
        "shutdown.cancelled().await",
        "watch::channel(false)",
        "run_agent(config, shutdown_receiver).await",
    ],
)
for forbidden in ("unsafe {", "CreateServiceW", "DeleteService", "ChangeServiceConfig"):
    assert forbidden not in agent, f"forbidden Agent service behavior: {forbidden}"

require(
    ROOT / "docs/WINDOWS_AGENT_SERVICE.md",
    [
        "XsNexusAgent",
        "START_PENDING",
        "RUNNING",
        "STOP_PENDING",
        "STOPPED",
        "LocalSystem",
        "Cross-target compilation does not prove SCM execution",
        "CreateServiceW",
    ],
)

print("Windows Agent Service source validation passed")
