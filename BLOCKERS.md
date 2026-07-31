# BLOCKERS.md — 外部阻塞

只有无法由代码、配置、测试或本地环境自行解决的外部条件才能写入本文件。

---

## BLK-001 Windows 测试环境

- 状态：阻塞 Windows 实机验收
- 当前环境核对：开发服务器仅有 `clang-cl`、CMake 和 Ninja，没有 WDK、MSBuild、Windows SDK、UMDF/NetAdapterCx 构建目标、测试签名模式或 Windows VM；因此不能生成或声称测试签名驱动包。
- 新增可执行准备：已安装与仓库 Rust 1.93.1 匹配的官方最小 rustup 工具链、`x86_64-pc-windows-msvc` 标准库、Clippy 和 rustfmt，用于不依赖 SDK 链接的最小 Windows crate check；本地 IPC crate 已通过，但完整 Agent 仍准确停在 `ring` 查找 `lib.exe`，因此该准备不解除 Windows SDK/WDK/VM 门禁。
- 需要：
  - Windows 11 测试 VM；
  - 快照；
  - Visual Studio Build Tools；
  - WDK；
  - 测试签名模式；
  - Driver Verifier。
- 不阻塞：
  - Linux 核心；
  - Controller；
  - Relay；
  - Console；
  - 驱动代码和交叉构建准备。
- 已完成的不受阻塞工作：按 Windows 11 LTSC 2024 门禁选择 UMDF 2.33 + NetAdapterCx 2.5，完成驱动/Agent ABI、单 owner、同步 direct-I/O、有界 IPv4 队列、ring copy、PnP/power 源码、隔离 `no_std + alloc` Win32 transport、无后台重试的单步 Agent 会话适配、测试专用安装/卸载、显式 WDK 测试包构建、六阶段 VM 采证和 exact ABI v1/DriverVer/driver-store/受限状态一致性门禁；transport 已真实通过 MSVC target check 与交叉 Clippy，Clang Release、ASan/UBSan、workspace 单测、真实 PostgreSQL Agent 控制面、Agent Clippy、源码不变量和本地 PowerShell 语法验证已通过。空 TX/满 RX 的 Win32 权威拒绝映射尚需 VM 证明，因此适配层不接入 runtime；完整 Agent 在缺少 Windows SDK C 头时无法构建，测试安装器只支持 clean install，不冒充热升级或生产回滚；VM 编排不自动重启、不修改 BCD、不声称场景验收。全部 Windows API 和设备操作尚未执行。Windows 10 支持矩阵冲突单列为 `KI-016`，不作兼容声明。
- 解除步骤：用户提供可测试 VM，Codex执行安装和验证。

---

## BLK-002 NAS 首次接入

- 状态：阻塞真实 NAS 验收
- 原因：NAS 位于 `192.168.0.100` 私网，公网服务器不能直接访问。
- 需要：用户在 NAS 本地执行签名安装命令。
- 已完成的不受阻塞工作：M3.2 已在隔离 namespace 中验证子网建议、审批、纯路由/NAT、ACL、离线撤销和回滚；M5.1 已完成测试签名 x86_64/aarch64 包、安装、升级、回滚、卸载和身份保留。证据为 `/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z` 与 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。
- 不阻塞：完整 NAS 安装包、文档、控制台子网审批和虚拟测试。
- 解除：用户从独立可信渠道取得正式发布公钥和签名包，在 NAS 本地完成普通节点安装、主动注册、Direct/Relay、升级和卸载验证后，再审批真实子网。

---

## BLK-003 DNS 和生产防火墙

- 状态：阻塞正式域名上线
- 需要：用户批准 DNS 和端口。
- 不阻塞：IP 和临时端口测试。

---

## BLK-004 正式驱动签名

- 状态：阻塞正式 Windows 公布
- 不阻塞：测试签名和测试 VM。
- 解除：完成签名流程。

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

## BLK-005 现有数据库公网暴露整改门禁

- 状态：阻塞最终 1Panel 安全验收和 Release Candidate
- 首次发现：2026-07-29
- 外部条件：用户批准修改现有 1Panel 端口映射或云防火墙规则，并安排临时凭据轮换。
- 证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt`
- 已尝试：完成只读 Docker、监听端口和外部连通性检查；未修改现有资源。
- 不受影响工作：仓库开发、隔离 namespace 实验、Controller/Agent/Relay/Console 实现和本地测试。
- 解除步骤：限制 TCP `5432`、`6379` 的公网访问，保留 `1panel-network` 容器内访问，轮换临时数据库和 Redis 密码。
- 解除后验证：外部端口不可达、容器 DNS 和内部端口可达、现有 1Panel 服务健康、项目数据库连接通过。
