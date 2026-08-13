# ACCEPTANCE.md — 产品级强制验收

所有 `MUST` 项必须有实际证据。没有证据视为未完成。

---

## A. 独立实现

- [x] MUST：核心运行路径不依赖禁用的组网/VPN/穿透产品。
- [x] MUST：`THIRD_PARTY.md` 列出全部依赖和许可证。
- [x] MUST：Clean-room 文档完整。
- [x] MUST：不存在复制的私有协议和包格式。
- [x] MUST：SBOM 可生成。

证据：M0.2 clean-room 与协议原创性验证 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`；全运行源码禁用引用门禁、321 个 Cargo 与 110 个 npm 精确锁定依赖许可证、确定性源码 CycloneDX 1.6/SPDX 2.3 生成和负向测试证据 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。四个运行镜像的精确 image ID、113 个已安装 OS 包、逐包许可证全文闭包、Dockerfile、Git revision 和 in-toto/SLSA provenance 证据为 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`；同目录的 Grype 0.116.1 扫描为 Critical 2、High 4、Medium 16、Negligible 24，无当前标注可修复项，有界处置已通过但不代表漏洞已修复。

---

## B. Linux 节点

- [x] MUST：Linux x86_64 Agent 可构建和安装。
- [x] MUST：Linux arm64 Agent 可交叉构建或在目标环境构建。
- [x] MUST：使用 `/dev/net/tun`。
- [x] MUST：通过 Netlink 管理接口和路由。
- [x] MUST：不依赖 `ip` 命令完成核心运行逻辑。
- [x] MUST：崩溃后不破坏默认网络。
- [x] MUST：卸载后无项目路由残留。
- [x] MUST：systemd 自动启动和重启策略正确。

证据：M5.1 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`；Gate 06 补充 revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02`、GitHub Actions run `31352258781`、job `93345151258` 和 artifact `9049384061` 证明真实 systemd `SIGKILL` 单次自动重启、状态保留、私有 TUN 重建、隔离链路变化与 cleanup。arm64 为真实交叉构建与 ELF 架构验证，整机 reboot/disk-full/DHCP/VPN 冲突及目标设备运行仍由普通主机和 NAS/arm64 实机门禁验证。任务书九个 CLI 命令的协议、Unix 运行和 Windows Named Pipe 交叉编译补充证据为 `/srv/xs-nexus/artifacts/qa/m1.2-cli-completion-20260802T100106Z` 及 `/srv/xs-nexus/artifacts/qa/final-local-audit-20260802T102009Z`。

---

## C. XSP/1 数据平面

- [x] MUST：双方身份认证。
- [x] MUST：临时密钥和前向安全设计。
- [x] MUST：AEAD 加密。
- [x] MUST：双向独立密钥。
- [x] MUST：协议和网络身份绑定。
- [x] MUST：抗重放。
- [x] MUST：密钥轮换。
- [x] MUST：篡改包丢弃。
- [x] MUST：伪造源地址丢弃。
- [x] MUST：抓包无法看到原始业务负载。
- [x] MUST：Relay 无法恢复业务明文。
- [x] MUST：明确记录未完成第三方安全审计。

Gate 04 补充证据：revision `875395352afc8a17dafc17b2496d62ce701ee4ae` 修复丢失 ServerFinish、KeyUpdateAck 和 PathResponse 时的有界恢复状态机，并让 KeyUpdate 尝试耗尽触发完整重握手。GitHub Actions run `31350065978` 的格式、严格 Clippy、全量测试、真实数据库/namespace、Console E2E、镜像复现和六目标 AddressSanitizer Fuzz 全部通过；Fuzz 各运行 180 秒，总计 134,262,706 次，525 项 artifact SHA-256 与无值秘密扫描通过。该内部验收不替代 Gate 05 独立安全审计。

---

## D. 控制平面

- [x] MUST：节点本地生成私钥。
- [x] MUST：一次性 Token 有有效期和使用次数。
- [x] MUST：Token 只存哈希。
- [x] MUST：节点凭证可吊销。
- [x] MUST：配置带版本和签名。
- [x] MUST：配置回滚攻击被拒绝。
- [x] MUST：Controller 短暂中断时已有连接继续。
- [x] MUST：恢复后增量同步。
- [x] MUST：审计日志覆盖安全和管理操作。
- [x] MUST：Controller 不承载普通业务数据。

证据：M1.1 `/srv/xs-nexus/artifacts/qa/m1.1-20260729T105009Z`、M1.3 `/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`、M3.1 `/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`。

---

## E. NAT 和路径

