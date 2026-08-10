# Gate 09 ACL Enforcement

Status: `PASS`  
Overall production decision: `NO_GO`

## Scope

Gate 09 independently revalidates ACL enforcement across real Linux Agent processes, TUN interfaces, XSP/1 sessions, Direct transport, authenticated Relay fallback, and an approved subnet-router path. It does not reuse the earlier M3.1 conclusion as proof.

The authoritative source revision is `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b`. Production was not changed and continues to run the older approved runtime revision recorded in `ENVIRONMENT.md`.

## Required Matrix

| Requirement | Evidence | Result |
|---|---|---|
| A to B allowed | Three real Agents/TUNs; ICMP, selected TCP, selected UDP | PASS |
| A to C denied | Sender emits no matching XSP/1 frame for ICMP/TCP/UDP | PASS |
| C to B denied | Sender emits no matching XSP/1 frame for ICMP/TCP/UDP | PASS |
| Unexpected port denied | Unapproved TCP and UDP ports produce no matching XSP/1 frame | PASS |
| Forged source virtual IP | C attempts to use A's virtual source identity | REJECTED |
| Forged Node ID | Authenticated XSP/1 source and destination Node ID bytes are altered | REJECTED |
| Stale configuration | Lower configuration version preserves the current state | REJECTED |
| Policy rollback | Higher envelope version with a lower policy version preserves the current state | REJECTED |
| Same-version equivocation | Same version with different signed content preserves the current state | REJECTED |
| Route through allowed node | C cannot obtain A's permission by presenting a routed A source | REJECTED |
| Sender enforcement | Denied traffic produces no matching encrypted frame | PASS |
| Receiver enforcement | Sender emits authenticated ciphertext, but B emits no denied plaintext | PASS |
| Relay bypass | Denied UDP produces no XSR/1 data frame and no destination plaintext | REJECTED |
| Subnet-router bypass | Denied TCP and UDP produce no XSP/1 frame and no LAN delivery | REJECTED |
| Controller disconnected | All three Agents remain active on the last signed policy with the Controller unavailable | PASS |

The Direct topology uses three isolated network namespaces joined only by a test bridge. The Relay and subnet-router regressions use separate isolated namespaces and real project binaries. These are real kernel/TUN/network-process tests, not mocks; they remain distinct from public-WAN and real-NAS Gates 07, 08, 10, and 12.

## Finding Closed

The review found that `NodeState::apply_configuration` enforced monotonic `configuration.version` but could accept a signed configuration with a higher envelope version and a lower `policy_version`. This could roll ACL policy back while satisfying the outer version check.

Revision `3f27ed9` adds an independent monotonic policy-version guard before mutation. The regression `configuration_update_rejects_policy_rollback_without_mutating_state` proves that the current signed state remains unchanged. Existing stale-version, same-version equivocation, invalid-signature, and invalid-ACL rejection tests remain enabled.

## Exact-Head Validation

| Evidence | Value |
|---|---|
| Git revision | `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` |
| GitHub Actions run | `31360862865` |
| Run result | `PASS`, all 7 jobs |
| ACL job | `93369332314` |
| ACL artifact | `9052383034` (`acl-enforcement-evidence`) |
| Artifact archive digest | `fd5cbc219591264ae6f1376db2d5c4aa9949c33be4684196887a3703a9ef8e23` |
| Run interval | `2026-08-10T06:08:19Z` to `2026-08-10T06:27:55Z` |

The seven passing jobs are baseline, protocol fuzzing, real Console E2E, image reproducibility, Linux Agent recovery, Relay resilience, and ACL enforcement. The ACL job runs ShellCheck, identity/configuration regressions, the disconnected-controller three-node matrix, Relay bypass, and subnet-router bypass.

## Downloaded Artifact Verification

The downloaded artifact was independently checked after extraction. Its internal `SHA256SUMS` verifies all six evidence payloads, and the repository no-value secret scanner reports zero findings.

| File | SHA-256 |
|---|---|
| `configuration.log` | `d611f95755d774e372f38e7ee6de44a196f1cd8086e8ea47d27c939daeba0da5` |
| `direct.log` | `06e88d7b7ce02142af94365ce2a3ba6afc31e94e8b3731c8d6cf47afbd7904a7` |
| `protocol.log` | `ead4a569dfe607e5556308ec30990944507a4b34a48e12fc9128300494d43bd2` |
| `relay.log` | `f3d041d2d28267e7d8f3d53af1e2b76cc716d4ca906dc5db778cd13ebbaec094` |
| `subnet.log` | `bb51fe716952f3f1c46c561359004179cc76f97ff23490768aa78f4be33df5ff` |
| `summary.txt` | `0988df5c4fe1794aa0f43032386a4d6f0f60825a22bb4bb0fcc94aa3782bb334` |
| `SHA256SUMS` | `8a43cf8a4619c957baf7e108c68b4e4f879e6934bbf42ab169f0aa14b97a5205` |

## Decision Boundary

Gate 09 changes from `PARTIAL` to `PASS`. This result does not claim real public-WAN behavior, real NAS/subnet-router operation, independent protocol review, production deployment, formal signing, credential rotation, or a production `GO`. Those gates retain their existing status, and the project remains `NO_GO`.
