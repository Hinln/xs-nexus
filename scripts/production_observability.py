#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import ipaddress
import json
import os
import re
import shlex
import socket
import ssl
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Iterable


SCHEMA_VERSION = 1
MAX_RESPONSE_BYTES = 1024 * 1024
MAX_PENDING_EVENTS = 256
FUTURE_SKEW_SECONDS = 300
SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
METRIC_NAME = re.compile(r"^[a-zA-Z_:][a-zA-Z0-9_:]*$")
SEVERITY_RANK = {"info": 0, "warning": 1, "critical": 2}


class ConfigurationError(Exception):
    pass


@dataclass(frozen=True)
class Thresholds:
    warning: int
    critical: int


@dataclass(frozen=True)
class Configuration:
    output_directory: Path
    state_file: Path
    proc_root: Path
    test_mode: bool
    test_disk_usage_percent: int | None
    disk_paths: tuple[Path, ...]
    disk: Thresholds
    inode: Thresholds
    cpu: Thresholds
    memory: Thresholds
    file_descriptors: Thresholds
    tasks: Thresholds
    logs: Thresholds
    cpu_sample_seconds: float
    containers: tuple[str, ...]
    health_urls: tuple[str, ...]
    log_paths: tuple[Path, ...]
    log_entry_limit: int
    backup_directory: Path | None
    backup_max_age_seconds: int
    backup_copy_receipt: Path | None
    backup_deep_receipt: Path | None
    backup_copy_max_age_seconds: int
    backup_deep_max_age_seconds: int
    tls_targets: tuple[str, ...]
    tls: Thresholds
    controller_url: str | None
    controller_token_file: Path | None
    minimum_online_nodes: int
    handshake_failures: Thresholds
    auth_failures: Thresholds
    replay_drops: Thresholds
    acl_drops: Thresholds
    relay_abuse: Thresholds
    webhook_url: str | None
    webhook_token_file: Path | None
    source_id: str


@dataclass(frozen=True)
class Observation:
    key: str
    severity: str
    check: str
    message: str


class MetricSink:
    def __init__(self) -> None:
        self.samples: list[tuple[str, float, dict[str, str]]] = []

    def add(self, name: str, value: float | int | bool, **labels: str) -> None:
        if not METRIC_NAME.fullmatch(name):
            raise ValueError("invalid metric name")
        numeric = int(value) if isinstance(value, bool) else value
        self.samples.append((name, float(numeric), dict(sorted(labels.items()))))

    def render(self) -> str:
        lines = []
        for name, value, labels in sorted(
            self.samples,
            key=lambda sample: (sample[0], tuple(sample[2].items())),
        ):
            encoded_labels = ""
            if labels:
                encoded = ",".join(
                    f'{key}="{prometheus_escape(label)}"'
                    for key, label in labels.items()
                )
                encoded_labels = "{" + encoded + "}"
            rendered = str(int(value)) if value.is_integer() else format(value, ".6f")
            lines.append(f"{name}{encoded_labels} {rendered}")
        return "\n".join(lines) + "\n"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


