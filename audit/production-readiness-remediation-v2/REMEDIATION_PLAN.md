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
- Credential rotation repository preparation is complete and `BLK-011` now accurately classifies execution as `BLOCKED_EXTERNAL`; new app/migrator identities are active, but owner-controlled bootstrap and every other disclosed credential still require real rotation and old-value rejection. Gate 02 remains `FAIL`.
- Gate 14 SSH/firewall, protected security updates, one-shot new-kernel reboot, full service/network regression, and bounded disk cleanup are complete. The gate remains `PARTIAL` only because external warning/critical disk-alert delivery and acknowledgement are unavailable; proceed to Gate 13 strict origin TLS and remaining internal findings without misreporting that external item.
- Gate 13 read-only diagnosis and all repository-side preparation are complete at `94ccae3`. Edge routes return `525`, direct planned-domain origin SNI fails, the origin lacks the planned certificate/vhost, and current root traffic targets Controller instead of Console. Production repair remains owner-controlled `BLOCKED_EXTERNAL`; continue internal Gates 23, 04, 06, 08, 09, 15, 18, 19, 20, 21, 22, 24, and 25 without changing DNS/CDN/1Panel.
- Gate 23 self-fixable remediation is complete at `3bf8619`: API parser disclosures, PostgreSQL log bounds, the RSA advisory path, and missing license policy are closed. Exact-head CI and clean-checkout validation pass, while retained failed runs are dispositioned and sealed. Gate 23 remains `FAIL` because global Critical/High and external findings remain open; continue the next unresolved internal gates without deploying this branch as a release.
- Gate 04 internal protocol security is complete at `875395352afc8a17dafc17b2496d62ce701ee4ae`: lost-response retransmission semantics, exact Finish caching, retry-exhaustion rehandshake, state-machine regressions, protocol documentation/vector/corpus discipline, strict CI, and six-target AddressSanitizer fuzz evidence pass. Gate 05 remains external and the branch is not approved for deployment; continue Gate 06 and the remaining independently solvable internal work.
- Gate 06 hosted real-systemd crash/restart and isolated link-change evidence is complete at `fb45fd43256d65cb4c72824d6cee0bec0884ad02` through run `31352258781`; failed harness runs remain retained. Gate 06 stays `PARTIAL` until an approved disposable ordinary Linux host completes reboot, disk-full, DHCP/address-churn, competing-VPN and arm64-relevant recovery. Continue Gate 08 and other self-solvable internal work without deploying the branch.
- Gate 08 hosted Relay admission/queue bounds, 5,000,000-frame sustained forwarding with real short-Lease renewal, and controlled two-Relay stop/restart recovery are complete at `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` through run `31358498444`; retained failures document each harness defect. Gate 08 stays `PARTIAL` until approved public-WAN, distributed-abuse, multi-region/multi-instance, and long-duration capacity evidence exists. Continue Gate 09 and other self-solvable internal work without deploying the branch.
- Gate 09 is `PASS` at `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` through run `31360862865`: a signed policy-version rollback was fixed, and real three-Agent/TUN Direct, disconnected-Controller, identity, sender/receiver, Relay, and subnet bypass matrices pass. Production was not changed; continue Gate 15 and the remaining self-solvable internal gates.
- Gate 15 is `PASS` at `8a9174866ebdf4ff76e7d987e006acb64312e3b6` through run `31504402285`: current Compose/lifecycle validation and a CI-only real Docker-daemon restart preserve the external network and unrelated sentinel, while prior production upgrade/rollback and protected host-reboot evidence closes the real 1Panel/OpenResty/database/SSH coexistence matrix. Production was not changed by this step; continue Gate 18 and the remaining self-solvable internal gates.
- Gate 18's repository-side matrix is complete at candidate revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` through all-green run `31515281011`: canonical release identity, bounded key overlap, dual online/offline revocation, hostile download/storage/platform inputs, rollback-before-execution, state preservation, real tmpfs ENOSPC, namespace route cleanup, and Console controls pass with independently verified artifact `9110864186`. Gate 18 stays `PARTIAL` until owner-controlled formal key ceremony/distribution, signed RC, real target-platform execution, and production release/rollback evidence exist; continue Gate 19 and the remaining self-solvable internal gates without deploying this branch.
- Gate 19's repository-side runtime and visual matrix is complete at exact revision `5505893710ab1d15e06495603dff08bf5c1e035f` through all-green run `31529393933`: real PostgreSQL/Controller/production Console build/Chromium, no API interception, complete security/concurrency/failure state coverage, six viewports, 132 screenshots and independently verified artifact `9116327161` pass. Gate 19 stays `PARTIAL` until the owner-controlled planned-domain strict TLS/CDN/public browser path and current release deployment exist; continue Gate 20 and remaining self-solvable internal gates without changing production.
- Gate 20's repository-side observability matrix is complete at exact revision `9291400ac030045e8ea2955ea137e7dc8be37a85` through all-green run `31537716553`: authenticated low-cardinality Agent/Relay/Controller telemetry, real PostgreSQL aggregation, host/Docker/TLS/backup collection, hardened systemd, bounded alert lifecycle/retry, proxy isolation, and independently verified artifact `9119468792` pass. Gate 20 stays `PARTIAL/BLOCKED_EXTERNAL` until a production-independent destination and on-call prove real warning, critical, acknowledgement/escalation, and resolved closure on the deployed approved release; continue Gate 21 and remaining self-solvable work without changing production.
- Gate 22's repository-side harness and latest 600-second calibration are complete at exact revision `795b1ea461a179958aed27e6935faba8f36e43ce` through all-green run `31658778589` and independently verified artifact `9165687987`. Gate 22 stays `UNKNOWN`: the mandatory >=86,400-second run has not started, because the production SSH host-key change has no out-of-band confirmation and no alternate approved privileged Linux QA host is available. Strict host verification was preserved and production was not changed.
- Gate 25's repository-side clean release/deployment submatrix is complete at the same exact revision through job `94318968293` and independently verified artifact `9165661918`: clean full-history checkout, test-only signed tag, double no-cache image reproducibility, external generated secrets, real Direct/Relay/subnet paths, update failures, two Docker lifecycles and exact cleanup all pass. Gate 25 remains `FAIL` because the test tag is not an owner-controlled signed RC, the operator/host are not independent, and the current production upgrade/rollback has not occurred.

