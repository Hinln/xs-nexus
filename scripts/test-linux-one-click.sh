#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BOOTSTRAP="$ROOT_DIR/installers/linux/xs-nexus-one-click.sh"

for command in bash curl openssl python3 script sha256sum stat tar; do
    command -v "$command" >/dev/null || { printf 'required command is unavailable: %s\n' "$command" >&2; exit 2; }
done
[[ $(id -u) -eq 0 ]] || { printf 'one-click integration test requires root\n' >&2; exit 2; }

case "$(uname -m)" in
    x86_64) target=x86_64-unknown-linux-gnu ;;
    aarch64|arm64) target=aarch64-unknown-linux-gnu ;;
    *) printf 'unsupported test architecture\n' >&2; exit 2 ;;
esac
release_commit=$(git -c "safe.directory=$ROOT_DIR" -C "$ROOT_DIR" rev-parse HEAD)
release_epoch=$(git -C "$ROOT_DIR" show -s --format=%ct HEAD)

temporary=$(mktemp -d)
server_pid=
cleanup() {
    local status=$?
    if [[ -n $server_pid ]]; then
        kill "$server_pid" >/dev/null 2>&1 || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf -- "$temporary"
    exit "$status"
}
trap cleanup EXIT INT TERM

release_directory="$temporary/releases"
package_name="xs-nexus-0.1.0-$target"
package_root="$temporary/package/$package_name"
mkdir -p \
    "$release_directory" \
    "$package_root/bin" \
    "$package_root/lib/systemd/system" \
    "$package_root/share/doc/xs-nexus" \
    "$package_root/share/xs-nexus"

test_token=$(printf '%s' 'xsenr1_bootstrap-test-token-0123456789')
installer_log="$temporary/installer-arguments"
installed_config="$temporary/installed-agent.json"
state_path="$temporary/node-state.json"
config_path="$temporary/existing-agent.json"

cat >"$package_root/share/xs-nexus/xs-nexus-installer.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
output=${XS_BOOTSTRAP_TEST_OUTPUT:?}
state_path=${XS_BOOTSTRAP_TEST_STATE:?}
expected_token=$(printf '%s' 'xsenr1_bootstrap-test-token-0123456789')
config=
token_file=
printf '%s\n' "$@" >"$output"
while (($#)); do
    case "$1" in
        --config) config=$2; shift 2 ;;
        --enrollment-token-file) token_file=$2; shift 2 ;;
        *) shift ;;
    esac
done
if [[ -n $token_file ]]; then
    [[ -f $token_file && ! -L $token_file && $(stat -c '%a' "$token_file") == 600 ]]
    [[ $(<"$token_file") == "$expected_token" ]]
fi
if [[ -n $config ]]; then
    cp -- "$config" "${XS_BOOTSTRAP_TEST_CONFIG:?}"
fi
touch "$state_path"
printf 'fixture installer completed\n'
EOF
chmod 0755 "$package_root/share/xs-nexus/xs-nexus-installer.sh"

cat >"$package_root/bin/xs-agent" <<EOF
#!/usr/bin/env bash
printf 'xs-agent version=0.1.0 commit=$release_commit protocol=XSP/1\n'
EOF
cat >"$package_root/bin/xs" <<EOF
#!/usr/bin/env bash
printf 'xs-cli version=0.1.0 commit=$release_commit protocol=XSP/1\n'
EOF
chmod 0755 "$package_root/bin/xs-agent" "$package_root/bin/xs"
printf '%s\n' '[Unit]' >"$package_root/lib/systemd/system/xs-agent.service"
printf '%s\n' '[Unit]' >"$package_root/lib/systemd/system/xs-agent-update.service"
printf '%s\n' '[Path]' >"$package_root/lib/systemd/system/xs-agent-update.path"
printf '%s\n' '# test documentation' >"$package_root/share/doc/xs-nexus/LINUX_INSTALLATION.md"
printf '%s\n' '{}' >"$package_root/share/xs-nexus/agent.example.json"
(
    cd "$package_root"
    LC_ALL=C find bin lib share -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum >PAYLOAD.SHA256
)

