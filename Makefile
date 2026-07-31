SHELL := /usr/bin/env bash
.RECIPEPREFIX := >

.PHONY: setup fmt fmt-check lint build test test-unit test-integration test-controller-db test-agent-control test-agent-systemd test-agent-data-plane test-agent-candidate-fallback test-agent-candidate-path test-agent-candidates test-agent-proactive-punch test-agent-nat-matrix test-agent-nat test-agent-relay test-agent-acl test-agent-subnet-route test-linux-installer test-docker-deployment test-windows-xsnet-abi test-windows-xsnet-source test-windows-xsnet-installer test-windows-xsnet-vm-scripts test-protocol-vectors test-spec test-network test-e2e test-visual security-check linux-package-x86_64 linux-package-aarch64 linux-packages validate-m21 validate-m22 validate-m23 validate-m31 validate-m32 validate-m42 validate-m51 validate-m52 release clean

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

test: test-unit test-integration test-controller-db test-agent-control test-protocol-vectors test-spec

test-unit:
>cargo test --workspace --lib --bins
>npm test

test-integration:
>./scripts/test-integration.sh

test-controller-db:
>./scripts/test-controller-db.sh

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

test-docker-deployment:
>./scripts/test-docker-deployment.sh

test-windows-xsnet-abi:
>./scripts/test-windows-xsnet-abi.sh

test-windows-xsnet-source:
>./scripts/validate-windows-xsnet-source.py

test-windows-xsnet-installer:
>./scripts/validate-windows-xsnet-installer.py

test-windows-xsnet-vm-scripts:
>./scripts/validate-windows-xsnet-vm.py

test-protocol-vectors:
>./scripts/test-protocol-vectors.sh

test-spec:
>python3 scripts/validate-m02.py

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
>python3 scripts/check-secrets.py --root . --reference-env /etc/xs-nexus/controller.env

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

release:
>@printf 'Release packaging is not implemented before M9.1\n' >&2
>@exit 2

clean:
>cargo clean
>rm -rf apps/console/dist