## Safety Rules

- Never delete, recreate, rename, or modify `1panel-network` or unrelated 1Panel resources.
- Never run a global Docker prune.
- Never expose PostgreSQL, Redis, or MySQL publicly.
- Before SSH, firewall, route, nftables, Docker-network, or system-networking changes: capture a baseline, keep the current session, open an independent session, schedule automatic rollback, apply the minimum change, validate through a new session, then cancel rollback.
- Never record a real secret, private key, token, cookie, password, or complete credential URI in Git or audit evidence.
- Namespace, cross-build, and simulated evidence remain explicitly distinct from real-device and real-WAN evidence.

## Exit Condition

This work may pause only when every remaining item is an accurately documented `BLOCKED_EXTERNAL` gate. Formal production `GO` requires all hard gates to be `PASS`.

## Gate 21 Current Progress (2026-08-12)

- Exact revision `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363` passed all 11 jobs in run `31603852656`.
- The internal single-instance 1,000-node and 1,000-control-session matrix is complete with fail-closed overflow admission, bounded large-message transport, current protocol/Agent measurements, and independently verified evidence artifact `9144433450`.
- `PRV2-018` remains `BLOCKED_EXTERNAL` for public-WAN, multi-region/multi-instance, volumetric/distributed-abuse, and hours/days capacity proof.
- Proceed to Gate 22 by implementing a current-revision 24-hour soak harness that samples all required resources and executes Controller, Relay, PostgreSQL, Agent, enrollment/revocation, and controlled network-fault recovery without changing production resources.

## Gate 22 Current Progress (2026-08-13)

- The harness, summarizer regressions, CI job, six-service sampling, eight-event fault matrix, host invariants, cleanup, manifests, and secret scan are complete.
- Latest exact revision `795b1ea461a179958aed27e6935faba8f36e43ce` passed all 13 jobs in run `31658778589`; job `94318968217` and artifact `9165687987` independently passed 84/84 hashes and zero secret findings.
- The successful run lasted 600 seconds in explicit calibration mode. It cannot satisfy Gate 22; status remains `UNKNOWN` and overall remains `NO_GO`.
- Formal execution requires an approved identity-verified privileged Linux QA host. Do not disable SSH host-key validation or run destructive/network fault injection on production merely to obtain evidence.

## Gate 25 Current Progress (2026-08-13)

- `scripts/test-clean-release-rehearsal.sh` now executes the complete safely automatable repository submatrix from an independent detached checkout with evidence outside the checkout and exact cleanup.
- Exact revision `795b1ea461a179958aed27e6935faba8f36e43ce` passed all 13 jobs in run `31658778589`; job `94318968293` and artifact `9165661918` passed 110 outer plus 8 nested hashes and zero secret findings.
- Nine retained failed runs expose shallow history, Buildx context, PostgreSQL lifecycle, Docker firewall initialization, database startup race/performance transition, hosted-runner hardware hot-plug, receiver Relay classification, post-configuration session recovery, and steady-state RTT sampling. No phase, threshold, or cleanup assertion was skipped.
- The repository submatrix is complete, but Gate 25 remains `FAIL` and overall remains `NO_GO` until formal signed-RC, independent fresh-host, and current production upgrade/rollback evidence exists.
