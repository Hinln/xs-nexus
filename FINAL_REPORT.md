# XS Nexus 最终交付报告

报告日期：2026-08-02  
当前检查点：签名更新与 24 小时稳定性验证工作树（以本报告后续提交为准）

## 1. 结论

- [ ] 完整 Release Candidate
- [x] 部分完成
- [ ] 不适合生产
- [ ] 仅研究/原型

核心 Linux/Controller/Relay/Console、安装生命周期、签名灰度更新、认证加密备份/复制边界、24 小时稳定性、协议安全边界和 Windows 路由准备已完成可复现验证；项目仍受 Windows VM/WDK/正式签名、真实 NAS、数据库公网端口整改、正式备份 identity/真实异地主机和第三方审计等门禁约束，不能标记 Release Candidate 或描述为公网生产就绪。

## 2. 版本和构建

- Git：以本报告后续提交为准；签名更新先在隔离工作树和临时远程克隆完成验证。
- 环境：Ubuntu x86_64 开发服务器，Rust workspace；Linux 与隔离 network namespace 测试。
- 构建：Linux x86_64/aarch64 发布构建与 Windows 相关最小 crate 的 `x86_64-pc-windows-msvc` target check 已验证。
- 签名：Linux 测试签名流程已验证；正式离线签名密钥和签名仪式未完成。
- SBOM：源码 CycloneDX/SPDX 与四个容器镜像 OS 包逐包许可证闭包、Dockerfile/revision/image digest 和 in-toto/SLSA provenance 已生成；证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z`。固定 Grype 0.116.1 报告为 Critical 2、High 4、Medium 16、Negligible 24，无当前可修复项；有界 disposition 已通过，但不等于漏洞修复。

## 3. 已完成功能

- Controller、PostgreSQL schema/IPAM、Enrollment、凭据和签名配置：M1.1/M1.2 证据。
- XSP/1 身份认证、X25519、AEAD、抗重放、Key Epoch、双向 TUN/UDP：M1.3 证据 `/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`。
- NAT 候选、打洞、Relay fallback/failover、ACL、子网路由和 Console：M2–M4 证据及三轮回归 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`。
- Linux 安装、升级、回滚、卸载、备份恢复和部署隔离：M5.1/M5.2 证据；备份又完成 age 流式加密、不同文件系统自动复制、深度认证、取回、保留和销毁墓碑。
- Windows xsnet ABI、队列/生命周期源码边界、Rust session、命名管道、私有存储、Service/SCM 隔离：M6.1 证据 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`。
- Windows 路由准备：精确 LUID、IP Helper、DAD、manifest、additions-first、补偿和恢复；`make test-windows-agent-routing` 通过 16 个单测、源码门禁、MSVC target check 和 Clippy。runtime 仍隔离。
- 性能基线：XSP/1 161,314 seal+open/s；Relay 87,822 包/s；Controller 100/500/1000 节点规模；release Agent Direct/Relay RTT 0.91/1.16 ms；Agent 空闲 CPU 0.55% 单核、RSS 7.95 MiB。证据见 `docs/PERFORMANCE_REPORT.md`。
- 签名更新：离线签名不可变发布、三通道确定性灰度、节点签名状态、Controller 签名通道、Agent/root helper 双重验证和原子回滚；见 `docs/UPDATE_SYSTEM.md`。
- 认证遥测：Agent 节点身份签名当前路径、业务流量、握手与 RTT；Relay 目录身份签名脱敏累计指标；Controller 验签、拒绝重放/回滚并保存有界 25 小时窗口，Console 展示新鲜/陈旧状态。
- 24 小时稳定性：三服务各 1420 次采样、一次受控 PID 转换、零额外自动重启；修正重启语义后的真实回归为 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z`。

## 4. 部分完成功能

- 容器镜像供应链：113/113 个已安装 OS 包均有逐包许可证闭包；镜像 digest、OS SBOM、构建来源、Grype 报告和 disposition 已验证。当前报告包含 2 Critical、4 High、16 Medium、24 Negligible，均无可修复版本；glibc 处置有效期至 2026-08-31，变化时需提前复核。
- Windows 路由：模型和平台 FFI 已完成静态/交叉验证，但没有 Windows SDK/WDK 编译、真实 IP Helper、DAD、PnP、睡眠恢复或设备实机证据。

## 5. 未完成功能与外部门禁

