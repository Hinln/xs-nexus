#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DIGEST_IMAGE_RE = re.compile(r"^[^\s@]+@sha256:[0-9a-f]{64}$")


def check(checks: list[dict[str, str]], check_id: str, passed: bool) -> None:
    checks.append({"check_id": check_id, "status": "PASS" if passed else "FAIL"})


def dictionary(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def string_list(value: Any) -> list[str]:
    return [item for item in value if isinstance(item, str)] if isinstance(value, list) else []


def audit_document(
    document: Any,
    container_name: str,
    network_name: str,
    max_size: str,
    max_file: str,
) -> dict[str, Any]:
    checks: list[dict[str, str]] = []
    containers = document if isinstance(document, list) else []
    check(checks, "single_inspect_record", len(containers) == 1)
    container = dictionary(containers[0]) if len(containers) == 1 else {}
    config = dictionary(container.get("Config"))
    host_config = dictionary(container.get("HostConfig"))
    state = dictionary(container.get("State"))
    health = dictionary(state.get("Health"))
    labels = dictionary(config.get("Labels"))
    log_config = dictionary(host_config.get("LogConfig"))
    log_options = dictionary(log_config.get("Config"))
    networks = dictionary(dictionary(container.get("NetworkSettings")).get("Networks"))
    environments = string_list(config.get("Env"))
    mounts = container.get("Mounts") if isinstance(container.get("Mounts"), list) else []

    check(checks, "container_name", container.get("Name") == f"/{container_name}")
    check(checks, "database_role_label", labels.get("com.xs-nexus.role") == "database")
    check(
        checks,
        "deployment_label",
        isinstance(labels.get("com.xs-nexus.deployment"), str)
        and bool(labels.get("com.xs-nexus.deployment")),
    )
    check(
        checks,
        "digest_pinned_image",
        isinstance(config.get("Image"), str) and bool(DIGEST_IMAGE_RE.fullmatch(config["Image"])),
    )
    check(checks, "running", state.get("Running") is True)
    check(checks, "healthy", health.get("Status") == "healthy")
    check(checks, "non_root_user", config.get("User") not in {None, "", "0", "0:0", "root"})
    check(checks, "read_only_root", host_config.get("ReadonlyRootfs") is True)
    check(checks, "not_privileged", host_config.get("Privileged") is False)
    check(checks, "drop_all_capabilities", "ALL" in string_list(host_config.get("CapDrop")))
    check(
        checks,
        "no_new_privileges",
        "no-new-privileges:true" in string_list(host_config.get("SecurityOpt")),
    )
    check(
        checks,
        "bounded_pids",
        isinstance(host_config.get("PidsLimit"), int) and host_config["PidsLimit"] > 0,
    )
    check(
        checks,
        "bounded_memory",
        isinstance(host_config.get("Memory"), int) and host_config["Memory"] > 0,
    )
    check(
        checks,
        "restart_policy",
        dictionary(host_config.get("RestartPolicy")).get("Name") == "unless-stopped",
    )
    check(
        checks,
        "no_host_ports",
        host_config.get("PortBindings") is None or host_config.get("PortBindings") == {},
    )
    check(
        checks,
        "external_network_only",
        host_config.get("NetworkMode") == network_name and set(networks) == {network_name},
    )
    check(checks, "json_file_logging", log_config.get("Type") == "json-file")
    check(checks, "bounded_log_size", log_options.get("max-size") == max_size)
    check(checks, "bounded_log_files", log_options.get("max-file") == max_file)
    check(
        checks,
        "password_file_environment",
        "POSTGRES_PASSWORD_FILE=/run/secrets/postgres-password" in environments,
    )
    check(
        checks,
        "no_password_environment",
        not any(value.startswith("POSTGRES_PASSWORD=") for value in environments),
    )

    secret_mount = next(
        (
            dictionary(mount)
            for mount in mounts
            if dictionary(mount).get("Destination") == "/run/secrets/postgres-password"
        ),
        {},
    )
    data_mount = next(
        (
            dictionary(mount)
            for mount in mounts
            if dictionary(mount).get("Destination") == "/var/lib/postgresql"
        ),
        {},
    )
    check(
        checks,
        "read_only_password_mount",
        secret_mount.get("Type") == "bind" and secret_mount.get("RW") is False,
    )
    check(
        checks,
        "database_data_mount",
        data_mount.get("Type") == "bind" and data_mount.get("RW") is True,
    )
    check(
        checks,
        "healthcheck_configured",
        bool(string_list(dictionary(config.get("Healthcheck")).get("Test"))),
    )

    failed = [item["check_id"] for item in checks if item["status"] == "FAIL"]
    return {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "container": container_name,
        "status": "PASS" if not failed else "FAIL",
        "checks": checks,
        "failed_check_ids": failed,
    }


def write_report(path: Path | None, report: dict[str, Any]) -> None:
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if path is None:
        print(rendered, end="")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(rendered, encoding="utf-8")
    os.replace(temporary, path)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Audit the production PostgreSQL container policy.")
    parser.add_argument("--inspect", required=True, type=Path)
    parser.add_argument("--container", default="xs-nexus-rc-postgres")
    parser.add_argument("--network", default="1panel-network")
    parser.add_argument("--max-size", default="10m")
    parser.add_argument("--max-file", default="5")
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    if not arguments.inspect.is_file() or arguments.inspect.is_symlink():
        parser.error("inspect input must be a regular non-symlink file")
    if arguments.inspect.stat().st_size > 16 * 1024 * 1024:
        parser.error("inspect input exceeds 16 MiB")
    for name, value in (
        ("container", arguments.container),
        ("network", arguments.network),
        ("max-size", arguments.max_size),
        ("max-file", arguments.max_file),
    ):
        if not value or any(character.isspace() or ord(character) < 32 for character in value):
            parser.error(f"{name} contains invalid characters")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    try:
        document = json.loads(arguments.inspect.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"failed to read Docker inspect JSON: {error.__class__.__name__}") from error
    report = audit_document(
        document,
        arguments.container,
        arguments.network,
        arguments.max_size,
        arguments.max_file,
    )
    write_report(arguments.output, report)
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
