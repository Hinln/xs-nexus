# Remediation V2 Changelog

## 2026-08-09

- Froze GitHub main, production checkout, production runtime, remediation revision, and V2 branch start.
- Created `production-readiness-remediation-v2` without modifying `main`.
- Preserved the complete prior audit under `audit/production-readiness/`.
- Initialized the V2 hard-gate table, open findings, evidence index, external-gate runbooks, and release-readiness checklist.
- No production host, credential, database, DNS, firewall, 1Panel, Docker network, or running service change has been made in this entry.

### Gate 01 internal provenance implementation

- Added one validated build identity across Controller, Relay, Console, Agent, CLI, OCI labels, Linux packages, installers, SBOM, release manifest, and in-toto/SLSA provenance.
- Added deterministic signed release-bundle generation and strict offline verification, including dirty source, tag, signature, identity, digest, subject, path, symlink, extra-file, and tamper rejection.
- Added runtime/package mismatch rejection and a regression proving that a correctly signed Linux manifest with a forged source commit cannot activate.
- Retained failed GitHub Actions run `31289302264` and artifact `9030926588`; fixed the missing Relay lock metadata rather than bypassing `--locked`.
- GitHub Actions run `31289641228` passed all four jobs for exact revision `fea456b3d6feff36856b1f2066ace8a22b650bce`; artifacts: image `9031132937`, fuzz `9031005160`, Console `9030990379`.
- Gate 01 remains `FAIL`: no formal key ceremony, signed RC tag/bundle, main merge, production deployment, runtime reverse verification, or old-production upgrade/rollback occurred.
- No production host, credential, database, DNS, firewall, 1Panel, Docker network, or running service change was made by this Gate 01 implementation.

### Gate 16 PostgreSQL least privilege and production deployment

- Split database duties into non-login owner, runtime application, and deployment-only migrator roles. Runtime startup no longer creates schemas or runs migrations and fails closed on role or migration drift.
- Added strict deployment identity checks, role hardening, post-create/post-restore grant repair, least-privilege negative tests, and owner-role backup restore.
- Exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` passed GitHub Actions run `31294988591` and isolated evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z`.
- Production baseline, verified encrypted backup, isolated restore, clean image build, role migration, deployment, independent new-session verification, and post-finalization evidence are stored under `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`.
- Retained four failed verifier attempts and four successful timed automatic rollbacks. The final attempt deployed `3d93656`, verified active `xs_nexus_app` sessions and negative permissions, then canceled rollback only after a separate SSH session passed.
- Promoted the repository-external canonical environment atomically, archived the prior environment as root-only rollback material, and removed the temporary raw password staging directory. No credential value entered Git or evidence.
- Preserved `1panel-network` ID/subnet, unrelated OpenResty, default route, IP rules, and all non-project nftables rules. Docker-managed project rules were validated against actual container addresses and published ports rather than falsely required to remain byte-identical across container recreation.
- Gate 16 is now `PASS`. Gate 02 remains `FAIL` until the bootstrap and all other disclosed credentials are rotated and old values are independently rejected.

### Gate 14 SSH and minimal host firewall

