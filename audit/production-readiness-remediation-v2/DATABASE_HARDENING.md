# PostgreSQL Least-Privilege Remediation

## Current Status

`FAIL`. The runtime Controller still uses a bootstrap superuser credential.

## Required Role Model

- `xs_nexus_owner`: non-login object owner where practical.
- `xs_nexus_app`: login role used only by the running Controller, with required schema/table/sequence/function privileges.
- `xs_nexus_migrator`: deployment-only login role with narrowly scoped schema migration rights.
- Bootstrap superuser: retained only for controlled platform administration and removed from long-running application secrets.

## Migration Sequence

1. Create and verify a current encrypted backup.
2. Capture ownership, grants, default privileges, extensions, functions, sequences, schema version, and active sessions without credential values.
3. Create roles through a no-echo secret channel.
4. Transfer ownership and set explicit/default grants.
5. Run migrations through the migrator role.
6. Run Controller integration and Console E2E through the app role.
7. Deploy the app credential from the repository-external secret store.
8. Prove the bootstrap credential is absent from the runtime environment and process.
9. Run negative tests proving the app role cannot create superusers/databases, alter unrelated schemas, or access unrelated objects.
10. Revoke obsolete runtime grants and prove old runtime credentials are rejected.

## Rollback

Before changes, record exact grants/owners and create a verified encrypted backup. If application validation fails, restore prior grants and secret reference without exposing values; if data state changes, use the documented schema restore transaction.

Status remains `FAIL` until the running Controller is reverse-verified against the least-privilege role and all negative tests pass.
