#!/usr/bin/env bash
set -Eeuo pipefail

REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPOSITORY_ROOT
EVIDENCE_DIRECTORY="${EVIDENCE_DIR:-}"
if [[ -n "${EVIDENCE_DIRECTORY}" ]]; then
    mkdir -p "${EVIDENCE_DIRECTORY}"
    chmod 0700 "${EVIDENCE_DIRECTORY}"
    EVIDENCE_DIRECTORY="$(cd -- "${EVIDENCE_DIRECTORY}" && pwd -P)"
fi
readonly EVIDENCE_DIRECTORY
TEMPORARY="$(mktemp -d)"
readonly TEMPORARY
SERVER_PID=""
TEST_STATUS="FAIL"

printf 'scenario\tstatus\n' >"${TEMPORARY}/scenario-matrix.tsv"

record_scenario() {
    printf '%s\tPASS\n' "$1" >>"${TEMPORARY}/scenario-matrix.tsv"
}

write_evidence() {
    local checksum_file
    [[ -n "${EVIDENCE_DIRECTORY}" ]] || return 0
    mkdir -p "${EVIDENCE_DIRECTORY}/samples"
    printf 'status=%s\n' "${TEST_STATUS}" >"${EVIDENCE_DIRECTORY}/status.txt"
    git -C "${REPOSITORY_ROOT}" rev-parse HEAD >"${EVIDENCE_DIRECTORY}/revision.txt"
    {
        uname -a
        python3 --version
        openssl version
    } >"${EVIDENCE_DIRECTORY}/environment.txt" 2>&1
    cp "${TEMPORARY}/scenario-matrix.tsv" "${EVIDENCE_DIRECTORY}/scenario-matrix.tsv"
    for name in healthy warning deduplicated critical recovered notification-failure; do
        if [[ -f "${TEMPORARY}/${name}.log" ]]; then
            cp "${TEMPORARY}/${name}.log" "${EVIDENCE_DIRECTORY}/${name}.log"
        fi
    done
    for name in metrics.prom snapshot.json alerts.json; do
        if [[ -f "${TEMPORARY}/output/${name}" ]]; then
            cp "${TEMPORARY}/output/${name}" "${EVIDENCE_DIRECTORY}/samples/${name}"
        fi
    done
    checksum_file="${TEMPORARY}/evidence-SHA256SUMS"
    (
        cd "${EVIDENCE_DIRECTORY}"
        find . -type f ! -name SHA256SUMS -print0 |
            sort -z |
            xargs -0 sha256sum
    ) >"${checksum_file}"
    mv -- "${checksum_file}" "${EVIDENCE_DIRECTORY}/SHA256SUMS"
}

cleanup() {
    local exit_status evidence_status
    exit_status=$1
    evidence_status=0
    set +e
    if [[ -n "${SERVER_PID}" ]]; then
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
    write_evidence || evidence_status=$?
    rm -rf -- "${TEMPORARY}"
    if ((exit_status == 0 && evidence_status != 0)); then
        return "${evidence_status}"
    fi
    return "${exit_status}"
}
trap 'cleanup $?' EXIT

mkdir -p \
    "${TEMPORARY}/bin" \
    "${TEMPORARY}/backups" \
    "${TEMPORARY}/logs" \
    "${TEMPORARY}/output" \
    "${TEMPORARY}/failure-output" \
    "${TEMPORARY}/systemd-root/etc/systemd/system" \
    "${TEMPORARY}/systemd-root/etc/xs-nexus" \
    "${TEMPORARY}/systemd-root/usr/local/libexec/xs-nexus" \
    "${TEMPORARY}/proc/net" \
    "${TEMPORARY}/proc/sys/fs" \
    "${TEMPORARY}/proc/1"
chmod 0700 "${TEMPORARY}/output" "${TEMPORARY}/failure-output"