class Collector:
    def __init__(self, configuration: Configuration) -> None:
        self.configuration = configuration
        self.metrics = MetricSink()
        self.observations: list[Observation] = []
        self.controller_snapshot: dict[str, Any] | None = None

    def observe(self, key: str, severity: str, check: str, message: str) -> None:
        if severity not in SEVERITY_RANK:
            raise ValueError("invalid severity")
        observation = Observation(key=key, severity=severity, check=check, message=message)
        self.observations.append(observation)
        log(severity, check, message)

    def threshold(
        self,
        key: str,
        check: str,
        value: float,
        thresholds: Thresholds,
        unit: str,
    ) -> None:
        if value >= thresholds.critical:
            severity = "critical"
        elif value >= thresholds.warning:
            severity = "warning"
        else:
            severity = "info"
        rendered = format(value, ".2f") if not float(value).is_integer() else str(int(value))
        self.observe(key, severity, check, f"value={rendered}{unit}")

    def collect(self) -> None:
        self.collect_cpu()
        self.collect_memory()
        self.collect_network()
        self.collect_file_descriptors()
        self.collect_tasks()
        self.collect_disks()
        self.collect_logs()
        self.collect_containers()
        self.collect_http()
        self.collect_backup()
        self.collect_tls()
        self.collect_controller()
        self.collect_notification_configuration()

    def collect_cpu(self) -> None:
        try:
            first = read_cpu_times(self.configuration.proc_root)
            time.sleep(self.configuration.cpu_sample_seconds)
            second = read_cpu_times(self.configuration.proc_root)
            total_delta = second[0] - first[0]
            idle_delta = second[1] - first[1]
            usage = 0.0 if total_delta <= 0 else (total_delta - idle_delta) * 100.0 / total_delta
            usage = min(max(usage, 0.0), 100.0)
            self.metrics.add("xs_nexus_host_cpu_usage_percent", usage)
            self.threshold("host.cpu", "cpu", usage, self.configuration.cpu, "%")
        except (OSError, ValueError) as error:
            self.observe("host.cpu", "critical", "cpu", f"collection_failed={type(error).__name__}")

    def collect_memory(self) -> None:
        try:
            memory = read_memory(self.configuration.proc_root)
            usage = (memory["MemTotal"] - memory["MemAvailable"]) * 100.0 / memory["MemTotal"]
            self.metrics.add("xs_nexus_host_memory_total_bytes", memory["MemTotal"] * 1024)
            self.metrics.add(
                "xs_nexus_host_memory_available_bytes", memory["MemAvailable"] * 1024
            )
            self.metrics.add("xs_nexus_host_memory_usage_percent", usage)
            self.threshold("host.memory", "memory", usage, self.configuration.memory, "%")
        except (OSError, KeyError, ValueError, ZeroDivisionError) as error:
            self.observe(
                "host.memory", "critical", "memory", f"collection_failed={type(error).__name__}"
            )

    def collect_network(self) -> None:
        try:
            received, transmitted = read_network(self.configuration.proc_root)
            self.metrics.add("xs_nexus_host_network_receive_bytes_total", received)
            self.metrics.add("xs_nexus_host_network_transmit_bytes_total", transmitted)
            self.observe("host.network", "info", "network", "counters_available")
        except (OSError, ValueError) as error:
            self.observe(
                "host.network", "critical", "network", f"collection_failed={type(error).__name__}"
            )

    def collect_file_descriptors(self) -> None:
        try:
            allocated, maximum = read_file_descriptors(self.configuration.proc_root)
            usage = allocated * 100.0 / maximum
            self.metrics.add("xs_nexus_host_file_descriptors_allocated", allocated)
            self.metrics.add("xs_nexus_host_file_descriptors_limit", maximum)
            self.metrics.add("xs_nexus_host_file_descriptors_usage_percent", usage)
            self.threshold(
                "host.file_descriptors",
                "file_descriptors",
                usage,
                self.configuration.file_descriptors,
                "%",
            )
        except (OSError, ValueError, ZeroDivisionError) as error:
            self.observe(
                "host.file_descriptors",
                "critical",
                "file_descriptors",
                f"collection_failed={type(error).__name__}",
            )

    def collect_tasks(self) -> None:
        try:
            tasks = sum(
                1
                for entry in self.configuration.proc_root.iterdir()
                if entry.name.isdigit() and entry.is_dir()
            )
            self.metrics.add("xs_nexus_host_tasks", tasks)
            self.threshold("host.tasks", "tasks", tasks, self.configuration.tasks, "")
        except OSError as error:
            self.observe(
                "host.tasks", "critical", "tasks", f"collection_failed={type(error).__name__}"
            )

    def collect_disks(self) -> None:
        for path in self.configuration.disk_paths:
            key = stable_key("disk", str(path))
            try:
                usage, inode_usage, total, available = disk_usage(
                    path, self.configuration.test_disk_usage_percent
                )
                self.metrics.add("xs_nexus_host_disk_total_bytes", total, path=str(path))
                self.metrics.add("xs_nexus_host_disk_available_bytes", available, path=str(path))
                self.metrics.add("xs_nexus_host_disk_usage_percent", usage, path=str(path))
                self.metrics.add("xs_nexus_host_inode_usage_percent", inode_usage, path=str(path))
                self.threshold(key, "disk", usage, self.configuration.disk, "%")
                self.threshold(
                    stable_key("inode", str(path)),
                    "inode",
                    inode_usage,
                    self.configuration.inode,
                    "%",
                )
            except (OSError, ValueError, ZeroDivisionError) as error:
                self.observe(key, "critical", "disk", f"collection_failed={type(error).__name__}")

    def collect_logs(self) -> None:
        for path in self.configuration.log_paths:
            key = stable_key("log", str(path))
            try:
                size, entries = bounded_path_size(path, self.configuration.log_entry_limit)
                self.metrics.add("xs_nexus_host_log_bytes", size, path=str(path))
                self.metrics.add("xs_nexus_host_log_entries", entries, path=str(path))
                self.threshold(key, "log", size, self.configuration.logs, "B")
            except (OSError, ValueError) as error:
                self.observe(key, "critical", "log", f"collection_failed={type(error).__name__}")

    def collect_containers(self) -> None:
        for container in self.configuration.containers:
            key = stable_key("container", container)
            inspected = run_command(
                [
                    "docker",
                    "inspect",
                    "--format",
                    "{{.State.Status}}|{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}|{{.LogPath}}",
                    container,
                ],
                15,
            )
            if inspected.returncode != 0:
                self.observe(key, "critical", "container", f"name={container} state=missing")
                continue
            fields = inspected.stdout.strip().split("|", 2)
            if len(fields) != 3:
                self.observe(key, "critical", "container", f"name={container} state=invalid")
                continue
            status, health, log_path = fields
            healthy = status == "running" and health in {"healthy", "none"}
            self.metrics.add(
                "xs_nexus_container_healthy",
                healthy,
                container=container,
            )
            self.observe(
                key,
                "info" if healthy else "critical",
                "container",
                f"name={container} state={status}/{health}",
            )
            if log_path:
                try:
                    log_size = Path(log_path).stat(follow_symlinks=False).st_size
                    self.metrics.add(
                        "xs_nexus_container_log_bytes", log_size, container=container
                    )
                    self.threshold(
                        stable_key("container_log", container),
                        "container_log",
                        log_size,
                        self.configuration.logs,
                        "B",
                    )
                except OSError:
                    self.observe(
                        stable_key("container_log", container),
                        "warning",
                        "container_log",
                        f"name={container} unavailable",
                    )
            stats = run_command(
                ["docker", "stats", "--no-stream", "--format", "{{json .}}", container],
                20,
            )
            if stats.returncode != 0:
                self.observe(
                    stable_key("container_metrics", container),
                    "critical",
                    "container_metrics",
                    f"name={container} unavailable",
                )
                continue
            try:
                values = json.loads(stats.stdout)
                cpu = parse_percent(values["CPUPerc"])
                memory = parse_percent(values["MemPerc"])
                received, transmitted = parse_io_pair(values["NetIO"])
                pids = int(values["PIDs"])
                self.metrics.add("xs_nexus_container_cpu_usage_percent", cpu, container=container)
                self.metrics.add(
                    "xs_nexus_container_memory_usage_percent", memory, container=container
                )
                self.metrics.add(
                    "xs_nexus_container_network_receive_bytes_total",
                    received,
                    container=container,
                )
                self.metrics.add(
                    "xs_nexus_container_network_transmit_bytes_total",
                    transmitted,
                    container=container,
                )
                self.metrics.add("xs_nexus_container_pids", pids, container=container)
                self.observe(
                    stable_key("container_metrics", container),
                    "info",
                    "container_metrics",
                    f"name={container} available",
                )
            except (KeyError, TypeError, ValueError, json.JSONDecodeError):
                self.observe(
                    stable_key("container_metrics", container),
                    "critical",
                    "container_metrics",
                    f"name={container} invalid",
                )

    def collect_http(self) -> None:
        for url in self.configuration.health_urls:
            key = stable_key("http", url)
            result = run_command(
                [
                    "curl",
                    "--disable",
                    "--proto",
                    "=http,https",
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--max-time",
                    "10",
                    "--output",
                    os.devnull,
                    "--write-out",
                    "%{http_code}",
                    url,
                ],
                15,
            )
            healthy = result.returncode == 0 and result.stdout.strip().isdigit()
            self.metrics.add("xs_nexus_http_probe_success", healthy, target=url)
            self.observe(
                key,
                "info" if healthy else "critical",
                "http",
                f"target={url} status={'healthy' if healthy else 'failed'}",
            )

    def collect_backup(self) -> None:
        directory = self.configuration.backup_directory
        if directory is None:
            self.observe("backup.configuration", "critical", "backup", "not_configured")
            return
        try:
            newest = newest_regular_file(directory, ".age")
            current_time = time.time()
            modified_at = newest.stat(follow_symlinks=False).st_mtime
            if modified_at > current_time + FUTURE_SKEW_SECONDS:
                raise ValueError("backup timestamp is in the future")
            age = max(0, int(current_time - modified_at))
            self.metrics.add("xs_nexus_backup_age_seconds", age)
            severity = "critical" if age > self.configuration.backup_max_age_seconds else "info"
            self.observe("backup.age", severity, "backup", f"age_seconds={age}")
        except (OSError, ValueError) as error:
            self.observe("backup.age", "critical", "backup", f"failed={type(error).__name__}")
        self.collect_receipt(
            "backup.copy_receipt",
            self.configuration.backup_copy_receipt,
            self.configuration.backup_copy_max_age_seconds,
        )
        self.collect_receipt(
            "backup.deep_receipt",
            self.configuration.backup_deep_receipt,
            self.configuration.backup_deep_max_age_seconds,
        )

    def collect_receipt(self, key: str, path: Path | None, maximum_age: int) -> None:
        check = key.split(".")[-1]
        if path is None:
            self.observe(key, "critical", check, "not_configured")
            return
        try:
            receipt = load_private_json(path, 64 * 1024)
            expected_operation = {
                "copy_receipt": "replica_copy",
                "deep_receipt": "deep_verification",
            }[check]
            if (
                receipt.get("schema_version") != 1
                or receipt.get("operation") != expected_operation
                or receipt.get("status") != "PASS"
            ):
                raise ValueError("receipt status")
            completed = parse_timestamp(receipt.get("completed_at"))
            age = max(0, int((datetime.now(timezone.utc) - completed).total_seconds()))
            self.metrics.add(f"xs_nexus_backup_{check}_age_seconds", age)
            severity = "critical" if age > maximum_age else "info"
            self.observe(key, severity, check, f"age_seconds={age}")
        except (OSError, TypeError, ValueError, json.JSONDecodeError) as error:
            self.observe(key, "critical", check, f"failed={type(error).__name__}")

    def collect_tls(self) -> None:
        if not self.configuration.tls_targets:
            self.observe("tls.configuration", "critical", "tls", "not_configured")
            return
        for target in self.configuration.tls_targets:
            key = stable_key("tls", target)
            try:
                host, port = parse_tls_target(target)
                context = ssl.create_default_context()
                with socket.create_connection((host, port), timeout=10) as raw_socket:
                    with context.wrap_socket(raw_socket, server_hostname=host) as tls_socket:
                        certificate = tls_socket.getpeercert()
                expiry_text = certificate.get("notAfter")
                if not isinstance(expiry_text, str):
                    raise ValueError("missing expiry")
                expiry = datetime.fromtimestamp(
                    ssl.cert_time_to_seconds(expiry_text), timezone.utc
                )
                remaining = int((expiry - datetime.now(timezone.utc)).total_seconds())
                self.metrics.add(
                    "xs_nexus_tls_certificate_expiry_seconds", remaining, target=target
                )
                if remaining <= self.configuration.tls.critical:
                    severity = "critical"
                elif remaining <= self.configuration.tls.warning:
                    severity = "warning"
                else:
                    severity = "info"
                self.observe(key, severity, "tls", f"target={target} remaining_seconds={remaining}")
            except (OSError, ValueError, ssl.SSLError) as error:
                self.metrics.add("xs_nexus_tls_probe_success", 0, target=target)
                self.observe(key, "critical", "tls", f"target={target} failed={type(error).__name__}")
            else:
                self.metrics.add("xs_nexus_tls_probe_success", 1, target=target)

    def collect_controller(self) -> None:
        if self.configuration.controller_url is None:
            self.observe(
                "controller.observability",
                "critical",
                "controller",
                "observability_endpoint_not_configured",
            )
            return
        try:
            token = read_secret_file(self.configuration.controller_token_file)
            request = urllib.request.Request(
                self.configuration.controller_url,
                headers={
                    "Accept": "application/json",
                    "Authorization": f"Bearer {token}",
                    "User-Agent": "xs-nexus-production-monitor/1",
                },
            )
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
            with opener.open(request, timeout=10) as response:
                if response.status != 200:
                    raise ValueError("unexpected status")
                payload = response.read(MAX_RESPONSE_BYTES + 1)
            if len(payload) > MAX_RESPONSE_BYTES:
                raise ValueError("response too large")
            snapshot = json.loads(payload)
            if not isinstance(snapshot, dict) or snapshot.get("schema_version") != 1:
                raise ValueError("invalid schema")
            self.controller_snapshot = snapshot
            self.metrics.add("xs_nexus_controller_observability_success", 1)
            self.observe("controller.observability", "info", "controller", "snapshot_available")
            self.apply_controller_snapshot(snapshot)
        except (
            OSError,
            TypeError,
            ValueError,
            json.JSONDecodeError,
            urllib.error.URLError,
        ) as error:
            self.metrics.add("xs_nexus_controller_observability_success", 0)
            self.observe(
                "controller.observability",
                "critical",
                "controller",
                f"snapshot_failed={type(error).__name__}",
            )

    def apply_controller_snapshot(self, snapshot: dict[str, Any]) -> None:
        mappings = {
            "xs_nexus_nodes_managed": ("nodes", "managed"),
            "xs_nexus_nodes_online": ("nodes", "online"),
            "xs_nexus_nodes_fresh_telemetry": ("nodes", "fresh_telemetry"),
            "xs_nexus_nodes_unknown_path": ("nodes", "unknown_path_nodes"),
            "xs_nexus_nodes_disconnected": ("nodes", "disconnected_nodes"),
            "xs_nexus_path_observations": ("nodes", "path_observations"),
            "xs_nexus_direct_path_observations": ("nodes", "direct_path_observations"),
            "xs_nexus_relay_path_observations": ("nodes", "relay_path_observations"),
            "xs_nexus_telemetry_nodes_24h": ("nodes", "telemetry_nodes_24h"),
            "xs_nexus_traffic_bytes_24h": ("nodes", "traffic_bytes_24h"),
            "xs_nexus_handshake_attempts_24h": ("nodes", "handshake_attempts_24h"),
            "xs_nexus_handshake_failures_24h": ("nodes", "handshake_failures_24h"),
            "xs_nexus_acl_drops_24h": ("security", "acl_drops_24h"),
            "xs_nexus_replay_drops_24h": ("security", "replay_drops_24h"),
            "xs_nexus_management_auth_failures_24h": (
                "security",
                "management_auth_failures_24h",
            ),
            "xs_nexus_control_auth_failures_since_start": (
                "security",
                "control_auth_failures_since_start",
            ),
            "xs_nexus_control_sessions_active": ("controller", "active_control_sessions"),
            "xs_nexus_control_sessions_maximum": ("controller", "maximum_control_sessions"),
            "xs_nexus_control_sessions_rejected_since_start": (
                "controller",
                "rejected_control_sessions_since_start",
            ),
            "xs_nexus_configuration_sends_active": (
                "controller",
                "active_configuration_sends",
            ),
            "xs_nexus_configuration_sends_maximum": (
                "controller",
                "maximum_configuration_sends",
            ),
            "xs_nexus_network_nodes_maximum": ("controller", "maximum_nodes_per_network"),
            "xs_nexus_largest_network_nodes": ("nodes", "largest_network_managed"),
            "xs_nexus_relays_configured": ("relays", "configured"),
            "xs_nexus_relays_unexpired_configured": ("relays", "unexpired_configured"),
            "xs_nexus_relays_fresh": ("relays", "fresh"),
            "xs_nexus_relay_telemetry_relays_24h": ("relays", "telemetry_relays_24h"),
            "xs_nexus_relay_packets_received_24h": ("relays", "packets_received_24h"),
            "xs_nexus_relay_packets_forwarded_24h": ("relays", "packets_forwarded_24h"),
            "xs_nexus_relay_registration_retries_24h": (
                "relays",
                "registration_retries_24h",
            ),
            "xs_nexus_relay_registrations_rejected_24h": (
                "relays",
                "registrations_rejected_24h",
            ),
            "xs_nexus_relay_invalid_drops_24h": ("relays", "invalid_drops_24h"),
            "xs_nexus_relay_authentication_drops_24h": (
                "relays",
                "authentication_drops_24h",
            ),
            "xs_nexus_relay_replay_drops_24h": ("relays", "replay_drops_24h"),
            "xs_nexus_relay_rate_limit_drops_24h": (
                "relays",
                "rate_limit_drops_24h",
            ),
            "xs_nexus_relay_queue_drops_24h": ("relays", "queue_drops_24h"),
            "xs_nexus_relay_destination_drops_24h": (
                "relays",
                "destination_drops_24h",
            ),
            "xs_nexus_relay_send_drops_24h": ("relays", "send_drops_24h"),
            "xs_nexus_relay_packets_dropped_24h": ("relays", "packets_dropped_24h"),
            "xs_nexus_relay_io_errors_24h": ("relays", "io_errors_24h"),
            "xs_nexus_routes_enabled": ("routing", "enabled_routes"),
            "xs_nexus_route_changes_24h": ("routing", "successful_changes_24h"),
            "xs_nexus_update_failed_nodes": ("updates", "failed_nodes"),
            "xs_nexus_audit_events_24h": ("audit", "events_24h"),
            "xs_nexus_audit_events_total": ("audit", "total_events"),
        }
        values: dict[str, int] = {}
        for metric, path in mappings.items():
            value = nested_integer(snapshot, path)
            values[metric] = value
            self.metrics.add(metric, value)
        nodes = nested_mapping(snapshot, ("nodes",))
        relays = nested_mapping(snapshot, ("relays",))
        controller = nested_mapping(snapshot, ("controller",))
        database = nested_mapping(snapshot, ("database",))
        redis = nested_mapping(snapshot, ("redis",))
        if controller.get("status") != "ok" or database.get("status") != "ok":
            self.observe("controller.health", "critical", "controller", "controller_or_database_unhealthy")
        else:
            self.observe("controller.health", "info", "controller", "controller_and_database_healthy")
        redis_status = redis.get("status")
        if redis_status not in {"ok", "not_applicable"}:
            self.observe("redis.health", "critical", "redis", "redis_status_invalid")
        elif redis_status == "not_applicable" and redis.get("reason") != "controller_has_no_redis_dependency":
            self.observe("redis.health", "critical", "redis", "redis_reason_invalid")
        else:
            self.observe("redis.health", "info", "redis", f"status={redis_status}")
        managed = values["xs_nexus_nodes_managed"]
        fresh = values["xs_nexus_nodes_fresh_telemetry"]
        online = values["xs_nexus_nodes_online"]
        path_observations = values["xs_nexus_path_observations"]
        direct_paths = values["xs_nexus_direct_path_observations"]
        relay_paths = values["xs_nexus_relay_path_observations"]
        self.capacity_observation(
            "capacity.control_sessions",
            values["xs_nexus_control_sessions_active"],
            values["xs_nexus_control_sessions_maximum"],
        )
        self.capacity_observation(
            "capacity.configuration_sends",
            values["xs_nexus_configuration_sends_active"],
            values["xs_nexus_configuration_sends_maximum"],
        )
        self.capacity_observation(
            "capacity.network_nodes",
            values["xs_nexus_largest_network_nodes"],
            values["xs_nexus_network_nodes_maximum"],
        )
        rejected_sessions = values["xs_nexus_control_sessions_rejected_since_start"]
        self.observe(
            "capacity.control_sessions_rejected",
            "critical" if rejected_sessions > 0 else "info",
            "capacity",
            f"rejected_since_start={rejected_sessions}",
        )
        if direct_paths + relay_paths != path_observations:
            raise ValueError("inconsistent path observations")
        if path_observations > 0:
            self.metrics.add("xs_nexus_direct_path_ratio", direct_paths / path_observations)
            self.metrics.add("xs_nexus_relay_path_ratio", relay_paths / path_observations)
        if online < self.configuration.minimum_online_nodes:
            self.observe(
                "nodes.minimum_online",
                "critical",
                "nodes",
                f"online={online} minimum={self.configuration.minimum_online_nodes}",
            )
        else:
            self.observe("nodes.minimum_online", "info", "nodes", f"online={online}")
        if managed > 0 and fresh == 0:
            self.observe("nodes.telemetry", "critical", "nodes", "all_path_telemetry_missing")
        elif not required_boolean(nodes, "path_telemetry_complete"):
            self.observe("nodes.telemetry", "warning", "nodes", "path_telemetry_incomplete")
        else:
            self.observe("nodes.telemetry", "info", "nodes", "path_telemetry_complete")
        unexpired_relays = values["xs_nexus_relays_unexpired_configured"]
        fresh_relays = values["xs_nexus_relays_fresh"]
        if unexpired_relays > 0 and fresh_relays == 0:
            self.observe("relays.telemetry", "critical", "relay", "all_relay_telemetry_missing")
        elif not required_boolean(relays, "telemetry_complete"):
            self.observe("relays.telemetry", "warning", "relay", "relay_telemetry_incomplete")
        else:
            self.observe("relays.telemetry", "info", "relay", "relay_telemetry_complete")
        self.threshold(
            "security.handshake_failures",
            "handshake_failures",
            values["xs_nexus_handshake_failures_24h"],
            self.configuration.handshake_failures,
            "",
        )

        management_auth = values["xs_nexus_management_auth_failures_24h"]
        self.threshold(
            "security.management_auth_failures",
            "auth_failures",
            management_auth,
            self.configuration.auth_failures,
            "",
        )
        self.threshold(
            "security.control_auth_failures",
            "auth_failures",
            values["xs_nexus_control_auth_failures_since_start"],
            self.configuration.auth_failures,
            "",
        )
        self.threshold(
            "security.replay_drops",
            "replay_drops",
            values["xs_nexus_replay_drops_24h"],
            self.configuration.replay_drops,
            "",
        )
        self.threshold(
            "security.acl_drops",
            "acl_drops",
            values["xs_nexus_acl_drops_24h"],
            self.configuration.acl_drops,
            "",
        )
        abuse_fields = (
            "invalid_drops_24h",
            "authentication_drops_24h",
            "replay_drops_24h",
            "rate_limit_drops_24h",
            "queue_drops_24h",
            "destination_drops_24h",
            "send_drops_24h",
            "registrations_rejected_24h",
        )
        abuse = sum(required_integer(relays, field) for field in abuse_fields)
        self.metrics.add("xs_nexus_relay_abuse_drops_24h", abuse)
        self.threshold(
            "relays.abuse",
            "relay_abuse",
            abuse,
            self.configuration.relay_abuse,
            "",
        )
        relay_io_errors = values["xs_nexus_relay_io_errors_24h"]
        self.observe(
            "relays.io_errors",
            "critical" if relay_io_errors > 0 else "info",
            "relay_io_errors",
            f"count={relay_io_errors}",
        )
        failed_updates = values["xs_nexus_update_failed_nodes"]
        self.observe(
            "updates.failed",
            "critical" if failed_updates > 0 else "info",
            "updates",
            f"failed_nodes={failed_updates}",
        )

    def capacity_observation(self, key: str, used: int, maximum: int) -> None:
        if maximum <= 0 or used < 0 or used > maximum:
            raise ValueError("invalid capacity snapshot")
        utilization_basis_points = used * 10_000 // maximum
        if utilization_basis_points >= 9_500:
            severity = "critical"
        elif utilization_basis_points >= 8_000:
            severity = "warning"
        else:
            severity = "info"
        self.observe(
            key,
            severity,
            "capacity",
            f"used={used} maximum={maximum} utilization_basis_points={utilization_basis_points}",
        )

    def collect_notification_configuration(self) -> None:
        if self.configuration.webhook_url is None:
            self.observe(
                "notification.configuration",
                "warning",
                "notification",
                "external_destination_not_configured",
            )


