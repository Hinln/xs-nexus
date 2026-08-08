#!/usr/bin/env python3

import argparse
import base64
import binascii
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
import urllib.parse
import uuid
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROJECT_NAME = "xs-nexus"
PROJECT_VERSION = "0.1.0"
DEFAULT_NPM_LICENSES = ROOT / "supply-chain" / "npm-licenses.json"
ALLOWED_LICENSES = {
    "0BSD",
    "Apache-2.0",
    "BSD-1-Clause",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MPL-2.0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
}
ALLOWED_EXCEPTIONS = {"LLVM-exception"}
FORBIDDEN_DEPENDENCIES = {
    "coturn",
    "frp",
    "headscale",
    "nebula",
    "netbird",
    "nps",
    "openvpn",
    "rathole",
    "softether",
    "tailscale",
    "tap-windows",
    "wintun",
    "wireguard",
    "zerotier",
}
TOKEN_RE = re.compile(r"\s*(AND|OR|WITH|\(|\)|[A-Za-z0-9.+-]+)")


class ValidationError(RuntimeError):
    pass


def normalize_license_expression(expression):
    return expression.replace("Apache-2.0/MIT", "Apache-2.0 OR MIT").replace(
        "MIT/Apache-2.0", "MIT OR Apache-2.0"
    )


class LicenseParser:
    def __init__(self, expression):
        normalized = normalize_license_expression(expression)
        self.tokens = self._tokenize(normalized)
        self.position = 0

    @staticmethod
    def _tokenize(expression):
        tokens = []
        position = 0
        while position < len(expression):
            match = TOKEN_RE.match(expression, position)
            if match is None:
                raise ValidationError(f"invalid license expression near {expression[position:]!r}")
            tokens.append(match.group(1))
            position = match.end()
        if not tokens:
            raise ValidationError("empty license expression")
        return tokens

    def parse(self):
        approved = self._parse_or()
        if self.position != len(self.tokens):
            raise ValidationError(f"unexpected license token {self.tokens[self.position]!r}")
        return approved

    def _accept(self, token):
        if self.position < len(self.tokens) and self.tokens[self.position] == token:
            self.position += 1
            return True
        return False

    def _parse_or(self):
        approved = self._parse_and()
        while self._accept("OR"):
            alternative = self._parse_and()
            approved = approved or alternative
        return approved

    def _parse_and(self):
        approved = self._parse_primary()
        while self._accept("AND"):
            required = self._parse_primary()
            approved = approved and required
        return approved

    def _parse_primary(self):
        if self._accept("("):
            approved = self._parse_or()
            if not self._accept(")"):
                raise ValidationError("unclosed license expression")
            return approved
        if self.position >= len(self.tokens):
            raise ValidationError("missing license identifier")
        identifier = self.tokens[self.position]
        self.position += 1
        if identifier in {"AND", "OR", "WITH", ")"}:
            raise ValidationError(f"expected license identifier, got {identifier!r}")
        approved = identifier in ALLOWED_LICENSES
        if self._accept("WITH"):
            if self.position >= len(self.tokens):
                raise ValidationError("missing license exception")
            exception = self.tokens[self.position]
            self.position += 1
            approved = approved and exception in ALLOWED_EXCEPTIONS
        return approved


def parse_args():
    parser = argparse.ArgumentParser(description="Generate deterministic source dependency SBOMs")
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--source-date-epoch", required=True, type=int)
    parser.add_argument("--npm-license-snapshot", type=Path, default=DEFAULT_NPM_LICENSES)
    return parser.parse_args()


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"cannot read JSON {path}: {error}") from error


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def sha256_file(path):
    return sha256_bytes(path.read_bytes())


def check_dependency_name(name):
    normalized = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    segments = normalized.split("-")
    for forbidden in FORBIDDEN_DEPENDENCIES:
        forbidden_segments = forbidden.split("-")
        width = len(forbidden_segments)
        for offset in range(len(segments) - width + 1):
            if segments[offset : offset + width] == forbidden_segments:
                raise ValidationError(f"forbidden dependency detected: {name}")


def check_license(expression, package_key):
    if not isinstance(expression, str) or not expression.strip():
        raise ValidationError(f"missing license for {package_key}")
    if not LicenseParser(expression.strip()).parse():
        raise ValidationError(f"unapproved license for {package_key}: {expression}")
    return normalize_license_expression(expression.strip())


