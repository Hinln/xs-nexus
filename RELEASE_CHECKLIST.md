# RELEASE_CHECKLIST.md — Release Candidate 检查

---

## 源代码

- [x] Git 状态干净（当前检查点）
- [x] 固定提交哈希（Git 检查点）
- [x] 无秘密（秘密扫描通过）
- [x] 无未知大文件（仓库清单审计）
- [x] 格式化通过
- [x] 静态分析通过
- [x] 依赖锁定（Cargo/npm 源码依赖）
- [x] THIRD_PARTY 完整（431 个源码依赖与许可证）
- [x] SBOM 生成（源码 CycloneDX/SPDX 与四个运行镜像 OS 包/逐包许可证闭包均完成）
- [x] 漏洞扫描完成（Grype 0.116.1；`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`；可修复项为 0，处置门禁通过）

## 构建

- [x] Linux x86_64（M5.1 测试签名构建）
- [x] Linux arm64（M5.1 真实交叉构建与 ELF 验证，目标运行待门禁）
- [x] Controller（精确 revision 生产候选镜像构建、迁移与健康验证）
- [x] Relay（精确 revision 生产候选镜像构建与健康验证）
- [x] Console（精确 revision 生产候选镜像构建与健康验证）
- [x] CLI（Linux x86_64/arm64 包）
- [ ] Windows Agent
- [ ] Windows Driver
- [x] 安装包（M5.1 测试签名范围）
- [x] 确定性或可追溯构建信息（image digest、Dockerfile hash、revision、provenance）

## 测试

- [x] 单元
- [x] 集成
- [x] 网络实验
- [x] 协议负向
- [x] NAT
- [x] Relay
- [x] Agent/Relay 身份签名遥测、重放/回滚拒绝与 Console 有界 24 小时聚合
- [x] ACL
- [x] 子网路由
- [x] Playwright
- [x] 视觉
- [x] 安装（Linux M5.1）
- [x] 升级（Linux M5.1）
- [x] 回滚（Linux M5.1）
- [x] 卸载（Linux M5.1）
- [x] 稳定性（24 小时长样本与修正重启门禁回归已审计）
- [x] 安全检查
- [x] Windows 11 xsnet standard/UMDF/Application Verifier 实机通过（`docs/WINDOWS_XSNET_VM_EVIDENCE.md`）

## 部署

- [x] 使用外部 `1panel-network`
- [x] Gate 15 共存（current-source external-only/lifecycle、真实 Docker restart、生产 host reboot、项目 upgrade/rollback 与 1Panel/OpenResty/SSH/网络不变量）
- [x] 无数据库公网端口（当前生产候选服务器外部探测与 Docker host binding 复核通过）
- [x] 容器非 root
- [x] Secret 仓库外
- [x] 健康检查
- [x] 日志轮转
- [x] 备份（age 流式认证加密、独立文件系统自动复制、公开/深度校验、保留和销毁墓碑）
- [x] 恢复演练（真实 PostgreSQL 测试 schema、错误 identity/篡改拒绝、异地取回和失败回滚）
- [ ] 正式离线备份 identity 与真实异地主机（产品边界已完成，外部门禁 `BLK-007`）
- [x] 防火墙最小开放（Gate 14：独立 nftables 表、默认拒绝、外部端口和 SSH 负向验证）
- [x] 生产主机补丁与 reboot（157 个升级、0 pending、`6.8.0-137-generic`、one-shot fallback/watchdog、内外部回归）
- [x] 有界磁盘治理（`83%`→`77%`；无 global prune/autoremove；`1panel-network` 和无关服务不变）
- [ ] 外部磁盘 warning/critical 真实送达、on-call 确认和 recovery closure（`BLOCKED_EXTERNAL`）
- [x] 开发和 RC 隔离

## 更新

- [x] 清单签名（临时测试 Ed25519 密钥；正式离线签名未完成）
- [x] 哈希（外部归档与包内逐文件 SHA-256）
- [x] 版本防回滚（外部降级拒绝）
- [x] 分批（网络/通道/平台/架构策略、基点分桶、暂停和最低版本）
- [x] 回滚（只允许已安装且签名/哈希仍有效版本）
- [ ] 离线签名私钥未进入服务器（产品接口只接受公钥，但正式离线签名仪式和密钥分发仍是门禁）

## 文档

- [x] README
- [x] 架构
- [x] XSP/1
- [x] API
- [x] 安全假设
- [x] 威胁模型
- [x] 安装（Linux）
- [x] 升级（Linux）
- [x] 卸载（Linux）
- [x] 恢复（Linux Agent 生命周期）
- [x] 1Panel（项目部署与恢复；当前主机数据库无 host binding，`BLK-005` 已解除）
- [x] 测试报告（`QA_MATRIX.md` 与各阶段证据目录）
- [x] 性能报告（`docs/PERFORMANCE_REPORT.md`；含 24 小时稳定性审计）
- [x] FINAL_REPORT（如实标记部分完成和生产不适用）

## 实机门禁

- [x] Linux 核心通过（开发服务器与隔离 namespace；真实 NAS/arm64 仍待门禁）
- [x] Windows 11 xsnet 测试签名驱动 VM 通过
- [ ] 完整 Windows Agent/SCM/路由/睡眠 VM 门禁通过
- [ ] NAS 由用户手动接入
- [ ] 日常 Windows 仅在 VM 通过后接入
- [ ] DNS 由用户批准
- [x] 正式驱动签名状态明确（首版 Windows 适配器使用固定且发行方签名的 Wintun 0.14.1；自研 xsnet 仅保留测试签名实验路径，不作正式签名声明）
- [ ] 所有临时密码待发布后轮换

