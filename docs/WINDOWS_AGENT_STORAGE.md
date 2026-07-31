# Windows Agent Private Storage

## Status

The Windows private storage boundary is implemented as source and cross-target compile preparation. It has not executed on Windows and does not complete M6.1 or any item in `ACCEPTANCE.md` section K.

## Security Contract

- Every state path must be absolute.
- Every existing component in the path chain must reject `FILE_ATTRIBUTE_REPARSE_POINT`.
- A private directory must be a real directory with a protected DACL containing exactly LocalSystem and built-in Administrators, with object/container inheritance.
- A private file must be a regular file with a protected DACL containing exactly LocalSystem and built-in Administrators.
- Reads verify the direct parent directory DACL, file DACL, object type, reparse state, bounded length, opened-handle length, and exact bytes read.
- Writes create a new same-directory temporary file, apply and re-read the exact private DACL before writing, flush file contents, then use write-through `ReplaceFileW` or non-overwriting `MoveFileExW`.
- Existing targets must already satisfy the private file contract before replacement. Failure removes only the generated temporary path.

The DACLs are:

```text
file:      D:P(A;;FA;;;SY)(A;;FA;;;BA)
directory: D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)
```

The implementation compares the complete ACL bytes returned by Windows with the ACL produced from the fixed SDDL and separately requires `SE_DACL_PROTECTED`. It does not accept merely equivalent-looking mode bits or inherited broad access.

## Isolation

All Windows security descriptor and atomic replacement FFI is isolated in `crates/windows-private-storage`. The Agent storage module remains safe Rust and shares identity, JSON, enrollment-token, size, parsing, and temporary-name logic across platforms. Existing Unix mode checks and directory fsync remain in the Unix transport.

## Validation

`scripts/test-windows-agent-storage.sh` runs:

- source invariants for fixed DACLs, protected-DACL verification, path-chain reparse rejection, direct-parent verification, same-directory `create_new`, flush, and write-through replacement;
- existing Agent identity/storage tests on Linux;
- `x86_64-pc-windows-msvc` check and Clippy with warnings denied for the isolated Windows crate.

## Remaining Gates

- Run create, read, replacement, crash, power-loss, ACL drift, junction, symlink, mount-point, hard-link, concurrent replacement, and oversized-file cases on Windows.
- Inspect effective owner/DACL and inheritance under the final Windows Service identity and installer-created ProgramData path.
- Verify enrollment-token handoff and secure deletion policy in the production installer.
- Compile and link the complete Windows Agent with the pinned SDK before enabling Windows runtime paths.
