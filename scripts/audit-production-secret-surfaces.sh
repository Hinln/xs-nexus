#!/usr/bin/env bash

set -Eeuo pipefail
umask 077

fail() {
  printf 'production secret audit failed: %s\n' "$1" >&2
  exit 1
}

require_argument() {
  local option=$1
  local value=${2:-}
  [[ -n $value ]] || fail "missing value for $option"
}

audit_dir=
ci_archive=
deployment=rc
deployment_root=
qa_root=/srv/xs-nexus-qa
repository=/srv/xs-nexus
scanner=

while (($# > 0)); do
  case "$1" in
    --audit-dir)
      require_argument "$1" "${2:-}"
      audit_dir=$2
      shift 2
      ;;
    --ci-archive)
      require_argument "$1" "${2:-}"
      ci_archive=$2
      shift 2
      ;;
    --deployment)
      require_argument "$1" "${2:-}"
      deployment=$2
      shift 2
      ;;
    --deployment-root)
      require_argument "$1" "${2:-}"
      deployment_root=$2
      shift 2
      ;;
    --qa-root)
      require_argument "$1" "${2:-}"
      qa_root=$2
      shift 2
      ;;
    --repository)
      require_argument "$1" "${2:-}"
      repository=$2
      shift 2
      ;;
    --scanner)
      require_argument "$1" "${2:-}"
      scanner=$2
      shift 2
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ $(id -u) -eq 0 ]] || fail 'must run as root'
[[ $deployment =~ ^[a-z0-9][a-z0-9-]{0,31}$ ]] || fail 'invalid deployment name'
[[ -n $audit_dir ]] || fail '--audit-dir is required'
[[ -n $scanner ]] || fail '--scanner is required'

audit_dir=$(realpath -e -- "$audit_dir")
repository=$(realpath -e -- "$repository")
scanner=$(realpath -e -- "$scanner")
if [[ -n $qa_root ]]; then
  qa_root=$(realpath -e -- "$qa_root")
fi
if [[ -n $ci_archive ]]; then
  ci_archive=$(realpath -e -- "$ci_archive")
fi
if [[ -z $deployment_root ]]; then
  deployment_root="/etc/xs-nexus/deployments/$deployment"
fi
deployment_root=$(realpath -e -- "$deployment_root")

[[ $audit_dir == /tmp/xs-nexus-secret-audit.* ]] \
  || fail 'audit directory must be a dedicated /tmp/xs-nexus-secret-audit.* path'
