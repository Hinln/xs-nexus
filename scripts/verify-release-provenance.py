#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit


GIT_RE = re.compile(r"[0-9a-f]{40}")
DIGEST_RE = re.compile(r"sha256:[0-9a-f]{64}")
HASH_RE = re.compile(r"[0-9a-f]{64}")
SEMVER_RE = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}")
TOKEN_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}")
BUILD_INPUT_NAME_RE = re.compile(r"[a-z0-9][a-z0-9._-]{0,63}")
BUILD_INPUT_VALUE_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9./:@_+=,-]{0,255}")
MAX_FILE_BYTES = 1024 * 1024 * 1024
MAX_SOURCE_DATE_EPOCH = 253402300799


class ValidationError(Exception):
    pass


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--public-key", type=Path, required=True)
    parser.add_argument("--expected-revision", required=True)
    parser.add_argument("--expected-tag", required=True)
    return parser.parse_args()


def read_regular(path: Path, maximum=MAX_FILE_BYTES) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"expected regular non-symlink file: {path}")
    size = path.stat().st_size
    if size <= 0 or size > maximum:
        raise ValidationError(f"file size is invalid: {path}")
    value = path.read_bytes()
    if len(value) != size:
        raise ValidationError(f"file changed while reading: {path}")
    return value


def load_json(path: Path):
    try:
        return json.loads(read_regular(path, 64 * 1024 * 1024))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid JSON: {path}") from error


def verify_signature(public_key: Path, value: Path, signature: Path):
    completed = subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-verify",
            "-rawin",
            "-pubin",
            "-inkey",
            str(public_key),
            "-in",
            str(value),
            "-sigfile",
            str(signature),
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise ValidationError(f"signature verification failed: {value.name}")


def safe_relative(value: str) -> Path:
    path = Path(value)
    if (
        not value
        or path.is_absolute()
        or "\\" in value
        or any(part in ("", ".", "..") for part in path.parts)
    ):
        raise ValidationError(f"unsafe bundle path: {value}")
    return path


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


def exact_keys(value, expected, label):
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValidationError(f"{label} fields are invalid")


