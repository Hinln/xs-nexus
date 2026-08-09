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

### Gate 16 PostgreSQL least privilege and production deployment

- Split database duties into non-login owner, runtime application, and deployment-only migrator roles. Runtime startup no longer creates schemas or runs migrations and fails closed on role or migration drift.
- Added strict deployment identity checks, role hardening, post-create/post-restore grant repair, least-privilege negative tests, and owner-role backup restore.
- Exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` passed GitHub Actions run `31294988591` and isolated evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z`.
- Production baseline, verified encrypted backup, isolated restore, clean image build, role migration, deployment, independent new-session verification, and post-finalization evidence are stored under `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`.
- Retained four failed verifier attempts and four successful timed automatic rollbacks. The final attempt deployed `3d93656`, verified active `xs_nexus_app` sessions and negative permissions, then canceled rollback only after a separate SSH session passed.
- Promoted the repository-external canonical environment atomically, archived the prior environment as root-only rollback material, and removed the temporary raw password staging directory. No credential value entered Git or evidence.
- Preserved `1panel-network` ID/subnet, unrelated OpenResty, default route, IP rules, and all non-project nftables rules. Docker-managed project rules were validated against actual container addresses and published ports rather than falsely required to remain byte-identical across container recreation.
- Gate 16 is now `PASS`. Gate 02 remains `FAIL` until the bootstrap and all other disclosed credentials are rotated and old values are independently rejected.
