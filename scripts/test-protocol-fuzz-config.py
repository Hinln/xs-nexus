#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-protocol-fuzz.py")
SPEC = importlib.util.spec_from_file_location("validate_protocol_fuzz", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load protocol fuzz validator")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

TARGETS = ("credential", "data", "discovery", "handshake", "relay", "session_state")
SESSION_SOURCE = "\n".join((*MODULE.SESSION_STATE_TOKENS, *MODULE.SESSION_RETRY_TOKENS))


def write_fixture(root: Path) -> None:
    bins = "\n".join(
        f'''[[bin]]
name = "{target}"
path = "fuzz_targets/{target}.rs"
test = false
doc = false
bench = false
'''
        for target in TARGETS
    )
    (root / "fuzz" / "fuzz_targets").mkdir(parents=True)
    (root / "scripts").mkdir()
    (root / ".github" / "workflows").mkdir(parents=True)
    (root / "fuzz" / "Cargo.toml").write_text(bins, encoding="utf-8")
    for target in TARGETS:
        source = SESSION_SOURCE if target == "session_state" else "#![no_main]\n"
        (root / "fuzz" / "fuzz_targets" / f"{target}.rs").write_text(
            source, encoding="utf-8"
        )
        corpus = root / "fuzz" / "corpus" / target
        corpus.mkdir(parents=True)
        (corpus / "seed").write_bytes(b"0")

    (root / "scripts" / "run-protocol-fuzz.sh").write_text(
        r'''readonly FUZZ_SECONDS="${FUZZ_SECONDS:-180}"
readonly FUZZ_SANITIZER="${FUZZ_SANITIZER:-address}"
readonly CARGO_FUZZ_VERSION="0.13.2"
readonly TARGETS=(credential data discovery handshake relay session_state)
if [[ "${FUZZ_SANITIZER}" != "address" ]]; then exit 2; fi
if [[ "${CARGO_FUZZ_ACTUAL_VERSION}" != "cargo-fuzz ${CARGO_FUZZ_VERSION}" ]]; then exit 2; fi
if [[ -e "${EVIDENCE_DIR}" ]]; then exit 2; fi
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=no
rustc "+${NIGHTLY_TOOLCHAIN}" -vV
cargo "+${NIGHTLY_TOOLCHAIN}" fmt \
    --manifest-path fuzz/Cargo.toml
cargo "+${NIGHTLY_TOOLCHAIN}" clippy \
    --locked
cargo "+${NIGHTLY_TOOLCHAIN}" fuzz build \
    --sanitizer "${FUZZ_SANITIZER}"
cargo "+${NIGHTLY_TOOLCHAIN}" fuzz run \
        --sanitizer "${FUZZ_SANITIZER}"
printf 'crash artifacts detected\n'
sha256sum > SHA256SUMS
''',
        encoding="utf-8",
    )
    (root / ".github" / "workflows" / "ci.yml").write_text(
        """jobs:
  protocol-fuzz:
    runs-on: ubuntu-24.04
    timeout-minutes: 45
    steps:
      - run: FUZZ_SECONDS=180 FUZZ_SANITIZER=address make test-protocol-fuzz EVIDENCE_DIR=artifacts/fuzz-ci
      - run: cargo install --locked cargo-fuzz --version 0.13.2
      - uses: actions/upload-artifact@example
        with:
          name: protocol-fuzz-evidence
          path: artifacts/fuzz-ci
          if-no-files-found: error
  next-job:
    runs-on: ubuntu-24.04
""",
        encoding="utf-8",
    )
    workflow = root / ".github" / "workflows" / "ci.yml"
    source = workflow.read_text(encoding="utf-8")
    workflow.write_text(
        source.replace(
            "    steps:\n",
            "    steps:\n"
            "      - uses: dtolnay/rust-toolchain@example\n"
            "        with:\n"
            "          toolchain: nightly-2026-08-01\n"
            "          components: rustfmt, clippy\n",
            1,
        ),
        encoding="utf-8",
    )


def validate_mutation(mutate, expected: str) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root)
        mutate(root)
        failures = MODULE.validate_repository(root)
        assert failures, "fixture unexpectedly passed"
        assert any(expected in failure for failure in failures), failures


def replace(path: Path, old: str, new: str) -> None:
    source = path.read_text(encoding="utf-8")
    assert old in source
    path.write_text(source.replace(old, new, 1), encoding="utf-8")


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root)
        assert MODULE.validate_repository(root) == []

    validate_mutation(
        lambda root: replace(
            root / "scripts" / "run-protocol-fuzz.sh", "FUZZ_SECONDS:-180", "FUZZ_SECONDS:-30"
        ),
        "at least 180",
    )
    validate_mutation(
        lambda root: replace(
            root / "scripts" / "run-protocol-fuzz.sh",
            "FUZZ_SANITIZER:-address",
            "FUZZ_SANITIZER:-none",
        ),
        "FUZZ_SANITIZER",
    )
    validate_mutation(
        lambda root: (root / "fuzz" / "corpus" / "session_state" / "seed").unlink(),
        "non-empty seed corpus",
    )
    validate_mutation(
        lambda root: replace(
            root / "fuzz" / "fuzz_targets" / "session_state.rs",
            ".retire_previous_epoch",
            ".retire_epoch",
        ),
        "missing lifecycle operation",
    )
    validate_mutation(
        lambda root: replace(
            root / "fuzz" / "fuzz_targets" / "session_state.rs",
            "assert_ne!(client_update_retry_frame, client_update_frame)",
            "assert_eq!(client_update_retry_frame, client_update_frame)",
        ),
        "missing fresh-sequence retry invariant",
    )
    validate_mutation(
        lambda root: replace(
            root / ".github" / "workflows" / "ci.yml",
            "if-no-files-found: error",
            "if-no-files-found: warn",
        ),
        "fail on missing files",
    )
    validate_mutation(
        lambda root: replace(
            root / "scripts" / "run-protocol-fuzz.sh", "SHA256SUMS", "CHECKSUMS"
        ),
        "SHA256SUMS",
    )
    validate_mutation(
        lambda root: replace(
            root / ".github" / "workflows" / "ci.yml",
            "cargo-fuzz --version 0.13.2",
            "cargo-fuzz --version 0.13.1",
        ),
        "cargo install --locked cargo-fuzz --version 0.13.2",
    )
    print("Protocol fuzz configuration validator regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
