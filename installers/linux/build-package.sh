#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
target=
signing_key=
output_directory=
binary_directory=
version=

usage() {
    cat >&2 <<'EOF'
Usage: build-package.sh --target TARGET --signing-key FILE --output DIRECTORY [options]

Options:
  --version VERSION       Override the workspace version (test packaging only).
  --binary-dir DIRECTORY  Package prebuilt xs-agent and xs binaries without invoking Cargo.
EOF
}

while (($#)); do
    case "$1" in
        --target|--signing-key|--output|--version|--binary-dir)
            (($# >= 2)) || { usage; exit 2; }
            case "$1" in
                --target) target=$2 ;;
                --signing-key) signing_key=$2 ;;
                --output) output_directory=$2 ;;
                --version) version=$2 ;;
                --binary-dir) binary_directory=$2 ;;
            esac
            shift 2
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

[[ -n "$target" && -n "$signing_key" && -n "$output_directory" ]] || { usage; exit 2; }
case "$target" in
    x86_64-unknown-linux-gnu) architecture=x86_64 ;;
    aarch64-unknown-linux-gnu) architecture=aarch64 ;;
    *) printf 'unsupported Linux release target: %s\n' "$target" >&2; exit 2 ;;
esac
if [[ ! -f "$signing_key" || -L "$signing_key" ]]; then
    printf 'release signing key must be a regular non-symlink file\n' >&2
    exit 2
fi
for command in gzip openssl readelf sha256sum stat tar; do
    command -v "$command" >/dev/null || { printf 'required command is unavailable: %s\n' "$command" >&2; exit 2; }
done
openssl pkey -in "$signing_key" -noout >/dev/null 2>&1 || {
    printf 'release signing key is not a readable OpenSSL private key\n' >&2
    exit 2
}

