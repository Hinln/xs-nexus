# Planned Domain And Strict TLS

## Current Status

`FAIL`. At `2026-08-09T10:03:34Z`, the planned domain returned CDN HTTP `525` for `/`, `/health/ready`, `/install`, and `/v1/control`; direct-origin connections using the planned hostname as SNI failed before HTTP. No production configuration was changed during this audit.

## Frozen Read-Only Findings

- Planned domain: `vpn.xiashikeji.cn`; observed DNS was CNAME `vpn.xiashikeji.cn.eo.dnse1.com` with edge address `117.139.140.63`.
- Client-to-edge TLS verified successfully with TLS 1.3 and a hostname-valid planned-domain certificate, but every required functional path returned Tencent EdgeOne `525`.
- Direct connection to origin `101.32.170.223:443` with SNI and `Host` set to the planned domain failed with TLS alert `unrecognized_name`; no HTTP response was available.
- The origin had no planned-domain 1Panel/OpenResty virtual host and the installed certificate covered only the current `qinwen.co` names.
- The existing `vpn.qinwen.co` virtual host routed `/` to Controller loopback `127.0.0.1:28080`, whose root returns `404`; Console loopback `127.0.0.1:28081` returned `200`, `/console-health` and `/health/ready` returned `200`, and `/v1/control` completed a `101` WebSocket upgrade.
- Exact read-only evidence: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094531Z`.
- Exact tool rerun for commit `94ccae3e8b4bf0279336d01db8b1ab53abf15aae`: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-strict-tls-tool-20260809T100334Z`.

The retained collector attempt at `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094447Z` failed because the evidence wrapper incorrectly assumed `curl` would create a response body after the expected TLS handshake failure. It made no production change and is not passing evidence.

## Diagnostic Chain

`client -> CDN -> origin TLS/SNI -> reverse proxy -> Console/Controller`

Collect and correlate:

- Authoritative DNS and CDN origin configuration.
- Origin IP/port and firewall reachability.
- SNI and `Host` routing.
- Certificate SAN/CN, full chain, issuer, validity, key use, and supported TLS versions.
- CDN strict/full verification mode and origin trust.
- Reverse-proxy HTTP, WebSocket upgrade, timeout, body-size, and health routes.
- Direct-origin tests with `openssl s_client`, `curl --resolve`, and a browser from an approved external network.

## Completed Internal Preparation

- `scripts/audit-strict-tls.py` performs certificate-required, hostname-verified TLS 1.2-or-newer checks against both edge DNS and a direct origin while preserving the planned-domain SNI and `Host` value.
- The tool checks exact HTTP statuses and validates the complete WebSocket `101`, `Upgrade`, `Connection`, and `Sec-WebSocket-Accept` handshake. Any certificate, hostname, TLS, HTTP, or protocol error fails closed.
- `scripts/test-audit-strict-tls.py` covers valid SNI, minimum TLS, strict verification, HTTP, WebSocket, malformed headers, input rejection, and certificate-verification failure.
- `deploy/host/openresty-xs-nexus-vhost.conf.example` is a placeholder-only production template. It terminates strict TLS 1.2/1.3, redirects HTTP to HTTPS, routes every path to Console loopback `127.0.0.1:28081`, and preserves WebSocket upgrade headers.
- `scripts/validate-public-edge.py` rejects a Controller-root upstream, TLS 1.0/1.1, disabled upstream verification markers, missing placeholders, or missing WebSocket forwarding in that template.

These repository changes are preparation only. They do not prove the CDN control-plane mode, install a certificate, create a 1Panel site, or alter the running origin.

## Owner-Authorized Change Checklist

1. Approve a change window and identify the DNS/CDN and 1Panel owners. Export the current planned-domain DNS/CDN origin configuration, all related 1Panel/OpenResty files, certificate metadata, listeners, routes, firewall state, and `1panel-network` identity before changing anything.
2. Issue or import a valid full-chain origin certificate whose SAN contains `vpn.xiashikeji.cn`. Keep the certificate and private key outside Git and evidence with root-only permissions; verify the key matches the certificate without printing either value.
3. Render `deploy/host/openresty-xs-nexus-vhost.conf.example` outside the repository with the approved domain and certificate paths. Add only the planned-domain virtual host, retain unrelated sites, route all paths to `127.0.0.1:28081`, and run the platform's OpenResty configuration test before reload.
4. Before changing CDN behavior, verify direct origin TLS with `--resolve` and SNI for the planned domain, then verify root, health, install, authenticated API, and WebSocket paths. Roll back the virtual host immediately if any existing site, service, route, firewall rule, or network invariant changes.
5. Configure the CDN origin for HTTPS `443`, origin SNI and host `vpn.xiashikeji.cn`, full certificate-chain and hostname verification, and the approved origin address. Flexible SSL, plaintext origin, ignored errors, or disabled verification are forbidden.
6. Re-run the strict audit from at least one independent external client, export the CDN strict-mode configuration without credentials, and complete browser login, authenticated API, WebSocket, Console, unknown-route, and certificate-chain checks.
7. Recheck unrelated 1Panel sites, project containers, default route, IP rules, nftables, public exposure, failed units, and the exact pre-change `1panel-network` ID/subnet. Preserve the rollback package and evidence until independent approval.

Repository-side preparation is complete. Steps 1 through 7 remain `BLOCKED_EXTERNAL` pending owner authorization and control-plane access.

## Required Final State

- Strict certificate verification from CDN to origin.
- Valid origin certificate for the configured hostname/SNI.
- No Flexible SSL, disabled verification, ignored certificate errors, or plaintext HTTP final origin.
- Public health, login, authenticated API, WebSocket, Console E2E, and unknown-route rejection all pass.

DNS/CDN/1Panel reverse-proxy changes require owner authorization, precise baseline, rollback, and no modification to unrelated sites.
