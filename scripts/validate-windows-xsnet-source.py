#!/usr/bin/env python3

from pathlib import Path
import re
import sys
import xml.etree.ElementTree as element_tree


ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "drivers" / "windows-xsnet"


def fail(message: str) -> None:
    raise AssertionError(message)


def require_text(path: Path, required: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    for value in required:
        if value not in text:
            fail(f"{path.relative_to(ROOT)} missing required value: {value}")
    return text


def validate_project() -> None:
    project = DRIVER / "xsnet.vcxproj"
    tree = element_tree.parse(project)
    namespace = {"msb": "http://schemas.microsoft.com/developer/msbuild/2003"}
    values = {
        element.tag.split("}")[-1]: (element.text or "").strip()
        for element in tree.findall(".//*")
    }
    expected = {
        "WindowsTargetPlatformVersion": "10.0.26100.0",
        "DriverType": "UMDF",
        "PlatformToolset": "WindowsUserModeDriver10.0",
        "UMDF_VERSION_MAJOR": "2",
        "UMDF_VERSION_MINOR": "33",
        "LinkToNetAdapterCx": "true",
        "NetAdapterCxMajorVersion": "2",
        "NetAdapterCxMinorVersion": "5",
        "TreatWarningAsError": "true",
        "SDLCheck": "true",
        "SignMode": "TestSign",
    }
    for name, expected_value in expected.items():
        if values.get(name) != expected_value:
            fail(f"xsnet.vcxproj {name} must be {expected_value}")
    configurations = tree.findall(".//msb:ProjectConfiguration", namespace)
    if {item.attrib["Include"] for item in configurations} != {
        "Debug|x64",
        "Release|x64",
    }:
        fail("xsnet.vcxproj must target x64 Debug and Release only")
    compile_sources = {
        item.attrib["Include"]
        for item in tree.findall(".//msb:ClCompile", namespace)
        if "Include" in item.attrib
    }
    for source in ("src\\abi.c", "src\\session.c", "src\\dataplane.c"):
        if source not in compile_sources:
            fail(f"xsnet.vcxproj must compile {source}")


def validate_inf() -> None:
    text = require_text(
        DRIVER / "xsnet.inf",
        [
            "NTamd64.10.0...26100",
            'HKR,,Security,,"D:P(A;;GA;;;SY)"',
            "HKR,,Exclusive,0x00010001,1",
            "UmdfLibraryVersion=2.33",
            "UmdfHostProcessSharing=ProcessSharingDisabled",
            "UmdfKernelModeClientPolicy=RejectKernelModeClients",
            "UmdfFileObjectPolicy=RejectNullAndUnknownFileObjects",
            "UmdfDirectHardwareAccess=RejectDirectHardwareAccess",
            "Include=WUDFRD.inf",
            "ServiceBinary=%13%\\xsnet.dll",
        ],
    )
    security_line = next(
        line for line in text.splitlines() if line.startswith("HKR,,Security")
    )
    for forbidden_sid in (";;;WD)", ";;;BU)", ";;;BA)", ";;;AU)", ";;;IU)"):
        if forbidden_sid in security_line:
            fail(f"xsnet.inf grants forbidden device access: {forbidden_sid}")


def validate_ioctl() -> None:
    text = require_text(
        DRIVER / "include" / "xsnet_ioctl.h",
        [
            "FILE_READ_ACCESS | FILE_WRITE_ACCESS",
            "METHOD_BUFFERED",
            "METHOD_OUT_DIRECT",
            "METHOD_IN_DIRECT",
        ],
    )
    for forbidden in ("FILE_ANY_ACCESS", "METHOD_NEITHER"):
        if forbidden in text:
            fail(f"xsnet_ioctl.h contains forbidden transfer/access mode: {forbidden}")
    if len(re.findall(r"CTL_CODE\(", text)) != 6:
        fail("xsnet_ioctl.h must define exactly six IOCTLs")


def validate_sources() -> None:
    sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in sorted((DRIVER / "src").glob("*.c"))
    )
    required = [
        "NetDeviceInitConfig",
        "WdfDeviceInitSetFileObjectConfig",
        "WdfRequestGetRequestorMode",
        "WdfRequestGetFileObject",
        "WdfWaitLockCreate",
        "NetAdapterInitSetDatapathCallbacks",
        "NET_ADAPTER_RX_CAPABILITIES_INIT_SYSTEM_MANAGED",
        "NET_ADAPTER_LINK_STATE_INIT_DISCONNECTED",
        "WdfRequestComplete(request, STATUS_CANCELLED)",
        "XsnetPacketQueuePushBatch",
        "XsnetPacketQueuePopBatch",
        "secure_zero",
    ]
    for value in required:
        if value not in sources:
            fail(f"Windows source missing lifecycle invariant: {value}")
    forbidden = [
        "Wintun",
        "TAP-Windows",
        "METHOD_NEITHER",
        "FILE_ANY_ACCESS",
        "printf(",
        "fprintf(",
        "OutputDebugString",
    ]
    for value in forbidden:
        if value in sources:
            fail(f"Windows source contains forbidden dependency or logging: {value}")
    ioctl_source = (DRIVER / "src" / "ioctl.c").read_text(encoding="utf-8")
    if "XSNET_MESSAGE_SET_LINK" not in ioctl_source or "STATUS_NOT_SUPPORTED" not in ioctl_source:
        fail("incomplete packet path must fail closed before link-up")


def main() -> int:
    try:
        validate_project()
        validate_inf()
        validate_ioctl()
        validate_sources()
    except (AssertionError, OSError, element_tree.ParseError) as error:
        print(f"xsnet source validation failed: {error}", file=sys.stderr)
        return 1
    print("xsnet UMDF source and package invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
