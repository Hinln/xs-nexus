# Planned Domain And Strict TLS

## Current Status

`FAIL`. The planned domain has not demonstrated a complete strict-TLS path for health, API, WebSocket, and Console traffic.

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

## Required Final State

- Strict certificate verification from CDN to origin.
- Valid origin certificate for the configured hostname/SNI.
- No Flexible SSL, disabled verification, ignored certificate errors, or plaintext HTTP final origin.
- Public health, login, authenticated API, WebSocket, Console E2E, and unknown-route rejection all pass.

DNS/CDN/1Panel reverse-proxy changes require owner authorization, precise baseline, rollback, and no modification to unrelated sites.
