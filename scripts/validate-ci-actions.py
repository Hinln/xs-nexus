#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


USES_PATTERN = re.compile(
    r"^\s*(?:-\s*)?uses:\s*(?P<reference>[^\s#]+)(?:\s+#\s*(?P<label>\S+))?\s*$"
)
SHA_PATTERN = re.compile(r"^[0-9a-f]{40}$")
REVIEWED_NODE24_ACTIONS = {
    "actions/checkout": ("3d3c42e5aac5ba805825da76410c181273ba90b1", "v7.0.1"),
    "actions/setup-node": ("820762786026740c76f36085b0efc47a31fe5020", "v7.0.0"),
    "actions/upload-artifact": ("043fb46d1a93c77aae656e7c1c64a875d1fc6a0a", "v7.0.1"),
    "docker/setup-buildx-action": ("bb05f3f5519dd87d3ba754cc423b652a5edd6d2c", "v4.2.0"),
}
WINDOWS_NATIVE_JOB_FRAGMENTS = (
    "    runs-on: windows-2025",
    "    timeout-minutes: 30",
    "          toolchain: 1.94.0",
    "          components: clippy",
    "      - run: python scripts/validate-windows-native-agent.py",
    "          Push-Location $env:RUNNER_TEMP",
    '            & "$env:GITHUB_WORKSPACE/scripts/windows/test-native-agent.ps1" -EvidenceDirectory artifacts/windows-native',
    "            Pop-Location",
    "        if: always()",
    "          name: windows-native-evidence",
    "          path: artifacts/windows-native",
    "          if-no-files-found: error",
)


def job_block(lines: list[str], job_name: str) -> list[str] | None:
    heading = f"  {job_name}:"
    try:
        start = lines.index(heading)
    except ValueError:
        return None
    end = len(lines)
    for index in range(start + 1, len(lines)):
        if re.fullmatch(r"  [A-Za-z0-9_-]+:", lines[index]):
            end = index
            break
    return lines[start:end]


def validate_ci_contract(path: Path, text: str) -> list[str]:
    if not re.search(r"(?m)^name:\s*ci\s*$", text):
        return []
    failures: list[str] = []
    block = job_block(text.splitlines(), "windows-native")
    if block is None:
        return [f"{path}: missing required windows-native job"]
    rendered = "\n".join(block)
    for fragment in WINDOWS_NATIVE_JOB_FRAGMENTS:
        if fragment not in rendered:
            failures.append(
                f"{path}: windows-native job is missing required fragment {fragment.strip()!r}"
            )
    return failures


def validate_workflow(path: Path, text: str) -> list[str]:
    failures: list[str] = []
    lines = text.splitlines()
    for line_number, line in enumerate(lines, start=1):
        stripped = line.lstrip()
        if not (stripped.startswith("uses:") or stripped.startswith("- uses:")):
            continue
        match = USES_PATTERN.fullmatch(line)
        if match is None:
            failures.append(f"{path}:{line_number}: malformed uses entry")
            continue
        reference = match.group("reference")
        if reference.startswith("./"):
            continue
        action_name, separator, revision = reference.rpartition("@")
        if not separator or not action_name or not SHA_PATTERN.fullmatch(revision):
            failures.append(
                f"{path}:{line_number}: remote action must use a lowercase 40-character commit SHA"
            )
            continue
        reviewed = REVIEWED_NODE24_ACTIONS.get(action_name)
        if reviewed is None:
            continue
        expected_revision, expected_label = reviewed
        if revision != expected_revision:
            failures.append(
                f"{path}:{line_number}: {action_name} must use reviewed Node 24 commit {expected_revision}"
            )
        if match.group("label") != expected_label:
            failures.append(
                f"{path}:{line_number}: {action_name} must retain version label {expected_label}"
            )
        if action_name == "actions/checkout":
            step_indent = len(line) - len(line.lstrip())
            checkout_block: list[str] = []
            for following_line in lines[line_number:]:
                following_indent = len(following_line) - len(following_line.lstrip())
                if following_line.lstrip().startswith("- ") and following_indent <= step_indent:
                    break
                checkout_block.append(following_line)
            if not any(
                re.fullmatch(r"\s*persist-credentials:\s*false\s*", block_line)
                for block_line in checkout_block
            ):
                failures.append(
                    f"{path}:{line_number}: actions/checkout must set persist-credentials: false"
                )
    failures.extend(validate_ci_contract(path, text))
    return failures


def workflow_files(root: Path) -> list[Path]:
    return sorted((*root.glob("*.yml"), *root.glob("*.yaml")))


def validate_directory(root: Path) -> list[str]:
    files = workflow_files(root)
    if not files:
        return [f"{root}: no workflow files found"]
    failures: list[str] = []
    for path in files:
        failures.extend(validate_workflow(path, path.read_text(encoding="utf-8")))
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workflows",
        type=Path,
        default=Path(".github/workflows"),
    )
    arguments = parser.parse_args()
    failures = validate_directory(arguments.workflows)
    if failures:
        print("CI action validation failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("CI action SHA and Node 24 review validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
