# BLOCKERS.md — 外部阻塞

只有无法由代码、配置、测试或本地环境自行解决的外部条件才能写入本文件。

---

## BLK-001 Windows 测试环境

- 状态：已解除 Windows 11 24H2 测试签名驱动环境门禁（2026-08-04）；完整 Agent 与生产分发边界转由已列明的 KI 和 `BLK-004` 跟踪
- 环境核对：用户提供的 NAS KVM 快照 VM 已安装 Visual Studio 2022 Build Tools、Windows SDK/WDK 26100、PowerShell 7 和 Driver/Application Verifier；最终通过后测试包已卸载，证据导出并准备恢复干净快照。
- 新增可执行准备：已安装与仓库 Rust 1.93.1 匹配的官方最小 rustup 工具链、`x86_64-pc-windows-msvc` 标准库、Clippy 和 rustfmt，用于不依赖 SDK 链接的最小 Windows crate check；本地 IPC、私有存储和 Service crate 已通过。该交叉准备本身当时没有解除 Windows SDK/WDK/VM 门禁，后续实机驱动证据才解除该环境门禁；完整 Agent 仍未链接。
- 已提供并验证：Windows 11 测试 VM、快照、Visual Studio Build Tools、Windows SDK/WDK、测试签名信任、standard Driver Verifier、UMDF Verifier 和 Application Verifier。
- 不阻塞：
  - Linux 核心；
  - Controller；
  - Relay；
  - Console；
  - 驱动代码和交叉构建准备。
- 已完成的不受阻塞工作：按 Windows 11 LTSC 2024 门禁选择 UMDF 2.33 + NetAdapterCx 2.5，完成驱动/Agent ABI、单 owner、同步 direct-I/O、有界 IPv4 队列、ring copy、PnP/power 源码、隔离 Win32 transport、无后台重试的单步 Agent 会话、安全命名管道 server 与 `xs-cli` client、显式长度帧、私有存储、固定名称 Service/SCM、IP Helper/DAD、可信同句柄 LUID、精确路由事务/manifest/恢复、Windows-only Agent 准备编排、测试专用安装/卸载、显式 WDK 测试包构建、六阶段 VM 采证和 exact ABI v1/DriverVer/driver-store/受限状态一致性门禁；相关最小 Windows crate 和 CLI 已真实通过 MSVC target check 与交叉 Clippy，Clang Release、ASan/UBSan、workspace 单测、真实 PostgreSQL Agent 控制面、源码不变量和 PowerShell 语法验证已通过。VM harness 已真实执行 Configuration Manager 枚举、独占设备打开、七个 IOCTL、Ethernet/IPv4 TX/RX、LUID、PnP restart、Verifier 和卸载；但空 TX/满 RX 的 Win32 权威拒绝映射、完整 Agent、Named Pipe、SCM、ACL、原子替换、IP Helper/DAD、service token、睡眠和生产更新仍未实机验证，因此适配层和路由准备不接入 runtime。测试安装器只支持 clean install，不冒充热升级或生产回滚；Windows 10 支持矩阵冲突单列为 `KI-016`，不作兼容声明。
- 解除证据：`docs/WINDOWS_XSNET_VM_EVIDENCE.md`。测试包 `15.39.27.376` 的构建/签名、clean install、SYSTEM Tx/Rx、PnP restart、standard/UMDF/Application Verifier、三轮重复收发和 clean uninstall 通过；新增 WDF/NDIS/相关 WER/错误事件均为 0。
- 剩余边界：不把该门禁解除解释为 Windows 客户端已可生产发布。正式签名由 `BLK-004` 阻塞；生产安装器、完整 Agent、SCM/Named Pipe/存储、路由/DAD/睡眠和 Windows 10 分别由 `KI-016`、`KI-017`、`KI-018`、`KI-020` 跟踪。

---

## BLK-002 NAS 首次接入

