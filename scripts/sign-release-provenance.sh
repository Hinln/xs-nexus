#!/usr/bin/env bash
set -euo pipefail

bundle=
signing_key=

while (($#)); do
    case "$1" in
        --bundle|--signing-key)
            (($# >= 2)) || exit 2
            if [[ $1 == --bundle ]]; then bundle=$2; else signing_key=$2; fi
            shift 2
            ;;
        *) exit 2 ;;
    esac
done

[[ -n "$bundle" && -n "$signing_key" ]] || exit 2
[[ -d "$bundle" && ! -L "$bundle" ]] || { printf 'bundle must be a real directory\n' >&2; exit 2; }
[[ -f "$signing_key" && ! -L "$signing_key" ]] || { printf 'signing key must be a regular file\n' >&2; exit 2; }
for file in manifest.json SHA256SUMS; do
    [[ -f "$bundle/$file" && ! -L "$bundle/$file" ]] || { printf 'missing bundle file: %s\n' "$file" >&2; exit 2; }
done
for signature in manifest.sig SHA256SUMS.sig; do
    [[ ! -e "$bundle/$signature" && ! -L "$bundle/$signature" ]] || { printf 'signature already exists: %s\n' "$signature" >&2; exit 2; }
done
openssl pkey -in "$signing_key" -noout >/dev/null 2>&1 || { printf 'signing key is invalid\n' >&2; exit 2; }

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT INT TERM
openssl pkey -in "$signing_key" -pubout -out "$temporary/public.pem" >/dev/null 2>&1
openssl pkeyutl -sign -rawin -inkey "$signing_key" -in "$bundle/manifest.json" -out "$temporary/manifest.sig"
openssl pkeyutl -sign -rawin -inkey "$signing_key" -in "$bundle/SHA256SUMS" -out "$temporary/SHA256SUMS.sig"
for signature in manifest.sig SHA256SUMS.sig; do
    [[ $(wc -c <"$temporary/$signature") -eq 64 ]] || {
        printf 'signing key must produce a 64-byte Ed25519 signature\n' >&2
        exit 2
    }
done
openssl pkeyutl -verify -rawin -pubin -inkey "$temporary/public.pem" -in "$bundle/manifest.json" -sigfile "$temporary/manifest.sig" >/dev/null
openssl pkeyutl -verify -rawin -pubin -inkey "$temporary/public.pem" -in "$bundle/SHA256SUMS" -sigfile "$temporary/SHA256SUMS.sig" >/dev/null
install -m 0644 "$temporary/manifest.sig" "$bundle/manifest.sig"
install -m 0644 "$temporary/SHA256SUMS.sig" "$bundle/SHA256SUMS.sig"
printf 'signed_bundle=%s\n' "$bundle"
