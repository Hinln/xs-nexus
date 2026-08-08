#!/usr/bin/env bash
set -Eeuo pipefail

REPOSITORY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPOSITORY_ROOT
TEMPORARY="$(mktemp -d)"
readonly TEMPORARY
trap 'rm -rf -- "${TEMPORARY}"' EXIT

mkdir -p "${TEMPORARY}/bin" "${TEMPORARY}/backups"

cat >"${TEMPORARY}/bin/df" <<'EOF'
#!/usr/bin/env bash
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\n'
printf '/dev/test 100 81 19 %s%% /\n' "${TEST_DISK_USAGE:-81}"
EOF

cat >"${TEMPORARY}/bin/docker" <<'EOF'
#!/usr/bin/env bash
if [[ "${TEST_CONTAINER_STATE:-running healthy}" == missing ]]; then
    exit 1
fi
printf '%s\n' "${TEST_CONTAINER_STATE:-running healthy}"
EOF

cat >"${TEMPORARY}/bin/curl" <<'EOF'
#!/usr/bin/env bash
exit "${TEST_CURL_STATUS:-0}"
EOF

chmod 0755 "${TEMPORARY}/bin/df" "${TEMPORARY}/bin/docker" "${TEMPORARY}/bin/curl"
touch "${TEMPORARY}/backups/current.dump.age"

common_environment=(
    PATH="${TEMPORARY}/bin:${PATH}"
    XS_MONITOR_CONTAINERS="controller relay"
    XS_MONITOR_HEALTH_URLS="http://127.0.0.1/health"
    XS_MONITOR_BACKUP_DIR="${TEMPORARY}/backups"
)

env "${common_environment[@]}" TEST_DISK_USAGE=81 \
    "${REPOSITORY_ROOT}/scripts/check-production-health.sh" >"${TEMPORARY}/warning.log"
grep -Fq 'level=warning check=disk' "${TEMPORARY}/warning.log"

if env "${common_environment[@]}" TEST_DISK_USAGE=95 \
    "${REPOSITORY_ROOT}/scripts/check-production-health.sh" >"${TEMPORARY}/critical.log"; then
    printf 'critical disk usage unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=disk' "${TEMPORARY}/critical.log"

if env "${common_environment[@]}" TEST_CONTAINER_STATE='running unhealthy' \
    "${REPOSITORY_ROOT}/scripts/check-production-health.sh" >"${TEMPORARY}/container.log"; then
    printf 'unhealthy container unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=container' "${TEMPORARY}/container.log"

if env "${common_environment[@]}" TEST_CURL_STATUS=22 \
    "${REPOSITORY_ROOT}/scripts/check-production-health.sh" >"${TEMPORARY}/http.log"; then
    printf 'failed health endpoint unexpectedly passed\n' >&2
    exit 1
fi
grep -Fq 'level=critical check=http' "${TEMPORARY}/http.log"

printf 'production health guard tests passed\n'
