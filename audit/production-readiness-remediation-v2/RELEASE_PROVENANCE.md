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

Remediation CI proved reproducible images for `8532eb6`, but no signed tag, formal production manifest, deployment, or runtime reverse verification exists. Gate 01 remains `FAIL`.