def parse_configuration(environment: dict[str, str]) -> Configuration:
    test_mode = environment.get("XS_MONITOR_TEST_MODE") == "1"
    output_directory = absolute_path(environment.get("XS_MONITOR_OUTPUT_DIR", "/var/lib/xs-nexus-monitor"))
    state_file = absolute_path(
        environment.get("XS_MONITOR_STATE_FILE", str(output_directory / "state.json"))
    )
    proc_root_value = environment.get("XS_MONITOR_PROC_ROOT", "/proc")
    if proc_root_value != "/proc" and not test_mode:
        raise ConfigurationError("XS_MONITOR_PROC_ROOT is test-only")
    test_disk_usage = environment.get("XS_MONITOR_TEST_DISK_USAGE_PERCENT")
    if test_disk_usage is not None and not test_mode:
        raise ConfigurationError("XS_MONITOR_TEST_DISK_USAGE_PERCENT is test-only")
    disk_override = (
        bounded_int(test_disk_usage, 0, 100, "XS_MONITOR_TEST_DISK_USAGE_PERCENT")
        if test_disk_usage is not None
        else None
    )
    containers = tuple(split_words(environment.get("XS_MONITOR_CONTAINERS", "")))
    for container in containers:
        if not SAFE_NAME.fullmatch(container):
            raise ConfigurationError("invalid container name")
    health_urls = tuple(split_words(environment.get("XS_MONITOR_HEALTH_URLS", "")))
    for url in health_urls:
        validate_probe_url(url, allow_loopback_http=True, require_loopback_http=True)
    controller_url = optional_text(environment.get("XS_MONITOR_CONTROLLER_OBSERVABILITY_URL"))
    if controller_url is not None:
        validate_probe_url(controller_url, allow_loopback_http=True, require_loopback_http=True)
        controller_host = urllib.parse.urlsplit(controller_url).hostname
        if controller_host is None or not is_loopback(controller_host):
            raise ConfigurationError("Controller observability URL must use loopback")
    controller_token_file = optional_path(environment.get("XS_MONITOR_CONTROLLER_TOKEN_FILE"))
    if controller_url is not None and controller_token_file is None:
        raise ConfigurationError("Controller token file is required")
    webhook_url = optional_text(environment.get("XS_MONITOR_WEBHOOK_URL"))
    webhook_token_file = optional_path(environment.get("XS_MONITOR_WEBHOOK_TOKEN_FILE"))
    if webhook_url is not None:
        allow_http = test_mode and environment.get("XS_MONITOR_ALLOW_HTTP_WEBHOOK_FOR_TESTS") == "1"
        validate_webhook_url(webhook_url, allow_http)
        if webhook_token_file is None:
            raise ConfigurationError("Webhook token file is required")
    source_id = environment.get("XS_MONITOR_SOURCE_ID", "xs-nexus")
    if not SAFE_NAME.fullmatch(source_id):
        raise ConfigurationError("invalid source ID")
    configuration = Configuration(
        output_directory=output_directory,
        state_file=state_file,
        proc_root=absolute_path(proc_root_value),
        test_mode=test_mode,
        test_disk_usage_percent=disk_override,
        disk=thresholds(environment, "XS_MONITOR_DISK", 80, 90, 100),
        inode=thresholds(environment, "XS_MONITOR_INODE", 80, 90, 100),
        cpu=thresholds(environment, "XS_MONITOR_CPU", 90, 98, 100),
        memory=thresholds(environment, "XS_MONITOR_MEMORY", 85, 95, 100),
        file_descriptors=thresholds(environment, "XS_MONITOR_FD", 80, 90, 100),
        tasks=thresholds(environment, "XS_MONITOR_TASK", 1000, 2000, 10_000_000),
        logs=thresholds(
            environment,
            "XS_MONITOR_LOG_BYTES",
            100 * 1024 * 1024,
            500 * 1024 * 1024,
            1 << 60,
        ),
        cpu_sample_seconds=bounded_float(
            environment.get("XS_MONITOR_CPU_SAMPLE_SECONDS", "0.2"),
            0.0 if test_mode else 0.05,
            5.0,
            "XS_MONITOR_CPU_SAMPLE_SECONDS",
        ),
        disk_paths=tuple(
            absolute_path(value) for value in split_words(environment.get("XS_MONITOR_DISK_PATHS", "/"))
        ),
        containers=containers,
        health_urls=health_urls,
        log_paths=tuple(
            absolute_path(value) for value in split_words(environment.get("XS_MONITOR_LOG_PATHS", ""))
        ),
        log_entry_limit=environment_int(environment, "XS_MONITOR_LOG_ENTRY_LIMIT", 10_000, 1, 1_000_000),
        backup_directory=optional_path(environment.get("XS_MONITOR_BACKUP_DIR")),
        backup_max_age_seconds=environment_int(
            environment, "XS_MONITOR_BACKUP_MAX_AGE_SECONDS", 93_600, 1, 31_536_000
        ),
        backup_copy_receipt=optional_path(environment.get("XS_MONITOR_BACKUP_COPY_RECEIPT")),
        backup_deep_receipt=optional_path(environment.get("XS_MONITOR_BACKUP_DEEP_VERIFY_RECEIPT")),
        backup_copy_max_age_seconds=environment_int(
            environment, "XS_MONITOR_BACKUP_COPY_MAX_AGE_SECONDS", 93_600, 1, 31_536_000
        ),
        backup_deep_max_age_seconds=environment_int(
            environment, "XS_MONITOR_BACKUP_DEEP_MAX_AGE_SECONDS", 604_800, 1, 31_536_000
        ),
        tls_targets=tuple(split_words(environment.get("XS_MONITOR_TLS_TARGETS", ""))),
        tls=tls_expiry_thresholds(environment),
        controller_url=controller_url,
        controller_token_file=controller_token_file,
        minimum_online_nodes=environment_int(
            environment, "XS_MONITOR_MINIMUM_ONLINE_NODES", 0, 0, 1_000_000
        ),
        handshake_failures=thresholds(
            environment, "XS_MONITOR_HANDSHAKE_FAILURES", 10, 100, 1 << 62
        ),
        auth_failures=thresholds(environment, "XS_MONITOR_AUTH_FAILURES", 5, 20, 1 << 62),
        replay_drops=thresholds(environment, "XS_MONITOR_REPLAY_DROPS", 1, 10, 1 << 62),
        acl_drops=thresholds(environment, "XS_MONITOR_ACL_DROPS", 100, 1000, 1 << 62),
        relay_abuse=thresholds(environment, "XS_MONITOR_RELAY_ABUSE", 100, 1000, 1 << 62),
        webhook_url=webhook_url,
        webhook_token_file=webhook_token_file,
        source_id=source_id,
    )
    if not configuration.disk_paths:
        raise ConfigurationError("at least one disk path is required")
    for target in configuration.tls_targets:
        parse_tls_target(target)
    return configuration


