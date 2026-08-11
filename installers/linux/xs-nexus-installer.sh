#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

INSTALLER_SCHEMA=2
MAX_UPDATE_ARCHIVE_BYTES=536870912
UNIT_NAME=xs-agent.service
UPDATE_PATH_NAME=xs-agent-update.path
install_root=/
service_manager=systemctl
archive=
manifest=
signature=
public_key=
config_source=
token_source=
rollback_version=
purge=false

usage() {
    cat >&2 <<'EOF'
Usage:
  xs-nexus-installer.sh install --archive FILE --manifest FILE --signature FILE --public-key FILE [options]
  xs-nexus-installer.sh rollback [--version VERSION] [--root DIRECTORY] [--service-manager FILE]
  xs-nexus-installer.sh uninstall [--purge] [--root DIRECTORY] [--service-manager FILE]
  xs-nexus-installer.sh status [--root DIRECTORY] [--service-manager FILE]

Install options:
  --config FILE                 Install an initial Agent configuration if none exists.
  --enrollment-token-file FILE  Enroll from a restricted token file; the token is not an argument.
  --root DIRECTORY              Install beneath an alternate absolute root (integration tests/images).
  --service-manager FILE        systemctl-compatible service manager command.
EOF
}

fail() {
    printf 'xs-nexus installer: %s\n' "$1" >&2
    exit 1
}

[[ $# -ge 1 ]] || { usage; exit 2; }
operation=$1
shift
case "$operation" in
    install|rollback|uninstall|status) ;;
    *) usage; exit 2 ;;
esac

while (($#)); do
    case "$1" in
        --archive|--manifest|--signature|--public-key|--config|--enrollment-token-file|--root|--service-manager|--version)
            (($# >= 2)) || { usage; exit 2; }
            case "$1" in
                --archive) archive=$2 ;;
                --manifest) manifest=$2 ;;
                --signature) signature=$2 ;;
                --public-key) public_key=$2 ;;
                --config) config_source=$2 ;;
                --enrollment-token-file) token_source=$2 ;;
                --root) install_root=$2 ;;
                --service-manager) service_manager=$2 ;;
                --version) rollback_version=$2 ;;
            esac
            shift 2
            ;;
        --purge)
            purge=true
            shift
            ;;
        *) usage; exit 2 ;;
    esac
done

case "$operation" in
    install)
        [[ "$purge" == false && -z "$rollback_version" ]] || { usage; exit 2; }
        ;;
    rollback)
        [[ "$purge" == false && -z "$archive$manifest$signature$public_key$config_source$token_source" ]] || { usage; exit 2; }
        ;;
    uninstall)
        [[ -z "$archive$manifest$signature$public_key$config_source$token_source$rollback_version" ]] || { usage; exit 2; }
        ;;
    status)
        [[ "$purge" == false && -z "$archive$manifest$signature$public_key$config_source$token_source$rollback_version" ]] || { usage; exit 2; }
        ;;
esac

[[ "$install_root" == /* ]] || { printf 'install root must be absolute\n' >&2; exit 2; }
mkdir -p "$install_root"
install_root=$(cd "$install_root" && pwd -P)
[[ "$install_root" != "/proc" && "$install_root" != "/sys" && "$install_root" != "/dev" ]] || fail 'unsafe install root'
if [[ $(id -u) -ne 0 && "$install_root" == "/" ]]; then
    fail 'root privileges are required for a host installation'
fi
if [[ "$service_manager" == */* ]]; then
    [[ -x "$service_manager" && ! -L "$service_manager" ]] || fail 'service manager must be an executable non-symlink file'
else
    command -v "$service_manager" >/dev/null || fail 'service manager is unavailable'
fi

root_path() {
    if [[ "$install_root" == "/" ]]; then
        printf '%s' "$1"
    else
        printf '%s%s' "$install_root" "$1"
    fi
}

lib_directory=$(root_path /usr/local/lib/xs-nexus)
versions_directory="$lib_directory/versions"
metadata_directory="$lib_directory/release-metadata"
current_link="$lib_directory/current"
previous_link="$lib_directory/previous"
cli_link=$(root_path /usr/local/bin/xs)
unit_path=$(root_path /etc/systemd/system/xs-agent.service)
update_service_path=$(root_path /etc/systemd/system/xs-agent-update.service)
update_path_path=$(root_path /etc/systemd/system/xs-agent-update.path)
config_directory=$(root_path /etc/xs-nexus)
config_path="$config_directory/agent.json"
pinned_key="$config_directory/release-public-key.pem"
revocations_file="$config_directory/release-revocations"
state_directory=$(root_path /var/lib/xs-nexus)
identity_path="$state_directory/identity.key"
node_state_path="$state_directory/node-state.json"
network_manifest_path="$state_directory/network-manifest.json"
user_marker="$state_directory/.installer-created-user"
group_marker="$state_directory/.installer-created-group"

service_call() {
    XS_NEXUS_INSTALL_ROOT="$install_root" "$service_manager" "$@"
}

service_is_active() {
    service_call is-active --quiet "$UNIT_NAME" >/dev/null 2>&1
}

