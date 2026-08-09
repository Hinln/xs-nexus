#!/usr/bin/env bash
set -Eeuo pipefail
set +x

export PATH=/usr/sbin:/usr/bin:/sbin:/bin
export LC_ALL=C
umask 077

readonly CONTROLLER_URL='https://vpn.qinwen.co/'
readonly RELEASE_BASE_URL='https://vpn.qinwen.co/downloads/linux/stable'
readonly RELEASE_VERSION='0.1.0'
readonly RELEASE_PUBLIC_KEY_SHA256='b987e95acebaf2ff24d08bab17ae6a3cc60cc89f9805920416a3ac8cabba5253'
readonly CONFIG_PATH='/etc/xs-nexus/agent.json'
readonly STATE_PATH='/var/lib/xs-nexus/node-state.json'
readonly MAX_ARCHIVE_BYTES=268435456

temporary_directory=

fail() {
    printf 'XS Nexus installation failed: %s\n' "$1" >&2
    exit 1
}

cleanup() {
    if [[ -n ${temporary_directory:-} ]]; then
        case "$temporary_directory" in
            /tmp/xs-nexus-bootstrap.*) rm -rf -- "$temporary_directory" ;;
        esac
    fi
}
trap cleanup EXIT INT TERM HUP

install_prerequisites() {
    local -a missing=()
    local command
    for command in curl openssl tar gzip sha256sum stat flock getent useradd groupadd runuser systemctl hostname sed tr cut awk grep sort mktemp getconf install; do
        command -v "$command" >/dev/null 2>&1 || missing+=("$command")
    done
    ((${#missing[@]} > 0)) || return 0

    printf 'Installing required system packages...\n'
    if command -v apt-get >/dev/null 2>&1; then
        DEBIAN_FRONTEND=noninteractive apt-get update
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            ca-certificates curl openssl tar gzip coreutils util-linux passwd grep gawk libc-bin
    elif command -v dnf >/dev/null 2>&1; then
        dnf install -y ca-certificates curl openssl tar gzip coreutils util-linux shadow-utils grep gawk glibc-common
    elif command -v yum >/dev/null 2>&1; then
        yum install -y ca-certificates curl openssl tar gzip coreutils util-linux shadow-utils grep gawk glibc-common
    elif command -v zypper >/dev/null 2>&1; then
        zypper --non-interactive install ca-certificates curl openssl tar gzip coreutils util-linux shadow grep gawk glibc
    elif command -v pacman >/dev/null 2>&1; then
        pacman --sync --refresh --noconfirm --needed ca-certificates curl openssl tar gzip coreutils util-linux shadow grep gawk glibc
    else
        fail "missing commands (${missing[*]}) and no supported package manager was found"
    fi

    for command in curl openssl tar gzip sha256sum stat flock getent useradd groupadd runuser systemctl hostname sed tr cut awk grep sort mktemp getconf install; do
        command -v "$command" >/dev/null 2>&1 || fail "required command is unavailable after package installation: $command"
    done
}

fetch_https() {
    local url=$1 destination=$2
    [[ "$url" == https://* ]] || fail 'refusing a non-HTTPS release URL'
    curl \
        --fail \
        --silent \
        --show-error \
        --location \
        --proto '=https' \
        --proto-redir '=https' \
        --connect-timeout 10 \
        --max-time 300 \
        --retry 2 \
        --retry-delay 1 \
        --output "$destination" \
        "$url"
    [[ -f "$destination" && ! -L "$destination" ]] || fail 'download did not create a regular file'
}

verify_payload_manifest() {
    local package_root=$1 expected_payload actual_payload line
    expected_payload=$'bin/xs\nbin/xs-agent\nlib/systemd/system/xs-agent-update.path\nlib/systemd/system/xs-agent-update.service\nlib/systemd/system/xs-agent.service\nshare/doc/xs-nexus/LINUX_INSTALLATION.md\nshare/xs-nexus/agent.example.json\nshare/xs-nexus/xs-nexus-installer.sh'
    actual_payload=
    while IFS= read -r line; do
        [[ "$line" =~ ^[0-9a-f]{64}\ \ (bin/xs|bin/xs-agent|lib/systemd/system/xs-agent\.service|lib/systemd/system/xs-agent-update\.service|lib/systemd/system/xs-agent-update\.path|share/doc/xs-nexus/LINUX_INSTALLATION\.md|share/xs-nexus/agent\.example\.json|share/xs-nexus/xs-nexus-installer\.sh)$ ]] \
            || fail 'payload hash manifest is invalid'
        [[ -z "$actual_payload" ]] || actual_payload+=$'\n'
        actual_payload+=${BASH_REMATCH[1]}
    done <"$package_root/PAYLOAD.SHA256"
    [[ "$actual_payload" == "$expected_payload" ]] || fail 'payload hash manifest is incomplete or duplicated'
    (cd "$package_root" && sha256sum --strict --check PAYLOAD.SHA256 >/dev/null) \
        || fail 'payload hash verification failed'
}

verify_and_extract_archive() {
    local archive=$1 package_name=$2 extraction_directory=$3
    local expected_members actual_members
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
        "$package_name/share/xs-nexus/xs-nexus-installer.sh" | sort)
    actual_members=$(tar -tzf "$archive" | sort) || fail 'release archive cannot be listed'
    [[ "$actual_members" == "$expected_members" ]] || fail 'release archive member allowlist verification failed'
    if tar -tvzf "$archive" | cut -c1 | grep -qvE '^[-d]$'; then
        fail 'release archive contains a non-file member'
    fi
    tar --no-same-owner --no-same-permissions -xzf "$archive" -C "$extraction_directory" \
        || fail 'release archive extraction failed'
    [[ -d "$extraction_directory/$package_name" && ! -L "$extraction_directory/$package_name" ]] \
        || fail 'release package root is invalid'
    verify_payload_manifest "$extraction_directory/$package_name"
}

make_node_name() {
    local raw_name normalized
    raw_name=$(hostname --short 2>/dev/null || hostname 2>/dev/null || true)
    normalized=$(printf '%s' "$raw_name" \
        | tr -d '\r\n' \
        | sed -E 's/[^A-Za-z0-9._-]+/-/g; s/^-+//; s/-+$//' \
        | cut -c1-64)
    [[ -n "$normalized" ]] || normalized=linux-node
    printf '%s' "$normalized"
}

read_enrollment_token() {
    local token
    [[ -r /dev/tty && -w /dev/tty ]] || fail 'an interactive terminal is required to enter the Enrollment Token'
    printf 'Enrollment Token: ' >/dev/tty
    IFS= read -r -s token </dev/tty || fail 'unable to read the Enrollment Token'
    printf '\n' >/dev/tty
    [[ ${#token} -ge 16 && ${#token} -le 96 && "$token" == xsenr1_* && "$token" != *[[:space:]]* ]] \
        || fail 'the Enrollment Token format is invalid'
    printf '%s' "$token" >"$temporary_directory/enrollment.token"
    chmod 0600 "$temporary_directory/enrollment.token"
    unset token
}

[[ $(id -u) -eq 0 ]] || fail 'run this command through sudo or as root'
[[ $(uname -s) == Linux ]] || fail 'only Linux is supported'

case "$(uname -m)" in
    x86_64)
        architecture=x86_64
        target=x86_64-unknown-linux-gnu
        ;;
    aarch64|arm64)
        architecture=aarch64
        target=aarch64-unknown-linux-gnu
        ;;
    *) fail "unsupported CPU architecture: $(uname -m)" ;;
esac

install_prerequisites

[[ -d /run/systemd/system ]] || fail 'systemd must be running as PID 1'
getconf GNU_LIBC_VERSION >/dev/null 2>&1 || fail 'this release requires a glibc-based Linux distribution'
if [[ ! -c /dev/net/tun ]] && command -v modprobe >/dev/null 2>&1; then
    modprobe tun >/dev/null 2>&1 || true
fi
[[ -c /dev/net/tun && -r /dev/net/tun && -w /dev/net/tun ]] \
    || fail '/dev/net/tun is unavailable or inaccessible'

temporary_directory=$(mktemp -d /tmp/xs-nexus-bootstrap.XXXXXX)
chmod 0700 "$temporary_directory"

public_key="$temporary_directory/release-public-key.pem"
manifest_name="xs-nexus-$RELEASE_VERSION-$target.manifest"
signature_name="$manifest_name.sig"
manifest="$temporary_directory/$manifest_name"
signature="$temporary_directory/$signature_name"

printf 'Downloading and verifying XS Nexus %s for %s...\n' "$RELEASE_VERSION" "$architecture"
fetch_https "$RELEASE_BASE_URL/release-public-key.pem" "$public_key"
[[ $(stat -c '%s' "$public_key") -le 8192 ]] || fail 'release public key is oversized'
[[ $(sha256sum "$public_key" | awk '{print $1}') == "$RELEASE_PUBLIC_KEY_SHA256" ]] \
    || fail 'release public key fingerprint verification failed'
openssl pkey -pubin -in "$public_key" -noout >/dev/null 2>&1 || fail 'release public key is invalid'

fetch_https "$RELEASE_BASE_URL/$manifest_name" "$manifest"
fetch_https "$RELEASE_BASE_URL/$signature_name" "$signature"
[[ $(stat -c '%s' "$manifest") -le 4096 ]] || fail 'release manifest is oversized'
[[ $(stat -c '%s' "$signature") -eq 64 ]] || fail 'release signature length is invalid'
openssl pkeyutl -verify -rawin -pubin -inkey "$public_key" -in "$manifest" -sigfile "$signature" \
    >/dev/null 2>&1 || fail 'release signature verification failed'

mapfile -t manifest_lines <"$manifest"
[[ ${#manifest_lines[@]} -eq 12 ]] || fail 'release manifest field count is invalid'
[[ ${manifest_lines[0]} == 'schema_version=2' ]] || fail 'release manifest schema is unsupported'
[[ ${manifest_lines[1]} == 'product=xs-nexus' ]] || fail 'release manifest product is invalid'
[[ ${manifest_lines[2]} == "version=$RELEASE_VERSION" ]] || fail 'release manifest version is invalid'
[[ ${manifest_lines[3]} =~ ^source_commit=[0-9a-f]{40}$ ]] || fail 'release manifest source commit is invalid'
[[ ${manifest_lines[4]} =~ ^source_date_epoch=[0-9]+$ ]] || fail 'release manifest source date epoch is invalid'
[[ ${manifest_lines[5]} == 'protocol_version=XSP/1' ]] || fail 'release manifest protocol version is invalid'
[[ ${manifest_lines[6]} == 'platform=linux' ]] || fail 'release manifest platform is invalid'
[[ ${manifest_lines[7]} == "architecture=$architecture" ]] || fail 'release manifest architecture is invalid'
[[ ${manifest_lines[8]} == "target=$target" ]] || fail 'release manifest target is invalid'
archive_name="xs-nexus-$RELEASE_VERSION-$target.tar.gz"
[[ ${manifest_lines[9]} == "archive=$archive_name" ]] || fail 'release manifest archive name is invalid'
[[ ${manifest_lines[10]} =~ ^archive_size=([1-9][0-9]{0,9})$ ]] || fail 'release archive size is invalid'
archive_size=${BASH_REMATCH[1]}
((10#$archive_size <= MAX_ARCHIVE_BYTES)) || fail 'release archive exceeds the maximum size'
[[ ${manifest_lines[11]} =~ ^archive_sha256=([0-9a-f]{64})$ ]] || fail 'release archive hash is invalid'
archive_sha256=${BASH_REMATCH[1]}

archive="$temporary_directory/$archive_name"
fetch_https "$RELEASE_BASE_URL/$archive_name" "$archive"
[[ $(stat -c '%s' "$archive") == "$archive_size" ]] || fail 'release archive size verification failed'
[[ $(sha256sum "$archive" | awk '{print $1}') == "$archive_sha256" ]] \
    || fail 'release archive hash verification failed'

package_name="xs-nexus-$RELEASE_VERSION-$target"
verify_and_extract_archive "$archive" "$package_name" "$temporary_directory"
installer="$temporary_directory/$package_name/share/xs-nexus/xs-nexus-installer.sh"
[[ -f "$installer" && ! -L "$installer" && -x "$installer" ]] || fail 'verified installer is unavailable'

install_arguments=(
    install
    --archive "$archive"
    --manifest "$manifest"
    --signature "$signature"
    --public-key "$public_key"
)

if [[ -e "$STATE_PATH" ]]; then
    [[ -f "$STATE_PATH" && ! -L "$STATE_PATH" ]] || fail 'existing node state path is unsafe'
    printf 'Existing node identity detected; preserving enrollment and applying the verified release.\n'
else
    read_enrollment_token
    if [[ -e "$CONFIG_PATH" ]]; then
        [[ -f "$CONFIG_PATH" && ! -L "$CONFIG_PATH" ]] || fail 'existing Agent configuration path is unsafe'
        grep -Eq '"controller_url"[[:space:]]*:[[:space:]]*"https://vpn\.qinwen\.co/"' "$CONFIG_PATH" \
            || fail 'existing Agent configuration uses a different Controller URL'
    else
        node_name=$(make_node_name)
        cat >"$temporary_directory/agent.json" <<EOF
{
  "controller_url": "$CONTROLLER_URL",
  "node_name": "$node_name",
  "device_type": "linux",
  "state_directory": "/var/lib/xs-nexus",
  "runtime_directory": "/run/xs-nexus",
  "interface_name": "xsn0",
  "mtu": 1280,
  "control_sync_interval_seconds": 15,
  "update_channel": "stable",
  "update_signing_public_key_path": "/etc/xs-nexus/release-public-key.pem"
}
EOF
        chmod 0600 "$temporary_directory/agent.json"
        install_arguments+=(--config "$temporary_directory/agent.json")
    fi
    install_arguments+=(--enrollment-token-file "$temporary_directory/enrollment.token")
fi

"$installer" "${install_arguments[@]}"

status_output=
for ((status_attempt = 1; status_attempt <= 20; status_attempt++)); do
    if status_output=$(/usr/local/bin/xs status 2>&1); then
        printf '\nXS Nexus installation completed.\n'
        systemctl --no-pager --full status xs-agent.service | sed -n '1,12p' || true
        printf '%s\n' "$status_output"
        cleanup
        temporary_directory=
        trap - EXIT INT TERM HUP
        exit 0
    fi
    sleep 1
done

systemctl --no-pager --full status xs-agent.service | sed -n '1,20p' || true
[[ -z $status_output ]] || printf '%s\n' "$status_output" >&2
fail 'Agent did not become ready within 20 seconds; inspect journalctl -u xs-agent.service'
