# Runtime image supply-chain evidence

`scripts/generate-image-sbom.py` inspects exact local runtime image IDs. It does not resolve a mutable tag after recording the image: each manifest subject is bound to Docker's `sha256:` image ID and must carry the exact requested `org.opencontainers.image.revision` label.

For Debian images, the generator parses the installed dpkg database and preserves package copyright files plus `/usr/share/common-licenses`. For Alpine images, it parses the installed apk database and records every declared package license. Generated dot-prefixed apk dependency metapackages may legitimately have no license field; they remain in the SBOM as virtual packages without an invented license. Available `/usr/share/licenses` files are preserved. Every image also receives a deterministic package-manager license declaration file.

The output contains:

- `manifest.json`: image IDs, package counts, license material hashes, Dockerfile hashes and declared base image references;
- `xs-nexus-images.cdx.json`: combined CycloneDX 1.6 OS package SBOM;
- `xs-nexus-images.provenance.json`: an in-toto/SLSA provenance statement binding the Git revision, Dockerfiles and final image IDs;
- `licenses/`: exact license/copyright material available in each runtime root filesystem.

Run `make validate-image-supply-chain` from a clean worktree. The validator builds Controller, Relay, Console and db-tools images for the current commit, generates the output twice, requires byte-identical results, rejects a revision-label mismatch, and proves that containers, Docker networks, `1panel-network`, the default route and nftables remain unchanged.

Run `make scan-image-vulnerabilities EVIDENCE_DIR=/absolute/evidence/path` to download the pinned Grype release into a temporary directory, verify its official archive SHA-256, update an isolated vulnerability database, re-check every local image ID against the SBOM manifest, and write one JSON report per image plus a deterministic summary. Scanner findings are evidence, not an automatic waiver: High/Critical results require explicit remediation or disposition.

The rootfs evidence alone does not prove that every distribution license expression has a complete corresponding full text in the minimal runtime image. Any missing license texts remain mandatory before Release Candidate.

The first complete current-commit run is preserved at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T180211Z`. It contains 344 installed OS package components and 247 exact rootfs license/declaration materials across the four images.

The remediation run for commit `9b727d424e9968b0dcec2c7f8553ea0d79057961` is preserved at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T182323Z`. Controller and Relay no longer install curl solely for health checks; their binaries perform a bounded fixed-loopback readiness probe. Console and db-tools apply available Alpine upgrades while building. Compared with the preceding scan, total Critical findings fell from 44 to 22 and High findings from 115 to 45. Grype reports no remaining finding with a currently available fix, but all residual Critical/High findings still require explicit disposition before Release Candidate.

The minimized runtime run for commit `2eefae940c99037024cbd351bc664351227976d4` is preserved at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T183424Z`. Controller and Relay use a digest-pinned distroless Debian 12 runtime; the generator parses its bounded `/var/lib/dpkg/status.d` metadata without executing image content. Console removes curl after applying Alpine updates. The resulting scan contains 2 Critical and 6 High findings, down from the initial 44 and 115, with no finding that currently advertises an available fix. Residual glibc and TIFF findings remain open for explicit disposition.

The follow-up run for commit `ca7d2c2e026385b6e8f4432de7942b672be71d3a` is preserved at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T190639Z`. Console removes its unused NGINX image-filter module and the resulting TIFF package chain. The scan contains 2 Critical and 4 High findings, all in glibc for Controller and Relay, and no currently fixable finding. `make verify-image-vulnerability-disposition EVIDENCE_DIR=...` fails closed unless the exact remaining CVE/package/fix-state set is present and the exact runtime binaries do not import the affected APIs. This bounded disposition expires on 2026-08-31 and must be repeated earlier if an image, glibc, scanner result, or binary import changes.
