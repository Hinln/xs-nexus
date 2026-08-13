#!/usr/bin/env python3

from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    script = (root / "scripts/windows/test-native-agent.ps1").read_text(encoding="utf-8")
    required = (
        "$secretScanner = Join-Path $root 'scripts\\check-secrets.py'",
        "& python $secretScanner --root $root",
        "& python $secretScanner --root $evidence",
    )
    for fragment in required:
        if fragment not in script:
            raise AssertionError(f"native Windows validation is missing {fragment!r}")
    forbidden = (
        "python scripts/check-secrets.py",
        "--root .",
    )
    for fragment in forbidden:
        if fragment in script:
            raise AssertionError(
                f"native Windows validation depends on the caller directory: {fragment!r}"
            )
    print("native Windows Agent path invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
