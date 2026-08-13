#!/usr/bin/env python3

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCANNED_ROOTS = ("apps", "crates", "deploy", "drivers", "installers", "scripts")
SCANNED_SUFFIXES = {".c", ".h", ".json", ".ps1", ".py", ".rs", ".sh", ".toml", ".yaml", ".yml"}
FORBIDDEN_PATTERNS = {
    "coturn": re.compile(r"(?i)\bcoturn\b"),
    "frp": re.compile(r"(?i)\bfrp\b"),
    "headscale": re.compile(r"(?i)\bheadscale\b"),
    "nebula": re.compile(r"(?i)\bnebula\b"),
    "netbird": re.compile(r"(?i)\bnetbird\b"),
    "nps": re.compile(r"(?i)\bnps\b"),
    "openvpn": re.compile(r"(?i)\bopenvpn\b"),
    "rathole": re.compile(r"(?i)\brathole\b"),
    "softether": re.compile(r"(?i)\bsoftether\b"),
    "tailscale": re.compile(r"(?i)\btailscale\b"),
    "tap-windows": re.compile(r"(?i)\btap-windows\b"),
    "wintun": re.compile(r"(?i)\bwintun\b"),
    "wireguard": re.compile(r"(?i)\bwireguard\b"),
    "zerotier": re.compile(r"(?i)\bzerotier\b"),
}
ALLOWED_NEGATIVE_REFERENCES = {
    ("apps/agent/src/subnet_routes.rs", "tailscale"): 1,
    ("scripts/validate-windows-xsnet-source.py", "tap-windows"): 1,
    ("scripts/validate-windows-xsnet-source.py", "wintun"): 1,
}
APPROVED_WINTUN_ROOTS = (
    "apps/agent/",
    "crates/windows-wintun/",
    "installers/windows/",
)
APPROVED_WINTUN_TOOL_FILES = {
    "scripts/windows/test-native-agent.ps1",
}
SELF = Path(__file__).resolve()
POLICY_TOOLS = {
    SELF,
    (ROOT / "scripts" / "generate-source-sbom.py").resolve(),
    (ROOT / "scripts" / "test-independent-implementation-validator.py").resolve(),
    (ROOT / "scripts" / "test-source-sbom.py").resolve(),
}


def is_approved_reference(relative: str, name: str) -> bool:
    if name == "wintun":
        return relative.startswith(APPROVED_WINTUN_ROOTS) or relative in APPROVED_WINTUN_TOOL_FILES
    return name == "wireguard" and relative.startswith("installers/windows/")


def source_files():
    for root_name in SCANNED_ROOTS:
        for path in sorted((ROOT / root_name).rglob("*")):
            if not path.is_file() or path.resolve() in POLICY_TOOLS:
                continue
            if path.suffix in SCANNED_SUFFIXES or path.name.startswith("Dockerfile"):
                yield path


def main():
    observed = {}
    unexpected = []
    for path in source_files():
        relative = path.relative_to(ROOT).as_posix()
        text = path.read_text(encoding="utf-8")
        for name, pattern in FORBIDDEN_PATTERNS.items():
            count = len(pattern.findall(text))
            if count == 0:
                continue
            key = (relative, name)
            if is_approved_reference(relative, name):
                continue
            observed[key] = count
            if key not in ALLOWED_NEGATIVE_REFERENCES:
                unexpected.append(f"{relative}: forbidden runtime reference {name} ({count})")
    if unexpected:
        print("\n".join(unexpected), file=sys.stderr)
        return 2
    if observed != ALLOWED_NEGATIVE_REFERENCES:
        print(
            f"negative-reference allowlist mismatch: expected={ALLOWED_NEGATIVE_REFERENCES} observed={observed}",
            file=sys.stderr,
        )
        return 2
    print("independent implementation source validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
