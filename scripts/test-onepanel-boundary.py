#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = Path(__file__).with_name("validate-onepanel-boundary.py")
SPEC = importlib.util.spec_from_file_location("validate_onepanel_boundary", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load 1Panel boundary validator")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def expect_failure(
    application_compose: str,
    edge_compose: str,
    stack: str,
    edge: str,
    fragment: str,
) -> None:
    failures = MODULE.validate_sources(application_compose, edge_compose, stack, edge)
    assert failures, "fixture unexpectedly passed"
    assert any(fragment in failure for failure in failures), failures


def main() -> int:
    application_compose = (ROOT / "deploy/docker/compose.yaml").read_text(encoding="utf-8")
    edge_compose = (ROOT / "deploy/docker/edge.compose.yaml").read_text(encoding="utf-8")
    stack = (ROOT / "deploy/docker/xs-nexus-stack.sh").read_text(encoding="utf-8")
    edge = (ROOT / "deploy/docker/xs-nexus-edge.sh").read_text(encoding="utf-8")
    assert MODULE.validate_sources(application_compose, edge_compose, stack, edge) == []

    expect_failure(
        application_compose.replace("external: true", "external: false", 1),
        edge_compose,
        stack,
        edge,
        "must be external",
    )
    expect_failure(
        application_compose.replace("      - 1panel-network", "      - isolated", 1),
        edge_compose,
        stack,
        edge,
        "is not attached",
    )
    expect_failure(
        application_compose,
        edge_compose,
        f"{stack}\ndocker system prune\n",
        edge,
        "forbidden lifecycle command",
    )
    expect_failure(
        application_compose,
        edge_compose,
        stack.replace("down --remove-orphans", "down --remove-orphans --volumes", 1),
        edge,
        "must not delete volumes",
    )
    expect_failure(
        application_compose,
        edge_compose.replace("    read_only: true", "    privileged: true", 1),
        stack,
        edge,
        "forbidden setting",
    )
    print("1Panel boundary validator regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
