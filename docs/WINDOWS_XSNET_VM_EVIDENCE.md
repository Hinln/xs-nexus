# Windows xsnet VM validation evidence

Validation date: 2026-08-04  
Scope: disposable Windows 11 24H2 x64 VM, test-signed `xsnet` driver only  
Result: accepted for the driver build/install/data-path/PnP/verifier/uninstall gate

This result does not authorize a production Windows release. The certificate was disposable and test-only. Production driver signing remains blocked by `BLK-004`, while the complete Windows Agent, SCM/Named Pipe/private-storage, IP Helper/DAD/route recovery, sleep and Windows 10 gates remain tracked by `KI-016`, `KI-017`, `KI-018` and `KI-020`.

## Environment

- Windows 11 IoT Enterprise LTSC 2024, x64, build `26100.8875`.
- Visual Studio 2022 Build Tools with amd64 MSBuild `17.14.51`.
- Windows SDK `10.0.26100.6901`, headers/libraries `10.0.26100.0`.
- WDK `10.0.26100.6584`, UMDF 2.33 and NetAdapterCx 2.5.
- Hypervisor snapshot assertion: `xs-nexus-pre-wdk-20260804-0805`.
- Disposable non-exportable test code-signing certificate; SHA-1 thumbprint `BA367AFF45DF2F1CD1873052E8FF280FB7ABF701`.

The certificate thumbprint is public package metadata, not a private key. The private key was created non-exportable in the disposable VM and was not copied into Git or the evidence export.

## Build result

The final test package has driver version `15.39.27.376` and exact ABI range `1..1`.

| File | SHA-256 |
|---|---|
| `xsnet.cat` | `62D5AC4A5FEE5C203CB577066AFEA4842D0C7E099739CF89EACEE15D4E9110A9` |
| `xsnet.dll` | `B63863E077348A6B0A413354538B31B6F5352142AFC4132EEEE8A9A08EC809EA` |
| `xsnet.inf` | `8F298BED8983E0CD0C584C619A825A5264AAF1CA269BDC3F6D1E79334EAA5A08` |

- Release x64 MSBuild completed with 0 warnings and 0 errors.
- InfVerif reported the INF valid.
- Inf2Cat generated the `10_GE_X64` catalog with no warning or error.
- SignTool verified the embedded DLL signature and catalog signature against the exact test signer.
- The package builder generated `manifest.json`; the local export was independently rehashed against every manifest entry.

## Failure captured before the final fix

Package `14.55.12.564` reproduced a PnP restart timeout under UMDF/Application Verifier. WDF identified `xsnet.dll`, verifier failure 414 (`0x19e`), and Windows Error Reporting recorded:

```text
LKD_0x15E_VRF_Mini_Nbl_Leak_IMAGE_netcxrd.sys
```

The failed implementation rewrote the transmit post and fragment ownership indices during cancellation. The fix follows the NetAdapterCx transmit cancellation contract: mark all packets in the completion range and advance only `packet_ring->BeginIndex`; never synthesize a `NextIndex` or fragment-ring advance. The source validator now rejects those ownership rewrites inside `cancel_transmit_rings`.

Failure evidence was exported separately under `work/windows-final-export/verifier-failure-003` before recovery.

## Accepted run

The append-only VM workflow used run ID `vm-validation-final-004`.

1. Proved zero pre-existing xsnet devices and packages.
2. Installed exactly one `ROOT\DEVGEN\XSNET` device and one `oem#.inf` package at version `15.39.27.376`.
3. Ran the SYSTEM smoke harness before restart. It proved:
   - one authoritative nonzero interface LUID;
   - Hello, Attach and LinkUp;
   - a deterministic IPv4 UDP packet entered TX (`transmit_batches=1`);
   - a bounded RX packet was accepted;
   - LinkDown, close, reopen and repeated session setup;
   - the interface LUID remained identical after reopen.
4. Restarted the PnP device in 285 ms with no verifier enabled, then repeated the full SYSTEM smoke successfully.
5. Enabled standard Driver Verifier flags `0x001209bb` for `xsnet.dll` in one-boot mode, rebooted, proved the active verifier list contained `xsnet.dll`, and repeated the full SYSTEM smoke successfully. The VM did not bugcheck.
6. Disabled standard Driver Verifier and rebooted.
7. Enabled UMDF `VerifierOn=1`, `VerifyDownLevel=1`, and Application Verifier checks for Heaps, Exceptions, Handles, Locks, Memory, TLS and Leak on `WUDFHost.exe`.
8. Repeated three complete PnP restart plus SYSTEM data-path cycles. Restart times were 1,443 ms, 2,113 ms and 877 ms; all three smoke runs passed.
9. After a WER settling interval, found zero new WDF dumps, zero new NDIS/LiveKernel dumps, zero relevant WER reports and zero relevant error-level events.
10. Removed all verifier settings, restarted once more in 736 ms, and proved the device remained healthy.
11. Uninstalled the exact recorded device and package. Final workflow evidence proves zero xsnet devices, zero xsnet driver packages, no installer state file and no `xsnet.inf_*` DriverStore directory.

The accepted evidence export is `work/windows-final-export/passed-004`. It contains 67 files (4,700,127 bytes), the package, PDB, harness, build logs, stage evidence and `evidence-sha256.json`. Independent verification found no package-manifest or evidence-manifest mismatch.

## Reproduction entry points

```powershell
pwsh -File .\scripts\windows\build-xsnet-test-package.ps1 <explicit arguments>
pwsh -File .\scripts\windows\invoke-xsnet-test-vm-stage.ps1 -Stage Initialize <explicit arguments>
# Install -> EnableVerifier -> reboot -> CollectVerifier -> DisableVerifier -> reboot -> Uninstall
```

Source-only regression remains available on non-Windows hosts:

```bash
make test-windows-xsnet-abi
make test-windows-xsnet-source
```

Neither source-only command substitutes for the VM evidence above.
