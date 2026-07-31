#!/usr/bin/env python3

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
INSTALLERS = ROOT / "installers" / "windows"


def require(path: Path, values: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in values:
        if value not in text:
            raise AssertionError(f"{path.relative_to(ROOT)} missing: {value}")
    return text


def main() -> int:
    try:
        module = require(
            INSTALLERS / "XsnetTestInstaller.psm1",
            [
                "Set-StrictMode -Version Latest",
                "Windows 11 build 26100",
                "Get-AuthenticodeSignature",
                "O=Microsoft Corporation",
                "Get-PnpDeviceProperty",
                "DEVPKEY_Device_HardwareIds",
                "Get-WindowsDriver -Online",
                "Test-XsnetDeviceRemovalTarget",
                "Test-XsnetDriverRemovalTarget",
                "state file cannot be a reparse point",
                "state.schema -ne 2",
                "$script:XsnetAbiVersion = 1",
                "Get-XsnetInfDriverVersion",
                "state.abi_version -ne $script:XsnetAbiVersion",
                "state.driver_version -notmatch",
                "^oem[0-9]+\\.inf$",
                "*S-1-5-18:(OI)(CI)F",
                "*S-1-5-32-544:(OI)(CI)F",
            ],
        )
        install = require(
            INSTALLERS / "install-xsnet-test.ps1",
            [
                "#Requires -RunAsAdministrator",
                "AllowTestSignedPackage",
                "ExpectedSignerThumbprint",
                "ExpectedDriverVersion",
                "Get-XsnetInfDriverVersion",
                "Get-XsnetAbiVersion",
                "xsnet.cat', 'xsnet.dll', 'xsnet.inf",
                "[IO.FileAttributes]::ReparsePoint",
                "Assert-XsnetSignature",
                "Assert-XsnetDevGen",
                "'/add-driver'",
                "'/bus'",
                "'ROOT'",
                "'/hardwareid'",
                "'Root\\XSNET'",
                "for ($attempt = 0; $attempt -lt 40",
                "'/remove-device'",
                "'/delete-driver'",
                "Write-XsnetInstallState",
                "schema = 2",
                "abi_version = $abiVersion",
                "driver_version = $driverVersion",
                "test_only = $true",
            ],
        )
        uninstall = require(
            INSTALLERS / "uninstall-xsnet-test.ps1",
            [
                "#Requires -RunAsAdministrator",
                "Read-XsnetInstallState",
                "Test-XsnetDeviceRemovalTarget",
                "Test-XsnetDriverRemovalTarget",
                "'/remove-device'",
                "'/delete-driver'",
                "for ($attempt = 0; $attempt -lt 40",
                "Get-XsnetDevices",
                "Get-XsnetDriverPackages",
                "Remove-XsnetState",
            ],
        )
        combined = module + install + uninstall
        for forbidden in (
            "Invoke-Expression",
            "iex ",
            "DownloadString",
            "WebClient",
            "while ($true)",
            "ExecutionPolicy Bypass",
            "Set-AuthenticodeSignature",
            "bcdedit",
            "devcon",
        ):
            if forbidden.lower() in combined.lower():
                raise AssertionError(f"Windows installer contains forbidden value: {forbidden}")
    except (AssertionError, OSError) as error:
        print(f"xsnet installer validation failed: {error}", file=sys.stderr)
        return 1
    print("xsnet test installer source invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
