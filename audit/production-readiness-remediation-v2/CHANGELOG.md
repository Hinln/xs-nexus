# Remediation V2 Changelog

## 2026-08-09

- Froze GitHub main, production checkout, production runtime, remediation revision, and V2 branch start.
- Created `production-readiness-remediation-v2` without modifying `main`.
- Preserved the complete prior audit under `audit/production-readiness/`.
- Initialized the V2 hard-gate table, open findings, evidence index, external-gate runbooks, and release-readiness checklist.
- No production host, credential, database, DNS, firewall, 1Panel, Docker network, or running service change has been made in this entry.

### Gate 01 internal provenance implementation

- Added one validated build identity across Controller, Relay, Console, Agent, CLI, OCI labels, Linux packages, installers, SBOM, release manifest, and in-toto/SLSA provenance.
- Added deterministic signed release-bundle generation and strict offline verification, including dirty source, tag, signature, identity, digest, subject, path, symlink, extra-file, and tamper rejection.
- Added runtime/package mismatch rejection and a regression proving that a correctly signed Linux manifest with a forged source commit cannot activate.
- Retained failed GitHub Actions run `31289302264` and artifact `9030926588`; fixed the missing Relay lock metadata rather than bypassing `--locked`.
- GitHub Actions run `31289641228` passed all four jobs for exact revision `fea456b3d6feff36856b1f2066ace8a22b650bce`; artifacts: image `9031132937`, fuzz `9031005160`, Console `9030990379`.
- Gate 01 remains `FAIL`: no formal key ceremony, signed RC tag/bundle, main merge, production deployment, runtime reverse verification, or old-production upgrade/rollback occurred.
- No production host, credential, database, DNS, firewall, 1Panel, Docker network, or running service change was made by this Gate 01 implementation.
