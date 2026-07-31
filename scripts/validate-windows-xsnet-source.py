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
        "NetTxQueueGetRingCollection",
        "NetRxQueueGetRingCollection",
        "NetTxQueueGetExtension",
        "NetRxQueueGetExtension",
        "NET_FRAGMENT_EXTENSION_VIRTUAL_ADDRESS_NAME",
        "NetExtensionGetFragmentVirtualAddress",
        "NetRingCollectionGetPacketRing",
        "NetRingCollectionGetFragmentRing",
        "NetRingIncrementIndex",
        "NetPacketLayer2TypeNull",
        "WdfExecutionLevelPassive",
        "WdfRequestRetrieveOutputBuffer",
        "WdfRequestCompleteWithInformation",
        "NetRxQueueNotifyMoreReceivedPacketsAvailable",
        "XsnetPacketQueueValidateBatch",
        "XsnetPacketQueueMeasureBatch",
        "XsnetWriteMessageHeader",
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
    for value in (
        "complete_transmit_request",
        "complete_receive_request",
        "transmit_queue_ready_locked",
        "receive_queue_ready_locked",
        "STATUS_NO_MORE_ENTRIES",
        "STATUS_BUFFER_TOO_SMALL",
    ):
        if value not in ioctl_source:
            fail(f"packet request path missing invariant: {value}")
    if "message_type == XSNET_MESSAGE_SET_LINK ||" in ioctl_source:
        fail("SetLink must not share the obsolete blanket packet rejection path")


def validate_portable_tests() -> None:
    cmake = require_text(
        DRIVER / "CMakeLists.txt",
        [
            "add_executable(xsnet_stress_test tests/stress_test.c)",
            "add_test(NAME xsnet_stress_validation COMMAND xsnet_stress_test)",
            "add_executable(xsnet_lifecycle_test tests/lifecycle_test.c)",
            "add_test(NAME xsnet_lifecycle_validation COMMAND xsnet_lifecycle_test)",
        ],
    )
    stress = require_text(
        DRIVER / "tests" / "stress_test.c",
        [
            "STRESS_ITERATIONS UINT32_C(30000)",
            "stress_message_parser",
            "stress_message_writer",
            "stress_batch_parser",
            "stress_session_atomicity",
            "stress_valid_session_mutations",
            "stress_packet_queue",
            "sessions_equal",
        ],
    )
    if cmake.count("xsnet_stress_validation") != 1:
        fail("CMakeLists.txt must register exactly one portable stress test")
    if cmake.count("xsnet_lifecycle_validation") != 1:
        fail("CMakeLists.txt must register exactly one lifecycle test")
    for message_type in (
        "XSNET_MESSAGE_HELLO",
        "XSNET_MESSAGE_ATTACH",
        "XSNET_MESSAGE_SET_LINK",
        "XSNET_MESSAGE_TX_BATCH",
        "XSNET_MESSAGE_RX_BATCH",
        "XSNET_MESSAGE_DETACH",
    ):
        if message_type not in stress:
            fail(f"portable stress test missing message mutation: {message_type}")
    require_text(
        DRIVER / "tests" / "lifecycle_test.c",
        [
            "test_all_teardown_interleavings",
            "permutation_count == 720",
            "test_completion_wins_before_teardown",
            "test_sleep_requires_new_owner_session",
            "test_queue_restart_preserves_owner",
            "test_repeated_teardown_is_idempotent",
            "requests_cancelled == 1",
            "#ifdef NDEBUG\n#undef NDEBUG",
        ],
    )


