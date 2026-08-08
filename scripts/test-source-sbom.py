#!/usr/bin/env python3

import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "generate-source-sbom.py"
SNAPSHOT = ROOT / "supply-chain" / "npm-licenses.json"
EXPECTED_COUNTS = {"cargo": 325, "npm": 110, "total": 435}
OUTPUT_FILES = (
    "manifest.json",
    "xs-nexus-source.cdx.json",
    "xs-nexus-source.spdx.json",
)


def run_generator(output, snapshot=SNAPSHOT, expect_success=True):
    completed = subprocess.run(
        [
            sys.executable,
            str(GENERATOR),
            "--output-dir",
            str(output),
            "--source-date-epoch",
            "0",
            "--npm-license-snapshot",
            str(snapshot),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if expect_success and completed.returncode != 0:
        raise AssertionError(f"generator failed: {completed.stderr}")
    if not expect_success and completed.returncode == 0:
        raise AssertionError("generator unexpectedly accepted invalid input")
    return completed


def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def assert_documents(output):
    manifest = load_json(output / "manifest.json")
    cdx = load_json(output / "xs-nexus-source.cdx.json")
    spdx = load_json(output / "xs-nexus-source.spdx.json")
    assert manifest["component_counts"] == EXPECTED_COUNTS, (
        manifest["component_counts"],
        EXPECTED_COUNTS,
    )
    assert manifest["network_required"] is False
    assert "container operating-system packages are excluded" in manifest["scope"]
    assert cdx["bomFormat"] == "CycloneDX"
    assert cdx["specVersion"] == "1.6"
    assert len(cdx["components"]) == EXPECTED_COUNTS["total"]
    references = [component["bom-ref"] for component in cdx["components"]]
    assert references == sorted(references)
    assert len(references) == len(set(references))
    assert all(component["licenses"][0]["expression"] for component in cdx["components"])
    assert all("/" not in component["licenses"][0]["expression"] for component in cdx["components"])
    ecosystems = [
        component["properties"][0]["value"] for component in cdx["components"]
    ]
    assert ecosystems.count("cargo") == EXPECTED_COUNTS["cargo"]
    assert ecosystems.count("npm") == EXPECTED_COUNTS["npm"]
    assert spdx["spdxVersion"] == "SPDX-2.3"
    assert len(spdx["packages"]) == EXPECTED_COUNTS["total"] + 1
    assert len(spdx["relationships"]) == EXPECTED_COUNTS["total"]
    dependency_packages = spdx["packages"][1:]
    assert all(package["licenseDeclared"] != "NOASSERTION" for package in dependency_packages)


def main():
    specification = importlib.util.spec_from_file_location("generate_source_sbom", GENERATOR)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    for forbidden in (
        "wireguard-wrapper",
        "vendor-wintun-adapter",
        "tailscale",
    ):
        try:
            module.check_dependency_name(forbidden)
        except module.ValidationError:
            pass
        else:
            raise AssertionError(f"forbidden dependency was accepted: {forbidden}")

    with tempfile.TemporaryDirectory(prefix="xs-nexus-sbom-test-") as temp:
        base = Path(temp)
        first = base / "first"
        second = base / "second"
        run_generator(first)
        run_generator(second)
        for filename in OUTPUT_FILES:
            assert (first / filename).read_bytes() == (second / filename).read_bytes()
        assert_documents(first)
        run_generator(first, expect_success=False)

        snapshot = load_json(SNAPSHOT)
        first_license = next(iter(snapshot["licenses"]))
        removed_package = snapshot["licenses"][first_license].pop()
        missing_snapshot = base / "missing.json"
        missing_snapshot.write_text(json.dumps(snapshot), encoding="utf-8")
        failure = run_generator(base / "missing-output", missing_snapshot, expect_success=False)
        assert removed_package in failure.stderr

        snapshot = load_json(SNAPSHOT)
        first_license = next(iter(snapshot["licenses"]))
        rejected_package = snapshot["licenses"][first_license].pop()
        snapshot["licenses"]["GPL-3.0-only"] = [rejected_package]
        rejected_snapshot = base / "rejected.json"
        rejected_snapshot.write_text(json.dumps(snapshot), encoding="utf-8")
        failure = run_generator(base / "rejected-output", rejected_snapshot, expect_success=False)
        assert "unapproved license" in failure.stderr

    print("source SBOM tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
