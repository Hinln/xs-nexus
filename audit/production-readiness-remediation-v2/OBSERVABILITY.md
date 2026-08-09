# Production Observability Remediation

## Current Status

`PARTIAL`. A local five-minute read-only health guard exists, but independent notification, formal on-call, certificate expiry, and complete service/platform metrics are not closed.

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
