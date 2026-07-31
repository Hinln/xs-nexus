# xsnet Windows test installer

These scripts are only for a disposable Windows 11 build 26100 or newer test VM with a snapshot. They are not a production installer and must not be used on a daily workstation.

The package directory must contain exactly `xsnet.inf`, `xsnet.cat`, and `xsnet.dll`. Installation requires the expected test signer certificate thumbprint and a valid Microsoft-signed `DevGen.exe` from the locally installed WDK. DevGen is not included or redistributed because Microsoft limits it to test scenarios.

```powershell
pwsh -File .\installers\windows\install-xsnet-test.ps1 `
  -PackageDirectory C:\xsnet-package `
  -ExpectedSignerThumbprint <TEST_CERT_THUMBPRINT> `
  -DevGenPath <WDK_TOOLS_PATH>\devgen.exe `
  -AllowTestSignedPackage

pwsh -File .\installers\windows\uninstall-xsnet-test.ps1
```

The installer rejects an existing xsnet device/package, unexpected files, directories, reparse points, invalid signatures, signer mismatch, non-Microsoft DevGen, unsupported OS builds, unhealthy devices, and ambiguous driver-store results. Failure attempts bounded removal of the exact xsnet device and staged `oem#.inf`.

Successful installation stores only package hashes, signer thumbprint, exact device instance IDs, published INF name, and timestamp under `%ProgramData%\XS Nexus`. The state directory ACL grants full access only to LocalSystem and built-in Administrators. Uninstall removes only those recorded objects and keeps state if any xsnet residual remains.

No Windows execution result exists until `BLK-001` is resolved. Run MSBuild, InfVerif, catalog generation, test signing, installation, uninstall, reboot, sleep, crash, malformed IOCTL, Driver Verifier, and residual checks in the VM and preserve the output.
