#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BUILDER="$ROOT_DIR/installers/linux/build-package.sh"
INSTALLER="$ROOT_DIR/installers/linux/xs-nexus-installer.sh"

for command in bash gcc gzip openssl sha256sum stat systemd-analyze tar; do
    command -v "$command" >/dev/null || { printf 'required command is unavailable: %s\n' "$command" >&2; exit 2; }
done

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT INT TERM
test_root="$temporary/root"
artifacts="$temporary/artifacts"
binaries="$temporary/binaries"
mkdir -p "$test_root" "$artifacts" "$binaries"

private_key="$temporary/release-private-key.pem"
public_key="$temporary/release-public-key.pem"
other_private_key="$temporary/other-private-key.pem"
other_public_key="$temporary/other-public-key.pem"
openssl genpkey -algorithm ED25519 -out "$private_key" >/dev/null 2>&1
openssl pkey -in "$private_key" -pubout -out "$public_key" >/dev/null 2>&1
openssl genpkey -algorithm ED25519 -out "$other_private_key" >/dev/null 2>&1
openssl pkey -in "$other_private_key" -pubout -out "$other_public_key" >/dev/null 2>&1
chmod 0600 "$private_key" "$other_private_key"

fake_service_manager="$temporary/fake-systemctl"
cat >"$fake_service_manager" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
root=${XS_NEXUS_INSTALL_ROOT:?}
state_directory="$root/run/fake-systemctl"
mkdir -p "$state_directory"
operation=$1
shift
printf '%s' "$operation" >>"$state_directory/calls"
printf ' %s' "$@" >>"$state_directory/calls"
printf '\n' >>"$state_directory/calls"
case "$operation" in
    is-active)
        [[ -f "$state_directory/active" ]]
        ;;
    start)
        current="$root/usr/local/lib/xs-nexus/current"
        release=$(basename "$(readlink "$current")")
        [[ ! -f "$root/fail-start-$release" ]] || exit 1
        touch "$state_directory/active"
        ;;
    stop)
        rm -f "$state_directory/active"
        ;;
    enable)
        touch "$state_directory/enabled"
        ;;
    disable)
        rm -f "$state_directory/enabled" "$state_directory/active"
        ;;
    daemon-reload)
        touch "$state_directory/reloaded"
        ;;
    *)
        printf 'unexpected fake systemctl operation: %s\n' "$operation" >&2
        exit 2
        ;;
esac
EOF
chmod 0755 "$fake_service_manager"

make_binaries() {
    local version=$1
    local directory="$binaries/$version"
    mkdir -p "$directory"
    cat >"$directory/xs-agent.c" <<EOF
#include <stdio.h>
#include <fcntl.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int main(int argument_count, char **arguments) {
    if (argument_count == 2 && strcmp(arguments[1], "--version") == 0) {
        puts("xs-agent $version");
        return 0;
    }
    if (argument_count >= 2 && strcmp(arguments[1], "cleanup") == 0) {
        return access("$test_root/fail-cleanup", F_OK) == 0 ? 1 : 0;
    }
    if (argument_count >= 2 && strcmp(arguments[1], "enroll") == 0) {
        const char *token_path = NULL;
        const char *expected_prefix = "$test_root/var/lib/xs-nexus/.enrollment-token.";
        for (int index = 2; index + 1 < argument_count; index++) {
            if (strcmp(arguments[index], "--token-file") == 0) {
                token_path = arguments[index + 1];
            }
        }
        if (token_path == NULL || strncmp(token_path, expected_prefix, strlen(expected_prefix)) != 0) {
            return 3;
        }
        if (unlink(token_path) != 0) {
            return 4;
        }
        int state = open("$test_root/var/lib/xs-nexus/node-state.json", O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (state < 0 || write(state, "{}", 2) != 2 || close(state) != 0) {
            return 5;
        }
        return 0;
    }
    return 2;
}
EOF
    cat >"$directory/xs.c" <<EOF
#include <stdio.h>
#include <string.h>

int main(int argument_count, char **arguments) {
    if (argument_count == 2 && strcmp(arguments[1], "--version") == 0) {
        puts("xs $version");
        return 0;
    }
    return 2;
}
EOF
    gcc -O2 -o "$directory/xs-agent" "$directory/xs-agent.c"
    gcc -O2 -o "$directory/xs" "$directory/xs.c"
}

