# xsnet Windows test installer

These scripts are only for an approved Windows 11 build 26100 or newer test target. The preferred target is a disposable VM with a snapshot. After the first VM driver gate has passed, a physical machine is allowed only when the owner dedicates it to testing for the entire run and has already prepared an external full-system image, bootable recovery media, disk-recovery material, and an onsite recovery operator. These scripts are not a production installer.

The package directory must contain exactly `xsnet.inf`, `xsnet.cat`, and `xsnet.dll`. Installation requires the expected test signer certificate thumbprint and a valid Microsoft-signed `DevGen.exe` from the locally installed WDK. DevGen is not included or redistributed because Microsoft limits it to test scenarios.

Build a new test package only on the approved target. The target must already have Windows test-signing policy configured by the operator; the script does not modify BCD or certificate trust. Every tool path is explicit and must resolve to a valid Microsoft-signed binary.

```powershell
pwsh -File .\scripts\windows\build-xsnet-test-package.ps1 `
  -ProjectRoot C:\src\xs-nexus `
  -OutputDirectory C:\xsnet-runs\package-001 `
  -MsBuildPath <VS_PATH>\MSBuild\Current\Bin\amd64\MSBuild.exe `
  -InfVerifPath <WDK_PATH>\infverif.exe `
  -Inf2CatPath <WDK_PATH>\Inf2Cat.exe `
  -SignToolPath <WDK_PATH>\signtool.exe `
  -TestSignerThumbprint <TEST_CERT_THUMBPRINT> `
  -TargetType VirtualMachine `
  -ConfirmDisposableVm `
  -AllowTestSigning
```

For a dedicated physical target, replace the VM confirmation with:

```powershell
  -TargetType PhysicalMachine `
  -ConfirmDedicatedPhysicalTarget `
  -ConfirmPhysicalRecoveryReady `
  -AllowTestSigning
```

The WDK 26100 build must use 64-bit MSBuild because its package verifier is
shipped for x64. The official WDK 26100.6584 `Inf2Cat.exe` uses Microsoft's
internal build-tools certificate rather than a publicly trusted signer; the
builder accepts only the exact pinned SHA-256 binary from that WDK release.

```powershell
pwsh -File .\installers\windows\install-xsnet-test.ps1 `
  -PackageDirectory C:\xsnet-package `
  -ExpectedSignerThumbprint <TEST_CERT_THUMBPRINT> `
  -ExpectedDriverVersion <MANIFEST_DRIVER_VERSION> `
  -DevGenPath <WDK_TOOLS_PATH>\devgen.exe `
  -AllowTestSignedPackage

pwsh -File .\installers\windows\uninstall-xsnet-test.ps1
```

Use `scripts/windows/invoke-xsnet-test-vm-stage.ps1` to preserve an append-only test record; the filename is retained for compatibility but the orchestrator supports both target types. Run the stages in this exact order: `Initialize`, `Install`, `EnableVerifier`, reboot, `CollectVerifier`, execute and separately record the required traffic/lifecycle scenarios, `DisableVerifier`, reboot, then `Uninstall`.

Every VM invocation requires the same run directory, snapshot identifier, `-TargetType VirtualMachine`, `-ConfirmDisposableVm`, and `-ConfirmSnapshotAvailable`. Every physical invocation requires the same run directory, system-image ID, recovery-media ID, disk-recovery-receipt ID, recovery-operator ID, `-TargetType PhysicalMachine`, and all five physical recovery confirmations. Identifiers are public operator assertions, not secret recovery values and not proof by themselves. Never pass or record a BitLocker recovery password.

Example physical-target initialization after the recovery set has been independently verified:

```powershell
$PhysicalTarget = @{
  TargetType = 'PhysicalMachine'
  RunDirectory = 'C:\xsnet-runs\physical-001'
  SystemImageId = 'system-image-20260813-01'
  RecoveryMediaId = 'winre-usb-20260813-01'
  DiskRecoveryReceiptId = 'disk-recovery-receipt-20260813-01'
  RecoveryOperatorId = 'owner-onsite-01'
  ConfirmDedicatedPhysicalTarget = $true
  ConfirmExternalSystemImageAvailable = $true
  ConfirmBootableRecoveryMediaAvailable = $true
  ConfirmDiskRecoveryMaterialAvailable = $true
  ConfirmOnsiteRecoveryOperatorAvailable = $true
}
& .\scripts\windows\invoke-xsnet-test-vm-stage.ps1 `
  -Stage Initialize @PhysicalTarget
```

The workflow rejects a target whose observed physical/virtual identity conflicts with the requested mode, pre-existing xsnet state, reused stage directories, changed recovery identifiers, missing required reboots, unhealthy or ambiguous device state, and uninstall residuals. It never restarts the target automatically. `CollectVerifier` captures Verifier settings, system errors, adapters, routes, devices, driver inventory, target identity and disk-protection status but explicitly does not claim the required scenarios or M6.2 acceptance. Successful final uninstall writes `evidence-sha256.json` over the collected files.

The installer rejects an existing xsnet device/package, unexpected files, directories, reparse points, invalid signatures, signer mismatch, INF `DriverVer` mismatch, staged driver-store version mismatch, non-Microsoft DevGen, unsupported OS builds, unhealthy devices, and ambiguous driver-store results. Failure attempts bounded removal of the exact xsnet device and staged `oem#.inf`.

Successful installation stores only ABI v1, the four-part driver version, package hashes, signer thumbprint, exact device instance IDs, published INF name, and timestamp under `%ProgramData%\XS Nexus`. The state directory ACL grants full access only to LocalSystem and built-in Administrators. Uninstall removes only those recorded objects and keeps state if any xsnet residual remains. The test workflow is clean-install only; compatibility and rollback boundaries are defined in `docs/WINDOWS_XSNET_COMPATIBILITY.md`.

The first accepted Windows driver run is documented in `docs/WINDOWS_XSNET_VM_EVIDENCE.md`. It covers WDK build, InfVerif, catalog generation, test signing, clean install, SYSTEM Tx/Rx, PnP restart, standard Driver Verifier, UMDF/Application Verifier, repeated lifecycle, clean uninstall and residual checks. It does not convert these scripts into a production installer or close the remaining complete-Agent, route/DAD, sleep, production-signing or Windows 10 gates.
