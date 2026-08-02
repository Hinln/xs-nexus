# ACCEPTANCE.md — 产品级强制验收

所有 `MUST` 项必须有实际证据。没有证据视为未完成。

---

## A. 独立实现

- [x] MUST：核心运行路径不依赖禁用的组网/VPN/穿透产品。
- [x] MUST：`THIRD_PARTY.md` 列出全部依赖和许可证。
- [x] MUST：Clean-room 文档完整。
- [x] MUST：不存在复制的私有协议和包格式。
- [x] MUST：SBOM 可生成。

证据：M0.2 clean-room 与协议原创性验证 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`；全运行源码禁用引用门禁、321 个 Cargo 与 110 个 npm 精确锁定依赖许可证、确定性源码 CycloneDX 1.6/SPDX 2.3 生成和负向测试证据 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。四个运行镜像的精确 image ID、113 个已安装 OS 包、逐包许可证全文闭包、Dockerfile、Git revision 和 in-toto/SLSA provenance 证据为 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z`；同目录的 Grype 0.116.1 扫描为 Critical 2、High 4、Medium 16、Negligible 24，无当前标注可修复项，有界处置已通过但不代表漏洞已修复。

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

证据：M5.1 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`；arm64 为真实交叉构建与 ELF 架构验证，目标设备运行仍由 NAS/arm64 实机门禁验证。

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

证据：M3.1 `/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`、M3.2 `/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`。真实 NAS 子网审批仍属于人工门禁，不以 namespace 结果替代。

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

证据：Relay `/metrics` 提供接收/转发字节、分类与总丢弃、I/O 错误、队列转发延迟样本/平均/最大值；真实双 Relay fallback、密文、failover 和 Direct 恢复聚合证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260731T185454Z`。这里的“丢包”仅指 Relay 可观测的协议、认证、重放、限速、队列、目的地和发送丢弃，不伪称测得公网链路中不可观测的 UDP 丢失。

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

---

## I. 安装、升级和卸载

- [x] MUST：Linux 一键安装。
- [x] MUST：安装包哈希。
- [x] MUST：发布签名。
- [x] MUST：失败回滚。
- [x] MUST：升级保留节点身份。
- [x] MUST：升级包篡改被拒绝。
- [x] MUST：卸载不残留接口和路由。
- [ ] MUST：Windows 安装器在测试 VM 通过。
- [ ] MUST：驱动和 Agent 版本兼容。
- [x] MUST：正式签名状态如实说明。

证据：M5.1 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。已验证测试密钥签名、首次公钥固定、错误密钥/签名/哈希/包内篡改拒绝；正式离线发布密钥和签名仪式未完成，不以测试签名冒充生产签名。Windows 项保持未完成。

---

## J. 1Panel 部署

- [x] MUST：服务加入外部 `1panel-network`。
- [x] MUST：不重建该网络。
- [ ] MUST：数据库不向公网暴露。
- [x] MUST：容器默认非 root。
- [x] MUST：健康检查。
- [x] MUST：日志轮转。
- [x] MUST：数据备份恢复。
- [x] MUST：数据库迁移失败可恢复。
- [x] MUST：开发和 RC 隔离。
- [x] MUST：不影响 1Panel 现有服务。

证据：M5.2 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`。项目 Compose 不包含数据库服务或数据库端口，但既有 1Panel PostgreSQL/Redis 公网暴露仍由 `KI-006`/`BLK-005` 阻塞宿主级“数据库不向公网暴露”，因此该项不勾选。

---

## K. Windows 驱动

- [ ] MUST：不使用 Wintun/TAP。
- [ ] MUST：驱动只做虚拟 NIC 和安全 IPC。
- [ ] MUST：所有输入边界校验。
- [ ] MUST：测试签名构建。
- [ ] MUST：Windows 11 测试 VM 安装/卸载。
- [ ] MUST：Driver Verifier 实际记录。
- [ ] MUST：无蓝屏。
- [ ] MUST：Agent 崩溃不破坏普通网络。
- [ ] MUST：未完成项目不可标记完成。

当前仅有平台无关 ABI/会话/有界 IPv4 队列、任意输入压力和 720 种 teardown 交错模型测试，以及 UMDF 源码不变量、测试安装器、测试包构建、六阶段 VM 编排和 exact ABI/DriverVer 一致性静态检查。安全 Rust `XsnetDeviceSession` 已验证无重试启动、协商上限、单步 TX/RX、失败启动释放、无效输入/Drop 零 I/O、失败毒化、显式 shutdown 重试和有序 Detach，但未接入运行时；源码门禁在 VM 前禁止 runtime 引用和后台行为。Windows 本地管理服务器、私有存储和 Service/SCM 已具备固定命名管道、受限 protected DACL、reparse 拒绝、write-through 原子替换、固定服务名、四阶段状态和一次性停止通知的隔离源码边界，并通过各自最小 MSVC target check；但未在 Windows 运行、未检查有效 ACL、替换语义、崩溃恢复或真实 SCM 启停，也没有 Windows CLI/正式安装器，因此不改变任何 K 项。空 TX/满 RX 的 Win32 失败映射必须先经 VM 证明，不能用通用错误码猜测权威拒绝。Agent、驱动和安装状态固定 ABI v1；INF、构建清单、显式期望版本和 staged driver-store 必须一致。该规则只支持 clean install/uninstall，不代表驱动和 Agent 升级兼容项通过。源码已加入同步 direct-I/O、SetLink 双队列门禁和系统缓冲区 TX/RX ring 复制；VM 编排固定快照声明、显式双重重启、Driver Verifier oneboot、系统基线、零残留与证据哈希。上述 PowerShell 均未执行设备、WDK、SCM 或 Verifier 操作，采证脚本也明确不声称场景验收。生命周期 harness 不等于 Agent crash、PnP/power 或 WDF 调度实测；`xsnet.vcxproj`、INF、DriverEntry、file object、IOCTL、PnP/power 与 ring API 尚未经过 WDK、InfVerif、签名或 VM；所有 K 项保持未勾选。最新自动化证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`。Windows 10 官方支持冲突见 `KI-016`，生产安装器差距见 `KI-017`。

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
- [ ] MUST：性能报告。
- [x] MUST：第三方依赖。
- [x] MUST：最终真实报告。
- [x] MUST：明确当前是否适合生产。

证据：`docs/ARCHITECTURE.md`、`docs/XSP1_PROTOCOL.md`、`docs/CONTROLLER_API.md`、`docs/LINUX_INSTALLATION.md`、`docs/UPDATE_SYSTEM.md`、`RECOVERY_RUNBOOK.md`、`docs/SECURITY_ASSUMPTIONS.md`、`docs/THREAT_MODEL.md`、`QA_MATRIX.md`、`THIRD_PARTY.md` 与 `FINAL_REPORT.md`。性能报告 `docs/PERFORMANCE_REPORT.md` 已纳入 24 小时长样本与修正后的真实重启回归。

---

## 完成判定

任何一个 MUST 项没有实际证据时，项目只能描述为“部分完成”或“Release Candidate 尚未满足”，不得描述为完整交付。
