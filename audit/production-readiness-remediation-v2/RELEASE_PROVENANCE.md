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

The implementation was subsequently carried through database-hardening revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`. GitHub Actions run [`31294988591`](https://github.com/Hinln/xs-nexus/actions/runs/31294988591) passed baseline, real Console E2E, executable protocol fuzz, and double no-cache image reproducibility for that exact head.

Production now runs Controller, Relay, and Console images tagged and labeled with exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`, built from the clean checkout `/srv/xs-nexus-production-releases/3d93656cc9ec3ea35d58e453118154b25bcc4e14/repo`. HTTP version endpoints, binary `--version`, OCI labels, active database role, container health, and host/network invariants were reverse-verified from an independent SSH session. Four failed verifier attempts each exercised the timed rollback to the previous running revision `ff9551d322067c934d2ac7d55a62af8896660bb3`; the fifth deployment remained active only after all checks passed. Raw evidence is indexed under the Gate 16 production evidence root.

Gate 01 nevertheless remains `FAIL`. The deployment is a remediation-branch build, not an owner-controlled formal release: there is no offline production release-key ceremony, authenticated public-key publication, valid formal signed RC tag, formally signed production release bundle, or merge to `main`. The production source checkout at `/srv/xs-nexus` also remains a historical operational checkout; the active containers are instead traced to the immutable clean release checkout above. These remaining chain elements cannot be inferred from successful branch deployment or test signing.
