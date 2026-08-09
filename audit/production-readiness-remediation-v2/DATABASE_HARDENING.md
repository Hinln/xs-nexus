# PostgreSQL Least-Privilege Remediation

## Current Status

`PASS` for Gate 16. Production revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` runs the Controller as `xs_nexus_app`; migrations use `xs_nexus_migrator` with an explicit transaction-local `SET ROLE xs_nexus_owner`; `xs_nexus_owner` cannot log in. The bootstrap superuser remains available only to controlled platform administration and is absent from the long-running Controller secret set.

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
10. Keep bootstrap rotation and old-value rejection in Gate 02; Gate 16 requires that bootstrap access is absent from the running application and cannot be reached through app or migrator roles.

## Implemented Controls

- `serve` never creates schemas or runs migrations. It verifies the configured role and exact `_sqlx_migrations` state before accepting traffic.
- Runtime and migration URLs are separate repository-external secrets. The migration URL cannot equal the runtime URL, and the owner role cannot log in.
- Role hardening transfers schema objects to `xs_nexus_owner`, grants only application DML/sequence/function use to `xs_nexus_app`, and prevents application writes to `_sqlx_migrations`.
- Migration reconciles grants after schema creation or restore, including the first migration into an empty schema.
- Backup and restore execute object restoration under the owner role; post-restore migration reapplies least-privilege grants.
- Actual negative tests reject database creation, role creation, schema creation, table alteration, and migration metadata writes by the application role.

## Validation And Production Evidence

- Code commits: `02fc54e`, `3e2caed`, `e0fd15d`, `0533727`, and `3d93656`.
- GitHub Actions run `31294988591` passed all jobs for exact revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`.
- Isolated full validation: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z`.
- Production evidence: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`.
- The production evidence retains four failed deployment-verifier attempts and four successful timed automatic rollbacks. The fifth attempt passed immediate verification, an independent new SSH session, timer cancellation, canonical environment promotion, secret-stage cleanup, and a final read-only check.
- `1panel-network` retained ID `7df70648b96ab2d6e5e178cce4e5892d655e7b451dd111f90f42ae86e3757ac0` and subnet `172.18.0.0/16`; OpenResty, the default route, IP rules, and all non-project nftables rules remained unchanged.

## Rollback

Before changes, exact grants/owners and row counts were captured and both the normal encrypted backup and a temporary isolated restore were verified. Each production attempt armed a 20-minute systemd rollback before role or container changes. Four verification failures automatically restored revision `ff9551d322067c934d2ac7d55a62af8896660bb3`; the old Controller remained compatible because the bootstrap role was not removed. The successful fifth attempt canceled rollback only after independent-session verification.

Gate 16 is closed. Gate 02 remains open: the bootstrap credential itself still requires an authorized rotation and explicit old-value rejection, and this document does not claim that broader credential rotation is complete.
