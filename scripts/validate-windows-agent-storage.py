#!/usr/bin/env python3

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def require(path: Path, values: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in values:
        if value not in text:
            raise AssertionError(f"{path.relative_to(ROOT)} missing required value: {value}")
    return text


require(ROOT / "Cargo.toml", ['"crates/windows-private-storage"'])
require(
    ROOT / "crates/windows-private-storage/Cargo.toml",
    [
        'name = "xs-windows-private-storage"',
        "[target.'cfg(windows)'.dependencies]",
        "windows-sys.workspace = true",
        'unsafe_code = "deny"',
    ],
)
library = require(
    ROOT / "crates/windows-private-storage/src/lib.rs",
    [
        "#![deny(unsafe_code)]",
        "#[allow(unsafe_code)]",
        "mod platform;",
        "pub use platform::{ensure_private_directory, read_private, write_private_atomic};",
    ],
)
assert "unsafe {" not in library

platform = require(
    ROOT / "crates/windows-private-storage/src/platform.rs",
    [
        'const PRIVATE_FILE_SDDL: &str = "D:P(A;;FA;;;SY)(A;;FA;;;BA)";',
        'const PRIVATE_DIRECTORY_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";',
        "FILE_ATTRIBUTE_REPARSE_POINT",
        "PROTECTED_DACL_SECURITY_INFORMATION",
        "SE_DACL_PROTECTED",
        "GetNamedSecurityInfoW",
        "SetNamedSecurityInfoW",
        "GetSecurityDescriptorDacl",
        "GetSecurityDescriptorControl",
        "ReplaceFileW",
        "REPLACEFILE_WRITE_THROUGH",
        "MoveFileExW",
        "MOVEFILE_WRITE_THROUGH",
        "create_new(true)",
        "file.sync_all()",
        "validate_path_chain",
        "verify_private_acl(parent, PathKind::Directory)",
        "verify_private_acl",
    ],
)
for forbidden in ("MOVEFILE_REPLACE_EXISTING", ";;;WD)", ";;;BU)", ";;;AU)", ";;;IU)"):
    assert forbidden not in platform, f"forbidden Windows storage setting: {forbidden}"
assert platform.count("unsafe {") == 10

agent_manifest = require(
    ROOT / "apps/agent/Cargo.toml",
    ['xs-windows-private-storage = { path = "../../crates/windows-private-storage" }'],
)
assert agent_manifest.count("xs-windows-private-storage") == 1

storage = require(
    ROOT / "apps/agent/src/storage.rs",
    [
        '#[cfg(unix)]',
        "mod unix;",
        '#[cfg(windows)]',
        "mod windows;",
        "platform::read_private",
        "platform::write_private_atomic",
        "platform::ensure_private_directory",
    ],
)
shared_storage = storage.split("#[cfg(test)]", maxsplit=1)[0]
for forbidden in ("std::os::unix", "PermissionsExt", "OpenOptionsExt", "unsafe {"):
    assert forbidden not in shared_storage, f"platform detail leaked into shared storage: {forbidden}"

require(
    ROOT / "apps/agent/src/storage/unix.rs",
    ["PermissionsExt", "OpenOptionsExt", "PRIVATE_MODE", "DIRECTORY_MODE"],
)
windows = require(
    ROOT / "apps/agent/src/storage/windows.rs",
    [
        "xs_windows_private_storage::ensure_private_directory",
        "xs_windows_private_storage::read_private",
        "xs_windows_private_storage::write_private_atomic",
    ],
)
assert "unsafe {" not in windows

print("Windows Agent private storage source validation passed")
