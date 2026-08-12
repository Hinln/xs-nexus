#!/usr/bin/env python3

import csv
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path


SERVICES = ("controller", "relay", "console", "postgres", "agent-a", "agent-b")
EXPECTED_PIDS = {
    "controller": 2,
    "relay": 2,
    "console": 1,
    "postgres": 2,
    "agent-a": 2,
    "agent-b": 1,
}
RSS_LIMIT_KIB = {
    "controller": 512 * 1024,
    "relay": 512 * 1024,
    "console": 512 * 1024,
    "postgres": 512 * 1024,
    "agent-a": 256 * 1024,
    "agent-b": 256 * 1024,
}


def integer(row, key):
    value = row[key]
    return None if value == "" else int(value)


def median_window(values, first):
    width = max(1, len(values) // 4)
    selected = values[:width] if first else values[-width:]
    return statistics.median(selected)


def assert_bounded_growth(service, samples, field, absolute_allowance, ratio):
    values = [item[field] for item in samples]
    start = median_window(values, True)
    end = median_window(values, False)
    limit = max(start + absolute_allowance, start * ratio)
    if end > limit:
        raise AssertionError(
            f"{service} {field} end-window median {end} exceeded bounded-growth limit {limit}"
        )
    return {"start_window_median": start, "end_window_median": end, "limit": limit}


def main():
    if len(sys.argv) != 5:
        raise SystemExit(
            "usage: summarize-current-revision-soak.py RESOURCES EVENTS MIN_SAMPLES OUTPUT"
        )
    resources_path, events_path, min_samples_text, output_path = sys.argv[1:]
    min_samples = int(min_samples_text)
    if min_samples < 2:
        raise AssertionError("minimum sample count must be at least two")

    rows = list(csv.DictReader(Path(resources_path).open(encoding="utf-8")))
    by_service = defaultdict(list)
    for row in rows:
        service = row["service"]
        if service not in SERVICES:
            raise AssertionError(f"unexpected service in resource samples: {service}")
        by_service[service].append(
            {
                "epoch": int(row["epoch"]),
                "pid": int(row["root_pid"]),
                "rss_kib": int(row["rss_kib"]),
                "threads": int(row["threads"]),
                "fds": int(row["fds"]),
                "cpu_ticks": int(row["cpu_ticks"]),
                "log_bytes": int(row["log_bytes"]),
                "restart_count": int(row["restart_count"]),
                "route_count": integer(row, "route_count"),
                "rule_count": integer(row, "rule_count"),
                "interface_count": integer(row, "interface_count"),
                "db_connections": integer(row, "db_connections"),
                "path_kind": row["path_kind"] or None,
                "tx_packets_total": integer(row, "tx_packets_total"),
                "rx_packets_total": integer(row, "rx_packets_total"),
            }
        )

    summary = {"services": {}, "events": {}}
    for service in SERVICES:
        samples = by_service[service]
        if len(samples) < min_samples:
            raise AssertionError(
                f"{service} has {len(samples)} samples, expected at least {min_samples}"
            )
        if min(item["rss_kib"] for item in samples) <= 0:
            raise AssertionError(f"{service} RSS sampling failed")
        if max(item["rss_kib"] for item in samples) >= RSS_LIMIT_KIB[service]:
            raise AssertionError(f"{service} exceeded RSS limit")
        if min(item["threads"] for item in samples) <= 0 or max(
            item["threads"] for item in samples
        ) >= 512:
            raise AssertionError(f"{service} thread count is outside the accepted range")
        if min(item["fds"] for item in samples) <= 0 or max(
            item["fds"] for item in samples
        ) >= 2048:
            raise AssertionError(f"{service} file-descriptor count is outside the accepted range")
        if max(item["log_bytes"] for item in samples) > 50 * 1024 * 1024:
            raise AssertionError(f"{service} log rotation bound exceeded")
        pids = sorted({item["pid"] for item in samples})
        if len(pids) != EXPECTED_PIDS[service]:
            raise AssertionError(f"{service} observed unexpected PIDs: {pids}")
        restart_counts = sorted({item["restart_count"] for item in samples})
        if len(restart_counts) != 1:
            raise AssertionError(
                f"{service} had an unexpected restart-policy restart: {restart_counts}"
            )

        rss_growth = assert_bounded_growth(service, samples, "rss_kib", 32 * 1024, 1.5)
        fd_growth = assert_bounded_growth(service, samples, "fds", 32, 1.5)
        thread_growth = assert_bounded_growth(service, samples, "threads", 16, 1.5)
        service_summary = {
            "samples": len(samples),
            "observed_pids": pids,
            "restart_count": restart_counts[0],
            "rss_kib_min": min(item["rss_kib"] for item in samples),
            "rss_kib_max": max(item["rss_kib"] for item in samples),
            "fds_min": min(item["fds"] for item in samples),
            "fds_max": max(item["fds"] for item in samples),
            "threads_min": min(item["threads"] for item in samples),
            "threads_max": max(item["threads"] for item in samples),
            "cpu_ticks_delta": max(item["cpu_ticks"] for item in samples)
            - min(item["cpu_ticks"] for item in samples),
            "log_bytes_start": samples[0]["log_bytes"],
            "log_bytes_end": samples[-1]["log_bytes"],
            "rss_growth": rss_growth,
            "fd_growth": fd_growth,
            "thread_growth": thread_growth,
        }
        if service == "postgres":
            connections = [item["db_connections"] for item in samples]
            if any(value is None or value < 1 or value > 32 for value in connections):
                raise AssertionError(f"PostgreSQL connection samples are invalid: {connections}")
            service_summary["db_connections_min"] = min(connections)
            service_summary["db_connections_max"] = max(connections)
        if service.startswith("agent-"):
            for field in ("route_count", "rule_count", "interface_count"):
                values = [item[field] for item in samples]
                if any(value is None or value <= 0 for value in values):
                    raise AssertionError(f"{service} {field} sampling failed")
                service_summary[f"{field}_min"] = min(values)
                service_summary[f"{field}_max"] = max(values)
            path_samples = [item["path_kind"] for item in samples if item["path_kind"]]
            if not path_samples:
                raise AssertionError(f"{service} has no authenticated path samples")
            service_summary["path_kinds"] = sorted(set(path_samples))

        summary["services"][service] = service_summary

    event_rows = list(csv.DictReader(Path(events_path).open(encoding="utf-8"), delimiter="\t"))
    required_events = {
        "controller_restart",
        "relay_restart",
        "postgres_restart",
        "agent_restart",
        "configuration_update",
        "enrollment_revocation",
        "network_loss_latency",
        "subnet_route_update",
    }
    observed_events = defaultdict(list)
    for row in event_rows:
        observed_events[row["event"]].append(row["status"])
    missing = required_events - observed_events.keys()
    if missing:
        raise AssertionError(f"missing required fault events: {sorted(missing)}")
    failed = {
        event: statuses
        for event, statuses in observed_events.items()
        if statuses != ["PASS"]
    }
    if failed:
        raise AssertionError(f"fault event status is not exactly one PASS: {failed}")
    summary["events"] = {event: statuses[0] for event, statuses in sorted(observed_events.items())}

    Path(output_path).write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
