# Agent 信任链基础

状态：M1.2 中间检查点  
日期：2026-07-29

## 已实现边界

- 严格 Agent 配置解析；远程 Controller 必须使用 HTTPS/WSS，明文 HTTP/WS 仅允许精确 loopback 测试地址。
- 本地 Ed25519 身份生成、`0700` 状态目录、`0600` 私钥文件、符号链接拒绝和原子持久化。
- Enrollment 响应大小限制、无重定向、短超时，以及凭证、配置签名、网络、节点、地址和密钥用途的完整验证。
- WebSocket 每连接 challenge 持钥证明、4096 字节消息边界、配置单调更新、重连退避和最后有效配置保留。
- Controller 与 Agent 共用严格 wire DTO，避免两端模型漂移。

## 明确未实现

- 未创建 TUN，未写入路由、nftables 或 network namespace。
- 未安装或启动 systemd 服务。
- 未实现本地 IPC、`xs status`、`xs peers` 或 `xs diagnostics`。
- 未接入真实 NAS、Windows 或生产网络。

## 恢复入口

下一步先为 Agent 增加本地 Controller enrollment/control 集成测试，再实现独立的 TUN/Netlink 生命周期模块；任何宿主机网络写操作继续只在隔离 namespace 中验证。
