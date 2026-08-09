#!/usr/bin/env python3

from __future__ import annotations

import json
import io
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path


def run(arguments: list[str], root: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        arguments,
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def git(root: Path, *arguments: str) -> None:
    process = run(["git", *arguments], root)
    if process.returncode != 0:
        raise RuntimeError("fixture git command failed")


def main() -> int:
    scanner = Path(__file__).with_name("scan-secret-surfaces.py").resolve()
    sensitive_name = b"api_" + b"sec" + b"ret"
    sensitive_value = b"history-only-sensitive-value"

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        git(root, "init", "-b", "main")
        git(root, "config", "user.name", "Secret Scanner Test")
        git(root, "config", "user.email", "scanner@example.invalid")

        tracked = root / "deleted.yml"
        tracked.write_bytes(sensitive_name + b": " + sensitive_value + b"\n")
        git(root, "add", "deleted.yml")
        git(root, "commit", "-m", "add fixture")
        git(root, "tag", "fixture-secret")
        tracked.unlink()
        git(root, "add", "-u")
        git(root, "commit", "-m", "remove fixture")

        report_path = root / "report.json"
        process = run(
            [
                sys.executable,
                str(scanner),
                "--root",
                str(root),
                "--surface",
                "current-tree",
                "--surface",
                "git-history",
                "--output",
                str(report_path),
                "--fail-on-findings",
                "--require-complete",
            ],
            root,
        )
        if process.returncode != 1:
            raise RuntimeError("deleted historical secret was not rejected")

        report_bytes = report_path.read_bytes()
        combined_output = process.stdout + process.stderr + report_bytes
        if sensitive_value in combined_output:
            raise RuntimeError("secret value leaked into scanner output")

        report = json.loads(report_bytes)
        history = next(
            surface
            for surface in report["surfaces"]
            if surface["surface_id"] == "git-history"
        )
        current = next(
            surface
            for surface in report["surfaces"]
            if surface["surface_id"] == "current-tree"
        )
        if current["findings"]:
            raise RuntimeError("deleted secret was reported in the current tree")
        if len(history["findings"]) != 1:
            raise RuntimeError("historical finding count is incorrect")
        finding = history["findings"][0]
        if finding["path"] != "deleted.yml" or finding["rule"] != "secret-assignment":
            raise RuntimeError("historical finding metadata is incorrect")
        if sensitive_value.decode() in json.dumps(finding):
            raise RuntimeError("secret value leaked into finding metadata")
        if not any(ref["name"] == "refs/tags/fixture-secret" for ref in report["repository"]["refs"]):
            raise RuntimeError("tag inventory is incomplete")

        reference_value = b"reference-only-sensitive-value"
        reference_path = root / "reference.bin"
        reference_path.write_bytes(reference_value)
        nested_tar = io.BytesIO()
        with tarfile.open(fileobj=nested_tar, mode="w") as layer:
            member = tarfile.TarInfo("removed.txt")
            member.size = len(reference_value)
            layer.addfile(member, io.BytesIO(reference_value))
        archive_path = root / "image.tar"
        with tarfile.open(archive_path, mode="w") as archive:
            layer_bytes = nested_tar.getvalue()
            member = tarfile.TarInfo("sha256/layer.tar")
            member.size = len(layer_bytes)
            archive.addfile(member, io.BytesIO(layer_bytes))

        archive_report_path = root / "archive-report.json"
        process = run(
            [
                sys.executable,
                str(scanner),
                "--root",
                str(root),
                "--archive",
                f"docker-image={archive_path}",
                "--reference-file",
                f"CRED_FIXTURE={reference_path}",
                "--output",
                str(archive_report_path),
                "--fail-on-findings",
                "--require-complete",
            ],
            root,
        )
        if process.returncode != 1:
            raise RuntimeError("nested archive reference was not rejected")
        archive_report_bytes = archive_report_path.read_bytes()
        if reference_value in process.stdout + process.stderr + archive_report_bytes:
            raise RuntimeError("reference value leaked into archive report")
        archive_report = json.loads(archive_report_bytes)
        archive_surface = archive_report["surfaces"][0]
        if archive_surface["files_scanned"] != 1 or not archive_surface["complete"]:
            raise RuntimeError("nested archive scan is incomplete")
        if len(archive_surface["findings"]) != 1:
            raise RuntimeError("nested archive finding count is incorrect")
        archive_finding = archive_surface["findings"][0]
        if archive_finding["rule"] != "reference:CRED_FIXTURE":
            raise RuntimeError("nested archive reference rule is incorrect")
        if archive_finding["path"] != "docker-image!sha256/layer.tar!removed.txt":
            raise RuntimeError("nested archive path is incorrect")

    print("secret surface audit regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
