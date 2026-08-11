SHELL := /usr/bin/env bash
.RECIPEPREFIX := >

.PHONY: setup fmt fmt-check lint build test test-unit test-integration test-controller-db test-controller-scale test-postgres-least-privilege test-production-postgres-policy test-protocol-throughput test-relay-throughput test-agent-rtt test-agent-control test-agent-systemd test-agent-data-plane test-agent-candidate-fallback test-agent-candidate-path test-agent-candidates test-agent-proactive-punch test-agent-nat-matrix test-agent-nat test-agent-relay test-agent-acl test-agent-subnet-route test-linux-installer test-linux-one-click test-update-supply-chain test-docker-deployment test-runtime-stability test-production-health test-console-real-e2e test-windows-xsnet-abi test-windows-xsnet-source test-windows-xsnet-installer test-windows-xsnet-vm-scripts test-windows-xsnet-compatibility test-windows-xsnet-transport test-windows-agent-ipc test-windows-agent-storage test-windows-agent-routing test-windows-agent-service test-host-hardening test-onepanel-boundary test-independent-implementation test-source-sbom test-image-sbom test-image-reproducibility test-release-provenance test-secret-surface-audit test-strict-tls-audit test-ci-actions test-protocol-fuzz-config source-sbom validate-image-supply-chain scan-image-vulnerabilities verify-image-vulnerability-disposition test-protocol-vectors test-protocol-fuzz test-spec test-network test-e2e test-visual security-check dependency-check linux-package-x86_64 linux-package-aarch64 linux-packages validate-m21 validate-m22 validate-m23 validate-m31 validate-m32 validate-m42 validate-m51 validate-m52 validate-m61-agent-session release clean

setup:
>npm ci

fmt:
>cargo fmt --all

fmt-check:
>cargo fmt --all -- --check

lint:
>cargo clippy --workspace --all-targets -- -D warnings
>npm run lint

build:
>cargo build --workspace
>npm run build

test: test-unit test-integration test-controller-db test-agent-control test-protocol-vectors test-release-provenance test-spec

test-unit:
>cargo test --workspace --lib --bins
>npm test

test-integration:
>./scripts/test-integration.sh

test-controller-db:
>./scripts/test-controller-db.sh

test-controller-scale:
>./scripts/test-controller-scale.sh

test-postgres-least-privilege:
>./scripts/test-postgres-least-privilege.sh

test-production-postgres-policy:
>python3 scripts/test-audit-production-postgres.py

test-protocol-throughput:
>./scripts/test-protocol-throughput.sh

test-relay-throughput:
>./scripts/test-relay-throughput.sh

test-agent-rtt:
>./scripts/test-agent-rtt.sh

test-agent-control:
>./scripts/test-agent-control.sh

test-agent-systemd:
>./scripts/test-agent-systemd.sh

test-agent-data-plane:
>./scripts/test-agent-data-plane.sh

test-agent-candidate-fallback:
>./scripts/test-agent-candidate-fallback.sh

test-agent-candidate-path:
>./scripts/test-agent-candidate-path.sh

test-agent-candidates: test-agent-candidate-fallback test-agent-candidate-path

test-agent-proactive-punch:
>./scripts/test-agent-proactive-punch.sh

test-agent-nat-matrix:
>./scripts/test-agent-nat-matrix.sh

test-agent-nat: test-agent-proactive-punch test-agent-nat-matrix

test-agent-relay:
>./scripts/test-agent-relay.sh

test-agent-acl:
>./scripts/test-agent-acl.sh

test-agent-subnet-route:
>./scripts/test-agent-subnet-route.sh

test-linux-installer:
>./scripts/test-linux-installer.sh

test-linux-one-click:
>./scripts/test-linux-one-click.sh

test-update-supply-chain:
>./scripts/test-update-supply-chain.sh

test-docker-deployment:
>./scripts/test-docker-deployment.sh

test-runtime-stability:
>test -n "$(ENV_FILE)"
>./scripts/test-runtime-stability.sh "$(ENV_FILE)" $(if $(EVIDENCE_DIR),"$(EVIDENCE_DIR)")

test-production-health:
>./scripts/test-production-health.sh

test-console-real-e2e:
>test -n "$(EVIDENCE_DIR)"
>./scripts/test-console-real-e2e.sh "$(EVIDENCE_DIR)"

test-windows-xsnet-abi:
>./scripts/test-windows-xsnet-abi.sh

test-windows-xsnet-source:
>./scripts/validate-windows-xsnet-source.py

test-windows-xsnet-installer:
>./scripts/validate-windows-xsnet-installer.py

test-windows-xsnet-vm-scripts:
>./scripts/validate-windows-xsnet-vm.py

test-windows-xsnet-compatibility:
>./scripts/validate-windows-xsnet-compatibility.py

test-windows-xsnet-transport:
>./scripts/test-windows-xsnet-transport.sh

test-windows-agent-ipc:
>./scripts/test-windows-agent-ipc.sh

test-windows-agent-storage:
>./scripts/test-windows-agent-storage.sh

test-windows-agent-routing:
>./scripts/test-windows-agent-routing.sh

