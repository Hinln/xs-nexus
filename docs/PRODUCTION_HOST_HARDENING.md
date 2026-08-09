# Production Host Hardening

## Inbound Policy

`xs_nexus_host_guard` is an independent `inet` nftables table. It never flushes or edits Docker, 1Panel, UFW, iptables compatibility, or unrelated nftables tables.

The host input chain allows only:

- loopback traffic;
- established and related traffic;
- IPv4/IPv6 ICMP required for diagnostics and neighbor discovery;
- DHCP client renewal;
- rate-limited new SSH connections on TCP `122`;
- HTTP/HTTPS on TCP `80`/`443` and QUIC on UDP `443`.

All other traffic addressed to the host is dropped and counted. XS Nexus Discovery and Relay UDP ports are Docker-published and DNAT-routed through the existing Docker forward path, not accepted by this host-input table. The project must continue to prove their Docker mappings and service health separately.

The 1Panel core listener on TCP `188` is intentionally not public. Administrators use the key-only SSH tunnel:

```bash
ssh -p 122 -L 1188:127.0.0.1:188 ubuntu@SERVER
```

Then open the existing 1Panel security path through `http://127.0.0.1:1188`. Do not record that path, session cookie, username, or password in Git or audit evidence.

## SSH Policy

- Only `ubuntu` may log in.
- Root, password, keyboard-interactive, empty-password, X11, agent forwarding, remote forwarding, stream-local forwarding, tunnels, and user environment files are disabled.
- Public-key authentication is mandatory.
- Local TCP forwarding is restricted to loopback 1Panel TCP `188` only.
- Login grace, authentication attempts, sessions, startup concurrency, and idle-client detection are bounded.

## Installation

Install the reviewed files as root:

```bash
install -o root -g root -m 0755 deploy/host/xs-nexus-host-firewall.sh /usr/local/sbin/xs-nexus-host-firewall
install -d -o root -g root -m 0755 /etc/xs-nexus
install -o root -g root -m 0644 deploy/host/xs-nexus-host-firewall.nft /etc/xs-nexus/host-firewall.nft
install -o root -g root -m 0644 deploy/host/xs-nexus-host-firewall.service /etc/systemd/system/xs-nexus-host-firewall.service
install -o root -g root -m 0644 deploy/host/sshd-xs-nexus-hardening.conf /etc/ssh/sshd_config.d/00-xs-nexus-hardening.conf
sshd -t
systemctl daemon-reload
systemctl enable xs-nexus-host-firewall.service
```

Do not apply these commands without a frozen baseline, two independent SSH sessions, a timed automatic rollback, and an out-of-band reachability check. Apply the nftables table before reloading SSH, verify from a third new SSH session, and cancel rollback only after production containers, OpenResty, `1panel-network`, routes, IP rules, and non-project nftables rules pass.

## Verification

```bash
/usr/local/sbin/xs-nexus-host-firewall verify
sshd -t
sshd -T | grep -E '^(permitrootlogin|passwordauthentication|authenticationmethods|allowusers|allowtcpforwarding|permitopen) '
```

From outside the server, TCP `80`, `122`, and `443` remain reachable; TCP `188` must be closed or filtered. Database and loopback application ports remain non-public.

## Rollback

Rollback restores the exact prior SSH drop-in and authorized-key file from a root-only archive, validates SSH before reload, removes only table `inet xs_nexus_host_guard`, and disables only `xs-nexus-host-firewall.service`. It must not call `flush ruleset`, restart Docker, modify `1panel-network`, or edit 1Panel resources.
