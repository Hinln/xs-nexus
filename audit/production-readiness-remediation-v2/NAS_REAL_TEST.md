# NAS Real-System Gate

## Status

`BLOCKED_EXTERNAL`. Namespace and AArch64 build evidence do not prove NAS compatibility.

## Required Order

1. User obtains a signed package and public key through an authenticated channel.
2. User runs the first install locally on the NAS; credentials are never stored in this repository.
3. Validate service start, TUN, enrollment, virtual IP, ordinary-node traffic, Direct, Relay, restart, upgrade, rollback, uninstall, and reinstall.
4. Only after ordinary-node stability, propose the real LAN prefix.
5. Approve the subnet in Console and validate allowed/denied ACL paths.
6. Reboot/offline the NAS, prove route withdrawal, then uninstall and prove cleanup.

## Risk And Rollback

Do not publish the LAN before ordinary-node validation. Preserve local NAS access and configuration backup; if virtual networking affects ordinary access, stop the Agent and run the trusted cleanup/uninstall path.

## Resume Condition

Provide no-secret system/build details, package manifest verification, command exit statuses, service/network before/after, Direct/Relay results, and cleanup evidence.