- Added the independent `inet xs_nexus_host_guard` INPUT table, rate-limited SSH, approved web/QUIC traffic, and default drop without flushing or editing Docker, 1Panel, UFW, iptables compatibility, or unrelated nftables tables.
- Restricted SSH to the `ubuntu` administrator and public keys; disabled root/password/keyboard-interactive authentication, X11, agent/remote/stream-local forwarding and tunnels. Local forwarding is limited to loopback 1Panel TCP `188`.
- Exact source `ae74783cdf9f75fd90e496fe837e50b744990310` passed GitHub Actions run `31300939362` and isolated namespace/source validation before production application.
- Applied with frozen baseline, two retained SSH sessions and a 20-minute systemd rollback. Fresh sessions, firewall service restart, external port probes, authentication negatives, restricted tunnel, production health, OpenResty, default routes, IP rules, non-project nftables and `1panel-network` passed before rollback cancellation.
- External TCP `80`/`122`/`443` remain reachable; TCP `22`/`188`/database/loopback application/TCP discovery-relay probes are closed or filtered. Docker-published UDP `42000`/`42001` remain intact.
- Final evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z` contains 117 SHA-256 verified files and a zero-finding no-value secret scan.
- Gate 14 moves from `FAIL` to `PARTIAL`. Security updates/reboot, root-disk pressure, and independent disk-alert delivery remain open and are not hidden by the firewall result.

### Gate 14 protected package, reboot, and disk maintenance

- Repacked and SHA-256 verified all 157 pre-upgrade package versions, captured critical configuration and a verified database backup, downloaded the candidate packages, and armed a 60-minute automatic package rollback before changing the host.
- Installed all 157 upgrades and 9 required dependencies. Independent verification proved zero remaining upgrades, empty `dpkg --audit`, key-only SSH, minimal firewall, unchanged default route/IP rules/container identities/`1panel-network`, healthy production services, and strict external port/authentication policy before canceling package rollback.
- Dispositioned Docker nftables regeneration without byte-level weakening: container IPs were mapped to stable identities, the complete canonical item multiset remained exact, non-allowlisted chain order remained exact, and every inverted pair in allowlisted Docker chains was proven predicate-disjoint.
- Booted `6.8.0-137-generic` once with `6.8.0-124-generic` saved as fallback. A persistent watchdog required internal health plus Boot-ID-bound external approval within 15 minutes or automatically rebooted to the old kernel. New-kernel internal and external checks passed, then temporary GRUB/watchdog state was removed and the normal first GRUB entry was independently verified as the new kernel.
- Removed only one exact project build cache, 157 verified rollback packages, the temporary critical-config archive, APT downloads, and maintenance-only `dpkg-repack`; no global Docker prune or autoremove ran. Root usage fell from `83%` to `77%` with about `14.15 GB` available.
- Final evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z` contains 218 SHA-256 verified files and a zero-finding no-value secret scan. Failed verifier/cleanup attempts and the explicit `grub-editenv` correction are retained.
- Gate 14 remains `PARTIAL`, not `PASS`, solely because no owner-approved independent external destination or on-call path exists for real warning/critical disk-alert delivery.

### Gate 13 planned-domain strict TLS diagnosis and preparation

- Re-ran a read-only origin/CDN/OpenResty audit. Client-to-edge TLS validates, but `/`, `/health/ready`, `/install`, and `/v1/control` all return `525`; direct origin with planned-domain SNI fails with `unrecognized_name` before HTTP.
- Confirmed there is no planned-domain origin virtual host or certificate. The current `vpn.qinwen.co` certificate does not cover the planned name, and its public root proxies to Controller `127.0.0.1:28080` rather than Console `127.0.0.1:28081`.
- Added strict certificate-required TLS 1.2/1.3, SNI, HTTP-status, and WebSocket-handshake audit tooling with negative regression tests. Added a placeholder-only OpenResty template that redirects HTTP, terminates strict TLS, forwards all paths to Console, and preserves WebSocket upgrade.
- Exact implementation commit `94ccae3e8b4bf0279336d01db8b1ab53abf15aae` passed local regression, public-edge source validation, live public smoke, repository secret scan, and diff checks. The Windows-only environment could not execute the unrelated `validate-m02.py` subprocess until a real `python3` executable is available; no assertion was skipped or modified.
- Sealed read-only evidence under `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094531Z` and the exact-commit tool rerun under `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-strict-tls-tool-20260809T100334Z`.
- No DNS, CDN, certificate, 1Panel, OpenResty, Docker, firewall, network, route, or running-service change was made. Gate 13 remains `FAIL`; production correction is `BLOCKED_EXTERNAL` pending owner authorization.

### Gate 23 internal defect and dependency remediation

