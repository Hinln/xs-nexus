# Production Gate Status V2

Status values are restricted to `PASS`, `FAIL`, `PARTIAL`, `SIMULATED_ONLY`, `UNKNOWN`, and `BLOCKED_EXTERNAL`.

| Gate | Scope | Initial V2 Status | Current Status | Next Proof Required |
|---|---|---:|---:|---|
| 01 | Code and deployment provenance | FAIL | FAIL | Owner-controlled signed RC tag/bundle, authenticated release key, and merge to `main`; branch deployment/reverse verification and `ff9551d3` rollback are complete |
| 02 | Secrets and credential rotation | FAIL | FAIL | Full rotation plus proof that every old credential is rejected |
| 03 | Formal key lifecycle | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Human-controlled offline ceremony, backup, revoke, and recovery |
| 04 | XSP/1 internal security | PARTIAL | PARTIAL | Extended fuzz/sanitizer/state-machine evidence on current revision |
| 05 | Independent security audit | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Independent report, findings, fixes, and retest |
| 06 | Linux Agent recovery | PARTIAL | PARTIAL | Real-host reboot/failure/network-change matrix |
| 07 | Real Direct/NAT | SIMULATED_ONLY | SIMULATED_ONLY | Different public networks, hotspot, UDP block, Relay and Direct recovery |
| 08 | Relay security and resilience | PARTIAL | PARTIAL | Adversarial public-network and sustained-capacity evidence |
| 09 | ACL enforcement | PARTIAL | PARTIAL | A/B/C topology and Relay/subnet bypass attempts |
| 10 | Real Subnet Router | SIMULATED_ONLY | SIMULATED_ONLY | Real NAS/router approval, revoke, reboot and cleanup |
| 11 | Windows online lifecycle | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Controlled Windows 11 VM full online matrix |
| 12 | Real NAS | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | User-local signed install and ordinary-node matrix |
| 13 | Planned domain/CDN/TLS | FAIL | FAIL | Owner-authorized planned-domain origin certificate/vhost and CDN strict-mode repair, then API, WebSocket, Console and browser evidence; current edge is 525 and direct-origin SNI fails |
| 14 | Production host hardening | FAIL | PARTIAL | Prove warning/critical disk alerts through an independent external destination and on-call acknowledgement |
| 15 | 1Panel coexistence | PARTIAL | PARTIAL | Restart/upgrade/rollback and safe reboot evidence |
| 16 | PostgreSQL least privilege | FAIL | PASS | Continue drift monitoring; bootstrap credential rotation remains a separate Gate 02 requirement |
| 17 | Offsite backup and recovery | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Independent failure domain and clean-server restore |
| 18 | Update and release supply chain | PARTIAL | PARTIAL | Formal keys, revocation, signed RC and platform failure matrix |
| 19 | Web Console | PARTIAL | PARTIAL | Real public strict-TLS runtime and complete E2E matrix |
| 20 | Production observability | PARTIAL | PARTIAL | External alert delivery, on-call, certificate and backup monitoring |
| 21 | Performance and capacity | PARTIAL | PARTIAL | WAN/concurrency evidence and documented safe operating envelope |
| 22 | Current-revision soak | UNKNOWN | UNKNOWN | At least 24 hours with sampling and fault injection |
| 23 | Defects and security findings | FAIL | FAIL | Exact-head self-fixable checks pass at `3bf8619`; all remaining P0/P1/Critical/High findings, including external findings, must reach zero and P2 dispositions must remain current |
| 24 | Dependencies and image supply chain | PARTIAL | PARTIAL | Formal signed/deployed release and bounded glibc disposition closure; dated `paste` topology review completed |
| 25 | Clean deployment rehearsal | FAIL | FAIL | Independent clean checkout/full lifecycle and production upgrade rollback |

## Initial Counts

- PASS: 0
- FAIL: 7
- BLOCKED_EXTERNAL: 5
- PARTIAL: 10
- SIMULATED_ONLY: 2
- UNKNOWN: 1

Counts change only after raw evidence has been indexed and independently checked.

## Current Counts

- PASS: 1
- FAIL: 5
- BLOCKED_EXTERNAL: 5
- PARTIAL: 11
- SIMULATED_ONLY: 2
- UNKNOWN: 1

Gate 16 changed to `PASS` after exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` passed full CI, isolated PostgreSQL and Docker lifecycle validation, production role migration, negative permissions, deployment, independent SSH reverse verification, and post-finalization checks. Gate 02 remains `FAIL` because the platform bootstrap credential and other disclosed credentials have not all been rotated and independently rejected.

Gate 14 changed from `FAIL` to `PARTIAL` after exact source `ae74783cdf9f75fd90e496fe837e50b744990310` passed CI and production evidence proved key-only SSH, old-key/root/password rejection, a minimal independent nftables INPUT table, public TCP `188` closure, restricted 1Panel tunneling, preserved routes/rules/non-project nftables/`1panel-network`, healthy protected services, timed rollback, and independent post-finalization sessions. Protected maintenance subsequently installed all 157 pending upgrades and 9 required dependencies, booted `6.8.0-137-generic` through a one-shot GRUB entry with automatic old-kernel fallback, repeated internal and external regression, and reduced root usage from `83%` to `77%` without global prune or autoremove. It is not `PASS`: independent external warning/critical disk-alert delivery and on-call acknowledgement remain absent.

Gate 13 remains `FAIL`. Read-only evidence proves client-to-edge TLS succeeds but all required routes return `525`; direct-origin planned-domain SNI fails before HTTP, no planned-domain origin virtual host/certificate exists, and the current public virtual host routes root to Controller instead of Console. Commit `94ccae3` adds strict audit tooling and a safe placeholder template only. DNS/CDN/1Panel changes remain owner-controlled and were not applied.

Gate 23 repository-side remediation is complete at exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b`. Generic JSON rejection handling, bounded PostgreSQL Docker logs, SQLx `0.9.0`, removal of `rsa` and `RUSTSEC-2023-0071`, and a complete `cargo-deny` license policy all passed clean-checkout validation and GitHub Actions run `31313868529`. The gate remains `FAIL`: open Critical/High findings in Gates 01/02/03/05/11/12/13/14/17/20/22/25 are not converted to PASS by an internal regression run.

Gate 24 remains `PARTIAL`. Exact-head evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` proves zero Rust vulnerabilities, an empty cargo-audit ignore list, complete cargo-deny checks, the exact `paste` topology, and that current upstream still has no release/main replacement removing it. `KI-026` is closed through ADR-089's visible, automatically expiring `2026-08-31` P2 disposition. Node 20 CI action annotations from passing run `31324714609` are corrected at `e63c59b`/`98ca145`; exact-head run `31325753985` passed with SHA-pinned Node 24 actions, disabled checkout credential persistence, and zero annotations. The production glibc findings remain under `KI-021`, and the validated branch is neither the deployed revision nor a formal signed release.
