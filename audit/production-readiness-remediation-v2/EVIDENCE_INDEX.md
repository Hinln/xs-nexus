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
- Gates 04/06/08/09/15/18/19/20/21/22/23/24/25 current-revision regressions.
- External Gate evidence for Windows, NAS, WAN, subnet router, offsite restore, key ceremony, and independent audit.
