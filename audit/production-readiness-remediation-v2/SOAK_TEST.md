# Current-Revision Soak Test

## Status

`UNKNOWN`. No 24-hour raw evidence exists for the current remediation revision. The latest 600-second calibration passed at revision `795b1ea461a179958aed27e6935faba8f36e43ce`, but calibration is not Gate 22 evidence.

## Minimum Run

- Mandatory duration: 24 hours; preferred duration: 72 hours.
- Record exact Git revision, image digests, host baseline, configuration hashes, and UTC start/end.
- Sample CPU, RAM, RSS, FD, tasks, DB pool, disk/log growth, route/rule/interface count, Direct/Relay state, reconnects, and error counters.

## Fault Injection

- Controller and Relay restart.
- Database restart/recovery; Redis only if the tested deployment uses it.
- Agent reconnect and configuration/route update.
- Enrollment and revocation.
- Controlled loss/latency and path recovery.

## Pass Criteria

- No leak trend, route/interface/rule residue, reconnect storm, unbounded log growth, corruption, or security fail-open.
- Default route, SSH, 1Panel resources, unrelated Docker resources, and `1panel-network` remain unchanged.

The run must restart from zero if the revision, image, configuration, or test harness materially changes.

## Harness Calibration

- Exact revision: `795b1ea461a179958aed27e6935faba8f36e43ce`.
- GitHub Actions run/job: `31658778589` / `94318968217`; all 13 workflow jobs passed.
- Artifact: `9165687987`, `current-revision-soak-calibration-evidence`; GitHub digest `sha256:7387b961dc813b033cc6b80cfe9e23ebec5fb858c858f4ecc1c4cfbef5d298f9`.
- Independent verification: 84/84 manifest entries, zero secret-scan findings, 258 service resource rows, eight PASS fault events, and nine byte-identical before/after Docker/network/system invariants.
- Duration: 600 seconds with explicit calibration mode. It cannot satisfy the mandatory duration.

The harness evaluates growth within each contiguous process generation so planned restarts do not create false trends. PostgreSQL terminal stability is evaluated alongside global FD and connection ceilings. Route replacement may retry only an actual optimistic-lock `409`, after rereading the current version, with every attempt recorded; every other response fails immediately.

The retained failed run `31655990346` proves that configuration-version application alone does not imply immediate data-plane readiness. The configuration-update event now requires both Agents to report the exact established Direct path before the original ping assertion; the product timers and per-side 180-second wait bound are unchanged.

## Formal-Run Blocker

No approved persistent privileged Linux QA environment is currently available. The production server presents a changed ED25519 host key that has not been confirmed through an independent control-plane channel. Strict SSH verification stopped before authentication; no password was sent and no production resource was changed. The formal run must not start by disabling host-key verification or automatically accepting a TOFU key.
