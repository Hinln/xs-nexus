# Windows Real-System Gate

## Status

`BLOCKED_EXTERNAL`. Native MSVC Agent/CLI builds and unit/boundary tests, cross-target builds, test-signed `xsnet` VM evidence, Wintun packaging, and offline smoke do not prove the complete online client lifecycle.

Repository evidence at revision `16ee4bf7be4687f2ac307c7a1235f7d92275e855`, run `31667104728`, artifact `9168434013` independently proves the native build boundary. Its summary explicitly records `device_installation=false`, `driver_verifier=false`, and `production_mutation=false`; it is prerequisite evidence, not this gate's PASS evidence.

## Required Environment

- Controlled Windows 11 x64 target. Prefer a VM with a verified restorable snapshot. A physical target is allowed only after the first VM driver gate has passed and the owner dedicates it to testing for the entire run, with a verified external full-system image, bootable recovery media, disk-recovery material, and an onsite recovery operator.
- Approved test network, current signed RC package, short-lived Enrollment Token, Linux peer, and evidence export path.

## Required Matrix

Install, service start, virtual NIC, enrollment, Windows-to-Linux traffic, Direct, Relay, reboot, sleep/resume, physical-network switch, Agent crash, route/DAD recovery, upgrade, failed-upgrade rollback, uninstall/reinstall, Driver Verifier where applicable, no BSOD, and zero device/route/service residue.

## Risk And Rollback

Do not continue ordinary work on the target during testing. Preserve ordinary-network access. Before installation, create the VM snapshot or complete and verify the physical-target recovery set. Disable/reset Verifier before leaving the test stage. After any boot/network instability, stop remote automation and use the approved local recovery path; do not repeatedly reboot an unknown driver state.

## Resume Condition

Provide the Windows build, target type, snapshot or system-image identifier, recovery-media and operator receipts for a physical target, exact RC manifest/digest, stage logs, route/device/service before/after, verifier/dump summary, traffic results, and cleanup proof without secrets, disk-recovery values, or Enrollment Token values.
