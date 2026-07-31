# xsnet Windows test installer

These scripts are only for a disposable Windows 11 build 26100 or newer test VM with a snapshot. They are not a production installer and must not be used on a daily workstation.

The package directory must contain exactly `xsnet.inf`, `xsnet.cat`, and `xsnet.dll`. Installation requires the expected test signer certificate thumbprint and a valid Microsoft-signed `DevGen.exe` from the locally installed WDK. DevGen is not included or redistributed because Microsoft limits it to test scenarios.

Build a new test package only inside the disposable VM. The VM must already have Windows test-signing policy configured by the operator; the script does not modify BCD or certificate trust. Every tool path is explicit and must resolve to a valid Microsoft-signed binary.

```powershell
pwsh -File .\scripts\windows\build-xsnet-test-package.ps1 `
  -ProjectRoot C:\src\xs-nexus `
  -OutputDirectory C:\xsnet-runs\package-001 `
  -MsBuildPath <VS_PATH>\MSBuild.exe `
  -InfVerifPath <WDK_PATH>\infverif.exe `
  -Inf2CatPath <WDK_PATH>\Inf2Cat.exe `
  -SignToolPath <WDK_PATH>\signtool.exe `
  -TestSignerThumbprint <TEST_CERT_THUMBPRINT> `
  -ConfirmDisposableVm `
  -AllowTestSigning
```

```powershell
pwsh -File .\installers\windows\install-xsnet-test.ps1 `
  -PackageDirectory C:\xsnet-package `
  -ExpectedSignerThumbprint <TEST_CERT_THUMBPRINT> `
  -DevGenPath <WDK_TOOLS_PATH>\devgen.exe `
  -AllowTestSignedPackage

pwsh -File .\installers\windows\uninstall-xsnet-test.ps1
```

Use `scripts/windows/invoke-xsnet-test-vm-stage.ps1` to preserve an append-only test record. Run the stages in this exact order: `Initialize`, `Install`, `EnableVerifier`, reboot, `CollectVerifier`, execute and separately record the required traffic/lifecycle scenarios, `DisableVerifier`, reboot, then `Uninstall`. Every invocation requires the same absolute run directory, snapshot identifier, `-ConfirmDisposableVm`, and `-ConfirmSnapshotAvailable`. The snapshot identifier is an operator assertion, not proof that a hypervisor snapshot exists.

The workflow refuses non-VM hosts, pre-existing xsnet state, reused stage directories, missing required reboots, unhealthy or ambiguous device state, and uninstall residuals. It never restarts the VM automatically. `CollectVerifier` captures Verifier settings, system errors, adapters, routes, devices, and driver inventory but explicitly does not claim the required scenarios or M6.2 acceptance. Successful final uninstall writes `evidence-sha256.json` over the collected files.

The installer rejects an existing xsnet device/package, unexpected files, directories, reparse points, invalid signatures, signer mismatch, non-Microsoft DevGen, unsupported OS builds, unhealthy devices, and ambiguous driver-store results. Failure attempts bounded removal of the exact xsnet device and staged `oem#.inf`.

Successful installation stores only package hashes, signer thumbprint, exact device instance IDs, published INF name, and timestamp under `%ProgramData%\XS Nexus`. The state directory ACL grants full access only to LocalSystem and built-in Administrators. Uninstall removes only those recorded objects and keeps state if any xsnet residual remains.

No Windows execution result exists until `BLK-001` is resolved. Run MSBuild, InfVerif, catalog generation, test signing, installation, uninstall, repeated lifecycle, traffic, network switching, reboot, sleep, crash, malformed IOCTL, Driver Verifier, and residual checks in the VM and preserve the output.
