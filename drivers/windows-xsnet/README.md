# xsnet Windows virtual NIC

`xsnet` is a clean-room Windows virtual IPv4 NIC for XS Nexus. It does not use Wintun, TAP-Windows, an NDIS sample, or any third-party tunneling core.

## Current status

- M6.1 has completed all source/cross-target work available on the Linux host and is blocked on the external WDK/Windows VM gate.
- The versioned Agent/driver ABI, canonical packet-batch validator, single-owner session state machine, and bounded packet queue are implemented and host-tested.
- The first installable target is Windows 11 24H2 UMDF 2.33 + NetAdapterCx 2.5; the Windows 10 support gap is tracked as `KI-016`.
- The WDK/NetAdapterCx source, INF, strict test-package builder, clean-install test scripts and VM evidence orchestrator are present; no package has been built or signed with WDK, and no installer or driver path has run in a VM.
- The checked-in UMDF source has bounded ring-copy callbacks and synchronous direct-I/O Agent requests; it remains unverified until WDK build and VM validation.
- Runtime compatibility is exact ABI v1. Test package replacement is clean-install only; see `docs/WINDOWS_XSNET_COMPATIBILITY.md`.
- `BLK-001` blocks WDK build and VM validation; `BLK-004` blocks production signing.

## Boundary

The driver may create and operate one virtual NIC, exchange bounded IPv4 batches with the LocalSystem Agent, enforce handle ownership and lifecycle, and report link/queue state. Encryption, XSP/1, identity, ACL, routing policy, NAT, Relay, updates, and secrets remain in user mode.

The ABI rejects unknown versions, types, flags, noncanonical lengths, zero sequence numbers, oversized batches, gaps, overlaps, hidden trailing bytes, and packets outside 20–9000 bytes.

The portable queue has a fixed 64-packet capacity per direction, validates canonical batches and raw IPv4 version/IHL/total length before mutating state, preserves packets when output is too small, and zeroes complete slots after dequeue or reset. NetAdapterCx callbacks copy only through system-managed buffers and `Layer2TypeNull`. The driver also enforces the smaller negotiated queue depths.

`IOCTL_XSNET_DEQUEUE_TX` takes a buffered 32-byte header-only TxBatch request and returns a direct framed TxBatch. `IOCTL_XSNET_ENQUEUE_RX` takes a framed RxBatch in the direct buffer and no buffered input. Empty/full/too-small requests return immediately without waiting and without advancing the session sequence.

`IOCTL_XSNET_QUERY_IDENTITY` is deliberately outside the six-message packet/session ABI. On the same exclusive device handle, it accepts an exact 8-byte identity schema v1 request and returns an exact 16-byte response containing the nonzero interface LUID obtained from `NetAdapterGetNetLuid`. It rejects unknown versions, nonzero reserved fields, wrong lengths, unavailable adapters, and zero LUIDs. The Agent must not substitute interface names or global adapter enumeration for this binding.

The deterministic host stress test runs 30,000 cases in each parser, writer, session, and queue domain. It checks arbitrary input under ASan/UBSan, verifies that rejected session operations preserve every state field, and mutates valid Hello, Attach, SetLink, TxBatch, RxBatch, and Detach messages.

The portable lifecycle harness exhausts all 720 orderings of cleanup, TX cancel, RX cancel, D0 exit, hardware release, and I/O stop from an active session. It verifies fail-closed link state, queue/session reset, cancellation-versus-completion single ownership, sleep reauthentication, queue restart, and idempotent teardown. This models callbacks serialized by the driver's single wait lock; it is not WDF scheduling, PnP, power, or Agent-crash evidence.

## Host ABI test

```bash
cmake -S drivers/windows-xsnet -B target/xsnet-abi -G Ninja \
  -DCMAKE_C_COMPILER=clang -DCMAKE_BUILD_TYPE=Release
cmake --build target/xsnet-abi
ctest --test-dir target/xsnet-abi --output-on-failure
```

Passing this test does not constitute a WDK build, driver installation, Driver Verifier result, or Windows compatibility claim.

`make test-windows-xsnet-abi` runs both the strict Clang Release build and the GCC ASan/UBSan build, including deterministic stress and lifecycle interleaving tests. `make test-windows-xsnet-source` additionally checks source, project, INF, access, lifecycle, and test-registration invariants. Neither command substitutes for MSBuild, InfVerif, signing, or VM testing.
