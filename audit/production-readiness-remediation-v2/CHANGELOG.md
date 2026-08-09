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
- Upgraded Rust to `1.94` and SQLx to `0.9.0`, removed `rsa` and `RUSTSEC-2023-0071` from the lock/advisory policy, required explicit dynamic-SQL safety markers, and completed the `cargo-deny` license allowlist. No advisory ignore or license skip remains.
- Exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b` passed GitHub Actions run `31313868529` and clean-checkout evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`.
- Preserved and sealed nine failed/non-authoritative validation roots: three API validation/harness roots plus six full-validation/headroom roots. The disk-health guard correctly rejected one run after every product check passed at 91% usage; two obsolete task-owned Rust 1.93 images and three exact XS Nexus BuildKit cache records were removed, with no global prune.
- Independent evidence verification passed at `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z`. Production remains healthy at `3d93656`; `1panel-network`, default route, failed-unit count, and temporary QA resource cleanup pass.
- Gate 23 remains `FAIL`, Gate 24 remains `PARTIAL`, and the project remains `NO_GO`: global Critical/High external findings, formal release, glibc disposition, and real-device/WAN/security-audit gates are still open.