- Replaced detailed framework JSON parser rejections with an exact generic unauthenticated 400 envelope and added malformed, unknown-field, wrong-type, and oversized-body regressions. Commit `03c7557` and evidence `gate23-api-json-20260809T103535Z` pass without a production mutation.
- Added and deployed bounded PostgreSQL Docker logging (`max-size=10m`, `max-file=5`) through encrypted backup, timed rollback, independent verification, and immutable host/1Panel baselines. Commit `9764d58` and evidence `gate23-postgres-logging-20260809T105634Z` pass; the failed first rollout remains retained.
- Upgraded Rust to `1.94` and SQLx to `0.9.0`, removed `rsa` and `RUSTSEC-2023-0071` from the lock/advisory policy, required explicit dynamic-SQL safety markers, and completed the `cargo-deny` license allowlist. No vulnerability advisory ignore or license skip remains; the sole `cargo-deny` exception is the dated informational `paste` review recorded in `KI-026`.
- Exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b` passed GitHub Actions run `31313868529` and clean-checkout evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`.
- Preserved and sealed nine failed/non-authoritative validation roots: three API validation/harness roots plus six full-validation/headroom roots. The disk-health guard correctly rejected one run after every product check passed at 91% usage; two obsolete task-owned Rust 1.93 images and three exact XS Nexus BuildKit cache records were removed, with no global prune.
- Independent evidence verification passed at `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z`. Production remains healthy at `3d93656`; `1panel-network`, default route, failed-unit count, and temporary QA resource cleanup pass.
- Gate 23 remains `FAIL`, Gate 24 remains `PARTIAL`, and the project remains `NO_GO`: global Critical/High external findings, formal release, glibc disposition, and real-device/WAN/security-audit gates are still open.

### Gate 24 bounded dependency-topology review

- Reviewed exact commit `45dbc19690f6f738a43cabf2c344118d0b8bc055` from a clean bundle checkout. Plain `cargo audit` recorded zero vulnerabilities and `settings.ignore=[]`; full cargo-deny advisories, bans, licenses, and sources passed.
- Proved the exact reachable path `xs-agent -> rtnetlink 0.21.0 -> netlink-packet-core 0.8.2 -> paste 1.0.15` and retained the informational `RUSTSEC-2024-0436` output.
- Captured current upstream `main` manifests at exact commits `e7799b6ee24267586e6aadc0e3fb415b4d921dd4` and `571d8bb5fa1dbaa875e8aede3f214c87f70b955b`; upstream still exposes `rtnetlink 0.21.0` and `paste = "1"` respectively.
- Accepted ADR-089: do not maintain a private netlink fork solely to suppress an informational warning. Keep the dated cargo-deny exception, empty cargo-audit ignore, `2026-08-31` fail-closed deadline, and immediate drift review.
- Evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` is root-only, SHA-256 verified, and proves production/1Panel invariants. `KI-026` is dispositioned; Gate 24 remains `PARTIAL` because `KI-021`, formal release/deployment, and independent audit remain open.
- GitHub Actions run `31324714609` passed but retained Node 20 deprecation annotations for checkout/setup-node/upload-artifact. Commits `e63c59b` and `98ca145` move to verified, SHA-pinned official Node 24 releases, disable checkout credential persistence, and add fail-closed static/negative validation under `security-check`; exact-head run `31325753985` passed all four jobs with zero check-run annotations.

### Gate 06 hosted Linux Agent recovery submatrix

- Extended `scripts/test-agent-systemd.sh` to exercise real `Restart=on-failure`: force-kill the Agent, require exactly one different replacement PID, retain signed state, recreate TUN only in the private namespace, survive an isolated link down/up event, and clean all test state on stop.
- Kept the production sandbox intact. The executable is copied to a unique test-only root outside the protected runner home; the formal service unit is verified in a disposable `systemd-analyze --root` tree, so `/usr/local/lib/xs-nexus/current` is never created or changed by the test.
- Retained three failed/superseded runs exposing ShellCheck, protected-home execution, and formal-path verification defects. No assertion, warning, or sandbox property was suppressed.
- Exact revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02` passed all five jobs in run `31352258781`. Dedicated job `93345151258` and artifact `9049384061` passed archive/inner SHA-256 and no-value secret verification.
- Gate 06 remains `PARTIAL`: ordinary-host reboot, disk-full, DHCP/address churn, competing VPN routes, repeated failure/start-limit, and arm64 hardware require a disposable approved host. Production was not changed and remains on `3d93656`.

