# XS Nexus 最终交付报告

报告日期：2026-07-31  
当前检查点：`9c89909 fix(windows): tighten route ownership semantics`

## 1. 结论

- [ ] 完整 Release Candidate
- [x] 部分完成
- [ ] 不适合生产
- [ ] 仅研究/原型

核心 Linux/Controller/Relay/Console、安装生命周期、协议安全边界和 Windows 路由准备已完成可复现验证；项目仍受 Windows VM/WDK/正式签名、真实 NAS、数据库公网端口整改和 24 小时稳定性长测等门禁约束，不能标记 Release Candidate 或描述为公网生产就绪。

## 2. 版本和构建

- Git：`9c89909`；工作树在本报告生成前保持干净。
- 环境：Ubuntu x86_64 开发服务器，Rust workspace；Linux 与隔离 network namespace 测试。
- 构建：Linux x86_64/aarch64 发布构建与 Windows 相关最小 crate 的 `x86_64-pc-windows-msvc` target check 已验证。
- 签名：Linux 测试签名流程已验证；正式离线签名密钥和签名仪式未完成。
- SBOM：源码 CycloneDX/SPDX 与容器镜像 OS 包 SBOM 已生成；漏洞报告使用固定 Grype 0.116.1 并绑定镜像 digest。完整许可证全文和最终构建来源证明仍未完成。

## 3. 已完成功能

- Controller、PostgreSQL schema/IPAM、Enrollment、凭据和签名配置：M1.1/M1.2 证据。
- XSP/1 身份认证、X25519、AEAD、抗重放、Key Epoch、双向 TUN/UDP：M1.3 证据 `/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`。
- NAT 候选、打洞、Relay fallback/failover、ACL、子网路由和 Console：M2–M4 证据及三轮回归 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`。
- Linux 安装、升级、回滚、卸载、备份恢复和部署隔离：M5.1/M5.2 证据。
- Windows xsnet ABI、队列/生命周期源码边界、Rust session、命名管道、私有存储、Service/SCM 隔离：M6.1 证据 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`。
- Windows 路由准备：精确 LUID、IP Helper、DAD、manifest、additions-first、补偿和恢复；`make test-windows-agent-routing` 通过 16 个单测、源码门禁、MSVC target check 和 Clippy。runtime 仍隔离。
- 性能基线：XSP/1 161,314 seal+open/s；Relay 87,822 包/s；Controller 100/500/1000 节点规模；release Agent Direct/Relay RTT 0.91/1.16 ms；Agent 空闲 CPU 0.55% 单核、RSS 7.95 MiB。证据见 `docs/PERFORMANCE_REPORT.md`。

## 4. 部分完成功能

