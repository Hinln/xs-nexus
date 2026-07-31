# Runtime image supply-chain evidence

`scripts/generate-image-sbom.py` inspects exact local runtime image IDs. It does not resolve a mutable tag after recording the image: each manifest subject is bound to Docker's `sha256:` image ID and must carry the exact requested `org.opencontainers.image.revision` label.

For Debian images, the generator parses the installed dpkg database and preserves package copyright files plus `/usr/share/common-licenses`. For Alpine images, it parses the installed apk database and records every declared package license. Generated dot-prefixed apk dependency metapackages may legitimately have no license field; they remain in the SBOM as virtual packages without an invented license. Available `/usr/share/licenses` files are preserved. Every image also receives a deterministic package-manager license declaration file.

The output contains:

- `manifest.json`: image IDs, package counts, license material hashes, Dockerfile hashes and declared base image references;
- `xs-nexus-images.cdx.json`: combined CycloneDX 1.6 OS package SBOM;
- `xs-nexus-images.provenance.json`: an in-toto/SLSA provenance statement binding the Git revision, Dockerfiles and final image IDs;
- `licenses/`: exact license/copyright material available in each runtime root filesystem.

Run `make validate-image-supply-chain` from a clean worktree. The validator builds Controller, Relay, Console and db-tools images for the current commit, generates the output twice, requires byte-identical results, rejects a revision-label mismatch, and proves that containers, Docker networks, `1panel-network`, the default route and nftables remain unchanged.

This evidence does not include a vulnerability database result and does not prove that every distribution license expression has a complete corresponding full text in the minimal runtime image. A digest-bound vulnerability report and any missing license texts remain mandatory before Release Candidate.

The first complete current-commit run is preserved at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T180211Z`. It contains 344 installed OS package components and 247 exact rootfs license/declaration materials across the four images.