build_package() {
    local version=$1
    make_binaries "$version"
    SOURCE_DATE_EPOCH=1700000000 "$BUILDER" \
        --target x86_64-unknown-linux-gnu \
        --signing-key "$private_key" \
        --output "$artifacts" \
        --version "$version" \
        --binary-dir "$binaries/$version" >/dev/null
}

package_path() {
    printf '%s/xs-nexus-%s-x86_64-unknown-linux-gnu%s' "$artifacts" "$1" "$2"
}

install_package() {
    local version=$1
    "$INSTALLER" install \
        --archive "$(package_path "$version" .tar.gz)" \
        --manifest "$(package_path "$version" .manifest)" \
        --signature "$(package_path "$version" .manifest.sig)" \
        --public-key "$public_key" \
        --root "$test_root" \
        --service-manager "$fake_service_manager"
}

assert_fails() {
    if "$@" >/dev/null 2>&1; then
        printf 'command unexpectedly succeeded:' >&2
        printf ' %q' "$@" >&2
        printf '\n' >&2
        exit 1
    fi
}

current_release() {
    basename "$(readlink "$test_root/usr/local/lib/xs-nexus/current")"
}

assert_active() {
    [[ -f "$test_root/run/fake-systemctl/active" ]] || { printf 'fixture service is not active\n' >&2; exit 1; }
}

assert_service_call() {
    grep -Fx "$1" "$test_root/run/fake-systemctl/calls" >/dev/null || {
        printf 'missing fake systemctl call: %s\n' "$1" >&2
        exit 1
    }
}

for version in 0.9.0 1.0.0 1.1.0 1.2.0; do
    build_package "$version"
done

enrollment_config="$temporary/enrollment-agent.json"
enrollment_token="$temporary/enrollment.token"
printf '{"fixture":"first-enrollment"}\n' >"$enrollment_config"
printf 'one-time-enrollment-token-for-installer-test' >"$enrollment_token"
chmod 0600 "$enrollment_config" "$enrollment_token"
"$INSTALLER" install \
    --archive "$(package_path 1.0.0 .tar.gz)" \
    --manifest "$(package_path 1.0.0 .manifest)" \
    --signature "$(package_path 1.0.0 .manifest.sig)" \
    --public-key "$public_key" \
    --config "$enrollment_config" \
    --enrollment-token-file "$enrollment_token" \
    --root "$test_root" \
    --service-manager "$fake_service_manager" >/dev/null
[[ -f "$test_root/var/lib/xs-nexus/node-state.json" ]]
[[ -z $(find "$test_root/var/lib/xs-nexus" -maxdepth 1 -name '.enrollment-token.*' -print -quit) ]]
[[ -f "$enrollment_token" ]]
assert_active
"$INSTALLER" uninstall --purge --root "$test_root" --service-manager "$fake_service_manager" >/dev/null

install -d -m 0750 "$test_root/etc/xs-nexus"
install -d -m 0700 "$test_root/var/lib/xs-nexus"
printf '{"fixture":"configuration-not-read-by-test-binary"}\n' >"$test_root/etc/xs-nexus/agent.json"
printf '%032d' 0 >"$test_root/var/lib/xs-nexus/identity.key"
printf '{"fixture":"signed-state-not-read-by-test-binary"}\n' >"$test_root/var/lib/xs-nexus/node-state.json"
chmod 0640 "$test_root/etc/xs-nexus/agent.json"
chmod 0600 "$test_root/var/lib/xs-nexus/identity.key" "$test_root/var/lib/xs-nexus/node-state.json"
identity_hash=$(sha256sum "$test_root/var/lib/xs-nexus/identity.key" | awk '{print $1}')

