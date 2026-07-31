# ACCEPTANCE.md — 产品级强制验收

所有 `MUST` 项必须有实际证据。没有证据视为未完成。

---

## A. 独立实现

- [x] MUST：核心运行路径不依赖禁用的组网/VPN/穿透产品。
- [x] MUST：`THIRD_PARTY.md` 列出全部依赖和许可证。
- [x] MUST：Clean-room 文档完整。
- [x] MUST：不存在复制的私有协议和包格式。
- [x] MUST：SBOM 可生成。

证据：M0.2 clean-room 与协议原创性验证 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`；全运行源码禁用引用门禁、321 个 Cargo 与 110 个 npm 精确锁定依赖许可证、确定性 CycloneDX 1.6/SPDX 2.3 生成和负向测试证据 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。该 SBOM 只覆盖源码依赖，容器操作系统包仍属于 Release Checklist 未完成项。

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
- [ ] MUST：监控字节、延迟、丢包和错误。
- [x] MUST：不记录业务内容。

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

M4.1/M4.2 说明：页面只显示 Controller 已知事实；尚未实现的更新发布、备份恢复、路径、流量、延迟和 Relay 指标均以不可用状态及原因展示，不以固定值满足验收。证据：`/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`。

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

当前仅有平台无关 ABI/会话/有界 IPv4 队列、任意输入压力和 720 种 teardown 交错模型测试，以及 UMDF 源码不变量、测试安装器、测试包构建、六阶段 VM 编排和 exact ABI/DriverVer 一致性静态检查。安全 Rust `XsnetDeviceSession` 已验证无重试启动、协商上限、单步 TX/RX、失败启动释放、无效输入/Drop 零 I/O、失败毒化、显式 shutdown 重试和有序 Detach，但未接入运行时；源码门禁在 VM 前禁止 runtime 引用和后台行为。Windows 本地管理服务器已具备固定命名管道、首实例、拒绝远程客户端和受限 DACL 的隔离源码边界，并通过最小 MSVC target check；但未在 Windows 运行、未检查有效 DACL，也没有 Windows CLI，因此不改变任何 K 项。空 TX/满 RX 的 Win32 失败映射必须先经 VM 证明，不能用通用错误码猜测权威拒绝。Agent、驱动和安装状态固定 ABI v1；INF、构建清单、显式期望版本和 staged driver-store 必须一致。该规则只支持 clean install/uninstall，不代表驱动和 Agent 升级兼容项通过。源码已加入同步 direct-I/O、SetLink 双队列门禁和系统缓冲区 TX/RX ring 复制；VM 编排固定快照声明、显式双重重启、Driver Verifier oneboot、系统基线、零残留与证据哈希。上述 PowerShell 均未执行设备、WDK 或 Verifier 操作，采证脚本也明确不声称场景验收。生命周期 harness 不等于 Agent crash、PnP/power 或 WDF 调度实测；`xsnet.vcxproj`、INF、DriverEntry、file object、IOCTL、PnP/power 与 ring API 尚未经过 WDK、InfVerif、签名或 VM；所有 K 项保持未勾选。最新自动化证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T040415Z`。Windows 10 官方支持冲突见 `KI-016`，生产安装器差距见 `KI-017`。

---

## L. 缺陷和稳定性

- [ ] MUST：P0 为 0。
- [ ] MUST：P1 为 0。
- [ ] MUST：P2 有明确结论。
- [ ] MUST：连续三轮全量回归无新增失败。
- [ ] MUST：24 小时稳定性测试或明确外部阻塞。
- [ ] MUST：无未解释资源泄漏。
- [ ] MUST：无未解释日志持续增长。
- [ ] MUST：故障恢复测试通过。

---

## M. 最终文档

- [ ] MUST：架构。
- [ ] MUST：协议。
- [ ] MUST：API。
- [ ] MUST：安装。
- [ ] MUST：升级。
- [ ] MUST：卸载。
- [ ] MUST：灾难恢复。
- [ ] MUST：安全假设。
- [ ] MUST：测试报告。
- [ ] MUST：性能报告。
- [ ] MUST：第三方依赖。
- [ ] MUST：最终真实报告。
- [ ] MUST：明确当前是否适合生产。

---

## 完成判定

任何一个 MUST 项没有实际证据时，项目只能描述为“部分完成”或“Release Candidate 尚未满足”，不得描述为完整交付。
