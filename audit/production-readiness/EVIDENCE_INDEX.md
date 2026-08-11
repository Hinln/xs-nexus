# Production Audit Evidence Index

## Roots

- 首轮证据：`/srv/xs-nexus-qa/artifacts/production-readiness-audit-20260808T110559Z`
- 精确部署源码：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d`
- 成功 M5.2：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d/artifacts/qa/m5.2-20260808T121327Z`
- 实际镜像 SBOM/Grype：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d/artifacts/qa/production-runtime-images-20260808T125447Z`

## Version And Runtime

- `METADATA.txt`、`git-freeze.txt`、`git-show-ff9551d.txt`、`git-show-8745b58.txt`
- `git-diff-name-status.txt`、`git-diff-numstat.txt`、`git-diff-stat.txt`
- `running-containers.json`、`runtime-images.json`、`runtime-config.jsonl`
- `runtime-file-sha256.txt`、`runtime-binary-metadata.txt`、`deployment-active-record.txt`
- `clean-no-cache-rebuild.log`、`clean-rebuild-images.txt`、`clean-rebuild-binary-sha256.txt`
- `database-migration-provenance.txt`、`git-signing-release-audit.txt`、`github-audit.txt`

## Security And Supply Chain

- `gitleaks-git.json`、`gitleaks-production-state.json`、`gitleaks-runtime-logs.json`
- `gitleaks-image-controller.json`、`gitleaks-image-relay.json`、`gitleaks-image-console.json`
- `gitleaks-initial-finding-disposition.txt`、`gitleaks-audit-evidence-final.status`
- `cargo-audit.json`、`cargo-deny.txt`、`rust-advisory-paths.txt`、`rsa-target-all.txt`
- `npm-audit.json`、`npm-audit-status.txt`、`npm-ci.txt`
- `runtime-supply-chain.log`（源 SBOM 漂移失败）
- `runtime-image-vulnerability-summary.txt`、`runtime-image-vulnerability-disposition.txt`

## Host And Network

- `host-baseline.txt`、`ssh-audit.txt`、`listeners.txt`、`nftables.json`
- `routes.json`、`rules.json`、`kernel-hardening.txt`、`packages-upgradable.txt`
- `docker-host-security.txt`、`container-hardening.txt`、`services-hardening.txt`
- `1panel-network.json`、`onepanel-audit.txt`、`linux-agent-runtime-audit.txt`
- `disk-pressure-incident.txt`

## Public Edge And Web

- `domain-http-audit.txt`、`tls-vpn.xiashikeji.cn.txt`、`tls-vpn.qinwen.co.txt`
- `tls-protocol-audit.txt`、`external-port-audit.json`、`public-api-security-probes.txt`
- `web-public-runtime-audit.txt`
- `web-console-mock.status`、`web-console-mock/playwright.log`
- `web-console-mock/screenshots.txt`、`web-console-mock/screenshots.sha256`

## Database, Backup And Operations

- `postgresql-runtime-audit.txt`、`backup-runtime-audit.txt`、`backup-runtime-audit-detail.txt`
- `backup-public-verify.txt`、`observability-runtime-audit.txt`
- `log-xs-nexus-rc-*-last200.txt`

## Fresh Performance

- `performance/protocol-throughput-direct/report.json`
- `performance/relay-throughput-direct/report.json`
- `performance/agent-rtt/report.json`
- `performance/controller-scale-direct/report.json`
- 初次包装失败证据保留在相邻 `.status` 和 `test-output.txt`，未删除或改写。

## Historical Evidence Verification

- `prior-evidence-inventory.txt`、`prior-evidence-files.txt`、`prior-evidence-verification.txt`
- `m52-prior-evidence-verification.txt`、`external-evidence-location-search.txt`
- 历史 Windows、24h soak、最终导出和大多数 `/srv/xs-nexus/artifacts/qa` 原始证据不存在，不能用于 PASS。

最终 SHA-256 清单在首轮和终审结束时重新生成；原始失败日志全部保留。

## Final Remediation Evidence

- 修复证据根：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z`
- 生产健康守卫：`.../monitoring`
- 最终 CI 与 artifacts：`.../ci-final-8532eb6`
- 最终生产只读复核：`.../ci-final-8532eb6/production-final.txt`
- 最终生产复核文件 SHA-256：`13e0b6c717b8fb52840dd75c6449f47a09fd783c6d847a04ec4872756d5a6df9`
- CI 证据归档上传前 SHA-256：`cac62337529bdc1c2a4c7557d40d91bb60ee2ccc4590129c10a25d1cf42b85a2`
- GitHub final run：[31270487478](https://github.com/Hinln/xs-nexus/actions/runs/31270487478)，revision `8532eb6992389568643f8a501055c6acc72395ab`
- Artifact `9025475853`：`console-real-e2e-evidence`，GitHub digest `sha256:a2667d4fcd00fe818a005cf26bb47b0073ed3b2075b27134e9153560a1e8ef11`
- Artifact `9025496390`：`protocol-fuzz-evidence`，GitHub digest `sha256:905a28e11fd96d6605bf9989322fa39c4e99974020c78a618438ce798f4fe941`
- Artifact `9025655997`：`image-reproducibility-evidence`，GitHub digest `sha256:c84f58fb11639c274bed70b93e0a18b008492643714dc26105639f9b2fe24069`
- 可复现 OCI SHA-256：Edge `3557136259b04170b302c0a1c0ac88aa9b42a1362b65f7bc321c29d1bb671cd4`；Console `88ab58ebc6a9e78932bc5fa22f449a203d5ddac39f6cd583a3a0cfefa21dffcf`；Controller `2740f6e6110a1d22ab8c9f36306567e58d9234a00e9bce6cc1a773919d24c6be`；Relay `70e751415f97b3fb890e9de78068aea5fb4adcc5293fcb192a1c17405bac176a`；db-tools `2f7c53cb37c33125eb37a5d790d72ba735ad50786a8f509f27cb8c08c07db334`。

## Gate 19 V2 Console Evidence

- Exact source: revision `5505893710ab1d15e06495603dff08bf5c1e035f`.
- Exact-head CI: [run 31529393933](https://github.com/Hinln/xs-nexus/actions/runs/31529393933), all 9 jobs passed.
- Console job: [93905489516](https://github.com/Hinln/xs-nexus/actions/runs/31529393933/job/93905489516), real PostgreSQL/Controller/production-build/Chromium, no API interception.
- Artifact `9116327161`: `console-real-e2e-evidence`; GitHub and independent ZIP digest `sha256:f6d4903febab15e6c92bd46ee91451bfbea849fae84a166afd4aa1c0b67632d6`.
- Inner manifest digest `abe90dbf2525ee4587c1d521208253c262c3f3418d2ffee354b10cf7fcd78b31`; 139 listed files equal 139 downloaded files and all hashes match.
- Results: 8 expected, 0 unexpected/skipped/flaky; 14 browser observation sets, 0 page error/5xx; 132 screenshots over six viewports and real loading/offline/data states; no-value scan 0 findings.
- Detailed scope, visual review, failure chain and residual boundary: `audit/production-readiness-remediation-v2/CONSOLE_VISUAL_UX.md`.
- Result: internal current-source submatrix `PASS`, hard Gate 19 `PARTIAL` because planned-domain strict TLS/CDN/public runtime and current deployment remain absent.
