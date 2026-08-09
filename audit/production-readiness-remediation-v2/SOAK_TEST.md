# Current-Revision Soak Test

## Status

`UNKNOWN`. No 24-hour raw evidence exists for the current remediation revision.

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