### Gate 08 hosted Relay security and resilience submatrix

- Added a verification-before-state global registration budget, exact global packet/byte queue caps, cross-limit configuration validation, transactional rejection, cleanup accounting, and low-cardinality queue gauges without weakening per-source, per-Lease, replay, endpoint, or ciphertext checks.
- Added a dedicated `relay-resilience` CI job covering all Relay targets, 5,000,000 release-profile authenticated frames with real short-Lease renewal, and two-Agent/two-Relay stop/failover/restart/re-registration/restored-primary/Direct recovery in namespaces.
- Retained the complete diagnostic chain for path-reason assertion, formatting, numeric conversion, evidence path, idle Lease, fixture identity, and strict Clippy failures. Packet count, minimum throughput, zero-drop, queue, authentication, restart, and cleanup assertions were not reduced or skipped.
- Exact revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` passed Relay job `93362562136` in run `31358498444`. Artifact `9051561308` has archive digest `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67`; both manifest layers verify directly after download and the no-value scan has zero findings.
- The measured hosted baseline is 5,000,000 × 216-byte frames in 70.213 seconds, 71,212.14 packet/s, 14.67 MiB/s, internal delay average 5 µs/max 150 µs, zero Relay drops and zero final queue. Gate 08 remains `PARTIAL` because public-WAN, distributed-abuse, cloud-DDoS, multi-region/multi-instance and long-duration evidence remains external; production was not changed.

### Gate 09 ACL enforcement

- Fixed a signed-policy rollback path by requiring both outer configuration version and ACL policy version to remain monotonic before Agent state mutation.
- Added a real three-Agent/TUN disconnected-Controller matrix for A→B allow, A→C/C→B deny, ICMP/TCP/UDP, unexpected ports, forged virtual source, allowed-node routing attempts, and independent receiver enforcement.
- Extended protocol identity negatives to forged source and destination Node IDs without consuming valid replay state.
- Added fail-closed Relay and subnet-router bypass regressions that require absence of matching encrypted transport frames and destination plaintext.
- First passing artifact `9052236354` was retained as valid but insufficiently explicit for independent evidence review. Assertion markers were added without changing behavior or thresholds, and the final exact-head run was repeated.
- Revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` passed all seven jobs in run `31360862865`; ACL job `93369332314`, artifact `9052383034`, archive digest, inner checksums, and no-value scan pass.
- Gate 09 changes to `PASS`. No production host, service, network, 1Panel resource, credential, DNS, firewall, or deployed image was modified; overall status remains `NO_GO`.

### Gate 15 1Panel coexistence

