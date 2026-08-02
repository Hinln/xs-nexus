#!/usr/bin/env python3

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PLAN = ROOT / "EXECUTION_PLAN.md"
ALLOWED = {
    "NOT_STARTED",
    "IN_PROGRESS",
    "BLOCKED_EXTERNAL",
    "FAILED",
    "PASSED",
    "DEFERRED_APPROVED",
}
MILESTONE = re.compile(r"^## (M\d+\.\d+)\s+.+$")
STATUS = re.compile(r"^- 状态：`([^`]+)`$")
OVERALL = re.compile(r"^当前总状态：`([^`]+)`$")


def main() -> int:
    lines = PLAN.read_text(encoding="utf-8").splitlines()
    errors: list[str] = []
    overall: list[tuple[int, str]] = []
    milestones: dict[str, tuple[int, str | None]] = {}
    current: str | None = None

    for number, line in enumerate(lines, start=1):
        if match := OVERALL.fullmatch(line):
            overall.append((number, match.group(1)))
            continue
        if match := MILESTONE.fullmatch(line):
            current = match.group(1)
            if current in milestones:
                errors.append(f"line {number}: duplicate milestone {current}")
            else:
                milestones[current] = (number, None)
            continue
        if match := STATUS.fullmatch(line):
            value = match.group(1)
            if current is None:
                errors.append(f"line {number}: status is outside a milestone")
            else:
                heading_line, previous = milestones[current]
                if previous is not None:
                    errors.append(f"line {number}: duplicate status for {current}")
                milestones[current] = (heading_line, value)

    if len(overall) != 1:
        errors.append(f"expected exactly one overall status, found {len(overall)}")
    elif overall[0][1] not in ALLOWED:
        errors.append(
            f"line {overall[0][0]}: invalid overall status {overall[0][1]!r}"
        )

    if not milestones:
        errors.append("no milestones found")
    for name, (heading_line, value) in milestones.items():
        if value is None:
            errors.append(f"line {heading_line}: {name} has no status")
        elif value not in ALLOWED:
            errors.append(f"line {heading_line}: {name} has invalid status {value!r}")

    if errors:
        raise SystemExit("execution plan status validation failed:\n- " + "\n- ".join(errors))

    print(
        "execution plan statuses passed: "
        f"overall={overall[0][1]}, milestones={len(milestones)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
