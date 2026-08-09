# Gate 24 Dependency Topology Review

## Decision

- Review result: `PASS` for the bounded `RUSTSEC-2024-0436` topology disposition only.
- Gate 24 result: `PARTIAL`.
- Overall production result: `NO_GO`.
- Reviewed source: `45dbc19690f6f738a43cabf2c344118d0b8bc055`.
- Evidence: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z`.

This review does not claim that Gate 24 or the release is complete. It closes the dated `KI-026` review requirement while preserving the separate runtime-image, formal-release, deployment, and external-audit gates.

## Raw Results

- `cargo audit --json` recorded 308 locked dependencies, zero vulnerabilities, `settings.ignore=[]`, and one informational unmaintained warning: `RUSTSEC-2024-0436` for `paste 1.0.15`.
- `cargo deny --all-features check` returned success for advisories, bans, licenses, and sources. Its only advisory exception is the explicit `RUSTSEC-2024-0436` entry with reason and deadline in `deny.toml`.
- `scripts/check-rust-advisories.sh` passed and fails closed after `2026-08-31`, if `rsa` re-enters the lock, or if the expected `paste` path no longer traverses `rtnetlink`.
- The exact locked path is `xs-agent -> rtnetlink 0.21.0 -> netlink-packet-core 0.8.2 -> paste 1.0.15`.
- The evidence root includes the exact source bundle hash, detached clean checkout, lock and policy files, full command output, the audit script itself, and a verified SHA-256 manifest.

## Upstream Review

- `rtnetlink` `main` resolved to `e7799b6ee24267586e6aadc0e3fb415b4d921dd4`; its manifest still declares version `0.21.0`.
- `netlink-packet-core` `main` resolved to `571d8bb5fa1dbaa875e8aede3f214c87f70b955b`; its manifest still declares `paste = "1"`.
- Therefore neither the locked release nor current upstream `main` removes this transitive dependency. Replacing it now would require carrying a project fork or vendor patch of a networking dependency rather than selecting an available upstream release.

## Bounded Disposition

- Do not fork or vendor the netlink stack solely to suppress an informational maintenance warning. That would transfer update and security ownership to XS Nexus without removing a demonstrated vulnerability.
- Keep the warning visible in plain `cargo audit`; do not add a cargo-audit ignore.
- Keep the single dated cargo-deny exception and its automated expiry at `2026-08-31`.
- Reopen the review immediately if `Cargo.lock`, the `rtnetlink` path or features, either captured upstream manifest, the advisory classification, the Rust/SQLx version, or the proposed RC revision changes.
- Prefer a normal upstream release that removes `paste`; then remove the exception and rerun the complete dependency, namespace-networking, protocol, image, and release regression matrix.

## CI Action Runtime

GitHub Actions run `31324714609` passed all four jobs for exact source `45dbc19690f6f738a43cabf2c344118d0b8bc055`, but GitHub annotated the old `actions/checkout`, `actions/setup-node`, and `actions/upload-artifact` commits as Node 20 actions being forced onto Node 24. A green run did not make that deprecation acceptable for the release pipeline.

- Commit `e63c59bc7c54b78de94b01885fa9af1b54a31746` moves those actions to official Node 24 releases pinned by exact commit: checkout `v7.0.1`, setup-node `v7.0.0`, and upload-artifact `v7.0.1`.
- Commit `98ca145c197b1d2af20d7cf5df28008820a25e50` sets `persist-credentials: false` on every checkout because no job pushes to the repository.
- `scripts/validate-ci-actions.py` now rejects mutable remote action references, any drift from the reviewed Node 24 commits/version labels, and checkout credential persistence. Its regression suite covers mutable tags, the prior Node 20 checkout commit, version-label drift, malformed references, and missing credential hardening.
- The three replacement commits were resolved from official repositories, expose `runs.using: node24`, and have valid GitHub commit verification. The existing pinned Docker Buildx action is also reviewed as Node 24.
- GitHub Actions run `31325753985` passed baseline, real Console E2E, protocol fuzz, and double no-cache image reproducibility for exact head `98ca145c197b1d2af20d7cf5df28008820a25e50`. All four check-run annotation arrays were empty.

## Production Invariants

The audit made no application deployment or host-network change. Before/after checks proved exact container, Docker network, `1panel-network`, default route, IP rule, canonical nftables, and failed-unit invariants. Temporary QA containers and worktrees were absent after completion. Production Controller, Relay, and Console remained healthy at `3d93656cc9ec3ea35d58e453118154b25bcc4e14`.

## Remaining Gate 24 Work

- Close or obtain owner acceptance for the bounded `KI-021` glibc findings after a fresh exact-image scan.
- Create the owner-controlled formal release keys and revocation material.
- Merge the authorized revision, create a valid signed RC tag and bundle, and reproduce all release images from a clean checkout.
- Deploy that exact signed revision with protected rollback and reverse-verify runtime provenance.
- Retain the independent third-party supply-chain audit as `BLOCKED_EXTERNAL` until performed.
