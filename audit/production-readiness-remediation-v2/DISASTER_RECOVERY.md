# Disaster Recovery Remediation

## Current Status

`BLOCKED_EXTERNAL` for production passage. Encrypted backup, integrity, retention, fetch, restore, and rollback logic exist, but the current replica is not an independent failure domain and no clean-server restoration has been performed.

## Codex-Completable Work

- Keep public-recipient-only encryption on the database host.
- Validate scheduling, retention, stale-backup alerting, copy receipts, fetch, deep verification, restore, and failure rollback.
- Provide infrastructure-neutral bootstrap and restore checklists for a new server.
- Produce no-secret evidence and a machine-readable recovery manifest.

## External Completion Steps

1. Provision storage or a host in a genuinely independent failure domain.
2. Generate and hold the age identity off the database host.
3. Copy a fresh production backup and verify receipt/integrity.
4. Provision a clean recovery server.
5. Restore database and required public configuration from documented inputs.
6. Validate migrations, users, ACL, routes, Controller/Relay/Console health, and selected application records.
7. Record RPO/RTO and destroy the temporary environment safely.

## Risk And Rollback

Never overwrite the active production schema during the first exercise. Restore into an isolated recovery environment; if validation fails, retain encrypted source evidence and discard only the isolated failed target.

## Resume Condition

Provide offsite receipt, deep-verification result, clean-server restore log, application checks, RPO/RTO, and evidence that the private identity never resided on the production database host.
