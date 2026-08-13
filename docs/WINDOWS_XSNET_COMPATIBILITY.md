# Windows xsnet compatibility and rollback boundary

## Current matrix

The first test package supports one exact runtime contract only:

| Agent ABI | Driver ABI | Result |
| --- | --- | --- |
| 1 | 1 | Eligible for Hello/Attach validation in the Windows VM |
| 1 | any other value | Reject and replace the handle/package; no fallback |
| any other value | 1 | Unsupported; do not install or start the pair |

The 32-byte message header is itself versioned. A v1 driver must parse the header before it can read the Hello version range, so the current Hello is not a cross-header-version negotiation channel. The Agent therefore sends ABI v1 as the header version and as both Hello minimum and maximum. A future ABI v2 must define and test an explicit compatibility or discovery mechanism; it must not silently widen the current range.

`DriverVer` is a package and driver-store identity, not the runtime ABI. The build manifest records the four-part INF version, exact ABI range `1..1`, and IPv4 capability. The test installer requires the operator to provide the expected `DriverVer`, verifies the INF before staging, verifies the staged driver-store version, and records that version with ABI v1 in protected install-state schema 2. No schema 1 state was ever executed on Windows; unknown or old state fails closed rather than being upgraded in place.

## Test installer scope

The current installer is clean-install only. It refuses any existing xsnet device or driver package and does not implement an in-place driver or Agent upgrade. Replacing a test package requires stopping the Agent, closing the device handle, running the exact-state uninstaller, rebooting when required by Windows, and installing the new package from an approved recovery baseline. VM recovery uses a snapshot. Dedicated physical-target recovery uses a verified external full-system image plus bootable media, disk-recovery material, and an onsite recovery operator. Failure recovery is whole-target restoration, not an unimplemented hot downgrade.

This boundary prevents a partially upgraded Agent/driver pair from being described as supported. It also means the M6.2 repeated install, failure rollback, and Agent/driver compatibility acceptance items remain incomplete until they run on the approved Windows target.

## Future production transaction

A production upgrade design must preserve the old signed package until the new pair completes all gates: signature and manifest validation, ABI compatibility preflight, Agent quiesce and handle close, driver update, reboot handling, fresh Hello/Attach/SetLink, ordinary-network health, and bounded observation. Failure must stop the new Agent, close the new handle, restore the exact prior signed driver and Agent, and re-run health checks. No rollback may select an unknown `oem#.inf`, an unsigned binary, a different ABI, or a package not recorded before the transaction.

No production upgrade or rollback implementation is claimed by this document.
