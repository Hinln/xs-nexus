#!/usr/bin/env python3

import argparse
import ipaddress
import json
import os
import select
import socket
import struct
import sys
import time
from pathlib import Path

ETHERNET_HEADER_LENGTH = 14
XSP_DATA_HEADER_LENGTH = 96
XSP_MAGIC = b"XSP1"
XSR_DATA_HEADER_LENGTH = 104
XSR_MAGIC = b"XSR1"
XSR_DATA_TYPE = 0x03
XSP_PACKET_TYPES = {
    "data": 0x01,
    "keepalive": 0x02,
    "path-challenge": 0x03,
    "path-response": 0x04,
    "key-update": 0x05,
    "key-update-ack": 0x06,
    "close": 0x07,
}


def checksum(data: bytes) -> int:
    if len(data) % 2:
        data += b"\0"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    total = (total >> 16) + (total & 0xFFFF)
    total += total >> 16
    return (~total) & 0xFFFF


def ipv4(value: str) -> str:
    return str(ipaddress.IPv4Address(value))


def port(value: str) -> int:
    parsed = int(value)
    if not 1 <= parsed <= 65535:
        raise argparse.ArgumentTypeError("port must be between 1 and 65535")
    return parsed


def positive(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be positive")
    return parsed


def nonnegative(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("value must be nonnegative")
    return parsed


def build_ipv4_udp(
    source: str,
    destination: str,
    source_port: int,
    destination_port: int,
    payload: bytes,
    packet_id: int,
) -> bytes:
    source_bytes = socket.inet_aton(source)
    destination_bytes = socket.inet_aton(destination)
    udp_length = 8 + len(payload)
    udp_header = struct.pack(
        "!HHHH",
        source_port,
        destination_port,
        udp_length,
        0,
    )
    total_length = 20 + udp_length
    ip_header = struct.pack(
        "!BBHHHBBH4s4s",
        0x45,
        0,
        total_length,
        packet_id & 0xFFFF,
        0x4000,
        64,
        socket.IPPROTO_UDP,
        0,
        source_bytes,
        destination_bytes,
    )
    ip_header = ip_header[:10] + struct.pack("!H", checksum(ip_header)) + ip_header[12:]
    return ip_header + udp_header + payload


def raw_ipv4_socket() -> socket.socket:
    sock = socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_RAW)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_HDRINCL, 1)
    return sock


def command_send_virtual(arguments: argparse.Namespace) -> None:
    payload = arguments.payload.encode()
    packet = build_ipv4_udp(
        arguments.source,
        arguments.destination,
        arguments.source_port,
        arguments.destination_port,
        payload,
        arguments.packet_id,
    )
    if arguments.output is not None:
        Path(arguments.output).write_bytes(packet)
    with raw_ipv4_socket() as sock:
        sock.sendto(packet, (arguments.destination, 0))
    print(f"sent-virtual bytes={len(packet)}")


def command_inject_xsp(arguments: argparse.Namespace) -> None:
    payload = bytearray(Path(arguments.input).read_bytes())
    if len(payload) < XSP_DATA_HEADER_LENGTH or payload[:4] != XSP_MAGIC:
        raise SystemExit("input is not an XSP/1 datagram")
    if arguments.flip_last:
        payload[-1] ^= 1
    packet = build_ipv4_udp(
        arguments.source,
        arguments.destination,
        arguments.source_port,
        arguments.destination_port,
        bytes(payload),
        arguments.packet_id,
    )
    with raw_ipv4_socket() as sock:
        sock.sendto(packet, (arguments.destination, 0))
    print(
        f"injected-xsp bytes={len(payload)} tampered={str(arguments.flip_last).lower()}"
    )


def command_collect_udp(arguments: argparse.Namespace) -> None:
    expected = arguments.expected.encode()
    received = 0
    deadline = time.monotonic() + arguments.timeout
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as server:
        server.bind((arguments.bind, arguments.port))
        server.setblocking(False)
        while time.monotonic() < deadline:
            ready, _, _ = select.select(
                [server],
                [],
                [],
                min(0.2, max(0.0, deadline - time.monotonic())),
            )
            if not ready:
                continue
            data, _ = server.recvfrom(65535)
            if data == expected:
                received += 1
    if received != arguments.count:
        raise SystemExit(
            f"unexpected UDP delivery count: expected={arguments.count} actual={received}"
        )
    print(f"udp-collect-ok count={received}")


