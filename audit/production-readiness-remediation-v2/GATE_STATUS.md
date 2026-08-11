# Production Gate Status V2

Status values are restricted to `PASS`, `FAIL`, `PARTIAL`, `SIMULATED_ONLY`, `UNKNOWN`, and `BLOCKED_EXTERNAL`.

| Gate | Scope | Initial V2 Status | Current Status | Next Proof Required |
|---|---|---:|---:|---|
| 01 | Code and deployment provenance | FAIL | FAIL | Owner-controlled signed RC tag/bundle, authenticated release key, and merge to `main`; branch deployment/reverse verification and `ff9551d3` rollback are complete |
| 02 | Secrets and credential rotation | FAIL | FAIL | Full rotation plus proof that every old credential is rejected |
| 03 | Formal key lifecycle | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Human-controlled offline ceremony, backup, revoke, and recovery |
| 04 | XSP/1 internal security | PARTIAL | PASS | Continue regression and obtain the separate Gate 05 independent review before any production-security claim |
| 05 | Independent security audit | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Independent report, findings, fixes, and retest |
| 06 | Linux Agent recovery | PARTIAL | PARTIAL | Real-host reboot, disk-full, DHCP/address-churn and competing-VPN conflict matrix; hosted real-systemd crash/restart and isolated link-change are complete |
| 07 | Real Direct/NAT | SIMULATED_ONLY | SIMULATED_ONLY | Different public networks, hotspot, UDP block, Relay and Direct recovery |
| 08 | Relay security and resilience | PARTIAL | PARTIAL | Adversarial public-network, multi-region/multi-instance, and long-duration capacity evidence; hosted resource, 5M-frame, and restart submatrix is complete |
| 09 | ACL enforcement | PARTIAL | PASS | Continue exact-head regression; Direct A/B/C, identity, offline-policy, Relay and subnet bypass evidence is complete |
| 10 | Real Subnet Router | SIMULATED_ONLY | SIMULATED_ONLY | Real NAS/router approval, revoke, reboot and cleanup |
| 11 | Windows online lifecycle | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Controlled Windows 11 VM full online matrix |
| 12 | Real NAS | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | User-local signed install and ordinary-node matrix |
| 13 | Planned domain/CDN/TLS | FAIL | FAIL | Owner-authorized planned-domain origin certificate/vhost and CDN strict-mode repair, then API, WebSocket, Console and browser evidence; current edge is 525 and direct-origin SNI fails |
| 14 | Production host hardening | FAIL | PARTIAL | Prove warning/critical disk alerts through an independent external destination and on-call acknowledgement |
| 15 | 1Panel coexistence | PARTIAL | PASS | Continue exact-head source regression; external-network, restart, reboot, upgrade and rollback evidence is complete |
| 16 | PostgreSQL least privilege | FAIL | PASS | Continue drift monitoring; bootstrap credential rotation remains a separate Gate 02 requirement |
| 17 | Offsite backup and recovery | BLOCKED_EXTERNAL | BLOCKED_EXTERNAL | Independent failure domain and clean-server restore |
| 18 | Update and release supply chain | PARTIAL | PARTIAL | Owner-controlled offline keys/ceremony, authenticated key and revocation distribution, signed RC, real target-platform execution, and production release/rollback chain; repository failure matrix is complete |
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

- PASS: 4
- FAIL: 5
- BLOCKED_EXTERNAL: 5
- PARTIAL: 8
- SIMULATED_ONLY: 2
- UNKNOWN: 1

Gate 04 changed from `PARTIAL` to `PASS` after code-bearing revision `875395352afc8a17dafc17b2496d62ce701ee4ae` fixed lost-response state-machine failures without changing XSP/1 framing or cryptographic primitives. Exact-head GitHub Actions run `31350065978` passed formatting, strict Clippy, all workspace tests, real PostgreSQL and namespace regressions, Console E2E, double no-cache image reproducibility, and six AddressSanitizer fuzz targets for 180 seconds each. Downloaded fuzz evidence contains 525 SHA-256 verified files, six PASS status files, zero secret-scan findings, and 134,262,706 total executions. Gate 05 remains `BLOCKED_EXTERNAL`; this internal gate result is not an independent protocol or cryptographic audit and does not change overall `NO_GO`.

Gate 06 remains `PARTIAL`. Exact revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02` passed all five jobs in GitHub Actions run `31352258781`. Dedicated job `93345151258` ran the Agent as a real transient systemd service with `Restart=on-failure` and a private network namespace, killed the main process with `SIGKILL`, observed exactly one replacement PID, preserved the signed state manifest, recreated the TUN only inside the service namespace, survived an isolated link down/up event, and left no host interface or active unit after stop. Artifact `9049384061` has verified archive digest `2a88d3a6344c66c699daff9966119df0a77718bcb6b68499ac43a293c742009f`, verified inner checksums, and a zero-finding no-value scan. This hosted x86_64 evidence does not cover a deployed ordinary host reboot, disk exhaustion, DHCP/address churn, competing VPN routes, or arm64/NAS operation; production still runs the older revision without a host Agent.

Gate 08 remains `PARTIAL`. Exact revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` passed the dedicated Relay job `93362562136` in GitHub Actions run `31358498444`. Unit tests prove verification-before-state global registration admission, per-Lease/global packet and byte queues, transactional rejection, and exact cleanup. The release capacity run forwards 5,000,000 authenticated frames with real short-Lease renewal at 71,212.14 packet/s and zero Relay drops/final queue; the namespace run proves two-Agent/two-Relay stop, failover, restart, re-registration, restored-primary recovery, and Direct return. Artifact `9051561308` has archive digest `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67`, portable inner checksums, and a zero-finding no-value scan. This is not public-WAN, distributed-abuse, multi-region/multi-instance, volumetric-DDoS, or long-duration production evidence.

