#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import zipfile
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Iterator

DEFAULT_EXCLUDED_DIRECTORIES = {".git", "dist", "node_modules", "target"}
DEFAULT_MAX_FILE_SIZE = 20_000_000


@dataclass(frozen=True)
class ScannedFile:
    path: str
    data: bytes
    object_id: str | None = None


def load_secret_scanner():
    module_path = Path(__file__).with_name("check-secrets.py")
    spec = importlib.util.spec_from_file_location("xs_secret_scanner", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load secret scanner")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Scan secret-bearing surfaces without emitting matched values"
    )
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--surface",
        action="append",
        choices=("current-tree", "git-history"),
        default=[],
    )
    parser.add_argument(
        "--directory",
        action="append",
        default=[],
        metavar="LABEL=PATH",
    )
    parser.add_argument(
        "--archive",
        action="append",
        default=[],
        metavar="LABEL=PATH",
    )
    parser.add_argument("--reference-env", type=Path, action="append", default=[])
    parser.add_argument(
        "--reference-file",
        action="append",
        default=[],
        metavar="SECRET_ID=PATH",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-file-size", type=int, default=DEFAULT_MAX_FILE_SIZE)
    parser.add_argument("--fail-on-findings", action="store_true")
    parser.add_argument("--require-complete", action="store_true")
    return parser.parse_args()


def run_git(root: Path, arguments: list[str], *, input_bytes: bytes | None = None) -> bytes:
    process = subprocess.run(
        ["git", "-C", str(root), *arguments],
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode != 0:
        raise RuntimeError(f"git command failed: {' '.join(arguments)}")
    return process.stdout


def safe_path(value: str) -> str:
    return value.replace("\\", "/").replace("\r", "?").replace("\n", "?")


def parse_labeled_path(value: str, kind: str) -> tuple[str, Path]:
    if "=" not in value:
        raise ValueError(f"{kind} must use LABEL=PATH")
    label, raw_path = value.split("=", 1)
    if not label or not raw_path:
        raise ValueError(f"{kind} must use non-empty LABEL=PATH")
    return label, Path(raw_path)


def collect_references(
    scanner: Any,
    env_paths: list[Path],
    file_arguments: list[str],
) -> dict[str, bytes]:
    references: dict[str, bytes] = {}
    for path in env_paths:
        for name, value in scanner.reference_values(path).items():
            reference_id = name
            suffix = 2
            while reference_id in references and references[reference_id] != value:
                reference_id = f"{name}_{suffix}"
                suffix += 1
            references[reference_id] = value

    for raw_argument in file_arguments:
        reference_id, path = parse_labeled_path(raw_argument, "reference file")
        if re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,63}", reference_id) is None:
            raise ValueError("reference file ID is invalid")
        if reference_id in references:
            raise ValueError("duplicate reference file ID")
        if path.is_symlink() or not path.is_file():
            raise ValueError("reference file must be a regular non-symlink file")
        data = path.read_bytes()
        if not data or len(data) > 1_000_000:
            raise ValueError("reference file size is invalid")
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            value = data
        else:
            stripped = text.rstrip("\r\n")
            if "\n" not in stripped and "\r" not in stripped:
                value = stripped.encode()
            else:
                value = data
        if not value:
            raise ValueError("reference file value is empty")
        references[reference_id] = value
    return references


def iter_directory_files(
    root: Path,
    max_file_size: int,
    skipped: list[dict[str, Any]],
    errors: list[str],
) -> Iterator[ScannedFile]:
    resolved = root.resolve()
    if not resolved.is_dir():
        raise RuntimeError("scan directory does not exist or is not a directory")

    for directory, names, filenames in os.walk(resolved, followlinks=False):
        names[:] = sorted(
            name for name in names if name not in DEFAULT_EXCLUDED_DIRECTORIES
        )
        for filename in sorted(filenames):
            path = Path(directory) / filename
            relative = safe_path(str(path.relative_to(resolved)))
            try:
                if path.is_symlink():
                    skipped.append({"path": relative, "reason": "symlink"})
                    continue
                size = path.stat().st_size
                if size > max_file_size:
                    skipped.append(
                        {"path": relative, "reason": "size-limit", "size": size}
                    )
                    continue
                yield ScannedFile(relative, path.read_bytes())
            except OSError:
                errors.append(relative)


def git_inventory(root: Path) -> dict[str, Any]:
    head = run_git(root, ["rev-parse", "HEAD"]).decode().strip()
    refs = []
    raw_refs = run_git(
        root,
        ["for-each-ref", "--format=%(refname)%00%(objectname)"],
    ).decode(errors="replace")
    for line in raw_refs.splitlines():
        name, separator, object_id = line.partition("\x00")
        if separator:
            refs.append({"name": safe_path(name), "object_id": object_id})
    commit_count = int(run_git(root, ["rev-list", "--all", "--count"]).decode())
    return {
        "head": head,
        "commit_count": commit_count,
        "refs": sorted(refs, key=lambda item: item["name"]),
    }


def git_history_files(
    root: Path,
    max_file_size: int,
) -> tuple[list[ScannedFile], list[dict[str, Any]], list[str]]:
    raw_objects = run_git(root, ["-c", "core.quotePath=false", "rev-list", "--objects", "--all"])
    object_paths: dict[str, set[str]] = {}
    for raw_line in raw_objects.decode(errors="surrogateescape").splitlines():
        object_id, separator, path = raw_line.partition(" ")
        if separator and path:
            object_paths.setdefault(object_id, set()).add(safe_path(path))

    object_ids = sorted(object_paths)
    checks = run_git(
        root,
        ["cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)"],
        input_bytes=("\n".join(object_ids) + "\n").encode(),
    ).decode()
    blobs: list[tuple[str, int]] = []
    skipped: list[dict[str, Any]] = []
    for line in checks.splitlines():
        object_id, object_type, raw_size = line.split(" ", 2)
        if object_type != "blob":
            continue
        size = int(raw_size)
        if size > max_file_size:
            for path in sorted(object_paths[object_id]):
                skipped.append(
                    {
                        "path": path,
                        "object_id": object_id,
                        "reason": "size-limit",
                        "size": size,
                    }
                )
            continue
        blobs.append((object_id, size))

    batch = run_git(
        root,
        ["cat-file", "--batch"],
        input_bytes=("\n".join(object_id for object_id, _ in blobs) + "\n").encode(),
    )
    offset = 0
    files: list[ScannedFile] = []
    errors: list[str] = []
    for expected_id, expected_size in blobs:
        header_end = batch.find(b"\n", offset)
        if header_end < 0:
            raise RuntimeError("truncated git cat-file header")
        header = batch[offset:header_end].decode()
        object_id, object_type, raw_size = header.split(" ", 2)
        size = int(raw_size)
        offset = header_end + 1
        data = batch[offset : offset + size]
        offset += size + 1
        if object_id != expected_id or object_type != "blob" or size != expected_size:
            raise RuntimeError("unexpected git cat-file response")
        if len(data) != size:
            raise RuntimeError("truncated git blob")
        for path in sorted(object_paths[object_id]):
            files.append(ScannedFile(path, data, object_id))
    return files, skipped, errors


def archive_member_path(parent: str, member: str) -> str:
    normalized = safe_path(member)
    pure_path = PurePosixPath(normalized)
    if pure_path.is_absolute() or ".." in pure_path.parts:
        raise ValueError("unsafe archive member path")
    return f"{parent}!{normalized}"


def archive_kind(path: str) -> str | None:
    lowered = path.lower()
    if lowered.endswith(".zip"):
        return "zip"
    if lowered.endswith((".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tar.xz")):
        return "tar"
    return None


def iter_tar_members(
    archive: tarfile.TarFile,
    parent: str,
    max_file_size: int,
    skipped: list[dict[str, Any]],
    errors: list[str],
    depth: int,
) -> Iterator[ScannedFile]:
    for member in archive:
        try:
            member_path = archive_member_path(parent, member.name)
        except ValueError:
            errors.append(f"{parent}!unsafe-member")
            continue
        if member.isfile():
            extracted = archive.extractfile(member)
            if extracted is None:
                errors.append(member_path)
                continue
            nested_kind = archive_kind(member.name)
            if nested_kind == "tar" and depth < 4:
                try:
                    with tarfile.open(fileobj=extracted, mode="r|*") as nested:
                        yield from iter_tar_members(
                            nested,
                            member_path,
                            max_file_size,
                            skipped,
                            errors,
                            depth + 1,
                        )
                except (OSError, tarfile.TarError):
                    errors.append(member_path)
                continue
            if nested_kind == "zip" and depth < 4:
                if member.size > max_file_size:
                    skipped.append(
                        {
                            "path": member_path,
                            "reason": "nested-archive-size-limit",
                            "size": member.size,
                        }
                    )
                    continue
                try:
                    yield from iter_zip_members(
                        zipfile.ZipFile(io.BytesIO(extracted.read())),
                        member_path,
                        max_file_size,
                        skipped,
                        errors,
                        depth + 1,
                    )
                except (OSError, zipfile.BadZipFile):
                    errors.append(member_path)
                continue
            if member.size > max_file_size:
                skipped.append(
                    {
                        "path": member_path,
                        "reason": "size-limit",
                        "size": member.size,
                    }
                )
                continue
            try:
                yield ScannedFile(member_path, extracted.read())
            except OSError:
                errors.append(member_path)
        elif member.issym() or member.islnk():
            yield ScannedFile(f"{member_path}@link-target", member.linkname.encode())


def iter_zip_members(
    archive: zipfile.ZipFile,
    parent: str,
    max_file_size: int,
    skipped: list[dict[str, Any]],
    errors: list[str],
    depth: int,
) -> Iterator[ScannedFile]:
    for member in sorted(archive.infolist(), key=lambda item: item.filename):
        if member.is_dir():
            continue
        try:
            member_path = archive_member_path(parent, member.filename)
        except ValueError:
            errors.append(f"{parent}!unsafe-member")
            continue
        nested_kind = archive_kind(member.filename)
        if member.file_size > max_file_size:
            skipped.append(
                {
                    "path": member_path,
                    "reason": (
                        "nested-archive-size-limit" if nested_kind else "size-limit"
                    ),
                    "size": member.file_size,
                }
            )
            continue
        try:
            data = archive.read(member)
        except (OSError, RuntimeError, zipfile.BadZipFile):
            errors.append(member_path)
            continue
        if nested_kind == "tar" and depth < 4:
            try:
                with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as nested:
                    yield from iter_tar_members(
                        nested,
                        member_path,
                        max_file_size,
                        skipped,
                        errors,
                        depth + 1,
                    )
            except (OSError, tarfile.TarError):
                errors.append(member_path)
            continue
        if nested_kind == "zip" and depth < 4:
            try:
                with zipfile.ZipFile(io.BytesIO(data)) as nested:
                    yield from iter_zip_members(
                        nested,
                        member_path,
                        max_file_size,
                        skipped,
                        errors,
                        depth + 1,
                    )
            except (OSError, zipfile.BadZipFile):
                errors.append(member_path)
            continue
        yield ScannedFile(member_path, data)


def iter_archive_files(
    path: Path,
    label: str,
    max_file_size: int,
    skipped: list[dict[str, Any]],
    errors: list[str],
) -> Iterator[ScannedFile]:
    resolved = path.resolve()
    if resolved.is_symlink() or not resolved.is_file():
        raise RuntimeError("archive must be a regular non-symlink file")
    kind = archive_kind(resolved.name)
    if kind == "tar":
        try:
            with tarfile.open(resolved, mode="r:*") as archive:
                yield from iter_tar_members(
                    archive,
                    label,
                    max_file_size,
                    skipped,
                    errors,
                    0,
                )
        except (OSError, tarfile.TarError) as error:
            raise RuntimeError("unable to read tar archive") from error
        return
    if kind == "zip":
        try:
            with zipfile.ZipFile(resolved) as archive:
                yield from iter_zip_members(
                    archive,
                    label,
                    max_file_size,
                    skipped,
                    errors,
                    0,
                )
        except (OSError, zipfile.BadZipFile) as error:
            raise RuntimeError("unable to read zip archive") from error
        return
    raise RuntimeError("unsupported archive extension")


def finding_record(surface_id: str, file: ScannedFile, finding: Any) -> dict[str, Any]:
    material = "\x00".join(
        (
            surface_id,
            file.path,
            str(finding.line),
            finding.rule,
            file.object_id or "",
        )
    ).encode()
    record: dict[str, Any] = {
        "finding_id": f"SEC-{hashlib.sha256(material).hexdigest()[:16]}",
        "path": file.path,
        "line": finding.line,
        "rule": finding.rule,
    }
    if file.object_id is not None:
        record["object_id"] = file.object_id
    return record


def scan_files(
    scanner: Any,
    surface_id: str,
    kind: str,
    files: Iterable[ScannedFile],
    references: dict[str, bytes],
    skipped: list[dict[str, Any]],
    errors: list[str],
) -> dict[str, Any]:
    findings: list[dict[str, Any]] = []
    bytes_scanned = 0
    files_scanned = 0
    for file in files:
        files_scanned += 1
        bytes_scanned += len(file.data)
        path = Path(PurePosixPath(file.path))
        for finding in scanner.scan_bytes(path, file.data, references):
            findings.append(finding_record(surface_id, file, finding))
    findings.sort(
        key=lambda item: (
            item["path"],
            item["line"],
            item["rule"],
            item.get("object_id", ""),
        )
    )
    return {
        "surface_id": surface_id,
        "kind": kind,
        "files_scanned": files_scanned,
        "bytes_scanned": bytes_scanned,
        "findings": findings,
        "skipped": skipped,
        "read_errors": sorted(errors),
        "complete": not skipped and not errors,
    }


def build_report(arguments: argparse.Namespace) -> dict[str, Any]:
    scanner = load_secret_scanner()
    root = arguments.root.resolve()
    references = collect_references(
        scanner,
        arguments.reference_env,
        arguments.reference_file,
    )
    inventory = git_inventory(root)
    surfaces: list[dict[str, Any]] = []

    for surface in arguments.surface:
        if surface == "current-tree":
            skipped: list[dict[str, Any]] = []
            errors: list[str] = []
            files = iter_directory_files(
                root,
                arguments.max_file_size,
                skipped,
                errors,
            )
            surfaces.append(
                scan_files(
                    scanner,
                    "current-tree",
                    "directory",
                    files,
                    references,
                    skipped,
                    errors,
                )
            )
        elif surface == "git-history":
            files, skipped, errors = git_history_files(root, arguments.max_file_size)
            surfaces.append(
                scan_files(
                    scanner,
                    "git-history",
                    "reachable-git-blobs",
                    files,
                    references,
                    skipped,
                    errors,
                )
            )

    for raw_directory in arguments.directory:
        label, path = parse_labeled_path(raw_directory, "directory")
        skipped = []
        errors = []
        files = iter_directory_files(
            path,
            arguments.max_file_size,
            skipped,
            errors,
        )
        surfaces.append(
            scan_files(
                scanner,
                safe_path(label),
                "directory",
                files,
                references,
                skipped,
                errors,
            )
        )

    for raw_archive in arguments.archive:
        label, path = parse_labeled_path(raw_archive, "archive")
        skipped: list[dict[str, Any]] = []
        errors: list[str] = []
        files = iter_archive_files(
            path,
            safe_path(label),
            arguments.max_file_size,
            skipped,
            errors,
        )
        surfaces.append(
            scan_files(
                scanner,
                safe_path(label),
                "archive",
                files,
                references,
                skipped,
                errors,
            )
        )

    finding_count = sum(len(surface["findings"]) for surface in surfaces)
    incomplete_count = sum(not surface["complete"] for surface in surfaces)
    return {
        "schema": 1,
        "generated_at": datetime.now(UTC).isoformat(),
        "value_disclosure": "matched values are never included",
        "repository": inventory,
        "references_loaded": sorted(references),
        "surfaces": surfaces,
        "summary": {
            "surface_count": len(surfaces),
            "finding_count": finding_count,
            "incomplete_surface_count": incomplete_count,
        },
    }


def main() -> int:
    arguments = parse_arguments()
    if not arguments.surface and not arguments.directory and not arguments.archive:
        raise SystemExit("at least one --surface, --directory, or --archive is required")
    if arguments.max_file_size <= 0:
        raise SystemExit("--max-file-size must be positive")
    report = build_report(arguments)
    output = arguments.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    temporary.replace(output)

    summary = report["summary"]
    print(
        "secret surface scan completed: "
        f"surfaces={summary['surface_count']} "
        f"findings={summary['finding_count']} "
        f"incomplete={summary['incomplete_surface_count']} "
        f"report={output.name}"
    )
    if arguments.require_complete and summary["incomplete_surface_count"]:
        return 2
    if arguments.fail_on_findings and summary["finding_count"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