cat >"${TEMPORARY}/proc/stat" <<'EOF'
cpu 100 0 100 800 0 0 0 0 0 0
EOF
cat >"${TEMPORARY}/proc/meminfo" <<'EOF'
MemTotal:       1000000 kB
MemAvailable:    800000 kB
EOF
cat >"${TEMPORARY}/proc/net/dev" <<'EOF'
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
EOF
printf '10 0 1000\n' >"${TEMPORARY}/proc/sys/fs/file-nr"
printf 'container log\n' >"${TEMPORARY}/container.log"
printf 'host log\n' >"${TEMPORARY}/logs/application.log"
touch "${TEMPORARY}/backups/current.dump.age"

now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf '{"schema_version":1,"operation":"replica_copy","status":"PASS","completed_at":"%s"}\n' \
    "${now}" >"${TEMPORARY}/copy-receipt.json"
printf '{"schema_version":1,"operation":"deep_verification","status":"PASS","completed_at":"%s"}\n' \
    "${now}" >"${TEMPORARY}/deep-receipt.json"
printf 'controller-monitor-token\n' >"${TEMPORARY}/controller.token"
printf 'webhook-monitor-token\n' >"${TEMPORARY}/webhook.token"
chmod 0600 \
    "${TEMPORARY}/controller.token" \
    "${TEMPORARY}/webhook.token" \
    "${TEMPORARY}/copy-receipt.json" \
    "${TEMPORARY}/deep-receipt.json"

openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 60 \
    -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost' \
    -keyout "${TEMPORARY}/tls.key" \
    -out "${TEMPORARY}/tls.crt" >/dev/null 2>&1
chmod 0600 "${TEMPORARY}/tls.key"

install -m 0755 "${REPOSITORY_ROOT}/scripts/check-production-health.sh" \
    "${TEMPORARY}/systemd-root/usr/local/libexec/xs-nexus/check-production-health"
install -m 0755 "${REPOSITORY_ROOT}/scripts/production_observability.py" \
    "${TEMPORARY}/systemd-root/usr/local/libexec/xs-nexus/production_observability.py"
install -m 0600 "${REPOSITORY_ROOT}/deploy/systemd/monitor.env.example" \
    "${TEMPORARY}/systemd-root/etc/xs-nexus/monitor.env"
install -m 0644 "${REPOSITORY_ROOT}/deploy/systemd/xs-nexus-healthcheck.service" \
    "${TEMPORARY}/systemd-root/etc/systemd/system/xs-nexus-healthcheck.service"
install -m 0644 "${REPOSITORY_ROOT}/deploy/systemd/xs-nexus-healthcheck.timer" \
    "${TEMPORARY}/systemd-root/etc/systemd/system/xs-nexus-healthcheck.timer"
systemd-analyze verify --recursive-errors=no --root="${TEMPORARY}/systemd-root" \
    xs-nexus-healthcheck.service xs-nexus-healthcheck.timer
record_scenario systemd_unit_verification

cat >"${TEMPORARY}/bin/docker" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail
case "${1:-}" in
    inspect)
        state="${TEST_CONTAINER_STATE:-running healthy}"
        if [[ "${state}" == missing ]]; then
            exit 1
        fi
        status="${state%% *}"
        health="${state#* }"
        printf '%s|%s|%s\n' "${status}" "${health}" "${TEST_CONTAINER_LOG:?}"
        ;;
    stats)
        printf '%s\n' '{"CPUPerc":"1.25%","MemPerc":"2.50%","NetIO":"1.5kB / 2.5kB","PIDs":"4"}'
        ;;
    *)
        exit 2
        ;;
esac
EOF

cat >"${TEMPORARY}/bin/curl" <<'EOF'
#!/usr/bin/env bash
if [[ "${TEST_CURL_STATUS:-0}" != 0 ]]; then
    exit "${TEST_CURL_STATUS}"
fi
printf '200'
EOF
chmod 0755 "${TEMPORARY}/bin/docker" "${TEMPORARY}/bin/curl"