acquire_lock() {
    command -v flock >/dev/null || fail 'flock is unavailable'
    lock_directory=$(root_path /run/lock)
    install -d -m 0755 "$lock_directory"
    exec 9>"$lock_directory/xs-nexus-installer.lock"
    flock -n 9 || fail 'another XS Nexus lifecycle operation is active'
}

validate_regular_file() {
    [[ -f "$1" && ! -L "$1" ]] || fail "$2 must be a regular non-symlink file"
}

canonical_u32() {
    local value=$1
    [[ $value =~ ^(0|[1-9][0-9]{0,9})$ ]] || return 1
    ((10#$value <= 4294967295))
}

canonical_version() {
    local value=$1 major minor patch extra
    IFS=. read -r major minor patch extra <<<"$value"
    [[ "$value" == "$major.$minor.$patch" && -z ${extra:-} ]] || return 1
    canonical_u32 "$major" && canonical_u32 "$minor" && canonical_u32 "$patch"
}

canonical_positive_u64() {
    local value=$1 high low
    [[ $value =~ ^[1-9][0-9]{0,19}$ ]] || return 1
    ((${#value} < 20)) && return 0
    high=${value:0:10}
    low=${value:10:10}
    ((10#$high < 1844674407 || (10#$high == 1844674407 && 10#$low <= 3709551615)))
}

parse_manifest() {
    local manifest_path=$1 platform_index
    mapfile -t release_lines <"$manifest_path"
    case ${release_lines[0]-} in
        schema_version=1)
            [[ ${#release_lines[@]} -eq 9 ]] || fail 'legacy release manifest field count is invalid'
            release_manifest_schema=1
            release_source_commit=unknown
            release_source_date_epoch=0
            release_protocol_version=XSP/1
            platform_index=3
            ;;
        "schema_version=$INSTALLER_SCHEMA")
            [[ ${#release_lines[@]} -eq 12 ]] || fail 'release manifest field count is invalid'
            release_manifest_schema=$INSTALLER_SCHEMA
            [[ ${release_lines[3]} =~ ^source_commit=([0-9a-f]{40})$ ]] || fail 'release source commit is invalid'
            release_source_commit=${BASH_REMATCH[1]}
            [[ ${release_lines[4]} =~ ^source_date_epoch=(.*)$ ]] || fail 'release source date epoch is invalid'
            release_source_date_epoch=${BASH_REMATCH[1]}
            canonical_positive_u64 "$release_source_date_epoch" || fail 'release source date epoch is invalid'
            [[ ${release_lines[5]} == 'protocol_version=XSP/1' ]] || fail 'release protocol version is invalid'
            release_protocol_version=XSP/1
            platform_index=6
            ;;
        *) fail 'release manifest schema is unsupported' ;;
    esac
    [[ ${release_lines[1]} == 'product=xs-nexus' ]] || fail 'release manifest product is invalid'
    [[ ${release_lines[2]} =~ ^version=(.*)$ ]] || fail 'release version is invalid'
    release_version=${BASH_REMATCH[1]}
    canonical_version "$release_version" || fail 'release version is invalid'
    [[ ${release_lines[$platform_index]} == 'platform=linux' ]] || fail 'release platform is invalid'
    [[ ${release_lines[$((platform_index + 1))]} =~ ^architecture=(x86_64|aarch64)$ ]] || fail 'release architecture is invalid'
    release_architecture=${BASH_REMATCH[1]}
    [[ ${release_lines[$((platform_index + 2))]} =~ ^target=(x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)$ ]] || fail 'release target is invalid'
    release_target=${BASH_REMATCH[1]}
    [[ ${release_lines[$((platform_index + 3))]} =~ ^archive=(xs-nexus-[0-9]+\.[0-9]+\.[0-9]+-(x86_64|aarch64)-unknown-linux-gnu\.tar\.gz)$ ]] || fail 'release archive name is invalid'
    release_archive=${BASH_REMATCH[1]}
    [[ ${release_lines[$((platform_index + 4))]} =~ ^archive_size=([1-9][0-9]*)$ ]] || fail 'release archive size is invalid'
    release_archive_size=${BASH_REMATCH[1]}
    if [[ $release_manifest_schema -eq $INSTALLER_SCHEMA ]]; then
        canonical_positive_u64 "$release_archive_size" || fail 'release archive size is invalid'
        ((10#$release_archive_size <= MAX_UPDATE_ARCHIVE_BYTES)) || fail 'release archive exceeds the update size limit'
    fi
    [[ ${release_lines[$((platform_index + 5))]} =~ ^archive_sha256=([0-9a-f]{64})$ ]] || fail 'release archive hash is invalid'
    release_archive_sha256=${BASH_REMATCH[1]}
    [[ "$release_archive" == "xs-nexus-$release_version-$release_target.tar.gz" ]] || fail 'release archive fields disagree'
    case "$(uname -m)" in
        x86_64) host_architecture=x86_64; host_target=x86_64-unknown-linux-gnu ;;
        aarch64|arm64) host_architecture=aarch64; host_target=aarch64-unknown-linux-gnu ;;
        *) fail 'host architecture is unsupported' ;;
    esac
    [[ "$release_architecture" == "$host_architecture" && "$release_target" == "$host_target" ]] || fail 'release does not match this host'
}

verify_release_binary_identity() {
    local release_root=$1 agent_output cli_output
    agent_output=$("$release_root/bin/xs-agent" --version) || fail 'Agent binary version command failed'
    cli_output=$("$release_root/bin/xs" --version) || fail 'CLI binary version command failed'
    if [[ $release_manifest_schema -eq 1 ]]; then
        [[ $agent_output == "xs-agent $release_version" ]] || fail 'legacy Agent binary version is invalid'
        [[ $cli_output == "xs $release_version" ]] || fail 'legacy CLI binary version is invalid'
        return
    fi
    [[ $agent_output == "xs-agent version=$release_version commit=$release_source_commit protocol=$release_protocol_version" ]] \
        || fail 'Agent binary identity does not match release provenance'
    [[ $cli_output == "xs-cli version=$release_version commit=$release_source_commit protocol=$release_protocol_version" ]] \
        || fail 'CLI binary identity does not match release provenance'
}

verify_manifest_signature() {
    local manifest_path=$1 signature_path=$2 key_path=$3
    local key_directory candidate existing permissions verified=false key_count=0
    local -a accepted_keys=()
    validate_regular_file "$manifest_path" 'release manifest'
    validate_regular_file "$signature_path" 'release signature'
    validate_regular_file "$key_path" 'release public key'
    [[ $(stat -c '%s' "$manifest_path") -le 4096 ]] || fail 'release manifest is oversized'
    [[ $(stat -c '%s' "$signature_path") -eq 64 ]] || fail 'release signature length is invalid'
    [[ $(stat -c '%s' "$key_path") -le 8192 ]] || fail 'release public key bundle is oversized'
    if [[ $install_root == / ]]; then
        [[ $(stat -c '%u' "$key_path") -eq 0 ]] || fail 'release public key bundle must be root-owned'
    fi
    permissions=$(stat -c '%a' "$key_path")
    (( (8#$permissions & 8#022) == 0 )) || fail 'release public key bundle permissions are unsafe'
    key_directory=$(mktemp -d)
    if ! awk -v directory="$key_directory" '
        BEGIN { inside = 0; count = 0 }
        /^-----BEGIN PUBLIC KEY-----$/ {
            if (inside || count == 4) exit 1
            count += 1
            inside = 1
            path = sprintf("%s/key-%d.pem", directory, count)
            print >path
            next
        }
        /^-----END PUBLIC KEY-----$/ {
            if (!inside) exit 1
            print >>path
            inside = 0
            next
        }
        {
            if (!inside || $0 !~ /^[A-Za-z0-9+\/=]+$/) exit 1
            print >>path
        }
        END { if (inside || count == 0) exit 1 }
    ' "$key_path"; then
        rm -rf -- "$key_directory"
        fail 'release public key bundle is invalid'
    fi
    for candidate in "$key_directory"/key-*.pem; do
        openssl pkey -pubin -in "$candidate" -noout >/dev/null 2>&1 || {
            rm -rf -- "$key_directory"
            fail 'release public key bundle is invalid'
        }
        for existing in "${accepted_keys[@]}"; do
            if cmp -s "$candidate" "$existing"; then
                rm -rf -- "$key_directory"
                fail 'release public key bundle contains a duplicate key'
            fi
        done
        accepted_keys+=("$candidate")
        key_count=$((key_count + 1))
        if openssl pkeyutl -verify -rawin -pubin -inkey "$candidate" \
            -in "$manifest_path" -sigfile "$signature_path" >/dev/null 2>&1; then
            verified=true
        fi
    done
    rm -rf -- "$key_directory"
    [[ $key_count -ge 1 && $key_count -le 4 && $verified == true ]] \
        || fail 'release signature verification failed'
}

verify_manifest_not_revoked() {
    local manifest_path=$1 manifest_hash line permissions previous=
    [[ -e "$revocations_file" ]] || return 0
    validate_regular_file "$revocations_file" 'release revocation ledger'
    [[ $(stat -c '%s' "$revocations_file") -ge 65 && $(stat -c '%s' "$revocations_file") -le 65536 ]] \
        || fail 'release revocation ledger size is invalid'
    if [[ $install_root == / ]]; then
        [[ $(stat -c '%u' "$revocations_file") -eq 0 ]] || fail 'release revocation ledger must be root-owned'
    fi
    permissions=$(stat -c '%a' "$revocations_file")
    (( (8#$permissions & 8#022) == 0 )) || fail 'release revocation ledger permissions are unsafe'
    [[ -z $(tail -c 1 "$revocations_file") ]] || fail 'release revocation ledger is not canonical'
    while IFS= read -r line; do
        [[ "$line" =~ ^[0-9a-f]{64}$ ]] || fail 'release revocation ledger is invalid'
        [[ -z "$previous" || "$previous" < "$line" ]] || fail 'release revocation ledger is not sorted and unique'
        previous=$line
    done <"$revocations_file"
    manifest_hash=$(sha256sum "$manifest_path" | awk '{print $1}')
    if grep -Fxq "$manifest_hash" "$revocations_file"; then
        fail 'release manifest has been revoked'
    fi
}

verify_payload_manifest() {
    local package_root=$1
    local expected_payload actual_payload line
    expected_payload=$'bin/xs\nbin/xs-agent\nlib/systemd/system/xs-agent-update.path\nlib/systemd/system/xs-agent-update.service\nlib/systemd/system/xs-agent.service\nshare/doc/xs-nexus/LINUX_INSTALLATION.md\nshare/xs-nexus/agent.example.json\nshare/xs-nexus/xs-nexus-installer.sh'
    actual_payload=
    while IFS= read -r line; do
        [[ "$line" =~ ^[0-9a-f]{64}\ \ (bin/xs|bin/xs-agent|lib/systemd/system/xs-agent\.service|lib/systemd/system/xs-agent-update\.service|lib/systemd/system/xs-agent-update\.path|share/doc/xs-nexus/LINUX_INSTALLATION\.md|share/xs-nexus/agent\.example\.json|share/xs-nexus/xs-nexus-installer\.sh)$ ]] || fail 'payload hash manifest is invalid'
        if [[ -n "$actual_payload" ]]; then
            actual_payload+=$'\n'
        fi
        actual_payload+=${BASH_REMATCH[1]}
    done <"$package_root/PAYLOAD.SHA256"
    [[ "$actual_payload" == "$expected_payload" ]] || fail 'payload hash manifest is incomplete or duplicated'
    (cd "$package_root" && sha256sum --strict --check PAYLOAD.SHA256 >/dev/null) || fail 'payload hash verification failed'
}

verify_archive() {
    validate_regular_file "$archive" 'release archive'
    [[ $(basename "$archive") == "$release_archive" ]] || fail 'release archive path does not match manifest'
    [[ $(stat -c '%s' "$archive") == "$release_archive_size" ]] || fail 'release archive size verification failed'
    [[ $(sha256sum "$archive" | awk '{print $1}') == "$release_archive_sha256" ]] || fail 'release archive hash verification failed'

    package_name="xs-nexus-$release_version-$release_target"
    expected_members=$(printf '%s\n' \
        "$package_name/" \
        "$package_name/PAYLOAD.SHA256" \
        "$package_name/bin/" \
        "$package_name/bin/xs" \
        "$package_name/bin/xs-agent" \
        "$package_name/lib/" \
        "$package_name/lib/systemd/" \
        "$package_name/lib/systemd/system/" \
        "$package_name/lib/systemd/system/xs-agent-update.path" \
        "$package_name/lib/systemd/system/xs-agent-update.service" \
        "$package_name/lib/systemd/system/xs-agent.service" \
        "$package_name/share/" \
        "$package_name/share/doc/" \
        "$package_name/share/doc/xs-nexus/" \
        "$package_name/share/doc/xs-nexus/LINUX_INSTALLATION.md" \
        "$package_name/share/xs-nexus/" \
        "$package_name/share/xs-nexus/agent.example.json" \
        "$package_name/share/xs-nexus/xs-nexus-installer.sh" | LC_ALL=C sort)
    actual_members=$(tar -tzf "$archive" | LC_ALL=C sort) || fail 'release archive cannot be listed'
    [[ "$actual_members" == "$expected_members" ]] || fail 'release archive member allowlist verification failed'
    if tar -tvzf "$archive" | cut -c1 | grep -qvE '^[-d]$'; then
        fail 'release archive contains a non-file member'
    fi

    extraction_directory=$(mktemp -d)
    tar --no-same-owner --no-same-permissions -xzf "$archive" -C "$extraction_directory" || fail 'release archive extraction failed'
    extracted_package="$extraction_directory/$package_name"
    verify_payload_manifest "$extracted_package"
    verify_release_binary_identity "$extracted_package"
}

version_less_than() {
    local left=$1 right=$2 left_major left_minor left_patch right_major right_minor right_patch
    IFS=. read -r left_major left_minor left_patch <<<"$left"
    IFS=. read -r right_major right_minor right_patch <<<"$right"
    ((10#$left_major < 10#$right_major)) && return 0
    ((10#$left_major > 10#$right_major)) && return 1
    ((10#$left_minor < 10#$right_minor)) && return 0
    ((10#$left_minor > 10#$right_minor)) && return 1
    ((10#$left_patch < 10#$right_patch))
}

current_release() {
    local target
    [[ -L "$current_link" ]] || return 1
    target=$(readlink "$current_link")
    [[ "$target" =~ ^versions/([0-9]+\.[0-9]+\.[0-9]+-(x86_64|aarch64)-unknown-linux-gnu)$ ]] || fail 'current release link is invalid'
    printf '%s' "${BASH_REMATCH[1]}"
}

atomic_release_link() {
    local link_path=$1 release=$2 temporary_link="$lib_directory/.link.$RANDOM.$$"
    ln -s "versions/$release" "$temporary_link"
    mv -Tf "$temporary_link" "$link_path"
}

prepare_account_and_directories() {
    local user_created=false group_created=false
    if [[ "$install_root" == "/" ]]; then
        if ! getent group xs-nexus >/dev/null; then
            groupadd --system xs-nexus
            group_created=true
        fi
        if ! id -u xs-nexus >/dev/null 2>&1; then
            useradd --system --gid xs-nexus --home-dir /var/lib/xs-nexus --shell /usr/sbin/nologin xs-nexus
            user_created=true
        fi
        account_record=$(getent passwd xs-nexus) || fail 'xs-nexus account lookup failed'
        IFS=: read -r _ _ account_id account_group _ account_home account_shell <<<"$account_record"
        expected_group=$(getent group xs-nexus | cut -d: -f3)
        [[ "$account_id" != 0 && "$account_group" == "$expected_group" && "$account_home" == /var/lib/xs-nexus ]] || fail 'existing xs-nexus account is unsafe'
        [[ "$account_shell" == /usr/sbin/nologin || "$account_shell" == /bin/false ]] || fail 'existing xs-nexus account has an interactive shell'
        install -d -m 0700 -o xs-nexus -g xs-nexus "$state_directory"
        install -d -m 0750 -o root -g xs-nexus "$config_directory"
        [[ "$user_created" == false ]] || install -m 0600 -o root -g root /dev/null "$user_marker"
        [[ "$group_created" == false ]] || install -m 0600 -o root -g root /dev/null "$group_marker"
    else
        install -d -m 0700 "$state_directory"
        install -d -m 0750 "$config_directory"
    fi
    install -d -m 0755 "$versions_directory" "$metadata_directory" "$(dirname "$cli_link")" "$(dirname "$unit_path")"
}

install_release_units() {
    local release_root=$1
    install -m 0644 "$release_root/lib/systemd/system/xs-agent.service" "$unit_path"
    install -m 0644 "$release_root/lib/systemd/system/xs-agent-update.service" "$update_service_path"
    install -m 0644 "$release_root/lib/systemd/system/xs-agent-update.path" "$update_path_path"
}

backup_release_units() {
    local backup_directory=$1
    install -d -m 0700 "$backup_directory"
    [[ ! -f "$unit_path" ]] || cp "$unit_path" "$backup_directory/xs-agent.service"
    [[ ! -f "$update_service_path" ]] || cp "$update_service_path" "$backup_directory/xs-agent-update.service"
    [[ ! -f "$update_path_path" ]] || cp "$update_path_path" "$backup_directory/xs-agent-update.path"
}

restore_release_units() {
    local backup_directory=$1 source_name destination
    for source_name in xs-agent.service xs-agent-update.service xs-agent-update.path; do
        case "$source_name" in
            xs-agent.service) destination=$unit_path ;;
            xs-agent-update.service) destination=$update_service_path ;;
            xs-agent-update.path) destination=$update_path_path ;;
        esac
        if [[ -f "$backup_directory/$source_name" ]]; then
            install -m 0644 "$backup_directory/$source_name" "$destination"
        else
            rm -f "$destination"
        fi
    done
    if [[ ! -f "$backup_directory/xs-agent-update.path" ]]; then
        service_call disable "$UPDATE_PATH_NAME" >/dev/null 2>&1 || true
    fi
}

verify_installed_release() {
    local release=$1
    local release_manifest="$metadata_directory/$release.manifest"
    local release_signature="$metadata_directory/$release.manifest.sig"
    local release_root="$versions_directory/$release"
    local saved_release_version=${release_version-}
    local saved_release_manifest_schema=${release_manifest_schema-}
    local saved_release_source_commit=${release_source_commit-}
    local saved_release_source_date_epoch=${release_source_date_epoch-}
    local saved_release_protocol_version=${release_protocol_version-}
    local saved_release_architecture=${release_architecture-}
    local saved_release_target=${release_target-}
    local saved_release_archive=${release_archive-}
    local saved_release_archive_size=${release_archive_size-}
    local saved_release_archive_sha256=${release_archive_sha256-}
    local saved_host_architecture=${host_architecture-}
    local saved_host_target=${host_target-}
    verify_manifest_signature "$release_manifest" "$release_signature" "$pinned_key"
    parse_manifest "$release_manifest"
    [[ "$release" == "$release_version-$release_target" ]] || fail 'installed release metadata does not match directory'
    [[ -d "$release_root" && ! -L "$release_root" ]] || fail 'installed release directory is invalid'
    verify_payload_manifest "$release_root"
    verify_release_binary_identity "$release_root"
    release_version=$saved_release_version
    release_manifest_schema=$saved_release_manifest_schema
    release_source_commit=$saved_release_source_commit
    release_source_date_epoch=$saved_release_source_date_epoch
    release_protocol_version=$saved_release_protocol_version
    release_architecture=$saved_release_architecture
    release_target=$saved_release_target
    release_archive=$saved_release_archive
    release_archive_size=$saved_release_archive_size
    release_archive_sha256=$saved_release_archive_sha256
    host_architecture=$saved_host_architecture
    host_target=$saved_host_target
}

restore_activation() {
    local old_release=$1 old_units_backup=$2 was_active=$3 created_release=$4 created_pin=$5 created_config=$6 identity_existed=$7 state_existed=$8
    service_call stop "$UNIT_NAME" >/dev/null 2>&1 || true
    if [[ -n "$old_release" ]]; then
        atomic_release_link "$current_link" "$old_release"
    else
        rm -f "$current_link" "$cli_link"
        service_call disable "$UNIT_NAME" >/dev/null 2>&1 || true
    fi
    restore_release_units "$old_units_backup"
    service_call daemon-reload >/dev/null 2>&1 || true
    if [[ "$was_active" == true && -n "$old_release" ]]; then
        service_call start "$UNIT_NAME" >/dev/null 2>&1 || printf 'warning: previous Agent service could not be restarted\n' >&2
    fi
    if [[ "$created_release" == true ]]; then
        rm -rf "${versions_directory:?}/${release_name:?}"
        rm -f "$metadata_directory/$release_name.manifest" "$metadata_directory/$release_name.manifest.sig"
    fi
    [[ "$created_pin" == false ]] || rm -f "$pinned_key"
    [[ "$created_config" == false ]] || rm -f "$config_path"
    [[ "$identity_existed" == true ]] || rm -f "$identity_path"
    [[ "$state_existed" == true ]] || rm -f "$node_state_path"
}

install_release() {
    [[ -n "$archive" && -n "$manifest" && -n "$signature" && -n "$public_key" ]] || { usage; exit 2; }
    for command in openssl sha256sum stat tar; do
        command -v "$command" >/dev/null || fail "required command is unavailable: $command"
    done
    acquire_lock
    temporary=$(mktemp -d)
    token_copy=
    staging_root=
    pending_release_root=
    pending_manifest=
    pending_signature=
    cleanup_install_temporary() {
        rm -rf -- "$temporary"
        [[ -z $token_copy ]] || rm -f -- "$token_copy"
        if [[ -n $staging_root && -d $staging_root && ! -L $staging_root ]]; then
            case "$staging_root" in
                "$versions_directory"/.staging-*) rm -rf -- "$staging_root" ;;
            esac
        fi
        if [[ -n $pending_release_root && -d $pending_release_root && ! -L $pending_release_root ]]; then
            case "$pending_release_root" in
                "$versions_directory"/*) rm -rf -- "$pending_release_root" ;;
            esac
        fi
        [[ -z $pending_manifest ]] || rm -f -- "$pending_manifest"
        [[ -z $pending_signature ]] || rm -f -- "$pending_signature"
    }
    trap cleanup_install_temporary EXIT INT TERM
    verify_manifest_signature "$manifest" "$signature" "$public_key"
    verify_manifest_not_revoked "$manifest"
    parse_manifest "$manifest"
    [[ $release_manifest_schema -eq $INSTALLER_SCHEMA ]] || fail 'external release manifest schema is obsolete'
    verify_archive
    if [[ -e "$cli_link" && ! -L "$cli_link" ]]; then
        fail 'CLI destination exists and is not a symlink'
    fi
    for systemd_path in "$unit_path" "$update_service_path" "$update_path_path"; do
        if [[ -e "$systemd_path" && ! -f "$systemd_path" ]]; then
            fail 'systemd unit destination exists and is not a regular file'
        fi
    done
    if [[ -n "$config_source" ]]; then
        validate_regular_file "$config_source" 'Agent configuration'
        if [[ -f "$config_path" ]]; then
            cmp -s "$config_source" "$config_path" || fail 'existing Agent configuration differs; refusing replacement'
        fi
    fi
    if [[ -n "$token_source" ]]; then
        validate_regular_file "$token_source" 'enrollment token'
        [[ -n "$config_source" || -f "$config_path" ]] || fail 'enrollment requires an installed Agent configuration'
        [[ ! -e "$node_state_path" ]] || fail 'refusing enrollment over existing node state'
    fi
    prepare_account_and_directories

    created_pin=false
    if [[ -f "$pinned_key" ]]; then
        cmp -s "$pinned_key" "$public_key" || fail 'release public key does not match the pinned key'
    else
        install -m 0644 "$public_key" "$pinned_key"
        created_pin=true
    fi

    old_release=$(current_release || true)
    if [[ -n "$old_release" ]]; then
        verify_installed_release "$old_release"
        old_version=${old_release%%-*}
        version_less_than "$release_version" "$old_version" && fail 'external package downgrade is forbidden; use rollback for an installed release'
    fi
    release_name="$release_version-$release_target"
    release_root="$versions_directory/$release_name"
    created_release=false
    if [[ -e "$release_root" ]]; then
        [[ -d "$release_root" && ! -L "$release_root" ]] || fail 'release destination is unsafe'
        cmp -s "$manifest" "$metadata_directory/$release_name.manifest" || fail 'same-version release metadata differs'
        cmp -s "$signature" "$metadata_directory/$release_name.manifest.sig" || fail 'same-version release signature differs'
        verify_installed_release "$release_name"
    else
        staging_root="$versions_directory/.staging-$release_name-$RANDOM-$$"
        install -d -m 0755 "$staging_root"
        cp -a "$extracted_package/." "$staging_root/"
        find "$staging_root" -type d -exec chmod 0755 {} +
        chmod 0755 \
            "$staging_root/bin/xs" \
            "$staging_root/bin/xs-agent" \
            "$staging_root/share/xs-nexus/xs-nexus-installer.sh"
        find "$staging_root" -type f \
            ! -path '*/bin/xs' \
            ! -path '*/bin/xs-agent' \
            ! -path '*/share/xs-nexus/xs-nexus-installer.sh' \
            -exec chmod 0644 {} +
        mv "$staging_root" "$release_root"
        staging_root=
        pending_release_root=$release_root
        pending_manifest="$metadata_directory/$release_name.manifest"
        pending_signature="$metadata_directory/$release_name.manifest.sig"
        install -m 0644 "$manifest" "$pending_manifest"
        install -m 0644 "$signature" "$pending_signature"
        created_release=true
    fi

    created_config=false
    if [[ -n "$config_source" ]]; then
        if [[ -f "$config_path" ]]; then
            :
        else
            if [[ "$install_root" == "/" ]]; then
                install -m 0640 -o root -g xs-nexus "$config_source" "$config_path"
            else
                install -m 0640 "$config_source" "$config_path"
            fi
            created_config=true
        fi
    fi
    if [[ -n "$token_source" ]]; then
        [[ -f "$config_path" ]] || fail 'enrollment requires an installed Agent configuration'
    fi

    old_units_backup="$temporary/old-units"
    backup_release_units "$old_units_backup"
    was_active=false
    service_is_active && was_active=true
    identity_existed=false
    state_existed=false
    [[ -e "$identity_path" ]] && identity_existed=true
    [[ -e "$node_state_path" ]] && state_existed=true
    if [[ "$was_active" == true ]]; then
        service_call stop "$UNIT_NAME" || fail 'could not stop the current Agent service'
    fi

    activation_failed=false
    atomic_release_link "$current_link" "$release_name" || activation_failed=true
    if [[ "$activation_failed" == false ]]; then
        ln -sfn ../lib/xs-nexus/current/bin/xs "$cli_link" || activation_failed=true
    fi
    if [[ "$activation_failed" == false ]]; then
        install_release_units "$release_root" || activation_failed=true
    fi
    if [[ "$activation_failed" == false ]]; then
        service_call daemon-reload || activation_failed=true
    fi
    if [[ "$activation_failed" == false && -n "$token_source" ]]; then
        token_copy=$(mktemp "$state_directory/.enrollment-token.XXXXXX") || activation_failed=true
        if [[ "$install_root" == "/" ]]; then
            if [[ "$activation_failed" == false ]]; then
                install -m 0600 -o xs-nexus -g xs-nexus "$token_source" "$token_copy" || activation_failed=true
            fi
            if [[ "$activation_failed" == false ]]; then
                runuser -u xs-nexus -- "$release_root/bin/xs-agent" enroll --config "$config_path" --token-file "$token_copy" >/dev/null || activation_failed=true
            fi
        else
            if [[ "$activation_failed" == false ]]; then
                install -m 0600 "$token_source" "$token_copy" || activation_failed=true
            fi
            if [[ "$activation_failed" == false ]]; then
                "$release_root/bin/xs-agent" enroll --config "$config_path" --token-file "$token_copy" >/dev/null || activation_failed=true
            fi
        fi
        rm -f "$token_copy"
        token_copy=
    fi
    if [[ "$activation_failed" == false ]]; then
        service_call enable "$UNIT_NAME" || activation_failed=true
    fi
    if [[ "$activation_failed" == false ]]; then
        service_call enable "$UPDATE_PATH_NAME" || activation_failed=true
    fi
    should_start=false
    if [[ "$was_active" == true || ( -f "$config_path" && -f "$node_state_path" ) ]]; then
        should_start=true
    fi
    if [[ "$activation_failed" == false && "$should_start" == true ]]; then
        service_call start "$UNIT_NAME" || activation_failed=true
        service_is_active || activation_failed=true
    fi
    if [[ "$activation_failed" == false ]]; then
        service_call start "$UPDATE_PATH_NAME" || activation_failed=true
    fi
    if [[ "$activation_failed" == true ]]; then
        restore_activation "$old_release" "$old_units_backup" "$was_active" "$created_release" "$created_pin" "$created_config" "$identity_existed" "$state_existed"
        fail 'release activation failed and the previous release was restored'
    fi
    if [[ -n "$old_release" && "$old_release" != "$release_name" ]]; then
        atomic_release_link "$previous_link" "$old_release"
    fi
    pending_release_root=
    pending_manifest=
    pending_signature=
    printf 'xs-nexus release %s installed\n' "$release_name"
}

rollback_release() {
    acquire_lock
    validate_regular_file "$pinned_key" 'pinned release public key'
    old_release=$(current_release || true)
    [[ -n "$old_release" ]] || fail 'no current release is installed'
    if [[ -n "$rollback_version" ]]; then
        canonical_version "$rollback_version" || fail 'rollback version is invalid'
        case "$(uname -m)" in
            x86_64) target=x86_64-unknown-linux-gnu ;;
            aarch64|arm64) target=aarch64-unknown-linux-gnu ;;
            *) fail 'host architecture is unsupported' ;;
        esac
        release_name="$rollback_version-$target"
    else
        [[ -L "$previous_link" ]] || fail 'no previous release is recorded'
        previous_target=$(readlink "$previous_link")
        [[ "$previous_target" =~ ^versions/([0-9]+\.[0-9]+\.[0-9]+-(x86_64|aarch64)-unknown-linux-gnu)$ ]] || fail 'previous release link is invalid'
        release_name=${BASH_REMATCH[1]}
    fi
    [[ "$release_name" != "$old_release" ]] || fail 'requested rollback release is already current'
    verify_manifest_not_revoked "$metadata_directory/$release_name.manifest"
    verify_installed_release "$old_release"
    verify_installed_release "$release_name"
    was_active=false
    service_is_active && was_active=true
    old_units_backup=$(mktemp -d)
    trap 'rm -rf "$old_units_backup"' EXIT INT TERM
    backup_release_units "$old_units_backup"
    [[ "$was_active" == false ]] || service_call stop "$UNIT_NAME" || fail 'could not stop the current Agent service'
    atomic_release_link "$current_link" "$release_name"
    install_release_units "$versions_directory/$release_name"
    service_call daemon-reload
    rollback_failed=false
    service_call enable "$UPDATE_PATH_NAME" || rollback_failed=true
    service_call start "$UPDATE_PATH_NAME" || rollback_failed=true
    if [[ "$was_active" == true ]]; then
        service_call start "$UNIT_NAME" || rollback_failed=true
        service_is_active || rollback_failed=true
    fi
    if [[ "$rollback_failed" == true ]]; then
        service_call stop "$UNIT_NAME" >/dev/null 2>&1 || true
        atomic_release_link "$current_link" "$old_release"
        restore_release_units "$old_units_backup"
        service_call daemon-reload >/dev/null 2>&1 || true
        [[ "$was_active" == false ]] || service_call start "$UNIT_NAME" >/dev/null 2>&1 || true
        fail 'rollback activation failed; the original release was restored'
    fi
    atomic_release_link "$previous_link" "$old_release"
    printf 'xs-nexus rolled back to %s\n' "$release_name"
}

uninstall_release() {
    acquire_lock
    old_release=$(current_release || true)
    if [[ -n "$old_release" ]]; then
        validate_regular_file "$pinned_key" 'pinned release public key'
        verify_installed_release "$old_release"
    fi
    was_active=false
    service_is_active && was_active=true
    [[ "$was_active" == false ]] || service_call stop "$UNIT_NAME" || fail 'could not stop the Agent service'
    cleanup_failed=false
    if [[ -n "$old_release" && -f "$config_path" && -f "$identity_path" && -f "$node_state_path" ]]; then
        "$current_link/bin/xs-agent" cleanup --config "$config_path" >/dev/null || cleanup_failed=true
    elif [[ -f "$network_manifest_path" ]]; then
        cleanup_failed=true
    fi
    [[ ! -e "$network_manifest_path" ]] || cleanup_failed=true
    if [[ "$cleanup_failed" == true ]]; then
        [[ "$was_active" == false ]] || service_call start "$UNIT_NAME" >/dev/null 2>&1 || true
        fail 'trusted network cleanup failed; uninstall was aborted'
    fi

    managed_user=false
    managed_group=false
    [[ -f "$user_marker" ]] && managed_user=true
    [[ -f "$group_marker" ]] && managed_group=true
    service_call stop "$UPDATE_PATH_NAME" >/dev/null 2>&1 || true
    service_call disable "$UPDATE_PATH_NAME" >/dev/null 2>&1 || true
    service_call disable "$UNIT_NAME" >/dev/null 2>&1 || true
    rm -f \
        "$unit_path" \
        "$update_service_path" \
        "$update_path_path" \
        "$cli_link" \
        "$current_link" \
        "$previous_link"
    service_call daemon-reload >/dev/null 2>&1 || true
    if [[ -d "$lib_directory" && ! -L "$lib_directory" ]]; then
        rm -rf "$lib_directory"
    fi
    if [[ "$purge" == true ]]; then
        if [[ -d "$config_directory" && ! -L "$config_directory" ]]; then
            rm -rf "$config_directory"
        fi
        if [[ -d "$state_directory" && ! -L "$state_directory" ]]; then
            rm -rf "$state_directory"
        fi
        if [[ "$install_root" == "/" && "$managed_user" == true ]] && id -u xs-nexus >/dev/null 2>&1; then
            userdel xs-nexus
        fi
        if [[ "$install_root" == "/" && "$managed_group" == true ]] && getent group xs-nexus >/dev/null; then
            groupdel xs-nexus
        fi
    fi
    printf 'xs-nexus uninstalled (purge=%s)\n' "$purge"
}

show_status() {
    release=$(current_release || true)
    active=false
    service_is_active && active=true
    printf 'release=%s\nservice_active=%s\nconfig_present=%s\nidentity_present=%s\nstate_present=%s\n' \
        "${release:-not-installed}" \
        "$active" \
        "$([[ -f "$config_path" ]] && printf true || printf false)" \
        "$([[ -f "$identity_path" ]] && printf true || printf false)" \
        "$([[ -f "$node_state_path" ]] && printf true || printf false)"
}

case "$operation" in
    install) install_release ;;
    rollback) rollback_release ;;
    uninstall) uninstall_release ;;
    status) show_status ;;
esac