- 性能报告和 24 小时稳定性测试：长测已启动，证据目录为 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T212242Z`，当前尚未生成最终 `summary.json`，不能提前勾选稳定性 MUST。
- 容器镜像供应链：镜像 digest、OS SBOM、Grype 报告和 disposition 已验证；当前报告包含 2 Critical、4 High、16 Medium、24 Negligible，均无可修复版本，需 RC 前逐项复核。
- Windows 路由：模型和平台 FFI 已完成静态/交叉验证，但没有 Windows SDK/WDK 编译、真实 IP Helper、DAD、PnP、睡眠恢复或设备实机证据。

## 5. 未完成功能与外部门禁

- Windows 11 测试 VM、WDK、测试签名、Driver Verifier、安装/卸载/崩溃/蓝屏和完整 Agent 链接：`BLK-001`。
- 正式 Windows 驱动签名：`BLK-004`。
- 真实 NAS 安装、升级、Direct/Relay、子网审批和离线撤销：`BLK-002`。
- 现有 PostgreSQL/Redis 公网端口整改和临时凭据轮换：`BLK-005`。
- DNS、生产防火墙、正式公网容量和第三方安全审计：人工/外部门禁。

## 6. 架构概览

- Controller：Axum/SQLx/PostgreSQL，控制面、Enrollment、签名配置和 WebSocket 广播。
- Relay：认证 Lease、限速、有界队列、XSR/1 转发和脱敏指标。
- Agent：Linux TUN/Netlink/XSP/1；Windows xsnet/session/route preparation 只作为隔离准备层，未接入 runtime。
- XSP/1：Ed25519 身份、X25519 会话、HKDF、ChaCha20-Poly1305、重放窗口和 Key Epoch。
- IPAM/ACL/路由：Controller 策略签名与 Agent 双端执行；Linux route manager 已实机隔离验证，Windows route manager 已静态/交叉验证。
- Console：React/Vite，显示 Controller 已知事实和不可用原因。
- 更新：Linux 测试签名包、哈希、回滚和身份保留；正式离线签名未完成。

## 7. 部署结果

- 开发服务器 Docker Compose 使用既有 external `1panel-network`，未重建或修改该网络。
- PostgreSQL/Redis 使用仓库外 secret 文件；项目未暴露新的生产端口。
- Controller/Relay/Console 非 root、健康检查、日志轮转和备份恢复演练已通过。
- 生产 TLS/DNS/防火墙和数据库公网端口仍为外部门禁；不记录任何密码或 token。

## 8. 测试

- 单元/集成/协议负向/NAT/Relay/ACL/子网路由/Playwright/视觉/安装升级回滚卸载：已有 M0–M7 证据和三轮回归。
- Windows 路由专项：`make test-windows-agent-routing` 通过；不代表 Windows 实机。
- 性能：`/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z`、`relay-throughput-20260731T211903Z`、`agent-rtt-20260731T221036Z`、Controller scale 和报告中列出的证据。
- 稳定性：`XS_STABILITY_DURATION_SECONDS=86400` 长测运行中，尚未完成最终审计。
- 未运行/无法运行：WDK/Windows VM/Driver Verifier、真实 NAS、公网跨地域 Relay、正式签名和生产防火墙验证。

## 9. 缺陷与风险

- P0/P1：当前自动化回归无新增 P0/P1；这不替代第三方安全审计。
- 高风险开放项：Windows 实机、正式签名、数据库公网暴露、真实 NAS、第三方协议/密码学审计。
- 镜像漏洞 disposition 已有明确结论，但不等于漏洞修复或自动接受。

## 10. 安全结论

身份、密钥、抗重放、ACL、Relay 明文隔离、更新哈希/签名和 secret 仓库外边界均有自动化证据；Windows 驱动安全和正式供应链仍未完成，第三方协议/密码学审计尚未执行。

## 11. 性能

- XSP/1：161,314 seal+open/s，184.61 MiB/s。
- Relay：87,822 包/s，18.09 MiB/s，内部平均 4 µs。
- Agent RTT：release Direct 平均 0.91 ms，Relay 平均 1.16 ms，平均增量 0.25 ms。
- Agent 空闲资源：0.55% 单核 CPU、7.95 MiB RSS、9 线程、15 FD。
- 限制：loopback/namespace 和单机基线不能代表公网容量、运营商 NAT 或长期稳定性。

## 12. 当前生产适用性

- 个人隔离测试：适合。
- 小规模可信设备：Linux 测试范围内可继续验证，但需接受未完成门禁。
- 公网生产：不适合，需先完成数据库暴露整改、正式签名、Windows/NAS 和长期稳定性门禁。
- 企业关键网络：不适合，另需第三方协议/密码学审计、真实故障演练和正式供应链复核。

## 13. 凭据轮换

项目完成或发布前必须轮换服务器、NAS、PostgreSQL、Redis、Web 管理员、Controller 和 Enrollment Token 凭据。报告不记录任何新凭据。

## 14. 后续优先级

1. 完成并审计 24 小时稳定性长测。
2. 获取 Windows VM/WDK/签名环境，执行 M6.1 实机门禁并决定是否接入 runtime。
3. 完成数据库公网端口整改、正式离线签名、许可证全文/构建来源和最终 RC 复核。

## 15. 复现入口

- 启动与约束：`start.md`、`BOOTSTRAP_PROMPT.md`、`AGENTS.md`。
- Linux 全量验证：`make validate-m61-agent-session` 及各 `validate-m*` 目标。
- Windows 路由准备：`make test-windows-agent-routing`。
- 性能报告：`docs/PERFORMANCE_REPORT.md`。
- 阻塞清单：`BLOCKERS.md`。
