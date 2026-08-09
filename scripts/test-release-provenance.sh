#!/usr/bin/env bash
set -Eeuo pipefail
export LC_ALL=C

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
readonly ROOT_DIR
TEMPORARY=$(mktemp -d)
readonly TEMPORARY
readonly REVISION=0123456789abcdef0123456789abcdef01234567
readonly TAG=v0.1.0
readonly PYTHON=${PYTHON:-python3}

cleanup() {
    rm -rf -- "$TEMPORARY"
}
trap cleanup EXIT INT TERM

cd "$ROOT_DIR"
mkdir -p "$TEMPORARY/source"
printf '{"spdxVersion":"SPDX-2.3"}\n' >"$TEMPORARY/source/SBOM.spdx.json"
printf '# Third-party notices\n' >"$TEMPORARY/source/THIRD_PARTY.md"
printf '# Release notes\n' >"$TEMPORARY/source/release-notes.md"
printf 'signed release artifact\n' >"$TEMPORARY/source/xs-agent-linux-amd64.tar.zst"

"$PYTHON" - "$TEMPORARY/source/inventory.json" "$REVISION" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
revision = sys.argv[2]
digest = "sha256:" + "a" * 64
inventory = {
    "schema_version": 1,
    "product": "xs-nexus",
    "version": "0.1.0",
    "revision": revision,
    "tag": "v0.1.0",
    "source_url": "https://github.com/Hinln/xs-nexus",
    "source_date_epoch": 0,
    "protocol_version": "XSP/1",
    "build_inputs": [
        {"name": "rust-toolchain", "value": "1.93.0"},
        {"name": "source-date-epoch", "value": "0"},
    ],
    "sbom_spdx": "SBOM.spdx.json",
    "third_party": "THIRD_PARTY.md",
    "release_notes": "release-notes.md",
    "artifacts": [
        {
            "component": "agent",
            "platform": "linux",
            "architecture": "amd64",
            "name": "xs-agent-linux-amd64.tar.zst",
            "path": "xs-agent-linux-amd64.tar.zst",
        }
    ],
    "images": [
        {
            "component": "controller",
            "reference": f"registry.example/xs-controller@{digest}",
            "digest": digest,
            "image_id": "sha256:" + "b" * 64,
        }
    ],
}
output.write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
PY

"$PYTHON" ./scripts/generate-release-provenance.py \
    --inventory "$TEMPORARY/source/inventory.json" \
    --output-dir "$TEMPORARY/bundle-first"
"$PYTHON" ./scripts/generate-release-provenance.py \
    --inventory "$TEMPORARY/source/inventory.json" \
    --output-dir "$TEMPORARY/bundle-second"
diff --recursive --no-dereference "$TEMPORARY/bundle-first" "$TEMPORARY/bundle-second"

cat >"$TEMPORARY/expected-unsigned.txt" <<'EOF'
SBOM.spdx.json
SHA256SUMS
THIRD_PARTY.md
artifacts/xs-agent-linux-amd64.tar.zst
manifest.json
provenance.json
release-notes.md
EOF
find "$TEMPORARY/bundle-first" -type f -printf '%P\n' | sort >"$TEMPORARY/actual-unsigned.txt"
diff -u "$TEMPORARY/expected-unsigned.txt" "$TEMPORARY/actual-unsigned.txt"

openssl genpkey -algorithm Ed25519 -out "$TEMPORARY/private.pem" >/dev/null 2>&1
chmod 0600 "$TEMPORARY/private.pem"
openssl pkey -in "$TEMPORARY/private.pem" -pubout -out "$TEMPORARY/public.pem" >/dev/null 2>&1
./scripts/sign-release-provenance.sh \
    --bundle "$TEMPORARY/bundle-first" \
    --signing-key "$TEMPORARY/private.pem"
"$PYTHON" ./scripts/verify-release-provenance.py \
    --bundle "$TEMPORARY/bundle-first" \
    --public-key "$TEMPORARY/public.pem" \
    --expected-revision "$REVISION" \
    --expected-tag "$TAG"

assert_verification_rejected() {
    local name=$1
    shift
    if "$@" >"$TEMPORARY/$name.stdout" 2>"$TEMPORARY/$name.stderr"; then
        printf 'release verifier accepted invalid case: %s\n' "$name" >&2
        exit 1
    fi
}

cp -a "$TEMPORARY/bundle-first" "$TEMPORARY/tampered-artifact"
printf 'tampered\n' >>"$TEMPORARY/tampered-artifact/artifacts/xs-agent-linux-amd64.tar.zst"
assert_verification_rejected tampered-artifact \
    "$PYTHON" ./scripts/verify-release-provenance.py --bundle "$TEMPORARY/tampered-artifact" \
    --public-key "$TEMPORARY/public.pem" --expected-revision "$REVISION" --expected-tag "$TAG"

cp -a "$TEMPORARY/bundle-first" "$TEMPORARY/tampered-signature"
printf 'invalid' >"$TEMPORARY/tampered-signature/manifest.sig"
assert_verification_rejected tampered-signature \
    "$PYTHON" ./scripts/verify-release-provenance.py --bundle "$TEMPORARY/tampered-signature" \
    --public-key "$TEMPORARY/public.pem" --expected-revision "$REVISION" --expected-tag "$TAG"

cp -a "$TEMPORARY/bundle-first" "$TEMPORARY/extra-file"
printf 'unexpected\n' >"$TEMPORARY/extra-file/untracked.txt"
printf '%s  untracked.txt\n' "$(sha256sum "$TEMPORARY/extra-file/untracked.txt" | cut -d ' ' -f 1)" \
    >>"$TEMPORARY/extra-file/SHA256SUMS"
