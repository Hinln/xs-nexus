#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
GRYPE_VERSION=0.116.1
GRYPE_ARCHIVE_SHA256=0122df7b655981abe547ad3d2190d65551dac6a2bfc80b4dc2a989b5d0587458
GRYPE_URL="https://github.com/anchore/grype/releases/download/v${GRYPE_VERSION}/grype_${GRYPE_VERSION}_linux_amd64.tar.gz"

if [[ $# -ne 1 ]]; then
    printf 'usage: %s EVIDENCE_DIRECTORY\n' "$0" >&2
    exit 2
fi

EVIDENCE_DIR=$(realpath -e -- "$1")
case "$EVIDENCE_DIR" in
    "$ROOT_DIR"/artifacts/qa/*) ;;
    *)
        printf 'evidence directory must be inside %s/artifacts/qa\n' "$ROOT_DIR" >&2
        exit 2
        ;;
esac
MANIFEST="$EVIDENCE_DIR/output/manifest.json"
OUTPUT_DIR="$EVIDENCE_DIR/vulnerabilities"
TEMPORARY=$(mktemp -d "$EVIDENCE_DIR/.vulnerability-scan.XXXXXX")
STAGING="$EVIDENCE_DIR/.vulnerabilities.staging-$$"
FIREWALL_BEFORE="$TEMPORARY/nftables-before.json"
FIREWALL_AFTER="$TEMPORARY/nftables-after.json"

cleanup() {
    local status=$?
    rm -rf -- "$TEMPORARY" "$STAGING"
    exit "$status"
}
trap cleanup EXIT INT TERM

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

[[ -f $MANIFEST ]]
[[ ! -e $OUTPUT_DIR && ! -e $STAGING ]]
mkdir -m 0700 "$STAGING"

docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_before=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_BEFORE"

curl --fail --silent --show-error --location --output "$TEMPORARY/grype.tar.gz" "$GRYPE_URL"
printf '%s  %s\n' "$GRYPE_ARCHIVE_SHA256" "$TEMPORARY/grype.tar.gz" | sha256sum --check
tar --extract --gzip --file "$TEMPORARY/grype.tar.gz" --directory "$TEMPORARY" grype
chmod 0755 "$TEMPORARY/grype"
"$TEMPORARY/grype" version >"$STAGING/grype-version.txt"

export GRYPE_DB_CACHE_DIR="$TEMPORARY/db"
export GRYPE_CHECK_FOR_APP_UPDATE=false
"$TEMPORARY/grype" db update
"$TEMPORARY/grype" db status >"$STAGING/grype-db-status.txt"

python3 - "$MANIFEST" >"$TEMPORARY/images.tsv" <<'PY'
import json
import re
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert manifest["schema"] == 1
assert re.fullmatch(r"[0-9a-f]{40}", manifest["revision"])
for name, image in sorted(manifest["images"].items()):
    assert re.fullmatch(r"[a-z0-9][a-z0-9_.-]{0,63}", name)
    assert re.fullmatch(r"sha256:[0-9a-f]{64}", image["image_id"])
    reference = image["reference"]
    assert "\t" not in reference and "\n" not in reference
    print(name, reference, image["image_id"], sep="\t")
PY

while IFS=$'\t' read -r name reference expected_id; do
    actual_id=$(docker image inspect "$reference" --format '{{.Id}}')
    [[ $actual_id == "$expected_id" ]]
    "$TEMPORARY/grype" "docker:$reference" --output json --file "$STAGING/$name.grype.json"
done <"$TEMPORARY/images.tsv"

python3 - "$MANIFEST" "$STAGING" "$GRYPE_VERSION" "$GRYPE_ARCHIVE_SHA256" <<'PY'
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

manifest_path = Path(sys.argv[1])
output = Path(sys.argv[2])
tool_version = sys.argv[3]
tool_hash = sys.argv[4]
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
reports = {}
totals = Counter()
fixable = Counter()
for name, image in sorted(manifest["images"].items()):
    path = output / f"{name}.grype.json"
    report = json.loads(path.read_text(encoding="utf-8"))
    matches = report.get("matches")
    if not isinstance(matches, list):
        raise AssertionError(f"invalid Grype report matches for {name}")
    counts = Counter()
    fixed_counts = Counter()
    for match in matches:
        vulnerability = match.get("vulnerability") or {}
        severity = vulnerability.get("severity")
        if severity not in {"Unknown", "Negligible", "Low", "Medium", "High", "Critical"}:
            raise AssertionError(f"invalid Grype severity for {name}: {severity}")
        counts[severity] += 1
        fix = vulnerability.get("fix") or {}
        if fix.get("state") == "fixed" and fix.get("versions"):
            fixed_counts[severity] += 1
    totals.update(counts)
    fixable.update(fixed_counts)
    value = path.read_bytes()
    reports[name] = {
        "image_id": image["image_id"],
        "reference": image["reference"],
        "report": path.name,
        "sha256": hashlib.sha256(value).hexdigest(),
        "findings": dict(sorted(counts.items())),
        "fixable_findings": dict(sorted(fixed_counts.items())),
    }
summary = {
    "schema": 1,
    "source_manifest": {
        "path": str(manifest_path),
        "sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "revision": manifest["revision"],
    },
    "scanner": {
        "name": "grype",
        "version": tool_version,
        "archive_sha256": tool_hash,
        "version_output_sha256": hashlib.sha256((output / "grype-version.txt").read_bytes()).hexdigest(),
        "database_status_sha256": hashlib.sha256((output / "grype-db-status.txt").read_bytes()).hexdigest(),
    },
    "reports": reports,
    "total_findings": dict(sorted(totals.items())),
    "total_fixable_findings": dict(sorted(fixable.items())),
}
(output / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

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

mv -- "$STAGING" "$OUTPUT_DIR"
trap - EXIT INT TERM
rm -rf -- "$TEMPORARY"
printf 'Image vulnerability reports written to %s\n' "$OUTPUT_DIR"
