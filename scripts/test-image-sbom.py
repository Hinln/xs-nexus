#!/usr/bin/env python3

import importlib.util
import io
import json
import tarfile
import tempfile
from unittest import mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-image-sbom.py"


def load_module():
    specification = importlib.util.spec_from_file_location("generate_image_sbom", GENERATOR)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def add_file(archive, name, value):
    info = tarfile.TarInfo(name)
    info.size = len(value)
    info.mode = 0o644
    info.mtime = 0
    archive.addfile(info, io.BytesIO(value))


def main():
    module = load_module()
    assert module.parse_images(["controller=example/controller:test"]) == {
        "controller": "example/controller:test"
    }
    for invalid in ("missing", "Bad=reference", "name=", "../name=reference"):
        try:
            module.parse_images([invalid])
        except module.ValidationError:
            pass
        else:
            raise AssertionError(f"invalid mapping accepted: {invalid}")
    dockerfiles = module.parse_dockerfiles(
        ["controller=deploy/docker/controller.Dockerfile"], {"controller": "image"}
    )
    assert dockerfiles["controller"]["declared_base_images"] == [
        "rust:1.93.0-bookworm",
        "gcr.io/distroless/cc-debian12:nonroot@sha256:"
        "fccdbb0a547c14e23fcf4ce8ad62ca5d43b4faae8d22cd292f490fef9946c96e",
    ]
    for invalid in (
        ["controller=../Dockerfile"],
        ["other=deploy/docker/controller.Dockerfile"],
    ):
        try:
            module.parse_dockerfiles(invalid, {"controller": "image"})
        except module.ValidationError:
            pass
        else:
            raise AssertionError("invalid Dockerfile mapping was accepted")

    debian_status = b"""Package: ca-certificates\nStatus: install ok installed\nArchitecture: all\nVersion: 20250419\n\n"""
    debian_status_fragment = b"""Package: libc6\nArchitecture: amd64\nVersion: 2.36-9+deb12u14\nDescription: runtime library\n"""
    alpine_status = b"""P:musl\nV:1.2.5-r10\nA:x86_64\nL:MIT\n\n"""
    alpine_virtual = b"""P:.runtime-deps\nV:20260731.000000\nA:x86_64\n\n"""
    assert module.parse_debian_status(debian_status)[0]["name"] == "ca-certificates"
    assert module.parse_debian_status(debian_status_fragment) == []
    assert module.parse_debian_status(debian_status_fragment, assume_installed=True)[0][
        "name"
    ] == "libc6"
    assert module.parse_apk_installed(alpine_status)[0]["license"] == "MIT"
    assert module.parse_apk_installed(alpine_virtual)[0]["virtual"] is True
    for parser, malformed in (
        (module.parse_debian_status, b"Package: broken\nStatus: install ok installed\n"),
        (module.parse_apk_installed, b"P:broken\nV:1\nA:x86_64\n\n"),
    ):
        try:
            parser(malformed)
        except module.ValidationError:
            pass
        else:
            raise AssertionError("incomplete package identity was accepted")

    with tempfile.TemporaryDirectory(prefix="xs-image-sbom-test-") as temporary:
        base = Path(temporary)
        archive_path = base / "rootfs.tar"
        with tarfile.open(archive_path, "w") as archive:
            add_file(archive, "var/lib/dpkg/status", debian_status)
            add_file(archive, "usr/share/doc/ca-certificates/copyright", b"copyright text\n")
            add_file(archive, "usr/share/common-licenses/GPL-2", b"license text\n")
        packages, materials = module.analyze_rootfs("controller", archive_path, base / "licenses")
        assert len(packages) == 1
        assert [item["path"] for item in materials] == sorted(item["path"] for item in materials)
        assert all((base / item["path"]).is_file() for item in materials)
        assert any(item["kind"] == "package-manager-declarations" for item in materials)
        document = module.cyclonedx(
            {
                "controller": {
                    "image_id": "sha256:" + "1" * 64,
                    "packages": packages,
                    "license_materials": materials,
                }
            },
            "1970-01-01T00:00:00Z",
        )
        assert document["bomFormat"] == "CycloneDX"
        assert len(document["components"]) == 1
        assert document["components"][0]["purl"].startswith("pkg:deb/debian/")
        statement = module.provenance(
            {
                "controller": {
                    "image_id": "sha256:" + "1" * 64,
                }
            },
            dockerfiles,
            "2" * 40,
            "1970-01-01T00:00:00Z",
        )
        assert statement["predicateType"] == "https://slsa.dev/provenance/v1"
        assert statement["subject"][0]["digest"]["sha256"] == "1" * 64

        distroless = base / "distroless.tar"
        with tarfile.open(distroless, "w") as archive:
            add_file(archive, "var/lib/dpkg/status.d/libc6", debian_status_fragment)
            add_file(archive, "var/lib/dpkg/status.d/libc6.md5sums", b"ignored\n")
            add_file(archive, "usr/share/doc/libc6/copyright", b"copyright text\n")
        packages, materials = module.analyze_rootfs(
            "controller", distroless, base / "distroless-licenses"
        )
        assert [package["name"] for package in packages] == ["libc6"]
        assert any(item["kind"] == "package-manager-declarations" for item in materials)

        malicious = base / "malicious.tar"
        with tarfile.open(malicious, "w") as archive:
            add_file(archive, "var/lib/dpkg/status", debian_status)
            add_file(archive, "usr/share/doc/../../escape/copyright", b"escape")
        try:
            module.analyze_rootfs("controller", malicious, base / "bad")
        except module.ValidationError:
            pass
        else:
            raise AssertionError("unsafe license path was accepted")

        source = base / "source"
        source.mkdir()
        (source / "value").write_text("complete", encoding="utf-8")
        destination_parent = base / "destination"
        destination_parent.mkdir()
        with mock.patch.object(module.os, "replace", wraps=module.os.replace) as replace:
            module.publish_output(source, destination_parent / "output")
            replace.assert_called_once()
        assert (destination_parent / "output" / "value").read_text(encoding="utf-8") == "complete"

    print("image SBOM tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