cat >"${TEMPORARY}/server.py" <<'PY'
import json
import os
import ssl
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SNAPSHOT = {
    "schema_version": 1,
    "controller": {
        "status": "ok",
        "control_auth_failures_since_start": 0,
        "active_control_sessions": 1,
        "maximum_control_sessions": 1000,
        "rejected_control_sessions_since_start": 0,
        "active_configuration_sends": 0,
        "maximum_configuration_sends": 64,
        "maximum_nodes_per_network": 1000,
    },
    "database": {"status": "ok", "reason": None},
    "redis": {"status": "not_applicable", "reason": "controller_has_no_redis_dependency"},
    "nodes": {
        "managed": 1,
        "largest_network_managed": 1,
        "online": 1,
        "fresh_telemetry": 1,
        "unknown_path_nodes": 0,
        "disconnected_nodes": 0,
        "path_telemetry_complete": True,
        "path_observations": 1,
        "direct_path_observations": 1,
        "relay_path_observations": 0,
        "telemetry_nodes_24h": 1,
        "traffic_bytes_24h": 100,
        "handshake_attempts_24h": 1,
        "handshake_failures_24h": 0,
    },
    "relays": {
        "configured": 1,
        "unexpired_configured": 1,
        "fresh": 1,
        "telemetry_complete": True,
        "telemetry_relays_24h": 1,
        "packets_received_24h": 100,
        "packets_forwarded_24h": 100,
        "registration_retries_24h": 0,
        "registrations_rejected_24h": 0,
        "invalid_drops_24h": 0,
        "authentication_drops_24h": 0,
        "replay_drops_24h": 0,
        "rate_limit_drops_24h": 0,
        "queue_drops_24h": 0,
        "destination_drops_24h": 0,
        "send_drops_24h": 0,
        "packets_dropped_24h": 0,
        "io_errors_24h": 0,
    },
    "security": {
        "management_auth_failures_24h": 0,
        "control_auth_failures_since_start": 0,
        "acl_drops_24h": 0,
        "replay_drops_24h": 0,
    },
    "routing": {"enabled_routes": 0, "successful_changes_24h": 0},
    "updates": {"failed_nodes": 0},
    "audit": {"events_24h": 1, "total_events": 1, "oldest_event_at": None},
}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        return

    def do_GET(self):
        if self.path != "/observability" or self.headers.get("Authorization") != "Bearer controller-monitor-token":
            self.send_response(401)
            self.end_headers()
            return
        body = json.dumps(SNAPSHOT, separators=(",", ":")).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path != "/webhook" or self.headers.get("Authorization") != "Bearer webhook-monitor-token":
            self.send_response(401)
            self.end_headers()
            return
        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length))
        with Path(os.environ["WEBHOOK_LOG"]).open("a", encoding="utf-8") as output:
            output.write(json.dumps(payload, sort_keys=True) + "\n")
        self.send_response(204)
        self.end_headers()


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
Path(os.environ["READY_FILE"]).write_text(str(server.server_port), encoding="ascii")
tls_server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
tls_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
tls_context.load_cert_chain(os.environ["TLS_CERT_FILE"], os.environ["TLS_KEY_FILE"])
tls_server.socket = tls_context.wrap_socket(tls_server.socket, server_side=True)
Path(os.environ["TLS_READY_FILE"]).write_text(str(tls_server.server_port), encoding="ascii")
threading.Thread(target=tls_server.serve_forever, daemon=True).start()
server.serve_forever()
PY

WEBHOOK_LOG="${TEMPORARY}/webhook.log" \
READY_FILE="${TEMPORARY}/server.port" \
TLS_READY_FILE="${TEMPORARY}/tls-server.port" \
TLS_CERT_FILE="${TEMPORARY}/tls.crt" \
TLS_KEY_FILE="${TEMPORARY}/tls.key" \
    python3 "${TEMPORARY}/server.py" &
