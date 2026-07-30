#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import ipaddress
import json
import subprocess
from pathlib import Path

REQUIRED_DOCUMENTS = {
    "docs/ARCHITECTURE.md": ["## 2. 逻辑组件", "## 5. 信任边界", "## 7. 故障行为"],
    "docs/REFERENCE_RESEARCH.md": ["## 1. 公开标准", "## 4. 研究流程"],
    "docs/CLEAN_ROOM_LOG.md": ["## 2026-07-29 / M0.2 初始设计", "### 未使用"],
    "docs/PROTOCOL_ORIGINALITY.md": ["## 1. 独立设计内容", "## 3. 明确不兼容"],
    "docs/THREAT_MODEL.md": ["## 5. 安全不变量", "## 6. 威胁与控制", "## 8. 测试映射"],
    "docs/SECURITY_ASSUMPTIONS.md": ["## 10. 明确未解决", "## 11. 失败关闭策略"],
    "docs/CRYPTOGRAPHIC_DESIGN.md": ["## 3. transcript", "## 7. 数据 nonce 与 AAD", "## 9. 抗重放"],
    "docs/CONTROLLER_API.md": ["## 6. 节点 Enrollment", "## 8. WebSocket 控制连接", "## 9. 数据库与审计"],
    "docs/XSP1_PROTOCOL.md": ["## 5. 握手帧", "## 7. 数据包", "## 19. 测试向量和 Fuzz"],
    "THIRD_PARTY.md": ["## 1. 当前运行时直接依赖", "## 6. 禁止依赖"],
}


def validate_documents(root: Path) -> None:
    for relative, markers in REQUIRED_DOCUMENTS.items():
        path = root / relative
        if not path.is_file():
            raise RuntimeError(f"missing required document: {relative}")
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                raise RuntimeError(f"missing marker in {relative}: {marker}")
        for unfinished in ("TODO", "TBD", "待补充"):
            if unfinished in text:
                raise RuntimeError(f"unfinished marker in {relative}: {unfinished}")


def validate_lengths() -> None:
    credential = [1, 3, 16, 16, 32, 4, 8, 8, 8, 4, 32, 4, 64]
    client_hello = [16, 16, 16, 32, 32, 8, 2, 200, 1, 2, 64]
    server_hello = [16, 16, 16, 32, 32, 32, 32, 16, 8, 2, 2, 200, 64]
    finish = [16, 8, 2, 2, 32, 16]
    data_header = [4, 1, 1, 2, 2, 2, 16, 16, 16, 16, 4, 8, 4, 4]
    discovery_request = [4, 1, 1, 2, 2, 2, 16, 16, 16, 8, 2, 200, 64]
    discovery_response = [4, 1, 1, 2, 2, 2, 16, 16, 16, 8, 24, 32, 64]
    expected = {
        "credential": (credential, 200),
        "client_hello": (client_hello, 389),
        "server_hello": (server_hello, 468),
        "finish": (finish, 76),
        "data_header": (data_header, 96),
        "discovery_request": (discovery_request, 334),
        "discovery_response": (discovery_response, 188),
    }
    for name, (parts, total) in expected.items():
        actual = sum(parts)
        if actual != total:
            raise RuntimeError(f"{name} length is {actual}, expected {total}")


def validate_data_header_vector(root: Path) -> None:
    vector_path = root / "tests/vectors/xsp1/data-header-v1.json"
    subprocess.run(
        [
            "python3",
            str(root / "scripts/generate-xsp1-vectors.py"),
            "--check",
            str(vector_path),
        ],
        check=True,
    )
    vector = json.loads(vector_path.read_text(encoding="utf-8"))
    header = bytes.fromhex(vector["header_hex"])
    if len(header) != 96:
        raise RuntimeError("checked-in data header is not 96 bytes")
    if hashlib.sha256(header).hexdigest() != vector["header_sha256"]:
        raise RuntimeError("checked-in data header hash mismatch")
    if vector["aad_hex"] != vector["header_hex"]:
        raise RuntimeError("AAD must be the exact 96-byte header")


def validate_credential_vector(root: Path) -> None:
    vector_path = root / "tests/vectors/xsp1/credential-v1.json"
    vector = json.loads(vector_path.read_text(encoding="utf-8"))
    credential = bytes.fromhex(vector["credential_hex"])
    if len(credential) != 200:
        raise RuntimeError("checked-in credential is not 200 bytes")
    if hashlib.sha256(credential).hexdigest() != vector["credential_sha256"]:
        raise RuntimeError("checked-in credential hash mismatch")

    expected_fields = {
        "version": (credential[0], 1),
        "reserved": (credential[1:4], b"\x00\x00\x00"),
        "network_id": (credential[4:20], bytes.fromhex(vector["network_id_hex"])),
        "node_id": (credential[20:36], bytes.fromhex(vector["node_id_hex"])),
        "identity_public_key": (
            credential[36:68],
            bytes.fromhex(vector["identity_public_key_hex"]),
        ),
        "virtual_ipv4": (
            credential[68:72],
            ipaddress.IPv4Address(vector["virtual_ipv4"]).packed,
        ),
        "serial": (
            int.from_bytes(credential[72:80], "big"),
            vector["serial"],
        ),
        "not_before": (
            int.from_bytes(credential[80:88], "big"),
            vector["not_before"],
        ),
        "not_after": (
            int.from_bytes(credential[88:96], "big"),
            vector["not_after"],
        ),
        "role_bitmap": (
            int.from_bytes(credential[96:100], "big"),
            vector["role_bitmap"],
        ),
        "role_set_digest": (
            credential[100:132],
            bytes.fromhex(vector["role_set_digest_hex"]),
        ),
        "controller_key_id": (
            int.from_bytes(credential[132:136], "big"),
            vector["controller_key_id"],
        ),
    }
    for name, (actual, expected) in expected_fields.items():
        if actual != expected:
            raise RuntimeError(f"credential vector {name} mismatch")

    corpus = (root / "fuzz/corpus/credential/valid-v1.bin").read_bytes()
    if corpus != credential:
        raise RuntimeError("credential Fuzz corpus differs from the signed vector")


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    validate_documents(root)
    validate_lengths()
    validate_data_header_vector(root)
    validate_credential_vector(root)
    print("M0.2 specification validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