install_package 1.0.0 >/dev/null
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]
assert_active
[[ -L "$test_root/usr/local/bin/xs" ]]
[[ $("$test_root/usr/local/bin/xs" --version) == 'xs 1.0.0' ]]
[[ -f "$test_root/etc/systemd/system/xs-agent-update.service" ]]
[[ -f "$test_root/etc/systemd/system/xs-agent-update.path" ]]
[[ -x "$test_root/usr/local/lib/xs-nexus/current/share/xs-nexus/xs-nexus-installer.sh" ]]
cmp "$test_root/etc/systemd/system/xs-agent-update.service" \
    "$test_root/usr/local/lib/xs-nexus/current/lib/systemd/system/xs-agent-update.service"
cmp "$test_root/etc/systemd/system/xs-agent-update.path" \
    "$test_root/usr/local/lib/xs-nexus/current/lib/systemd/system/xs-agent-update.path"
systemd-analyze verify --recursive-errors=no --root="$test_root" \
    xs-agent-update.service xs-agent-update.path
assert_service_call 'enable xs-agent-update.path'
assert_service_call 'start xs-agent-update.path'
install_package 1.0.0 >/dev/null
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]
assert_active

tampered_directory="$temporary/tampered"
mkdir -p "$tampered_directory"
cp "$(package_path 1.1.0 .tar.gz)" "$tampered_directory/$(basename "$(package_path 1.1.0 .tar.gz)")"
printf 'x' | dd of="$tampered_directory/$(basename "$(package_path 1.1.0 .tar.gz)")" bs=1 seek=100 conv=notrunc status=none
assert_fails "$INSTALLER" install \
    --archive "$tampered_directory/$(basename "$(package_path 1.1.0 .tar.gz)")" \
    --manifest "$(package_path 1.1.0 .manifest)" \
    --signature "$(package_path 1.1.0 .manifest.sig)" \
    --public-key "$public_key" --root "$test_root" --service-manager "$fake_service_manager"
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]

bad_signature="$temporary/bad-signature"
cp "$(package_path 1.1.0 .manifest.sig)" "$bad_signature"
printf 'x' | dd of="$bad_signature" bs=1 seek=0 conv=notrunc status=none
assert_fails "$INSTALLER" install \
    --archive "$(package_path 1.1.0 .tar.gz)" \
    --manifest "$(package_path 1.1.0 .manifest)" \
    --signature "$bad_signature" \
    --public-key "$public_key" --root "$test_root" --service-manager "$fake_service_manager"
assert_fails "$INSTALLER" install \
    --archive "$(package_path 1.1.0 .tar.gz)" \
    --manifest "$(package_path 1.1.0 .manifest)" \
    --signature "$(package_path 1.1.0 .manifest.sig)" \
    --public-key "$other_public_key" --root "$test_root" --service-manager "$fake_service_manager"

trusted_tamper="$temporary/trusted-tamper"
mkdir -p "$trusted_tamper/unpack" "$trusted_tamper/output"
tar -xzf "$(package_path 1.1.0 .tar.gz)" -C "$trusted_tamper/unpack"
printf '# trusted-signer payload tamper\n' >>"$trusted_tamper/unpack/xs-nexus-1.1.0-x86_64-unknown-linux-gnu/bin/xs-agent"
(
    cd "$trusted_tamper/unpack"
    tar -czf "$trusted_tamper/output/xs-nexus-1.1.0-x86_64-unknown-linux-gnu.tar.gz" xs-nexus-1.1.0-x86_64-unknown-linux-gnu
)
trusted_archive="$trusted_tamper/output/xs-nexus-1.1.0-x86_64-unknown-linux-gnu.tar.gz"
trusted_manifest="$trusted_tamper/output/xs-nexus-1.1.0-x86_64-unknown-linux-gnu.manifest"
head -n 7 "$(package_path 1.1.0 .manifest)" >"$trusted_manifest"
printf 'archive_size=%s\narchive_sha256=%s\n' \
    "$(stat -c '%s' "$trusted_archive")" \
    "$(sha256sum "$trusted_archive" | awk '{print $1}')" >>"$trusted_manifest"
openssl pkeyutl -sign -rawin -inkey "$private_key" -in "$trusted_manifest" -out "$trusted_manifest.sig"
assert_fails "$INSTALLER" install \
    --archive "$trusted_archive" --manifest "$trusted_manifest" --signature "$trusted_manifest.sig" \
    --public-key "$public_key" --root "$test_root" --service-manager "$fake_service_manager"
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]

