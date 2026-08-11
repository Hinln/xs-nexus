# Production Observability Remediation

Status: `PARTIAL`  
Automated repository submatrix: `PASS`  
Overall production decision: `NO_GO`

## Scope

This report covers the observability work that can be reproduced without changing production: authenticated low-cardinality Controller aggregation, host and Docker collection, strict TLS and backup checks, bounded alert state, webhook delivery, systemd hardening, real PostgreSQL integration, and Linux alert lifecycle tests. It does not claim that a production destination received or acknowledged an alert.

Validated source revision: `9291400ac030045e8ea2955ea137e7dc8be37a85`  
GitHub Actions run: [`31537716553`](https://github.com/Hinln/xs-nexus/actions/runs/31537716553)  
Dedicated job: [`93932721320`](https://github.com/Hinln/xs-nexus/actions/runs/31537716553/job/93932721320)  
Artifact: `9119468792` (`production-observability-evidence`)  
Archive SHA-256: `7b5fadda34a612716a6176767b465006a5e6e4175a61464e6959f4ec64fd5c79`

The dedicated job passed ShellCheck, Python compilation, a real PostgreSQL Controller integration test, the complete host alert matrix, evidence secret scanning, and both evidence checksum layers. The exact-head workflow completed all ten jobs successfully, including strict baseline, six-target protocol fuzzing, image reproducibility, real Console E2E, Agent recovery, ACL, Relay resilience, update supply chain, and 1Panel coexistence.

## Collection Contract

- Agent schema 2 reports authenticated cumulative traffic, handshake, ACL-drop, and replay-drop counters while preserving schema 1 rolling compatibility.
- Relay reports authenticated cumulative receive/forward, retry, classified drop, I/O-error, and queue-related counters. Reports omit Network, Node, endpoint, Lease, and payload identity.
- Controller stores bounded samples and exposes only authenticated aggregate totals, freshness, completeness, Direct/Relay observation ratios, routing/update state, database status, and explicit Redis `not_applicable` state.
- The root-only host collector reads `/proc`, statvfs, bounded log paths, exact Docker containers, loopback health, the Controller aggregate, TLS targets, encrypted-backup age, and private copy/deep-verification receipts.
- Latest Prometheus, snapshot, alerts, and state files are atomically replaced. The component stores no unbounded history and emits no node-, network-, relay-, user-, token-, or payload-level labels.

## Security Boundaries

- Controller collection is restricted to loopback and disables environment proxies; its bearer token is read from an owner-matched `0600` regular file with `O_NOFOLLOW`.
- Plain HTTP probes are loopback-only. Probe URLs reject credentials, query, and fragment; curl configuration and redirects are disabled.
- Production webhooks require HTTPS, reject URL credentials/query/fragment, disable redirects and environment proxies, and use a separate private token file.
- Receipt and secret reads are size-bounded regular-file reads. Private files must match the service UID and expose no group/other permissions.
- Backup and receipt timestamps fail closed on missing, stale, malformed, unsafe, or future values. TLS uses default trust, hostname validation, SNI, and expiry windows.
- The systemd oneshot has an empty capability set, strict filesystem/kernel protection, private devices/tmp, bounded address families, namespace restrictions, native syscall architecture, and a four-minute timeout.
- Notification failures become local critical state but are excluded from notification transitions, preventing recursive alert storms.

## Automated Matrix

| Area | Required assertion | Result |
|---|---|---|
| Static gates | ShellCheck and Python compilation | PASS |
| Controller | Real PostgreSQL registration, telemetry persistence, aggregation, auth, freshness, and schema compatibility | PASS |
| Host | CPU, memory, network, FD, tasks, disk, inode, logs, Docker state/stats, and HTTP health | PASS |
| TLS | Real local certificate chain, hostname, SNI, and expiry probe | PASS |
| Backup | Encrypted artifact age plus private copy and deep-verification receipt contracts | PASS |
| Alert lifecycle | warning firing, unchanged deduplication, severity change, critical exit, resolved delivery | PASS |
| Faults | unhealthy container, failed HTTP endpoint, failed webhook, bounded pending retry | PASS |
| Security | URL guards, private-file checks, proxy isolation, credential non-persistence, atomic output | PASS |
| Queue | Maximum 256 unique pending events and explicit overflow accounting | PASS |
| Service | Offline `systemd-analyze verify` of the hardened service and timer | PASS |
| Evidence | Scenario matrices, exact revision, environments, two SHA-256 manifests, and no-value scan | PASS |

The host scenario matrix contains eleven passing scenarios. Warning exits remain successful for metric-driven escalation, while any critical observation returns nonzero and fails the oneshot service.

## Evidence Integrity

- The independently downloaded ZIP matches archive SHA-256 `7b5fadda34a612716a6176767b465006a5e6e4175a61464e6959f4ec64fd5c79`.
- The outer `SHA256SUMS` digest is `d6d0b12763e7bd10e1e37e9cc1384db40f8b353a5e8bd8776e57d35ef1727f95` and verifies 21 entries.
- The host `SHA256SUMS` digest is `b2713ff0203d14df272cb2c5fe43be00d93f30ba9f2ef15d5001855656ee7a88` and verifies 13 entries.
- Both the GitHub-extracted copy and an independent ZIP extraction pass `scripts/check-secrets.py` with zero findings.
- The artifact records Ubuntu 24.04 hosted Linux, Rust 1.94.0, Python 3.12.3, ShellCheck 0.9.0, OpenSSL 3.0.13, and exact revision `9291400ac030045e8ea2955ea137e7dc8be37a85`.

## Retained Failures

| Run | Artifact | Failure exposed | Disposition |
|---|---:|---|---|
| `31535053669` | n/a | Duplicate SQLx migration version and strict function-length lint failures | Migration moved to unique version 11; large functions split without semantic changes |
| `31536076514` | n/a | Protocol vector generator exceeded the strict 100-line function limit | Vector writer extracted; vector bytes and wire semantics unchanged |
| `31537181594` | `9119262890` | ShellCheck found declaration/checksum issues; the Docker fixture referenced an unset default state | Root causes fixed; failed archive retained with SHA-256 `929f20a399fe2bb61842b13291b6a70b4799015ca022723e512eb70acc0af264` |
| `31537531579` | `9119401167` | Webhook proxy-isolation fake response omitted the production client's bounded response read | Fake now implements and asserts the one-byte read; failed archive retained with SHA-256 `96fba0425835127f992781e505ae5edd0cea70c4d0fce5dde366b1e6744c4aa8` |

Superseded long workflows were cancelled only after a newer exact revision started the same complete job set. The final revision does not skip, suppress, allow-fail, or weaken any gate.

## Production Passage

Repository-side metrics and alerting are complete, but Gate 20 remains `PARTIAL/BLOCKED_EXTERNAL`. Production passage still requires all of the following on the exact approved release:

1. Deploy the collector with reviewed real container names, thresholds, TLS targets, backup paths, and root-only token files.
2. Retain continuous Controller, Relay, database, host, TLS, and backup observations at the independent monitoring destination.
3. Deliver at least one warning and one critical event to a destination independent of the monitored host.
4. Record the documented on-call acknowledgement and escalation path.
5. Remove the fault and prove the matching resolved event closes at the independent destination.
6. Seal the production evidence with exact revision, configuration hashes, timestamps, retention proof, checksums, and a no-value secret scan.

Gate 14 evidence still records the missing external disk-alert path at `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z/external-disk-alert-status.txt`. CI fixtures, localhost webhooks, local state, and journal entries are not accepted as production delivery evidence. Production was not modified during this work, and the overall decision remains `NO_GO`.
