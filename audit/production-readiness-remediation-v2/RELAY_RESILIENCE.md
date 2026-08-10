# Gate 08 Relay Security and Resilience

Status: `PARTIAL`  
Automated hosted submatrix: `PASS`  
Overall production decision: `NO_GO`

## Scope

This report covers the independently reproducible Relay work that can run safely on a GitHub hosted Linux runner: aggregate admission and queue bounds, authenticated sustained forwarding, short-Lease renewal, controlled two-Relay process loss/restart, ciphertext preservation, replay/source rejection, and cleanup. It does not claim public-WAN, cloud-DDoS, multi-region, horizontal-scaling, or long-duration production readiness.

Validated source revision: `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e`  
GitHub Actions run: [`31358498444`](https://github.com/Hinln/xs-nexus/actions/runs/31358498444)  
Relay job: [`93362562136`](https://github.com/Hinln/xs-nexus/actions/runs/31358498444/job/93362562136)  
Artifact: `9051561308` (`relay-resilience-evidence`)  
Archive digest: `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67`

The exact-head run completed all six jobs successfully: baseline, protocol-fuzz, console-real-e2e, image-reproducibility, linux-agent-recovery, and relay-resilience.

## Security Controls

- A global one-second registration budget is consumed before allocating per-source state or performing signature verification.
- Per-destination packet/byte queues remain bounded and are additionally constrained by exact process-wide packet and byte limits.
- Configuration fails closed if a global queue cannot contain one complete per-node queue.
- Queue rejection occurs before replay-window or traffic-budget commit, so a rejected authenticated datagram can be retried after capacity is released.
- Enqueue, dequeue, Lease replacement, expiration, and cleanup update the same global counters and queue gauges.
- Existing credential, node signature, Network/Node/Relay binding, random short Lease, source endpoint, expiration, replay, per-Lease rate, destination Lease, ciphertext, and non-amplification checks remain enabled.

## Automated Matrix

| Area | Required assertion | Result |
|---|---|---|
| Registration source spray | Global budget rejects new source identities before source-state growth | PASS |
| Shared packet capacity | Multiple destination Leases cannot exceed the process packet limit | PASS |
| Shared byte capacity | Multiple destination Leases cannot exceed the process byte limit | PASS |
| Transactional rejection | Queue rejection leaves replay and traffic state unconsumed | PASS |
| Cleanup | Expiration/removal releases packet/byte accounting and permits retry | PASS |
| Sustained forwarding | Exactly 5,000,000 authenticated frames, zero Relay drops, zero final queue, at least 10,000 packet/s | PASS |
| Lease lifecycle | Both nodes renew using the same identity/endpoint and unique signed requests; new Lease sequence starts independently | PASS |
| Primary loss | Authenticated traffic continues through the secondary Relay | PASS |
| Primary restart | Both Agents re-register with the restarted Relay | PASS |
| Secondary loss | Authenticated traffic recovers through the restarted primary | PASS |
| Direct restoration | Both directions return to an authenticated Direct path | PASS |
| Confidentiality/abuse | Relay capture excludes business plaintext; replay and forged source are rejected | PASS |
| Cleanup | Agents, Relays, namespaces, TUNs, schema, and temporary state are removed | PASS |

## Capacity Result

The release-profile test forwarded 5,000,000 frames of 216 bytes in 70.213 seconds:

- 71,212.14 packet/s;
- 14.67 MiB/s;
- internal forwarding latency average 5 µs, maximum 150 µs;
- `packets_forwarded=5,000,000`;
- `packets_dropped=0`;
- final global queued packets and bytes both zero.

The two nodes renew every 400,000 frames with the same identity and UDP endpoint but a unique signed request. This exercises the protocol's short-lived Lease behavior and fresh per-Lease replay sequence. The fixture does not lengthen production TTL, bypass expiration, increase registration allowances, reduce the packet count, lower the throughput threshold, or tolerate packet loss.

## Evidence Integrity

The downloaded artifact passed `scripts/check-secrets.py` with zero findings. Its outer `SHA256SUMS` verifies the integration log, unit log, capacity manifest, environment, report, revision, test output, and summary. The capacity manifest stores relative names and verifies directly after download:

| File | SHA-256 |
|---|---|
| `environment.txt` | `6814fd758dc08525f5c15db864b3d406b65c23673fe7ecdba956a2b864ab57bf` |
| `report.json` | `74e08d337e505957f06b210847a8e00211c2a97ae449a7e501483c120653eb6e` |
| `revision.txt` | `5d6d0a09c5fa0022de9a1fe978f5ddbf83d2d9db7a6e8440c763d9a853ed1d18` |
| `test-output.txt` | `a323489694070c22671af44b6393183304ca6eb9fc74d5f028a65e25d36cb665` |

## Retained Failures

| Run | Failure exposed | Disposition |
|---|---|---|
| `31354320136` | Restarted primary was authenticated but the harness accepted only one valid Relay reason string | Assertion now binds the authenticated endpoint and accepts only the two protocol-defined Relay reasons |
| `31354554543` | Baseline rustfmt failure | Formatting applied; no check disabled |
| `31355195399` | Invalid report numeric conversion | Type-safe floating calculation added |
| `31355385268` | Cargo package working directory invalidated a relative report path | Evidence directory canonicalized before test execution |
| `31355663342` | Silent destination reached real idle Lease expiry during the sustained run | Both real identities now renew periodically; TTL/expiry remains enforced |
| `31356055876` | Refactor omitted the fixture identity seed | Identity fixture data preserved explicitly |
| `31356167042` | Strict Clippy rejected a needless borrow after Relay itself passed | Borrow corrected; strict `-D warnings` retained |
| `31357225628` | All jobs passed, but independent artifact review found the inner manifest recorded runner-absolute paths | Manifest generation changed to portable relative names and rerun |

No failed test was deleted, ignored, converted to mock evidence, or weakened.

## Residual Hard Gates

The following remain `BLOCKED_EXTERNAL` under `PRV2-018` and `KI-028`:

- adversarial traffic from independent public networks and carriers;
- packet loss, duplication, reordering, jitter, and path changes across real WANs;
- distributed valid-node registration and traffic abuse;
- volumetric DDoS and independent cloud-edge mitigation evidence;
- multi-region and multi-instance load distribution, replacement, and recovery;
- hours/days resource, connection-storm, and safe-operating-envelope evidence;
- independent security and capacity review.

Production was not modified or deployed during this Gate 08 work. The running production revision remains older than the validated branch. Gate 08 therefore remains `PARTIAL`, and XS Nexus remains `NO_GO`.