- [x] MUST：候选地址收集。
- [x] MUST：公网映射发现。
- [x] MUST：认证探测包。
- [x] MUST：同 LAN 优先。
- [x] MUST：IPv6 可用时优先合理路径。
- [x] MUST：UDP 打洞。
- [x] MUST：无法直连自动 Relay。
- [x] MUST：Relay 故障切换。
- [x] MUST：恢复后尝试 Direct。
- [x] MUST：路径变化有真实原因记录。
- [x] MUST：UDP 被封锁时行为明确。

---

## F. 路由和 ACL

- [x] MUST：默认拒绝。
- [x] MUST：发送端与接收端双重执行。
- [x] MUST：节点身份与源虚拟 IP 绑定。
- [x] MUST：策略版本和签名。
- [x] MUST：控制器离线继续使用最近有效策略。
- [x] MUST：IPAM 无活动地址冲突。
- [x] MUST：`100.88.0.0/16` 冲突检测。
- [x] MUST：重叠子网检测。
- [x] MUST：子网发布需要审批。
- [x] MUST：网关离线后路由失效。
- [x] MUST：卸载和禁用可撤销路由。

证据：M3.1 `/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`、M3.2 `/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`。Gate 09 又在精确 revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b` 以三台真实 Linux Agent/TUN、Controller 离线、Direct/Relay/子网路由分别验证 A→B 允许、A→C/C→B 拒绝、ICMP/TCP/UDP、异常端口、伪造虚拟源/Node ID、双端执行、配置与策略双重回滚拒绝，以及 Relay/子网绕过失败；GitHub Actions run `31360862865` 全部七个 job 通过，ACL job `93369332314`、artifact `9052383034` 的归档/内部 SHA-256 和无值秘密扫描通过。Gate 09 为 `PASS`；真实 NAS 子网审批仍属于独立人工门禁，不以 namespace 结果替代。

---

## G. Relay

- [x] MUST：认证节点才可使用。
- [x] MUST：限制速率、并发、队列和会话。
- [x] MUST：无匿名开放转发。
- [x] MUST：防反射放大。
- [x] MUST：多 Relay。
- [x] MUST：健康检查。
- [x] MUST：监控字节、延迟、丢包和错误。
- [x] MUST：不记录业务内容。

证据：Relay `/metrics` 提供接收/转发字节、分类与总丢弃、I/O 错误、队列转发延迟样本/平均/最大值；真实双 Relay fallback、密文、failover 和 Direct 恢复聚合证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260731T185454Z`。Gate 08 又在精确 revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` 增加全局注册验签预算、每节点及全局包/字节队列硬上限和精确释放计数；GitHub Actions run `31358498444` 的专项 job `93362562136` 完成 5,000,000 帧持续转发、零产品丢弃/零残留队列、同身份短期 Lease 续租、双 Relay 停止/重启/重新注册/恢复和 Direct 回切，artifact `9051561308` 通过。这里的“丢包”仅指 Relay 可观测的协议、认证、重放、限速、队列、目的地和发送丢弃，不伪称测得公网链路中不可观测的 UDP 丢失；真实公网恶意流量、多地域和长时多实例容量仍属于生产 Gate 08 外部证据。

---

## H. Web 控制台

- [x] MUST：真实登录和权限。
- [x] MUST：首页真实指标。
- [x] MUST：节点管理。
- [x] MUST：网络和地址池。
- [x] MUST：Token。
- [x] MUST：ACL。
- [x] MUST：子网审批。
- [x] MUST：Relay。
- [x] MUST：拓扑。
- [x] MUST：审计日志。
- [x] MUST：更新管理。
- [x] MUST：加载、空、错误、无权限状态。
- [x] MUST：Playwright 主流程通过。
- [x] MUST：视觉验收通过。
- [x] MUST：无未解释浏览器错误。

M4.1/M4.2 说明：页面只显示 Controller 已知事实；更新发布、灰度策略、节点分配通道和 Agent 签名更新状态现已接入真实 API。后续可观测性闭环又接入节点身份签名的路径/流量/握手/RTT 报告与 Relay 目录身份签名的累计指标；Controller 保存有界 25 小时窗口，缺失或陈旧数据仍显示不可用/陈旧，不以固定值满足验收。隔离验证覆盖真实 PostgreSQL、双 Agent/双 Relay 控制与数据面、10 条 Playwright 主流程和 6 个视口；备份恢复 UI 仍只描述已实现能力，不伪造执行状态。

Gate 19 候选 revision `5505893710ab1d15e06495603dff08bf5c1e035f` 的 run `31529393933` 全九 job 通过；专项 job `93905489516` 和 artifact `9116327161` 使用真实 PostgreSQL、Controller、生产 Console build 与 Chromium，执行 7 个生产矩阵场景和原始真实流程，8 项均为 expected、无 skip/flaky/unexpected。132 张截图覆盖六视口、全部管理页、登录、详情、404、loading 和 offline；浏览器观测无 page error/5xx，只有显式断网的两项预期失败。归档 digest、139 项内部清单和无值秘密扫描通过。该证据不包含 API 拦截，但仍不是计划域名严格 TLS、公网 Console 或当前生产部署证据；Gate 19 保持 `PARTIAL`，总体保持 `NO_GO`。

---

## I. 安装、升级和卸载

- [x] MUST：Linux 一键安装。
- [x] MUST：安装包哈希。
- [x] MUST：发布签名。
- [x] MUST：失败回滚。
- [x] MUST：升级保留节点身份。
- [x] MUST：升级包篡改被拒绝。
- [x] MUST：卸载不残留接口和路由。
- [ ] MUST：Windows 安装器在受控 Windows 测试目标通过；VM 必须有快照，实体机必须有外置完整系统镜像、可启动恢复介质、磁盘恢复材料和现场救援能力。
- [ ] MUST：驱动和 Agent 版本兼容。
- [x] MUST：正式签名状态如实说明。

证据：M5.1 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。已验证测试密钥签名、首次公钥固定、错误密钥/签名/哈希/包内篡改拒绝；Gate 01 V2 又在 exact revision `fea456b3d6feff36856b1f2066ace8a22b650bce` 的 GitHub Actions run `31289641228` 验证统一 build identity、确定性 release bundle、OCI/Console identity、in-toto/SLSA subjects、严格 Ed25519 verifier 和 signed-forged-commit 安装拒绝。正式离线发布密钥、签名仪式、正式 signed tag/bundle、生产反向核验仍未完成，不以测试签名冒充生产签名。Windows 项保持未完成。

Gate 18 候选 revision `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1` 的 run `31515281011` 全九 job 通过；专项 job `93858773221` 和 artifact `9110864186` 证明 schema 2 统一清单、密钥重叠、在线/离线撤销、中断/截断/超限/真实 ENOSPC、错误平台、篡改、旧版、撤销回滚、身份与现有版本保持及 namespace 路由清理。归档 SHA-256 与 GitHub digest 一致，八个内部 payload 和无值秘密扫描通过。该证据使用开发密钥和 hosted x86_64，不能替代正式离线仪式、认证分发、签名 RC、真实目标平台或生产发布/回滚链；Gate 18 保持 `PARTIAL`，Windows 两项保持未完成。

---

## J. 1Panel 部署

- [x] MUST：服务加入外部 `1panel-network`。
- [x] MUST：不重建该网络。
- [x] MUST：数据库不向公网暴露。
- [x] MUST：容器默认非 root。
- [x] MUST：健康检查。
- [x] MUST：日志轮转。
- [x] MUST：数据备份恢复。
- [x] MUST：数据库迁移失败可恢复。
- [x] MUST：开发和 RC 隔离。
- [x] MUST：不影响 1Panel 现有服务。

证据：M5.2 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`，以及生产主机精确提交 `ff9551d322067c934d2ac7d55a62af8896660bb3` 的全量 `/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z` 和初始部署 `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z`。Gate 16 又以精确提交 `3d93656cc9ec3ea35d58e453118154b25bcc4e14` 完成运行/迁移/所有者数据库角色分离、生产升级、四次自动回滚和独立 SSH 反向核验；证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`。`/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z` 证明项目 PostgreSQL 没有 host binding，外部 TCP `3306`、`5432`、`6379`、`28080`、`28081` 均不可达；当前四个项目容器健康且 `1panel-network`、默认路由、IP rule 和非项目 nftables 规则未变。正式离线 identity 和真实异地主机仍由 `BLK-007` 阻塞生产灾难恢复验收。

Gate 15 在 exact revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6` 完成正式共存复核：GitHub Actions run `31504402285` 全八 job 通过，专项 job `93822197946` 证明 current Compose/lifecycle 只 external reference、project-scoped down 不删除网络或未知 sentinel、真实 Docker daemon restart 保留 exact network/sentinel、route/inventory 与 cleanup；artifact `9106406005` 的归档/内部 SHA-256 和无值秘密扫描通过。Gate 14 的真实 host reboot 与 Gate 16 的 production upgrade/四次 rollback 共同覆盖 1Panel/OpenResty/网站、数据库边界、SSH 和生产网络不变量。Gate 15 为 `PASS`，但当前分支未部署且总体仍为 `NO_GO`。