SERVER_PID=$!
for _ in $(seq 1 100); do
    [[ -s "${TEMPORARY}/server.port" && -s "${TEMPORARY}/tls-server.port" ]] && break
    sleep 0.05
done
[[ -s "${TEMPORARY}/server.port" ]]
[[ -s "${TEMPORARY}/tls-server.port" ]]
port="$(cat "${TEMPORARY}/server.port")"
tls_port="$(cat "${TEMPORARY}/tls-server.port")"

common_environment=(
    PATH="${TEMPORARY}/bin:${PATH}"
    XS_MONITOR_TEST_MODE=1
    XS_MONITOR_PROC_ROOT="${TEMPORARY}/proc"
    XS_MONITOR_CPU_SAMPLE_SECONDS=0
    XS_MONITOR_OUTPUT_DIR="${TEMPORARY}/output"
    XS_MONITOR_DISK_PATHS="${TEMPORARY}"
    XS_MONITOR_DISK_WARNING_PERCENT=80
    XS_MONITOR_DISK_CRITICAL_PERCENT=90
    XS_MONITOR_INODE_WARNING=99
    XS_MONITOR_INODE_CRITICAL=100
    XS_MONITOR_CPU_WARNING=99
    XS_MONITOR_CPU_CRITICAL=100
    XS_MONITOR_MEMORY_WARNING=99
    XS_MONITOR_MEMORY_CRITICAL=100
    XS_MONITOR_FD_WARNING=99
    XS_MONITOR_FD_CRITICAL=100
    XS_MONITOR_TASK_WARNING=100
    XS_MONITOR_TASK_CRITICAL=200
    XS_MONITOR_LOG_BYTES_WARNING=1000000
    XS_MONITOR_LOG_BYTES_CRITICAL=2000000
    XS_MONITOR_LOG_PATHS="${TEMPORARY}/logs"
    XS_MONITOR_CONTAINERS="controller relay"
    XS_MONITOR_HEALTH_URLS="http://127.0.0.1/health"
    XS_MONITOR_BACKUP_DIR="${TEMPORARY}/backups"
    XS_MONITOR_BACKUP_COPY_RECEIPT="${TEMPORARY}/copy-receipt.json"
    XS_MONITOR_BACKUP_DEEP_VERIFY_RECEIPT="${TEMPORARY}/deep-receipt.json"
    XS_MONITOR_TLS_TARGETS="localhost:${tls_port}"
    XS_MONITOR_CONTROLLER_OBSERVABILITY_URL="http://127.0.0.1:${port}/observability"
    XS_MONITOR_CONTROLLER_TOKEN_FILE="${TEMPORARY}/controller.token"
    XS_MONITOR_WEBHOOK_URL="http://127.0.0.1:${port}/webhook"
    XS_MONITOR_WEBHOOK_TOKEN_FILE="${TEMPORARY}/webhook.token"
    XS_MONITOR_ALLOW_HTTP_WEBHOOK_FOR_TESTS=1
    SSL_CERT_FILE="${TEMPORARY}/tls.crt"
    TEST_CONTAINER_LOG="${TEMPORARY}/container.log"
)

run_monitor() {
    env "${common_environment[@]}" "$@" \
        "${REPOSITORY_ROOT}/scripts/check-production-health.sh"
}