- 状态：阻塞真实 NAS 验收
- 原因：NAS 位于 `192.168.0.100` 私网，公网服务器不能直接访问。
- 需要：用户在 NAS 本地执行签名安装命令。
- 已完成的不受阻塞工作：M3.2 已在隔离 namespace 中验证子网建议、审批、纯路由/NAT、ACL、离线撤销和回滚；M5.1 已完成测试签名 x86_64/aarch64 包、安装、升级、回滚、卸载和身份保留。证据为 `/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z` 与 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。
- 不阻塞：完整 NAS 安装包、文档、控制台子网审批和虚拟测试。
- 解除：用户从独立可信渠道取得正式发布公钥和签名包，在 NAS 本地完成普通节点安装、主动注册、Direct/Relay、升级和卸载验证后，再审批真实子网。

---

## BLK-003 DNS/CDN 与严格源站 TLS

- 状态：阻塞任务书计划域名 `vpn.xiashikeji.cn` 上线；2026-08-08 最终复核中边缘 TLS 可完成且根路径返回 404，但 `/health/ready` 与 `/install` 仍返回 `525 SSL Handshake Failed with Origin Server`
- 需要：用户批准并在 1Panel/CDN 中完成正式源站证书、SNI、反向代理和 strict TLS 配置。
- 已验证：服务器错误时钟已恢复 Chrony/NTP，同步后功能路径 525 仍可复现；Controller 回环健康，当前固定 `vpn.qinwen.co` 的健康入口、Linux/Windows 公网引导、全部 10 个发布文件逐字节比较和未知文件 404 均通过，因此不能把 525 归因于应用路由。证据 `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z/public-checks.txt`。
- 已完成：生产最小 INPUT 防火墙和 1Panel TCP `188` 公网关闭已由 Gate 14 独立完成，不再属于本阻塞项；不得为了修复 525 放宽该策略或关闭 TLS 验证。
- 不阻塞：当前 `vpn.qinwen.co` 测试发布、IP 和临时端口测试。

---

## BLK-004 正式驱动签名

- 状态：首版 Wintun 发布路径已解除；自研 xsnet 正式分发仍未签名且不纳入当前发布声明
- 决策：项目所有者明确允许使用 GitHub 开源、已有发行方签名的 Windows 底层适配器，并确认没有自研驱动正式签名条件。首版固定 Wintun 0.14.1、归档/DLL 哈希、`CN=WireGuard LLC` Authenticode 和许可证；XS Nexus 仍独立实现控制、身份、XSP/1、加密、ACL、路由、NAT 和 Relay。
- 证据：ADR-077、`THIRD_PARTY.md`、`docs/WINDOWS_INSTALLATION.md` 和 Windows 11 VM 离线包/Wintun smoke 证据。
- 剩余：`xsnet` 只能作为测试签名实验路径；不得宣称它已正式签名。Windows 在线安装、服务、路由、睡眠和普通网络恢复仍由 `KI-022` 等条目跟踪。

---

## 新阻塞模板

```markdown
## BLK-NNN 标题

- 状态：
- 首次发现：
- 外部条件：
- 证据：
- 已尝试：
- 不受影响工作：
- 解除步骤：
- 解除后验证：
```

---

## BLK-005 现有数据库公网暴露整改门禁（已解除）

