#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path


EXPECTED_APPLICATION_SERVICES = {
    "controller",
    "db-tools",
    "migration",
    "network-probe",
    "relay",
    "console",
}
FORBIDDEN_LIFECYCLE_PATTERNS = (
    re.compile(r"\bdocker\s+system\s+prune\b"),
    re.compile(r"\bdocker\s+network\s+(?:create|rm|prune|connect|disconnect)\b"),
    re.compile(r"\bdocker\s+(?:container|image|volume|builder)\s+prune\b"),
    re.compile(r"\bdocker\s+(?:rm|rmi)\b"),
    re.compile(r"\bdocker\s+volume\s+rm\b"),
    re.compile(r"\bdocker\s+compose\s+down\b"),
)


def indented_blocks(source: str, parent: str) -> dict[str, str]:
    lines = source.splitlines()
    parent_index = next(
        (index for index, line in enumerate(lines) if line == f"{parent}:"),
        None,
    )
    if parent_index is None:
        return {}
    blocks: dict[str, str] = {}
    current_name: str | None = None
    current_lines: list[str] = []
    for line in lines[parent_index + 1 :]:
        if line and not line.startswith(" "):
            break
        match = re.fullmatch(r"  ([A-Za-z0-9_-]+):", line)
        if match is not None:
            if current_name is not None:
                blocks[current_name] = "\n".join(current_lines)
            current_name = match.group(1)
            current_lines = [line]
        elif current_name is not None:
            current_lines.append(line)
    if current_name is not None:
        blocks[current_name] = "\n".join(current_lines)
    return blocks


def validate_compose_source(
    source: str,
    expected_services: set[str],
    require_project_name: bool,
) -> list[str]:
    failures: list[str] = []
    if require_project_name and not source.startswith(
        "name: ${XS_COMPOSE_PROJECT_NAME:-xs-nexus-dev}\n"
    ):
        failures.append("application compose must use the isolated project name")
    services = indented_blocks(source, "services")
    if set(services) != expected_services:
        failures.append(
            f"compose services differ: expected {sorted(expected_services)}, got {sorted(services)}"
        )
    for name, block in services.items():
        if "1panel-network" not in block:
            failures.append(f"service {name} is not attached to 1panel-network")
        for forbidden in ("privileged:", "network_mode: host", "/var/run/docker.sock"):
            if forbidden in block:
                failures.append(f"service {name} contains forbidden setting {forbidden!r}")
    networks = indented_blocks(source, "networks")
    if set(networks) != {"1panel-network"}:
        failures.append("compose must define only the external 1panel-network reference")
        return failures
    network = networks["1panel-network"]
    if not re.search(r"^    external:\s*true\s*$", network, re.MULTILINE):
        failures.append("1panel-network must be external")
    if not re.search(r"^    name:\s*1panel-network\s*$", network, re.MULTILINE):
        failures.append("1panel-network must retain its exact external name")
    for managed_key in ("driver:", "driver_opts:", "ipam:", "subnet:", "attachable:"):
        if managed_key in network:
            failures.append(f"external network must not contain managed key {managed_key!r}")
    return failures


def validate_lifecycle_source(stack: str, edge: str) -> list[str]:
    failures: list[str] = []
    for name, source in (("xs-nexus-stack.sh", stack), ("xs-nexus-edge.sh", edge)):
        for pattern in FORBIDDEN_LIFECYCLE_PATTERNS:
            if pattern.search(source):
                failures.append(f"{name} contains forbidden lifecycle command {pattern.pattern!r}")
    required_stack = (
        "COMPOSE=(docker compose --env-file \"$ENVIRONMENT_FILE\" -f \"$COMPOSE_FILE\")",
        "'1panel-network bridge 172.18.0.0/16 '",
        'assert network.get("external") is True',
        'assert network.get("name") == "1panel-network"',
        '"${COMPOSE[@]}" down --remove-orphans',
    )
    for fragment in required_stack:
        if fragment not in stack:
            failures.append(f"xs-nexus-stack.sh is missing {fragment!r}")
    if stack.count('"${COMPOSE[@]}" down --remove-orphans') != 2:
        failures.append("stack cleanup must have exactly two project-scoped down paths")
    if re.search(r"\bdown\b[^\n]*(?:--volumes|(?:^|\s)-v(?:\s|$))", stack):
        failures.append("stack cleanup must not delete volumes")
    required_edge = (
        "'1panel-network bridge 172.18.0.0/16 '",
        '"${COMPOSE[@]}" stop --timeout 15 edge',
        '"${COMPOSE[@]}" rm -f edge',
    )
    for fragment in required_edge:
        if fragment not in edge:
            failures.append(f"xs-nexus-edge.sh is missing {fragment!r}")
    return failures


def validate_sources(
    application_compose: str,
    edge_compose: str,
    stack: str,
    edge: str,
) -> list[str]:
    return [
        *validate_compose_source(
            application_compose,
            EXPECTED_APPLICATION_SERVICES,
            require_project_name=True,
        ),
        *validate_compose_source(edge_compose, {"edge"}, require_project_name=False),
        *validate_lifecycle_source(stack, edge),
    ]


def validate_rendered_compose(root: Path) -> list[str]:
    docker = shutil.which("docker")
    if docker is None:
        return ["docker CLI is unavailable for rendered Compose validation"]
    command = [
        docker,
        "compose",
        "-f",
        str(root / "deploy/docker/compose.yaml"),
        "-f",
        str(root / "deploy/docker/edge.compose.yaml"),
        "config",
        "--no-interpolate",
        "--format",
        "json",
    ]
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        return [f"docker compose config failed: {detail}"]
    try:
        configuration = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        return [f"docker compose config returned invalid JSON: {error}"]
    failures: list[str] = []
    network = configuration.get("networks", {}).get("1panel-network", {})
    if network.get("external") is not True or network.get("name") != "1panel-network":
        failures.append("rendered Compose does not retain the external 1panel-network")
    services = configuration.get("services", {})
    expected = EXPECTED_APPLICATION_SERVICES | {"edge"}
    if set(services) != expected:
        failures.append("rendered Compose service set differs from the reviewed set")
    for name, service in services.items():
        networks = service.get("networks", {})
        if "1panel-network" not in networks:
            failures.append(f"rendered service {name} is not attached to 1panel-network")
    return failures


def validate_repository(root: Path, require_compose: bool = False) -> list[str]:
    failures = validate_sources(
        (root / "deploy/docker/compose.yaml").read_text(encoding="utf-8"),
        (root / "deploy/docker/edge.compose.yaml").read_text(encoding="utf-8"),
        (root / "deploy/docker/xs-nexus-stack.sh").read_text(encoding="utf-8"),
        (root / "deploy/docker/xs-nexus-edge.sh").read_text(encoding="utf-8"),
    )
    if require_compose:
        failures.extend(validate_rendered_compose(root))
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--require-compose", action="store_true")
    arguments = parser.parse_args()
    failures = validate_repository(arguments.root.resolve(), arguments.require_compose)
    if failures:
        print("1Panel boundary validation failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("1Panel external-network boundary validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