- Added a fail-closed source validator for the external `1panel-network`, all application/edge service attachments, project-scoped lifecycle commands, privilege boundaries, and global prune/network/volume-delete prohibitions.
- Added a dedicated hosted CI fixture that refuses to run when the exact network already exists, then verifies project-scoped Compose down and a real Docker-daemon restart preserve the exact external network, an unrelated sentinel, the host route, and the stable network inventory before exact labeled cleanup.
- Retained four failed runs and artifacts. Compose interpolation, inactive profiles, opaque cleanup status, and unstable built-in bridge IDs were fixed at their root causes without skip, allowed failure, assertion reduction, or production mutation.
- Exact revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6` passed all eight jobs in run `31504402285`; job `93822197946`, artifact `9106406005`, archive digest, inner checksums, and no-value scan pass.
- Mapped existing production Gate 14 and Gate 16 raw evidence to real host reboot/1Panel recovery, project restart/upgrade/four automatic rollbacks, and preservation of OpenResty, database boundary, SSH, routes/rules, non-project nftables, protected containers, and the production external network.
- Gate 15 changes from `PARTIAL` to `PASS`. The current branch was not deployed and the overall decision remains `NO_GO`.

### Gate 18 update and release supply-chain submatrix

- Unified release verification on schema 2 with canonical source/version/platform/archive identity, strict numeric and 512 MiB bounds, and one-to-four unique Ed25519 PEM public keys.
- Added one-way Controller revocation, policy pause/generation updates, redacted audit, directive suppression, exact Agent cancellation, and a root-owned host ledger checked before stage, apply, and rollback execution.
- Added interrupted, truncated, oversized, write-failure, real tmpfs ENOSPC, platform/architecture, identity, tamper, old-version, revoked-build, rollback, state-preservation, namespace-route-cleanup, and Console revocation regressions.
- Retained eight failed runs/artifacts and fixed their root causes without skip, suppression, allowed failure, timeout reduction, or assertion weakening.
- Candidate revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` passed all nine jobs in run `31515281011`; job `93858773221`, artifact `9110864186`, matching archive digest, eight inner hashes, and a zero-finding no-value scan pass.
- Gate 18 remains `PARTIAL` because formal offline keys/ceremony, authenticated production distribution, a signed RC, real target-platform execution, and the production release/rollback chain remain absent. Production was not changed and overall status remains `NO_GO`.

### Gate 19 real Console and visual matrix

- Added a dedicated real-runtime Playwright matrix using an isolated PostgreSQL schema, the real Controller binary, a production Console build/preview and Chromium. A source guard rejects API and HAR interception.
- Covered Controller stop/resume loading, bad and successful login, empty/initialized data, every management page, exact rapid-submit request count, tokens, two enrolled nodes, auditor creation, ACL default deny, 400/403/404 boundaries, concurrent stale-state 409, destructive cancel/single confirm, direct auditor bypass rejection, logout invalidation and real browser offline recovery.
- Added six-viewport visual and accessibility coverage for login, all 16 management pages, node detail and not-found, plus initial real-data and loading/offline screenshots. The final artifact contains 132 PNG files and asserts document overflow, bounded table scrolling, accessible names and skip-link focus.
- Fixed real defects found by the matrix: duplicate network creation, a Chromium-invalid username pattern, and cross-tab CSRF invalidation. Stable CSRF is domain-separated from the random HttpOnly session token; only hashes are stored.
- Disabled real traces to avoid persisting filled test values. Corrected exact API-response assertions, Chromium's successful-204 pseudo-failure classification and artifact hidden-file manifest alignment without allowing genuine network failures or weakening product assertions.
- Retained the complete workflow failure chain. Two failed trace-bearing artifacts were deleted for secret hygiene after diagnosis; immutable workflow logs remain and the successful artifact has a zero-finding no-value scan.
- Exact revision `5505893710ab1d15e06495603dff08bf5c1e035f` passed all nine jobs in run `31529393933`; job `93905489516`, artifact `9116327161`, matching archive digest and exact 139/139 inner manifest pass.
- Gate 19 remains `PARTIAL` because planned-domain strict origin TLS/SNI, CDN routing, the external public browser/API/WebSocket path and deployment of the current branch remain absent. Production was not changed and overall status remains `NO_GO`.

### Gate 20 production observability submatrix

