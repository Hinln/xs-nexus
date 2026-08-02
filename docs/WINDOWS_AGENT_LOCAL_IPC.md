# Windows Agent Local IPC

## Status

The Windows local management server and `xs` named-pipe client boundaries are implemented as source and cross-target compile preparation. They have not run on Windows and do not complete M6.1 or any item in `ACCEPTANCE.md` section K.

## Endpoint

- The endpoint is fixed to `\\.\pipe\xs-nexus-agent`; configuration cannot select an arbitrary pipe.
- The first server instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE` so startup fails if another process already owns the name.
- `PIPE_REJECT_REMOTE_CLIENTS` is always enabled.
- The protected DACL grants generic-all only to LocalSystem and built-in Administrators: `D:P(A;;GA;;;SY)(A;;GA;;;BA)`.
- The pipe handle is not inheritable.

The DACL conversion and the raw `SECURITY_ATTRIBUTES` call are isolated in `crates/windows-local-ipc`. The crate denies unsafe code except for the three counted Windows FFI blocks that convert the SDDL, create the Tokio pipe instance, and release the converted descriptor. Agent policy and request handling remain safe Rust.

## Protocol and Bounds

Windows and Unix use the same strict, one-request-per-connection protocol. Each JSON request and response is prefixed by one unsigned 32-bit big-endian byte length, so a duplex Windows named pipe does not depend on a Unix-only half-close/EOF signal. Unknown JSON fields, zero or oversized lengths, truncated frames and response-type mismatches fail closed.

State, peer, path, route, netcheck and diagnostics requests are read-only. `ping` asks the running Agent to perform a bounded authenticated XSP/1 path probe; `reconnect` asks it to rebuild only the Controller WebSocket and is protected by a one-second manual cooldown, bounded runtime channel and explicit accepted/error response. Neither command exposes keys, credentials, raw configuration or arbitrary OS/network mutation.

The limits are:

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
- all nine CLI protocol tests, including failure exit status and response-type enforcement;
- the local IPC crate unit test;
- `x86_64-pc-windows-msvc` check and Clippy with warnings denied for both the isolated Windows server crate and `xs-cli` named-pipe client.

The complete Agent Windows check still stops in the TLS dependency `ring` because this server does not have the Windows SDK compiler and librarian. That external failure does not prove the Agent binary links or the pipe works at runtime.

## Remaining Gates

- Compile and link the complete Agent with the pinned Windows Rust toolchain and Windows SDK.
- Run as the intended Windows Service identity and inspect the effective named-pipe DACL.
- Verify LocalSystem and Administrators can connect while standard and remote users cannot.
- Exercise malformed, oversized, stalled, concurrent, disconnect, service-stop, sleep, and network-change cases.
- Execute the named-pipe CLI against the Service-hosted Agent on Windows, including frame truncation, all commands, cooldown, disconnect and service-stop cases; complete the production installer transaction separately.