def thresholds(
    environment: dict[str, str], prefix: str, warning: int, critical: int, maximum: int
) -> Thresholds:
    warning_name = (
        f"{prefix}_WARNING_PERCENT"
        if f"{prefix}_WARNING_PERCENT" in environment
        else f"{prefix}_WARNING"
    )
    critical_name = (
        f"{prefix}_CRITICAL_PERCENT"
        if f"{prefix}_CRITICAL_PERCENT" in environment
        else f"{prefix}_CRITICAL"
    )
    configured = Thresholds(
        warning=environment_int(environment, warning_name, warning, 0, maximum),
        critical=environment_int(environment, critical_name, critical, 1, maximum),
    )
    if configured.warning >= configured.critical:
        raise ConfigurationError(f"{prefix} warning must be lower than critical")
    return configured


def tls_expiry_thresholds(environment: dict[str, str]) -> Thresholds:
    warning = environment_int(
        environment, "XS_MONITOR_TLS_WARNING_SECONDS", 2_592_000, 1, 315_360_000
    )
    critical = environment_int(
        environment,
        "XS_MONITOR_TLS_CRITICAL_SECONDS",
        int(environment.get("XS_MONITOR_TLS_MIN_VALIDITY_SECONDS", "1209600")),
        1,
        315_360_000,
    )
    if critical >= warning:
        raise ConfigurationError("TLS critical expiry must be lower than warning expiry")
    return Thresholds(warning=warning, critical=critical)