def command_udp_server(arguments: argparse.Namespace) -> None:
    request = arguments.request.encode()
    response = arguments.response.encode()
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as server:
        server.bind((arguments.bind, arguments.port))
        server.settimeout(arguments.timeout)
        data, address = server.recvfrom(65535)
        if data != request:
            raise SystemExit("unexpected UDP request")
        server.sendto(response, address)
    print(f"udp-server-ok peer={address[0]}")


def command_udp_client(arguments: argparse.Namespace) -> None:
    request = arguments.request.encode()
    expected = arguments.response.encode()
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client:
        client.settimeout(arguments.timeout)
        client.sendto(request, (arguments.destination, arguments.port))
        response, address = client.recvfrom(65535)
    if response != expected:
        raise SystemExit("unexpected UDP response")
    print(f"udp-client-ok peer={address[0]}")


def command_tcp_server(arguments: argparse.Namespace) -> None:
    request = arguments.request.encode()
    response = arguments.response.encode()
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((arguments.bind, arguments.port))
        server.listen(1)
        server.settimeout(arguments.timeout)
        connection, address = server.accept()
        with connection:
            connection.settimeout(arguments.timeout)
            data = connection.recv(65535)
            if data != request:
                raise SystemExit("unexpected TCP request")
            connection.sendall(response)
    print(f"tcp-server-ok peer={address[0]}")


def command_tcp_client(arguments: argparse.Namespace) -> None:
    request = arguments.request.encode()
    expected = arguments.response.encode()
    with socket.create_connection(
        (arguments.destination, arguments.port),
        timeout=arguments.timeout,
    ) as client:
        client.sendall(request)
        response = client.recv(65535)
    if response != expected:
        raise SystemExit("unexpected TCP response")
    print("tcp-client-ok")