run_monitor XS_MONITOR_TEST_DISK_USAGE_PERCENT=50 >"${TEMPORARY}/healthy.log"
grep -Fq 'level=info check=disk' "${TEMPORARY}/healthy.log"
grep -Fq 'level=info check=controller' "${TEMPORARY}/healthy.log"
python3 -m json.tool "${TEMPORARY}/output/snapshot.json" >/dev/null
python3 -m json.tool "${TEMPORARY}/output/alerts.json" >/dev/null
grep -Fq 'xs_nexus_host_cpu_usage_percent' "${TEMPORARY}/output/metrics.prom"
grep -Fq 'xs_nexus_replay_drops_24h 0' "${TEMPORARY}/output/metrics.prom"
grep -Fq 'xs_nexus_control_sessions_maximum 1000' "${TEMPORARY}/output/metrics.prom"
grep -Fq 'level=info check=capacity' "${TEMPORARY}/healthy.log"
[[ "$(stat -c '%a' "${TEMPORARY}/output/metrics.prom")" == 644 ]]
[[ "$(stat -c '%a' "${TEMPORARY}/output/snapshot.json")" == 600 ]]
[[ "$(stat -c '%a' "${TEMPORARY}/output/alerts.json")" == 600 ]]
[[ "$(stat -c '%a' "${TEMPORARY}/output/state.json")" == 600 ]]
record_scenario healthy_collection

run_monitor XS_MONITOR_TEST_DISK_USAGE_PERCENT=81 >"${TEMPORARY}/warning.log"
grep -Fq 'level=warning check=disk' "${TEMPORARY}/warning.log"
[[ "$(wc -l <"${TEMPORARY}/webhook.log")" == 1 ]]
grep -Fq '"status": "firing"' "${TEMPORARY}/webhook.log"
record_scenario warning_delivery

run_monitor XS_MONITOR_TEST_DISK_USAGE_PERCENT=81 >"${TEMPORARY}/deduplicated.log"
[[ "$(wc -l <"${TEMPORARY}/webhook.log")" == 1 ]]
record_scenario unchanged_warning_deduplication

if run_monitor XS_MONITOR_TEST_DISK_USAGE_PERCENT=95 >"${TEMPORARY}/critical.log"; then
    printf 'critical disk usage unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=disk' "${TEMPORARY}/critical.log"
[[ "$(wc -l <"${TEMPORARY}/webhook.log")" == 2 ]]
record_scenario severity_change_delivery

run_monitor XS_MONITOR_TEST_DISK_USAGE_PERCENT=50 >"${TEMPORARY}/recovered.log"
[[ "$(wc -l <"${TEMPORARY}/webhook.log")" == 3 ]]
tail -1 "${TEMPORARY}/webhook.log" | grep -Fq '"status": "resolved"'
record_scenario recovery_delivery

if run_monitor \
    XS_MONITOR_TEST_DISK_USAGE_PERCENT=50 \
    TEST_CONTAINER_STATE='running unhealthy' >"${TEMPORARY}/container.log.output"; then
    printf 'unhealthy container unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=container' "${TEMPORARY}/container.log.output"
record_scenario unhealthy_container

rm -f "${TEMPORARY}/output/state.json"
if run_monitor \
    XS_MONITOR_TEST_DISK_USAGE_PERCENT=50 \
    TEST_CURL_STATUS=22 >"${TEMPORARY}/http.log"; then
    printf 'failed health endpoint unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=http' "${TEMPORARY}/http.log"
record_scenario failed_http_probe

failure_environment=(
    "${common_environment[@]}"
    XS_MONITOR_OUTPUT_DIR="${TEMPORARY}/failure-output"
    XS_MONITOR_WEBHOOK_URL=http://127.0.0.1:1/webhook
)
if env "${failure_environment[@]}" XS_MONITOR_TEST_DISK_USAGE_PERCENT=95 \
    "${REPOSITORY_ROOT}/scripts/check-production-health.sh" >"${TEMPORARY}/notification-failure.log"; then
    printf 'failed notification unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=notification' "${TEMPORARY}/notification-failure.log"
grep -Fq '"notification.delivery"' "${TEMPORARY}/failure-output/alerts.json"
record_scenario notification_delivery_failure

if grep -R -F -e controller-monitor-token -e webhook-monitor-token \
    "${TEMPORARY}/output" "${TEMPORARY}/failure-output"; then
    printf 'monitor output persisted a credential value\n' >&2
    exit 1
