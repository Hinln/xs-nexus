# Linux Agent installation and lifecycle

XS Nexus Linux releases are distributed as a target-specific archive, a signed release manifest,
and a detached Ed25519 signature. The signing private key is an offline release input and must not
be copied to a Controller, Relay, Agent, package, or source checkout.

## Requirements

- Linux with systemd, `/dev/net/tun`, nftables, and an available `CAP_NET_ADMIN` capability;
- `bash`, GNU `tar`, `gzip`, `sha256sum`, `openssl`, `flock`, and standard account tools;
- a release archive, its matching `.manifest` and `.manifest.sig`, and the trusted release public
  key obtained through a separate authenticated channel.

The installer accepts only `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` packages that
match the current host. It verifies the detached manifest signature before parsing the manifest,
then verifies archive name, size, SHA-256, member allowlist, member types, and every payload hash.
The first installation pins the release public key at `/etc/xs-nexus/release-public-key.pem`;
later upgrades reject a different key.

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
