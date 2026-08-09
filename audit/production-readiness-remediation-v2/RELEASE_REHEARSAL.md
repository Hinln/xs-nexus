# Clean Release And Deployment Rehearsal

## Current Status

`FAIL`. Existing CI and isolated lifecycle evidence do not replace an independent clean-environment release rehearsal or the required real upgrade/rollback from production revision `ff9551d3`.

## Required Clean Rehearsal

1. Fresh clone/checkout of the exact signed RC tag.
2. Inject secrets only from approved repository-external stores.
3. No-cache reproducible build and signed release manifest.
4. Database backup, migration, deployment and health checks.
5. Create a network and enroll a Linux node.
6. Verify Direct, Relay, ACL, and subnet simulation.
7. Verify signed update, interrupted/bad update rejection, rollback, backup, restore, uninstall and clean reinstall.
8. Prove no change to unrelated Docker/1Panel resources or `1panel-network`.

## Required Production Exercise

Perform `ff9551d3 -> signed RC`, verify data, identity, ACL, routes and all services, then perform at least one documented real rollback. Database compatibility must be explicitly proved in both directions; a destructive migration without a tested rollback blocks the exercise.

All failures are retained. This gate passes only after an independent operator can follow the documented procedure without hidden local state.