fi
if find "${TEMPORARY}/output" "${TEMPORARY}/failure-output" -maxdepth 1 -name '.*.????????' -print -quit | grep -q .; then
    printf 'atomic output left a temporary file\n' >&2
    exit 1
fi
record_scenario credential_non_persistence_and_atomic_writes

python3 - "${REPOSITORY_ROOT}" <<'PY'
import importlib.util
import os
import sys
import tempfile
from pathlib import Path

root = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location(
    "production_observability", root / "scripts" / "production_observability.py"
)
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)

assert module.parse_size("1.5kB") == 1500
assert module.parse_size("2MiB") == 2 * 1024 * 1024
assert module.parse_tls_target("example.com:443") == ("example.com", 443)
legacy = module.thresholds(
    {
        "XS_MONITOR_DISK_WARNING_PERCENT": "81",
        "XS_MONITOR_DISK_CRITICAL_PERCENT": "91",
    },
    "XS_MONITOR_DISK",
    80,
    90,
    100,
)
assert legacy.warning == 81
assert legacy.critical == 91
try:
    module.validate_webhook_url("https://user@example.com/hook", False)
except module.ConfigurationError:
    pass
else:
    raise AssertionError("webhook credentials must be rejected")
try:
    module.validate_webhook_url("https://example.com/hook?token=value", False)
except module.ConfigurationError:
    pass
else:
    raise AssertionError("webhook query must be rejected")
for probe in ("http://example.com/health", "https://example.com/health?credential=value"):
    try:
        module.validate_probe_url(
            probe,
            allow_loopback_http=True,
            require_loopback_http=True,
        )
    except module.ConfigurationError:
        pass
    else:
        raise AssertionError("unsafe probe URL must be rejected")
notification = {
    "severity": "critical",
    "check": "notification",
    "message": "delivery_failed",
}
assert module.alert_transitions(
    {"active": {"notification.delivery": notification}},
    {},
    "2026-01-01T00:00:00Z",
) == []


class CapacityFixture:
    def __init__(self):
        self.observations = []

    def observe(self, *values):
        self.observations.append(values)


for used, expected in ((799, "info"), (800, "warning"), (950, "critical")):
    fixture = CapacityFixture()
    module.Collector.capacity_observation(fixture, "capacity.fixture", used, 1000)
    assert fixture.observations[-1][1] == expected


class FakeResponse:
    status = 204

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self, amount):
        assert amount == 1
        return b""


class FakeOpener:
    def open(self, _request, timeout):
        assert timeout == 10
        return FakeResponse()


captured_handlers = []
original_build_opener = module.urllib.request.build_opener


def capture_opener(*handlers):
    captured_handlers.extend(handlers)
    return FakeOpener()


with tempfile.TemporaryDirectory() as directory:
    token_path = Path(directory) / "token"
    token_path.write_text("test-webhook-token\n", encoding="utf-8")
    os.chmod(token_path, 0o600)
    configuration = type(
        "WebhookConfiguration",
        (),
        {
            "webhook_url": "https://example.com/hook",
            "webhook_token_file": token_path,
            "source_id": "test-monitor",
        },
    )()
    module.urllib.request.build_opener = capture_opener
    try:
        module.deliver_webhook(
            configuration,
            [{"id": "event"}],
            "2026-01-01T00:00:00Z",
        )
    finally:
        module.urllib.request.build_opener = original_build_opener

assert any(
    isinstance(handler, module.urllib.request.ProxyHandler)
    and handler.proxies == {}
    for handler in captured_handlers
)
events = [{"id": str(index)} for index in range(module.MAX_PENDING_EVENTS + 3)]
pending, dropped = module.merge_pending(events, [])
assert len(pending) == module.MAX_PENDING_EVENTS
assert dropped == 3
PY

record_scenario parser_and_state_guards
TEST_STATUS="PASS"

printf 'production observability guard tests passed\n'