def environment_int(
    environment: dict[str, str], name: str, default: int, minimum: int, maximum: int
) -> int:
    return bounded_int(environment.get(name, str(default)), minimum, maximum, name)


def bounded_int(value: str, minimum: int, maximum: int, name: str) -> int:
    try:
        parsed = int(value)
    except (TypeError, ValueError) as error:
        raise ConfigurationError(f"{name} must be an integer") from error
    if parsed < minimum or parsed > maximum:
        raise ConfigurationError(f"{name} is out of range")
    return parsed


def bounded_float(value: str, minimum: float, maximum: float, name: str) -> float:
    try:
        parsed = float(value)
    except (TypeError, ValueError) as error:
        raise ConfigurationError(f"{name} must be numeric") from error
    if parsed < minimum or parsed > maximum:
        raise ConfigurationError(f"{name} is out of range")
    return parsed


def split_words(value: str) -> list[str]:
    try:
        return shlex.split(value)
    except ValueError as error:
        raise ConfigurationError("invalid quoted list") from error


def optional_text(value: str | None) -> str | None:
    return value if value else None


def absolute_path(value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        raise ConfigurationError("paths must be absolute")
    return path


def optional_path(value: str | None) -> Path | None:
    return absolute_path(value) if value else None


def validate_probe_url(
    value: str,
    allow_loopback_http: bool,
    require_loopback_http: bool = False,
) -> None:
    parsed = urllib.parse.urlsplit(value)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        raise ConfigurationError("invalid probe URL")
    validate_url_port(parsed)
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ConfigurationError("probe URL cannot contain credentials, query, or fragment")
    if parsed.scheme == "http":
        if not allow_loopback_http or (require_loopback_http and not is_loopback(parsed.hostname)):
            raise ConfigurationError("plain HTTP is restricted to loopback")


def validate_webhook_url(value: str, allow_http: bool) -> None:
    parsed = urllib.parse.urlsplit(value)
    schemes = {"https"} | ({"http"} if allow_http else set())
    if parsed.scheme not in schemes or not parsed.hostname:
        raise ConfigurationError("invalid webhook URL")
    validate_url_port(parsed)
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ConfigurationError("webhook URL cannot contain credentials, query, or fragment")


def validate_url_port(parsed: urllib.parse.SplitResult) -> None:
    try:
        port = parsed.port
    except ValueError as error:
        raise ConfigurationError("invalid URL port") from error
    if port is not None and not 1 <= port <= 65535:
        raise ConfigurationError("invalid URL port")


def is_loopback(hostname: str) -> bool:
    if hostname == "localhost":
        return True
    try:
        return ipaddress.ip_address(hostname).is_loopback
    except ValueError:
        return False


def read_cpu_times(proc_root: Path) -> tuple[int, int]:
    fields = (proc_root / "stat").read_text(encoding="ascii").splitlines()[0].split()
    if not fields or fields[0] != "cpu" or len(fields) < 5:
        raise ValueError("invalid cpu stat")
    values = [int(value) for value in fields[1:]]
    return sum(values), values[3] + (values[4] if len(values) > 4 else 0)


def read_memory(proc_root: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    for line in (proc_root / "meminfo").read_text(encoding="ascii").splitlines():
        name, raw = line.split(":", 1)
        if name in {"MemTotal", "MemAvailable"}:
            values[name] = int(raw.strip().split()[0])
    if values.get("MemTotal", 0) <= 0 or values.get("MemAvailable", -1) < 0:
        raise ValueError("invalid memory stat")
    return values


def read_network(proc_root: Path) -> tuple[int, int]:
    received = 0
    transmitted = 0
    for line in (proc_root / "net" / "dev").read_text(encoding="ascii").splitlines()[2:]:
        _, raw = line.split(":", 1)
        fields = raw.split()
        if len(fields) < 16:
            raise ValueError("invalid network stat")
        received += int(fields[0])
        transmitted += int(fields[8])
    return received, transmitted


def read_file_descriptors(proc_root: Path) -> tuple[int, int]:
    fields = (proc_root / "sys" / "fs" / "file-nr").read_text(encoding="ascii").split()
    if len(fields) != 3:
        raise ValueError("invalid file descriptor stat")
    allocated = int(fields[0]) - int(fields[1])
    maximum = int(fields[2])
    if allocated < 0 or maximum <= 0:
        raise ValueError("invalid file descriptor values")
    return allocated, maximum


def disk_usage(path: Path, override_percent: int | None) -> tuple[float, float, int, int]:
    values = os.statvfs(path)
    total = values.f_blocks * values.f_frsize
    available = values.f_bavail * values.f_frsize
    usage = (total - available) * 100.0 / total
    if override_percent is not None:
        usage = float(override_percent)
    inode_usage = 0.0
    if values.f_files > 0:
        inode_usage = (values.f_files - values.f_favail) * 100.0 / values.f_files
    return usage, inode_usage, total, available


def bounded_path_size(path: Path, maximum_entries: int) -> tuple[int, int]:
    metadata = path.stat(follow_symlinks=False)
    if stat.S_ISLNK(metadata.st_mode):
        raise ValueError("symlink")
    if stat.S_ISREG(metadata.st_mode):
        return metadata.st_size, 1
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError("unsupported path")
    total = 0
    entries = 0
    pending = [path]
    while pending:
        current = pending.pop()
        with os.scandir(current) as directory:
            for entry in directory:
                entries += 1
                if entries > maximum_entries:
                    raise ValueError("entry limit")
                if entry.is_symlink():
                    continue
                if entry.is_dir(follow_symlinks=False):
                    pending.append(Path(entry.path))
                elif entry.is_file(follow_symlinks=False):
                    total += entry.stat(follow_symlinks=False).st_size
    return total, entries


def run_command(arguments: list[str], timeout_seconds: int) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            arguments,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
            env=os.environ.copy(),
        )
    except (OSError, subprocess.TimeoutExpired):
        return subprocess.CompletedProcess(arguments, 127, "", "")


def parse_percent(value: str) -> float:
    if not isinstance(value, str) or not value.endswith("%"):
        raise ValueError("invalid percent")
    parsed = float(value[:-1])
    if parsed < 0:
        raise ValueError("invalid percent")
    return parsed


def parse_io_pair(value: str) -> tuple[int, int]:
    if not isinstance(value, str):
        raise ValueError("invalid IO")
    fields = value.split("/")
    if len(fields) != 2:
        raise ValueError("invalid IO")
    return parse_size(fields[0].strip()), parse_size(fields[1].strip())


def parse_size(value: str) -> int:
    match = re.fullmatch(r"([0-9]+(?:\.[0-9]+)?)\s*([A-Za-z]+)", value)
    if match is None:
        raise ValueError("invalid size")
    units = {
        "B": 1,
        "kB": 1000,
        "MB": 1000**2,
        "GB": 1000**3,
        "TB": 1000**4,
        "KiB": 1024,
        "MiB": 1024**2,
        "GiB": 1024**3,
        "TiB": 1024**4,
    }
    multiplier = units.get(match.group(2))
    if multiplier is None:
        raise ValueError("invalid size unit")
    return int(float(match.group(1)) * multiplier)


def newest_regular_file(directory: Path, suffix: str) -> Path:
    metadata = directory.stat(follow_symlinks=False)
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError("not a directory")
    candidates = []
    for entry in directory.iterdir():
        if entry.name.endswith(suffix):
            entry_metadata = entry.stat(follow_symlinks=False)
            if stat.S_ISREG(entry_metadata.st_mode):
                candidates.append((entry_metadata.st_mtime, entry))
    if not candidates:
        raise ValueError("no backup")
    return max(candidates)[1]


def load_json_regular(path: Path, maximum_bytes: int) -> dict[str, Any]:
    value = json.loads(read_regular_bytes(path, maximum_bytes, require_private=False))
    if not isinstance(value, dict):
        raise ValueError("invalid JSON object")
    return value


def load_private_json(path: Path, maximum_bytes: int) -> dict[str, Any]:
    value = json.loads(read_regular_bytes(path, maximum_bytes, require_private=True))
    if not isinstance(value, dict):
        raise ValueError("invalid JSON object")
    return value


def parse_timestamp(value: Any) -> datetime:
    if not isinstance(value, str):
        raise ValueError("invalid timestamp")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("timestamp requires timezone")
    normalized = parsed.astimezone(timezone.utc)
    if normalized > datetime.now(timezone.utc) + timedelta(seconds=FUTURE_SKEW_SECONDS):
        raise ValueError("timestamp is in the future")
    return normalized


def parse_tls_target(target: str) -> tuple[str, int]:
    if target.startswith("["):
        closing = target.find("]")
        if closing < 2 or closing + 2 >= len(target) or target[closing + 1] != ":":
            raise ConfigurationError("invalid TLS target")
        host = target[1:closing]
        port_text = target[closing + 2 :]
    else:
        if target.count(":") != 1:
            raise ConfigurationError("invalid TLS target")
        host, port_text = target.rsplit(":", 1)
    port = bounded_int(port_text, 1, 65535, "TLS port")
    if not host or any(character.isspace() for character in host):
        raise ConfigurationError("invalid TLS host")
    return host, port


def read_secret_file(path: Path | None) -> str:
    if path is None:
        raise ValueError("missing secret file")
    raw = read_regular_bytes(path, 4096, require_private=True)
    if not raw:
        raise ValueError("invalid secret size")
    value = raw.decode("utf-8").strip()
    if not value or any(ord(character) < 0x20 for character in value):
        raise ValueError("invalid secret value")
    return value


def read_regular_bytes(path: Path, maximum_bytes: int, require_private: bool) -> bytes:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError("invalid regular file")
        if require_private and (metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077):
            raise ValueError("insecure private file")
        if metadata.st_size > maximum_bytes:
            raise ValueError("file is too large")
        chunks = []
        remaining = maximum_bytes + 1
        while remaining > 0:
            chunk = os.read(descriptor, min(remaining, 64 * 1024))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
    finally:
        os.close(descriptor)
    value = b"".join(chunks)
    if len(value) > maximum_bytes:
        raise ValueError("file is too large")
    return value


def nested_mapping(value: dict[str, Any], path: tuple[str, ...]) -> dict[str, Any]:
    current: Any = value
    for item in path:
        if not isinstance(current, dict) or item not in current:
            raise ValueError("missing response field")
        current = current[item]
    if not isinstance(current, dict):
        raise ValueError("invalid response field")
    return current


def nested_integer(value: dict[str, Any], path: tuple[str, ...]) -> int:
    current = nested_mapping(value, path[:-1])
    return required_integer(current, path[-1])


def required_integer(value: dict[str, Any], name: str) -> int:
    result = value.get(name)
    if isinstance(result, bool) or not isinstance(result, int) or result < 0:
        raise ValueError("invalid integer field")
    return result


def required_boolean(value: dict[str, Any], name: str) -> bool:
    result = value.get(name)
    if not isinstance(result, bool):
        raise ValueError("invalid boolean field")
    return result


def stable_key(prefix: str, value: str) -> str:
    digest = hashlib.sha256(value.encode("utf-8")).hexdigest()[:16]
    return f"{prefix}.{digest}"


def prometheus_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace('"', '\\"')


def log(level: str, check: str, message: str) -> None:
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    print(
        f"{timestamp} level={level} check={check} message={shlex.quote(message)}",
        flush=True,
    )


def ensure_secure_directory(path: Path) -> None:
    path.mkdir(parents=True, mode=0o700, exist_ok=True)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
        raise OSError("invalid output directory")
    if metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077:
        raise OSError("insecure output directory")


def atomic_write(path: Path, data: bytes, mode: int) -> None:
    ensure_secure_directory(path.parent)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "wb", closefd=True) as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary_name, path)
        os.chmod(path, mode)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    except Exception:
        try:
            os.close(descriptor)
        except OSError:
            pass
        try:
            os.unlink(temporary_name)
        except OSError:
            pass
        raise


