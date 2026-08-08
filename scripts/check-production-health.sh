#!/usr/bin/env bash
set -Eeuo pipefail

readonly DISK_WARNING_PERCENT="${XS_MONITOR_DISK_WARNING_PERCENT:-80}"
readonly DISK_CRITICAL_PERCENT="${XS_MONITOR_DISK_CRITICAL_PERCENT:-90}"
readonly BACKUP_MAX_AGE_SECONDS="${XS_MONITOR_BACKUP_MAX_AGE_SECONDS:-93600}"
readonly TLS_MIN_VALIDITY_SECONDS="${XS_MONITOR_TLS_MIN_VALIDITY_SECONDS:-1209600}"

result=0
read -r -a disk_paths <<<"${XS_MONITOR_DISK_PATHS:-/}"
read -r -a containers <<<"${XS_MONITOR_CONTAINERS:-}"
read -r -a health_urls <<<"${XS_MONITOR_HEALTH_URLS:-}"
read -r -a tls_targets <<<"${XS_MONITOR_TLS_TARGETS:-}"

log() {
    printf '%s level=%s check=%s message=%q\n' \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" "$2" "$3"
}

require_positive_integer() {
    if [[ ! "$2" =~ ^[1-9][0-9]*$ ]]; then
        log critical configuration "$1 must be a positive integer"
        exit 2
    fi
}

require_positive_integer XS_MONITOR_DISK_WARNING_PERCENT "${DISK_WARNING_PERCENT}"
require_positive_integer XS_MONITOR_DISK_CRITICAL_PERCENT "${DISK_CRITICAL_PERCENT}"
require_positive_integer XS_MONITOR_BACKUP_MAX_AGE_SECONDS "${BACKUP_MAX_AGE_SECONDS}"
require_positive_integer XS_MONITOR_TLS_MIN_VALIDITY_SECONDS "${TLS_MIN_VALIDITY_SECONDS}"

if ((DISK_WARNING_PERCENT >= DISK_CRITICAL_PERCENT || DISK_CRITICAL_PERCENT > 100)); then
    log critical configuration "disk thresholds must satisfy warning < critical <= 100"
    exit 2
fi

for path in "${disk_paths[@]}"; do
    usage="$(df -P -- "$path" | awk 'NR == 2 { gsub(/%/, "", $5); print $5 }')"
    if [[ ! "${usage}" =~ ^[0-9]+$ ]]; then
        log critical disk "unable to read usage for ${path}"
        result=1
    elif ((usage >= DISK_CRITICAL_PERCENT)); then
        log critical disk "${path} usage is ${usage}%"
        result=1
    elif ((usage >= DISK_WARNING_PERCENT)); then
        log warning disk "${path} usage is ${usage}%"
    else
        log info disk "${path} usage is ${usage}%"
    fi
done

for container in "${containers[@]}"; do
    if ! state="$(docker inspect --format '{{.State.Status}} {{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$container" 2>/dev/null)"; then
        log critical container "${container} is missing"
        result=1
    elif [[ "${state}" != "running healthy" && "${state}" != "running none" ]]; then
        log critical container "${container} state is ${state}"
        result=1
    else
        log info container "${container} state is ${state}"
    fi
done

for url in "${health_urls[@]}"; do
    if curl --fail --silent --show-error --max-time 10 --output /dev/null "$url"; then
        log info http "${url} is healthy"
    else
        log critical http "${url} failed"
        result=1
    fi
done

if [[ -n "${XS_MONITOR_BACKUP_DIR:-}" ]]; then
    if [[ ! -d "${XS_MONITOR_BACKUP_DIR}" ]]; then
        log critical backup "backup directory is missing"
        result=1
    else
        newest="$(find "${XS_MONITOR_BACKUP_DIR}" -maxdepth 1 -type f -name '*.age' -printf '%T@ %p\n' | sort -n | tail -1)"
        if [[ -z "${newest}" ]]; then
            log critical backup "no encrypted backup exists"
            result=1
        else
            timestamp="${newest%% *}"
            timestamp="${timestamp%%.*}"
            age_seconds="$(($(date -u +%s) - timestamp))"
            if ((age_seconds < 0 || age_seconds > BACKUP_MAX_AGE_SECONDS)); then
                log critical backup "newest encrypted backup age is ${age_seconds}s"
                result=1
            else
                log info backup "newest encrypted backup age is ${age_seconds}s"
            fi
        fi
    fi
fi

for target in "${tls_targets[@]}"; do
    host="${target%%:*}"
    if [[ -z "${host}" || "${host}" == "${target}" ]]; then
        log critical configuration "invalid TLS target ${target}"
        result=1
    elif openssl s_client -connect "${target}" -servername "${host}" </dev/null 2>/dev/null \
        | openssl x509 -noout -checkend "${TLS_MIN_VALIDITY_SECONDS}" >/dev/null; then
        log info tls "${target} certificate validity is sufficient"
    else
        log critical tls "${target} certificate expires too soon or is unavailable"
        result=1
    fi
done

exit "${result}"