openssl pkeyutl -sign -rawin -inkey "$TEMPORARY/private.pem" \
    -in "$TEMPORARY/extra-file/SHA256SUMS" \
    -out "$TEMPORARY/extra-file/SHA256SUMS.sig"
assert_verification_rejected extra-file \
    "$PYTHON" ./scripts/verify-release-provenance.py --bundle "$TEMPORARY/extra-file" \
    --public-key "$TEMPORARY/public.pem" --expected-revision "$REVISION" --expected-tag "$TAG"

mkdir "$TEMPORARY/source-repository"
git -C "$TEMPORARY/source-repository" init --quiet
git -C "$TEMPORARY/source-repository" config user.name 'XS Nexus Release Test'
git -C "$TEMPORARY/source-repository" config user.email 'release-test@xs-nexus.invalid'
git -C "$TEMPORARY/source-repository" config commit.gpgSign false
git -C "$TEMPORARY/source-repository" config tag.gpgSign false
git -C "$TEMPORARY/source-repository" config gpg.format ssh
ssh-keygen -q -t ed25519 -N '' -f "$TEMPORARY/tag-signing-key"
printf 'release-test@xs-nexus.invalid %s\n' "$(cat "$TEMPORARY/tag-signing-key.pub")" \
    >"$TEMPORARY/allowed-signers"
git -C "$TEMPORARY/source-repository" config user.signingkey "$TEMPORARY/tag-signing-key"
git -C "$TEMPORARY/source-repository" config gpg.ssh.allowedSignersFile "$TEMPORARY/allowed-signers"
git -C "$TEMPORARY/source-repository" remote add github https://github.com/Hinln/xs-nexus.git
printf 'release source\n' >"$TEMPORARY/source-repository/source.txt"
git -C "$TEMPORARY/source-repository" add source.txt
git -C "$TEMPORARY/source-repository" commit --quiet -m 'release source fixture'
source_revision=$(git -C "$TEMPORARY/source-repository" rev-parse HEAD)
source_epoch=$(git -C "$TEMPORARY/source-repository" show -s --format=%ct HEAD)
git -C "$TEMPORARY/source-repository" tag -s v0.1.0 -m 'signed release fixture'
"$PYTHON" - "$TEMPORARY/source/inventory.json" "$TEMPORARY/source-inventory.json" \
    "$source_revision" "$source_epoch" <<'PY'
import json
import sys
from pathlib import Path

value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
value["revision"] = sys.argv[3]
value["source_date_epoch"] = int(sys.argv[4])
Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="utf-8")
PY
"$PYTHON" ./scripts/verify-release-source.py \
    --inventory "$TEMPORARY/source-inventory.json" \
    --repository "$TEMPORARY/source-repository"
git -C "$TEMPORARY/source-repository" tag -a v0.1.1 -m 'unsigned release fixture'
"$PYTHON" - "$TEMPORARY/source-inventory.json" "$TEMPORARY/unsigned-inventory.json" <<'PY'
import json
import sys
from pathlib import Path

value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
value["version"] = "0.1.1"
value["tag"] = "v0.1.1"
Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="utf-8")
PY
assert_verification_rejected unsigned-tag \
    "$PYTHON" ./scripts/verify-release-source.py \
    --inventory "$TEMPORARY/unsigned-inventory.json" \
    --repository "$TEMPORARY/source-repository"

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "$TEMPORARY/rsa-private.pem" >/dev/null 2>&1
if ./scripts/sign-release-provenance.sh \
    --bundle "$TEMPORARY/bundle-second" \
    --signing-key "$TEMPORARY/rsa-private.pem" >/dev/null 2>&1; then
    printf 'release signer accepted a non-Ed25519 key\n' >&2
    exit 1
fi

"$PYTHON" - "$TEMPORARY/source/inventory.json" "$TEMPORARY/bad-inventory.json" <<'PY'
import json
import sys
from pathlib import Path

value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
value["revision"] = "not-a-commit"
Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="utf-8")
PY
if "$PYTHON" ./scripts/generate-release-provenance.py \
    --inventory "$TEMPORARY/bad-inventory.json" \
    --output-dir "$TEMPORARY/rejected-revision" >/dev/null 2>&1; then
    printf 'release generator accepted an invalid revision\n' >&2
    exit 1
fi

ln -s "$TEMPORARY/source/xs-agent-linux-amd64.tar.zst" "$TEMPORARY/source/linked-artifact"
if [[ -L "$TEMPORARY/source/linked-artifact" ]]; then
    "$PYTHON" - "$TEMPORARY/source/inventory.json" "$TEMPORARY/source/linked-inventory.json" <<'PY'
import json
import sys
from pathlib import Path

value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
value["artifacts"][0]["path"] = "linked-artifact"
Path(sys.argv[2]).write_text(json.dumps(value) + "\n", encoding="utf-8")
PY
    if "$PYTHON" ./scripts/generate-release-provenance.py \
        --inventory "$TEMPORARY/source/linked-inventory.json" \
        --output-dir "$TEMPORARY/rejected-symlink" >/dev/null 2>&1; then
        printf 'release generator accepted a symlink artifact\n' >&2
        exit 1
    fi
else
    printf 'symlink rejection case skipped: filesystem did not create a symlink\n' >&2
fi

if ./scripts/sign-release-provenance.sh \
    --bundle "$TEMPORARY/bundle-first" \
    --signing-key "$TEMPORARY/private.pem" >/dev/null 2>&1; then
    printf 'release signer overwrote existing signatures\n' >&2
    exit 1
fi

printf 'release provenance tests passed\n'
