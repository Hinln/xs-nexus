# Production Readiness Remediation V2 Plan

## Objective

Move XS Nexus from the independently audited `NO_GO / CRITICAL` state to a state that can undergo a new, full production-readiness audit without weakening, merging, or deleting any hard gate.

## Frozen Baseline

- Required remediation base: `8532eb6992389568643f8a501055c6acc72395ab`
- V2 branch start: `f6dc6021f7b7ea82b2022840465b07b69f7ad693`, a direct descendant that preserves the final prior-audit documentation
- GitHub main at freeze: `8745b5804312587534c1e91980dfb11720952ed1`
- Production checkout at freeze: `8745b5804312587534c1e91980dfb11720952ed1`
- Production running revision at freeze: `ff9551d322067c934d2ac7d55a62af8896660bb3`
- Working branch: `production-readiness-remediation-v2`

## Ordered Work

1. Establish release provenance and runtime version reporting.
2. Inventory every secret without printing values; rotate and prove old-value rejection where an authorized channel exists.
3. Harden SSH and host exposure with baseline, independent sessions, timed rollback, and post-change verification.
4. Move PostgreSQL runtime and migration duties to separate least-privilege roles.
5. Repair strict origin TLS for the planned production domain without weakening verification.
6. Re-run and close all internal P0, P1, Critical, and High findings.
7. Complete release/update supply-chain evidence and anti-rollback tests.
8. Add production metrics, alerts, certificate/backup monitoring, and an external notification destination.
9. Complete capacity tests and a current-revision 24-hour soak.
10. Execute real Windows, NAS, WAN NAT/Relay, subnet-router, offsite restore, key ceremony, and third-party audit gates when their external environments are available.
11. Merge only after review, create a signed RC tag, build from a clean checkout, rehearse deployment, upgrade from `ff9551d3`, and prove rollback.
12. Re-run every hard gate in `audit/production-readiness-final/`.

## Current Progress

- Internal provenance implementation and exact-head CI are complete; formal signed RC/main chain remains open.
- PostgreSQL least privilege and production deployment are complete: Gate 16 is `PASS` at running revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`.
- Credential rotation remains in progress; new app/migrator identities are active, but bootstrap and other disclosed credentials still require rotation and old-value rejection.
- Gate 14 SSH/firewall, protected security updates, one-shot new-kernel reboot, full service/network regression, and bounded disk cleanup are complete. The gate remains `PARTIAL` only because external warning/critical disk-alert delivery and acknowledgement are unavailable; proceed to Gate 13 strict origin TLS and remaining internal findings without misreporting that external item.
- Gate 13 read-only diagnosis and all repository-side preparation are complete at `94ccae3`. Edge routes return `525`, direct planned-domain origin SNI fails, the origin lacks the planned certificate/vhost, and current root traffic targets Controller instead of Console. Production repair remains owner-controlled `BLOCKED_EXTERNAL`; continue internal Gates 23, 04, 06, 08, 09, 15, 18, 19, 20, 21, 22, 24, and 25 without changing DNS/CDN/1Panel.
- Gate 23 self-fixable remediation is complete at `3bf8619`: API parser disclosures, PostgreSQL log bounds, the RSA advisory path, and missing license policy are closed. Exact-head CI and clean-checkout validation pass, while retained failed runs are dispositioned and sealed. Gate 23 remains `FAIL` because global Critical/High and external findings remain open; continue the next unresolved internal gates without deploying this branch as a release.
- Gate 04 internal protocol security is complete at `875395352afc8a17dafc17b2496d62ce701ee4ae`: lost-response retransmission semantics, exact Finish caching, retry-exhaustion rehandshake, state-machine regressions, protocol documentation/vector/corpus discipline, strict CI, and six-target AddressSanitizer fuzz evidence pass. Gate 05 remains external and the branch is not approved for deployment; continue Gate 06 and the remaining independently solvable internal work.
- Gate 06 hosted real-systemd crash/restart and isolated link-change evidence is complete at `fb45fd43256d65cb4c72824d6cee0bec0884ad02` through run `31352258781`; failed harness runs remain retained. Gate 06 stays `PARTIAL` until an approved disposable ordinary Linux host completes reboot, disk-full, DHCP/address-churn, competing-VPN and arm64-relevant recovery. Continue Gate 08 and other self-solvable internal work without deploying the branch.
- Gate 08 hosted Relay admission/queue bounds, 5,000,000-frame sustained forwarding with real short-Lease renewal, and controlled two-Relay stop/restart recovery are complete at `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` through run `31358498444`; retained failures document each harness defect. Gate 08 stays `PARTIAL` until approved public-WAN, distributed-abuse, multi-region/multi-instance, and long-duration capacity evidence exists. Continue Gate 09 and other self-solvable internal work without deploying the branch.
- Gate 09 is `PASS` at `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` through run `31360862865`: a signed policy-version rollback was fixed, and real three-Agent/TUN Direct, disconnected-Controller, identity, sender/receiver, Relay, and subnet bypass matrices pass. Production was not changed; continue Gate 15 and the remaining self-solvable internal gates.
- Gate 15 is `PASS` at `8a9174866ebdf4ff76e7d987e006acb64312e3b6` through run `31504402285`: current Compose/lifecycle validation and a CI-only real Docker-daemon restart preserve the external network and unrelated sentinel, while prior production upgrade/rollback and protected host-reboot evidence closes the real 1Panel/OpenResty/database/SSH coexistence matrix. Production was not changed by this step; continue Gate 18 and the remaining self-solvable internal gates.

## Safety Rules

- Never delete, recreate, rename, or modify `1panel-network` or unrelated 1Panel resources.
- Never run a global Docker prune.
- Never expose PostgreSQL, Redis, or MySQL publicly.
- Before SSH, firewall, route, nftables, Docker-network, or system-networking changes: capture a baseline, keep the current session, open an independent session, schedule automatic rollback, apply the minimum change, validate through a new session, then cancel rollback.
- Never record a real secret, private key, token, cookie, password, or complete credential URI in Git or audit evidence.
- Namespace, cross-build, and simulated evidence remain explicitly distinct from real-device and real-WAN evidence.

## Exit Condition

This work may pause only when every remaining item is an accurately documented `BLOCKED_EXTERNAL` gate. Formal production `GO` requires all hard gates to be `PASS`.