test-windows-agent-service:
>./scripts/test-windows-agent-service.sh

test-host-hardening:
>./scripts/test-host-hardening.sh

test-onepanel-boundary:
>python3 scripts/test-onepanel-boundary.py
>python3 scripts/validate-onepanel-boundary.py

test-independent-implementation:
>./scripts/validate-independent-implementation.py

test-source-sbom:
>./scripts/test-source-sbom.py

test-image-sbom:
>./scripts/test-image-sbom.py

test-image-reproducibility:
>test -n "$(EVIDENCE_DIR)"
>./scripts/test-image-reproducibility.sh "$(EVIDENCE_DIR)"

test-release-provenance:
>./scripts/test-release-provenance.sh

validate-image-supply-chain:
>./scripts/validate-image-supply-chain.sh

scan-image-vulnerabilities:
>test -n "$(EVIDENCE_DIR)"
>./scripts/scan-image-vulnerabilities.sh "$(EVIDENCE_DIR)"

verify-image-vulnerability-disposition:
>test -n "$(EVIDENCE_DIR)"
>./scripts/verify-image-vulnerability-disposition.sh "$(EVIDENCE_DIR)"

source-sbom:
>test -n "$(SBOM_OUTPUT)"
>test -n "$(SOURCE_DATE_EPOCH)"
>./scripts/generate-source-sbom.py --output-dir "$(SBOM_OUTPUT)" --source-date-epoch "$(SOURCE_DATE_EPOCH)"

test-protocol-vectors:
>./scripts/test-protocol-vectors.sh

test-protocol-fuzz:
>test -n "$(EVIDENCE_DIR)"
>./scripts/run-protocol-fuzz.sh "$(EVIDENCE_DIR)"

test-strict-tls-audit:
>python3 scripts/test-audit-strict-tls.py

test-spec: test-strict-tls-audit test-production-postgres-policy
>python3 scripts/validate-m02.py
>python3 scripts/validate-plan-status.py
>python3 scripts/validate-public-edge.py

test-network:
>./scripts/test-network-capabilities.sh
>./scripts/test-agent-network.sh
>./scripts/test-agent-systemd.sh

test-e2e:
>npm run test:e2e

test-visual:
>npm run test:visual

security-check:
>python3 scripts/test-secret-scanner.py
>python3 scripts/test-secret-surface-audit.py
>python3 scripts/check-secrets.py --root . --reference-env /etc/xs-nexus/controller.env
>python3 scripts/test-ci-action-validator.py
>python3 scripts/validate-ci-actions.py
>python3 scripts/test-protocol-fuzz-config.py
>python3 scripts/validate-protocol-fuzz.py
>./scripts/test-host-hardening.sh
>python3 scripts/test-onepanel-boundary.py
>python3 scripts/validate-onepanel-boundary.py

test-secret-surface-audit:
>python3 scripts/test-secret-surface-audit.py

test-ci-actions:
>python3 scripts/test-ci-action-validator.py
>python3 scripts/validate-ci-actions.py

test-protocol-fuzz-config:
>python3 scripts/test-protocol-fuzz-config.py
>python3 scripts/validate-protocol-fuzz.py

dependency-check:
>./scripts/check-rust-advisories.sh

linux-package-x86_64:
>test -n "$(RELEASE_SIGNING_KEY)"
>./installers/linux/build-package.sh --target x86_64-unknown-linux-gnu --signing-key "$(RELEASE_SIGNING_KEY)" --output "$(or $(RELEASE_OUTPUT),artifacts/release)"

linux-package-aarch64:
>test -n "$(RELEASE_SIGNING_KEY)"
>./installers/linux/build-package.sh --target aarch64-unknown-linux-gnu --signing-key "$(RELEASE_SIGNING_KEY)" --output "$(or $(RELEASE_OUTPUT),artifacts/release)"

linux-packages: linux-package-x86_64 linux-package-aarch64

validate-m21:
>./scripts/validate-m21.sh

validate-m22:
>./scripts/validate-m22.sh

validate-m23:
>./scripts/validate-m23.sh

validate-m31:
>./scripts/validate-m31.sh

validate-m32:
>./scripts/validate-m32.sh

validate-m42:
>./scripts/validate-m42.sh

validate-m51:
>./scripts/validate-m51.sh

validate-m52:
>./scripts/validate-m52.sh

validate-m61-agent-session:
>./scripts/validate-m61-agent-session.sh

release:
>test -n "$(RELEASE_INVENTORY)"
>test -n "$(RELEASE_SIGNING_KEY)"
>test -n "$(RELEASE_OUTPUT)"
>./scripts/verify-release-source.py --inventory "$(RELEASE_INVENTORY)" --repository .
>./scripts/generate-release-provenance.py --inventory "$(RELEASE_INVENTORY)" --output-dir "$(RELEASE_OUTPUT)"
>./scripts/sign-release-provenance.sh --bundle "$(RELEASE_OUTPUT)" --signing-key "$(RELEASE_SIGNING_KEY)"

clean:
>cargo clean
>rm -rf apps/console/dist