archive="$release_directory/$package_name.tar.gz"
(
    cd "$temporary/package"
    tar -czf "$archive" "$package_name"
)
archive_size=$(stat -c '%s' "$archive")
archive_sha256=$(sha256sum "$archive" | awk '{print $1}')
manifest="$release_directory/$package_name.manifest"
cat >"$manifest" <<EOF
schema_version=2
product=xs-nexus
version=0.1.0
source_commit=$release_commit
source_date_epoch=$release_epoch
protocol_version=XSP/1
platform=linux
architecture=${target%%-*}
target=$target
archive=$package_name.tar.gz
archive_size=$archive_size
archive_sha256=$archive_sha256
EOF

private_key="$temporary/release-private-key.pem"
public_key="$release_directory/release-public-key.pem"
signing_public_key="$temporary/release-signing-public-key.pem"
old_private_key="$temporary/release-old-private-key.pem"
old_public_key="$temporary/release-old-public-key.pem"
openssl genpkey -algorithm ED25519 -out "$private_key" >/dev/null 2>&1
openssl pkey -in "$private_key" -pubout -out "$signing_public_key" >/dev/null 2>&1
openssl genpkey -algorithm ED25519 -out "$old_private_key" >/dev/null 2>&1
openssl pkey -in "$old_private_key" -pubout -out "$old_public_key" >/dev/null 2>&1
cat "$old_public_key" "$signing_public_key" >"$public_key"
openssl pkeyutl -sign -rawin -inkey "$private_key" -in "$manifest" -out "$manifest.sig"
public_key_sha256=$(sha256sum "$public_key" | awk '{print $1}')

certificate="$temporary/server-certificate.pem"
certificate_key="$temporary/server-key.pem"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
    -subj '/CN=127.0.0.1' \
    -addext 'subjectAltName=IP:127.0.0.1' \
    -keyout "$certificate_key" -out "$certificate" >/dev/null 2>&1

cat >"$temporary/https-server.py" <<'PY'
import http.server
import ssl
import sys
from pathlib import Path

directory, certificate, key, port_file = sys.argv[1:]
handler = lambda *args, **kwargs: http.server.SimpleHTTPRequestHandler(
    *args, directory=directory, **kwargs
)
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(certificate, key)
server.socket = context.wrap_socket(server.socket, server_side=True)
Path(port_file).write_text(str(server.server_port), encoding="ascii")
server.serve_forever()
PY
port_file="$temporary/server-port"
python3 "$temporary/https-server.py" "$release_directory" "$certificate" "$certificate_key" "$port_file" \
    >"$temporary/server.log" 2>&1 &
server_pid=$!
for _attempt in $(seq 1 50); do
    [[ -s $port_file ]] && break
    sleep 0.1
done
[[ -s $port_file ]] || { printf 'HTTPS fixture server did not start\n' >&2; exit 1; }
port=$(<"$port_file")

status_attempts="$temporary/status-attempts"
status_command="$temporary/status-command"
cat >"$status_command" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
attempts_file=${XS_BOOTSTRAP_TEST_STATUS_ATTEMPTS:?}
attempt=0
[[ ! -f $attempts_file ]] || attempt=$(<"$attempts_file")
attempt=$((attempt + 1))
printf '%s\n' "$attempt" >"$attempts_file"
if ((attempt < 3)); then
    printf 'xs error=cli_ipc_failed\n' >&2
    exit 1
fi
printf 'fixture_status=ready\n'
EOF
chmod 0755 "$status_command"
export XS_BOOTSTRAP_TEST_STATUS_ATTEMPTS="$status_attempts"

test_bootstrap="$temporary/xs-nexus-one-click.sh"
cp -- "$BOOTSTRAP" "$test_bootstrap"
sed -i \
    -e "s|https://vpn.qinwen.co/downloads/linux/stable|https://127.0.0.1:$port|" \
    -e "s|b987e95acebaf2ff24d08bab17ae6a3cc60cc89f9805920416a3ac8cabba5253|$public_key_sha256|" \
    -e "s|/etc/xs-nexus/agent.json|$config_path|" \
    -e "s|/var/lib/xs-nexus/node-state.json|$state_path|" \
    -e "s#systemctl --no-pager --full status xs-agent.service | sed -n '1,12p' || true#true#" \
    -e "s|/usr/local/bin/xs status|$status_command|" \
    "$test_bootstrap"
