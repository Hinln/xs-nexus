# Clean Release And Deployment Rehearsal

## Current Status

`FAIL`. The reproducible repository-side rehearsal is complete, but it used an ephemeral CI signing key and the same project automation on a GitHub-hosted runner. It is not an owner-controlled signed RC, an independent operator rehearsal, a fresh non-hosted server, or the required production upgrade and rollback.

## Repository-Side Rehearsal

Exact revision `6e63424298e491c0035e1d138de5d03b0ab83c27` passed all 13 jobs in GitHub Actions run `31649030446`. Dedicated job `94289075249` and artifact `9162246723` (`clean-release-rehearsal-evidence`, GitHub digest `sha256:d69d7ff0d24195f629a5032152b1c1851f0cab8d79048ae90ca00af5fa9659fe`) prove:

1. A full-history Git bundle and detached clean checkout exactly match the requested revision and pass `git fsck`.
2. An explicitly test-only ephemeral SSH-signed tag verifies, while evidence records `formal_signed_rc=false` and `independent_operator=false`.
3. All five runtime images reproduce across two no-cache OCI builds.
4. Repository-external generated database secrets and separate bootstrap/application roles are used without committing or uploading values.
5. Real namespace Direct ACL, Relay fallback/recovery, and subnet-router paths pass.
6. Signed update failure/rollback tests and two complete Docker deployment lifecycles pass from the clean checkout.
7. The temporary external `1panel-network`, PostgreSQL container, sentinel, volumes, namespaces, links, routes, rules, nftables, failed services, and Docker resources return to their exact baseline.

The artifact records 10 of 10 phases, 10 of 10 fixture invariants, and 10 of 10 final cleanup invariants as PASS. All 111 manifest entries independently verify and the repository secret scanner reports zero findings. `production_mutation=false`; no production resource was contacted or changed.

## Retained Failure Chain

| Run | Retained failure |
|---|---|
| `31641678475` | A shallow source checkout produced a disconnected Git bundle. |
| `31642240288` | `sudo` lost the Buildx context and the finalizer deleted its active checkout before scanning. |
| `31643179415` | PostgreSQL used a host port and anonymous volume, leaving Docker state. |
| `31644737926` | First loopback publication initialized Docker's raw table after the baseline. |
| `31646007655` | `pg_isready` observed PostgreSQL's temporary initialization server; the separate performance job also retained Direct path-transition outliers. |
| `31647482539` | The 10-phase submatrix passed, but a GitHub runner PCI interface hot-plugged after the original baseline, so final link equality failed. |

The final implementation waits for PostgreSQL's completed-init marker and a real SQL query, measures 100 steady-state RTT samples after a 10-packet Direct warm-up without changing thresholds, initializes Docker firewall state before the baseline, and permits only an added kernel hardware-backed interface with no address or route. Virtual-interface additions, interface removals, or any route/rule/nftables/Docker drift still fail closed. The passing run required no hot-plug exception; link inventories were byte-identical.

## Required Formal Rehearsal

1. Merge the reviewed release to `main` and verify an owner-controlled signed RC tag and bundle through an authenticated release key.
2. Use an independent operator on a fresh, non-hosted, identity-verified server with approved repository-external production secrets.
3. Build or import the exact signed artifacts, then execute backup, migration, installation, enrollment, Direct, Relay, ACL, subnet, update, restore, uninstall, and clean reinstall.
4. Prove no change to unrelated Docker/1Panel resources or the pre-existing production `1panel-network`.
5. Upgrade production from the currently deployed revision to the exact signed RC, independently verify data and runtime identity, execute a documented rollback, then restore the approved release.

Historical `ff9551d3 -> 3d93656` deployment and four automatic rollbacks remain valid evidence for that historical revision, but cannot substitute for the current signed RC chain. Gate 25 can pass only after the formal exercise above is independently completed; until then the release decision remains `NO_GO`.
