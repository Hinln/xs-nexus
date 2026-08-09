# Production Host Hardening

## Current Status

`FAIL`. The prior audit found password/root SSH access, a permissive host INPUT policy, public management exposure, pending updates, and root-filesystem pressure. The five-minute local health guard is useful but insufficient for production hardening.

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