[[ $(stat -c %u "$audit_dir") -eq 0 ]] || fail 'audit directory must be root-owned'
audit_mode=$(stat -c %a "$audit_dir")
(((8#$audit_mode & 077) == 0)) || fail 'audit directory permissions are too broad'
[[ -f $scanner && ! -L $scanner ]] || fail 'scanner must be a regular non-symlink file'
[[ -d $repository/.git ]] || fail 'repository is not a Git checkout'
[[ -z $ci_archive || -f $ci_archive ]] || fail 'CI archive is not a regular file'

config_dir="$audit_dir/config"
history_dir="$audit_dir/history"
log_dir="$audit_dir/logs"
report_dir="$audit_dir/reports"
install -d -m 0700 "$config_dir" "$history_dir" "$log_dir" "$report_dir"

network_before=$(docker network inspect 1panel-network | sha256sum | awk '{print $1}')
default_route_before=$(ip -json route show default | sha256sum | awk '{print $1}')
nft_before=$(
  nft list ruleset \
    | sed -E 's/counter packets [0-9]+ bytes [0-9]+/counter/g; s/handle [0-9]+//g' \
    | sha256sum \
    | awk '{print $1}'
)
failed_units_before=$(systemctl --failed --no-legend | sha256sum | awk '{print $1}')

compose_environment="/etc/xs-nexus/deployments/$deployment.compose.env"
for config_file in "$compose_environment" "$compose_environment".pre-* /etc/xs-nexus/monitor.env; do
  [[ -f $config_file ]] || continue
  install -m 0600 "$config_file" "$config_dir/$(basename "$config_file")"
done
for public_file in \
  "$deployment_root/controller/relay-catalog.json" \
  "$deployment_root/controller/backup-recipient" \
  "$deployment_root/controller/update-signing-public-key" \
  "$deployment_root/relay/controller-credential-public-key"; do
  [[ -f $public_file ]] || continue
  install -m 0600 "$public_file" "$config_dir/$(basename "$(dirname "$public_file")")-$(basename "$public_file")"
done

for history_file in /root/.bash_history /home/ubuntu/.bash_history; do
  [[ -f $history_file ]] || continue
  install -m 0600 "$history_file" "$history_dir/$(basename "$(dirname "$history_file")")-$(basename "$history_file")"
done
if [[ -f /var/log/auth.log ]]; then
  install -m 0600 /var/log/auth.log "$log_dir/auth.log"
fi
journalctl --no-pager -u xs-nexus-healthcheck.service -u xs-agent.service \
  >"$log_dir/systemd-project.log" 2>&1 || true

mapfile -t containers < <(
  docker ps -a --format '{{.Names}}' \
    | grep -E "^xs-nexus-${deployment}(-|$)" \
    | sort
)
((${#containers[@]} > 0)) || fail 'no project containers found'
for container in "${containers[@]}"; do
  docker logs --timestamps "$container" >"$log_dir/$container.log" 2>&1 || true
  docker inspect "$container" >"$log_dir/$container.inspect.json"
  image_id=$(docker inspect -f '{{.Image}}' "$container")
  docker history --no-trunc "$image_id" >"$log_dir/$container.image-history.txt"
done

reference_arguments=()
add_reference() {
  local reference_id=$1
  local path=$2
  [[ -f $path && ! -L $path ]] || fail "missing reference file: $reference_id"
  reference_arguments+=(--reference-file "$reference_id=$path")
}

add_reference CRED_ADMIN_API_TOKEN "$deployment_root/controller/admin-api-token"
add_reference CRED_CONSOLE_BOOTSTRAP "$deployment_root/controller/console-bootstrap-password"
add_reference CRED_CONTROLLER_CONFIG_SIGNING "$deployment_root/controller/configuration-signing-key"
add_reference CRED_CONTROLLER_CREDENTIAL_SIGNING "$deployment_root/controller/credential-signing-key"
add_reference CRED_DATABASE_URL "$deployment_root/controller/database-url"
add_reference CRED_POSTGRES_BOOTSTRAP "$deployment_root/postgres/postgres-password"
add_reference CRED_RELAY_IDENTITY "$deployment_root/relay/identity-key"

scan_arguments=(
  --root "$repository"
  --surface current-tree
  --surface git-history
  --directory "system-config=$config_dir"
  --directory "shell-history=$history_dir"
  --directory "runtime-logs=$log_dir"
  --max-file-size 500000000
  --require-complete
  --output "$report_dir/host-surfaces.json"
)
if [[ -n $qa_root ]]; then
  scan_arguments+=(--directory "qa-evidence=$qa_root")
  archive_counter=0
  while IFS= read -r -d '' archive_path; do
    archive_counter=$((archive_counter + 1))
    scan_arguments+=(--archive "qa-archive-$archive_counter=$archive_path")
  done < <(
    find "$qa_root" -type f \
      \( -iname '*.tar' -o -iname '*.tar.gz' -o -iname '*.tgz' -o -iname '*.zip' \) \
      -print0
  )
fi
if [[ -n $ci_archive ]]; then
  scan_arguments+=(--archive "retained-ci=$ci_archive")
fi
scan_arguments+=("${reference_arguments[@]}")
python3 "$scanner" "${scan_arguments[@]}"

mapfile -t image_ids < <(
  for container in "${containers[@]}"; do
    docker inspect -f '{{.Image}}' "$container"
  done | sort -u
)
image_counter=0
for image_id in "${image_ids[@]}"; do
  image_counter=$((image_counter + 1))
  image_tar="$audit_dir/runtime-image-$image_counter.tar"
  docker save --output "$image_tar" "$image_id"
  python3 "$scanner" \
    --root "$repository" \
    --archive "runtime-image-$image_counter=$image_tar" \
    --max-file-size 500000000 \
    --require-complete \
    --output "$report_dir/runtime-image-$image_counter.json" \
    "${reference_arguments[@]}"
  rm -f -- "$image_tar"
done

network_after=$(docker network inspect 1panel-network | sha256sum | awk '{print $1}')
default_route_after=$(ip -json route show default | sha256sum | awk '{print $1}')
nft_after=$(
  nft list ruleset \
    | sed -E 's/counter packets [0-9]+ bytes [0-9]+/counter/g; s/handle [0-9]+//g' \
    | sha256sum \
    | awk '{print $1}'
)
failed_units_after=$(systemctl --failed --no-legend | sha256sum | awk '{print $1}')

[[ $network_before == "$network_after" ]] || fail '1panel-network changed during audit'
[[ $default_route_before == "$default_route_after" ]] || fail 'default route changed during audit'
[[ $nft_before == "$nft_after" ]] || fail 'nftables semantics changed during audit'
[[ $failed_units_before == "$failed_units_after" ]] || fail 'failed systemd units changed during audit'

{
  printf 'timestamp=%s\n' "$(date --iso-8601=seconds)"
  printf 'repository_head=%s\n' "$(git -C "$repository" rev-parse HEAD)"
  printf 'deployment=%s\n' "$deployment"
  printf 'container_count=%s\n' "${#containers[@]}"
  printf 'image_count=%s\n' "${#image_ids[@]}"
  printf 'onepanel_network_unchanged=yes\n'
  printf 'default_route_unchanged=yes\n'
  printf 'nftables_semantics_unchanged=yes\n'
  printf 'failed_units_unchanged=yes\n'
} >"$report_dir/audit-metadata.txt"
(
  cd "$report_dir"
  sha256sum ./*.json ./audit-metadata.txt >SHA256SUMS
)
find "$audit_dir" -type f -exec chmod 0600 {} +

python3 - "$report_dir" <<'PY'
import json
import sys
from pathlib import Path

report_directory = Path(sys.argv[1])
reports = [json.loads(path.read_text(encoding="utf-8")) for path in sorted(report_directory.glob("*.json"))]
print(f"report_count={len(reports)}")
print(f"finding_count={sum(report['summary']['finding_count'] for report in reports)}")
print(f"incomplete_surface_count={sum(report['summary']['incomplete_surface_count'] for report in reports)}")
print("production_secret_audit_complete=yes")
PY
