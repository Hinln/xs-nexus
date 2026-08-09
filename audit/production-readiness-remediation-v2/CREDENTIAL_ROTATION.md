# Credential Rotation Register

No credential values are permitted in this file.

| Secret ID | Category | Rotated | Old Rejected | Evidence | Status |
|---|---|---:|---:|---|---|
| CRED-SSH-ROOT | Server root/password authentication | no | no | Gate 14 proves the SSH password path disabled; the disclosed OS credential itself still requires rotation and rejection | OPEN |
| CRED-SSH-ADMIN | Deployment administrator key/account | yes | yes | Gate 14 current-key success and prior-key rejection | COMPLETE |
| CRED-NAS | NAS login | no | no | external owner action required | BLOCKED_EXTERNAL |
| CRED-PG-BOOTSTRAP | PostgreSQL bootstrap/superuser | no | no | removed from runtime; authorized rotation still pending | OPEN |
| CRED-PG-APP | PostgreSQL runtime application role | yes (new role) | n/a (no prior role credential) | Gate 16 production evidence | COMPLETE |
| CRED-PG-MIGRATOR | PostgreSQL migration role | yes (new role) | n/a (no prior role credential) | Gate 16 production evidence | COMPLETE |
| CRED-REDIS | Redis authentication | no | no | service not currently used; historical values still require owner review | OPEN |
| CRED-MYSQL | MySQL authentication | no | no | service not currently used; historical values still require owner review | OPEN |
| CRED-CONSOLE-ADMIN | Console administrator | no | no | pending | OPEN |
| CRED-CONTROLLER | Controller online signing/session secrets | no | no | pending | OPEN |
| CRED-ENROLLMENT | Enrollment tokens | no | no | revoke-all and negative verification pending | OPEN |
| CRED-NODE | Node credentials | no | no | inventory and selective re-enrollment pending | OPEN |
| CRED-TLS | TLS/origin credentials | no | no | external DNS/CDN/origin control required | BLOCKED_EXTERNAL |
| CRED-BACKUP | Backup encryption identity | no | no | formal offline ceremony required | BLOCKED_EXTERNAL |
| CRED-UPDATE | Release/update signing identity | no | no | formal offline ceremony required | BLOCKED_EXTERNAL |
| CRED-RECOVERY | Recovery credentials | no | no | formal owner-controlled procedure required | BLOCKED_EXTERNAL |
| CRED-CI | CI/registry/GitHub credentials | no | no | owner/organization review required | BLOCKED_EXTERNAL |

## Required Scan Surfaces

- Current tree, full Git history, all branches and tags.
- GitHub workflow logs and artifacts.
- Docker image configuration, history, and exported layers.
- Host and container logs, shell history, screenshots, source maps, QA evidence, and backup metadata.

## Completion Rule

An entry is complete only when the new credential is active through an approved secret channel and the old credential is independently proven rejected. Reports record only identifiers and yes/no outcomes.