Gate 09 changed from `PARTIAL` to `PASS`. Revision `3f27ed9` closes a signed ACL rollback path by rejecting a lower `policy_version` even when the outer configuration version increases. Final revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` passed all seven jobs in run `31360862865`; ACL job `93369332314` proves three real Agents/TUNs with a disconnected Controller, A→B allow, A→C/C→B deny, protocol/port coverage, forged Node and virtual-source rejection, independent receiver enforcement, and Relay/subnet bypass rejection. Artifact `9052383034` has verified archive/inner hashes and a zero-finding no-value scan. This does not close real WAN, real NAS/subnet-router, independent audit, formal release, or deployment gates; overall remains `NO_GO`.

Gate 15 changed from `PARTIAL` to `PASS`. Exact revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6` passed all eight jobs in run `31504402285`; dedicated job `93822197946` validates the current Compose/lifecycle source and preserves an exact external-network ID plus an unrelated sentinel across project-scoped down and a real Docker-daemon restart. Artifact `9106406005` has independently verified archive and inner SHA-256 values and a zero-finding no-value scan. Existing production evidence separately proves project restart/upgrade/four automatic rollbacks, a real host reboot that restarts the 1Panel runtime, and preservation of OpenResty/web service, database boundary, SSH, routes/rules, non-project nftables, protected containers, and the exact production `1panel-network`. The current branch remains undeployed and overall remains `NO_GO`.

Gate 18 remains `PARTIAL`. Candidate revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` passed all nine jobs in GitHub Actions run `31515281011`; dedicated job `93858773221` proves canonical schema-2 manifests, bounded key rotation, one-way Controller revocation, root-owned local revocation enforcement, download/storage/platform/tamper failures, rollback-before-execution checks, identity preservation, namespace route cleanup, and Console revocation controls. Artifact `9110864186` has matching GitHub and independently downloaded archive SHA-256 `9885bc0d7146d518d53d6de8a11a8a7702d1816b5a49378dc13a797b85be2376`, verified inner hashes, and a zero-finding no-value scan. Formal offline key ceremony, authenticated production distribution, a signed RC, real target-platform execution, and the production release/rollback chain remain absent; no production resource was changed and overall remains `NO_GO`.

Gate 16 changed to `PASS` after exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` passed full CI, isolated PostgreSQL and Docker lifecycle validation, production role migration, negative permissions, deployment, independent SSH reverse verification, and post-finalization checks. Gate 02 remains `FAIL` because the platform bootstrap credential and other disclosed credentials have not all been rotated and independently rejected.

Gate 14 changed from `FAIL` to `PARTIAL` after exact source `ae74783cdf9f75fd90e496fe837e50b744990310` passed CI and production evidence proved key-only SSH, old-key/root/password rejection, a minimal independent nftables INPUT table, public TCP `188` closure, restricted 1Panel tunneling, preserved routes/rules/non-project nftables/`1panel-network`, healthy protected services, timed rollback, and independent post-finalization sessions. Protected maintenance subsequently installed all 157 pending upgrades and 9 required dependencies, booted `6.8.0-137-generic` through a one-shot GRUB entry with automatic old-kernel fallback, repeated internal and external regression, and reduced root usage from `83%` to `77%` without global prune or autoremove. It is not `PASS`: independent external warning/critical disk-alert delivery and on-call acknowledgement remain absent.

Gate 13 remains `FAIL`. Read-only evidence proves client-to-edge TLS succeeds but all required routes return `525`; direct-origin planned-domain SNI fails before HTTP, no planned-domain origin virtual host/certificate exists, and the current public virtual host routes root to Controller instead of Console. Commit `94ccae3` adds strict audit tooling and a safe placeholder template only. DNS/CDN/1Panel changes remain owner-controlled and were not applied.

Gate 23 repository-side remediation is complete at exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b`. Generic JSON rejection handling, bounded PostgreSQL Docker logs, SQLx `0.9.0`, removal of `rsa` and `RUSTSEC-2023-0071`, and a complete `cargo-deny` license policy all passed clean-checkout validation and GitHub Actions run `31313868529`. The gate remains `FAIL`: open Critical/High findings in Gates 01/02/03/05/11/12/13/14/17/20/22/25 are not converted to PASS by an internal regression run.

Gate 24 remains `PARTIAL`. Exact-head evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` proves zero Rust vulnerabilities, an empty cargo-audit ignore list, complete cargo-deny checks, the exact `paste` topology, and that current upstream still has no release/main replacement removing it. `KI-026` is closed through ADR-089's visible, automatically expiring `2026-08-31` P2 disposition. Node 20 CI action annotations from passing run `31324714609` are corrected at `e63c59b`/`98ca145`; exact-head run `31325753985` passed with SHA-pinned Node 24 actions, disabled checkout credential persistence, and zero annotations. The production glibc findings remain under `KI-021`, and the validated branch is neither the deployed revision nor a formal signed release.