install_package 1.1.0 >/dev/null
[[ $(current_release) == 1.1.0-x86_64-unknown-linux-gnu ]]
[[ $(basename "$(readlink "$test_root/usr/local/lib/xs-nexus/previous")") == 1.0.0-x86_64-unknown-linux-gnu ]]
[[ $(sha256sum "$test_root/var/lib/xs-nexus/identity.key" | awk '{print $1}') == "$identity_hash" ]]
assert_active
assert_fails install_package 0.9.0
[[ $(current_release) == 1.1.0-x86_64-unknown-linux-gnu ]]

touch "$test_root/fail-start-1.2.0-x86_64-unknown-linux-gnu"
assert_fails install_package 1.2.0
rm "$test_root/fail-start-1.2.0-x86_64-unknown-linux-gnu"
[[ $(current_release) == 1.1.0-x86_64-unknown-linux-gnu ]]
[[ ! -e "$test_root/usr/local/lib/xs-nexus/versions/1.2.0-x86_64-unknown-linux-gnu" ]]
[[ $(sha256sum "$test_root/var/lib/xs-nexus/identity.key" | awk '{print $1}') == "$identity_hash" ]]
assert_active

"$INSTALLER" rollback --version 1.0.0 --root "$test_root" --service-manager "$fake_service_manager" >/dev/null
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]
assert_active
cmp "$test_root/etc/systemd/system/xs-agent-update.service" \
    "$test_root/usr/local/lib/xs-nexus/current/lib/systemd/system/xs-agent-update.service"
cmp "$test_root/etc/systemd/system/xs-agent-update.path" \
    "$test_root/usr/local/lib/xs-nexus/current/lib/systemd/system/xs-agent-update.path"

touch "$test_root/fail-cleanup"
assert_fails "$INSTALLER" uninstall --root "$test_root" --service-manager "$fake_service_manager"
rm "$test_root/fail-cleanup"
[[ $(current_release) == 1.0.0-x86_64-unknown-linux-gnu ]]
assert_active

status_output=$("$INSTALLER" status --root "$test_root" --service-manager "$fake_service_manager")
[[ "$status_output" == *'release=1.0.0-x86_64-unknown-linux-gnu'* ]]
[[ "$status_output" == *'service_active=true'* ]]
[[ "$status_output" != *'fixture'* ]]

"$INSTALLER" uninstall --root "$test_root" --service-manager "$fake_service_manager" >/dev/null
[[ ! -e "$test_root/usr/local/lib/xs-nexus" ]]
[[ ! -e "$test_root/usr/local/bin/xs" ]]
[[ ! -e "$test_root/etc/systemd/system/xs-agent.service" ]]
[[ ! -e "$test_root/etc/systemd/system/xs-agent-update.service" ]]
[[ ! -e "$test_root/etc/systemd/system/xs-agent-update.path" ]]
assert_service_call 'stop xs-agent-update.path'
assert_service_call 'disable xs-agent-update.path'
[[ -f "$test_root/etc/xs-nexus/agent.json" ]]
[[ -f "$test_root/var/lib/xs-nexus/identity.key" ]]
[[ $(sha256sum "$test_root/var/lib/xs-nexus/identity.key" | awk '{print $1}') == "$identity_hash" ]]

install_package 1.1.0 >/dev/null
[[ $(current_release) == 1.1.0-x86_64-unknown-linux-gnu ]]
[[ $(sha256sum "$test_root/var/lib/xs-nexus/identity.key" | awk '{print $1}') == "$identity_hash" ]]
assert_active
"$INSTALLER" uninstall --purge --root "$test_root" --service-manager "$fake_service_manager" >/dev/null
[[ ! -e "$test_root/usr/local/lib/xs-nexus" ]]
[[ ! -e "$test_root/etc/xs-nexus" ]]
[[ ! -e "$test_root/var/lib/xs-nexus" ]]
[[ ! -f "$test_root/run/fake-systemctl/active" ]]

printf 'Linux signed package lifecycle test passed\n'
