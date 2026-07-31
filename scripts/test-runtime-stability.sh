#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"
ENVIRONMENT_FILE=${1:?usage: test-runtime-stability.sh ENV_FILE [EVIDENCE_DIR]}
EVIDENCE_DIR=${2:-"$ROOT_DIR/artifacts/qa/runtime-stability-$(date -u +%Y%m%dT%H%M%SZ)"}
DURATION_SECONDS=${XS_STABILITY_DURATION_SECONDS:-300}
SAMPLE_INTERVAL_SECONDS=${XS_STABILITY_SAMPLE_INTERVAL_SECONDS:-10}
CONTROLLER_PORT=${XS_CONTROLLER_HTTP_PORT:-38180}
CONSOLE_PORT=${XS_CONSOLE_HTTP_PORT:-38181}
PROJECT=${XS_COMPOSE_PROJECT_NAME:-xs-nexus-dev}

[[ $DURATION_SECONDS =~ ^[0-9]+$ && $DURATION_SECONDS -ge 30 ]]
[[ $SAMPLE_INTERVAL_SECONDS =~ ^[0-9]+$ && $SAMPLE_INTERVAL_SECONDS -ge 1 ]]
[[ -r $ENVIRONMENT_FILE ]]
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"

compose() {
    docker compose --env-file "$ENVIRONMENT_FILE" -f "$COMPOSE_FILE" "$@"
}

wait_for_http() {
    local url=$1
    for _attempt in $(seq 1 30); do
        if curl --fail --silent --show-error "$url" >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done
    return 1
}

wait_for_service() {
    local service=$1
    case "$service" in
        controller) wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready" ;;
        console) wait_for_http "http://127.0.0.1:$CONSOLE_PORT/console-health" ;;
        relay)
            local container
            container=$(compose ps -q relay)
            for _attempt in $(seq 1 30); do
                if docker exec "$container" /usr/local/bin/xs-relay healthcheck >/dev/null 2>&1; then
                    return
                fi
                sleep 1
            done
            return 1
            ;;
        *) return 2 ;;
    esac
}

sample_service() {
    local timestamp=$1 service=$2 container pid metrics rss_kib threads fds cpu_ticks log_path log_bytes restarts
    container=$(compose ps -q "$service")
    [[ -n $container ]]
    pid=$(docker inspect "$container" --format '{{.State.Pid}}')
    metrics=$(python3 - "$pid" <<'PY'
import os
import sys
from pathlib import Path

root = int(sys.argv[1])
parents = {}
for entry in Path("/proc").iterdir():
    if not entry.name.isdigit():
        continue
    try:
        stat = (entry / "stat").read_text().rsplit(") ", 1)[1].split()
        parents[int(entry.name)] = int(stat[1])
    except (FileNotFoundError, IndexError, PermissionError, ProcessLookupError, ValueError):
        pass

pids = {root}
changed = True
while changed:
    changed = False
    for pid, parent in parents.items():
        if parent in pids and pid not in pids:
            pids.add(pid)
            changed = True

rss_kib = threads = fds = cpu_ticks = 0
for pid in pids:
    proc = Path("/proc") / str(pid)
    try:
        values = {}
        for line in (proc / "status").read_text().splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                if key in {"VmRSS", "Threads"}:
                    tokens = value.strip().split()
                    if tokens:
                        values[key] = tokens[0]
        rss_kib += int(values.get("VmRSS", 0))
        threads += int(values.get("Threads", 0))
        fds += len(list((proc / "fd").iterdir()))
        stat = (proc / "stat").read_text().rsplit(") ", 1)[1].split()
        cpu_ticks += int(stat[11]) + int(stat[12])
    except (FileNotFoundError, IndexError, PermissionError, ProcessLookupError, ValueError):
        pass

print(rss_kib, threads, fds, cpu_ticks)
PY
)
    read -r rss_kib threads fds cpu_ticks <<<"$metrics"
    log_path=$(docker inspect "$container" --format '{{.LogPath}}')
    if [[ -n $log_path && -e $log_path ]]; then
        log_bytes=$(stat -c %s "$log_path")
    else
        log_bytes=0
    fi
    restarts=$(docker inspect "$container" --format '{{.RestartCount}}')
    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
        "$timestamp" "$service" "$pid" "$rss_kib" "$threads" "$fds" "$cpu_ticks" "$log_bytes" "$restarts" \
        >>"$EVIDENCE_DIR/resources.csv"
}

