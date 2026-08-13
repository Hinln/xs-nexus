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

## Gate 06 Linux Agent Recovery Evidence

| Evidence | Path | Result |
|---|---|---|
| Recovery harness implementation | Git revisions `998e06fe8d11c7186d6c883e9dce40e7abda23c9`, `b967691547b99b002692ab4d413a6215ac1f6884`, `9ca211512f9aac2fae0a78ef22e288818be9b76a`, `fb45fd43256d65cb4c72824d6cee0bec0884ad02` | VERIFIED |
| Exact-head GitHub Actions | GitHub Actions run [`31352258781`](https://github.com/Hinln/xs-nexus/actions/runs/31352258781), exact head `fb45fd43256d65cb4c72824d6cee0bec0884ad02` | PASS, 5 JOBS |
| Real-systemd recovery job | GitHub Actions job [`93345151258`](https://github.com/Hinln/xs-nexus/actions/runs/31352258781/job/93345151258) | PASS |
| Recovery artifact | GitHub artifact `9049384061`; verified copy `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate06-ci-fb45fd4` | PASS, ARCHIVE SHA-256 VERIFIED |
| Artifact archive digest | `2a88d3a6344c66c699daff9966119df0a77718bcb6b68499ac43a293c742009f` | VERIFIED AGAINST GITHUB METADATA |
| Inner evidence checksums | `test.log` `53a3cd38ef1f6d7fabb7e9b0facaa2615fd9f2b235798dfa5d156456459dcce8`; `summary.txt` `14db1ee65fcb6d7d1e90fa8373375a92f6ce8395ec29094f3fe683a3af64364a` | VERIFIED |
| Downloaded artifact no-value scan | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate06-ci-fb45fd4` | PASS, 0 FINDINGS |
| Retained ShellCheck failure | GitHub Actions run [`31351583134`](https://github.com/Hinln/xs-nexus/actions/runs/31351583134) | FAILED JOB RETAINED; SUPERSEDED RUN CANCELLED |
| Retained protected-home execution failure | GitHub Actions run [`31351658402`](https://github.com/Hinln/xs-nexus/actions/runs/31351658402) | `203/EXEC` RETAINED; SUPERSEDED RUN CANCELLED |
| Retained host-path unit-verification failure | GitHub Actions run [`31352027606`](https://github.com/Hinln/xs-nexus/actions/runs/31352027606), failed artifact `9049311530` | FAILED JOB/ARTIFACT RETAINED; SUPERSEDED RUN CANCELLED |

The passing job uses a unique test-only binary below `/usr/local/lib/xs-nexus-tests/<unit>`, validates the packaged service file through an isolated `systemd-analyze --root` tree, and never creates or changes the formal `/usr/local/lib/xs-nexus/current` installation. The transient service uses `PrivateNetwork`, real `/dev/net/tun`, `Restart=on-failure`, and the production sandbox boundaries relevant to TUN/Netlink. A forced crash produced exactly one new main PID, retained state, recreated the TUN only inside the service namespace, survived an isolated network-link change, and cleaned the unit/interface on stop.

This is authoritative automated evidence for the crash/restart and isolated link-change submatrix only. It is not evidence of an ordinary deployed-host reboot, disk-full behavior, DHCP lease/address churn, coexistence with another VPN and competing routes, real arm64 hardware, or NAS operation. Those items require a disposable approved Linux host and remain open; Gate 06 stays `PARTIAL`.

## Gate 08 Relay Security and Resilience Evidence

| Evidence | Path | Result |
|---|---|---|
| Global admission/queue implementation and dedicated regression | Git revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` | VERIFIED |
| Exact-head GitHub Actions | GitHub Actions run [`31358498444`](https://github.com/Hinln/xs-nexus/actions/runs/31358498444), exact head `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` | PASS, ALL 6 JOBS |
| Relay resilience job | GitHub Actions job [`93362562136`](https://github.com/Hinln/xs-nexus/actions/runs/31358498444/job/93362562136) | PASS |
| Relay resilience artifact | GitHub artifact `9051561308`; verified copy `C:\Users\panyo\Documents\Codex\2026-07-29\yue\relay-pass-bad114e-artifact` | PASS, PORTABLE SHA-256 VERIFIED |
| Artifact archive digest | `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67` | VERIFIED AGAINST GITHUB METADATA |
| Capacity report | 5,000,000 × 216-byte frames; 70.213 seconds; 71,212.14 packet/s; 14.67 MiB/s; internal latency average 5 µs/max 150 µs | PASS, ZERO RELAY DROPS, ZERO FINAL QUEUE |
| Downloaded artifact no-value scan | Same verified copy | PASS, 0 FINDINGS |
| Retained restart-classification failure | GitHub Actions run [`31354320136`](https://github.com/Hinln/xs-nexus/actions/runs/31354320136), Relay artifact `9050139049` | FAILED ASSERTION RETAINED; SUPERSEDED RUN CANCELLED |
| Retained report-number compilation failure | GitHub Actions run [`31355195399`](https://github.com/Hinln/xs-nexus/actions/runs/31355195399), Relay artifact `9050386927` | FAILED COMPILATION RETAINED; SUPERSEDED RUN CANCELLED |
| Retained relative report-path failure | GitHub Actions run [`31355385268`](https://github.com/Hinln/xs-nexus/actions/runs/31355385268), Relay artifact `9050465451` | FAILED REPORT WRITE RETAINED; SUPERSEDED RUN CANCELLED |
| Retained idle-Lease capacity failure | GitHub Actions run [`31355663342`](https://github.com/Hinln/xs-nexus/actions/runs/31355663342), Relay artifact `9050582896` | FAILED AT REAL IDLE EXPIRY; SUPERSEDED RUN CANCELLED |
| Retained fixture-identity compilation failure | GitHub Actions run [`31356055876`](https://github.com/Hinln/xs-nexus/actions/runs/31356055876), Relay artifact `9050672375` | FAILED COMPILATION RETAINED; SUPERSEDED RUN CANCELLED |
| Retained strict-Clippy failure | GitHub Actions run [`31356167042`](https://github.com/Hinln/xs-nexus/actions/runs/31356167042), passing Relay artifact `9050798784` | RELAY PASS, BASELINE LINT FAIL RETAINED |

The artifact's outer manifest verifies `test.log`, `unit-tests.log`, the capacity manifest, environment, report, revision, test output, and summary. The capacity manifest now records relative names and verifies directly after download without runner-path rewriting. Unit coverage includes global source spray, shared packet/byte capacity, rejection without replay/rate-state consumption, and cleanup release. The integration log ends with authenticated fallback, ciphertext, failover, restart recovery, and Direct restoration passing.

This evidence closes only the safely reproducible hosted submatrix. Public-WAN packet loss/reordering, distributed valid credentials, volumetric DDoS, cloud mitigation, multi-region/multi-instance behavior, and hours/days capacity remain `PRV2-018`/`KI-028`; Gate 08 stays `PARTIAL`.

## Gate 24 Dependency Topology Evidence

| Evidence | Path | Result |
|---|---|---|
| Exact-head dependency and upstream topology review | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` | PASS, BOUNDED P2 DISPOSITION |
| Pre-fix exact-head CI | GitHub Actions run [`31324714609`](https://github.com/Hinln/xs-nexus/actions/runs/31324714609), head `45dbc19690f6f738a43cabf2c344118d0b8bc055` | PASS WITH NODE 20 DEPRECATION ANNOTATIONS |
| Node 24 action migration | Commits `e63c59bc7c54b78de94b01885fa9af1b54a31746`, `98ca145c197b1d2af20d7cf5df28008820a25e50`; GitHub Actions run [`31325753985`](https://github.com/Hinln/xs-nexus/actions/runs/31325753985) | PASS, 4 JOBS, 0 ANNOTATIONS |

The root binds source revision `45dbc19690f6f738a43cabf2c344118d0b8bc055`, the source-bundle hash, QA image, lock/policy files, the exact audit script, raw cargo-audit/cargo-deny/advisory-policy output, target-all dependency tree, upstream ref and manifest bytes, host/Docker/1Panel baselines, status ledger, and verified SHA-256 manifest. Raw cargo-audit JSON records zero vulnerabilities and an empty ignore list. Upstream still retains the `paste` dependency, so ADR-089 rejects a private netlink fork and keeps a fail-closed `2026-08-31` review deadline.

Production Controller/Relay/Console remained healthy at `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; `1panel-network` ID/subnet, default route, IP rules, canonical nftables, containers, Docker networks, and failed units were unchanged, and temporary QA resources were absent. This evidence closes only `KI-026`; Gate 24 remains `PARTIAL`.

## Gate 09 ACL Enforcement Evidence

| Evidence | Path | Result |
|---|---|---|
| Policy rollback fix | Git revision `3f27ed9` | VERIFIED |
| Final exact source | Git revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31360862865`](https://github.com/Hinln/xs-nexus/actions/runs/31360862865) | PASS, ALL 7 JOBS |
| ACL job | GitHub Actions job [`93369332314`](https://github.com/Hinln/xs-nexus/actions/runs/31360862865/job/93369332314) | PASS |
| Final ACL artifact | Artifact `9052383034`, `acl-enforcement-evidence` | PASS |
| Artifact archive digest | `fd5cbc219591264ae6f1376db2d5c4aa9949c33be4684196887a3703a9ef8e23` | VERIFIED |
| Downloaded inner manifest | `SHA256SUMS` hash `8a43cf8a4619c957baf7e108c68b4e4f879e6934bbf42ab169f0aa14b97a5205` | PASS, 6 PAYLOADS |
| Downloaded no-value scan | Repository scanner over extracted artifact | PASS, 0 FINDINGS |
| Superseded terse artifact | Run `31360425165`, artifact `9052236354` | PASSING BUT NON-FINAL; ASSERTION MARKERS INCOMPLETE |

The final artifact binds exact revision, configuration rollback tests, forged Node/source identity tests, a three-Agent disconnected-Controller Direct matrix, authenticated Relay bypass regression, and approved subnet-router bypass regression. Structured markers prove offline policy retention, sender deny, receiver deny, Relay deny, and subnet TCP/UDP deny; traffic tests use real Agent processes, TUN interfaces, XSP/1/XSR/1 frames, and Linux namespaces.

Gate 09 is `PASS`. The artifact does not claim public-WAN conditions, real NAS/router operation, independent protocol audit, formal release provenance, or production deployment. Production remains unchanged and overall status remains `NO_GO`.

## Gate 15 1Panel Coexistence Evidence

| Evidence | Path | Result |
|---|---|---|
| Exact current source | Git revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31504402285`](https://github.com/Hinln/xs-nexus/actions/runs/31504402285) | PASS, ALL 8 JOBS |
| Dedicated coexistence job | GitHub Actions job [`93822197946`](https://github.com/Hinln/xs-nexus/actions/runs/31504402285/job/93822197946) | PASS |
| Coexistence artifact | GitHub artifact `9106406005`, `onepanel-coexistence-evidence` | PASS |
| Artifact archive digest | `93e174890d19264398090dcb891ab434a3839d769c4f0f14854982245678a28f` | VERIFIED AGAINST INDEPENDENT DOWNLOAD |
| Downloaded evidence | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate15-ci-8a91748` | PASS |
| Downloaded inner manifest | `SHA256SUMS` hash `46c23da39ac14ecf545806f4f02cdea276d25cadee1ba931fe973f469f058191` | PASS, 4 PAYLOADS |
| Downloaded no-value scan | Same verified copy | PASS, 0 FINDINGS |
| Real production host reboot and 1Panel/OpenResty/SSH regression | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z` | PASS |
| Production project upgrade and four automatic rollbacks | `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z` | PASS |
| Full Gate 15 mapping | `audit/production-readiness-remediation-v2/ONEPANEL_COEXISTENCE.md` | PASS |

The hosted fixture fails closed when the exact network already exists, so it cannot run against an actual 1Panel host. It proves current-source external-network ownership, project-scoped lifecycle behavior, unrelated sentinel survival, exact target network identity across a real Docker-daemon restart, stable network inventory/default route, and successful cleanup. The production roots separately prove real host reboot, 1Panel/OpenResty/SSH recovery, project upgrade/restart, automatic rollback, database boundary, and preservation of the production external network.

Failed runs `31364684902`, `31366046340`, `31367632436`, and `31504209932` and artifacts `9053670838`, `9054165902`, `9054770008`, and `9106338810` remain retained. They document Compose interpolation, profile activation, opaque cleanup status, and hosted built-in bridge-ID assumptions. No assertion was skipped or weakened; the final PASS occurs only after target identity and cleanup/baseline checks complete.

Gate 15 is `PASS`. This is not a formal release, production deployment of the current branch, planned-domain repair, credential rotation, external-device proof, or overall production approval; overall status remains `NO_GO`.

## Gate 18 Update Supply-Chain Evidence

| Evidence | Path | Result |
|---|---|---|
| Candidate source | Git revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31515281011`](https://github.com/Hinln/xs-nexus/actions/runs/31515281011) | PASS, ALL 9 JOBS |
| Dedicated supply-chain job | GitHub Actions job [`93858773221`](https://github.com/Hinln/xs-nexus/actions/runs/31515281011/job/93858773221) | PASS |
| Supply-chain artifact | GitHub artifact `9110864186`, `update-supply-chain-evidence` | PASS |
| Artifact archive digest | `9885bc0d7146d518d53d6de8a11a8a7702d1816b5a49378dc13a797b85be2376` | VERIFIED AGAINST GITHUB AND INDEPENDENT DOWNLOAD |
| Downloaded evidence | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate18-ci-b8cd49c` | PASS |
| Downloaded inner manifest | `SHA256SUMS` hash `b43c39434453c8d30bea945faa1ff14005430a2432581d04a6a3596285469c66` | PASS, 8 PAYLOADS |
| Downloaded no-value scan | Same verified copy | PASS, 0 FINDINGS, 0 INCOMPLETE SURFACES |
| Full Gate 18 mapping and retained failure chain | `audit/production-readiness-remediation-v2/UPDATE_SUPPLY_CHAIN.md` | PARTIAL |

The internal matrix proves canonical schema-2 release identity, strict numeric/archive bounds, bounded key overlap, Controller and host-local revocation, interrupted/truncated/oversized/ENOSPC failure handling, platform/architecture binding, rollback safety, identity/state preservation, namespace route cleanup, and irreversible Console revocation controls. Eight preceding runs and artifacts retain every ShellCheck, fixture, installer, rollback-ordering, Relay convergence, Clippy, and SQLx failure; no failure was skipped, suppressed, or weakened.

Gate 18 remains `PARTIAL`. Hosted x86_64, development keys, namespace routes, tmpfs ENOSPC, and a local ledger do not replace the owner-controlled offline key ceremony, authenticated production key/revocation distribution, formally signed RC, real arm64/NAS/Windows execution, or the production release/rollback chain. Production was not changed and overall status remains `NO_GO`.

## Gate 19 Console and Visual UX Evidence

| Evidence | Path | Result |
|---|---|---|
| Candidate source | Git revision `5505893710ab1d15e06495603dff08bf5c1e035f` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31529393933`](https://github.com/Hinln/xs-nexus/actions/runs/31529393933) | PASS, ALL 9 JOBS |
| Dedicated Console job | GitHub Actions job [`93905489516`](https://github.com/Hinln/xs-nexus/actions/runs/31529393933/job/93905489516) | PASS |
| Console artifact | GitHub artifact `9116327161`, `console-real-e2e-evidence` | PASS |
| Artifact archive digest | `f6d4903febab15e6c92bd46ee91451bfbea849fae84a166afd4aa1c0b67632d6` | VERIFIED AGAINST GITHUB AND INDEPENDENT ZIP DOWNLOAD |
| Downloaded evidence | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\ci-evidence\31529393933-console` | PASS |
| Downloaded inner manifest | `SHA256SUMS` hash `abe90dbf2525ee4587c1d521208253c262c3f3418d2ffee354b10cf7fcd78b31` | PASS, 139 LISTED = 139 DOWNLOADED FILES |
| Playwright results | `results.json`, `scenario-matrix.tsv`, `browser-observations.json` | 8 EXPECTED, 0 UNEXPECTED/SKIPPED/FLAKY; 14 OBSERVATIONS; 0 PAGE ERROR/5XX |
| Visual evidence | `screenshots/` | PASS, 132 PNG FILES ACROSS 6 VIEWPORTS AND REAL STATES |
| Source guard | `source-guard.log` | PASS, REQUEST INTERCEPTION ABSENT |
| Downloaded no-value scan | Same verified copy | PASS, 0 FINDINGS |
| Full Gate 19 mapping and retained failure chain | `audit/production-readiness-remediation-v2/CONSOLE_VISUAL_UX.md` | PARTIAL |

The matrix uses a dedicated real PostgreSQL schema, the actual Controller binary, a production Console build/preview and Chromium. It covers real loading, login, every management page, data initialization, rapid double activation, ACL/API boundaries, multi-tab stale-state rejection, all required viewports, destructive confirmation, auditor UI/server authorization, logout invalidation and real offline recovery. The current deployment does not use Redis in this path, and no Redis or API mock is substituted.

Nine preceding workflow states retain the full diagnostic chain for ShellCheck, duplicate submission, browser pattern/trace handling, CSRF tab rotation, API contract assertions, selector ambiguity, hosted package infrastructure, successful-204 browser classification and artifact manifest portability. Two failed trace-bearing artifacts were deleted after diagnosis because they persisted generated test values; failed workflow logs remain. No final test was skipped, suppressed or weakened.

Gate 19 remains `PARTIAL`. Internal hosted runtime and visual evidence cannot replace the owner-controlled planned-domain origin certificate/SNI/vhost, CDN strict routing, public browser/API/WebSocket path or deployment of the current branch. Production was not changed and overall status remains `NO_GO`.

## Gate 20 Production Observability Evidence

| Evidence | Path | Result |
|---|---|---|
| Exact current source | Git revision `9291400ac030045e8ea2955ea137e7dc8be37a85` | VERIFIED |
| Exact-head CI | GitHub Actions run [`31537716553`](https://github.com/Hinln/xs-nexus/actions/runs/31537716553) | PASS, ALL 10 JOBS |
| Dedicated observability job | GitHub Actions job [`93932721320`](https://github.com/Hinln/xs-nexus/actions/runs/31537716553/job/93932721320) | PASS |
| Observability artifact | GitHub artifact `9119468792`, `production-observability-evidence` | PASS |
| Artifact archive digest | `7b5fadda34a612716a6176767b465006a5e6e4175a61464e6959f4ec64fd5c79` | VERIFIED AGAINST INDEPENDENT ZIP DOWNLOAD |
| Downloaded evidence | `C:\Users\panyo\Documents\Codex\2026-07-29\yue\evidence-31537716553-zip` | PASS |
| Outer manifest | `SHA256SUMS` hash `d6d0b12763e7bd10e1e37e9cc1384db40f8b353a5e8bd8776e57d35ef1727f95` | PASS, 21 ENTRIES |
| Host manifest | `host/SHA256SUMS` hash `b2713ff0203d14df272cb2c5fe43be00d93f30ba9f2ef15d5001855656ee7a88` | PASS, 13 ENTRIES |
| Scenario matrices | Top 5 checks and host 11 scenarios | PASS, 0 SKIPPED/ALLOWED FAILURE |
| Downloaded no-value scans | GitHub extraction and independent ZIP extraction | PASS, 0 FINDINGS |
| Full Gate 20 mapping and failure chain | `audit/production-readiness-remediation-v2/OBSERVABILITY.md` | PARTIAL |

The matrix uses real PostgreSQL and real local TLS, verifies the hardened systemd service/timer, and exercises host, Docker, HTTP, Controller, certificate, backup, warning, critical, deduplication, severity change, resolved, delivery failure, queue, private-file, proxy, credential non-persistence, and atomic-write boundaries. Agent/Relay reports are authenticated, cumulative, monotonic, bounded, and aggregated without subject identifiers.

Failed run `31537181594`, artifact `9119262890`, and archive digest `929f20a399fe2bb61842b13291b6a70b4799015ca022723e512eb70acc0af264` retain the ShellCheck and unset-fixture failures. Failed run `31537531579`, artifact `9119401167`, and archive digest `96fba0425835127f992781e505ae5edd0cea70c4d0fce5dde366b1e6744c4aa8` retain the incomplete Webhook fake. Earlier run `31535053669` retains migration/lint failures. No product assertion or threshold was skipped, suppressed, allowed to fail, or weakened.

Gate 20 remains `PARTIAL/BLOCKED_EXTERNAL`. Localhost delivery cannot replace a destination independent of the monitored production host, real warning/critical receipt, documented on-call acknowledgement/escalation, resolved closure, formal retention, or continuous production TLS/backup evidence. Production was not changed and overall remains `NO_GO`.

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
- Gate 06 ordinary-host reboot, disk-full, DHCP/address-churn, competing-VPN and arm64 matrix; its hosted real-systemd crash/restart and isolated link-change submatrix is complete.
- Gates 21/22/25 current-revision regressions. Gate 20's repository metrics and alert matrix is complete, while independent production delivery/on-call/resolved closure and formal TLS/backup evidence remain external. Gate 19's repository runtime/visual matrix is complete, while its Gate 13 planned-domain strict TLS/CDN/public browser path remains external. Gate 18's repository-side failure matrix is complete, while formal keys/ceremony, authenticated production distribution, signed RC, real target-platform execution, and the production release chain remain open. Gates 09 and 15 are complete. Gate 08 hosted resource/capacity/restart evidence is complete, while its public-WAN/multi-region/long-duration matrix remains external. Gate 04 internal evidence is complete, while Gate 05 independent review remains external. Gate 23 self-fixable exact-head checks are complete, but the gate remains failed until all global Critical/High findings are closed; Gate 24 has completed the dated `paste` review but still requires formal release/deployment and bounded glibc disposition closure.
- External Gate evidence for Windows, NAS, WAN, subnet router, offsite restore, key ceremony, and independent audit.

## Gate 21 Performance and Capacity (2026-08-12)

- Exact revision: `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363`.
- GitHub Actions run: `31603852656`; performance job: `94137661764`.
- Artifact: `9144433450`, `performance-capacity-evidence`; GitHub digest `sha256:d0c05972b353476362cd1e62ff86f4bfc77fa027371bc5e67977131ab686f0b8`.
- Independent extracted verification: outer manifest hash `9f6590bc3b586652c4b8d9346a5d274f68cc184e80c171f8242ec459f88550f5`, 41/41 files; Agent 11/11, Controller 8/8, Protocol 8/8; exact revision bindings matched; zero secret-scan findings.
- Scope: 1,000 enrolled nodes, 1,000 authenticated control sessions, 1,001st enrollment/session fail-closed, bounded `chunked-v1` large-response transport, resource ceilings, XSP/1 throughput, and two-Agent namespace Direct/Relay RTT.
- Detailed report: `audit/production-readiness-remediation-v2/PERFORMANCE_CAPACITY.md`.
- Decision: `PARTIAL/BLOCKED_EXTERNAL`; public-WAN, multi-region/multi-instance, volumetric/distributed-abuse, and hours/days proof remains. Gate 22 remains `UNKNOWN`.

## Gate 22 Soak Harness Calibration (2026-08-13)

- Exact revision: `6e63424298e491c0035e1d138de5d03b0ab83c27`.
- GitHub Actions run/job: `31649030446` / `94289075250`; workflow result 13/13 PASS.
- Artifact: `9162201059`, `current-revision-soak-calibration-evidence`; GitHub digest `sha256:73918119786fe5d25dcceb7f8cdb9509ff3a1ac5439780e1972c4201e8d08e59`.
- Independent extraction: 84/84 `SHA256SUMS` entries passed and the repository secret scanner reported zero findings.
- Raw scope: 600 seconds, 44 samples for each of Controller, Relay, Console, PostgreSQL and two Agents; 264 rows total. Eight required fault events passed. Nine before/after Docker, route, rule, link, nftables, service and `1panel-network` invariants were byte-identical.
- Decision: calibration PASS, Gate 22 `UNKNOWN`. No >=86,400-second evidence exists. Formal execution is externally blocked by an unverified changed SSH host key and lack of an alternate approved privileged Linux QA host; production was not changed.

## Gate 25 Clean Release Rehearsal Submatrix (2026-08-13)

- Exact revision: `6e63424298e491c0035e1d138de5d03b0ab83c27`.
- GitHub Actions run/job: `31649030446` / `94289075249`; workflow result 13/13 PASS.
- Artifact: `9162246723`, `clean-release-rehearsal-evidence`; GitHub digest `sha256:d69d7ff0d24195f629a5032152b1c1851f0cab8d79048ae90ca00af5fa9659fe`.
- Independent extraction: 111/111 `SHA256SUMS` entries passed and the repository secret scanner reported zero findings.
- Raw scope: full-history Git bundle and exact detached checkout, ephemeral test-only signed tag, five-image double no-cache reproducibility, external generated secrets and least-privilege PostgreSQL roles, real namespace Direct ACL/Relay/subnet, signed update matrix, and two complete Docker lifecycles.
- Cleanup scope: 10/10 phases, 10/10 fixture invariants and 10/10 final invariants passed; Docker containers/networks/volumes, default route, rules, links, namespaces, nftables, failed services and `1panel-network` returned to baseline.
- Formal boundary: artifact values are `formal_signed_rc=false`, `independent_operator=false`, and `production_mutation=false`. Gate 25 stays `FAIL` until an owner-signed RC is rehearsed by an independent operator on a fresh non-hosted server and the current production upgrade/rollback chain passes.
- Detailed report: `audit/production-readiness-remediation-v2/RELEASE_REHEARSAL.md`.

## Exact-Head Relay Convergence Regression (2026-08-13)

- Failed exact-head revision/run: `f4c87074a65cf8e8b8df41664700a3f81131a40c` / `31652099523`; 12/13 jobs passed and Relay job `94298488921` failed on the first ICMP sample after primary-Relay loss. Failed artifact `9163073115` and its raw logs remain retained.
- Root cause: the harness verified only the sending Agent's failover state before asserting a bidirectional exchange, while Relay failure detection is independent on both Agents. The fix requires both Agents to report the exact established Relay or Direct path at every transition; product timers, per-Agent wait bounds, packet success, ACL, encryption, and cleanup assertions are unchanged.
- Passing exact revision/run: `c0c059e84806f70586f37ca3f3bb33cdd602c4a4` / [`31653564044`](https://github.com/Hinln/xs-nexus/actions/runs/31653564044), all 13 jobs PASS.
- Relay job/artifact: [`94302909276`](https://github.com/Hinln/xs-nexus/actions/runs/31653564044/job/94302909276) / `9163588782`; GitHub digest `sha256:8b6495407a822ebefaa6259e5c64f43effa61628250550705648b2f217c05803`. Independent extraction passed 8/8 outer and 4/4 capacity hashes plus a zero-finding secret scan.
- Gate 25 affected artifact: `9163844194`; GitHub digest `sha256:376e136188c0782b2af3797395c0b871fb39747c007475c4241c58075ce84a08`. Independent extraction passed 110/110 outer and 8/8 nested hashes, all 10 phases, both sets of 10 invariants, and a zero-finding secret scan.
- Formal boundary is unchanged: the artifact records `formal_signed_rc=false`, `independent_operator=false`, and `production_mutation=false`. Gate 25 remains `FAIL`, Gate 22 remains `UNKNOWN`, and overall status remains `NO_GO`.

## Exact-Head Recovery And Performance Closure (2026-08-13)

- Retained Gate 25 failure: revision/run/job/artifact `b18b3171d261839c251f5273c6e89d65917d8a5d` / `31654905139` / `94307126471` / `9164303385`; GitHub digest `sha256:ecdebd8bc487ba85b02b9f47e6ccfce06437dda33e42f4b8426dc5c4d0ea5ea7`. The exact authenticated receiver endpoint was active with legitimate `relay_failover`, while the old harness allowed only `relay_fallback`.
- Retained Gate 22 failure: revision/run/job/artifact `df9b452b951c7763d733d0cfa7774e28a15b4e2d` / `31655990346` / `94310433905` / `9164596254`; GitHub digest `sha256:fbfb49cd42b46f5163086fbca9e39ef1206c1a04367e8bf5669451115634d5e6`. Both versions applied, but the immediate post-update ping hit `agent_peer_session_unavailable` before both Direct paths recovered.
- Retained performance failure: revision/run/job/artifact `d56d672a1c8156f276437e1db018204eb2fe661d` / `31657413129` / `94314821959` / `9164999014`; GitHub digest `sha256:360f2873b4aa8f252f1592cf90d16666d77a88a6c0531fcf50a963eaae797e23`. Direct warm-up p95 was `27.383` ms and the 100-sample Direct average/p95 was `7.242`/`39.441` ms, correctly failing unchanged 5/10 ms bounds.
- Passing exact revision/run: `795b1ea461a179958aed27e6935faba8f36e43ce` / [`31658778589`](https://github.com/Hinln/xs-nexus/actions/runs/31658778589), all 13 jobs PASS.
- Performance job/artifact: `94318968086` / `9165514010`; GitHub digest `sha256:71a187d3453f13f19f4a0a585cc75d9fafcb69195870678f8cd0d7c4713911f1`. Independent extraction passed 43 outer, 13 Agent, 8 Controller and 8 Protocol hashes; Direct/Relay p95 was `0.408`/`0.465` ms and the no-value scan passed.
- Gate 22 job/artifact: `94318968217` / `9165687987`; GitHub digest `sha256:7387b961dc813b033cc6b80cfe9e23ebec5fb858c858f4ecc1c4cfbef5d298f9`. Independent extraction passed 84/84 hashes, 600 seconds, 258 resource rows, eight events, nine byte-identical host invariants and the no-value scan.
- Relay job/artifact: `94318968289` / `9165498146`; GitHub digest `sha256:1494e05a86be00a75fc51eb9be751dcc50349b6e2a9b84eb12132aa69657088b`. Independent extraction passed 8 outer paths after deterministic upload-root normalization and 4/4 capacity hashes.
- Gate 25 job/artifact: `94318968293` / `9165661918`; GitHub digest `sha256:08a9a05c09a827b8d4cfc35c0eb0d4a5ae086be125b24e0c17ac45b83f7e6ffa`. Independent extraction passed 110 outer and 8 nested hashes, 10/10 phases, 10/10 fixture invariants, 10/10 cleanup invariants and the no-value scan.
- Formal boundary is unchanged: Gate 22 has only 600 seconds, while Gate 25 still records `formal_signed_rc=false`, `independent_operator=false`, and `production_mutation=false`. Production was not contacted or changed; overall remains `NO_GO`.

## Docker Occupied-Port Fixture Closure (2026-08-13)

- Retained failed revision/run/job/artifact: `999b420cbf274e47bf28d3a4f86600e4f81f72c4` / `31660307047` / `94323554733` / `9166162670`; GitHub digest `sha256:51cffde14a0d1f74e498b76032336ed04147bb6b512c760e5bdb1f1bb56f4206`. The first eight Gate 25 phases passed, then the Docker lifecycle negative fixture ran preflight before proving its listener was ready.
- Passing exact revision/run: `67152427acc197deca49443a8527c74b18d71098` / [`31661323846`](https://github.com/Hinln/xs-nexus/actions/runs/31661323846), all 13 jobs PASS.
- Performance job/artifact: `94326567382` / `9166384854`; GitHub digest `sha256:0596b994919de5f083006ee4f4093dab4c0cdf5dfbdb649656b9b9e06a5708a3`. Independent extraction passed 72 manifest entries; Direct/Relay p95 was `0.385`/`0.431` ms and the no-value scan passed.
- Gate 22 job/artifact: `94326567469` / `9166550402`; GitHub digest `sha256:a24e372d230ed2b48decac796838189f5bd78f951bed0c81359dc4f171b2d91e`. Independent extraction passed 84/84 hashes, 600 seconds, 258 resource rows, eight events, nine byte-identical host invariants and the no-value scan.
- Relay job/artifact: `94326567416` / `9166363481`; GitHub digest `sha256:17b4fd8f12fb6b39b205465261b0e502f159741d4d06c085b4a2b46ec52cc458`. Independent extraction passed 8 normalized outer paths and 4/4 nested capacity hashes.
- Gate 25 job/artifact: `94326567391` / `9166570284`; GitHub digest `sha256:b19a27070227992de3c5440fae4377d67e58351f4b012fdd9b3129fdee954825`. Independent extraction passed 110 outer and 8 nested hashes, 10/10 phases, 10/10 fixture invariants, 10/10 cleanup invariants and the no-value scan.
- Formal boundary is unchanged: this closes a hosted test-fixture defect only. Gate 22 remains `UNKNOWN`, Gate 25 remains `FAIL`, production was not contacted or changed, and overall remains `NO_GO`.
