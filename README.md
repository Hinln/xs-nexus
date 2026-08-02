# 拾枢（XS Nexus）

拾枢是独立设计和实现的三层安全内网互通系统。Linux 控制面、Agent、Relay、Web Console、NAT 路径选择、ACL、子网路由、安装生命周期和隔离部署已经形成自动化证据；Windows 驱动与 Agent 仍处于源码和交叉编译准备阶段，尚未通过 Windows 11 VM、WDK、测试签名或 Driver Verifier 实机门禁。

当前项目是“部分完成”，不是 Release Candidate，也不适合直接投入公网生产或企业关键网络。权威状态见 `PROGRESS.md`、`ACCEPTANCE.md`、`BLOCKERS.md` 和 `FINAL_REPORT.md`。

## 组件

- `xs-controller`：Enrollment、节点凭据、IPAM、签名配置、策略和控制连接。
- `xs-relay`：认证租约、有界队列、速率限制、XSR/1 密文转发和脱敏指标。
- `xs-agent`：Linux TUN/Netlink、XSP/1、NAT 候选、Direct/Relay、ACL 和子网路由。
- `xs-cli`：Unix socket / Windows Named Pipe 本地状态、路径、路由、诊断和受限控制操作。
- `xs-console`：基于真实 Controller API 的管理控制台。
- `windows-xsnet`：自研 UMDF/NetAdapterCx 虚拟 NIC 源码、ABI、测试安装和 VM 采证流程；未通过实机验收。

核心运行路径不依赖 Tailscale、WireGuard/Wintun、ZeroTier、OpenVPN、FRP 或其他现成组网、VPN、穿透和中继产品。密码学使用成熟库中的标准原语；自研的是协议组合与系统实现，不自创密码算法。

## 仓库入口

- 架构：`docs/ARCHITECTURE.md`
- XSP/1 协议：`docs/XSP1_PROTOCOL.md`
- Controller API：`docs/CONTROLLER_API.md`
- 威胁模型与安全假设：`docs/THREAT_MODEL.md`、`docs/SECURITY_ASSUMPTIONS.md`
- Linux 安装、升级、回滚和卸载：`docs/LINUX_INSTALLATION.md`
- 签名更新、灰度策略和权限边界：`docs/UPDATE_SYSTEM.md`
- 1Panel 隔离部署及项目自带 HTTPS Edge：`docs/DOCKER_1PANEL_DEPLOYMENT.md`
- 生产部署、验收与运维总手册：`docs/PRODUCTION_DEPLOYMENT_GUIDE.md`
- 一键公网栈：`deploy/docker/xs-nexus-public-deploy.sh`（不依赖 1Panel 站点配置）
- 恢复手册：`RECOVERY_RUNBOOK.md`
- 性能与稳定性：`docs/PERFORMANCE_REPORT.md`
- 第三方依赖与许可证：`THIRD_PARTY.md`、`docs/IMAGE_SUPPLY_CHAIN.md`

## 开发验证

```bash
make setup
make fmt-check
make lint
make build
make test
make test-network
make test-e2e
make test-visual
make security-check
```

重要专项入口：

```bash
make validate-m52
make validate-m61-agent-session
make validate-image-supply-chain
make test-runtime-stability
```

测试会使用真实 PostgreSQL、Docker、Linux network namespace、TUN 和浏览器渲染等适用环境。Windows 源码门禁或 MSVC target check 不等同于 WDK 构建和 Windows 实机结果。

本地 CLI：

```text
xs status
xs peers
xs ping <virtual-ip>
xs path <virtual-ip>
xs routes
xs netcheck
xs diagnostics
xs reconnect
xs version
```

`ping` 使用已认证 XSP/1 路径探测，不发送明文 ICMP 探测；`reconnect` 只请求重建 Controller 控制连接。读取结果和 JSON 输出不包含私钥、凭证或 Token。

## 构建与部署边界

- Controller、Relay、Console 和数据库运维工具使用 Docker；普通服务默认非 root。
- Linux Agent 使用宿主机 systemd、`/dev/net/tun` 和 Netlink，不依赖 `ip` 命令完成核心运行逻辑。
- Compose 只引用既有 external `1panel-network`，不得创建、删除、断开或修改该网络。
- 不得执行 Docker prune，不得修改未知 1Panel 容器、数据库、Redis、卷、默认路由或生产防火墙。
- 网络实验必须位于项目可回收的 namespace，并验证默认路由、nftables、TUN 和项目资源无残留。

真实配置位于仓库外的 `/etc/xs-nexus/controller.env`。不得向 Git、Markdown、日志、截图或测试产物写入 SSH、数据库、Redis、JWT、Enrollment、签名私钥或其他真实秘密。

## 当前证据与限制

- Linux 全链路与三轮聚合回归：`/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`
- 九个 CLI、长度帧 IPC 和真实双 Agent 路径/重连：`/srv/xs-nexus/artifacts/qa/m1.2-cli-completion-20260802T100106Z` 及 `/srv/xs-nexus/artifacts/qa/final-local-audit-20260802T102009Z`
- Windows 路由和 Agent 隔离准备：`/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T184122Z`
- 运行镜像 SBOM、113/113 包许可证闭包、provenance 和漏洞报告：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`
- 24 小时稳定性长测：`/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T212242Z`，4261 行资源样本覆盖三服务各 1420 次采样；受控重启 PID 转换、零自动重启和资源/日志上限已审计。
- 修正重启证据语义后的真实回归：`/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z`，三服务各 11 次采样、恰好一次 PID 转换且 `RestartCount` 全程为零。
- 数据库备份采用 age X25519 流式认证加密、不同文件系统自动复制、离线 identity 深度校验、取回、保留和销毁墓碑；正式异地主机与密钥仪式仍由 `BLK-007` 阻塞。

当前不可绕过的门禁包括 Windows 11 VM/WDK/Driver Verifier、正式驱动签名、真实 NAS、既有数据库公网端口整改、DNS/生产防火墙、正式离线签名和第三方协议/密码学审计。不得将这些项目描述为已完成。
