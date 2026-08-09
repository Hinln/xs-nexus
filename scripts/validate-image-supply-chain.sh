#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/image-supply-chain-$STAMP"
LOG_FILE="$QA_DIR/validate-image-supply-chain.log"
SUMMARY_FILE="$QA_DIR/summary.txt"
OUTPUT_DIR="$QA_DIR/output"
TEMPORARY=$(mktemp -d)
SECOND_OUTPUT="$TEMPORARY/output"
FIREWALL_BEFORE="$TEMPORARY/nftables-before.json"
FIREWALL_AFTER="$TEMPORARY/nftables-after.json"

cleanup() {
    local status=$?
    rm -rf -- "$TEMPORARY"
    printf 'validation_status=%s\n' "$status" | tee "$SUMMARY_FILE"
    printf 'evidence=%s\n' "$QA_DIR" | tee -a "$SUMMARY_FILE"
}
trap cleanup EXIT

snapshot_firewall() {
    nft -j list ruleset | python3 -c '
import json
import sys


def normalize(value):
    if isinstance(value, dict):
        normalized = {key: normalize(item) for key, item in value.items()}
        counter = normalized.get("counter")
        if isinstance(counter, dict):
            counter.pop("packets", None)
            counter.pop("bytes", None)
        return normalized
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


json.dump(normalize(json.load(sys.stdin)), sys.stdout, sort_keys=True, separators=(",", ":"))
'
}

mkdir -p "$QA_DIR"
cd "$ROOT_DIR"
exec > >(tee "$LOG_FILE") 2>&1

[[ -z $(git status --short) ]]
revision=$(git rev-parse HEAD)
short_revision=${revision:0:12}
source_date_epoch=$(git show -s --format=%ct HEAD)
version=0.1.0
source_url=https://github.com/Hinln/xs-nexus
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_before=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_BEFORE"

printf 'Image supply-chain validation started at %s\n' "$(date -u --iso-8601=seconds)"
printf 'git_head=%s\n' "$revision"

make test-image-sbom
docker build --pull=false --build-arg "VCS_REF=$revision" --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch" --build-arg "XS_VERSION=$version" --build-arg "XS_SOURCE_URL=$source_url" \
    --tag "xs-nexus/controller:sbom-$short_revision" \
    --file deploy/docker/controller.Dockerfile .
docker build --pull=false --build-arg "VCS_REF=$revision" --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch" --build-arg "XS_VERSION=$version" --build-arg "XS_SOURCE_URL=$source_url" \
    --tag "xs-nexus/relay:sbom-$short_revision" \
    --file deploy/docker/relay.Dockerfile .
docker build --pull=false --build-arg "VCS_REF=$revision" --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch" --build-arg "XS_VERSION=$version" --build-arg "XS_SOURCE_URL=$source_url" \
    --tag "xs-nexus/console:sbom-$short_revision" \
    --file deploy/docker/console.Dockerfile .
docker build --pull=false --build-arg "VCS_REF=$revision" --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch" --build-arg "XS_VERSION=$version" --build-arg "XS_SOURCE_URL=$source_url" \
    --tag "xs-nexus/db-tools:sbom-$short_revision" \
    --file deploy/docker/db-tools.Dockerfile .
docker build --pull=false --build-arg "VCS_REF=$revision" --build-arg "SOURCE_DATE_EPOCH=$source_date_epoch" --build-arg "XS_VERSION=$version" --build-arg "XS_SOURCE_URL=$source_url" \
    --tag "xs-nexus/edge:sbom-$short_revision" \
    --file deploy/docker/edge.Dockerfile .