if [[ -z "$version" ]]; then
    version=$(awk '
        $0 == "[workspace.package]" { in_workspace_package = 1; next }
        /^\[/ { in_workspace_package = 0 }
        in_workspace_package && $1 == "version" {
            gsub(/["[:space:]]/, "", $3)
            print $3
            exit
        }
    ' "$ROOT_DIR/Cargo.toml")
fi
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    printf 'release version must use numeric major.minor.patch form\n' >&2
    exit 2
}

build_commit=${XS_BUILD_GIT_COMMIT:-}
source_date_epoch=${SOURCE_DATE_EPOCH:-}
if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    repository_commit=$(git -C "$ROOT_DIR" rev-parse HEAD)
    if [[ -n "$build_commit" && "$build_commit" != "$repository_commit" ]]; then
        printf 'XS_BUILD_GIT_COMMIT differs from the repository HEAD\n' >&2
        exit 2
    fi
    build_commit=$repository_commit
    if [[ -z "$source_date_epoch" ]]; then
        source_date_epoch=$(git -C "$ROOT_DIR" show -s --format=%ct HEAD)
    fi
fi
[[ "$build_commit" =~ ^[0-9a-f]{40}$ ]] || {
    printf 'XS_BUILD_GIT_COMMIT must identify an exact lowercase Git commit\n' >&2
    exit 2
}
[[ "$source_date_epoch" =~ ^[0-9]+$ ]] || { printf 'SOURCE_DATE_EPOCH must be an integer\n' >&2; exit 2; }

if [[ -z "$binary_directory" ]]; then
    command -v cargo >/dev/null || { printf 'required command is unavailable: cargo\n' >&2; exit 2; }
    if [[ "$target" == aarch64-unknown-linux-gnu ]]; then
        command -v aarch64-linux-gnu-gcc >/dev/null || {
            printf 'aarch64-linux-gnu-gcc is required for the aarch64 release\n' >&2
            exit 2
        }
        rust_source="$(rustc --print sysroot)/lib/rustlib/src/rust/library"
        [[ -d "$rust_source" ]] || {
            printf 'Rust standard-library source is required for the aarch64 release\n' >&2
            exit 2
        }
        (
            cd "$ROOT_DIR"
            XS_BUILD_GIT_COMMIT="$build_commit" \
            XS_BUILD_DATE_EPOCH="$source_date_epoch" \
            RUSTC_BOOTSTRAP=1 \
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
                cargo build --locked --release --target "$target" \
                    -Z build-std=std,panic_abort -p xs-agent -p xs-cli
        )
    else
        (
            cd "$ROOT_DIR"
            XS_BUILD_GIT_COMMIT="$build_commit" \
            XS_BUILD_DATE_EPOCH="$source_date_epoch" \
                cargo build --locked --release --target "$target" -p xs-agent -p xs-cli
        )
    fi
    binary_directory="$ROOT_DIR/target/$target/release"
fi
for binary in xs-agent xs; do
    if [[ ! -f "$binary_directory/$binary" || -L "$binary_directory/$binary" || ! -x "$binary_directory/$binary" ]]; then
        printf 'required release binary is unavailable: %s/%s\n' "$binary_directory" "$binary" >&2
        exit 2
    fi
done
case "$architecture" in
    x86_64) expected_machine='Advanced Micro Devices X86-64' ;;
    aarch64) expected_machine='AArch64' ;;
esac
for binary in xs-agent xs; do
    machine=$(readelf -h "$binary_directory/$binary" 2>/dev/null | awk -F: '/^[[:space:]]*Machine:/ { sub(/^[[:space:]]+/, "", $2); print $2 }')
    [[ "$machine" == "$expected_machine" ]] || {
        printf 'release binary architecture is invalid: %s/%s\n' "$binary_directory" "$binary" >&2
        exit 2
    }
done

mkdir -p "$output_directory"
output_directory=$(cd "$output_directory" && pwd -P)
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT INT TERM

package_name="xs-nexus-$version-$target"
package_root="$temporary/$package_name"
install -d -m 0755 \
    "$package_root/bin" \
    "$package_root/lib/systemd/system" \
    "$package_root/share/doc/xs-nexus" \
    "$package_root/share/xs-nexus"
install -m 0755 "$binary_directory/xs-agent" "$package_root/bin/xs-agent"
install -m 0755 "$binary_directory/xs" "$package_root/bin/xs"
install -m 0644 "$ROOT_DIR/deploy/systemd/xs-agent.service" "$package_root/lib/systemd/system/xs-agent.service"
install -m 0644 "$ROOT_DIR/deploy/systemd/xs-agent-update.service" "$package_root/lib/systemd/system/xs-agent-update.service"
install -m 0644 "$ROOT_DIR/deploy/systemd/xs-agent-update.path" "$package_root/lib/systemd/system/xs-agent-update.path"
install -m 0755 "$ROOT_DIR/installers/linux/xs-nexus-installer.sh" "$package_root/share/xs-nexus/xs-nexus-installer.sh"
install -m 0644 "$ROOT_DIR/deploy/systemd/agent.example.json" "$package_root/share/xs-nexus/agent.example.json"
install -m 0644 "$ROOT_DIR/docs/LINUX_INSTALLATION.md" "$package_root/share/doc/xs-nexus/LINUX_INSTALLATION.md"
(
    cd "$package_root"
    find bin lib share -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum >PAYLOAD.SHA256
)
chmod 0644 "$package_root/PAYLOAD.SHA256"

archive="$output_directory/$package_name.tar.gz"
manifest="$output_directory/$package_name.manifest"
signature="$manifest.sig"
(
    cd "$temporary"
    tar \
        --sort=name \
        --owner=0 \
        --group=0 \
        --numeric-owner \
        --mtime="@$source_date_epoch" \
        --pax-option=delete=atime,delete=ctime \
        -cf - "$package_name" | gzip -n >"$archive"
)
archive_size=$(stat -c '%s' "$archive")
archive_sha256=$(sha256sum "$archive" | awk '{print $1}')
cat >"$manifest" <<EOF
schema_version=2
product=xs-nexus
version=$version
source_commit=$build_commit
source_date_epoch=$source_date_epoch
protocol_version=XSP/1
platform=linux
architecture=$architecture
target=$target
archive=$package_name.tar.gz
archive_size=$archive_size
archive_sha256=$archive_sha256
EOF
openssl pkeyutl -sign -rawin -inkey "$signing_key" -in "$manifest" -out "$signature"
public_key="$temporary/release-public-key.pem"
openssl pkey -in "$signing_key" -pubout -out "$public_key" >/dev/null 2>&1
openssl pkeyutl -verify -rawin -pubin -inkey "$public_key" -in "$manifest" -sigfile "$signature" >/dev/null
chmod 0644 "$archive" "$manifest" "$signature"

printf 'archive=%s\nmanifest=%s\nsignature=%s\n' "$archive" "$manifest" "$signature"
