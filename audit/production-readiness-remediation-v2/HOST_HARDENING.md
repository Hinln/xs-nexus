# Production Host Hardening

## Current Status

`PARTIAL`. Key-only SSH, the minimal host INPUT policy, public 1Panel management-port closure, independent-session verification, and rollback safety are complete. The host still has a material security-update backlog, root-filesystem pressure, and no independently delivered disk alert, so Gate 14 is not `PASS`.

## Change Preconditions

1. Capture `ss`, nftables, routes/rules, interfaces, Docker/container/network, `1panel-network`, failed units, disk, memory, SSH configuration, and current sessions.
2. Keep the current SSH connection open and establish an independent second connection.
3. Verify a non-root administrator key and passwordless/controlled sudo path.
4. Validate candidate SSH configuration with `sshd -t`.
5. Install a timed automatic rollback that restores the exact prior SSH/firewall files and reloads validated services.
6. Apply the minimum change.
7. Verify through a new third SSH connection, verify 1Panel and project health, then cancel rollback.

## Target State

- `PermitRootLogin no` and `PasswordAuthentication no`.
- Explicit minimal inbound policy for approved SSH, TLS, discovery/Relay UDP, and separately approved 1Panel administration sources.
- Every listener has protocol, process, purpose, exposure, rate limit, and owner.
- No project container is privileged, mounts Docker socket, uses host networking, or has unnecessary capabilities.
- Agent systemd uses only the sandboxing compatible with TUN/Netlink requirements.
- Security updates and reboot requirements are documented and applied in a maintenance window.
- Disk warning/critical thresholds alert externally; cleanup is limited to identified project-owned, reproducible data.

## Protected Invariants

- `1panel-network` ID, driver, subnet `172.18.0.0/16`, and unrelated members/resources.
- SSH reachability, default route, 1Panel operation, unrelated Docker services, and production database non-exposure.

No hardening change is recorded as applied until raw before/after and rollback evidence is indexed.

## Applied Controls

- Exact source commit `ae74783cdf9f75fd90e496fe837e50b744990310` passed GitHub Actions run `31300939362` before production use.
- `sshd` permits only `ubuntu` with public-key authentication. Root, password, keyboard-interactive, X11, agent forwarding, remote forwarding, stream-local forwarding, tunnels, and user environment files are disabled. Local forwarding is restricted to loopback TCP `188` for 1Panel administration.
- The independent `inet xs_nexus_host_guard` table has INPUT policy drop and never flushes or edits Docker, 1Panel, UFW, iptables compatibility, or unrelated nftables tables. It permits loopback, established/related traffic, required ICMP/DHCP, rate-limited TCP `122`, TCP `80`/`443`, and UDP `443` only.
- External probes after activation and again after rollback cancellation proved TCP `80`, `122`, and `443` reachable while TCP `22`, `188`, `3306`, `5432`, `6379`, `28080`, `28081`, `42000`, and `42001` were closed or filtered. UDP `42000`/`42001` remain Docker-published through the forward path.
- The current administrator key succeeds; password-only, root-key, and prior administrator-key attempts are rejected. The restricted SSH tunnel to loopback TCP `188` succeeds.
- Project containers remain non-root, read-only, `cap_drop: ALL`, non-privileged, without host networking or Docker socket mounts. Unrelated OpenResty remains running.
- Before/after comparison proves default routes, IP rules, protected container IDs, `1panel-network` ID/subnet, and all non-project nftables semantics unchanged. No failed systemd units remain.
- Two retained SSH sessions and a 20-minute systemd rollback protected the change. Rollback was canceled only after fresh-session, external, service-restart, route/rule/nftables, 1Panel, and production-health checks passed; rollback material was then removed.

## Evidence

- Source implementation: Git commit `ae74783cdf9f75fd90e496fe837e50b744990310`.
- Exact-head CI: GitHub Actions run `31300939362`.
- Production baseline and final evidence: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z`.
- Final root manifest: `SHA256SUMS.final`, 117 files verified.
- Application evidence: `application-20260809T075244Z`, 56 files verified by its own `SHA256SUMS`.
- Evidence secret scan: one complete surface, zero findings; matched values are never emitted.

The initial apply process returned nonzero because `systemctl` briefly reported SSH as `reloading`; the timer stayed armed and a new SSH session immediately proved the service active. A separate verifier wrapper also had a log-output defect after all checks passed. Both tooling issues are explicitly dispositioned in `verification-tooling-disposition.txt`; the corrected independent verification reran all checks.

## Remaining Work

- Baseline package evidence records 157 upgrades plus 9 newly installed dependency packages in the simulated dist-upgrade; 119 upgrade operations are sourced from security updates. Apply them in a protected maintenance window and perform the required reboot/service/network regression.
- Root filesystem usage is `83%` with about `9.8G` free. Identify project-owned removable data without global pruning and prove warning/critical thresholds through an external notification target.
- Re-run listener, Docker privilege, SSH, firewall, route/rule, `1panel-network`, production-health, and failed-unit checks after patching/reboot.

Until these items pass, Gate 14 remains `PARTIAL`.