## 结论

- [ ] 可标记 Release Candidate
- [ ] 仍为部分完成
- [x] 不适合生产

必须三选一，并在 `FINAL_REPORT.md` 给出证据。

当前生产运行 revision 为 `3d93656cc9ec3ea35d58e453118154b25bcc4e14`；Gate 16 隔离和生产证据分别为 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z` 与 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`，Gate 14 防火墙与维护证据分别为 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z` 和 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z`。未完成的 Windows 在线、NAS、计划域名 strict TLS、外部磁盘告警/on-call、正式密钥/异地恢复、全量凭据轮换和第三方审计门禁禁止勾选 Release Candidate。

## Runtime image license closure checkpoint (2026-07-31)

- [x] Exact installed OS package set has one closure record per package in implementation dry runs.
- [x] Referenced license materials are path/size/SHA-256 bound and independently rehashed.
- [x] Missing SPDX text, unsafe rootfs paths and package identity drift fail closed.
- [x] Clean-commit full image build, deterministic regeneration, vulnerability scan and disposition evidence completed at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`.

## Production provenance remediation V2 (2026-08-09)

- [x] Unified version/commit/protocol/build identity for Controller, Relay, Console, Agent, CLI, OCI images and Linux packages.
- [x] Deterministic release manifest, SHA256SUMS, SBOM, in-toto/SLSA provenance and strict detached Ed25519 verification tooling.
- [x] Runtime/package identity mismatch and signed forged-commit manifest rejection.
- [x] Exact revision `fea456b3d6feff36856b1f2066ace8a22b650bce` passed GitHub Actions run `31289641228`, including double no-cache image reproducibility.
- [ ] Human-controlled offline release key ceremony, authenticated public-key publication, rotation/revocation and recovery.
- [ ] Valid formal signed RC tag and signed production release bundle.
- [x] Remediation revision `3d93656` clean production deployment, runtime reverse verification, and repeated `ff9551d3 -> 3d93656 -> ff9551d3` rollback rehearsal.
- [ ] Merge to `main`, owner-controlled valid signed RC tag/bundle, and deployment of that exact formal RC.

These unchecked items keep Gate 01 `FAIL`, Release Candidate unchecked, and the overall result `NO_GO`.

## Planned-domain strict TLS remediation V2 (2026-08-09)

- [x] Independent read-only DNS, edge TLS, direct-origin SNI, installed certificate, OpenResty routing, loopback health, and WebSocket diagnosis.
- [x] Strict TLS/SNI/HTTP/WebSocket audit tool, negative regression tests, and placeholder-only Console OpenResty template at exact commit `94ccae3`.
- [x] Failure evidence and exact-commit rerun secret-scanned, SHA-256 sealed, and indexed without production mutation.
- [ ] Owner-approved planned-domain origin certificate and isolated 1Panel/OpenResty virtual host.
- [ ] CDN HTTPS origin, planned Origin Host/SNI, full strict certificate and hostname verification; no Flexible/plaintext/ignored-error mode.
- [ ] Independent direct-origin and CDN health/login/authenticated API/WebSocket/Console/unknown-route/browser E2E with unrelated-site and host-network invariants.

The first three items are preparation and diagnosis only. The unchecked production items keep Gate 13 `FAIL`, Gate 19 `PARTIAL`, Release Candidate unchecked, and the overall result `NO_GO`.

## Gate 23 internal remediation checkpoint (2026-08-09)

- [x] Generic unauthenticated JSON rejection envelope with parser-detail negative tests.
- [x] Production PostgreSQL Docker log rotation bounded to `10m`/`5`, protected by backup, rollback, and independent verification.
- [x] Rust `1.94` and SQLx `0.9.0`; `rsa`/`RUSTSEC-2023-0071` removed without an advisory ignore.
- [x] Full `cargo-deny` advisories, bans, licenses, and sources policy; plain `cargo audit` reports zero vulnerabilities.
- [x] Exact commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b` passed GitHub Actions run `31313868529` and clean-checkout evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`.
- [x] Nine failed/non-authoritative Gate 23 roots retained, dispositioned, independently checksummed, and reverified at `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z`.
- [x] `paste 1.0.15` informational warning formally reviewed with exact locked/upstream topology, visible output, and an automatically expiring `2026-08-31` disposition; evidence `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z`.
- [x] Exact head `98ca145c197b1d2af20d7cf5df28008820a25e50` uses reviewed SHA-pinned Node 24 actions, disables checkout credential persistence, and passes all four jobs in run `31325753985` with zero check-run annotations.
- [ ] `KI-021` glibc disposition closed through fixed images or formally accepted by the owner after a fresh exact-image scan.
- [ ] All global P0/P1/Critical/High findings, external hard gates, formal signed RC/main provenance, and deployment of the exact release revision complete.

The checked items close four self-fixable findings and one bounded P2 dependency review only. Production still runs `3d93656`, Gate 23 remains `FAIL`, Gate 24 remains `PARTIAL`, Release Candidate remains unchecked, and the overall result remains `NO_GO`.