---

## K. Windows 驱动

- [x] MUST：除经用户批准、仅限 Windows L3 适配器边界的 Wintun 0.14.1 外，不使用 TAP、WireGuard 协议/内核实现或任何第三方组网、VPN、穿透、Relay 实现；该例外必须固定版本、归档/DLL 哈希、发行方签名和许可证，并且不得替代 XS Nexus 的控制、认证、加密、XSP/1、ACL、路由或 Relay。
- [x] MUST：驱动只做虚拟 NIC 和安全 IPC。
- [x] MUST：所有输入边界校验。
- [x] MUST：测试签名构建。
- [x] MUST：Windows 11 测试 VM 安装/卸载。
- [x] MUST：Driver Verifier 实际记录。
- [x] MUST：无蓝屏。
- [ ] MUST：Agent 崩溃不破坏普通网络。
- [x] MUST：未完成项目不可标记完成。

源码独立实现、驱动职责和全部线格式/队列/事务输入边界已有可执行模型、任意输入压力、720 种 teardown 交错、源码门禁与 MSVC target check；依赖门禁拒绝 TAP、WireGuard 协议/内核实现和其他第三方组网核心，只允许 ADR-077 固定的 Wintun adapter/session 例外。测试签名 `xsnet` 又在 Windows 11 24H2 VM 完成 WDK Release 构建、InfVerif/Inf2Cat/签名校验、clean install、SYSTEM Tx/Rx、普通 PnP restart、standard Driver Verifier、UMDF/Application Verifier 三轮 restart/smoke、零新相关 dump/WER/error event 和 clean uninstall；首版 Wintun 路径另有发行方签名、离线 adapter smoke、整包完整性和生产回环下载证据。在线 Agent crash 不破坏普通网络、真实 SCM/Named Pipe/IP Helper/power 和公网安装仍开放，因此项目继续明确标记“部分完成”。Windows 10 支持冲突见 `KI-016`，在线安装闭环见 `KI-022`。

