# Clean Release And Deployment Rehearsal

## Current Status

`FAIL`. The reproducible repository-side rehearsal is complete, but it used an ephemeral CI signing key and the same project automation on a GitHub-hosted runner. It is not an owner-controlled signed RC, an independent operator rehearsal, a fresh non-hosted server, or the required production upgrade and rollback.

## Repository-Side Rehearsal

Exact revision `795b1ea461a179958aed27e6935faba8f36e43ce` passed all 13 jobs in GitHub Actions run `31658778589`. Dedicated job `94318968293` and artifact `9165661918` (`clean-release-rehearsal-evidence`, GitHub digest `sha256:08a9a05c09a827b8d4cfc35c0eb0d4a5ae086be125b24e0c17ac45b83f7e6ffa`) prove:

1. A full-history Git bundle and detached clean checkout exactly match the requested revision and pass `git fsck`.
2. An explicitly test-only ephemeral SSH-signed tag verifies, while evidence records `formal_signed_rc=false` and `independent_operator=false`.
3. All five runtime images reproduce across two no-cache OCI builds.
4. Repository-external generated database secrets and separate bootstrap/application roles are used without committing or uploading values.
5. Real namespace Direct ACL, Relay fallback/recovery, and subnet-router paths pass.
6. Signed update failure/rollback tests and two complete Docker deployment lifecycles pass from the clean checkout.
7. The temporary external `1panel-network`, PostgreSQL container, sentinel, volumes, namespaces, links, routes, rules, nftables, failed services, and Docker resources return to their exact baseline.

The artifact records 10 of 10 phases, 10 of 10 fixture invariants, and 10 of 10 final cleanup invariants as PASS. All 110 outer and 8 nested manifest entries independently verify and the repository secret scanner reports zero findings. `production_mutation=false`; no production resource was contacted or changed.

## Retained Failure Chain

| Run | Retained failure |
|---|---|
| `31641678475` | A shallow source checkout produced a disconnected Git bundle. |
| `31642240288` | `sudo` lost the Buildx context and the finalizer deleted its active checkout before scanning. |
| `31643179415` | PostgreSQL used a host port and anonymous volume, leaving Docker state. |
| `31644737926` | First loopback publication initialized Docker's raw table after the baseline. |
| `31646007655` | `pg_isready` observed PostgreSQL's temporary initialization server; the separate performance job also retained Direct path-transition outliers. |
| `31647482539` | The 10-phase submatrix passed, but a GitHub runner PCI interface hot-plugged after the original baseline, so final link equality failed. |
| `31654905139` | The receiver authenticated the exact primary Relay with legitimate `relay_failover`, while the old harness allowed only `relay_fallback`. |
| `31655990346` | The separate soak calibration applied the new configuration but sampled before both Direct sessions recovered. |
| `31657413129` | The separate performance matrix began formal Direct sampling while the single warm-up batch still exceeded the p95 envelope. |

The final implementation waits for PostgreSQL's completed-init marker and a real SQL query, measures 100 steady-state RTT samples only after a bounded, evidence-bearing ten-packet Direct attempt meets the unchanged p95 envelope, initializes Docker firewall state before the baseline, and permits only an added kernel hardware-backed interface with no address or route. At most 12 warm-up attempts are allowed; exhaustion fails and every attempt remains in the artifact. Virtual-interface additions, interface removals, or any route/rule/nftables/Docker drift still fail closed. The passing run required no hot-plug exception; link inventories were byte-identical.

## Required Formal Rehearsal

1. Merge the reviewed release to `main` and verify an owner-controlled signed RC tag and bundle through an authenticated release key.
2. Use an independent operator on a fresh, non-hosted, identity-verified server with approved repository-external production secrets.
3. Build or import the exact signed artifacts, then execute backup, migration, installation, enrollment, Direct, Relay, ACL, subnet, update, restore, uninstall, and clean reinstall.
4. Prove no change to unrelated Docker/1Panel resources or the pre-existing production `1panel-network`.
5. Upgrade production from the currently deployed revision to the exact signed RC, independently verify data and runtime identity, execute a documented rollback, then restore the approved release.

Historical `ff9551d3 -> 3d93656` deployment and four automatic rollbacks remain valid evidence for that historical revision, but cannot substitute for the current signed RC chain. Gate 25 can pass only after the formal exercise above is independently completed; until then the release decision remains `NO_GO`.
