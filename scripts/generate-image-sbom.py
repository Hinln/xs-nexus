#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.parse
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[1]
PROJECT = "xs-nexus"
MAX_ROOTFS_BYTES = 512 * 1024 * 1024
MAX_LICENSE_BYTES = 4 * 1024 * 1024
MAX_PACKAGES = 4096
IMAGE_NAME_RE = re.compile(r"[a-z0-9][a-z0-9_.-]{0,63}\Z")
SHA256_RE = re.compile(r"sha256:[0-9a-f]{64}\Z")


class ValidationError(RuntimeError):
    pass


def parse_args():
    parser = argparse.ArgumentParser(
        description="Generate deterministic OS package SBOM and license materials from local images"
    )
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--source-date-epoch", required=True, type=int)
    parser.add_argument("--revision", required=True)
    parser.add_argument(
        "--image",
        action="append",
        required=True,
        metavar="NAME=REFERENCE",
        help="logical image name and exact local Docker reference",
    )
    parser.add_argument(
        "--dockerfile",
        action="append",
        required=True,
        metavar="NAME=PATH",
        help="logical image name and repository-relative Dockerfile",
    )
    return parser.parse_args()


def run(command, *, stdout=None):
    try:
        return subprocess.run(
            command,
            cwd=ROOT,
            check=True,
            stdout=stdout or subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=stdout is None,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        detail = getattr(error, "stderr", "")
        raise ValidationError(f"command failed: {' '.join(command)}: {detail}") from error


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def publish_output(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(
        tempfile.mkdtemp(
            prefix=f".{destination.name}.staging-",
            dir=destination.parent,
        )
    )
    try:
        shutil.rmtree(staging)
        shutil.copytree(source, staging)
        os.replace(staging, destination)
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def parse_images(values):
    images = {}
    for value in values:
        name, separator, reference = value.partition("=")
        if separator != "=" or not IMAGE_NAME_RE.fullmatch(name) or not reference:
            raise ValidationError(f"invalid image mapping: {value}")
        if name in images:
            raise ValidationError(f"duplicate image name: {name}")
        images[name] = reference
    return dict(sorted(images.items()))


def parse_dockerfiles(values, image_names):
    mappings = parse_images(values)
    if set(mappings) != set(image_names):
        raise ValidationError("Dockerfile mappings must exactly match image mappings")
    result = {}
    for name, value in mappings.items():
        relative = PurePosixPath(value)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValidationError(f"unsafe Dockerfile path: {value}")
        path = ROOT.joinpath(*relative.parts)
        if not path.is_file() or path.is_symlink():
            raise ValidationError(f"Dockerfile is not a regular repository file: {value}")
        content = path.read_bytes()
        from_references = []
        arguments = {}
        for raw_line in content.decode("utf-8").splitlines():
            line = raw_line.strip()
            if line.startswith("ARG ") and "=" in line[4:]:
                key, default = line[4:].split("=", 1)
                arguments[key] = default
            if line.upper().startswith("FROM "):
                reference = line.split()[1]
                match = re.fullmatch(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}", reference)
                if match:
                    reference = arguments.get(match.group(1), reference)
                from_references.append(reference)
        if not from_references:
            raise ValidationError(f"Dockerfile has no FROM instruction: {value}")
        result[name] = {
            "path": value,
            "sha256": sha256_bytes(content),
            "declared_base_images": from_references,
        }
    return result


def inspect_image(reference, revision):
    completed = run(["docker", "image", "inspect", reference])
    try:
        values = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ValidationError(f"invalid Docker inspect JSON for {reference}") from error
    if not isinstance(values, list) or len(values) != 1:
        raise ValidationError(f"Docker inspect must return one image for {reference}")
    value = values[0]
    image_id = value.get("Id")
    labels = (value.get("Config") or {}).get("Labels") or {}
    if not isinstance(image_id, str) or not SHA256_RE.fullmatch(image_id):
        raise ValidationError(f"image does not have an exact sha256 ID: {reference}")
    if labels.get("org.opencontainers.image.revision") != revision:
        raise ValidationError(f"image revision label mismatch: {reference}")
    source = labels.get("org.opencontainers.image.source")
    title = labels.get("org.opencontainers.image.title")
    if not isinstance(source, str) or not source or not isinstance(title, str) or not title:
        raise ValidationError(f"image OCI source/title labels are missing: {reference}")
    return {
        "reference": reference,
        "image_id": image_id,
        "title": title,
        "source": source,
    }


def export_rootfs(reference, destination):
    container = run(["docker", "create", reference]).stdout.strip()
    if not re.fullmatch(r"[0-9a-f]{64}", container):
        raise ValidationError(f"docker create returned an invalid container ID for {reference}")
    try:
        with destination.open("wb") as output:
            run(["docker", "export", container], stdout=output)
        size = destination.stat().st_size
        if size <= 0 or size > MAX_ROOTFS_BYTES:
            raise ValidationError(f"exported rootfs size is invalid for {reference}: {size}")
    finally:
        run(["docker", "rm", "--force", container])


def tar_member_bytes(archive, member, maximum):
    if not member.isfile() or member.size < 0 or member.size > maximum:
        raise ValidationError(f"unsafe rootfs member: {member.name}")
    stream = archive.extractfile(member)
    if stream is None:
        raise ValidationError(f"cannot read rootfs member: {member.name}")
    value = stream.read(maximum + 1)
    if len(value) != member.size or len(value) > maximum:
        raise ValidationError(f"rootfs member length mismatch: {member.name}")
    return value


def parse_debian_status(value, assume_installed=False):
    packages = []
    for paragraph in value.decode("utf-8").split("\n\n"):
        fields = {}
        for line in paragraph.splitlines():
            if ": " in line and not line.startswith((" ", "\t")):
                key, field_value = line.split(": ", 1)
                fields[key] = field_value
        status = fields.get("Status")
        if status is None and assume_installed:
            status = "install ok installed"
        if status != "install ok installed":
            continue
        if not all(fields.get(key) for key in ("Package", "Version", "Architecture")):
            raise ValidationError("installed Debian package has incomplete identity")
        packages.append(
            {
                "manager": "dpkg",
                "name": fields["Package"],
                "version": fields["Version"],
                "architecture": fields["Architecture"],
                "license": None,
            }
        )
    return packages


def parse_apk_installed(value):
    packages = []
    for paragraph in value.decode("utf-8").split("\n\n"):
        fields = {}
        for line in paragraph.splitlines():
            if len(line) >= 2 and line[1] == ":":
                fields[line[0]] = line[2:]
        if not paragraph.strip():
            continue
        if not all(fields.get(key) for key in ("P", "V", "A")):
            missing = [key for key in ("P", "V", "A", "L") if not fields.get(key)]
            raise ValidationError(
                f"installed Alpine package has incomplete identity/license: "
                f"package={fields.get('P', '<unknown>')} missing={missing}"
            )
        virtual = fields["P"].startswith(".")
        if not fields.get("L") and not virtual:
            raise ValidationError(
                f"installed Alpine package has no declared license: {fields['P']}"
            )
        packages.append(
            {
                "manager": "apk",
                "name": fields["P"],
                "version": fields["V"],
                "architecture": fields["A"],
                "license": fields.get("L"),
                "virtual": virtual,
            }
        )
    return packages


def package_purl(package):
    package_type = "deb/debian" if package["manager"] == "dpkg" else "apk/alpine"
    name = urllib.parse.quote(package["name"], safe="")
    version = urllib.parse.quote(package["version"], safe="")
    architecture = urllib.parse.quote(package["architecture"], safe="")
    return f"pkg:{package_type}/{name}@{version}?arch={architecture}"


def analyze_rootfs(image_name, archive_path, license_root):
    packages = None
    license_materials = []
    with tarfile.open(archive_path, mode="r:") as archive:
        members = archive.getmembers()
        by_name = {member.name.lstrip("./"): member for member in members}
        if "var/lib/dpkg/status" in by_name:
            packages = parse_debian_status(
                tar_member_bytes(archive, by_name["var/lib/dpkg/status"], 32 * 1024 * 1024)
            )
            license_prefixes = ("usr/share/doc/", "usr/share/common-licenses/")
            selected = [
                member
                for member in members
                if member.isfile()
                and member.name.lstrip("./").startswith(license_prefixes)
                and (
                    member.name.lstrip("./").endswith("/copyright")
                    or member.name.lstrip("./").startswith("usr/share/common-licenses/")
                )
            ]
        elif any(
            name.startswith("var/lib/dpkg/status.d/") and not name.endswith(".md5sums")
            for name in by_name
        ):
            status_fragments = []
            total_status_bytes = 0
            for name, member in sorted(by_name.items()):
                if not name.startswith("var/lib/dpkg/status.d/") or name.endswith(".md5sums"):
                    continue
                if not member.isfile():
                    raise ValidationError(f"unsafe Debian status fragment: {member.name}")
                content = tar_member_bytes(archive, member, 1024 * 1024)
                total_status_bytes += len(content)
                if total_status_bytes > 32 * 1024 * 1024:
                    raise ValidationError("Debian status fragments exceed size limit")
                status_fragments.extend(parse_debian_status(content, assume_installed=True))
            packages = status_fragments
            license_prefixes = ("usr/share/doc/", "usr/share/common-licenses/")
            selected = [
                member
                for member in members
                if member.isfile()
                and member.name.lstrip("./").startswith(license_prefixes)
                and (
                    member.name.lstrip("./").endswith("/copyright")
                    or member.name.lstrip("./").startswith("usr/share/common-licenses/")
                )
            ]
        elif "lib/apk/db/installed" in by_name:
            packages = parse_apk_installed(
                tar_member_bytes(archive, by_name["lib/apk/db/installed"], 32 * 1024 * 1024)
            )
            selected = [
                member
                for member in members
                if member.isfile()
                and member.name.lstrip("./").startswith("usr/share/licenses/")
            ]
        else:
            raise ValidationError(f"unsupported image package database: {image_name}")
        if not packages or len(packages) > MAX_PACKAGES:
            raise ValidationError(f"invalid package count for {image_name}")
        destination_root = license_root / image_name
        for member in sorted(selected, key=lambda item: item.name):
            relative = PurePosixPath(member.name.lstrip("./"))
            if relative.is_absolute() or ".." in relative.parts:
                raise ValidationError(f"unsafe license path: {member.name}")
            content = tar_member_bytes(archive, member, MAX_LICENSE_BYTES)
            destination = destination_root.joinpath(*relative.parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
            license_materials.append(
                {
                    "path": str(Path("licenses") / image_name / Path(*relative.parts)).replace("\\", "/"),
                    "sha256": sha256_bytes(content),
                    "size": len(content),
                }
            )
    packages.sort(key=lambda item: package_purl(item))
    purls = [package_purl(package) for package in packages]
    if len(purls) != len(set(purls)):
        raise ValidationError(f"duplicate package identity in {image_name}")
    declarations = [
        {
            "name": package["name"],
            "version": package["version"],
            "manager": package["manager"],
            "license": package["license"],
            "virtual": package.get("virtual", False),
        }
        for package in packages
    ]
    declaration_path = license_root / image_name / "declared-packages.json"
    declaration_path.parent.mkdir(parents=True, exist_ok=True)
    declaration_content = (json.dumps(declarations, indent=2, sort_keys=True) + "\n").encode()
    declaration_path.write_bytes(declaration_content)
    license_materials.append(
        {
            "kind": "package-manager-declarations",
            "path": str(Path("licenses") / image_name / "declared-packages.json").replace("\\", "/"),
            "sha256": sha256_bytes(declaration_content),
            "size": len(declaration_content),
        }
    )
    for material in license_materials:
        material.setdefault("kind", "license-text")
    license_materials.sort(key=lambda item: item["path"])
    return packages, license_materials


def cyclonedx(images, timestamp):
    components = []
    for image_name, image in images.items():
        for package in image["packages"]:
            purl = package_purl(package)
            component = {
                "type": "library",
                "bom-ref": f"{image_name}:{purl}",
                "name": package["name"],
                "version": package["version"],
                "purl": purl,
                "properties": [
                    {"name": "xs-nexus:image", "value": image_name},
                    {"name": "xs-nexus:package-manager", "value": package["manager"]},
                    {"name": "xs-nexus:virtual-package", "value": str(package.get("virtual", False)).lower()},
                ],
            }
            if package["license"]:
                component["licenses"] = [{"expression": package["license"]}]
            components.append(component)
    components.sort(key=lambda item: item["bom-ref"])
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{hashlib.md5(json.dumps(images, sort_keys=True).encode(), usedforsecurity=False).hexdigest()}",
        "version": 1,
        "metadata": {"timestamp": timestamp, "component": {"type": "application", "name": PROJECT}},
        "components": components,
    }


def provenance(images, dockerfiles, revision, timestamp):
    subjects = [
        {
            "name": image_name,
            "digest": {"sha256": image["image_id"].removeprefix("sha256:")},
        }
        for image_name, image in images.items()
    ]
    return {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": subjects,
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "buildDefinition": {
                "buildType": "https://xs-nexus.invalid/build/dockerfile/v1",
                "externalParameters": {
                    "revision": revision,
                    "dockerfiles": dockerfiles,
                },
                "resolvedDependencies": [
                    {"uri": f"git+xs-nexus@{revision}", "digest": {"gitCommit": revision}}
                ],
            },
            "runDetails": {
                "builder": {"id": "xs-nexus-local-docker"},
                "metadata": {"invocationId": sha256_bytes(json.dumps(subjects, sort_keys=True).encode()), "startedOn": timestamp, "finishedOn": timestamp},
            },
        },
    }


def main():
    args = parse_args()
    if args.source_date_epoch < 0:
        raise ValidationError("source date epoch must be nonnegative")
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValidationError("revision must be an exact lowercase Git object ID")
    mappings = parse_images(args.image)
    dockerfiles = parse_dockerfiles(args.dockerfile, mappings)
    if args.output_dir.exists():
        raise ValidationError("output directory already exists")
    timestamp = datetime.fromtimestamp(args.source_date_epoch, timezone.utc).isoformat().replace(
        "+00:00", "Z"
    )
    temporary = Path(tempfile.mkdtemp(prefix="xs-nexus-image-sbom-"))
    try:
        output = temporary / "output"
        license_root = output / "licenses"
        output.mkdir()
        images = {}
        for image_name, reference in mappings.items():
            identity = inspect_image(reference, args.revision)
            archive = temporary / f"{image_name}.tar"
            export_rootfs(reference, archive)
            packages, materials = analyze_rootfs(image_name, archive, license_root)
            images[image_name] = {
                **identity,
                "packages": packages,
                "license_materials": materials,
            }
        manifest_images = {}
        for image_name, image in images.items():
            manifest_images[image_name] = {
                key: value for key, value in image.items() if key != "packages"
            }
            manifest_images[image_name]["package_count"] = len(image["packages"])
            manifest_images[image_name]["dockerfile"] = dockerfiles[image_name]
        manifest = {
            "schema": 1,
            "project": PROJECT,
            "revision": args.revision,
            "generated_at": timestamp,
            "network_required": False,
            "scope": "installed operating-system packages and available license materials in exact local runtime images",
            "images": manifest_images,
            "vulnerability_scan": "not-included; attach a digest-bound scanner report before Release Candidate",
        }
        write_json(output / "manifest.json", manifest)
        write_json(output / "xs-nexus-images.cdx.json", cyclonedx(images, timestamp))
        write_json(
            output / "xs-nexus-images.provenance.json",
            provenance(images, dockerfiles, args.revision, timestamp),
        )
        publish_output(output, args.output_dir)
    finally:
        shutil.rmtree(temporary, ignore_errors=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValidationError as error:
        print(f"image SBOM generation failed: {error}", file=os.sys.stderr)
        raise SystemExit(1)
