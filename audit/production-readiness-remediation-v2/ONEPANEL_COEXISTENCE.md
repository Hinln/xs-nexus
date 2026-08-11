# Gate 15 1Panel Coexistence

## Decision

`PASS` for Hard Gate 15 at candidate revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6`.

This result is limited to the 1Panel/Docker coexistence boundary. It does not approve deployment of the current branch, does not change the running production revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14`, and does not change the overall `NO_GO` decision.

## Requirement Matrix

| Formal requirement | Evidence | Result |
|---|---|---|
| `1panel-network` is not project-managed | Both Compose files declare only `external: true` with exact name `1panel-network`; the current-source validator rejects driver, IPAM, subnet, attachable, privilege, host networking, Docker socket, and any additional managed network | PASS |
| Subnet remains `172.18.0.0/16` | The hosted runtime fixture creates and verifies that exact external-network shape; production Gate 14 and Gate 16 evidence retain the same production network ID and subnet before/after maintenance, reboot, deployment, and rollback | PASS |
| Compose only references the external network | Source parsing and fully interpolated `docker compose config --format json` validate all seven application/edge services and the single external network | PASS |
| Project lifecycle cannot delete the network | Static lifecycle validation rejects network create/remove/connect/disconnect and global Compose down; the hosted fixture proves project-scoped `compose down --remove-orphans` preserves the exact external network ID | PASS |
| Project lifecycle cannot prune unknown resources | Static validation rejects system/container/image/volume/builder/network prune and unscoped remove commands; a non-Compose sentinel container survives project down and Docker daemon restart with the same container ID | PASS |
| Docker restart | Dedicated GitHub-hosted job restarts the real Docker daemon and proves the external network ID, attached sentinel ID/running state, network inventory, and host default route are preserved | PASS |
| Host reboot | Gate 14 protected production maintenance performed a real Boot-ID-changing host reboot with automatic old-kernel fallback and independent internal/external post-reboot verification | PASS |
| Project restart | Gate 16 production deployment and four automatic rollback attempts repeatedly recreated/restarted the project stack and preserved 1Panel, OpenResty, SSH, routes/rules, non-project nftables, and the external network | PASS |
| 1Panel restart | The Gate 14 host reboot restarted the host Docker/1Panel runtime; Boot-ID-bound verification proved the management tunnel, OpenResty, protected containers, and 1Panel operation after boot | PASS |
| Project upgrade | Gate 16 upgraded production from `ff9551d322067c934d2ac7d55a62af8896660bb3` to `3d93656cc9ec3ea35d58e453118154b25bcc4e14`; four failed verifier attempts automatically rolled back and the fifth passed independent-session verification | PASS |
| Lifecycle does not affect 1Panel, existing websites, database, Docker network, or SSH | Gate 14 and Gate 16 production evidence jointly preserve unrelated OpenResty/website service, database identities and exposure boundary, key-only SSH, protected container identities, default route, IP rules, non-project nftables, and exact `1panel-network` identity/subnet | PASS |

## Current-Revision Source and Runtime Evidence

- Exact revision: `8a9174866ebdf4ff76e7d987e006acb64312e3b6`.
- GitHub Actions run: `31504402285`, all eight jobs passed.
- Dedicated job: `93822197946`, `onepanel-coexistence`.
- Artifact: `9106406005`, `onepanel-coexistence-evidence`.
- GitHub archive digest and independently downloaded archive SHA-256: `93e174890d19264398090dcb891ab434a3839d769c4f0f14854982245678a28f`.
- Downloaded artifact: `C:\Users\panyo\Documents\Codex\2026-07-29\yue\gate15-ci-8a91748`.
- Downloaded `SHA256SUMS` SHA-256: `46c23da39ac14ecf545806f4f02cdea276d25cadee1ba931fe973f469f058191`.
- Inner payload hashes:
  - `runtime.log`: `135c4c74d36f6c76b93de1e7f88238813923f20ec17382734efef7886bf7ab34`.
  - `source-tests.log`: `2a941a94ee3218091e52e18917643949e7773211dfe4d84f753c80d5fe109aae`.
  - `source-validation.log`: `be0aeb527aad5fc49923dd4461c7b8802858a5a87d12f1acba18b40c54ad2934`.
  - `summary.txt`: `8f478ab28656fcc99b23dc1e3c345578fb8e6f1a9c9fbc70533ae655413c22da`.
- The downloaded artifact passed the repository no-value secret scanner with zero findings.
- The runtime fixture is CI-only and fails closed unless `GITHUB_ACTIONS=true`, `XS_ONEPANEL_CI_FIXTURE=1`, and the exact network name does not already exist. It did not run against production or any actual 1Panel network.

## Production Evidence

- Host hardening and protected service restart: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z`.
- Protected package upgrade, real host reboot, Docker/1Panel/OpenResty/SSH regression, and bounded cleanup: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z`.
- Production project upgrade, four automatic rollbacks, independent new-session verification, and finalization: `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`.
- Initial production deployment and 1Panel boundary evidence: `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z` and `/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z`.

The current and deployed revisions share identical blobs for `deploy/docker/edge.compose.yaml`, `deploy/docker/xs-nexus-stack.sh`, and `deploy/docker/xs-nexus-edge.sh`. The application Compose difference since the deployed revision adds only three Relay resource-limit environment variables; the service-to-network bindings and external-network declaration are unchanged and are revalidated at the exact current revision.

## Retained Failure Chain

| Run | Revision | Failure | Artifact |
|---|---|---|---|
| `31364684902` | `757a3fc6863ced789a34f9b5544f92318c3d7a4a` | Uninterpolated host IP was rejected by Compose rendering | `9053670838`, digest `e877be949ed565d7e019feb5cb2ae0eeb8a0e5f483eee98c82759b1f20f2cea2` |
| `31366046340` | `d2c133d007cdc3fddf1a5e2c47b9277f46fdbf17` | Profiled services were omitted from the rendered service set | `9054165902`, digest `ad094ffea38ea015f9487b2254261b7a7cf27ef86cd7b24349da6b3f5e8df539` |
| `31367632436` | `97b9b655430e5c9b0e3ff68c219765c211347482` | Runtime assertions passed but cleanup returned nonzero without sufficient diagnostics | `9054770008`, digest `4ece311ca6f5fc5f4b388c1ffae3117a60b5f9a009d9063afbc0391e54bb06fa` |
| `31504209932` | `daf8a7db411853fd41b91234d71b2674804a3619` | Diagnostics proved the hosted daemon legitimately recreated only the built-in `bridge` network ID; the target external network ID remained exact | `9106338810`, digest `5a9db126f9af834420e1a5d0c54580e8cec825301652ab259f5b5e393baee1e1` |

No failure was deleted, rerun as skipped, converted to an allowed failure, or hidden by lowering assertions. The final test compares the stable global network inventory while retaining exact identity checks for the external network and sentinel, then emits final PASS only after cleanup and route/inventory verification succeed.

## Boundary

Gate 15 is `PASS`; Gate 01, Gate 02, Gate 13, Gate 18, Gate 20, Gate 22, Gate 23, Gate 24, Gate 25, and external Windows/NAS/WAN/offsite/audit gates remain unchanged. The current branch is not a signed RC and is not approved for production deployment.
