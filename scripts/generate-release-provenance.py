#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
import re
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit


SEMVER_RE = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
GIT_RE = re.compile(r"[0-9a-f]{40}")
DIGEST_RE = re.compile(r"sha256:[0-9a-f]{64}")
NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}")
TOKEN_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}")
BUILD_INPUT_NAME_RE = re.compile(r"[a-z0-9][a-z0-9._-]{0,63}")
BUILD_INPUT_VALUE_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9./:@_+=,-]{0,255}")
MAX_DOCUMENT_BYTES = 64 * 1024 * 1024
MAX_ARTIFACT_BYTES = 1024 * 1024 * 1024
MAX_SOURCE_DATE_EPOCH = 253402300799


class ValidationError(Exception):
    pass


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args()


def read_json(path: Path, maximum: int):
    data = read_regular(path, maximum)
    try:
        return json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid JSON: {path}") from error


def read_regular(path: Path, maximum: int) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"expected regular non-symlink file: {path}")
    size = path.stat().st_size
    if size <= 0 or size > maximum:
        raise ValidationError(f"file size is invalid: {path}")
    value = path.read_bytes()
    if len(value) != size:
        raise ValidationError(f"file changed while reading: {path}")
    return value


def exact_keys(value, expected, label):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValidationError(f"{label} fields are invalid")


