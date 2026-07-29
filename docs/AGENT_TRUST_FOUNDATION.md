# Agent 信任链基础

状态：M1.2 完成
日期：2026-07-29

## 已实现边界

- 严格 Agent 配置解析；远程 Controller 必须使用 HTTPS/WSS，明文 HTTP/WS 仅允许精确 loopback 测试地址。
- 本地 Ed25519 身份生成、`0700` 状态目录、`0600` 私钥文件、符号链接拒绝和原子持久化。
- Enrollment 响应大小限制、无重定向、短超时，以及凭证、配置签名、网络、节点、地址和密钥用途的完整验证。
- WebSocket 每连接 challenge 持钥证明、4096 字节消息边界、配置单调更新、重连退避和最后有效配置保留。
- Controller 与 Agent 共用严格 wire DTO，避免两端模型漂移。
- Linux Agent 使用非持久 TUN FD；通过 Netlink 设置 MTU、`/32` 地址、接口状态和地址池路由，拒绝默认路由、过宽路由和接管同名接口。
- Agent 运行时统一管理控制循环、TUN 读取、Unix IPC 和 SIGTERM 清理；异常退出、Drop 和 stale manifest 恢复均不会在宿主机残留接口或项目路由。
- 本地只读 Unix IPC 使用 `0600` socket、有界帧、严格 JSON 和并发限制；`xs status`、`xs peers` 与 `xs diagnostics` 不返回凭证或密钥材料。
- systemd 单元使用专用 `xs-nexus` 用户、`CAP_NET_ADMIN`、`/dev/net/tun` 设备白名单、`NoNewPrivileges` 和只读系统目录。

## 验证证据

- M1.2 全量验证：`/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`。
- 独立 network namespace 验证真实 TUN、地址、路由、默认路由保持、shutdown、Drop 和 stale manifest 恢复。
- `PrivateNetwork=yes` transient systemd 单元验证服务启动、CLI 诊断、SIGTERM 和宿主机无 `xssvc0` 泄漏。
- 严格 Clippy、Rust/Node 构建、单测、PostgreSQL 集成、Agent 控制面、IPC、ShellCheck 和秘密扫描全部通过。

## 明确未实现

- 尚未实现 XSP/1 握手、会话密钥、AEAD、抗重放、Key Epoch 和 TUN/UDP 双向数据循环；M1.3 前 TUN 包只读取并安全丢弃。
- Agent 运行时尚未写入 nftables；network namespace 仅用于隔离测试，不作为正式部署拓扑。
- 尚未接入真实 NAS、Windows 或生产网络。

## 恢复入口

下一步完整读取 `crates/protocol/AGENTS.md`、`docs/XSP1_PROTOCOL.md` 和 `docs/THREAT_MODEL.md`，先补充失败测试与握手/密钥派生向量，再实现 M1.3；任何宿主机网络写操作继续只在隔离 namespace 中验证。
