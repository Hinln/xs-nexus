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
- Gates 15/18/19/20/21/22/25 current-revision regressions. Gate 09 is complete. Gate 08 hosted resource/capacity/restart evidence is complete, while its public-WAN/multi-region/long-duration matrix remains external. Gate 04 internal evidence is complete, while Gate 05 independent review remains external. Gate 23 self-fixable exact-head checks are complete, but the gate remains failed until all global Critical/High findings are closed; Gate 24 has completed the dated `paste` review but still requires formal release/deployment and bounded glibc disposition closure.
- External Gate evidence for Windows, NAS, WAN, subnet router, offsite restore, key ceremony, and independent audit.
