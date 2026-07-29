#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

HEADER_FORMAT = "!4sBBHHH16s16s16s16sIQII"


def build_vector() -> dict[str, object]:
    network_id = bytes(range(0x00, 0x10))
    source_node_id = bytes(range(0x10, 0x20))
    destination_node_id = bytes(range(0x20, 0x30))
    session_id = bytes(range(0x30, 0x40))
    values = {
        "magic": b"XSP1",
        "version": 1,
        "packet_type": 1,
        "flags": 1,
        "header_length": 96,
        "payload_length": 32,
        "network_id": network_id,
        "source_node_id": source_node_id,
        "destination_node_id": destination_node_id,
        "session_id": session_id,
        "key_epoch": 2,
        "sequence": 0x0000000001020304,
        "path_id": 0x0A0B0C0D,
        "reserved": 0,
    }
    header = struct.pack(
        HEADER_FORMAT,
        values["magic"],
        values["version"],
        values["packet_type"],
        values["flags"],
        values["header_length"],
        values["payload_length"],
        values["network_id"],
        values["source_node_id"],
        values["destination_node_id"],
        values["session_id"],
        values["key_epoch"],
        values["sequence"],
        values["path_id"],
        values["reserved"],
    )
    if len(header) != 96:
        raise RuntimeError(f"unexpected header length: {len(header)}")

    return {
        "schema": "xsp1-data-header-v1",
        "header_length": len(header),
        "datagram_length_with_ciphertext_and_tag": len(header) + 32 + 16,
        "fields": {
            "magic_ascii": "XSP1",
            "version": values["version"],
            "packet_type": values["packet_type"],
            "flags": values["flags"],
            "payload_length": values["payload_length"],
            "network_id_hex": network_id.hex(),
            "source_node_id_hex": source_node_id.hex(),
            "destination_node_id_hex": destination_node_id.hex(),
            "session_id_hex": session_id.hex(),
            "key_epoch": values["key_epoch"],
            "sequence": values["sequence"],
            "path_id": values["path_id"],
            "reserved": values["reserved"],
        },
        "header_hex": header.hex(),
        "header_sha256": hashlib.sha256(header).hexdigest(),
        "aad_hex": header.hex(),
    }


def encoded_vector() -> str:
    return json.dumps(build_vector(), ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate deterministic XSP/1 vectors")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", type=Path)
    group.add_argument("--check", type=Path)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    expected = encoded_vector()
    if arguments.write is not None:
        arguments.write.parent.mkdir(parents=True, exist_ok=True)
        arguments.write.write_text(expected, encoding="utf-8")
        print(f"wrote {arguments.write}")
        return 0

    actual = arguments.check.read_text(encoding="utf-8")
    if actual != expected:
        print(f"vector mismatch: {arguments.check}")
        return 1
    print(f"vector check passed: {arguments.check}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
