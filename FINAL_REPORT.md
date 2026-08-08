# XS Nexus 最终交付报告

报告日期：2026-08-08  
当前检查点：生产候选恢复、Windows Wintun 发布路径与剩余外部门禁（以本报告后续提交为准）

## 1. 结论

- [ ] 完整 Release Candidate
- [x] 部分完成
- [x] 不适合生产
- [ ] 仅研究/原型

核心 Linux/Controller/Relay/Console、安装生命周期、签名灰度更新、认证加密备份/复制边界、24 小时稳定性、协议安全边界和 Windows xsnet 测试签名驱动 WDK/VM/Verifier 已完成可复现验证。项目所有者又批准首版 Windows 使用发行方签名的 Wintun 0.14.1 作为仅 L3 适配器的例外；XS Nexus 的控制、身份、XSP/1、加密、ACL、路由、NAT 和 Relay 保持自研。当前 `vpn.qinwen.co` 下载可用，但任务书计划域名的 CDN 525、Windows 在线服务/网络、真实 NAS、正式发布/备份密钥、真实异地主机和第三方审计等门禁仍未完成，不能标记 Release Candidate 或描述为公网生产就绪。

## 2. 版本和构建

- Git：以本报告后续提交为准；签名更新先在隔离工作树和临时远程克隆完成验证。
- 环境：Ubuntu x86_64 开发服务器，Rust workspace；Linux 与隔离 network namespace 测试。
- 构建：Linux x86_64/aarch64 发布构建、Windows 相关最小 crate 的 `x86_64-pc-windows-msvc` target check，以及 Windows 11 24H2/WDK 26100 的 xsnet Release x64 test package 已验证。
- 签名：Linux 测试签名流程已验证；正式离线签名密钥和签名仪式未完成。
- SBOM：源码 CycloneDX/SPDX 与四个容器镜像 OS 包逐包许可证闭包、Dockerfile/revision/image digest 和 in-toto/SLSA provenance 已生成；证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`。固定 Grype 0.116.1 报告为 Critical 2、High 4、Medium 16、Negligible 24，无当前可修复项；有界 disposition 已通过，但不等于漏洞修复。db-tools 的 age 固定上游 `v1.3.1` 提交并以 `x/crypto v0.52.0` 构建，旧包的可修复项未获豁免。

## 3. 已完成功能

- Controller、PostgreSQL schema/IPAM、Enrollment、凭据和签名配置：M1.1/M1.2 证据。
- 本地运维 CLI：任务书九个命令全部实现；Unix 真实双 Agent 验证路径、路由、健康和控制面重连，Windows Named Pipe client 通过 MSVC target check/Clippy。协议使用显式长度帧，不依赖 Unix EOF；`ping` 为认证 XSP/1 探测，`reconnect` 只影响 Controller WebSocket。
- XSP/1 身份认证、X25519、AEAD、抗重放、Key Epoch、双向 TUN/UDP：M1.3 证据 `/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`。
- NAT 候选、打洞、Relay fallback/failover、ACL、子网路由和 Console：M2–M4 证据及三轮回归 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`。
- Linux 安装、升级、回滚、卸载、备份恢复和部署隔离：M5.1/M5.2 证据；备份又完成 age 流式加密、不同文件系统自动复制、深度认证、取回、保留和销毁墓碑。
- Windows xsnet ABI、队列/生命周期源码边界、Rust session、命名管道 server/CLI client、私有存储、Service/SCM 隔离：M6.1 证据 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T184122Z` 及最终本地审计 `/srv/xs-nexus/artifacts/qa/final-local-audit-20260802T102009Z`。测试签名驱动又在 Windows 11 24H2 VM 完成 WDK build、install、SYSTEM Tx/Rx、PnP restart、standard/UMDF/Application Verifier 和 clean uninstall；见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。
- Windows 路由准备：精确 LUID、IP Helper、DAD、manifest、additions-first、补偿和恢复；`make test-windows-agent-routing` 通过 16 个单测、源码门禁、MSVC target check 和 Clippy。runtime 仍隔离。
- Windows Wintun 发布路径：固定官方 0.14.1 x64 DLL、归档/DLL SHA-256、`CN=WireGuard LLC` Authenticode 和许可证；Windows 11 VM 已完成真实 adapter/session smoke 与离线 r3 包完整性验证。生产 Controller 回环及 `vpn.qinwen.co` 公网已提供精确 bootstrap/manifest/ZIP 与 404 allowlist；当前缺少可控 Windows VM 会话，尚未完成在线 Enrollment、服务和数据面验收。
- 性能基线：XSP/1 161,314 seal+open/s；Relay 87,822 包/s；Controller 100/500/1000 节点规模；release Agent Direct/Relay RTT 0.91/1.16 ms；Agent 空闲 CPU 0.55% 单核、RSS 7.95 MiB。证据见 `docs/PERFORMANCE_REPORT.md`。
- 签名更新：离线签名不可变发布、三通道确定性灰度、节点签名状态、Controller 签名通道、Agent/root helper 双重验证和原子回滚；见 `docs/UPDATE_SYSTEM.md`。
- 认证遥测：Agent 节点身份签名当前路径、业务流量、握手与 RTT；Relay 目录身份签名脱敏累计指标；Controller 验签、拒绝重放/回滚并保存有界 25 小时窗口，Console 展示新鲜/陈旧状态。
- 24 小时稳定性：三服务各 1420 次采样、一次受控 PID 转换、零额外自动重启；修正重启语义后的真实回归为 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z`。

