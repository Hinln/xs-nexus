# Linux Agent installation and lifecycle

XS Nexus Linux releases are distributed as a target-specific archive, a signed release manifest,
and a detached Ed25519 signature. The signing private key is an offline release input and must not
be copied to a Controller, Relay, Agent, package, or source checkout.

Manifest schema 2 binds the semantic version to the exact lowercase Git commit, source-date epoch,
`XSP/1` protocol version, target architecture, archive size, and archive SHA-256. The installer also
requires `xs-agent --version` and `xs --version` to report the same version, commit, and protocol
before activating the release.

## Requirements

- Linux with systemd, `/dev/net/tun`, nftables, and an available `CAP_NET_ADMIN` capability;
- `bash`, GNU `tar`, `gzip`, `sha256sum`, `openssl`, `flock`, and standard account tools;
- a release archive, its matching `.manifest` and `.manifest.sig`, and the trusted release public
  key obtained through a separate authenticated channel.

The production bootstrap supports glibc-based x86_64 and aarch64 Linux hosts running systemd. It
installs missing command-line prerequisites with apt, dnf, yum, zypper, or pacman, but it does not
support Alpine/musl, containers without systemd, or Windows.

## Production one-click enrollment

Create a one-time Enrollment Token in the Console, then run this command on the Linux device:

```bash
wget -qO- https://vpn.qinwen.co/install | sudo bash
```

The Controller URL, stable release URL, interface name, MTU, update channel, state paths, and node
name policy are fixed by the bootstrap. The node name is derived from the host name. The only
interactive value is the Enrollment Token, which is read with terminal echo disabled directly from
`/dev/tty`; it is not placed in the command line, shell history, process arguments, Controller
download logs, or installer output.

Before executing any downloaded installer, the bootstrap:

1. selects the exact x86_64 or aarch64 release;
2. verifies the downloaded Ed25519 public key against the fingerprint embedded in the bootstrap;
3. verifies the detached manifest signature;
4. enforces the exact version, source commit, source-date epoch, protocol, platform, architecture,
   target, archive name, size, and SHA-256;
5. checks the archive member allowlist, member types, and every payload hash;
6. writes the token only to a mode `0600` temporary file and removes the entire private staging
   directory on success, failure, signal, or enrollment rejection.

Running the command again on an enrolled node preserves its configuration, identity, signed state,
and virtual IP and applies only the verified release lifecycle. A pre-existing unrecognized config,
state path, pinned release key, unsupported architecture, unavailable TUN device, or non-systemd
host fails closed.

The installer accepts only `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` packages that
match the current host. It verifies the detached manifest signature before parsing the manifest,
then verifies archive name, size, SHA-256, member allowlist, member types, and every payload hash.
The first installation pins the release public key at `/etc/xs-nexus/release-public-key.pem`;
later upgrades reject a different key.

New external installs require manifest schema 2. The lifecycle verifier can still validate an
already-installed schema 1 release during rollback, but it will not accept schema 1 as a new
external package.

Release maintainers build both packages with an offline signing-key path:

```bash
make linux-packages \
  RELEASE_SIGNING_KEY=/offline/release-private-key.pem \
  RELEASE_OUTPUT=artifacts/release
```

The x86_64 build uses the installed Rust standard library. On the current Ubuntu build host, the
aarch64 build uses the distribution's matching `rust-src`, `gcc-aarch64-linux-gnu`, and
`libc6-dev-arm64-cross`; Cargo builds the standard library for the target and links with
`aarch64-linux-gnu-gcc`. This is a build-host procedure only and adds no runtime dependency.

## Install or upgrade

The following manual workflow remains available to release maintainers and recovery operators.

```bash
sudo ./installers/linux/xs-nexus-installer.sh install \
  --archive ./xs-nexus-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --manifest ./xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest \
  --signature ./xs-nexus-0.1.0-x86_64-unknown-linux-gnu.manifest.sig \
  --public-key /secure/release-public-key.pem
```

For first enrollment, prepare a restricted Agent JSON configuration and one-time token file. Pass
their paths with `--config` and `--enrollment-token-file`. The token value is never passed on the
command line and the installer's private staging copy is removed after enrollment.

Releases are installed under `/usr/local/lib/xs-nexus/versions/`. The `current` symlink is switched
atomically. Existing `/etc/xs-nexus/agent.json`, `/var/lib/xs-nexus/identity.key`, and signed node
state are not replaced during upgrade. A lower external version is rejected; an operator may only
roll back to a previously installed, still signature- and hash-valid release.

## Rollback and status

```bash
sudo ./installers/linux/xs-nexus-installer.sh rollback
sudo ./installers/linux/xs-nexus-installer.sh rollback --version 0.1.0
sudo ./installers/linux/xs-nexus-installer.sh status
sudo xs status
```

An upgrade captures the active service state and previous unit, stops the Agent, switches the
release, reloads systemd, and validates the new service. Any activation failure restores the old
release and unit and restarts the previously active service. `status` reports paths, versions, and
presence flags only; it does not print identity, credential, token, or configuration contents.

## Uninstall

```bash
sudo ./installers/linux/xs-nexus-installer.sh uninstall
sudo ./installers/linux/xs-nexus-installer.sh uninstall --purge
```

Uninstall first stops the Agent and invokes its trusted-state cleanup command. Cleanup refuses an
active project interface, an untrusted manifest, or state that does not match the local identity.
If cleanup fails, uninstall aborts and restores a previously active service. The default preserves
configuration, identity, signed state, diagnostics, and the pinned release key for safe reinstallation.
`--purge` additionally removes those preserved project directories and an installer-created service
account, after network cleanup succeeds.

## Recovery

Before manual repair, preserve `readlink /usr/local/lib/xs-nexus/current`, the config and state file
metadata, `ip -details link`, `ip route`, `ip rule`, and `nft list ruleset`. Never delete unrelated
routes, nftables tables, interfaces, Docker networks, or 1Panel resources. The Agent only cleans the
interface and gateway resources bound to its validated local plan and recovery manifest.

## Controller-managed staged updates

When the Controller has imported a valid offline-signed release and an eligible rollout policy,
the Agent downloads the exact HTTPS archive into a private staging directory, independently checks
the pinned release key and archive metadata, and publishes an atomic update request. The installed
`xs-agent-update.path` then invokes the network-disabled root helper, which copies the archive with
link protection, verifies it again, and calls this same installer transaction. The root helper never
trusts the Controller directive or the unprivileged staging result by itself.

The Controller-assigned channel is carried inside the signed node configuration. New installations
default to `stable`; an older signed configuration without the field falls back to the local Agent
setting. Operators change channels and rollout policies in the Console with optimistic version or
generation checks. See `docs/UPDATE_SYSTEM.md` for the full trust boundary and decision rules.
