# Agent 信任链基础

状态：M1.2 历史信任基础；后续能力见当前进展与专项文档
日期：2026-07-29

## 已实现边界

- 严格 Agent 配置解析；远程 Controller 必须使用 HTTPS/WSS，明文 HTTP/WS 仅允许精确 loopback 测试地址。
- 本地 Ed25519 身份生成、`0700` 状态目录、`0600` 私钥文件、符号链接拒绝和原子持久化。
- Enrollment 响应大小限制、无重定向、短超时，以及凭证、配置签名、网络、节点、地址和密钥用途的完整验证。
- WebSocket 每连接 challenge 持钥证明、4096 字节消息边界、配置单调更新、重连退避和最后有效配置保留。
- Controller 与 Agent 共用严格 wire DTO，避免两端模型漂移。
- Linux Agent 使用非持久 TUN FD；通过 Netlink 设置 MTU、`/32` 地址、接口状态和地址池路由，拒绝默认路由、过宽路由和接管同名接口。
- Agent 运行时统一管理控制循环、TUN 读取、Unix IPC 和 SIGTERM 清理；异常退出、Drop 和 stale manifest 恢复均不会在宿主机残留接口或项目路由。
- 本地 Unix IPC 使用 `0600` socket、32 位大端长度帧、严格 JSON 和并发限制；当前九个 CLI 命令均不返回凭证或密钥材料。`ping` 只触发认证 XSP/1 探测，`reconnect` 只触发有冷却和确认的 Controller WebSocket 重连；Windows 使用同一协议的受限命名管道，见 `WINDOWS_AGENT_LOCAL_IPC.md`。
- systemd 单元使用专用 `xs-nexus` 用户、`CAP_NET_ADMIN`、`/dev/net/tun` 设备白名单、`NoNewPrivileges` 和只读系统目录。

## 验证证据

- M1.2 全量验证：`/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`。
- 独立 network namespace 验证真实 TUN、地址、路由、默认路由保持、shutdown、Drop 和 stale manifest 恢复。
- `PrivateNetwork=yes` transient systemd 单元验证服务启动、CLI 诊断、SIGTERM 和宿主机无 `xssvc0` 泄漏。
- 严格 Clippy、Rust/Node 构建、单测、PostgreSQL 集成、Agent 控制面、IPC、ShellCheck 和秘密扫描全部通过。

## M1.2 当时未实现（现状已由后续里程碑取代）

- M1.2 检查点当时尚未实现 XSP/1 握手、AEAD、Key Epoch 和数据循环；这些内容已在 M1.3 及后续里程碑完成，不能把本节当作当前缺口。
- Linux 子网网关的项目独占 nftables、更新、认证遥测和完整 CLI 也已由后续证据覆盖。
- 真实 NAS、Windows VM/WDK 和生产网络仍是当前外部门禁；权威状态见 `PROGRESS.md`、`BLOCKERS.md` 和 `FINAL_REPORT.md`。

## 恢复入口

本文件只保留 M1.2 信任根设计。恢复当前工作应读取 `PROGRESS.md`、`EXECUTION_PLAN.md`、`BLOCKERS.md` 和当前目录级 `AGENTS.md`；任何宿主机网络写操作继续只在隔离 namespace 或明确项目所有权/恢复边界中验证。