def cargo_components():
    lock_path = ROOT / "Cargo.lock"
    try:
        lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValidationError(f"cannot read Cargo.lock: {error}") from error
    locked = {}
    for package in lock.get("package", []):
        if "source" not in package:
            continue
        key = (package["name"], package["version"], package["source"])
        checksum = package.get("checksum")
        if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
            raise ValidationError(f"missing Cargo checksum for {package['name']}@{package['version']}")
        locked[key] = checksum
    command = ["cargo", "metadata", "--locked", "--format-version", "1"]
    environment = os.environ.copy()
    environment["CARGO_NET_OFFLINE"] = "true"
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            check=True,
            capture_output=True,
            env=environment,
            text=True,
        )
        metadata = json.loads(completed.stdout)
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() if error.stderr else str(error)
        raise ValidationError(f"cargo metadata failed: {detail}") from error
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"cargo metadata failed: {error}") from error
    components = []
    seen = set()
    for package in metadata.get("packages", []):
        source = package.get("source")
        if source is None:
            continue
        name = package["name"]
        version = package["version"]
        key = (name, version, source)
        checksum = locked.get(key)
        if checksum is None:
            raise ValidationError(f"cargo metadata package is not checksummed in Cargo.lock: {name}@{version}")
        license_expression = package.get("license")
        package_key = f"{name}@{version}"
        check_dependency_name(name)
        license_expression = check_license(license_expression, package_key)
        purl = f"pkg:cargo/{urllib.parse.quote(name, safe='')}@{urllib.parse.quote(version, safe='') }"
        if purl in seen:
            raise ValidationError(f"duplicate Cargo component reference: {purl}")
        seen.add(purl)
        components.append(
            {
                "ecosystem": "cargo",
                "name": name,
                "version": version,
                "license": license_expression,
                "purl": purl,
                "hash_algorithm": "SHA-256",
                "hash": checksum,
            }
        )
    if len(components) != len(locked):
        raise ValidationError(
            f"Cargo.lock/metadata component count mismatch: lock={len(locked)} metadata={len(components)}"
        )
    return sorted(components, key=lambda component: component["purl"])


def npm_license_map(snapshot_path):
    snapshot = read_json(snapshot_path)
    if snapshot.get("schema") != 1 or snapshot.get("source") != "https://registry.npmjs.org":
        raise ValidationError("invalid npm license snapshot metadata")
    groups = snapshot.get("licenses")
    if not isinstance(groups, dict):
        raise ValidationError("npm license snapshot licenses must be an object")
    result = {}
    for license_expression, packages in groups.items():
        if not isinstance(packages, list) or packages != sorted(packages):
            raise ValidationError(f"npm license group must be a sorted list: {license_expression}")
        for package_key in packages:
            if package_key in result:
                raise ValidationError(f"duplicate npm license snapshot package: {package_key}")
            result[package_key] = check_license(license_expression, package_key)
    return result


def npm_components(snapshot_path):
    lock_path = ROOT / "package-lock.json"
    lock = read_json(lock_path)
    if lock.get("lockfileVersion") != 3:
        raise ValidationError("package-lock.json must use lockfileVersion 3")
    packages = {}
    for path, metadata in lock.get("packages", {}).items():
        if not path or "node_modules/" not in path or metadata.get("link") is True:
            continue
        name = path.rsplit("node_modules/", 1)[1]
        version = metadata.get("version")
        integrity = metadata.get("integrity")
        if not isinstance(version, str) or not isinstance(integrity, str):
            raise ValidationError(f"incomplete npm lock entry: {path}")
        package_key = f"{name}@{version}"
        previous = packages.setdefault(package_key, integrity)
        if previous != integrity:
            raise ValidationError(f"conflicting npm integrity for {package_key}")
    licenses = npm_license_map(snapshot_path)
    if set(packages) != set(licenses):
        missing = sorted(set(packages) - set(licenses))
        stale = sorted(set(licenses) - set(packages))
        raise ValidationError(f"npm license snapshot mismatch: missing={missing} stale={stale}")
    components = []
    for package_key in sorted(packages):
        name, version = package_key.rsplit("@", 1)
        check_dependency_name(name)
        algorithm, separator, encoded = packages[package_key].partition("-")
        if separator != "-" or algorithm.lower() != "sha512":
            raise ValidationError(f"npm integrity must use SHA-512: {package_key}")
        try:
            digest = base64.b64decode(encoded, validate=True).hex()
        except (binascii.Error, ValueError) as error:
            raise ValidationError(f"invalid npm integrity for {package_key}") from error
        if len(digest) != 128:
            raise ValidationError(f"invalid npm SHA-512 length for {package_key}")
        if name.startswith("@"):
            scope, unscoped = name[1:].split("/", 1)
            purl_name = f"{urllib.parse.quote(scope, safe='')}/{urllib.parse.quote(unscoped, safe='')}"
        else:
            purl_name = urllib.parse.quote(name, safe="")
        purl = f"pkg:npm/{purl_name}@{urllib.parse.quote(version, safe='')}"
        components.append(
            {
                "ecosystem": "npm",
                "name": name,
                "version": version,
                "license": licenses[package_key],
                "purl": purl,
                "hash_algorithm": "SHA-512",
                "hash": digest,
            }
        )
    return sorted(components, key=lambda component: component["purl"])


