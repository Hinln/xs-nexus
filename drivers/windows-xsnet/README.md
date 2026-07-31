# xsnet Windows virtual NIC

`xsnet` is a clean-room Windows virtual IPv4 NIC for XS Nexus. It does not use Wintun, TAP-Windows, an NDIS sample, or any third-party tunneling core.

## Current status

- M6.1 is in progress.
- The versioned Agent/driver ABI, canonical packet-batch validator, single-owner session state machine, and bounded packet queue are implemented and host-tested.
- The first installable target is Windows 11 24H2 UMDF 2.33 + NetAdapterCx 2.5; the Windows 10 support gap is tracked as `KI-016`.
- The WDK/NetAdapterCx driver, INF, test-signed package, installer, and VM evidence are not complete.
- The checked-in UMDF source has bounded ring-copy callbacks and synchronous direct-I/O Agent requests; it remains unverified until WDK build and VM validation.
- `BLK-001` blocks WDK build and VM validation; `BLK-004` blocks production signing.

## Boundary

The driver may create and operate one virtual NIC, exchange bounded IPv4 batches with the LocalSystem Agent, enforce handle ownership and lifecycle, and report link/queue state. Encryption, XSP/1, identity, ACL, routing policy, NAT, Relay, updates, and secrets remain in user mode.

The ABI rejects unknown versions, types, flags, noncanonical lengths, zero sequence numbers, oversized batches, gaps, overlaps, hidden trailing bytes, and packets outside 20–9000 bytes.

The portable queue has a fixed 64-packet capacity per direction, validates canonical batches and raw IPv4 version/IHL/total length before mutating state, preserves packets when output is too small, and zeroes complete slots after dequeue or reset. NetAdapterCx callbacks copy only through system-managed buffers and `Layer2TypeNull`. The driver also enforces the smaller negotiated queue depths.

`IOCTL_XSNET_DEQUEUE_TX` takes a buffered 32-byte header-only TxBatch request and returns a direct framed TxBatch. `IOCTL_XSNET_ENQUEUE_RX` takes a framed RxBatch in the direct buffer and no buffered input. Empty/full/too-small requests return immediately without waiting and without advancing the session sequence.

## Host ABI test

```bash
cmake -S drivers/windows-xsnet -B target/xsnet-abi -G Ninja \
  -DCMAKE_C_COMPILER=clang -DCMAKE_BUILD_TYPE=Release
cmake --build target/xsnet-abi
ctest --test-dir target/xsnet-abi --output-on-failure
```

Passing this test does not constitute a WDK build, driver installation, Driver Verifier result, or Windows compatibility claim.

`make test-windows-xsnet-source` additionally checks source, project, INF, access, and lifecycle invariants. It is not a substitute for MSBuild, InfVerif, signing, or VM testing.
