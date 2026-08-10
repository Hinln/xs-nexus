# Open Remediation Findings

| ID | Severity | Gate | Finding | Status |
|---|---|---:|---|---|
| PRV2-001 | Critical | 02 | Previously disclosed infrastructure, application, enrollment, and recovery credentials lack complete rotation and old-value rejection evidence | OPEN |
| PRV2-002 | Critical | 03/18 | No formal offline release/recovery key ceremony, authenticated public-key distribution, revocation, or restore exercise | BLOCKED_EXTERNAL |
| PRV2-003 | High | 14 | SSH/firewall, protected security updates, new-kernel reboot regression, and bounded disk cleanup are closed; independently delivered and acknowledged warning/critical disk alerts remain open | BLOCKED_EXTERNAL |
| PRV2-004 | High | 16 | Long-running Controller database access depended on a bootstrap superuser; production now uses separated owner/app/migrator roles | CLOSED |
| PRV2-005 | High | 13/19 | Planned production domain returns CDN 525 because planned-domain origin SNI/certificate/vhost is absent; current public root is also routed to Controller rather than Console | BLOCKED_EXTERNAL |
| PRV2-006 | High | 05 | Proprietary protocol, cryptography, API, Relay, Agents, Windows, and supply chain have no independent audit and retest | BLOCKED_EXTERNAL |
| PRV2-007 | High | 11 | Windows online enrollment, SCM, routing, sleep, upgrade, rollback, and cleanup matrix is incomplete | BLOCKED_EXTERNAL |
| PRV2-008 | High | 12 | Real NAS ordinary-node and subnet-router evidence is absent | BLOCKED_EXTERNAL |
| PRV2-009 | High | 17 | Backup replica remains in the same failure domain and no clean-server restore exists | BLOCKED_EXTERNAL |
| PRV2-010 | High | 20 | Local health guard lacks independent external delivery, formal on-call, TLS-expiry and full metrics closure | OPEN |
| PRV2-011 | High | 22 | Current remediation revision has no mandatory 24-hour soak evidence | OPEN |
| PRV2-012 | High | 01/25 | Revision `3d93656` is built, deployed, reverse-verified and repeatedly rolled back to `ff9551d3`, but is not merged to `main` or covered by an owner-controlled signed RC tag/bundle | OPEN |
| PRV2-013 | High | 23/24 | SQLx `0.8.6` pulled unreachable `rsa` code affected by `RUSTSEC-2023-0071`; the advisory was previously ignored | CLOSED at `3bf8619`: SQLx `0.9.0`, `rsa` absent, ignore removed, plain audit PASS |
| PRV2-014 | Medium | 20/23 | Production PostgreSQL Docker logs had no bounded rotation policy | CLOSED at `9764d58`: `10m`/`5` policy applied with rollback and production invariants |
| PRV2-015 | Medium | 19/23 | Framework JSON extraction errors exposed detailed parser rejection text to unauthenticated clients | CLOSED at `03c7557`: exact generic 400 envelope and negative regressions |
| PRV2-016 | High | 24 | `cargo-deny` did not enforce a complete workspace license policy | CLOSED at `3bf8619`: explicit allowlist and full all-feature check PASS |
| PRV2-017 | High | 06 | Hosted real-systemd crash/restart and isolated link-change pass, but no approved ordinary host has completed reboot, disk-full, DHCP/address-churn, competing-VPN and arm64 recovery | BLOCKED_EXTERNAL |
| PRV2-018 | High | 08/21 | Hosted Relay resource bounds, 5M-frame sustained forwarding, and controlled two-Relay restart pass, but public-WAN adversarial, multi-region/multi-instance, volumetric and long-duration capacity evidence is absent | BLOCKED_EXTERNAL |
| PRV2-019 | High | 09 | A higher signed configuration version could carry a lower ACL policy version and roll Agent authorization state back | CLOSED at `3f27ed9`; exact-head Gate 09 PASS at `e908e67` |

Findings remain open until the associated raw evidence is linked from `EVIDENCE_INDEX.md`.

`PRV2-005` repository-side preparation is complete at commit `94ccae3`: a strict TLS/SNI/HTTP/WebSocket auditor, negative tests, and a placeholder-only OpenResty template route the approved future origin to Console. The production finding is not closed. DNS/CDN/1Panel ownership and certificate material require explicit owner authorization; current raw evidence remains a functional failure.

The four closed Gate 23 findings were independently revalidated at exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b`. GitHub Actions run `31313868529` passed all four jobs, and the clean-checkout evidence root `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z` passed every recorded status. These closures do not close the remaining global Critical/High findings, do not deploy `3bf8619`, and do not change the overall `NO_GO` decision.

`PRV2-017` has complete automated evidence for its safely reproducible submatrix at exact revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02`, GitHub Actions run `31352258781`, job `93345151258`, and artifact `9049384061`. It remains blocked because destructive host failure modes must not be exercised on the production server and no disposable approved Linux/arm64 host is currently available.

`PRV2-018` has complete automated evidence for bounded single-process Relay admission/queue accounting, authenticated 5,000,000-frame forwarding with short-Lease renewal, and controlled two-Relay stop/restart recovery at exact revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e`, run `31358498444`, job `93362562136`, and artifact `9051561308`. It remains blocked because hosted loopback/namespace evidence cannot prove Internet-scale abuse resistance, cloud-edge saturation controls, multi-region/multi-instance behavior, or an hours/days production operating envelope.

`PRV2-019` is closed by independent monotonic enforcement of `configuration.version` and `policy_version`, with rejection-before-mutation regression. Exact revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b`, run `31360862865`, job `93369332314`, and artifact `9052383034` additionally prove the complete Direct/Relay/subnet ACL matrix. The closure changes Gate 09 only and does not alter external findings or overall `NO_GO`.
