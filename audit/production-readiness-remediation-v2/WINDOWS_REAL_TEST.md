# Windows Real-System Gate

## Status

`BLOCKED_EXTERNAL`. Cross-target builds, test-signed `xsnet` VM evidence, Wintun packaging, and offline smoke do not prove the complete online client lifecycle.

## Required Environment

- Controlled Windows 11 x64 VM with a verified restorable snapshot.
- Approved test network, current signed RC package, short-lived Enrollment Token, Linux peer, and evidence export path.

## Required Matrix

Install, service start, virtual NIC, enrollment, Windows-to-Linux traffic, Direct, Relay, reboot, sleep/resume, physical-network switch, Agent crash, route/DAD recovery, upgrade, failed-upgrade rollback, uninstall/reinstall, Driver Verifier where applicable, no BSOD, and zero device/route/service residue.

## Risk And Rollback

Do not use a daily workstation. Preserve ordinary-network access, take a snapshot before installation, disable/reset Verifier before leaving the test stage, and restore the snapshot after any boot/network instability.

## Resume Condition

Provide the VM build, snapshot identifier, exact RC manifest/digest, stage logs, route/device/service before/after, verifier/dump summary, traffic results, and cleanup proof without secrets or Enrollment Token values.