- Windows 11 测试 VM、WDK、测试签名、Driver Verifier、安装/卸载/崩溃/蓝屏和完整 Agent 链接：`BLK-001`。
- 正式 Windows 驱动签名：`BLK-004`。
- 真实 NAS 安装、升级、Direct/Relay、子网审批和离线撤销：`BLK-002`。
- 现有 PostgreSQL/Redis 公网端口整改和临时凭据轮换：`BLK-005`。
- DNS、生产防火墙、正式公网容量和第三方安全审计：人工/外部门禁。
- 正式离线发布签名、公钥认证分发/轮换和 RC 更新回滚：`BLK-006`。
- 正式备份 identity、真实异地主机/对象存储挂载和生产恢复演练：`BLK-007`。

## 6. 架构概览

- Controller：Axum/SQLx/PostgreSQL，控制面、Enrollment、签名配置和 WebSocket 广播。
- Relay：认证 Lease、限速、有界队列、XSR/1 转发和脱敏指标。
- Agent：Linux TUN/Netlink/XSP/1；Windows xsnet/session/route preparation 只作为隔离准备层，未接入 runtime。
- XSP/1：Ed25519 身份、X25519 会话、HKDF、ChaCha20-Poly1305、重放窗口和 Key Epoch。
- IPAM/ACL/路由：Controller 策略签名与 Agent 双端执行；Linux route manager 已实机隔离验证，Windows route manager 已静态/交叉验证。
- Console：React/Vite，显示 Controller 已知事实和不可用原因。
- 更新：Controller 只持发布公钥；不可变签名清单、灰度策略、签名运行时报告、Agent staging、无网络 root helper 和原子回滚已实现；正式离线签名仪式未完成。

## 7. 部署结果

- 开发服务器 Docker Compose 使用既有 external `1panel-network`，未重建或修改该网络。
- PostgreSQL/Redis 使用仓库外 secret 文件；项目未暴露新的生产端口。
- Controller/Relay/Console 非 root、健康检查、日志轮转，以及 age 加密/自动复制/取回/保留/恢复演练已通过。
- 生产 TLS/DNS/防火墙和数据库公网端口仍为外部门禁；不记录任何密码或 token。

## 8. 测试

- 单元/集成/协议负向/NAT/Relay/ACL/子网路由/Playwright/视觉/安装升级回滚卸载：已有 M0–M7 证据和三轮回归。
- Windows 路由专项：`make test-windows-agent-routing` 通过；不代表 Windows 实机。
- 性能：`/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z`、`relay-throughput-20260731T211903Z`、`agent-rtt-20260731T221036Z`、Controller scale 和报告中列出的证据。
- 稳定性：24 小时长样本与修正后的完整部署/重启回归已审计，详见 `docs/PERFORMANCE_REPORT.md`。
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
- 限制：24 小时结果证明当前单机 Linux 容器基线稳定，不代表公网容量、运营商 NAT、多机水平扩展或 Windows/NAS 稳定性。

## 12. 当前生产适用性

- 个人隔离测试：适合。
- 小规模可信设备：Linux 测试范围内可继续验证，但需接受未完成门禁。
- 公网生产：不适合，需先完成数据库暴露整改、正式签名、Windows/NAS、真实备份异地主机/密钥仪式和第三方审计门禁。
- 企业关键网络：不适合，另需第三方协议/密码学审计、真实故障演练和正式供应链复核。

## 13. 凭据轮换

项目完成或发布前必须轮换服务器、NAS、PostgreSQL、Redis、Web 管理员、Controller 和 Enrollment Token 凭据。报告不记录任何新凭据。

## 14. 后续优先级

1. 完成 `BLK-007` 的正式备份 identity 仪式、真实异地主机挂载和生产恢复演练。
2. 获取 Windows VM/WDK/签名环境，执行 M6.1 实机门禁并决定是否接入 runtime。
3. 完成数据库公网端口整改、正式离线签名和最终 RC 供应链复核。

## 15. 复现入口

- 启动与约束：`start.md`、`BOOTSTRAP_PROMPT.md`、`AGENTS.md`。
- Linux 全量验证：`make validate-m61-agent-session` 及各 `validate-m*` 目标。
- Windows 路由准备：`make test-windows-agent-routing`。
- 性能报告：`docs/PERFORMANCE_REPORT.md`。
- 阻塞清单：`BLOCKERS.md`。
