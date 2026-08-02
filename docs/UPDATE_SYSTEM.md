# Signed update and staged rollout system

Status: implemented for Linux and covered by unit, PostgreSQL, control-plane, installer, systemd,
Console E2E, and visual tests. Production release signing remains an operator gate.

## Trust boundary

- The Ed25519 release private key is offline input to the release builder. It must never enter the
  Controller, Console, Agent state directory, source checkout, or deployment secrets.
- The Controller receives only the raw 32-byte Ed25519 public key through
  `UPDATE_SIGNING_PUBLIC_KEY_PATH`. If the variable is absent, release import is unavailable and
  the Console reports the reason instead of pretending the capability exists.
- Each Agent pins the PEM release public key at `/etc/xs-nexus/release-public-key.pem`. The first
  signed install creates this pin; later installs and updates reject a different key.
- A Controller directive is scheduling input, not installation authority. The unprivileged Agent
  and the root update helper independently verify the release manifest, signature, target,
  archive name, size, SHA-256, and package contents.

## Release and policy model

A release is an immutable signed Linux manifest plus its exact HTTPS archive URL. The manifest is
the same strict nine-field format consumed by the Linux installer. Versions are canonical
`major.minor.patch`; supported targets are Linux `x86_64` and `aarch64`. Duplicate manifests,
signature failures, URL/file-name drift, non-HTTPS URLs, and attempts to mutate an existing
release fail closed.

Policies are keyed by network, channel, platform, and architecture. Channels are `stable`,
`testing`, and `development`. A policy contains the target release, optional minimum version,
rollout percentage in basis points, pause state, and optimistic generation. Evaluation rules are:

1. pause overrides every other rule;
2. a node at or above the target is never downgraded;
3. a node below the minimum version is required to update;
4. remaining nodes use a deterministic SHA-256 bucket over network and node identity;
5. generation conflicts return HTTP 409 rather than overwriting another operator's change.

The assigned node channel is part of the Controller-signed per-node configuration. Channel changes
use the network configuration version as an optimistic lock, publish a new signed configuration,
and append a redacted audit event. Older signed configurations without this optional field remain
readable; the Agent then uses its local `update_channel` only as a compatibility fallback.

## Agent report and directive flow

After control authentication and every synchronization interval, the Agent sends a domain-separated
Ed25519 identity signature over network ID, node ID, Agent version, platform, architecture, assigned
channel, update state, observed release, error code, and UTC generation time. A newly applied signed
configuration triggers an immediate report. The Controller rejects identity drift, channel drift,
invalid signatures, invalid platform/architecture pairs, malformed error codes, stale reports, and
non-monotonic report time.

Only `eligible` and `required` decisions produce directives. The Agent verifies that a directive
matches its signed assigned channel, local platform/architecture, current version, minimum-version
semantics, pinned release key, and strict HTTPS archive identity. Downloads have bounded connect and
overall timeouts and a 512 MiB hard limit. Partial staging is never published as ready.

## Privilege separation and activation

The ordinary Agent stages verified public materials under `/var/lib/xs-nexus/update-staging/` and
atomically writes `/var/lib/xs-nexus/update-request.json`. `xs-agent-update.path` observes only that
ready request and starts the oneshot `xs-agent-update.service`.

The root helper:

1. opens only the exact request and release directory;
2. rejects links and unsafe permissions, then copies the archive into a private root-owned runtime
   directory;
3. re-verifies manifest, signature, archive metadata, and content after the privileged copy;
4. invokes the installed `xs-nexus-installer.sh install` transaction;
5. relies on the existing atomic `current` symlink, health check, identity-preserving upgrade, and
   exact previous-release rollback.

The systemd unit has no network access and grants write access only to the necessary XS Nexus state,
configuration, systemd-unit, and version directories. Install, rollback, uninstall, and package
allowlists all include the update path/service units. Uninstall disables the path unit and preserves
the pinned key unless the operator explicitly purges project state.

## Console operations

The Updates page provides:

- signed manifest/signature import without any private-key upload field;
- release and policy lists with loading, empty, forbidden, and service-error states;
- explicit confirmation for rollout/pause changes;
- assigned-channel changes for active nodes with configuration-version conflict protection;
- signed Agent runtime state, version, architecture, release, error code, and report time;
- read-only rendering for auditors.

## Verification

```bash
cargo test -p xs-core --lib
cargo test -p xs-agent --lib --bins
cargo test -p xs-controller --lib --bins
cargo clippy -p xs-core -p xs-controller -p xs-agent --all-targets -- -D warnings
make test-controller-db
make test-agent-control
make test-linux-installer
make test-agent-systemd
npm run build --workspace @xs-nexus/console
npm run test:e2e --workspace @xs-nexus/console
npm run test:visual --workspace @xs-nexus/console
```

Production use additionally requires an approved offline signing ceremony, authenticated public-key
distribution, real release storage, operator key rotation/revocation procedures, and a signed RC
rollback exercise. Those external gates must not be inferred from development-key test evidence.
