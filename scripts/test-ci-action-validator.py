#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-ci-actions.py")
SPEC = importlib.util.spec_from_file_location("validate_ci_actions", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load CI action validator")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def validate_fixture(source: str) -> list[str]:
    with tempfile.TemporaryDirectory() as temporary:
        workflow = Path(temporary) / "ci.yml"
        workflow.write_text(source, encoding="utf-8")
        return MODULE.validate_directory(workflow.parent)


def expect_failure(source: str, fragment: str) -> None:
    failures = validate_fixture(source)
    assert failures, "fixture unexpectedly passed"
    assert any(fragment in failure for failure in failures), failures


def main() -> int:
    valid = """jobs:
  test:
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
      - uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
      - uses: docker/setup-buildx-action@bb05f3f5519dd87d3ba754cc423b652a5edd6d2c # v4.2.0
      - uses: example/action@1111111111111111111111111111111111111111
      - uses: ./local-action
"""
    assert validate_fixture(valid) == []

    expect_failure(
        valid.replace(
            "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
            "actions/checkout@v7",
        ),
        "40-character commit SHA",
    )
    expect_failure(
        valid.replace(
            "3d3c42e5aac5ba805825da76410c181273ba90b1",
            "11bd71901bbe5b1630ceea73d27597364c9af683",
        ),
        "reviewed Node 24 commit",
    )
    expect_failure(
        valid.replace("# v7.0.0", "# v6.0.0"),
        "must retain version label v7.0.0",
    )
    expect_failure(
        "jobs:\n  test:\n    steps:\n      - uses: malformed action reference\n",
        "malformed uses entry",
    )
    print("CI action validator regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