- Added authenticated cumulative ACL/replay and detailed Relay telemetry with schema 1/2 rolling compatibility, monotonic persistence, bounded retention, and a low-cardinality authenticated Controller aggregate endpoint.
- Replaced the shallow shell guard with a standard-library Python collector for host, Docker, HTTP, Controller, strict TLS, encrypted backup, and private verification receipts; added atomic Prometheus/JSON output and hardened systemd units.
- Added firing, severity-change, deduplication, resolved, bounded retry, queue-overflow, and local notification-failure semantics. Production webhooks require HTTPS, disable redirects/environment proxies, and read a separate private token file.
- Retained migration/lint and two dedicated matrix failures. Fixed unique migration numbering, strict function size, ShellCheck/evidence atomicity, an unset Docker fixture state, and an incomplete Webhook fake without skipping or weakening tests.
- Exact revision `9291400ac030045e8ea2955ea137e7dc8be37a85` passed all ten jobs in run `31537716553`; job `93932721320`, artifact `9119468792`, archive digest `7b5fadda34a612716a6176767b465006a5e6e4175a61464e6959f4ec64fd5c79`, both inner manifests, eleven host scenarios, and two independent no-value scans pass.
- Gate 20 remains `PARTIAL/BLOCKED_EXTERNAL` because no production-independent destination/on-call has proved warning, critical, acknowledgement/escalation, and resolved closure, and the current branch is not deployed. Production was not changed and overall status remains `NO_GO`.

## 2026-08-12 Gate 21 Internal Capacity Matrix

- Added authenticated bounded `chunked-v1` control transport for large signed configurations, with strict canonical framing, ordering, length, digest, nesting, and legacy-client fail-closed checks.
- Added enforced 1,000-node and 1,000-control-session limits, bounded configuration-send concurrency, pre-allocation enrollment rejection, pre-upgrade WebSocket rejection, and low-cardinality capacity observability.
- Fixed a false-green cleanup trap and retained control-frame memory growth; Controller RSS fell from 490,496 KiB in the retained failing run to 66,668 KiB in the final run.
- Exact revision `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363` passed all 11 jobs in GitHub Actions run `31603852656`; artifact `9144433450` passed independent extracted manifests, revision binding, and zero-finding secret scan.
- Gate 21 remains `PARTIAL/BLOCKED_EXTERNAL`; Gate 22 remains `UNKNOWN`; overall remains `NO_GO`. Production was not changed.

## 2026-08-13 Gate 22 Soak Harness Calibration

- Added a fail-closed current-revision soak harness with separate calibration/formal modes, six-service resource sampling, eight required fault events, host-invariant baselines, cleanup checks, manifests, and evidence secret scanning.
- Fixed path-recovery backoff, manual-probe cooldown isolation, Relay lease recovery waiting, pre-enabled gateway forwarding, process-generation resource analysis, and version-aware bounded route-update retries. No product threshold, ACL, signature, identity, or security assertion was weakened.
- That exact revision `6e63424298e491c0035e1d138de5d03b0ab83c27` passed all 13 jobs in run `31649030446`; job `94289075250` and artifact `9162201059` passed 84/84 independent hashes, zero secret findings, 264 resource rows, eight events, and nine invariant classes.
- This is a 600-second calibration only. Gate 22 remains `UNKNOWN` until an exact-revision run reaches at least 86,400 seconds and independently passes all checks.
- The formal run did not start because the production SSH host key changed and has no out-of-band confirmation, while no alternate approved privileged Linux QA host is available. Strict verification was preserved; no password was sent and production was not changed.

## 2026-08-13 Gate 25 Clean Release Rehearsal Submatrix

