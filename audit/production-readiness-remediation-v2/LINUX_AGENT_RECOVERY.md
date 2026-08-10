# Linux Agent Recovery Gate

## Current Result

`PARTIAL`.

The hosted x86_64 real-systemd crash/restart and isolated link-change submatrix is complete. The ordinary-host reboot, disk-full, DHCP/address-churn, competing-VPN and arm64/NAS matrix is not complete.

## Verified Automated Scope

- Exact source revision: `fb45fd43256d65cb4c72824d6cee0bec0884ad02`.
- GitHub Actions run: [`31352258781`](https://github.com/Hinln/xs-nexus/actions/runs/31352258781), all five jobs `PASS`.
- Dedicated recovery job: [`93345151258`](https://github.com/Hinln/xs-nexus/actions/runs/31352258781/job/93345151258).
- Recovery artifact: `9049384061`, archive SHA-256 `2a88d3a6344c66c699daff9966119df0a77718bcb6b68499ac43a293c742009f`.
- Inner evidence: `test.log` SHA-256 `53a3cd38ef1f6d7fabb7e9b0facaa2615fd9f2b235798dfa5d156456459dcce8`; `summary.txt` SHA-256 `14db1ee65fcb6d7d1e90fa8373375a92f6ce8395ec29094f3fe683a3af64364a`.
- Downloaded evidence no-value secret scan: `PASS`, zero findings.

The test runs a unique test-only Agent binary as a real transient systemd service with the relevant production sandbox properties and `PrivateNetwork`. It proves:

- `Restart=on-failure` replaces a `SIGKILL`ed main process with exactly one different PID.
- The signed state manifest persists across the crash.
- The non-persistent TUN is recreated inside the service network namespace only.
- An isolated link down/up event does not terminate the Agent.
- Service stop removes the transient unit and leaves no host TUN interface.
- Unit-file validation and runtime execution do not create or modify the formal production installation path.

## Retained Failed Evidence

- Run [`31351583134`](https://github.com/Hinln/xs-nexus/actions/runs/31351583134): ShellCheck exposed an invalid final negation pattern; corrected without suppressing the rule.
- Run [`31351658402`](https://github.com/Hinln/xs-nexus/actions/runs/31351658402): the Agent binary under the runner home was correctly blocked by `ProtectHome=yes` with `203/EXEC`; execution moved to a unique test-only installation root.
- Run [`31352027606`](https://github.com/Hinln/xs-nexus/actions/runs/31352027606), artifact `9049311530`: offline verification still required the formal host installation path; verification moved to an isolated `systemd-analyze --root` tree.

All three superseded runs were canceled only after their failing recovery jobs completed and the root cause was captured. No test, sandbox property, assertion, or warning was skipped or weakened.

## Remaining Hard Matrix

An approved disposable ordinary Linux host must still demonstrate:

- full operating-system reboot with Agent enabled and post-boot TUN/route/control recovery;
- state/config writes and restart behavior under bounded disk exhaustion, followed by recovery;
- DHCP lease/address/default-route churn without loss of ordinary networking;
- coexistence and conflict rejection with a separate VPN and competing routes;
- repeated service failure/start-limit handling and operator recovery;
- arm64 hardware behavior where applicable, with NAS kept under its separate approval gate.

These tests must not run on the current production server. They require a disposable host, console access, rollback/reimage capability, and evidence export independent of the tested host.

## Production Boundary

No production service, host route, firewall, Docker network, 1Panel resource, credential, or deployed image was changed by this work. Production continues to run revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` without a host Agent. Gate 06 remains `PARTIAL`; overall status remains `NO_GO`.
