#!/usr/bin/env python3

import argparse
import json
import os
import re
import subprocess
from pathlib import Path
from urllib.parse import urlsplit


GIT_RE = re.compile(r"[0-9a-f]{40}")
SEMVER_RE = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
MAX_SOURCE_DATE_EPOCH = 253402300799


class ValidationError(Exception):
    pass


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    return parser.parse_args()


def regular_path(path: Path, label: str) -> Path:
    path = Path(os.path.abspath(path))
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"{label} must be a regular non-symlink file")
    return path


def run_git(repository: Path, *arguments: str, check=True) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repository), *arguments],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        env={**os.environ, "LC_ALL": "C"},
        check=False,
    )
    if check and completed.returncode != 0:
        raise ValidationError(f"Git command failed: {' '.join(arguments)}")
    return completed.stdout.strip()


def canonical_source_url(value: str) -> str:
    if not isinstance(value, str):
        raise ValidationError("inventory source URL is invalid")
    parsed = urlsplit(value)
    if (
        parsed.scheme != "https"
        or parsed.hostname is None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
    ):
        raise ValidationError("inventory source URL is invalid")
    return value.rstrip("/").removesuffix(".git")


def canonical_remote_url(value: str) -> str | None:
    value = value.strip()
    if value.startswith("git@") and ":" in value:
        authority, path = value[4:].split(":", 1)
        value = f"https://{authority}/{path}"
    parsed = urlsplit(value)
    if (
        parsed.scheme != "https"
        or parsed.hostname is None
        or parsed.username is not None
        or parsed.password is not None
    ):
        return None
    return value.rstrip("/").removesuffix(".git")


def main():
    args = parse_args()
    inventory_path = regular_path(args.inventory, "inventory")
    repository = Path(os.path.abspath(args.repository))
    if repository.is_symlink() or not repository.is_dir():
        raise ValidationError("repository must be a real directory")
    try:
        inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError("inventory is not valid UTF-8 JSON") from error
    if not isinstance(inventory, dict):
        raise ValidationError("inventory must be an object")
    revision = inventory.get("revision")
    version = inventory.get("version")
    tag = inventory.get("tag")
    epoch = inventory.get("source_date_epoch")
    if not isinstance(revision, str) or not GIT_RE.fullmatch(revision):
        raise ValidationError("inventory revision is invalid")
    if not isinstance(version, str) or not SEMVER_RE.fullmatch(version):
        raise ValidationError("inventory version is invalid")
    if tag != f"v{version}":
        raise ValidationError("inventory tag and version differ")
    if (
        not isinstance(epoch, int)
        or isinstance(epoch, bool)
        or epoch < 0
        or epoch > MAX_SOURCE_DATE_EPOCH
    ):
        raise ValidationError("inventory source date epoch is invalid")
    source_url = canonical_source_url(inventory.get("source_url"))

    root = Path(run_git(repository, "rev-parse", "--show-toplevel"))
    if Path(os.path.abspath(root)) != repository:
        raise ValidationError("repository path must be the Git worktree root")
    if run_git(repository, "status", "--porcelain=v1", "--untracked-files=all"):
        raise ValidationError("release source worktree is not clean")
    head = run_git(repository, "rev-parse", "HEAD")
    if head != revision:
        raise ValidationError("inventory revision differs from HEAD")
    if run_git(repository, "show", "-s", "--format=%ct", "HEAD") != str(epoch):
        raise ValidationError("inventory source date epoch differs from HEAD")
    tag_reference = f"refs/tags/{tag}"
    if run_git(repository, "cat-file", "-t", tag_reference) != "tag":
        raise ValidationError("release tag is not an annotated signed tag object")
    if run_git(repository, "rev-parse", f"{tag_reference}^{{commit}}") != revision:
        raise ValidationError("release tag does not identify HEAD")
    run_git(repository, "verify-tag", "--raw", tag_reference)

    remotes = run_git(repository, "remote", "-v").splitlines()
    source_remotes = {
        normalized
        for line in remotes
        if line.endswith("(fetch)") and len(line.split()) >= 2
        for normalized in [canonical_remote_url(line.split()[1])]
        if normalized is not None
    }
    if source_url not in source_remotes:
        raise ValidationError("inventory source URL is not a configured fetch remote")
    print(f"release source verified revision={revision} tag={tag}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValidationError as error:
        print(f"release source verification failed: {error}", file=os.sys.stderr)
        raise SystemExit(1)
