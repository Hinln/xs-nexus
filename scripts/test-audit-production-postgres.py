#!/usr/bin/env python3

from __future__ import annotations

import copy
import importlib.util
import json
import sys
from pathlib import Path


SCRIPT = Path(__file__).with_name("audit-production-postgres.py")
SPEC = importlib.util.spec_from_file_location("audit_production_postgres", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load production PostgreSQL audit module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def fixture() -> list[dict[str, object]]:
    return [
        {
            "Name": "/xs-nexus-rc-postgres",
            "Config": {
                "Image": "postgres@sha256:" + "a" * 64,
                "User": "70:70",
                "Labels": {
                    "com.xs-nexus.deployment": "rc",
                    "com.xs-nexus.role": "database",
                },
                "Env": [
                    "POSTGRES_USER=xs_nexus",
                    "POSTGRES_DB=xs_nexus",
                    "POSTGRES_PASSWORD_FILE=/run/secrets/postgres-password",
                ],
                "Healthcheck": {"Test": ["CMD-SHELL", "pg_isready"]},
            },
            "State": {"Running": True, "Health": {"Status": "healthy"}},
            "HostConfig": {
                "ReadonlyRootfs": True,
                "Privileged": False,
                "CapDrop": ["ALL"],
                "SecurityOpt": ["no-new-privileges:true"],
                "PidsLimit": 256,
                "Memory": 805306368,
                "RestartPolicy": {"Name": "unless-stopped"},
                "PortBindings": {},
                "NetworkMode": "1panel-network",
                "LogConfig": {
                    "Type": "json-file",
                    "Config": {"max-size": "10m", "max-file": "5"},
                },
            },
            "NetworkSettings": {"Networks": {"1panel-network": {}}},
            "Mounts": [
                {
                    "Type": "bind",
                    "Destination": "/run/secrets/postgres-password",
                    "RW": False,
                },
                {"Type": "bind", "Destination": "/var/lib/postgresql", "RW": True},
            ],
        }
    ]


def audit(document: object) -> dict[str, object]:
    return MODULE.audit_document(
        document,
        "xs-nexus-rc-postgres",
        "1panel-network",
        "10m",
        "5",
    )


def assert_failure(document: object, check_id: str) -> None:
    report = audit(document)
    if report["status"] != "FAIL" or check_id not in report["failed_check_ids"]:
        raise RuntimeError(f"expected failed check {check_id}")


def main() -> int:
    passing = audit(fixture())
    if passing["status"] != "PASS" or passing["failed_check_ids"]:
        raise RuntimeError("valid production PostgreSQL policy did not pass")

    missing_rotation = fixture()
    missing_rotation[0]["HostConfig"]["LogConfig"]["Config"] = {}
    assert_failure(missing_rotation, "bounded_log_size")
    assert_failure(missing_rotation, "bounded_log_files")

    public_database = fixture()
    public_database[0]["HostConfig"]["PortBindings"] = {"5432/tcp": [{"HostPort": "5432"}]}
    assert_failure(public_database, "no_host_ports")

    wrong_network = fixture()
    wrong_network[0]["NetworkSettings"]["Networks"]["bridge"] = {}
    assert_failure(wrong_network, "external_network_only")

    exposed_value = "not-for-reporting"
    password_environment = fixture()
    password_environment[0]["Config"]["Env"].append(f"POSTGRES_PASSWORD={exposed_value}")
    report = audit(password_environment)
    assert_failure(password_environment, "no_password_environment")
    if exposed_value in json.dumps(report):
        raise RuntimeError("audit report leaked an environment value")

    unpinned = fixture()
    unpinned[0]["Config"]["Image"] = "postgres:latest"
    assert_failure(unpinned, "digest_pinned_image")

    writable_secret = copy.deepcopy(fixture())
    writable_secret[0]["Mounts"][0]["RW"] = True
    assert_failure(writable_secret, "read_only_password_mount")

    print("production PostgreSQL policy audit regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
