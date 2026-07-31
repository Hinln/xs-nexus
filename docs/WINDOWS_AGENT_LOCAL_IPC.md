# Windows Agent Local IPC

## Status

The Windows local management server boundary is implemented as source and cross-target compile preparation. It has not run on Windows and does not complete M6.1 or any item in `ACCEPTANCE.md` section K.

## Endpoint

- The endpoint is fixed to `\\.\pipe\xs-nexus-agent`; configuration cannot select an arbitrary pipe.
- The first server instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE` so startup fails if another process already owns the name.
- `PIPE_REJECT_REMOTE_CLIENTS` is always enabled.
- The protected DACL grants generic-all only to LocalSystem and built-in Administrators: `D:P(A;;GA;;;SY)(A;;GA;;;BA)`.
- The pipe handle is not inheritable.

The DACL conversion and the raw `SECURITY_ATTRIBUTES` call are isolated in `crates/windows-local-ipc`. The crate denies unsafe code except for the three counted Windows FFI blocks that convert the SDDL, create the Tokio pipe instance, and release the converted descriptor. Agent policy and request handling remain safe Rust.

## Protocol and Bounds

Windows and Unix use the same strict JSON request and response implementation. The protocol remains read-only and keeps the existing limits:

- request: 4 KiB;
- response: 512 KiB;
- active handlers: 16;
- peer rows: 512;
- per-connection I/O timeout: 2 seconds.

Windows permits 17 pipe instances: at most 16 connected handlers plus one listener. When all handler permits are occupied, the listener stops accepting rather than creating an unbounded task or terminating the server.

## Validation

`scripts/test-windows-agent-ipc.sh` runs:

- source invariants for the fixed name, DACL, first-instance flag, remote-client rejection, exact unsafe count, buffer limits, and platform separation;
- the existing Linux private-socket integration test to prove the refactor preserves behavior;
- the local IPC crate unit test;
- `x86_64-pc-windows-msvc` check and Clippy with warnings denied for the isolated Windows crate.

The complete Agent Windows check still stops in the TLS dependency `ring` because this server does not have the Windows SDK compiler and librarian. That external failure does not prove the Agent binary links or the pipe works at runtime.

## Remaining Gates

- Compile and link the complete Agent with the pinned Windows Rust toolchain and Windows SDK.
- Run as the intended Windows Service identity and inspect the effective named-pipe DACL.
- Verify LocalSystem and Administrators can connect while standard and remote users cannot.
- Exercise malformed, oversized, stalled, concurrent, disconnect, service-stop, sleep, and network-change cases.
- Add and validate the Windows CLI/client path and production installer transaction.
