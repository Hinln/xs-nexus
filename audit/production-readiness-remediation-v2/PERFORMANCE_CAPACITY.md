# Gate 21 Performance and Capacity Evidence

Date: 2026-08-12  
Decision: `PARTIAL`  
Overall release decision: `NO_GO`

## Decision

Exact revision `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363` completes the reproducible internal single-instance capacity matrix for 1,000 enrolled nodes and 1,000 authenticated control sessions. It does not establish a public-WAN, multi-region, multi-instance, volumetric-abuse, or hours/days production operating envelope. Gate 21 therefore remains `PARTIAL/BLOCKED_EXTERNAL`.

No production container, host network, firewall, route, service, or `1panel-network` resource was changed by this validation.

## Exact Evidence

| Item | Evidence |
|---|---|
| Revision | `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363` |
| GitHub Actions run | `31603852656` |
| Performance job | `94137661764` |
| Artifact | `9144433450`, `performance-capacity-evidence` |
| GitHub artifact digest | `sha256:d0c05972b353476362cd1e62ff86f4bfc77fa027371bc5e67977131ab686f0b8` |
| Artifact expiry | `2026-11-10T13:54:02Z` |
| Extracted outer manifest hash | `9f6590bc3b586652c4b8d9346a5d274f68cc184e80c171f8242ec459f88550f5` |
| Outer manifest verification | 41 of 41 files |
| Agent manifest | `f3cde522980a0aa96ff4bf01d201f01f1520d6a34a050e8afb35d8f6d8b4d5f8`, 11 of 11 files |
| Controller manifest | `a298ae4a9ffe4bcd5ede80257c781a21dcbb23355cb5c76079f72e5f8c28bb5d`, 8 of 8 files |
| Protocol manifest | `3dbcfe9ad6ba3df186e1f828338a53c7c4e9366883900e53a053d85e3c2c05af`, 8 of 8 files |
| Secret scan | `python scripts/check-secrets.py --root <download>`: PASS, zero findings |
| Environment | GitHub hosted Ubuntu, Linux `6.17.0-1020-azure`, x86_64, Rust/Cargo `1.94.0`, Python `3.12.3`, ShellCheck `0.9.0`, iproute2 `6.1.0` |

The GitHub API digest binds the uploaded artifact. The independently downloaded extracted payload, nested manifests, revision files, summaries, and reports were separately verified. This report does not claim an independently computed ZIP digest.

Latest exact code revision `67152427acc197deca49443a8527c74b18d71098` passed all 13 jobs in run `31661323846`. Performance job `94326567382` and artifact `9166384854` have GitHub digest `sha256:0596b994919de5f083006ee4f4093dab4c0cdf5dfb649656b9b9e06a5708a3`; all 72 manifest entries, nested status/revision bindings, and a new independent secret scan pass. The final 100-sample Direct average/p95 is `0.322`/`0.385` ms and Relay is `0.365`/`0.431` ms. Failed run `31657413129` remains retained with Direct average/p95 `7.242`/`39.441` ms; the steady-state gate now allows at most 12 separately recorded ten-packet attempts and does not change the formal 5/10 ms bounds.

## Bounded Controller Envelope

The validated hard bounds are 1,000 enrolled nodes per network, 1,000 simultaneous control sessions, 64 concurrent signed-configuration sends, a 512 KiB logical control message limit, and authenticated `chunked-v1` transport for large configuration responses. Admission of the 1,001st node and 1,001st control session fails closed before allocating a credential or admitting a WebSocket.

| Measurement | Result | Enforced acceptance bound |
|---|---:|---:|
| First 100 node registrations | 166.14/s | at least 5/s |
| Nodes 101-500 | 121.72/s | at least 5/s |
| Nodes 501-1000 | 68.61/s | at least 5/s |
| 1,000 control authentications | 321.90/s | at least 5/s |
| Authentication average / p95 | 195.53 / 461.39 ms | p95 at most 10 s |
| Configuration synchronization average / p95 | 57.26 / 63.99 ms | p95 at most 5 s |
| Enrollment-time Console snapshots | 18.82 / 24.40 / 34.65 ms | at most 2 s |
| 1,000-node Console snapshot | 63.57 ms | at most 2 s |
| PostgreSQL count query | 0.829 / 1.034 / 1.199 ms | at most 100 ms |
| Online / offline convergence | 56.05 / 274.97 ms | at most 30 s |
| Controller RSS | 66,668 KiB | at most 512 MiB |
| Controller file descriptors | 1,022 | at most 2,500 |
| PostgreSQL connections | 11 | at most 16 |

All 1,000 authenticated large configuration responses used chunk transport. The signed configuration was 434,580 bytes and 73 chunks. A legacy client failed closed on that large response. The 1,001st enrollment returned HTTP 503 with `capacity_exhausted`; its token use count remained zero and active-node count remained 1,000. The 1,001st control session was rejected before upgrade. Capacity telemetry reported 1,000 active sessions, maximum 1,000, one rejected session, send concurrency zero after completion, maximum send concurrency 64, and maximum nodes 1,000.

The retained-frame root-cause fix reduced Controller RSS from 490,496 KiB in the retained failing run to 66,668 KiB in the final run, an 86.4% reduction. No limit or security assertion was weakened to obtain the result.

## Gate 21 Capacity-Revision Results

| Area | Result |
|---|---|
| XSP/1, 50,000 iterations, 1,200-byte payload | 188,340.94 seal+open operations/s; 215.54 MiB/s |
| Direct Agent RTT, 100 steady-state samples after 10 warm-up packets | average 1.8324 ms; p95 0.6138 ms; maximum 105.4797 ms |
| Relay Agent RTT, 100 samples | average 0.3281 ms; p95 0.6151 ms; maximum 4.1320 ms |
| Relay increment | average -1.5043 ms; p95 0.0013 ms |
| Agent idle resources | average 0.3000% of one core; 8,560 KiB RSS; 5 threads; 15 FDs per Agent |

The unchanged enforced bounds remain Direct average/p95 at most 5/10 ms, Relay average/p95 at most 10/20 ms, and Relay p95 increment at most 15 ms. The retained 105.4797 ms Direct maximum is not hidden; one maximum outlier does not change the 100-sample p95, while the larger sample set prevents two path-transition outliers from dominating a 30-sample p95.

## Retained Failure Chain

| Finding | Retained evidence |
|---|---|
| `XS-2026-0066` cleanup trap hid a failed capacity test | run `31550700862`, artifact `9124266819` |
| `XS-2026-0067` retained control frames caused excessive Controller RSS | runs `31551651020` and `31552305120`, artifacts `9124595037` and `9124815539` |
| `XS-2026-0068` bounded transport and capacity implementation | final exact-head run `31603852656` |
| `XS-2026-0069` strict Clippy regressions during finalization | runs `31558236452`, `31602351182`, and `31603038239` |

The final exact-head run passed all 11 jobs: baseline, ACL enforcement, Console real E2E, image reproducibility, Linux Agent recovery, 1Panel coexistence, performance capacity, production observability, protocol fuzz, Relay resilience, and update supply chain.

## Residual External Proof

Gate 21 cannot become `PASS` until current release evidence covers:

- public-WAN latency, loss, jitter, MTU, NAT, and carrier variability;
- multi-region and multi-instance Controller/Relay behavior;
- volumetric and distributed valid-credential abuse at the cloud edge;
- horizontal saturation, rate limiting, alerting, and recovery;
- hours/days control-session and traffic behavior at the claimed operating scale.

These items remain tracked by `PRV2-018` and `KI-028`. Gate 22 current-revision 24-hour soak is separate and remains `UNKNOWN`.
