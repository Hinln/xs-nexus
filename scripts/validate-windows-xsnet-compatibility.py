#!/usr/bin/env python3

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def one(pattern: str, text: str, source: str) -> str:
    matches = re.findall(pattern, text, flags=re.MULTILINE)
    if len(matches) != 1:
        raise AssertionError(f"{source} must contain exactly one match for {pattern}")
    return matches[0]


def require(text: str, source: str, values: tuple[str, ...]) -> None:
    for value in values:
        if value not in text:
            raise AssertionError(f"{source} missing compatibility invariant: {value}")


def main() -> int:
    try:
        header = read("drivers/windows-xsnet/include/xsnet_abi.h")
        agent = read("apps/agent/src/windows_xsnet.rs")
        inf = read("drivers/windows-xsnet/xsnet.inf")
        session = read("drivers/windows-xsnet/src/session.c")
        installer = read("installers/windows/install-xsnet-test.ps1")
        module = read("installers/windows/XsnetTestInstaller.psm1")
        package = read("scripts/windows/build-xsnet-test-package.ps1")
        vm = read("scripts/windows/invoke-xsnet-test-vm-stage.ps1")

        c_abi = int(one(r"#define XSNET_ABI_VERSION UINT16_C\((\d+)\)", header, "xsnet_abi.h"))
        rust_abi = int(
            one(
                r"pub const XSNET_ABI_VERSION: u16 = (\d+);",
                agent,
                "windows_xsnet.rs",
            )
        )
        module_abi = int(
            one(r"\$script:XsnetAbiVersion = (\d+)", module, "XsnetTestInstaller.psm1")
        )
        if {c_abi, rust_abi, module_abi} != {1}:
            raise AssertionError("driver, Agent, and installer must pin exact ABI v1")

        driver_version = one(
            r"^DriverVer=[^,]+,([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)$",
            inf,
            "xsnet.inf",
        )
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+", driver_version):
            raise AssertionError("xsnet INF DriverVer must be four numeric parts")

        require(
            agent,
            "windows_xsnet.rs",
            (
                "write_u16(&mut payload, 0, XSNET_ABI_VERSION);",
                "write_u16(&mut payload, 2, XSNET_ABI_VERSION);",
                "write_u16(&mut message, 4, XSNET_ABI_VERSION);",
                "&XSNET_ABI_VERSION.to_le_bytes()",
            ),
        )
        require(
            session,
            "session.c",
            (
                "minimum_version > maximum_version",
                "minimum_version > XSNET_ABI_VERSION",
                "maximum_version < XSNET_ABI_VERSION",
            ),
        )
        require(
            package,
            "build-xsnet-test-package.ps1",
            (
                "driver_version = $driverVersion",
                "abi_min = 1",
                "abi_max = 1",
                "capabilities = @('ipv4')",
            ),
        )
        require(
            installer + module + vm,
            "Windows install workflow",
            (
                "ExpectedDriverVersion",
                "Get-XsnetInfDriverVersion",
                "Get-XsnetAbiVersion",
                "abi_version = $abiVersion",
                "driver_version = $driverVersion",
                "schema = 2",
                "state.schema -ne 2",
                "state.abi_version -ne $script:XsnetAbiVersion",
                "state.driver_version -notmatch",
            ),
        )
        if "an xsnet device or driver package already exists; uninstall it first" not in installer:
            raise AssertionError("test installer must remain clean-install only")
    except (AssertionError, OSError, ValueError) as error:
        print(f"xsnet compatibility validation failed: {error}", file=sys.stderr)
        return 1
    print("xsnet exact ABI v1 and clean-install compatibility invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