## 4. 部分完成功能

- 容器镜像供应链：113/113 个已安装 OS 包均有逐包许可证闭包；镜像 digest、OS SBOM、构建来源、Grype 报告和 disposition 已验证。当前报告包含 2 Critical、4 High、16 Medium、24 Negligible，均无可修复版本；glibc 处置有效期至 2026-08-31，变化时需提前复核。
- Windows 路由：模型和平台 FFI 已完成静态/交叉验证；驱动设备、LUID 和 PnP 已有实机证据，但完整 Agent/route manager 尚未链接，没有真实 IP Helper、DAD、路由事务、manifest 恢复或睡眠证据。

## 5. 未完成功能与外部门禁

- Windows 测试签名驱动环境门禁 `BLK-001` 已解除；完整 Agent、SCM/Named Pipe/存储、route/DAD/sleep 与生产安装器仍由 `KI-016`、`KI-017`、`KI-018`、`KI-020` 跟踪。
- Windows 签名边界已明确：首版只发布发行方签名的 Wintun 0.14.1；自研 xsnet 保持测试签名实验路径，不宣称正式签名或生产分发。
- 真实 NAS 安装、升级、Direct/Relay、子网审批和离线撤销：`BLK-002`。
- 当前生产候选数据库公网暴露已解除：项目 PostgreSQL 无 host binding，外部数据库端口不可达；临时凭据轮换仍是发布前独立要求。
- DNS、生产防火墙、正式公网容量和第三方安全审计：人工/外部门禁。
- 正式离线发布签名、公钥认证分发/轮换和 RC 更新回滚：`BLK-006`。
- 正式备份 identity、真实异地主机/对象存储挂载和生产恢复演练：`BLK-007`。

## 6. 架构概览

- Controller：Axum/SQLx/PostgreSQL，控制面、Enrollment、签名配置和 WebSocket 广播。
- Relay：认证 Lease、限速、有界队列、XSR/1 转发和脱敏指标。
- Agent：Linux TUN/Netlink/XSP/1；Windows xsnet 驱动已通过专用 SYSTEM harness，但 Rust session/route preparation 仍作为隔离准备层，未接入 runtime。
- XSP/1：Ed25519 身份、X25519 会话、HKDF、ChaCha20-Poly1305、重放窗口和 Key Epoch。
- IPAM/ACL/路由：Controller 策略签名与 Agent 双端执行；Linux route manager 已实机隔离验证，Windows route manager 已静态/交叉验证。
- Console：React/Vite，显示 Controller 已知事实和不可用原因。
- 更新：Controller 只持发布公钥；不可变签名清单、灰度策略、签名运行时报告、Agent staging、无网络 root helper 和原子回滚已实现；正式离线签名仪式未完成。

## 7. 部署结果

- 当前生产候选服务器 Docker Compose 使用既有 external `1panel-network`，未重建或修改该网络。
- 项目 PostgreSQL 使用仓库外 secret 文件、无宿主端口映射；外部 TCP `3306`、`5432`、`6379` 不可达，`KI-006`/`BLK-005` 已解除。
- Controller/Relay/Console 非 root、健康检查、日志轮转，以及 age 加密/自动复制/取回/保留/恢复演练已通过。
- Chrony/NTP 已恢复，系统时钟修正 86400.300154 秒后容器、网络、路由和 nftables 不变。当前 `vpn.qinwen.co` 公网下载通过；任务书计划域名 `vpn.xiashikeji.cn` 的 TLS/DNS/CDN 和生产防火墙仍为外部门禁。不记录任何密码或 token。

