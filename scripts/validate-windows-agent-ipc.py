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
    ['"crates/windows-local-ipc"', 'Win32_Security_Authorization'],
)
assert workspace.count('"crates/windows-local-ipc"') == 1

manifest = require(
    ROOT / "crates/windows-local-ipc/Cargo.toml",
    [
        'name = "xs-windows-local-ipc"',
        "[target.'cfg(windows)'.dependencies]",
        "tokio.workspace = true",
        "windows-sys.workspace = true",
        'unsafe_code = "deny"',
        'unsafe_op_in_unsafe_fn = "deny"',
    ],
)
assert "[dependencies]" not in manifest

library = require(
    ROOT / "crates/windows-local-ipc/src/lib.rs",
    [
        "#![deny(unsafe_code)]",
        '#[cfg(windows)]',
        "#[allow(unsafe_code)]",
        "mod platform;",
        "pub const AGENT_PIPE_NAME",
        r"\\.\pipe\xs-nexus-agent",
        "pub use platform::create_agent_pipe_server;",
    ],
)
assert "unsafe {" not in library

platform = require(
    ROOT / "crates/windows-local-ipc/src/platform.rs",
    [
        'const PRIVATE_PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)";',
        "const MAX_PIPE_INSTANCES: usize = 17;",
        "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        "SDDL_REVISION_1",
        "SECURITY_ATTRIBUTES",
        "LocalFree",
        ".first_pipe_instance(first_instance)",
        ".reject_remote_clients(true)",
        ".max_instances(MAX_PIPE_INSTANCES)",
        ".in_buffer_size(MAX_REQUEST_BYTES)",
        ".out_buffer_size(MAX_RESPONSE_BYTES)",
        "create_with_security_attributes_raw",
        "bInheritHandle: 0",
    ],
)
assert platform.count("unsafe {") == 3
for forbidden in ("PIPE_ACCEPT_REMOTE_CLIENTS", ";;;WD)", ";;;BU)", ";;;AU)", ";;;IU)"):
    assert forbidden not in platform, f"forbidden Windows IPC setting: {forbidden}"

agent_manifest = require(
    ROOT / "apps/agent/Cargo.toml",
    ['xs-windows-local-ipc = { path = "../../crates/windows-local-ipc" }'],
)
assert agent_manifest.count("xs-windows-local-ipc") == 1

ipc = require(
    ROOT / "apps/agent/src/ipc.rs",
    [
        '#[cfg(unix)]',
        "mod unix;",
        '#[cfg(windows)]',
        "mod windows;",
        "pub async fn run_ipc_server",
        "async fn handle_connection",
        "FRAME_PREFIX_BYTES",
        "u32::from_be_bytes(prefix)",
    ],
)
for forbidden in ("std::os::unix", "UnixListener", "UnixStream", "NamedPipeServer"):
    assert forbidden not in ipc, f"platform type leaked into shared IPC: {forbidden}"

require(
    ROOT / "apps/agent/src/ipc/unix.rs",
    ["UnixListener", "UnixStream", "ensure_private_directory", "SocketGuard"],
)
windows = require(
    ROOT / "apps/agent/src/ipc/windows.rs",
    [
        "create_agent_pipe_server(true)",
        "create_agent_pipe_server(false)",
        "server.connect()",
        "if permits.available_permits() > 0",
        "MAX_CONNECTIONS",
    ],
)
for forbidden in ("unsafe {", "std::thread", "tokio::time::sleep", "PIPE_ACCEPT_REMOTE_CLIENTS"):
    assert forbidden not in windows, f"forbidden Windows IPC behavior: {forbidden}"

config = require(
    ROOT / "apps/agent/src/config.rs",
    ['#[cfg(unix)]', '#[cfg(windows)]', "xs_windows_local_ipc::AGENT_PIPE_NAME"],
)
assert "agent.sock" in config

cli = require(
    ROOT / "apps/cli/src/main.rs",
    [
        '#[cfg(unix)]',
        '#[cfg(windows)]',
        "tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient}",
        r'const DEFAULT_SOCKET_PATH: &str = r"\\.\pipe\xs-nexus-agent";',
        "fn connect_local",
        "ClientOptions::new().read(true).write(true).open(path)",
        "std::future::ready",
        "FRAME_PREFIX_BYTES",
        "u32::from_be_bytes(prefix)",
    ],
)
assert cli.count("fn connect_local") == 2
assert "unsafe {" not in cli

print("Windows Agent IPC source validation passed")