def validate_agent_client() -> None:
    agent = (
        ROOT / "apps" / "agent" / "src" / "windows_xsnet.rs"
    ).read_text(encoding="utf-8")
    runtime = (ROOT / "apps" / "agent" / "src" / "runtime.rs").read_text(
        encoding="utf-8"
    )
    transport = (ROOT / "crates" / "windows-transport" / "src" / "lib.rs").read_text(
        encoding="utf-8"
    )
    platform = (
        ROOT / "crates" / "windows-transport" / "src" / "platform.rs"
    ).read_text(encoding="utf-8")
    require_text(
        ROOT / "apps" / "agent" / "src" / "windows_xsnet.rs",
        [
            "const ABI_MAGIC: u32 = 0x314e_5358;",
            "const DEVICE_TYPE: u32 = 0x8337;",
            "pub struct XsnetClient",
            "pub trait XsnetTransport",
            "pub enum TransportOutcome",
            "ClientState::ReconnectRequired",
            "complete_rejected",
            "complete_indeterminate",
            "decode_transmit_response",
            "encode_packet_batch",
            "constants_and_header_match_driver_contract",
            "transport_classification_controls_recovery",
            "0x8337_e00e",
            "0x8337_e011",
            "pub struct Win32DeviceTransport",
            "pub struct XsnetDeviceSession",
            "validate_session_configuration",
            "device_session_invalid_configuration_performs_no_io_and_drops_transport",
            "device_session_startup_failure_stops_and_drops_transport",
            "device_session_drop_does_not_issue_device_io",
            "device_session_rejects_invalid_rx_before_transport",
            "device_session_shutdown_rejection_requires_explicit_retry",
            "xs_windows_transport::Request::new",
            "xs_windows_transport::Outcome::Indeterminate",
        ],
    )
    try:
        session_impl = agent.split(
            "impl<T: XsnetTransport> XsnetDeviceSession<T> {", 1
        )[1].split("impl XsnetDeviceSession<Win32DeviceTransport> {", 1)[0]
        open_impl = agent.split(
            "impl XsnetDeviceSession<Win32DeviceTransport> {", 1
        )[1].split("\n}\n\n#[derive(Debug)]", 1)[0]
        session_validation = session_impl.index("validate_session_configuration")
        first_ioctl = session_impl.index("prepare_hello")
        open_validation = open_impl.index("validate_session_configuration")
        device_open = open_impl.index("Win32DeviceTransport::open")
    except (IndexError, ValueError):
        fail("xsnet session validation/open ordering is not statically recognizable")
    if session_validation > first_ioctl:
        fail("xsnet session must validate configuration before its first IOCTL")
    if open_validation > device_open:
        fail("xsnet session must validate configuration before opening the device")
    for forbidden in (
        "thread::spawn",
        "tokio::spawn",
        "tokio::time::sleep",
        "Drop for XsnetDeviceSession",
    ):
        if forbidden in agent:
            fail(f"xsnet Agent session contains forbidden background behavior: {forbidden}")
    if "windows_xsnet" in runtime:
        fail("xsnet session must remain outside Agent runtime before WDK/VM validation")
    require_text(
        ROOT / "apps" / "agent" / "src" / "lib.rs",
        ["pub mod windows_xsnet;"],
    )
    for value in (
        "#![no_std]",
        "#![deny(unsafe_code)]",
        "#[allow(unsafe_code)]\nmod platform;",
        "RequestKind::Buffered",
        "RequestKind::Dequeue",
        "RequestKind::Enqueue",
        "UnsupportedIoctl",
        "InvalidBuffers",
        "AmbiguousInterface",
    ):
        if value not in transport:
            fail(f"Windows transport boundary missing invariant: {value}")
    for value in (
        "CM_Get_Device_Interface_List_SizeW",
        "CM_Get_Device_Interface_ListW",
        "GUID_DEVINTERFACE_XSNET",
        "GENERIC_READ | GENERIC_WRITE",
        "CreateFileW",
        "DeviceIoControl",
        "direct_input.as_slice() == request.direct_input()",
        "Outcome::Indeterminate",
        "&raw mut character_count",
        "&raw mut bytes_returned",
    ):
        if value not in platform:
            fail(f"Win32 platform transport missing invariant: {value}")
    if transport.count("unsafe {") != 0 or platform.count("unsafe {") != 5:
        fail("Win32 unsafe blocks must remain isolated and exactly counted")
    for forbidden in (
        "FILE_FLAG_OVERLAPPED",
        "GetLastError",
        "loop {",
        "std::thread",
        "METHOD_NEITHER",
    ):
        if forbidden in platform:
            fail(f"Win32 platform transport contains forbidden behavior: {forbidden}")
    require_text(
        ROOT / "docs" / "WINDOWS_XSNET_TRANSPORT.md",
        [
            "The existing Agent crate remains `#![forbid(unsafe_code)]`",
            "no buffered input",
            "initial Win32 implementation should not infer",
            "Every cancellation, device removal",
            "never guess whether the driver advanced sequence",
        ],
    )


def main() -> int:
    try:
        validate_project()
        validate_inf()
        validate_ioctl()
        validate_sources()
        validate_portable_tests()
        validate_agent_client()
    except (AssertionError, OSError, element_tree.ParseError) as error:
        print(f"xsnet source validation failed: {error}", file=sys.stderr)
        return 1
    print("xsnet UMDF source and package invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