- Added a root-only disposable-host harness that builds a full-history bundle, verifies an exact detached clean checkout and test-only signed tag, generates secrets outside Git, reproduces five no-cache images, and runs real Direct ACL, Relay, subnet, update and double deployment lifecycles.
- Added exact fixture and final invariants for Docker resources, routes, rules, links, namespaces, normalized nftables, failed services and the temporary external `1panel-network`; cleanup is restricted to exact per-run labels and safe temporary paths.
- Retained six failed runs covering shallow history, Buildx context, PostgreSQL volume/network/startup behavior, Docker firewall initialization, performance path-transition sampling, and hosted-runner hardware hot-plug. No phase, threshold or security assertion was skipped or reduced.
- Current RTT evidence uses 10 Direct warm-up packets plus 100 formal samples per path with unchanged average/p95 bounds and retained maximum outliers. PostgreSQL requires the final-init marker, readiness, and a real SQL query.
- Exact revision `6e63424298e491c0035e1d138de5d03b0ab83c27` passed all 13 jobs in run `31649030446`. Job `94289075249` and artifact `9162246723` passed 111/111 hashes, zero secret findings, 10/10 phases, and 20/20 fixture/final invariants.
- Gate 25 remains `FAIL`: the evidence explicitly records test-only signing, non-independent operation and no production mutation. Formal owner-signed RC/main, independent fresh-host rehearsal and current production upgrade/rollback remain external; overall remains `NO_GO`.

## 2026-08-13 Gate 02 External Rotation Boundary

- Rechecked every no-value credential-register row after all self-solvable repository work and exact-head CI completed.
- Reclassified `PRV2-001` and `KI-023` from generic `OPEN` to `BLOCKED_EXTERNAL`; no credential is marked rotated or rejected without real evidence, and Gate 02 remains `FAIL`.
- Added `BLK-011` for owner-controlled production/NAS/CI/device inventory, secret channels, maintenance windows, replacement activation and independent old-value rejection.
- No historical credential was reused, no SSH host-key check was bypassed, and no production, NAS, organization, DNS/CDN or CI secret was changed.

## 2026-08-13 Exact-Head Relay Convergence Regression

- Preserved run `31652099523`, where 12/13 jobs passed but Relay recovery failed on the first ICMP sample after the primary Relay stopped; the failure was not rerun away or replaced with an older green revision.
- Fixed the harness to require both Agents to converge on the exact established primary Relay, backup Relay, restarted Relay, and restored Direct path before each bidirectional assertion. Product timers, per-Agent bounds, packet/ACL/encryption checks, and cleanup requirements remain unchanged.
- Exact revision `c0c059e84806f70586f37ca3f3bb33cdd602c4a4` passed all 13 jobs in run `31653564044`. Relay artifact `9163588782` and Gate 25 artifact `9163844194` passed independent manifests and zero-finding secret scans.
- Production was not connected or changed. Gate 02 remains `FAIL`, Gate 22 remains `UNKNOWN`, Gate 25 remains `FAIL`, and the overall decision remains `NO_GO`.

## 2026-08-13 Exact-Head Recovery And Performance Closure

- Preserved three consecutive 12/13 runs instead of replacing them with an older green result: `31654905139` exposed a legitimate authenticated receiver `relay_failover` classification, `31655990346` exposed post-configuration Direct-session recovery, and `31657413129` exposed Direct RTT sampling before the path reached steady state.
- Kept exact endpoint/session, bidirectional ping, product timers, per-side wait bounds, 100 formal samples, 5/10 ms Direct bounds, 10/20 ms Relay bounds, 15 ms p95 increment, ACL, encryption, cleanup and secret-scan assertions unchanged.
- Added bounded evidence-bearing steady-state measurement: at most 12 ten-packet attempts, exact established Direct paths before and after every attempt, and warm-up p95 at most 10 ms before formal sampling. Exhaustion fails and retains every attempt.
- Exact revision `795b1ea461a179958aed27e6935faba8f36e43ce` passed all 13 jobs in run `31658778589`. Performance, Gate 22, Relay and Gate 25 artifacts `9165514010`, `9165687987`, `9165498146` and `9165661918` passed independent manifests, revision/status binding and zero-finding secret scans.
- Production was not connected or changed. Gate 22 remains `UNKNOWN`, Gate 25 remains `FAIL`, and overall remains `NO_GO` because formal duration, owner-controlled signed RC, independent operation and production upgrade/rollback are still external.
