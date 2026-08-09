#!/usr/bin/env python3
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
FIREWALL = (ROOT / "deploy/host/xs-nexus-host-firewall.nft").read_text(
    encoding="utf-8"
)
SCRIPT = (ROOT / "deploy/host/xs-nexus-host-firewall.sh").read_text(
    encoding="utf-8"
)
SERVICE = (ROOT / "deploy/host/xs-nexus-host-firewall.service").read_text(
    encoding="utf-8"
)
SSH = (ROOT / "deploy/host/sshd-xs-nexus-hardening.conf").read_text(
    encoding="utf-8"
)


assert "table inet xs_nexus_host_guard" in FIREWALL
assert "type filter hook input priority -10; policy drop;" in FIREWALL
assert "flush ruleset" not in FIREWALL.lower()
assert "188" not in FIREWALL
assert 'tcp dport 122 ct state new limit rate 20/minute burst 40 packets' in FIREWALL
assert 'tcp dport { 80, 443 }' in FIREWALL
assert 'udp dport 443' in FIREWALL
comments = set(re.findall(r'comment "([^"]+)"', FIREWALL))
assert comments == {
    "xs-allow-loopback",
    "xs-drop-invalid",
    "xs-allow-established",
    "xs-allow-icmp4",
    "xs-allow-icmp6",
    "xs-allow-dhcp4",
    "xs-allow-dhcp6",
    "xs-allow-ssh",
    "xs-allow-web-tcp",
    "xs-allow-quic",
    "xs-drop-unapproved",
}

assert "flush ruleset" not in SCRIPT.lower()
assert 'TABLE_NAME=xs_nexus_host_guard' in SCRIPT
assert 'nft --check --file "$batch"' in SCRIPT
assert 'delete table %s %s' in SCRIPT
assert "host_firewall_verification=pass" in SCRIPT

required_service = {
    "DefaultDependencies=no",
    "Before=network-pre.target shutdown.target",
    "CapabilityBoundingSet=CAP_NET_ADMIN",
    "NoNewPrivileges=yes",
    "ProtectSystem=strict",
    "ProtectHome=yes",
    "PrivateDevices=yes",
    "RestrictAddressFamilies=AF_UNIX AF_NETLINK",
}
assert required_service <= set(SERVICE.splitlines())
assert "ExecStop=" not in SERVICE

required_ssh = {
    "PermitRootLogin no",
    "PasswordAuthentication no",
    "KbdInteractiveAuthentication no",
    "AuthenticationMethods publickey",
    "AllowUsers ubuntu",
    "X11Forwarding no",
    "AllowAgentForwarding no",
    "AllowTcpForwarding local",
    "AllowStreamLocalForwarding no",
    "GatewayPorts no",
    "PermitTunnel no",
    "PermitOpen 127.0.0.1:188 [::1]:188",
}
assert required_ssh <= set(SSH.splitlines())

print("host hardening source tests passed")