def command_icmp(arguments: argparse.Namespace) -> None:
    payload = arguments.payload.encode()
    identifier = os.getpid() & 0xFFFF
    samples = []
    with socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_ICMP) as sock:
        sock.setblocking(False)
        for offset in range(arguments.count):
            sequence = arguments.sequence + offset
            if sequence > 65535:
                raise SystemExit("ICMP sequence exceeds 65535")
            header = struct.pack("!BBHHH", 8, 0, 0, identifier, sequence)
            packet = (
                struct.pack(
                    "!BBHHH",
                    8,
                    0,
                    checksum(header + payload),
                    identifier,
                    sequence,
                )
                + payload
            )
            started = time.perf_counter_ns()
            deadline = time.monotonic() + arguments.timeout
            sock.sendto(packet, (arguments.destination, 0))
            while time.monotonic() < deadline:
                ready, _, _ = select.select(
                    [sock],
                    [],
                    [],
                    min(0.2, max(0.0, deadline - time.monotonic())),
                )
                if not ready:
                    continue
                response, address = sock.recvfrom(65535)
                header_length = (response[0] & 0x0F) * 4
                icmp = response[header_length:]
                if len(icmp) < 8:
                    continue
                packet_type, code, _, response_id, response_sequence = struct.unpack(
                    "!BBHHH", icmp[:8]
                )
                if (
                    packet_type == 0
                    and code == 0
                    and response_id == identifier
                    and response_sequence == sequence
                    and icmp[8:] == payload
                ):
                    samples.append((time.perf_counter_ns() - started) / 1_000_000)
                    break
            else:
                raise SystemExit(f"ICMP echo timed out at sample {offset + 1}")
            if offset + 1 < arguments.count and arguments.interval > 0:
                time.sleep(arguments.interval)
    ordered = sorted(samples)
    percentile = lambda value: ordered[min(len(ordered) - 1, round((len(ordered) - 1) * value))]
    report = {
        "peer": address[0],
        "count": len(samples),
        "min_ms": ordered[0],
        "average_ms": sum(ordered) / len(ordered),
        "p50_ms": percentile(0.50),
        "p95_ms": percentile(0.95),
        "max_ms": ordered[-1],
    }
    if arguments.output is not None:
        Path(arguments.output).write_text(
            json.dumps(report, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
    print(
        "icmp-ok "
        f"peer={report['peer']} count={report['count']} "
        f"average_ms={report['average_ms']:.3f} p95_ms={report['p95_ms']:.3f}"
    )


def parse_udp_frame(frame: bytes) -> dict[str, object] | None:
    if len(frame) < ETHERNET_HEADER_LENGTH:
        return None
    offset = ETHERNET_HEADER_LENGTH
    ether_type = struct.unpack("!H", frame[12:14])[0]
    if ether_type == 0x8100:
        if len(frame) < 18:
            return None
        ether_type = struct.unpack("!H", frame[16:18])[0]
        offset = 18
    if ether_type != 0x0800 or len(frame) < offset + 20:
        return None
    header_length = (frame[offset] & 0x0F) * 4
    if frame[offset] >> 4 != 4 or header_length < 20:
        return None
    if len(frame) < offset + header_length + 8 or frame[offset + 9] != socket.IPPROTO_UDP:
        return None
    total_length = struct.unpack("!H", frame[offset + 2 : offset + 4])[0]
    ip_end = offset + total_length
    if total_length < header_length + 8 or len(frame) < ip_end:
        return None
    source_ip = socket.inet_ntoa(frame[offset + 12 : offset + 16])
    destination_ip = socket.inet_ntoa(frame[offset + 16 : offset + 20])
    udp_offset = offset + header_length
    source_port, destination_port, udp_length, _ = struct.unpack(
        "!HHHH", frame[udp_offset : udp_offset + 8]
    )
    if udp_length < 8 or udp_offset + udp_length > ip_end:
        return None
    payload = frame[udp_offset + 8 : udp_offset + udp_length]
    return {
        "source_ip": source_ip,
        "destination_ip": destination_ip,
        "source_port": source_port,
        "destination_port": destination_port,
        "payload": payload,
    }


def parse_xsp_frame(frame: bytes) -> dict[str, object] | None:
    udp = parse_udp_frame(frame)
    if udp is None:
        return None
    payload = udp["payload"]
    if len(payload) < XSP_DATA_HEADER_LENGTH or payload[:4] != XSP_MAGIC:
        return None
    return {
        "source_ip": udp["source_ip"],
        "destination_ip": udp["destination_ip"],
        "source_port": udp["source_port"],
        "destination_port": udp["destination_port"],
        "packet_type": payload[5],
        "epoch": struct.unpack("!I", payload[76:80])[0],
        "sequence": struct.unpack("!Q", payload[80:88])[0],
        "payload": payload,
    }


def parse_relay_frame(frame: bytes) -> dict[str, object] | None:
    udp = parse_udp_frame(frame)
    if udp is None:
        return None
    payload = udp["payload"]
    if (
        len(payload) < XSR_DATA_HEADER_LENGTH
        or payload[:4] != XSR_MAGIC
        or payload[4] != 1
        or payload[5] != XSR_DATA_TYPE
        or payload[6:8] != b"\0\0"
        or struct.unpack("!H", payload[8:10])[0] != XSR_DATA_HEADER_LENGTH
        or payload[12:16] != b"\0\0\0\0"
    ):
        return None
    inner_length = struct.unpack("!H", payload[10:12])[0]
    inner = payload[XSR_DATA_HEADER_LENGTH:]
    if (
        inner_length != len(inner)
        or len(inner) < 16
        or inner[:4] != XSP_MAGIC
        or inner[4] != 1
    ):
        return None
    return {
        "source_ip": udp["source_ip"],
        "destination_ip": udp["destination_ip"],
        "source_port": udp["source_port"],
        "destination_port": udp["destination_port"],
        "sequence": struct.unpack("!Q", payload[96:104])[0],
        "inner_packet_type": inner[5],
        "inner_length": inner_length,
        "payload": payload,
    }


def frame_matches(record: dict[str, object], arguments: argparse.Namespace) -> bool:
    return (
        record["packet_type"] == XSP_PACKET_TYPES[arguments.packet_type]
        and (arguments.source is None or record["source_ip"] == arguments.source)
        and (
            arguments.destination is None
            or record["destination_ip"] == arguments.destination
        )
        and (
            arguments.source_port is None
            or record["source_port"] == arguments.source_port
        )
        and (
            arguments.destination_port is None
            or record["destination_port"] == arguments.destination_port
        )
    )


def relay_frame_matches(
    record: dict[str, object], arguments: argparse.Namespace
) -> bool:
    return (
        record["inner_packet_type"]
        == XSP_PACKET_TYPES[arguments.inner_packet_type]
        and (arguments.source is None or record["source_ip"] == arguments.source)
        and (
            arguments.destination is None
            or record["destination_ip"] == arguments.destination
        )
        and (
            arguments.source_port is None
            or record["source_port"] == arguments.source_port
        )
        and (
            arguments.destination_port is None
            or record["destination_port"] == arguments.destination_port
        )
    )


def open_packet_socket(interface: str, timeout: float) -> socket.socket:
    sock = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(0x0003))
    sock.bind((interface, 0))
    sock.settimeout(min(0.2, timeout))
    return sock


def command_capture_xsp(arguments: argparse.Namespace) -> None:
    deadline = time.monotonic() + arguments.timeout
    records: list[dict[str, object]] = []
    payloads: list[bytes] = []
    with open_packet_socket(arguments.interface, arguments.timeout) as sock:
        while time.monotonic() < deadline and len(records) < arguments.count:
            try:
                frame = sock.recv(65535)
            except TimeoutError:
                continue
            record = parse_xsp_frame(frame)
            if record is None or not frame_matches(record, arguments):
                continue
            payload = record.pop("payload")
            if arguments.forbid_text is not None and arguments.forbid_text.encode() in payload:
                raise SystemExit("plaintext marker leaked into XSP datagram")
            if arguments.forbid_file is not None:
                forbidden = Path(arguments.forbid_file).read_bytes()
                if forbidden in payload:
                    raise SystemExit("original virtual IP packet leaked into XSP datagram")
            records.append(record)
            payloads.append(payload)
    if len(records) != arguments.count:
        raise SystemExit(
            f"XSP capture timed out: expected={arguments.count} actual={len(records)}"
        )
    if arguments.output is not None:
        if len(payloads) != 1:
            raise SystemExit("--output requires --count 1")
        Path(arguments.output).write_bytes(payloads[0])
    if arguments.metadata is not None:
        Path(arguments.metadata).write_text(
            json.dumps(records, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(records, sort_keys=True))


def command_capture_relay(arguments: argparse.Namespace) -> None:
    deadline = time.monotonic() + arguments.timeout
    records: list[dict[str, object]] = []
    payloads: list[bytes] = []
    with open_packet_socket(arguments.interface, arguments.timeout) as sock:
        while time.monotonic() < deadline and len(records) < arguments.count:
            try:
                frame = sock.recv(65535)
            except TimeoutError:
                continue
            record = parse_relay_frame(frame)
            if record is None or not relay_frame_matches(record, arguments):
                continue
            payload = record.pop("payload")
            if arguments.forbid_text is not None and arguments.forbid_text.encode() in payload:
                raise SystemExit("plaintext marker leaked into XSR datagram")
            if arguments.forbid_file is not None:
                forbidden = Path(arguments.forbid_file).read_bytes()
                if forbidden in payload:
                    raise SystemExit("original virtual IP packet leaked into XSR datagram")
            records.append(record)
            payloads.append(payload)
    if len(records) != arguments.count:
        raise SystemExit(
            f"XSR capture timed out: expected={arguments.count} actual={len(records)}"
        )
    if arguments.output is not None:
        if len(payloads) != 1:
            raise SystemExit("--output requires --count 1")
        Path(arguments.output).write_bytes(payloads[0])
    if arguments.metadata is not None:
        Path(arguments.metadata).write_text(
            json.dumps(records, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(records, sort_keys=True))


def command_assert_no_xsp(arguments: argparse.Namespace) -> None:
    deadline = time.monotonic() + arguments.timeout
    with open_packet_socket(arguments.interface, arguments.timeout) as sock:
        while time.monotonic() < deadline:
            try:
                frame = sock.recv(65535)
            except TimeoutError:
                continue
            record = parse_xsp_frame(frame)
            if record is not None and frame_matches(record, arguments):
                raise SystemExit("unexpected XSP datagram observed")
    print("no-xsp-observed")


def add_udp_endpoint_filters(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--source", type=ipv4)
    parser.add_argument("--destination", type=ipv4)
    parser.add_argument("--source-port", type=port)
    parser.add_argument("--destination-port", type=port)


def add_endpoint_filters(parser: argparse.ArgumentParser) -> None:
    add_udp_endpoint_filters(parser)
    parser.add_argument(
        "--packet-type",
        choices=sorted(XSP_PACKET_TYPES),
        default="data",
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subcommands = parser.add_subparsers(dest="command", required=True)

    send_virtual = subcommands.add_parser("send-virtual")
    send_virtual.add_argument("--source", required=True, type=ipv4)
    send_virtual.add_argument("--destination", required=True, type=ipv4)
    send_virtual.add_argument("--source-port", required=True, type=port)
    send_virtual.add_argument("--destination-port", required=True, type=port)
    send_virtual.add_argument("--payload", required=True)
    send_virtual.add_argument("--packet-id", type=nonnegative, default=1)
    send_virtual.add_argument("--output")
    send_virtual.set_defaults(handler=command_send_virtual)

    inject_xsp = subcommands.add_parser("inject-xsp")
    inject_xsp.add_argument("--source", required=True, type=ipv4)
    inject_xsp.add_argument("--destination", required=True, type=ipv4)
    inject_xsp.add_argument("--source-port", required=True, type=port)
    inject_xsp.add_argument("--destination-port", required=True, type=port)
    inject_xsp.add_argument("--input", required=True)
    inject_xsp.add_argument("--packet-id", type=nonnegative, default=2)
    inject_xsp.add_argument("--flip-last", action="store_true")
    inject_xsp.set_defaults(handler=command_inject_xsp)

    collect_udp = subcommands.add_parser("collect-udp")
    collect_udp.add_argument("--bind", required=True, type=ipv4)
    collect_udp.add_argument("--port", required=True, type=port)
    collect_udp.add_argument("--expected", required=True)
    collect_udp.add_argument("--count", required=True, type=nonnegative)
    collect_udp.add_argument("--timeout", type=float, default=2.0)
    collect_udp.set_defaults(handler=command_collect_udp)

    udp_server = subcommands.add_parser("udp-server")
    udp_server.add_argument("--bind", required=True, type=ipv4)
    udp_server.add_argument("--port", required=True, type=port)
    udp_server.add_argument("--request", required=True)
    udp_server.add_argument("--response", required=True)
    udp_server.add_argument("--timeout", type=float, default=5.0)
    udp_server.set_defaults(handler=command_udp_server)

    udp_client = subcommands.add_parser("udp-client")
    udp_client.add_argument("--destination", required=True, type=ipv4)
    udp_client.add_argument("--port", required=True, type=port)
    udp_client.add_argument("--request", required=True)
    udp_client.add_argument("--response", required=True)
    udp_client.add_argument("--timeout", type=float, default=5.0)
    udp_client.set_defaults(handler=command_udp_client)

    tcp_server = subcommands.add_parser("tcp-server")
    tcp_server.add_argument("--bind", required=True, type=ipv4)
    tcp_server.add_argument("--port", required=True, type=port)
    tcp_server.add_argument("--request", required=True)
    tcp_server.add_argument("--response", required=True)
    tcp_server.add_argument("--timeout", type=float, default=5.0)
    tcp_server.set_defaults(handler=command_tcp_server)

    tcp_client = subcommands.add_parser("tcp-client")
    tcp_client.add_argument("--destination", required=True, type=ipv4)
    tcp_client.add_argument("--port", required=True, type=port)
    tcp_client.add_argument("--request", required=True)
    tcp_client.add_argument("--response", required=True)
    tcp_client.add_argument("--timeout", type=float, default=5.0)
    tcp_client.set_defaults(handler=command_tcp_client)

    icmp = subcommands.add_parser("icmp")
    icmp.add_argument("--destination", required=True, type=ipv4)
    icmp.add_argument("--payload", required=True)
    icmp.add_argument("--sequence", type=positive, default=1)
    icmp.add_argument("--timeout", type=float, default=3.0)
    icmp.add_argument("--count", type=positive, default=1)
    icmp.add_argument("--interval", type=float, default=0.05)
    icmp.add_argument("--output")
    icmp.set_defaults(handler=command_icmp)

    capture_xsp = subcommands.add_parser("capture-xsp")
    capture_xsp.add_argument("--interface", required=True)
    capture_xsp.add_argument("--count", required=True, type=positive)
    capture_xsp.add_argument("--timeout", type=float, default=5.0)
    capture_xsp.add_argument("--output")
    capture_xsp.add_argument("--metadata")
    capture_xsp.add_argument("--forbid-text")
    capture_xsp.add_argument("--forbid-file")
    add_endpoint_filters(capture_xsp)
    capture_xsp.set_defaults(handler=command_capture_xsp)

    capture_relay = subcommands.add_parser("capture-relay")
    capture_relay.add_argument("--interface", required=True)
    capture_relay.add_argument("--count", required=True, type=positive)
    capture_relay.add_argument("--timeout", type=float, default=5.0)
    capture_relay.add_argument("--output")
    capture_relay.add_argument("--metadata")
    capture_relay.add_argument("--forbid-text")
    capture_relay.add_argument("--forbid-file")
    capture_relay.add_argument(
        "--inner-packet-type",
        choices=sorted(XSP_PACKET_TYPES),
        default="data",
    )
    add_udp_endpoint_filters(capture_relay)
    capture_relay.set_defaults(handler=command_capture_relay)

    assert_no_xsp = subcommands.add_parser("assert-no-xsp")
    assert_no_xsp.add_argument("--interface", required=True)
    assert_no_xsp.add_argument("--timeout", type=float, default=1.0)
    add_endpoint_filters(assert_no_xsp)
    assert_no_xsp.set_defaults(handler=command_assert_no_xsp)

    return parser


def main() -> None:
    arguments = build_parser().parse_args()
    arguments.handler(arguments)


if __name__ == "__main__":
    main()
