# Gate 18 Update and Release Supply Chain

## Decision

`PARTIAL` for Hard Gate 18 at candidate revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1`.

The independently executable repository-side failure matrix is complete and passed. Gate 18 is not `PASS` because the project still lacks the owner-controlled offline signing ceremony, authenticated production key and revocation distribution, a formally signed RC, real target-platform release execution, and the production release/rollback chain required by Gates 01 and 25. The overall decision remains `NO_GO`.

## Internal Requirement Matrix

| Requirement | Evidence | Result |
|---|---|---|
| One canonical release contract | Core, Agent, Controller, package builder, installer, and one-click path consume schema 2 with exact source commit, source epoch, protocol, platform, architecture, target, archive identity, size, and SHA-256 fields | PASS |
| Canonical numeric and size bounds | Semantic version components are canonical `u32`; source epoch is a canonical positive `u64`; archives are capped at 512 MiB | PASS |
| Signature and key rotation | One to four unique Ed25519 PEM public keys are accepted; old/new overlap works; unknown, duplicate, retired, unsigned, and tampered inputs fail closed | PASS |
| Controller revocation | Revocation is one-way, reason-bounded, audit-redacted, pauses all referring policies, increments generation, rejects future policy assignment, and suppresses directives | PASS |
| Offline revocation boundary | A root-owned, non-group/world-writable, sorted unique manifest-hash ledger is checked before staging, privileged apply, and rollback execution | PASS |
| Download and storage failures | Truncation, network interruption, oversized response, write failure, and real tmpfs ENOSPC never publish a ready candidate | PASS |
| Platform and identity binding | Wrong platform, wrong architecture, source-identity drift, archive drift, and payload tamper are rejected before activation | PASS |
| Rollback safety | Old-version external install is rejected; explicit rollback revalidates metadata and revocation; a revoked rollback executes no installed candidate binary | PASS |
| State preservation | Failed update preserves current release, node identity, signed state, service state, and route state; pre-activation staging is removed | PASS |
| Network cleanup | A real isolated network namespace proves the test route is singular while active and leaves no namespace or route residue | PASS |
| Console operation | Release revocation exposes a fixed reason selector, irreversible confirmation, revoked state/filtering, and no private-key upload surface | PASS |

## Exact-Head Evidence

- Exact revision: `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1`.
- GitHub Actions run: [`31515281011`](https://github.com/Hinln/xs-nexus/actions/runs/31515281011), all nine jobs passed.
- Dedicated job: [`93858773221`](https://github.com/Hinln/xs-nexus/actions/runs/31515281011/job/93858773221), `update-supply-chain`.
- Artifact: `9110864186`, `update-supply-chain-evidence`.
- GitHub artifact digest and independently downloaded archive SHA-256: `9885bc0d7146d518d53d6de8a11a8a7702d1816b5a49378dc13a797b85be2376`.
- Downloaded evidence: `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate18-ci-b8cd49c`.
- Downloaded `SHA256SUMS` SHA-256: `b43c39434453c8d30bea945faa1ff14005430a2432581d04a6a3596285469c66`.
- All eight payload hashes independently match the downloaded files.
- Downloaded artifact no-value secret scan: zero findings and zero incomplete surfaces.
- The artifact summary binds Linux `6.17.0-1022-azure`, Rust `1.94.0`, the exact revision, and `status=PASS`.

### Inner Payload Hashes

| Payload | SHA-256 |
|---|---|
| `agent-update.log` | `6c6df58a2c0341919eca8a9cd1b58d284f8f8d272c803a0283b13effaa6bafc4` |
| `controller-revocation.log` | `52acd55b2708430f17daf20ec6c14affe203808289b5c4a932fbd18ec0d48b87` |
| `core-contract.log` | `2f976e9afff3325b870bc3037673a9d052c89be25d354027843734db8728a55a` |
| `installer-lifecycle.log` | `0f44c9098ad13e622d2b8f615a732923ce104255af915c1382129ffc0b422d16` |
| `one-click.log` | `377f4925aca6ff4b0abda59fcda5552e8f9a98d2565242fa26dc93d064469dc8` |
| `scenario-matrix.tsv` | `6ef97842eb9d1671265b8b20a65ba548fd2a6b00b801e9d77145e3368028a4b0` |
| `source-shellcheck.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `summary.txt` | `3c0c2cebe4a13a0a5538b679d919fa0203dc34473837c81e79afb6091db81cf1` |

## Retained Failure Chain

| Run | Revision | Dedicated result or full-run blocker | Artifact |
|---|---|---|---|
| `31510934548` | `bc59dfbe78e78b283d9dbf8e43be2e5d5333ee6d` | ShellCheck `SC2071` rejected lexical numeric comparisons | `9109033398` |
| `31511341568` | `1be12f320f190b967762ed552b8d4f2a4425bec1` | Archive-hash negative fixture also changed `source_commit` | `9109197337` |
| `31511912254` | `7e9149399be2bb22dbe334ba334a2a4d2c17ed85` | Installer lifecycle exited at a hosted route assertion before diagnostics were added | `9109507310` |
| `31512364301` | `73ae92f9f915a14cabd01130563350f1dde8c62d` | Diagnostics identified an exact-text `iproute2` output assumption | `9109688746` |
| `31512767254` | `e842ca481b1b8621109d98ba93898e3a73e69d03` | Revoked rollback still executed the current installed binary before target rejection | `9109857831` |
| `31513216122` | `9fb7051630bae50a07a1d0c527f7da7a7491c99b` | Gate 18 passed; full run exposed a one-shot Relay Direct-convergence race | `9110017348` |
| `31513788232` | `c257bb7cd26767ca1645d17fe9b1e41ae2867bc4` | Gate 18 and Relay passed; strict baseline Clippy rejected a 101-line Controller function | `9110258489` |
| `31514717354` | `1f114d9030b4a8f7d57bc76c4961ba1045af261e` | Extracted SQLx helper used the transaction wrapper instead of its connection executor | `9110599058` |

Runs with a definitive failure were retained and their remaining superseded jobs were cancelled only after the failure artifact and logs existed; every job was rerun at the final exact revision. No failing test, ShellCheck diagnostic, Clippy warning, package count, timeout, route assertion, or security condition was skipped, ignored, allowed to fail, or weakened.

## Security and Production Boundary

- Controller withdrawal narrows the online scheduling window, while the local ledger is the final offline execution boundary. This dual mechanism is recorded in ADR-096.
- The repository does not provide an authenticated production transport or human ceremony for publishing the ledger and trusted key bundle. A compromised online control plane or stale host cannot be declared covered until that external process exists and is exercised.
- Hosted x86_64, test keys, namespace routes, tmpfs ENOSPC, and a local ledger do not prove real arm64/NAS, Windows, public artifact storage, CDN, production host upgrade, or operator recovery behavior.
- No production host, image, service, credential, DNS/CDN setting, firewall, 1Panel resource, Docker network, route, or database was changed by this Gate 18 work.
- Production continues to run the older revision. The current branch is not merged, tagged, formally signed, or approved for deployment.
