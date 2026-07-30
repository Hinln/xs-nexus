#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
import re
from dataclasses import dataclass
from pathlib import Path

EXCLUDED_DIRECTORIES = {".git", "artifacts", "dist", "node_modules", "target"}
REFERENCE_SECRET_NAMES = {
    "DATABASE_URL",
    "DEV_SERVER_PASSWORD",
    "MYSQL_URL",
    "NAS_PASSWORD",
    "POSTGRESQL_URL",
    "REDIS_URL",
    "SESSION_SECRET",
}
PSEUDOCODE_ASSIGNMENT_VALUES = {"HKDF-Expand("}
SOURCE_SUFFIXES = {".c", ".cc", ".cpp", ".go", ".h", ".hpp", ".js", ".jsx", ".py", ".rs", ".ts", ".tsx"}
SHELL_SUFFIXES = {".bash", ".sh"}
PLACEHOLDERS = {
    "CHANGE_ME",
    "EXAMPLE",
    "PLACEHOLDER",
    "XS_DB_NAME",
    "XS_DB_USER",
    "changeme",
}


@dataclass(frozen=True)
class Finding:
    path: Path
    line: int
    rule: str


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Scan repository files for secrets")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--reference-env",
        type=Path,
        default=Path("/etc/xs-nexus/controller.env"),
    )
    return parser.parse_args()


def reference_values(path: Path) -> dict[str, bytes]:
    if not path.exists():
        return {}

    values: dict[str, bytes] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        name, value = line.split("=", 1)
        if (
            name in REFERENCE_SECRET_NAMES
            and len(value) >= 8
            and value not in PLACEHOLDERS
        ):
            values[name] = value.encode()
    return values


def repository_files(root: Path):
    for directory, names, filenames in os.walk(root, followlinks=False):
        names[:] = [name for name in names if name not in EXCLUDED_DIRECTORIES]
        for filename in filenames:
            path = Path(directory) / filename
            if path.is_symlink() or path.name == ".env":
                continue
            try:
                if path.stat().st_size <= 2_000_000:
                    yield path
            except OSError:
                continue


def scan(root: Path, references: dict[str, bytes]) -> list[Finding]:
    findings: list[Finding] = []
    private_header = re.compile(rb"-----BEGIN (?:OPENSSH |RSA |EC |DSA )?PRIVATE KEY-----")
    credential_uri = re.compile(rb"(?i)(?:postgres(?:ql)?|mysql|redis)://[^\s/@:]+:[^\s/@]+@")
    source_literal = re.compile(rb"^(?:b?[\"'`]|(?:br|r)#*\")")
    assignment = re.compile(
        rb"(?i)(?:pass"
        rb"word|passwd|sec"
        rb"ret|token|private[_-]?key)\s*(?::(?!:)|=(?!>))\s*([^\s]+)"
    )

    for path in repository_files(root):
        try:
            data = path.read_bytes()
        except OSError:
            continue
        relative = path.relative_to(root)
        lines = data.splitlines()

        for name, value in references.items():
            for line_number, line in enumerate(lines, start=1):
                if value in line:
                    findings.append(Finding(relative, line_number, f"reference:{name}"))

        for line_number, line in enumerate(lines, start=1):
            if private_header.search(line):
                findings.append(Finding(relative, line_number, "private-key"))
            if path.name != ".env.example" and credential_uri.search(line):
                findings.append(Finding(relative, line_number, "credential-uri"))
            if path.name != ".env.example":
                match = assignment.search(line)
                if match is not None:
                    raw_value = match.group(1).strip()
                    value = raw_value.strip(b"\"'").decode(errors="ignore")
                    rust_type_annotation = (
                        path.suffix == ".rs"
                        and b"=" not in line
                        and line.rstrip().endswith(b",")
                    )
                    source_nonliteral_assignment = (
                        path.suffix in SOURCE_SUFFIXES
                        and source_literal.match(raw_value) is None
                    )
                    shell_nonliteral_assignment = (
                        path.suffix in SHELL_SUFFIXES
                        and re.search(rb"\$(?:[A-Za-z_{(])", raw_value) is not None
                    )
                    if (
                        value not in PLACEHOLDERS
                        and len(value) >= 8
                        and value not in PSEUDOCODE_ASSIGNMENT_VALUES
                        and not rust_type_annotation
                        and not source_nonliteral_assignment
                        and not shell_nonliteral_assignment
                    ):
                        findings.append(Finding(relative, line_number, "secret-assignment"))

    return sorted(set(findings), key=lambda item: (str(item.path), item.line, item.rule))


def main() -> int:
    arguments = parse_arguments()
    root = arguments.root.resolve()
    findings = scan(root, reference_values(arguments.reference_env))
    if findings:
        for finding in findings:
            print(f"{finding.path}:{finding.line}: {finding.rule}")
        print(f"secret scan failed with {len(findings)} finding(s)")
        return 1

    print("secret scan passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
