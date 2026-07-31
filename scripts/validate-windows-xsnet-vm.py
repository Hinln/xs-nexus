#!/usr/bin/env python3

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
WINDOWS_SCRIPTS = ROOT / "scripts" / "windows"


def require(path: Path, values: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in values:
        if value not in text:
            raise AssertionError(f"{path.relative_to(ROOT)} missing: {value}")
    return text


def main() -> int:
    try:
        package = require(
            WINDOWS_SCRIPTS / "build-xsnet-test-package.ps1",
            [
                "#Requires -RunAsAdministrator",
                "ConfirmDisposableVm",
                "AllowTestSigning",
                "Windows build 26100",
                "output directory must be an absolute path",
                "[IO.FileAttributes]::ReparsePoint",
                "Get-AuthenticodeSignature",
                "Microsoft",
                "1.3.6.1.5.5.7.3.3",
                "'/p:SignMode=Off'",
                "'/m:1'",
                "'/w'",
                "'/v'",
                "'/os:10_GE_X64'",
                "@('sign', '/v', '/fd', 'SHA256'",
                "xsnet.cat', 'xsnet.dll', 'xsnet.inf",
                "Get-FileHash -Algorithm SHA256",
                "driver_version = $driverVersion",
                "abi_min = 1",
                "abi_max = 1",
                "capabilities = @('ipv4')",
                "test_only = $true",
            ],
        )
        vm = require(
            WINDOWS_SCRIPTS / "invoke-xsnet-test-vm-stage.ps1",
            [
                "#Requires -RunAsAdministrator",
                "'Initialize', 'Install', 'EnableVerifier', 'CollectVerifier'",
                "'DisableVerifier', 'Uninstall'",
                "ConfirmDisposableVm",
                "ConfirmSnapshotAvailable",
                "ExpectedDriverVersion",
                "snapshot_assertion_only = $true",
                "HypervisorPresent",
                "not identified as a virtual machine",
                "Windows build 26100",
                "run directory must be an absolute path",
                "Get-NetAdapter -IncludeHidden",
                "Get-NetRoute",
                "Get-PnpDevice -Class Net",
                "Get-WindowsDriver -Online",
                "Read-XsnetInstallState",
                "state.abi_version -ne (Get-XsnetAbiVersion)",
                "install-xsnet-test.ps1",
                "uninstall-xsnet-test.ps1",
                "-AllowTestSignedPackage -Confirm:$false",
                "@('/standard', '/driver', 'xsnet.dll')",
                "@('/bootmode', 'oneboot')",
                "@('/query')",
                "@('/querysettings')",
                "@('/reset')",
                "VM must reboot after enabling Driver Verifier",
                "VM must reboot after disabling Driver Verifier",
                "scenario_results_included = $false",
                "acceptance_claimed = $false",
                "xsnet residual remains after uninstall",
                "Get-FileHash -Algorithm SHA256",
                "evidence-sha256.json",
            ],
        )
        dll_sign = package.index("sign-xsnet.dll.log")
        catalog_generation = package.index("Invoke-NativeTool -Path $inf2cat")
        catalog_sign = package.index("sign-xsnet.cat.log")
        if not dll_sign < catalog_generation < catalog_sign:
            raise AssertionError(
                "test package must sign the DLL before catalog generation, then sign the catalog"
            )
        combined = package + vm
        for forbidden in (
            "Invoke-Expression",
            "iex ",
            "DownloadString",
            "WebClient",
            "Invoke-WebRequest",
            "while ($true)",
            "ExecutionPolicy Bypass",
            "Set-AuthenticodeSignature",
            "bcdedit",
            "devcon",
            "Remove-Item",
            "Restart-Computer",
            "Stop-Computer",
        ):
            if forbidden.lower() in combined.lower():
                raise AssertionError(
                    f"Windows VM workflow contains forbidden value: {forbidden}"
                )
    except (AssertionError, OSError) as error:
        print(f"xsnet Windows VM script validation failed: {error}", file=sys.stderr)
        return 1
    print("xsnet Windows package and VM workflow source invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