def load_state(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"schema_version": SCHEMA_VERSION, "active": {}, "pending_events": []}
    state_value = load_json_regular(path, MAX_RESPONSE_BYTES)
    if state_value.get("schema_version") != SCHEMA_VERSION:
        raise ValueError("invalid state schema")
    active = state_value.get("active")
    pending = state_value.get("pending_events")
    if not isinstance(active, dict) or not isinstance(pending, list):
        raise ValueError("invalid state")
    return state_value


def active_alerts(observations: Iterable[Observation]) -> dict[str, dict[str, str]]:
    active: dict[str, dict[str, str]] = {}
    for observation in observations:
        if SEVERITY_RANK[observation.severity] == 0:
            continue
        existing = active.get(observation.key)
        if existing is None or SEVERITY_RANK[observation.severity] > SEVERITY_RANK[existing["severity"]]:
            active[observation.key] = {
                "severity": observation.severity,
                "check": observation.check,
                "message": observation.message,
            }
    return active


def alert_transitions(
    previous: dict[str, Any], current: dict[str, dict[str, str]], generated_at: str
) -> list[dict[str, str]]:
    events: list[dict[str, str]] = []
    previous_active = {
        key: value
        for key, value in previous.get("active", {}).items()
        if key != "notification.delivery"
    }
    transition_current = {
        key: value for key, value in current.items() if key != "notification.delivery"
    }
    for key, alert in sorted(transition_current.items()):
        old = previous_active.get(key)
        if not isinstance(old, dict) or old.get("severity") != alert["severity"]:
            events.append(alert_event(key, "firing", alert, generated_at))
    for key, old in sorted(previous_active.items()):
        if key not in transition_current and isinstance(old, dict):
            normalized = {
                "severity": str(old.get("severity", "warning")),
                "check": str(old.get("check", "unknown")),
                "message": "condition_recovered",
            }
            events.append(alert_event(key, "resolved", normalized, generated_at))
    return events


