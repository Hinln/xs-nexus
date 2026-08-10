# Remediation V2 Evidence Index

## Frozen Git Evidence

| Evidence | Location | Status |
|---|---|---|
| Prior final audit | `audit/production-readiness/GO_NO_GO_FINAL.md` | AVAILABLE |
| Prior hard-gate table | `audit/production-readiness/HARD_GATES.md` | AVAILABLE |
| Prior open findings | `audit/production-readiness/OPEN_FINDINGS.md` | AVAILABLE |
| Prior evidence index | `audit/production-readiness/EVIDENCE_INDEX.md` | AVAILABLE |
| V2 branch start | Git commit `f6dc6021f7b7ea82b2022840465b07b69f7ad693` | VERIFIED LOCALLY |
| Required remediation ancestor | Git commit `8532eb6992389568643f8a501055c6acc72395ab` | VERIFIED LOCALLY |

## Existing Raw Evidence Roots

| Scope | Location | Notes |
|---|---|---|
| Product validation | `/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z` | Revision `ff9551d3` validation |
| Production deployment | `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z` | Running deployment evidence |
| Audit remediation CI | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z/ci-final-8532eb6` | Product remediation CI artifacts |
| Final documentation CI | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z/ci-final-f6dc602` | Documentation-head CI artifacts |
| Production final read-only check | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z/ci-final-8532eb6/production-final.txt` | Read-only production state |

## Gate 01 V2 Evidence

| Evidence | Location | Status |
|---|---|---|
| Provenance implementation | Git commits `825666102b9fcbb4adf585e8ab9858b236048049`, `fea456b3d6feff36856b1f2066ace8a22b650bce` | VERIFIED |
| Failed locked-build regression | GitHub Actions run [`31289302264`](https://github.com/Hinln/xs-nexus/actions/runs/31289302264), artifact `9030926588` | RETAINED FAILED EVIDENCE |
| Corrected full CI | GitHub Actions run [`31289641228`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228), exact head `fea456b3d6feff36856b1f2066ace8a22b650bce` | PASS |
| Baseline job | GitHub Actions job [`93184528450`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228/job/93184528450) | PASS |
| Protocol fuzz job and artifact | GitHub Actions job [`93184528497`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228/job/93184528497), artifact `9031005160` | PASS |
| Image reproducibility job and artifact | GitHub Actions job [`93184528518`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228/job/93184528518), artifact `9031132937` | PASS |
| Real Console E2E job and artifact | GitHub Actions job [`93184528569`](https://github.com/Hinln/xs-nexus/actions/runs/31289641228/job/93184528569), artifact `9030990379` | PASS |

The CI run proves the exact branch revision and internal tooling. It is not evidence of a formal key ceremony, signed RC tag, signed production bundle, merge, deployment, runtime reverse verification, or production upgrade/rollback.

## Gate 16 PostgreSQL Least-Privilege Evidence

| Evidence | Location | Status |
|---|---|---|
| Runtime/migration role implementation | Git commits `02fc54e`, `3e2caed`, `e0fd15d`, `0533727`, `3d93656` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31294988591`](https://github.com/Hinln/xs-nexus/actions/runs/31294988591), exact head `3d93656cc9ec3ea35d58e453118154b25bcc4e14` | PASS |
| Isolated PostgreSQL and Docker lifecycle | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z` | PASS |
| Production baseline, encrypted backup, isolated restore, role migration, deployment and reverse verification | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z` | PASS |
| Retained failed attempts | `deployment.log`, `automatic-rollback.log`, `deployment-attempts-summary.txt` in the production evidence root | 4 FAILURES, 4 AUTOMATIC ROLLBACKS PASS |
| Independent new-session verification | `third-ssh-after.txt` in the production evidence root | PASS |
| Finalization and read-only post-check | `deployment-finalization.txt`, `post-finalization.txt`, `evidence-secret-scan-disposition.txt` | PASS |
| Evidence integrity | `SHA256SUMS` in the production evidence root | 81 FILES VERIFIED |

The running Controller, Relay, and Console images expose revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; the Controller has active `xs_nexus_app` sessions. Metadata and actual negative operations prove that the application role is non-superuser, cannot create databases/roles/schemas, cannot alter tables, and cannot write migration metadata. `xs_nexus_migrator` is also non-superuser; object ownership is held by non-login `xs_nexus_owner`.

The production rollout used an exact clean release checkout and five prebuilt images. Four verifier defects were retained rather than hidden: container tmpfs rejected `docker cp`, the hardened database container lacked `CAP_CHOWN`, binary `--version` was text rather than JSON, and byte-exact nftables comparison rejected expected Docker rule refresh. Every failed attempt left its 20-minute rollback armed and automatically restored `ff9551d3`; only the fifth attempt canceled rollback after independent SSH verification.

## Gate 14 Production Host Hardening Evidence

| Evidence | Location | Status |
|---|---|---|
| Firewall/SSH implementation | Git commit `ae74783cdf9f75fd90e496fe837e50b744990310` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31300939362`](https://github.com/Hinln/xs-nexus/actions/runs/31300939362) | PASS |
| Read-only OS/network/SSH/Docker/update baseline | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z` | PASS |
| Protected production application | `application-20260809T075244Z` under the Gate 14 evidence root | PASS |
| Independent new-session verification | `independent-new-ssh-verification.txt` | PASS |
| External exposure and authentication | `external-verification.txt`, `external-post-cancel.txt` | PASS |
| Route/rule/non-project nftables invariants | `ip-route-default-*.json`, `ip-rule-*.json`, `nftables-nonproject-*.canonical.json` | PASS |
| Rollback protection and cancellation | `rollback-*.txt`, `finalization.txt`, `post-finalization-new-ssh.txt` | PASS |
| Verification-tooling disposition | `apply.log`, `verification-tooling-disposition.txt` | REVIEWED |
| Evidence secret scan | `evidence-secret-scan.json`, `evidence-secret-scan-summary.txt` | PASS, 0 FINDINGS |
| Initial update/disk state | `residual-host-items.txt`, baseline apt/disk files | SUPERSEDED BY PROTECTED MAINTENANCE |
| Evidence integrity | `SHA256SUMS.final` in the Gate 14 root | 117 FILES VERIFIED |
| Protected package-upgrade baseline and rollback set | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z/upgrade-20260809T082139Z` | PASS |
| Independent post-upgrade and external verification | `post-upgrade-independent.txt`, `external-verification-after-upgrade.txt` | PASS |
| One-shot new-kernel reboot and automatic fallback | `reboot-20260809T091342Z` under the maintenance root | PASS |
| Post-reboot internal/external verification | `post-reboot-independent.txt`, `external-verification-after-reboot.txt`, `post-reboot-approved.txt` | PASS |
| GRUB/watchdog finalization and correction disposition | `reboot-finalization.txt`, `reboot-finalization-independent-correction.txt` | PASS |
| Bounded cache cleanup and final disk state | `disk-cleanup-after-reboot.txt`, `disk-after-final-cache-cleanup.txt`, `final-maintenance-state.txt` | PASS, ROOT 77% |
| External disk-alert delivery | `external-disk-alert-status.txt` | BLOCKED_EXTERNAL |
| Maintenance secret scan and integrity | `evidence-secret-scan.txt`, `SHA256SUMS.final` in the maintenance root | PASS, 0 FINDINGS, 218 FILES VERIFIED |

The deployed host firewall is an independent `inet xs_nexus_host_guard` table and does not modify existing 1Panel/Docker tables. TCP `188` is no longer public and is reachable only through the key-only, destination-restricted SSH tunnel. The production application revision remains `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; `1panel-network` remains ID `7df70648b96ab2d6e5e178cce4e5892d655e7b451dd111f90f42ae86e3757ac0`, subnet `172.18.0.0/16`.

Gate 14 remains `PARTIAL`, not `PASS`. Security updates, protected reboot, post-reboot regression, and bounded disk cleanup are complete; independent external disk-alert delivery and on-call acknowledgement remain outstanding.

## Gate 13 Planned-Domain TLS Evidence

| Evidence | Location | Status |
|---|---|---|
| Strict audit and OpenResty template implementation | Git commit `94ccae3e8b4bf0279336d01db8b1ab53abf15aae` | VERIFIED LOCALLY |
| Complete origin/OpenResty/CDN/DNS read-only collection | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094531Z` | FAIL, ROOT CAUSE VERIFIED |
| Retained collector-wrapper failure | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094447Z` | RETAINED FAILED EVIDENCE |
| Exact-commit external strict TLS tool rerun | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-strict-tls-tool-20260809T100334Z` | FAIL AS EXPECTED, 8 FILES VERIFIED |
| Repository regression and public-edge source validation | `scripts/test-audit-strict-tls.py`, `scripts/validate-public-edge.py` | PASS |
| Production DNS/CDN/1Panel/certificate change | Owner-controlled external action | BLOCKED_EXTERNAL |

The successful read-only root includes the current-domain control, direct-origin SNI failure, installed certificate metadata, current virtual-host routing, loopback Controller/Console/API/WebSocket checks, independent Windows probes, no-value secret scan, and final SHA-256 manifest. The exact-commit rerun independently records edge HTTP `525` for all four required paths and direct-origin TLS failure for the same paths. Neither directory contains a production mutation; Gate 13 remains `FAIL`.

## Gate 23 Defect and Dependency Evidence

| Evidence | Location | Status |
|---|---|---|
| Generic unauthenticated JSON rejection implementation | Git commit `03c7557` | VERIFIED |
| JSON rejection targeted validation | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-api-json-20260809T103535Z` | PASS, PRODUCTION UNCHANGED |
| Retained JSON validation/harness failures | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-api-json-20260809T101603Z`, `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-api-json-20260809T102237Z`, `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-api-json-20260809T102250Z` | FAILED NON-AUTHORITATIVE, 33 FILES EACH SEALED |
| Bounded PostgreSQL log policy implementation | Git commit `9764d58` | VERIFIED |
| Protected production PostgreSQL log-policy rollout | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-postgres-logging-20260809T105634Z` | PASS, `10m`/`5`, ROLLBACK EXERCISED |
| Retained PostgreSQL rollout failure and rollback | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-postgres-logging-20260809T105152Z` | RETAINED FAILED EVIDENCE |
| SQLx/Rust/advisory/license remediation | Git commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b` | VERIFIED |
| Exact-head GitHub Actions | GitHub Actions run [`31313868529`](https://github.com/Hinln/xs-nexus/actions/runs/31313868529) | PASS, 4 JOBS, 3 EVIDENCE ARTIFACTS |
| Clean-checkout full Gate 23 validation | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z` | PASS, SHA-256 VERIFIED |
| Retained malformed-CI-input run | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T111531Z` | FAILED NON-AUTHORITATIVE, 16 FILES SEALED |
| Retained invalid-toolchain-command run | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T111735Z` | FAILED NON-AUTHORITATIVE, 19 FILES SEALED |
| Retained disk-pressure termination | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T111856Z` | FAILED NON-AUTHORITATIVE, 36 FILES SEALED |
| Retained pre-fix dependency/harness failure | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T112518Z` | FAILED NON-AUTHORITATIVE, 64 FILES SEALED |
| Retained all-product-pass/disk-health failure | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T130155Z` | FAILED NON-AUTHORITATIVE, 59 FILES SEALED |
| Retained insufficient-headroom cleanup run | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-disk-headroom-20260809T133002Z` | FAILED NON-AUTHORITATIVE, 17 FILES SEALED |
| Exact XS Nexus BuildKit cache cleanup | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-build-cache-cleanup-20260809T133939Z` | PASS, NO GLOBAL PRUNE |
| Independent failed/successful evidence verification | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z` | PASS, SHA-256 VERIFIED |

The full run records successful formatting, strict all-target/all-feature Clippy, all-feature workspace tests, real PostgreSQL and namespace network tests, frontend lint/unit/build/high-severity audit, real Console E2E, executable protocol fuzz, double no-cache image reproducibility, source-risk review, repository/history secret scanning, and full `cargo-audit`/`cargo-deny`. `rsa` is absent from `Cargo.lock`; no vulnerability advisory ignore remains. Plain `cargo audit` has an empty ignore list. The sole `cargo-deny` exception is informational `RUSTSEC-2024-0436` for unmaintained transitive `paste 1.0.15`, with a bounded review recorded in `KI-026`.

The evidence-seal verification rechecked every failed-root manifest and disposition, the successful-root manifest and summary, the production health guard, zero failed systemd units, zero temporary QA containers/networks/namespaces, `1panel-network` ID `7df70648b96ab2d6e5e178cce4e5892d655e7b451dd111f90f42ae86e3757ac0`, subnet `172.18.0.0/16`, and default gateway `10.3.0.1` on `eth0`. Production Controller/Relay/Console continue to run `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; `3bf8619` was not deployed.

## Gate 04 XSP/1 Internal Security Evidence

| Evidence | Path | Result |
|---|---|---|
| Loss-tolerant control retry implementation | Git revision `875395352afc8a17dafc17b2496d62ce701ee4ae` | VERIFIED |
| Exact-head GitHub Actions | GitHub Actions run [`31350065978`](https://github.com/Hinln/xs-nexus/actions/runs/31350065978) | PASS, 4 JOBS |
| Protocol fuzz artifact | GitHub artifact `9048923658`; verified copy `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate04-ci-8753953\protocol-fuzz-evidence` | PASS, SHA-256 VERIFIED |
| Image reproducibility artifact | GitHub artifact `9048762848` | PASS |
| Console real E2E artifact | GitHub artifact `9048653712` | PASS |
| Downloaded artifact no-value scan | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate04-ci-8753953-secret-scan.json` | PASS, 0 FINDINGS, 0 INCOMPLETE |
| Retained pre-format run | GitHub Actions run [`31349380836`](https://github.com/Hinln/xs-nexus/actions/runs/31349380836) | FAILED, RUSTFMT DIFFERENCE RETAINED |
| Retained pre-refactor run | GitHub Actions run [`31349709018`](https://github.com/Hinln/xs-nexus/actions/runs/31349709018) | FAILED/CANCELLED AFTER CLIPPY DIAGNOSIS |

The state-machine regressions prove that lost KeyUpdateAck and PathResponse packets recover through fresh-sequence AEAD retries, duplicate ClientFinish receives the exact cached ServerFinish, and exhausted KeyUpdate retries request a full handshake instead of silently restarting the same update. XSP/1 version, fields, lengths, key derivation, first-message vectors, and primitive selection are unchanged; test-vector metadata, Fuzz target/corpus, normative protocol text, threat model, and compatibility ADR were updated together.

The AddressSanitizer artifact records six targets at 180 seconds each: credential 1,400,229 executions, data 131,659,099, discovery 339,852, handshake 97,594, relay 688,081, and session_state 77,851, totaling 134,262,706. All six status files and the overall summary are PASS; all 525 `SHA256SUMS` entries independently match. This closes only internal Gate 04. Gate 05 independent review, formal keys, real WAN/device gates, signed release, deployment provenance, soak, and other open production gates remain unchanged.

## Gate 24 Dependency Topology Evidence

| Evidence | Path | Result |
|---|---|---|
| Exact-head dependency and upstream topology review | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` | PASS, BOUNDED P2 DISPOSITION |
| Pre-fix exact-head CI | GitHub Actions run [`31324714609`](https://github.com/Hinln/xs-nexus/actions/runs/31324714609), head `45dbc19690f6f738a43cabf2c344118d0b8bc055` | PASS WITH NODE 20 DEPRECATION ANNOTATIONS |
| Node 24 action migration | Commits `e63c59bc7c54b78de94b01885fa9af1b54a31746`, `98ca145c197b1d2af20d7cf5df28008820a25e50`; GitHub Actions run [`31325753985`](https://github.com/Hinln/xs-nexus/actions/runs/31325753985) | PASS, 4 JOBS, 0 ANNOTATIONS |

The root binds source revision `45dbc19690f6f738a43cabf2c344118d0b8bc055`, the source-bundle hash, QA image, lock/policy files, the exact audit script, raw cargo-audit/cargo-deny/advisory-policy output, target-all dependency tree, upstream ref and manifest bytes, host/Docker/1Panel baselines, status ledger, and verified SHA-256 manifest. Raw cargo-audit JSON records zero vulnerabilities and an empty ignore list. Upstream still retains the `paste` dependency, so ADR-089 rejects a private netlink fork and keeps a fail-closed `2026-08-31` review deadline.

Production Controller/Relay/Console remained healthy at `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; `1panel-network` ID/subnet, default route, IP rules, canonical nftables, containers, Docker networks, and failed units were unchanged, and temporary QA resources were absent. This evidence closes only `KI-026`; Gate 24 remains `PARTIAL`.

## V2 Evidence Rules

- Every new run gets an immutable UTC timestamped directory outside Git.
- Evidence records exact Git revision, command, tool version, environment, exit status, and SHA-256 manifest.
- Secret scans run before evidence is indexed or copied.
- A Markdown conclusion never substitutes for raw logs, reports, packet captures, screenshots, signed manifests, or runtime inspection.
- Failed evidence is retained and clearly marked failed.

## Pending V2 Evidence

- Gate 01 owner-controlled formal key ceremony, signed RC tag/bundle, authenticated public-key publication, and merge to `main`. Branch production deployment, runtime reverse verification, and rollback to `ff9551d3` are now evidenced.
- Gate 02 no-value secret inventory, rotation receipts, old-value rejection checks, deep artifact/history/layer scan.
- Gate 14 independent warning/critical disk-alert delivery and on-call acknowledgement. SSH/firewall, security-update/reboot regression, and bounded disk cleanup are now evidenced.
- Gate 13 origin TLS chain, SNI, CDN mode, browser/API/WebSocket/Console E2E.
- Gates 06/08/09/15/18/19/20/21/22/25 current-revision regressions. Gate 04 internal evidence is complete, while Gate 05 independent review remains external. Gate 23 self-fixable exact-head checks are complete, but the gate remains failed until all global Critical/High findings are closed; Gate 24 has completed the dated `paste` review but still requires formal release/deployment and bounded glibc disposition closure.
- External Gate evidence for Windows, NAS, WAN, subnet router, offsite restore, key ceremony, and independent audit.