generator_arguments=(
    --source-date-epoch "$source_date_epoch"
    --revision "$revision"
    --image "controller=xs-nexus/controller:sbom-$short_revision"
    --dockerfile controller=deploy/docker/controller.Dockerfile
    --image "relay=xs-nexus/relay:sbom-$short_revision"
    --dockerfile relay=deploy/docker/relay.Dockerfile
    --image "console=xs-nexus/console:sbom-$short_revision"
    --dockerfile console=deploy/docker/console.Dockerfile
    --image "db-tools=xs-nexus/db-tools:sbom-$short_revision"
    --dockerfile db-tools=deploy/docker/db-tools.Dockerfile
    --image "edge=xs-nexus/edge:sbom-$short_revision"
    --dockerfile edge=deploy/docker/edge.Dockerfile
)
./scripts/generate-image-sbom.py --output-dir "$OUTPUT_DIR" "${generator_arguments[@]}"
./scripts/generate-image-sbom.py --output-dir "$SECOND_OUTPUT" "${generator_arguments[@]}"
diff --recursive --no-dereference "$OUTPUT_DIR" "$SECOND_OUTPUT"

python3 - "$OUTPUT_DIR" "$revision" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
revision = sys.argv[2]
manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
cyclonedx = json.loads((root / "xs-nexus-images.cdx.json").read_text(encoding="utf-8"))
provenance = json.loads((root / "xs-nexus-images.provenance.json").read_text(encoding="utf-8"))
assert manifest["schema"] == 1
assert manifest["revision"] == revision
assert manifest["network_required"] is False
assert set(manifest["images"]) == {"controller", "relay", "console", "db-tools", "edge"}
assert all(value["package_count"] > 0 for value in manifest["images"].values())
assert all(value["version"] == "0.1.0" for value in manifest["images"].values())
assert all(
    value["source"] == "https://github.com/Hinln/xs-nexus"
    for value in manifest["images"].values()
)
assert all(value["license_materials"] for value in manifest["images"].values())
assert all(
    value["license_closure_count"] == value["package_count"]
    for value in manifest["images"].values()
)
assert cyclonedx["bomFormat"] == "CycloneDX"
assert cyclonedx["specVersion"] == "1.6"
assert len(cyclonedx["components"]) == sum(
    value["package_count"] for value in manifest["images"].values()
)
assert provenance["predicateType"] == "https://slsa.dev/provenance/v1"
assert len(provenance["subject"]) == 5
for image in manifest["images"].values():
    assert image["image_id"].startswith("sha256:")
    dockerfile = Path(image["dockerfile"]["path"])
    assert hashlib.sha256(dockerfile.read_bytes()).hexdigest() == image["dockerfile"]["sha256"]
    material_by_rootfs_path = {
        material["rootfs_path"]: material for material in image["license_materials"]
    }
    assert len(material_by_rootfs_path) == len(image["license_materials"])
    closure_packages = {entry["package"] for entry in image["license_closure"]}
    assert len(closure_packages) == image["package_count"]
    for entry in image["license_closure"]:
        assert entry["status"] in {
            "package-copyright",
            "package-license",
            "spdx-license-text",
            "public-domain-declaration",
            "virtual",
        }
        for rootfs_path in entry["materials"]:
            material = material_by_rootfs_path[rootfs_path]
            material_path = root / material["path"]
            assert material_path.is_file() and not material_path.is_symlink()
            assert material_path.stat().st_size == material["size"]
            assert hashlib.sha256(material_path.read_bytes()).hexdigest() == material["sha256"]
PY

if ./scripts/generate-image-sbom.py \
    --output-dir "$TEMPORARY/rejected" \
    --source-date-epoch "$source_date_epoch" \
    --revision 0000000000000000000000000000000000000000 \
    --image "controller=xs-nexus/controller:sbom-$short_revision" \
    --dockerfile controller=deploy/docker/controller.Dockerfile >/dev/null 2>&1; then
    printf 'image SBOM generator accepted a revision-label mismatch\n' >&2
    exit 1
fi

docker_after=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_after=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_after=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_after=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_AFTER"
[[ "$docker_before" == "$docker_after" ]]
[[ "$docker_networks_before" == "$docker_networks_after" ]]
[[ "$network_members_before" == "$network_members_after" ]]
[[ "$default_routes_before" == "$default_routes_after" ]]
cmp -s "$FIREWALL_BEFORE" "$FIREWALL_AFTER"
if systemctl --failed --no-legend | grep -q '[^[:space:]]'; then
    systemctl --failed --no-pager >&2
    exit 1
fi

printf 'Image supply-chain validation passed at %s\n' "$(date -u --iso-8601=seconds)"
