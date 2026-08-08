#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_DIR="${1:?usage: test-image-reproducibility.sh EVIDENCE_DIR}"
readonly SERVICES=(controller relay console db-tools edge)

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

{
    printf 'revision=%s\n' "${revision}"
    printf 'source_date_epoch=%s\n' "${source_date_epoch}"
    docker version
    docker buildx version
    docker buildx inspect --bootstrap
} >"${EVIDENCE_DIR}/environment.txt" 2>&1

for pass in first second; do
    for service in "${SERVICES[@]}"; do
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
        rm -f -- "${output}"
    done
done

for service in "${SERVICES[@]}"; do
    first_hash="$(cut -d ' ' -f 1 "${EVIDENCE_DIR}/first/${service}.sha256")"
    second_hash="$(cut -d ' ' -f 1 "${EVIDENCE_DIR}/second/${service}.sha256")"
    if [[ "${first_hash}" != "${second_hash}" ]]; then
        printf '%s image is not reproducible: %s != %s\n' \
            "${service}" "${first_hash}" "${second_hash}" >&2
        exit 1
    fi
    printf '%s %s\n' "${service}" "${first_hash}"
done | tee "${EVIDENCE_DIR}/summary.txt"