## 8. 测试

- 单元/集成/协议负向/NAT/Relay/ACL/子网路由/Playwright/视觉/安装升级回滚卸载：已有 M0–M7 证据和三轮回归。
- Windows 路由专项：`make test-windows-agent-routing` 通过；xsnet 驱动另有 Windows 实机证据，但不代表 route manager 实机。
- CLI/IPC 专项：`make test-windows-agent-ipc`、CLI/Core/Agent 测试和真实 `test-agent-candidate-path` 通过；Windows 结果仍只是交叉编译，不代表命名管道实机。
- 性能：`/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z`、`relay-throughput-20260731T211903Z`、`agent-rtt-20260731T221036Z`、Controller scale 和报告中列出的证据。
- 稳定性：24 小时长样本与修正后的完整部署/重启回归已审计，详见 `docs/PERFORMANCE_REPORT.md`。
- 未运行/无法运行：完整 Windows Agent/SCM/route/DAD/sleep、真实 NAS、公网跨地域 Relay、正式签名和生产防火墙验证。WDK/Windows xsnet VM/Verifier 已运行。

## 9. 缺陷与风险

- P0/P1：当前自动化回归无新增 P0/P1；这不替代第三方安全审计。
- 高风险开放项：Windows 在线 Agent/service/network、真实 NAS、正式密钥/异地恢复、源站 TLS/防火墙和第三方协议/密码学审计。
- 镜像漏洞 disposition 已有明确结论，但不等于漏洞修复或自动接受。

## 10. 安全结论

身份、密钥、抗重放、ACL、Relay 明文隔离、更新哈希/签名和 secret 仓库外边界均有自动化证据；Windows 测试签名驱动已有 standard/UMDF/Application Verifier 证据，但完整 Agent 与正式驱动供应链仍未完成，第三方协议/密码学审计尚未执行。

## 11. 性能

- XSP/1：161,314 seal+open/s，184.61 MiB/s。
- Relay：87,822 包/s，18.09 MiB/s，内部平均 4 µs。
- Agent RTT：release Direct 平均 0.91 ms，Relay 平均 1.16 ms，平均增量 0.25 ms。
- Agent 空闲资源：0.55% 单核 CPU、7.95 MiB RSS、9 线程、15 FD。
- 限制：24 小时结果证明当前单机 Linux 容器基线稳定，不代表公网容量、运营商 NAT、多机水平扩展或 Windows/NAS 稳定性。

## 12. 当前生产适用性

- 个人隔离测试：适合。
- 小规模可信设备：Linux 测试范围内可继续验证，但需接受未完成门禁。
- 公网生产：不适合，需先完成源站 TLS/防火墙、Windows 在线 Agent/NAS、真实备份异地主机/密钥仪式、正式发布签名和第三方审计门禁。
- 企业关键网络：不适合，另需第三方协议/密码学审计、真实故障演练和正式供应链复核。

## 13. 凭据轮换

项目完成或发布前必须轮换服务器、NAS、PostgreSQL、Redis、Web 管理员、Controller 和 Enrollment Token 凭据。报告不记录任何新凭据。

## 14. 后续优先级

1. 在当前 `vpn.qinwen.co` 入口完成 Windows 11 VM 在线安装、服务、数据面、卸载和重装；并由人工修复 `BLK-003` 的计划域名源站 TLS/SNI/反向代理与最小防火墙。
2. 在已验证的 Windows 11 VM 使用一次性 Enrollment Token 执行真实在线安装、SCM/Named Pipe/存储、route/DAD/sleep、Agent crash、普通网络、卸载和重装门禁。
3. 完成 `BLK-007` 的正式备份 identity、真实异地主机恢复演练，以及 `BLK-006` 的正式离线发布签名和最终 RC 供应链复核。

## 15. 复现入口

- 启动与约束：`start.md`、`BOOTSTRAP_PROMPT.md`、`AGENTS.md`。
- Linux 全量验证：`make validate-m61-agent-session` 及各 `validate-m*` 目标。
- Windows 驱动 VM：`docs/WINDOWS_XSNET_VM_EVIDENCE.md`；Windows 路由准备：`make test-windows-agent-routing`。
- 性能报告：`docs/PERFORMANCE_REPORT.md`。
- 阻塞清单：`BLOCKERS.md`。