def main():
    args = parse_args()
    bundle = Path(os.path.abspath(args.bundle))
    public_key = Path(os.path.abspath(args.public_key))
    if not GIT_RE.fullmatch(args.expected_revision):
        raise ValidationError("expected revision is invalid")
    if not re.fullmatch(rf"v{SEMVER_RE.pattern}", args.expected_tag):
        raise ValidationError("expected tag is invalid")
    if bundle.is_symlink() or not bundle.is_dir():
        raise ValidationError("bundle must be a real directory")
    read_regular(public_key, 64 * 1024)
    for path in bundle.rglob("*"):
        if path.is_symlink():
            raise ValidationError(f"bundle contains a symlink: {path.relative_to(bundle)}")
    required = {
        "manifest.json",
        "manifest.sig",
        "SHA256SUMS",
        "SHA256SUMS.sig",
        "SBOM.spdx.json",
        "THIRD_PARTY.md",
        "release-notes.md",
        "provenance.json",
    }
    for name in required:
        read_regular(bundle / name)
    if len(read_regular(bundle / "manifest.sig", 1024)) != 64 or len(
        read_regular(bundle / "SHA256SUMS.sig", 1024)
    ) != 64:
        raise ValidationError("Ed25519 signature size is invalid")
    verify_signature(public_key, bundle / "manifest.json", bundle / "manifest.sig")
    verify_signature(public_key, bundle / "SHA256SUMS", bundle / "SHA256SUMS.sig")

    expected_sum_paths = {
        path.relative_to(bundle).as_posix()
        for path in bundle.rglob("*")
        if path.is_file()
        and path.name not in {"manifest.sig", "SHA256SUMS", "SHA256SUMS.sig"}
    }
    observed_sum_paths = set()
    try:
        sum_lines = read_regular(bundle / "SHA256SUMS", 16 * 1024 * 1024).decode(
            "utf-8"
        ).splitlines()
    except UnicodeDecodeError as error:
        raise ValidationError("SHA256SUMS is not UTF-8") from error
    for line in sum_lines:
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        if match is None:
            raise ValidationError("SHA256SUMS format is invalid")
        expected_hash, name = match.groups()
        relative = safe_relative(name)
        if name in observed_sum_paths:
            raise ValidationError("SHA256SUMS contains a duplicate path")
        observed_sum_paths.add(name)
        value = read_regular(bundle / relative)
        if hashlib.sha256(value).hexdigest() != expected_hash:
            raise ValidationError(f"bundle hash mismatch: {name}")
    if observed_sum_paths != expected_sum_paths:
        raise ValidationError("SHA256SUMS file set differs from the bundle")

    manifest = load_json(bundle / "manifest.json")
    expected_manifest_keys = {
        "schema_version",
        "product",
        "version",
        "revision",
        "tag",
        "source_url",
        "generated_at",
        "source_date_epoch",
        "protocol_version",
        "build_inputs",
        "artifacts",
        "images",
        "materials",
    }
    exact_keys(manifest, expected_manifest_keys, "manifest")
    if (
        manifest["schema_version"] != 1
        or manifest["product"] != "xs-nexus"
        or manifest["revision"] != args.expected_revision
        or manifest["tag"] != args.expected_tag
        or manifest["protocol_version"] != "XSP/1"
    ):
        raise ValidationError("manifest release identity is invalid")
    if not isinstance(manifest["version"], str) or not SEMVER_RE.fullmatch(
        manifest["version"]
    ):
        raise ValidationError("manifest version is invalid")
    if manifest["tag"] != f"v{manifest['version']}":
        raise ValidationError("manifest tag and version differ")
    if not valid_source_url(manifest["source_url"]):
        raise ValidationError("manifest source URL is invalid")
    epoch = manifest["source_date_epoch"]
    if (
        not isinstance(epoch, int)
        or isinstance(epoch, bool)
        or epoch < 0
        or epoch > MAX_SOURCE_DATE_EPOCH
    ):
        raise ValidationError("manifest source date epoch is invalid")
    timestamp = datetime.fromtimestamp(epoch, timezone.utc).isoformat().replace("+00:00", "Z")
    if manifest["generated_at"] != timestamp:
        raise ValidationError("manifest generation timestamp is invalid")
    build_inputs = manifest["build_inputs"]
    if not isinstance(build_inputs, list) or not build_inputs:
        raise ValidationError("manifest build inputs are invalid")
    build_input_names = set()
    for index, build_input in enumerate(build_inputs):
        exact_keys(build_input, {"name", "value"}, f"build input {index}")
        name = build_input["name"]
        value = build_input["value"]
        if (
            not isinstance(name, str)
            or not BUILD_INPUT_NAME_RE.fullmatch(name)
            or name in build_input_names
            or any(marker in name for marker in ("password", "secret", "token", "private-key"))
            or not isinstance(value, str)
            or not BUILD_INPUT_VALUE_RE.fullmatch(value)
        ):
            raise ValidationError("manifest build input is invalid or duplicated")
        build_input_names.add(name)
    if build_inputs != sorted(build_inputs, key=lambda item: item["name"]):
        raise ValidationError("manifest build inputs are not sorted")
    if not isinstance(manifest["artifacts"], list) or not manifest["artifacts"]:
        raise ValidationError("manifest artifacts are invalid")
    artifact_subjects = {}
    for artifact in manifest["artifacts"]:
        exact_keys(artifact, {
            "component",
            "platform",
            "architecture",
            "name",
            "path",
            "size",
            "sha256",
        }, "manifest artifact")
        if not all(
            isinstance(artifact[key], str) and TOKEN_RE.fullmatch(artifact[key])
            for key in ("component", "platform", "architecture")
        ):
            raise ValidationError("manifest artifact identity is invalid")
        if not isinstance(artifact["name"], str) or not NAME_RE.fullmatch(artifact["name"]):
            raise ValidationError("manifest artifact name is invalid")
        if artifact["path"] != f"artifacts/{artifact['name']}":
            raise ValidationError("manifest artifact path is invalid")
        if artifact["path"] in artifact_subjects:
            raise ValidationError("manifest artifact is duplicated")
        if (
            not isinstance(artifact["size"], int)
            or isinstance(artifact["size"], bool)
            or artifact["size"] <= 0
            or not isinstance(artifact["sha256"], str)
            or not HASH_RE.fullmatch(artifact["sha256"])
        ):
            raise ValidationError("manifest artifact metadata is invalid")
        relative = safe_relative(artifact["path"])
        value = read_regular(bundle / relative)
        if len(value) != artifact["size"] or hashlib.sha256(value).hexdigest() != artifact["sha256"]:
            raise ValidationError(f"manifest artifact mismatch: {artifact['path']}")
        artifact_subjects[artifact["path"]] = artifact["sha256"]
    if not isinstance(manifest["images"], list) or not manifest["images"]:
        raise ValidationError("manifest images are invalid")
    image_subjects = {}
    image_components = set()
    for image in manifest["images"]:
        exact_keys(image, {"component", "reference", "digest", "image_id"}, "manifest image")
        if (
            not isinstance(image["component"], str)
            or not TOKEN_RE.fullmatch(image["component"])
            or image["component"] in image_components
            or not isinstance(image["reference"], str)
            or image["reference"] in image_subjects
        ):
            raise ValidationError("manifest image identity is invalid or duplicated")
        if (
            not isinstance(image["digest"], str)
            or not DIGEST_RE.fullmatch(image["digest"])
            or not isinstance(image["image_id"], str)
            or not DIGEST_RE.fullmatch(image["image_id"])
        ):
            raise ValidationError("manifest image digest is invalid")
        if not image["reference"].endswith(f"@{image['digest']}"):
            raise ValidationError("manifest image reference is not digest-pinned")
        image_components.add(image["component"])
        image_subjects[image["reference"]] = image["digest"].removeprefix("sha256:")
    if not isinstance(manifest["materials"], dict) or set(manifest["materials"]) != {
        "SBOM.spdx.json",
        "THIRD_PARTY.md",
        "release-notes.md",
    }:
        raise ValidationError("manifest material set is invalid")
    for name, material in manifest["materials"].items():
        value = read_regular(bundle / name)
        if material != {"path": name, "size": len(value), "sha256": hashlib.sha256(value).hexdigest()}:
            raise ValidationError(f"manifest material mismatch: {name}")

    expected_files = required | set(artifact_subjects)
    actual_files = {
        path.relative_to(bundle).as_posix() for path in bundle.rglob("*") if path.is_file()
    }
    if actual_files != expected_files:
        raise ValidationError("bundle file set differs from the manifest")

    provenance = load_json(bundle / "provenance.json")
    exact_keys(provenance, {"_type", "subject", "predicateType", "predicate"}, "provenance")
    if provenance.get("_type") != "https://in-toto.io/Statement/v1" or provenance.get(
        "predicateType"
    ) != "https://slsa.dev/provenance/v1":
        raise ValidationError("provenance statement identity is invalid")
    predicate = provenance["predicate"]
    exact_keys(predicate, {"buildDefinition", "runDetails"}, "provenance predicate")
    build_definition = predicate["buildDefinition"]
    exact_keys(
        build_definition,
        {"buildType", "externalParameters", "resolvedDependencies"},
        "provenance build definition",
    )
    dependencies = build_definition["resolvedDependencies"]
    if not isinstance(dependencies, list) or len(dependencies) != 1:
        raise ValidationError("provenance source dependency is invalid")
    expected_dependency = {
        "uri": f"git+{manifest['source_url']}@{args.expected_revision}",
        "digest": {"gitCommit": args.expected_revision},
    }
    if dependencies[0] != expected_dependency:
        raise ValidationError("provenance revision differs from the manifest")
    if build_definition["buildType"] != "https://xs-nexus.invalid/build/release-bundle/v1":
        raise ValidationError("provenance build type is invalid")
    if build_definition["externalParameters"] != {
        "version": manifest["version"],
        "tag": manifest["tag"],
        "source_date_epoch": epoch,
        "build_inputs": build_inputs,
    }:
        raise ValidationError("provenance external parameters differ from the manifest")
    run_details = predicate["runDetails"]
    exact_keys(run_details, {"builder", "metadata"}, "provenance run details")
    if run_details != {
        "builder": {"id": "xs-nexus-release-provenance-v1"},
        "metadata": {"startedOn": timestamp, "finishedOn": timestamp},
    }:
        raise ValidationError("provenance run details are invalid")
    provenance_subjects = provenance["subject"]
    if not isinstance(provenance_subjects, list) or not provenance_subjects:
        raise ValidationError("provenance subjects are invalid")
    subjects = {}
    for subject in provenance_subjects:
        if not isinstance(subject, dict) or set(subject) != {"name", "digest"}:
            raise ValidationError("provenance subject is invalid")
        digest = subject["digest"]
        if not isinstance(digest, dict) or set(digest) != {"sha256"} or not HASH_RE.fullmatch(
            digest["sha256"]
        ):
            raise ValidationError("provenance subject digest is invalid")
        if subject["name"] in subjects:
            raise ValidationError("provenance subject is duplicated")
        subjects[subject["name"]] = digest["sha256"]
    if subjects != artifact_subjects | image_subjects:
        raise ValidationError("provenance subjects differ from the manifest")
    print(
        f"release provenance verified revision={args.expected_revision} tag={args.expected_tag}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValidationError as error:
        print(f"release provenance verification failed: {error}", file=os.sys.stderr)
        raise SystemExit(1)
