#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_DIR="${1:?usage: test-image-reproducibility.sh EVIDENCE_DIR}"
readonly SERVICES=(edge console controller relay db-tools)

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
    python3 - "$1" "$2" <<'PY'
import json
import hashlib
import sys
import tarfile
from pathlib import Path

archive_path, output_path = sys.argv[1:]
with tarfile.open(archive_path) as archive:
    index = json.load(archive.extractfile("index.json"))
    descriptor = index["manifests"][0]
    manifest_digest = descriptor["digest"].removeprefix("sha256:")
    manifest = json.load(archive.extractfile(f"blobs/sha256/{manifest_digest}"))
    config_digest = manifest["config"]["digest"].removeprefix("sha256:")
    config = json.load(archive.extractfile(f"blobs/sha256/{config_digest}"))
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
            --file "${dockerfiles[${service}]}" \
            --output "type=oci,dest=${output},rewrite-timestamp=true,compatibility-version=20" \
            . >"${EVIDENCE_DIR}/${pass}/${service}.build.log" 2>&1
        sha256sum "${output}" >"${EVIDENCE_DIR}/${pass}/${service}.sha256"
        record_oci_metadata \
            "${output}" \
            "${EVIDENCE_DIR}/${pass}/${service}.metadata.json"
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