def cyclonedx_document(components, timestamp, serial_number):
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": serial_number,
        "version": 1,
        "metadata": {
            "timestamp": timestamp,
            "component": {
                "type": "application",
                "bom-ref": f"pkg:generic/{PROJECT_NAME}@{PROJECT_VERSION}",
                "name": PROJECT_NAME,
                "version": PROJECT_VERSION,
                "purl": f"pkg:generic/{PROJECT_NAME}@{PROJECT_VERSION}",
            },
            "tools": {
                "components": [
                    {
                        "type": "application",
                        "name": "generate-source-sbom.py",
                        "version": "1",
                    }
                ]
            },
        },
        "components": [
            {
                "type": "library",
                "bom-ref": component["purl"],
                "name": component["name"],
                "version": component["version"],
                "purl": component["purl"],
                "licenses": [{"expression": component["license"]}],
                "hashes": [
                    {
                        "alg": component["hash_algorithm"],
                        "content": component["hash"],
                    }
                ],
                "properties": [
                    {"name": "xs-nexus:ecosystem", "value": component["ecosystem"]}
                ],
            }
            for component in components
        ],
    }


def spdx_document(components, timestamp, namespace):
    root_id = "SPDXRef-Package-xs-nexus"
    packages = [
        {
            "SPDXID": root_id,
            "name": PROJECT_NAME,
            "versionInfo": PROJECT_VERSION,
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
            "primaryPackagePurpose": "APPLICATION",
        }
    ]
    relationships = []
    for index, component in enumerate(components, start=1):
        package_id = f"SPDXRef-Package-{index:04d}"
        packages.append(
            {
                "SPDXID": package_id,
                "name": component["name"],
                "versionInfo": component["version"],
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
                "licenseConcluded": component["license"],
                "licenseDeclared": component["license"],
                "copyrightText": "NOASSERTION",
                "checksums": [
                    {
                        "algorithm": component["hash_algorithm"].replace("-", ""),
                        "checksumValue": component["hash"],
                    }
                ],
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceType": "purl",
                        "referenceLocator": component["purl"],
                    }
                ],
                "primaryPackagePurpose": "LIBRARY",
            }
        )
        relationships.append(
            {
                "spdxElementId": root_id,
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": package_id,
            }
        )
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{PROJECT_NAME}-source-dependencies",
        "documentNamespace": namespace,
        "creationInfo": {
            "created": timestamp,
            "creators": ["Tool: generate-source-sbom.py-1"],
        },
        "documentDescribes": [root_id],
        "packages": packages,
        "relationships": relationships,
    }


def validate_output_path(raw_output):
    if not raw_output.is_absolute():
        raise ValidationError("output directory must be absolute")
    if os.path.lexists(raw_output):
        raise ValidationError("output directory must not already exist")
    parent = raw_output.parent
    if not parent.is_dir():
        raise ValidationError("output directory parent must exist")
    if parent.is_symlink() or parent.resolve() != parent:
        raise ValidationError("output directory parent must be a real path")
    return raw_output


def main():
    args = parse_args()
    try:
        if args.source_date_epoch < 0:
            raise ValidationError("source-date-epoch must be nonnegative")
        output = validate_output_path(args.output_dir)
        snapshot = args.npm_license_snapshot.resolve(strict=True)
        if not snapshot.is_file():
            raise ValidationError("npm license snapshot must be a file")
        components = cargo_components() + npm_components(snapshot)
        components.sort(key=lambda component: component["purl"])
        references = [component["purl"] for component in components]
        if len(references) != len(set(references)):
            raise ValidationError("duplicate component references across ecosystems")
        timestamp = datetime.fromtimestamp(args.source_date_epoch, timezone.utc).strftime(
            "%Y-%m-%dT%H:%M:%SZ"
        )
        input_hashes = {
            "Cargo.lock": sha256_file(ROOT / "Cargo.lock"),
            "package-lock.json": sha256_file(ROOT / "package-lock.json"),
            "supply-chain/npm-licenses.json": sha256_file(snapshot),
        }
        identity = json.dumps(input_hashes, sort_keys=True, separators=(",", ":"))
        document_uuid = uuid.uuid5(uuid.NAMESPACE_URL, f"https://xs-nexus.invalid/sbom/{identity}")
        output.mkdir(mode=0o750)
        cdx_path = output / "xs-nexus-source.cdx.json"
        spdx_path = output / "xs-nexus-source.spdx.json"
        manifest_path = output / "manifest.json"
        write_json(cdx_path, cyclonedx_document(components, timestamp, f"urn:uuid:{document_uuid}"))
        write_json(
            spdx_path,
            spdx_document(
                components,
                timestamp,
                f"https://xs-nexus.invalid/spdx/{document_uuid}",
            ),
        )
        cargo_count = sum(component["ecosystem"] == "cargo" for component in components)
        npm_count = sum(component["ecosystem"] == "npm" for component in components)
        write_json(
            manifest_path,
            {
                "schema": 1,
                "project": f"{PROJECT_NAME}@{PROJECT_VERSION}",
                "created": timestamp,
                "scope": "source dependency locks only; container operating-system packages are excluded",
                "network_required": False,
                "component_counts": {
                    "cargo": cargo_count,
                    "npm": npm_count,
                    "total": len(components),
                },
                "inputs": input_hashes,
                "outputs": {
                    cdx_path.name: sha256_file(cdx_path),
                    spdx_path.name: sha256_file(spdx_path),
                },
            },
        )
        print(f"generated {len(components)} source dependency components in {output}")
    except ValidationError as error:
        print(f"source SBOM validation failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
