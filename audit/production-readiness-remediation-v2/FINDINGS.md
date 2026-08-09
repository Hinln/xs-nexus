# Open Remediation Findings

| ID | Severity | Gate | Finding | Status |
|---|---|---:|---|---|
| PRV2-001 | Critical | 02 | Previously disclosed infrastructure, application, enrollment, and recovery credentials lack complete rotation and old-value rejection evidence | OPEN |
| PRV2-002 | Critical | 03/18 | No formal offline release/recovery key ceremony, authenticated public-key distribution, revocation, or restore exercise | BLOCKED_EXTERNAL |
| PRV2-003 | High | 14 | SSH/firewall, protected security updates, new-kernel reboot regression, and bounded disk cleanup are closed; independently delivered and acknowledged warning/critical disk alerts remain open | BLOCKED_EXTERNAL |
| PRV2-004 | High | 16 | Long-running Controller database access depended on a bootstrap superuser; production now uses separated owner/app/migrator roles | CLOSED |
| PRV2-005 | High | 13/19 | Planned production domain does not provide a complete strict-TLS Console/API/WebSocket path | OPEN |
| PRV2-006 | High | 05 | Proprietary protocol, cryptography, API, Relay, Agents, Windows, and supply chain have no independent audit and retest | BLOCKED_EXTERNAL |
| PRV2-007 | High | 11 | Windows online enrollment, SCM, routing, sleep, upgrade, rollback, and cleanup matrix is incomplete | BLOCKED_EXTERNAL |
| PRV2-008 | High | 12 | Real NAS ordinary-node and subnet-router evidence is absent | BLOCKED_EXTERNAL |
| PRV2-009 | High | 17 | Backup replica remains in the same failure domain and no clean-server restore exists | BLOCKED_EXTERNAL |
| PRV2-010 | High | 20 | Local health guard lacks independent external delivery, formal on-call, TLS-expiry and full metrics closure | OPEN |
| PRV2-011 | High | 22 | Current remediation revision has no mandatory 24-hour soak evidence | OPEN |
| PRV2-012 | High | 01/25 | Revision `3d93656` is built, deployed, reverse-verified and repeatedly rolled back to `ff9551d3`, but is not merged to `main` or covered by an owner-controlled signed RC tag/bundle | OPEN |

Findings remain open until the associated raw evidence is linked from `EVIDENCE_INDEX.md`.