def resolve_input(base: Path, value, label: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValidationError(f"{label} path is invalid")
    path = Path(value)
    if not path.is_absolute():
        path = base / path
    path = Path(os.path.abspath(path))
    if path.is_symlink() or not path.exists():
        raise ValidationError(f"{label} path is missing or a symlink")
    return path


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def write_json(path: Path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.chmod(path, 0o644)


def copy_input(source: Path, destination: Path, maximum: int):
    value = read_regular(source, maximum)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(value)
    os.chmod(destination, 0o644)
    return {"path": destination.name, "size": len(value), "sha256": sha256_bytes(value)}


def valid_source_url(value) -> bool:
    if not isinstance(value, str):
        return False
    parsed = urlsplit(value)
    return (
        parsed.scheme == "https"
        and parsed.hostname is not None
        and parsed.username is None
        and parsed.password is None
        and not parsed.query
        and not parsed.fragment
    )


def validate_inventory(value, base: Path):
    expected = {
        "schema_version",
        "product",
        "version",
        "revision",
        "tag",
        "source_url",
        "source_date_epoch",
        "protocol_version",
        "build_inputs",
        "sbom_spdx",
        "third_party",
        "release_notes",
        "artifacts",
        "images",
    }
    exact_keys(value, expected, "inventory")
    if value["schema_version"] != 1 or value["product"] != "xs-nexus":
        raise ValidationError("inventory identity is invalid")
    version = value["version"]
    revision = value["revision"]
    epoch = value["source_date_epoch"]
    if not isinstance(version, str) or not SEMVER_RE.fullmatch(version):
        raise ValidationError("release version is invalid")
    if not isinstance(revision, str) or not GIT_RE.fullmatch(revision):
        raise ValidationError("release revision is invalid")
    if value["tag"] != f"v{version}":
        raise ValidationError("release tag must exactly match the version")
    if not valid_source_url(value["source_url"]):
        raise ValidationError("source URL must use HTTPS")
    if (
        not isinstance(epoch, int)
        or isinstance(epoch, bool)
        or epoch < 0
        or epoch > MAX_SOURCE_DATE_EPOCH
    ):
        raise ValidationError("source date epoch is invalid")
    if value["protocol_version"] != "XSP/1":
        raise ValidationError("protocol version is invalid")
    build_inputs = value["build_inputs"]
    if not isinstance(build_inputs, list) or not build_inputs:
        raise ValidationError("at least one build input is required")
    normalized_build_inputs = []
    build_input_names = set()
    for index, build_input in enumerate(build_inputs):
        exact_keys(build_input, {"name", "value"}, f"build input {index}")
        name = build_input["name"]
        input_value = build_input["value"]
        if (
            not isinstance(name, str)
            or not BUILD_INPUT_NAME_RE.fullmatch(name)
            or name in build_input_names
            or any(marker in name for marker in ("password", "secret", "token", "private-key"))
        ):
            raise ValidationError(f"build input {index} name is invalid or duplicated")
        if not isinstance(input_value, str) or not BUILD_INPUT_VALUE_RE.fullmatch(input_value):
            raise ValidationError(f"build input {index} value is invalid")
        build_input_names.add(name)
        normalized_build_inputs.append({"name": name, "value": input_value})
    normalized_build_inputs.sort(key=lambda item: item["name"])
    documents = {
        "SBOM.spdx.json": resolve_input(base, value["sbom_spdx"], "SBOM"),
        "THIRD_PARTY.md": resolve_input(base, value["third_party"], "third-party"),
        "release-notes.md": resolve_input(base, value["release_notes"], "release notes"),
    }
    artifacts = value["artifacts"]
    images = value["images"]
    if not isinstance(artifacts, list) or not artifacts:
        raise ValidationError("at least one artifact is required")
    if not isinstance(images, list) or not images:
        raise ValidationError("at least one image is required")
    normalized_artifacts = []
    names = set()
    for index, artifact in enumerate(artifacts):
        exact_keys(
            artifact,
            {"component", "platform", "architecture", "name", "path"},
            f"artifact {index}",
        )
        if not all(
            isinstance(artifact[key], str) and TOKEN_RE.fullmatch(artifact[key])
            for key in ("component", "platform", "architecture")
        ):
            raise ValidationError(f"artifact {index} identity is invalid")
        name = artifact["name"]
        if not isinstance(name, str) or not NAME_RE.fullmatch(name) or name in names:
            raise ValidationError(f"artifact {index} name is invalid or duplicated")
        names.add(name)
        normalized_artifacts.append(
            {
                **{key: artifact[key] for key in ("component", "platform", "architecture", "name")},
                "source_path": resolve_input(base, artifact["path"], f"artifact {index}"),
            }
        )
    normalized_images = []
    components = set()
    for index, image in enumerate(images):
        exact_keys(image, {"component", "reference", "digest", "image_id"}, f"image {index}")
        component = image["component"]
        if not isinstance(component, str) or not TOKEN_RE.fullmatch(component) or component in components:
            raise ValidationError(f"image {index} component is invalid or duplicated")
        if not isinstance(image["reference"], str) or "@sha256:" not in image["reference"]:
            raise ValidationError(f"image {index} reference must be digest-pinned")
        if not isinstance(image["digest"], str) or not DIGEST_RE.fullmatch(image["digest"]):
            raise ValidationError(f"image {index} digest is invalid")
        if not image["reference"].endswith(f"@{image['digest']}"):
            raise ValidationError(f"image {index} reference and digest differ")
        if not isinstance(image["image_id"], str) or not DIGEST_RE.fullmatch(image["image_id"]):
            raise ValidationError(f"image {index} image ID is invalid")
        components.add(component)
        normalized_images.append(dict(image))
    return value, documents, normalized_build_inputs, normalized_artifacts, normalized_images


def publish(source: Path, destination: Path):
    destination = destination.resolve(strict=False)
    if destination.exists() or destination.is_symlink():
        raise ValidationError("output directory already exists")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{destination.name}-", dir=destination.parent))
    try:
        shutil.copytree(source, staging / "bundle")
        os.replace(staging / "bundle", destination)
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def main():
    args = parse_args()
    inventory_path = Path(os.path.abspath(args.inventory))
    inventory, documents, build_inputs, artifacts, images = validate_inventory(
        read_json(inventory_path, MAX_DOCUMENT_BYTES), inventory_path.parent
    )
    temporary = Path(tempfile.mkdtemp(prefix="xs-nexus-release-"))
    try:
        output = temporary / "output"
        output.mkdir()
        materials = {}
        for name, source in documents.items():
            materials[name] = copy_input(source, output / name, MAX_DOCUMENT_BYTES)
        artifact_records = []
        for artifact in artifacts:
            destination = output / "artifacts" / artifact["name"]
            value = read_regular(artifact["source_path"], MAX_ARTIFACT_BYTES)
            destination.parent.mkdir(exist_ok=True)
            destination.write_bytes(value)
            os.chmod(destination, 0o644)
            artifact_records.append(
                {
                    **{key: artifact[key] for key in ("component", "platform", "architecture", "name")},
                    "path": f"artifacts/{artifact['name']}",
                    "size": len(value),
                    "sha256": sha256_bytes(value),
                }
            )
        artifact_records.sort(key=lambda item: item["name"])
        images.sort(key=lambda item: item["component"])
        timestamp = datetime.fromtimestamp(
            inventory["source_date_epoch"], timezone.utc
        ).isoformat().replace("+00:00", "Z")
        manifest = {
            "schema_version": 1,
            "product": "xs-nexus",
            "version": inventory["version"],
            "revision": inventory["revision"],
            "tag": inventory["tag"],
            "source_url": inventory["source_url"],
            "generated_at": timestamp,
            "source_date_epoch": inventory["source_date_epoch"],
            "protocol_version": "XSP/1",
            "build_inputs": build_inputs,
            "artifacts": artifact_records,
            "images": images,
            "materials": materials,
        }
        write_json(output / "manifest.json", manifest)
        subjects = [
            {"name": item["path"], "digest": {"sha256": item["sha256"]}}
            for item in artifact_records
        ]
        subjects.extend(
            {
                "name": image["reference"],
                "digest": {"sha256": image["digest"].removeprefix("sha256:")},
            }
            for image in images
        )
        provenance = {
            "_type": "https://in-toto.io/Statement/v1",
            "subject": subjects,
            "predicateType": "https://slsa.dev/provenance/v1",
            "predicate": {
                "buildDefinition": {
                    "buildType": "https://xs-nexus.invalid/build/release-bundle/v1",
                    "externalParameters": {
                        "version": inventory["version"],
                        "tag": inventory["tag"],
                        "source_date_epoch": inventory["source_date_epoch"],
                        "build_inputs": build_inputs,
                    },
                    "resolvedDependencies": [
                        {
                            "uri": f"git+{inventory['source_url']}@{inventory['revision']}",
                            "digest": {"gitCommit": inventory["revision"]},
                        }
                    ],
                },
                "runDetails": {
                    "builder": {"id": "xs-nexus-release-provenance-v1"},
                    "metadata": {"startedOn": timestamp, "finishedOn": timestamp},
                },
            },
        }
        write_json(output / "provenance.json", provenance)
        sum_paths = sorted(
            path for path in output.rglob("*") if path.is_file() and path.name != "SHA256SUMS"
        )
        lines = [
            f"{sha256_bytes(path.read_bytes())}  {path.relative_to(output).as_posix()}"
            for path in sum_paths
        ]
        (output / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
        os.chmod(output / "SHA256SUMS", 0o644)
        publish(output, args.output_dir)
    finally:
        shutil.rmtree(temporary, ignore_errors=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValidationError as error:
        print(f"release provenance generation failed: {error}", file=os.sys.stderr)
        raise SystemExit(1)