assert_project_scope() {
    local service container project_label
    for service in controller relay console; do
        container=$(compose ps -q "$service")
        [[ -n $container ]]
        project_label=$(docker inspect "$container" --format '{{index .Config.Labels "com.docker.compose.project"}}')
        [[ $project_label == "$PROJECT" ]]
    done
}

printf 'timestamp_utc,service,pid,rss_kib,threads,fds,cpu_ticks,log_bytes,restart_count\n' >"$EVIDENCE_DIR/resources.csv"
git -C "$ROOT_DIR" rev-parse HEAD >"$EVIDENCE_DIR/revision.txt"
printf '%s\n' "$DURATION_SECONDS" >"$EVIDENCE_DIR/duration-seconds.txt"
printf '%s\n' "$SAMPLE_INTERVAL_SECONDS" >"$EVIDENCE_DIR/sample-interval-seconds.txt"
compose config --images >"$EVIDENCE_DIR/images.txt"
compose ps --format json >"$EVIDENCE_DIR/containers-before.json"
assert_project_scope

start_epoch=$(date +%s)
deadline=$((start_epoch + DURATION_SECONDS))
fault_epoch=$((start_epoch + DURATION_SECONDS / 2))
faults_injected=0
while (( $(date +%s) < deadline )); do
    timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    for service in controller relay console; do
        wait_for_service "$service"
        sample_service "$timestamp" "$service"
    done

    if (( faults_injected == 0 && $(date +%s) >= fault_epoch )); then
        for service in controller relay console; do
            container=$(compose ps -q "$service")
            docker restart --time 20 "$container" >"$EVIDENCE_DIR/restart-$service.txt"
            wait_for_service "$service"
            sample_service "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$service"
        done
        faults_injected=1
    fi
    sleep "$SAMPLE_INTERVAL_SECONDS"
done
(( faults_injected == 1 ))

for service in controller relay console; do
    wait_for_service "$service"
done
compose ps --format json >"$EVIDENCE_DIR/containers-after.json"

python3 - "$EVIDENCE_DIR/resources.csv" "$EVIDENCE_DIR/summary.json" <<'PY'
import csv
import json
import sys
from collections import defaultdict
from pathlib import Path

rows = list(csv.DictReader(Path(sys.argv[1]).open(encoding="utf-8")))
by_service = defaultdict(list)
for row in rows:
    by_service[row["service"]].append({
        key: int(row[key])
        for key in ("pid", "rss_kib", "threads", "fds", "cpu_ticks", "log_bytes", "restart_count")
    })

summary = {}
for service in ("controller", "relay", "console"):
    samples = by_service[service]
    assert len(samples) >= 2, f"insufficient samples for {service}"
    before = samples[0]
    after = samples[-1]
    assert min(item["rss_kib"] for item in samples) > 0, f"{service} RSS sampling failed"
    assert min(item["fds"] for item in samples) > 0, f"{service} file-descriptor sampling failed"
    assert min(item["threads"] for item in samples) > 0, f"{service} thread sampling failed"
    assert max(item["rss_kib"] for item in samples) < 512 * 1024, f"{service} exceeded 512 MiB RSS"
    assert max(item["fds"] for item in samples) < 512, f"{service} exceeded 512 file descriptors"
    assert max(item["threads"] for item in samples) < 256, f"{service} exceeded 256 threads"
    assert max(item["log_bytes"] for item in samples) <= 50 * 1024 * 1024, f"{service} log rotation bound exceeded"
    summary[service] = {
        "samples": len(samples),
        "rss_kib_min": min(item["rss_kib"] for item in samples),
        "rss_kib_max": max(item["rss_kib"] for item in samples),
        "fds_min": min(item["fds"] for item in samples),
        "fds_max": max(item["fds"] for item in samples),
        "threads_min": min(item["threads"] for item in samples),
        "threads_max": max(item["threads"] for item in samples),
        "cpu_ticks_delta": max(item["cpu_ticks"] for item in samples) - min(item["cpu_ticks"] for item in samples),
        "log_bytes_start": before["log_bytes"],
        "log_bytes_end": after["log_bytes"],
        "observed_pids": sorted({item["pid"] for item in samples}),
    }

Path(sys.argv[2]).write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

printf 'runtime stability verification passed; evidence: %s\n' "$EVIDENCE_DIR"
