# Windows xsnet Agent transport

## Status

The safe transport contract is implemented in `apps/agent/src/windows_xsnet.rs`. The Win32 device transport is not implemented or compiled. This document is a build and review specification, not Windows execution evidence.

Runtime version and package replacement rules are defined in `WINDOWS_XSNET_COMPATIBILITY.md`. The current transport contract is eligible only for exact ABI v1; no cross-ABI fallback or hot upgrade is implied.

## Boundary

The Win32 implementation must live in a dedicated target-specific module or crate. It may only enumerate the `GUID_DEVINTERFACE_XSNET` interface, own one exclusive device handle, issue the six fixed IOCTLs, return owned response bytes, cancel or close the handle, and classify the result. It must not parse protocol state, inspect packets, perform cryptography, manage routes, log payloads, or retain borrowed request buffers.

The existing Agent crate remains `#![forbid(unsafe_code)]`. Any required Win32 `unsafe` must be isolated behind the `XsnetTransport` trait, use the smallest possible blocks, deny unsafe operations inside unsafe functions, and receive a separate source audit. The workspace lint must not be weakened globally to admit the transport.

## Handle lifecycle

1. Enumerate only the exact xsnet device-interface GUID.
2. Reject zero, duplicate, truncated, non-UTF-16, or ambiguous interface paths.
3. Open with read and write access, no sharing, no inheritance, and synchronous I/O for the first release.
4. Keep one owned handle per `XsnetClient`; a second active owner is an error.
5. Close the handle on cleanup, Agent crash, D0/device loss, malformed successful response, or any indeterminate result.
6. Reopen with a fresh `XsnetClient::opened()` and restart Hello/Attach/SetLink; never copy sequence or negotiated state across handles.

The first release must not use overlapped I/O, completion ports, shared memory, background retries, or a writable mapped ring. Those mechanisms require a new cancellation and ownership design before implementation.

## Buffer mapping

- Hello, Attach, SetLink, and Detach use the canonical framed message as the buffered input and require zero output bytes.
- Dequeue TX uses the 32-byte header-only request as buffered input and an owned direct output buffer bounded to `32 + 1 MiB`.
- Enqueue RX passes no buffered input. Its owned mutable direct buffer contains the complete framed RX batch; successful completion must report zero output bytes.
- The transport must check all Rust lengths before conversion to Win32 integer widths and must never expose uninitialized capacity as initialized bytes.
- Request memory remains alive and immovable until the synchronous call returns. Response bytes are copied into an owned vector before leaving the transport.

## Result classification

- `Success` is returned only when the Win32 call reports success and the byte count is within the supplied initialized buffer. The client then validates the full response before committing state.
- `Rejected` is permitted only for a status whose driver contract proves the request was not committed. The initial Win32 implementation should not infer this from a generic translated Win32 error.
- Every cancellation, device removal, short or impossible byte count, transport panic boundary, ambiguous OS error, or failed Win32 call is `Indeterminate` until Windows tests prove a narrower safe mapping.
- A malformed response after Win32 success is treated as indeterminate by `execute_prepared`, poisons the client, and forces handle replacement.

No result path may retry automatically. Known rejection reuses the same sequence only when the no-commit guarantee is authoritative; indeterminate results never guess whether the driver advanced sequence.

## Required validation

Before the Win32 transport can be connected to runtime, all of the following are mandatory:

- compile the actual Windows target with the pinned SDK and Rust toolchain;
- source-audit every unsafe block and handle conversion;
- run ABI/IOCTL fixed vectors against the built driver;
- inject every documented driver rejection and verify sequence reuse only for proven no-commit statuses;
- cancel and remove the device around every IOCTL and verify reconnect without double completion;
- run Agent crash, sleep/resume, network switch, repeated install/uninstall, and Driver Verifier in a snapshot VM;
- confirm ordinary physical networking remains available without the Agent and after every failure.

Until this evidence exists, M6.1 and every Windows acceptance item remain incomplete.
