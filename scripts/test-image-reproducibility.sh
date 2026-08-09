#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_DIR="${1:?usage: test-image-reproducibility.sh EVIDENCE_DIR}"
readonly SERVICES=(edge console controller relay db-tools)
readonly VERSION=0.1.0
readonly SOURCE_URL=https://github.com/Hinln/xs-nexus

declare -A dockerfiles=(
    [controller]=deploy/docker/controller.Dockerfile
    [relay]=deploy/docker/relay.Dockerfile
    [console]=deploy/docker/console.Dockerfile
    [db-tools]=deploy/docker/db-tools.Dockerfile
    [edge]=deploy/docker/edge.Dockerfile
)

if [[ -n "$(git status --short)" ]]; then
    printf 'image reproducibility requires a clean worktree\n' >&2
    exit 2
fi

revision="$(git rev-parse HEAD)"
source_date_epoch="$(git show -s --format=%ct HEAD)"
mkdir -p "${EVIDENCE_DIR}/first" "${EVIDENCE_DIR}/second"
: >"${EVIDENCE_DIR}/summary.txt"

{
    printf 'revision=%s\n' "${revision}"
    printf 'source_date_epoch=%s\n' "${source_date_epoch}"
    docker version
    docker buildx version
    docker buildx inspect --bootstrap
} >"${EVIDENCE_DIR}/environment.txt" 2>&1

record_oci_metadata() {
    python3 - "$1" "$2" "$3" "$4" "$5" "$6" "$7" <<'PY'
import json
import hashlib
import sys
import tarfile
from pathlib import Path

archive_path, output_path, service, revision, version, source_url, source_date_epoch = sys.argv[1:]
with tarfile.open(archive_path) as archive:
    index = json.load(archive.extractfile("index.json"))
    descriptor = index["manifests"][0]
    manifest_digest = descriptor["digest"].removeprefix("sha256:")
    manifest = json.load(archive.extractfile(f"blobs/sha256/{manifest_digest}"))
    config_digest = manifest["config"]["digest"].removeprefix("sha256:")
    config = json.load(archive.extractfile(f"blobs/sha256/{config_digest}"))
    labels = config.get("config", {}).get("Labels", {})
    expected_labels = {
        "org.opencontainers.image.revision": revision,
        "org.opencontainers.image.version": version,
        "org.opencontainers.image.source": source_url,
    }
    for name, expected in expected_labels.items():
        if labels.get(name) != expected:
            raise SystemExit(f"{service} OCI label mismatch for {name}")
    console_version = None
    if service == "console":
        for layer in manifest["layers"]:
            layer_digest = layer["digest"].removeprefix("sha256:")
            with tarfile.open(
                fileobj=archive.extractfile(f"blobs/sha256/{layer_digest}"), mode="r|*"
            ) as layer_archive:
                for member in layer_archive:
                    name = member.name.removeprefix("./")
                    if name == "usr/share/nginx/html/version.json" and member.isfile():
                        console_version = json.load(layer_archive.extractfile(member))
        expected_console_version = {
            "product": "xs-nexus",
            "component": "xs-console",
            "version": version,
            "commit": revision,
            "protocol_version": "XSP/1",
            "build_date_epoch": source_date_epoch,
            "source": source_url,
        }
        if console_version is None:
            raise SystemExit("console version.json is missing")
        if console_version != expected_console_version:
            raise SystemExit("console version.json identity is invalid")
    last_layer = manifest["layers"][-1]
    last_layer_digest = last_layer["digest"].removeprefix("sha256:")
    entries = []
    with tarfile.open(
        fileobj=archive.extractfile(f"blobs/sha256/{last_layer_digest}"), mode="r|*"
    ) as layer_archive:
        for member in layer_archive:
            entry = {
                "name": member.name,
                "type": member.type.decode("latin-1"),
                "size": member.size,
                "mode": member.mode,
                "uid": member.uid,
                "gid": member.gid,
                "uname": member.uname,
                "gname": member.gname,
                "mtime": member.mtime,
                "linkname": member.linkname,
                "pax_headers": member.pax_headers,
            }
            if member.isfile():
                content = layer_archive.extractfile(member)
                digest = hashlib.sha256()
                for chunk in iter(lambda: content.read(1024 * 1024), b""):
                    digest.update(chunk)
                entry["sha256"] = digest.hexdigest()
            entries.append(entry)
Path(output_path).write_text(
    json.dumps(
        {
            "index_descriptor": descriptor,
            "manifest": manifest,
            "config": config,
            "validated_labels": expected_labels,
            "console_version": console_version,
            "last_layer_entries": entries,
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

for service in "${SERVICES[@]}"; do
    for pass in first second; do
        output="${EVIDENCE_DIR}/${pass}/${service}.oci.tar"
        docker buildx build \
            --pull=false \
            --no-cache \
            --progress=plain \
            --provenance=false \
            --sbom=false \
            --build-arg "VCS_REF=${revision}" \
            --build-arg "SOURCE_DATE_EPOCH=${source_date_epoch}" \
            --build-arg "XS_VERSION=${VERSION}" \
            --build-arg "XS_SOURCE_URL=${SOURCE_URL}" \
            --file "${dockerfiles[${service}]}" \
            --output "type=oci,dest=${output},rewrite-timestamp=true,compatibility-version=20" \
            . >"${EVIDENCE_DIR}/${pass}/${service}.build.log" 2>&1
        sha256sum "${output}" >"${EVIDENCE_DIR}/${pass}/${service}.sha256"
        record_oci_metadata \
            "${output}" \
            "${EVIDENCE_DIR}/${pass}/${service}.metadata.json" \
            "${service}" \
            "${revision}" \
            "${VERSION}" \
            "${SOURCE_URL}" \
            "${source_date_epoch}"
        rm -f -- "${output}"
    done
    first_hash="$(cut -d ' ' -f 1 "${EVIDENCE_DIR}/first/${service}.sha256")"
    second_hash="$(cut -d ' ' -f 1 "${EVIDENCE_DIR}/second/${service}.sha256")"
    if [[ "${first_hash}" != "${second_hash}" ]]; then
        printf '%s image is not reproducible: %s != %s\n' \
            "${service}" "${first_hash}" "${second_hash}" >&2
        exit 1
    fi
    printf '%s %s\n' "${service}" "${first_hash}" | tee -a "${EVIDENCE_DIR}/summary.txt"
done
