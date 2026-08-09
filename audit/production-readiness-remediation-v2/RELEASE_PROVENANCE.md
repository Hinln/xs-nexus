# Release Provenance Remediation

## Required Chain

`Git commit -> signed tag -> build inputs -> artifact SHA-256 / OCI digest -> signed release manifest -> deployed instance -> runtime version`

## Required Runtime Identity

Controller, Relay, Console, Agent, and CLI must expose or print product version, full source commit, protocol version, and build/release identity in a stable, testable form without secret data.

## Required Image And Artifact Metadata

- OCI `org.opencontainers.image.revision`, `version`, `source`, and creation metadata normalized for reproducibility.
- Manifest binding every image/artifact filename, platform/architecture, byte length, SHA-256/digest, source commit, signed tag, toolchain/base-image inputs, and SBOM/provenance subjects.
- Clean checkout and no-cache build while preserving the existing double-build reproducibility gate.

## Deployment Reverse Verification

After deployment, query runtime identity, inspect immutable image IDs/digests and labels, compare the active deployment record and migration revision, and verify every value against the signed manifest. Any mismatch keeps Gate 01 `FAIL`.

## Current Status

The remediation branch now contains the complete code-level provenance implementation at `fea456b3d6feff36856b1f2066ace8a22b650bce`:

- Controller, Relay, Console, Agent, and CLI expose a build identity derived from one validated compile-time source revision, release version, XSP/1 protocol version, and source epoch.
- Controller `/v1/version`, Relay `/version`, binary `--version`, Console `version.json`, OCI labels, Linux package manifests, installer verification, image SBOM, and release manifests bind the same identity.
- `make release` rejects a dirty checkout, a non-annotated or invalidly signed tag, a tag not pointing at `HEAD`, a mismatched source remote, and non-canonical build inputs. It emits a deterministic release directory, SHA-256 inventory, in-toto/SLSA provenance, SBOM, notes, and detached Ed25519 signatures.
- The verifier rejects missing or extra files, symlinks, path traversal, malformed metadata, subject drift, digest drift, non-Ed25519 signatures, and tampering.
- Linux installation rejects a signed package whose runtime binary identity differs from its schema-2 manifest, including a correctly signed manifest containing a forged source commit.

GitHub Actions run [`31289641228`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228) completed successfully for exact head `fea456b3d6feff36856b1f2066ace8a22b650bce`: baseline, executable protocol fuzz, real PostgreSQL/Controller Console E2E, and double no-cache image reproducibility all passed. The image, fuzz, and Console evidence artifacts are `9031132937`, `9031005160`, and `9030990379` respectively. The preceding run `31289302264` failed because the Relay lock entry omitted newly declared test dependencies; the failure artifact `9030926588` was retained, `Cargo.lock` was corrected, and the same image gate then passed without weakening any assertion.

This proves the internal implementation and CI behavior only. There is still no owner-controlled offline release-key ceremony, valid formal signed RC tag, formally signed release bundle, merge to `main`, clean production deployment, runtime reverse verification, or upgrade/rollback rehearsal from production revision `ff9551d322067c934d2ac7d55a62af8896660bb3`. The production checkout remains `8745b5804312587534c1e91980dfb11720952ed1`, and the running images remain `ff9551d322067c934d2ac7d55a62af8896660bb3`. Gate 01 therefore remains `FAIL`.