chmod 0755 "$test_bootstrap"

capture="$temporary/bootstrap-output"
tty_stdout="$temporary/bootstrap-tty-stdout"
if ! {
    sleep 1
    printf '%s\n' "$test_token"
} | env \
        CURL_CA_BUNDLE="$certificate" \
        XS_BOOTSTRAP_TEST_OUTPUT="$installer_log" \
        XS_BOOTSTRAP_TEST_STATE="$state_path" \
        XS_BOOTSTRAP_TEST_CONFIG="$installed_config" \
        script -qefc "bash '$test_bootstrap'" "$capture" >"$tty_stdout" 2>&1; then
    cat "$tty_stdout" >&2
    [[ ! -f $capture ]] || cat "$capture" >&2
    exit 1
fi

[[ -f $state_path && -f $installed_config && -f $installer_log ]]
[[ $(<"$status_attempts") == 3 ]]
grep -F 'fixture_status=ready' "$capture" >/dev/null
if grep -F 'xs error=cli_ipc_failed' "$capture" "$tty_stdout" >/dev/null; then
    printf 'transient Agent readiness failure leaked into bootstrap output\n' >&2
    exit 1
fi
grep -F '"controller_url": "https://vpn.qinwen.co/"' "$installed_config" >/dev/null
grep -F '"update_signing_public_key_path": "/etc/xs-nexus/release-public-key.pem"' "$installed_config" >/dev/null
grep -Fx -- '--enrollment-token-file' "$installer_log" >/dev/null
if grep -F "$test_token" "$capture" "$tty_stdout" "$installer_log" "$installed_config" >/dev/null; then
    printf 'Enrollment Token leaked into bootstrap output or non-token fixture files\n' >&2
    exit 1
fi

CURL_CA_BUNDLE="$certificate" \
XS_BOOTSTRAP_TEST_OUTPUT="$installer_log" \
XS_BOOTSTRAP_TEST_STATE="$state_path" \
XS_BOOTSTRAP_TEST_CONFIG="$installed_config" \
    bash "$test_bootstrap" >"$temporary/reinstall-output" 2>&1
if grep -F "$test_token" "$temporary/reinstall-output" "$installer_log" >/dev/null; then
    printf 'Enrollment Token leaked during repeat installation\n' >&2
    exit 1
fi

cp -- "$archive" "$temporary/archive-backup"
printf 'x' | dd of="$archive" bs=1 seek=100 conv=notrunc status=none
if CURL_CA_BUNDLE="$certificate" bash "$test_bootstrap" >"$temporary/tampered-archive-output" 2>&1; then
    printf 'tampered release archive unexpectedly passed bootstrap verification\n' >&2
    exit 1
fi
grep -F 'release archive hash verification failed' "$temporary/tampered-archive-output" >/dev/null
cp -- "$temporary/archive-backup" "$archive"

cp -- "$manifest.sig" "$temporary/signature-backup"
printf 'x' | dd of="$manifest.sig" bs=1 seek=0 conv=notrunc status=none
if CURL_CA_BUNDLE="$certificate" bash "$test_bootstrap" >"$temporary/tampered-signature-output" 2>&1; then
    printf 'tampered release signature unexpectedly passed bootstrap verification\n' >&2
    exit 1
fi
grep -F 'release signature verification failed' "$temporary/tampered-signature-output" >/dev/null
cp -- "$temporary/signature-backup" "$manifest.sig"

cp -- "$public_key" "$temporary/public-key-backup"
openssl genpkey -algorithm ED25519 -out "$temporary/other-private.pem" >/dev/null 2>&1
openssl pkey -in "$temporary/other-private.pem" -pubout -out "$public_key" >/dev/null 2>&1
if CURL_CA_BUNDLE="$certificate" bash "$test_bootstrap" >"$temporary/wrong-key-output" 2>&1; then
    printf 'wrong release public key unexpectedly passed fingerprint verification\n' >&2
    exit 1
fi
grep -F 'release public key fingerprint verification failed' "$temporary/wrong-key-output" >/dev/null
cp -- "$temporary/public-key-backup" "$public_key"

printf 'Linux one-click bootstrap security test passed\n'