def alert_event(
    key: str, status_value: str, alert: dict[str, str], generated_at: str
) -> dict[str, str]:
    identity = "\0".join(
        (key, status_value, alert["severity"], alert["check"], generated_at)
    )
    return {
        "id": hashlib.sha256(identity.encode("utf-8")).hexdigest(),
        "key": key,
        "status": status_value,
        "severity": alert["severity"],
        "check": alert["check"],
        "message": alert["message"],
        "generated_at": generated_at,
    }


def merge_pending(
    previous: list[Any], new: list[dict[str, str]]
) -> tuple[list[dict[str, str]], int]:
    merged: list[dict[str, str]] = []
    seen: set[str] = set()
    for event in [*previous, *new]:
        if not isinstance(event, dict) or not isinstance(event.get("id"), str):
            continue
        if event["id"] in seen:
            continue
        seen.add(event["id"])
        merged.append({str(key): str(value) for key, value in event.items()})
    dropped = max(0, len(merged) - MAX_PENDING_EVENTS)
    return merged[-MAX_PENDING_EVENTS:], dropped


def deliver_webhook(
    configuration: Configuration, events: list[dict[str, str]], generated_at: str
) -> None:
    if configuration.webhook_url is None or not events:
        return
    headers = {
        "Content-Type": "application/json",
        "User-Agent": "xs-nexus-production-monitor/1",
    }
    if configuration.webhook_token_file is not None:
        headers["Authorization"] = f"Bearer {read_secret_file(configuration.webhook_token_file)}"
    body = json.dumps(
        {
            "schema_version": SCHEMA_VERSION,
            "source": configuration.source_id,
            "generated_at": generated_at,
            "events": events,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    request = urllib.request.Request(
        configuration.webhook_url,
        data=body,
        headers=headers,
        method="POST",
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=10) as response:
        if response.status < 200 or response.status >= 300:
            raise ValueError("webhook status")
        response.read(1)


def write_results(
    collector: Collector,
    configuration: Configuration,
    generated_at: str,
    state: dict[str, Any],
    notification_status: str,
) -> None:
    critical = any(item.severity == "critical" for item in collector.observations)
    warning = any(item.severity == "warning" for item in collector.observations)
    status_value = "critical" if critical else "warning" if warning else "ok"
    collector.metrics.add("xs_nexus_monitor_status", 2 if critical else 1 if warning else 0)
    collector.metrics.add("xs_nexus_monitor_collected_timestamp_seconds", int(time.time()))
    for key, alert in active_alerts(collector.observations).items():
        collector.metrics.add(
            "xs_nexus_monitor_alert_active",
            1,
            alert=key,
            severity=alert["severity"],
        )
    snapshot = {
        "schema_version": SCHEMA_VERSION,
        "collected_at": generated_at,
        "status": status_value,
        "notification": {
            "status": notification_status,
            "pending_events": len(state["pending_events"]),
        },
        "observations": [asdict(item) for item in collector.observations],
        "controller_observability": collector.controller_snapshot,
    }
    alerts = {
        "schema_version": SCHEMA_VERSION,
        "collected_at": generated_at,
        "active": state["active"],
        "pending_events": state["pending_events"],
        "notification_status": notification_status,
    }
    atomic_write(
        configuration.output_directory / "metrics.prom",
        collector.metrics.render().encode("utf-8"),
        0o644,
    )
    atomic_write(
        configuration.output_directory / "snapshot.json",
        (json.dumps(snapshot, indent=2, sort_keys=True) + "\n").encode("utf-8"),
        0o600,
    )
    atomic_write(
        configuration.output_directory / "alerts.json",
        (json.dumps(alerts, indent=2, sort_keys=True) + "\n").encode("utf-8"),
        0o600,
    )
    atomic_write(
        configuration.state_file,
        (json.dumps(state, indent=2, sort_keys=True) + "\n").encode("utf-8"),
        0o600,
    )


def run() -> int:
    try:
        configuration = parse_configuration(dict(os.environ))
        ensure_secure_directory(configuration.output_directory)
        collector = Collector(configuration)
        collector.collect()
        generated_at = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        previous_state = load_state(configuration.state_file)
        current = active_alerts(collector.observations)
        pending, dropped_events = merge_pending(
            previous_state.get("pending_events", []),
            alert_transitions(previous_state, current, generated_at),
        )
        if dropped_events:
            collector.observe(
                "notification.queue_overflow",
                "critical",
                "notification",
                f"dropped_events={dropped_events}",
            )
            overflow = active_alerts(collector.observations)["notification.queue_overflow"]
            pending, _ = merge_pending(
                pending,
                [
                    alert_event(
                        "notification.queue_overflow",
                        "firing",
                        overflow,
                        generated_at,
                    )
                ],
            )
        notification_status = "not_configured"
        if configuration.webhook_url is not None:
            if pending:
                try:
                    deliver_webhook(configuration, pending, generated_at)
                    pending = []
                    notification_status = "delivered"
                except (OSError, ValueError, urllib.error.URLError) as error:
                    notification_status = "failed"
                    collector.observe(
                        "notification.delivery",
                        "critical",
                        "notification",
                        f"delivery_failed={type(error).__name__}",
                    )
            else:
                notification_status = "idle"
        current = active_alerts(collector.observations)
        state = {
            "schema_version": SCHEMA_VERSION,
            "updated_at": generated_at,
            "active": current,
            "pending_events": pending,
        }
        write_results(
            collector,
            configuration,
            generated_at,
            state,
            notification_status,
        )
        return 1 if any(item.severity == "critical" for item in collector.observations) else 0
    except (ConfigurationError, OSError, ValueError, json.JSONDecodeError) as error:
        log("critical", "configuration", f"monitor_failed={type(error).__name__}")
        return 2


if __name__ == "__main__":
    sys.exit(run())
