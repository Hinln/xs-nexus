# Real Public-Network NAT And Relay Gate

## Status

`SIMULATED_ONLY`. Namespace coverage remains valid but does not satisfy the real-WAN gate.

## Required Topologies

- Home broadband to cloud.
- Mobile hotspot to cloud.
- Two different public networks.
- Direct-capable path.
- UDP-blocked path with authenticated Relay fallback.
- Direct recovery after policy/path recovery.
- Endpoint/public-address change and reconvergence.
- IPv6 where available.

## Evidence

Record node/platform revisions, public-network class without exposing unnecessary addresses, candidates, authenticated path reason, build/handshake time, RTT/loss, Relay identity, packet-capture summary showing encrypted payload, fault timeline, and recovery time.

## Safety

Use approved test nodes and bounded traffic. Do not modify production firewall policy merely to force a pass. No result is `PASS` until the real topology and packet evidence are indexed.