- 状态：已解除（2026-08-08）
- 首次发现：2026-07-29
- 外部条件：用户批准修改现有 1Panel 端口映射或云防火墙规则，并安排临时凭据轮换。
- 证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt`
- 已尝试：完成只读 Docker、监听端口和外部连通性检查；未修改现有资源。
- 不受影响工作：仓库开发、隔离 namespace 实验、Controller/Agent/Relay/Console 实现和本地测试。
- 解除结果：新生产候选服务器没有 Redis/MySQL 项目容器，项目 PostgreSQL 无 host binding；外部 TCP `3306`、`5432`、`6379` 不可达，项目四容器健康，`1panel-network` 保持 4 个项目成员。
- 解除证据：`/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z`。
- 后续：所有临时凭据仍必须轮换；该要求独立于端口暴露门禁。

---

## BLK-006 正式离线发布签名与公钥分发

- 状态：阻塞签名更新用于 Release Candidate 或生产节点
- 首次发现：2026-07-31（实现边界于 2026-08-02 完成）
- 外部条件：项目所有者批准离线密钥生成、双人授权、备份/恢复、轮换/撤回和独立认证公钥分发流程，并提供正式 HTTPS 发布存储。
- 已完成的不受阻塞工作：Controller 只持公钥的不可变发布/灰度策略、节点签名上报、Agent 与 root helper 双重验证、原子安装回滚、Console 管理及全套隔离测试已完成，见 `docs/UPDATE_SYSTEM.md`。
- 解除步骤：在不联网的受控环境生成正式根密钥，从固定干净提交构建并签名 RC；分别向 Controller 分发 raw 公钥、向 Agent/安装器分发 PEM 公钥，演练撤回、轮换和旧版本回滚。
- 解除后验证：公开材料可独立复核，私钥未出现在服务器/仓库/日志/证据，真实 Agent 完成 stable→testing→stable 灰度和失败回滚。

---

## BLK-007 正式备份密钥与异地主机

- 状态：阻塞生产数据库灾难恢复验收，不阻塞加密备份产品能力和隔离测试。
- 首次发现：2026-08-02（`KI-015` 产品实现完成后转为外部门禁）。
- 外部条件：项目所有者提供独立于数据库主机的受控设备生成并托管 age X25519 identity，批准真实异地主机/对象存储挂载及保留策略，并安排生产数据恢复窗口。
- 已完成的不受阻塞工作：数据库明文不落盘的流式加密、认证 manifest、密文 hash、不同文件系统 marker、自动复制/取回、离线 identity 深度校验、恢复回滚、保留和销毁墓碑均已实现并通过真实 PostgreSQL/Docker 隔离测试。
- 解除步骤：离线生成 identity，只向数据库主机分发 public recipient；把 `XS_BACKUP_REPLICA_DIR` 挂载到真实独立故障域，写入匹配 deployment/target marker；执行备份、断开本地副本、异地取回、深度校验和受控恢复演练。
- 解除后验证：私钥不在数据库主机、仓库、日志或证据；异地主机断连时失败关闭；恢复后的 schema/迁移/业务抽查通过；销毁墓碑和保留记录归档到审计系统。

---

## BLK-008 正式生产安全与运营门禁

- 状态：阻塞 Release Candidate 和正式生产；最终审计 `NO_GO`。
- 首次发现：2026-08-08 正式生产发布门禁审计。
- 外部条件：生产所有者完成剩余全量凭据轮换和旧值拒绝、计划 DNS/CDN/strict origin TLS；SRE 提供独立通知 provider/destination/credential 和正式 on-call；独立第三方完成安全审计和 retest。
- 已完成的不受阻塞工作：固定 CI/依赖/镜像、源 SBOM、真实 Console E2E、协议 fuzz、五镜像可复现、五分钟本地健康守卫、PostgreSQL 最小权限、仅密钥 SSH、最小 INPUT 防火墙、1Panel 公网管理端口关闭、全部宿主安全更新、受自动 fallback 保护的新内核 reboot 和有界磁盘清理。当前根分区 `77%`，0 pending upgrade、0 failed unit、无 reboot-required。
- 磁盘告警外部阻塞：没有批准的独立目的地和 on-call 时不能安全配置或证明真实送达；`external-disk-alert-status.txt` 明确为 `BLOCKED_EXTERNAL`，本机日志不计作通过。
- 证据：`audit/production-readiness/GO_NO_GO_FINAL.md`、`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z`、Gate 16 生产证据、Gate 14 防火墙证据和 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z`。
- 解除后验证：旧凭据全部被拒绝；SSH/端口/防火墙回归；正式域名 API/WS/Console/health/TLS 通过；应用角色非 superuser；外部告警真实送达；第三方 findings 修复并 retest；重新执行全部 Hard Gate。
