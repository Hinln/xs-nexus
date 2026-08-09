# Production Observability Remediation

## Current Status

`PARTIAL`. A local five-minute read-only health guard exists. Gate 14 maintenance reduced root usage from `83%` to `77%` and recorded the final disk state, but independent notification, formal on-call, certificate expiry, and complete service/platform metrics are not closed.

## Required Metrics

- Controller, Relay, PostgreSQL, and Redis availability where applicable.
- Online nodes, Direct/Relay rates, handshake/auth failures, replay drops, ACL drops, route changes, and update failures.
- CPU, RAM, disk, inode, file descriptor, task, log growth, and network usage.
- Certificate expiry and TLS probe status.
- Backup age, copy receipt, and last deep verification.

## Required Alerts

- Controller/Relay/database unavailable.
- Disk warning and critical thresholds.
- Certificate expiry windows and strict-TLS failure.
- Backup stale or replica copy failure.
- Sustained error/auth/Relay abuse thresholds.

## Production Passage

At least one alert must be delivered to a destination independent of the monitored host, acknowledged by the documented on-call path, and recovered/closed after fault removal. Until then Gate 20 remains `PARTIAL`.

Current Gate 14 evidence explicitly records the disk-alert control as `BLOCKED_EXTERNAL` in `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z/external-disk-alert-status.txt`; no local log or simulated threshold is accepted as delivery proof.