---

## L. 缺陷和稳定性

- [x] MUST：P0 为 0。
- [x] MUST：P1 为 0。
- [x] MUST：P2 有明确结论。
- [x] MUST：连续三轮全量回归无新增失败。
- [x] MUST：24 小时稳定性测试或明确外部阻塞。
- [x] MUST：无未解释资源泄漏。
- [x] MUST：无未解释日志持续增长。
- [x] MUST：故障恢复测试通过。

Gate 23 当前精确回归基线为 `3bf861922c8b3cc62c3bfd1617835565fd86fc6b`：JSON parser detail、PostgreSQL 无界 Docker 日志、`rsa`/`RUSTSEC-2023-0071` 和缺失 license policy 四项可自行修复 finding 已关闭；源 SBOM 预期计数为 Cargo 296、npm 110、总计 406。GitHub Actions run `31313868529` 与 clean-checkout 证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z` 通过。上述勾选是项目内部验收，不代表生产 Hard Gate 23 已 PASS；全局 Critical/High、外部门禁、正式签名发布和当前 revision soak 未关闭，Gate 23 仍为 `FAIL`、Gate 24 为 `PARTIAL`、总体为 `NO_GO`。

Gate 21 当前精确内部证据为 revision `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363`、run `31603852656`、job `94137661764` 和 artifact `9144433450`，覆盖 1,000 节点/控制会话、失败关闭容量、资源阈值、协议吞吐与 namespace Agent RTT。该证据不覆盖公网、多地域、多实例或长时容量，因此 Gate 21 为 `PARTIAL`。上面的历史 24 小时验收勾选也不关闭当前 revision Gate 22；至少 24 小时完整采样与故障注入仍为 `UNKNOWN`，总体继续 `NO_GO`。

---

## M. 最终文档

- [x] MUST：架构。
- [x] MUST：协议。
- [x] MUST：API。
- [x] MUST：安装。
- [x] MUST：升级。
- [x] MUST：卸载。
- [x] MUST：灾难恢复。
- [x] MUST：安全假设。
- [x] MUST：测试报告。
- [x] MUST：性能报告。
- [x] MUST：第三方依赖。
- [x] MUST：最终真实报告。
- [x] MUST：明确当前是否适合生产。

证据：`docs/ARCHITECTURE.md`、`docs/XSP1_PROTOCOL.md`、`docs/CONTROLLER_API.md`、`docs/LINUX_INSTALLATION.md`、`docs/UPDATE_SYSTEM.md`、`RECOVERY_RUNBOOK.md`、`docs/SECURITY_ASSUMPTIONS.md`、`docs/THREAT_MODEL.md`、`QA_MATRIX.md`、`THIRD_PARTY.md` 与 `FINAL_REPORT.md`。性能报告 `docs/PERFORMANCE_REPORT.md` 已纳入 Gate 21 当前版本内部容量矩阵、历史 24 小时长样本与当前 revision Gate 22 未完成边界。

---

## 完成判定

任何一个 MUST 项没有实际证据时，项目只能描述为“部分完成”或“Release Candidate 尚未满足”，不得描述为完整交付。
