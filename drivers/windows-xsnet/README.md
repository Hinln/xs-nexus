# xsnet Windows virtual NIC

`xsnet` is a clean-room Windows virtual IPv4 NIC for XS Nexus. It does not use Wintun, TAP-Windows, an NDIS sample, or any third-party tunneling core.

## Current status

- M6.1 is in progress.
- The versioned user/kernel ABI and canonical packet-batch validator are implemented and host-tested.
- The WDK/NetAdapterCx driver, INF, test-signed package, installer, and VM evidence are not complete.
- `BLK-001` blocks WDK build and VM validation; `BLK-004` blocks production signing.

## Boundary

The driver may create and operate one virtual NIC, exchange bounded IPv4 batches with the LocalSystem Agent, enforce handle ownership and lifecycle, and report link/queue state. Encryption, XSP/1, identity, ACL, routing policy, NAT, Relay, updates, and secrets remain in user mode.

The ABI rejects unknown versions, types, flags, noncanonical lengths, zero sequence numbers, oversized batches, gaps, overlaps, hidden trailing bytes, and packets outside 20–9000 bytes.

## Host ABI test

```bash
cmake -S drivers/windows-xsnet -B target/xsnet-abi -G Ninja \
  -DCMAKE_C_COMPILER=clang -DCMAKE_BUILD_TYPE=Release
cmake --build target/xsnet-abi
ctest --test-dir target/xsnet-abi --output-on-failure
```

Passing this test does not constitute a WDK build, driver installation, Driver Verifier result, or Windows compatibility claim.
