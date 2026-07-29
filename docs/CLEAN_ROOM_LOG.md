# Clean-room 记录

## 规则

- 不复制、翻译或改写禁止项目的源代码；
- 不记录第三方私有包字节、密钥文件或目录结构；
- 标准密码学只通过成熟库调用；
- 每次参考研究记录日期、来源类型、提取原则和独立产出；
- 任何来源不清楚的片段都不进入仓库。

## 2026-07-29 / M0.2 初始设计

### 阅读范围

- 项目任务书和仓库规则；
- RFC 5116、5869、7748、8032、8439、8489、8445、8656、6479、9000 的公开原则；
- Linux TUN、rtnetlink、network namespace 和 nftables 官方接口说明；
- Microsoft NetAdapterCx、WDF、Driver Verifier 和签名流程说明；
- 参考产品仅使用任务书中列出的高层架构方向，未读取其源代码。

### 提取的公开原则

- AEAD nonce 在单个密钥下必须唯一；
- HKDF 标签应绑定协议、版本、网络、会话和方向；
- X25519 临时密钥提供会话秘密输入，Ed25519 提供长期身份签名；
- 候选地址需要认证探测和明确优先级；
- Relay 必须认证、限额且不能成为匿名反射服务；
- TUN 和路由变更应通过原生系统接口实施并可回滚；
- 驱动应最小化并在隔离 VM 中验证。

### 独立产出

- `ARCHITECTURE.md` 中的组件边界、部署边界和故障行为；
- `CRYPTOGRAPHIC_DESIGN.md` 中的 XSP/1 transcript、HKDF 标签和密钥生命周期；
- `XSP1_PROTOCOL.md` 中的 96 字节数据包头、握手消息、状态机和错误处理；
- `THREAT_MODEL.md` 中的资产、主体、信任边界和测试映射；
- `PROTOCOL_ORIGINALITY.md` 中的差异与禁止复刻记录。

### 未使用

- 第三方组网源代码；
- WireGuard、Noise、QUIC、STUN、TURN 或其他项目的私有/完整线格式；
- 第三方节点密钥格式；
- 第三方安装器、目录布局和品牌资产。

## 2026-07-29 / M1.1 Controller 控制面

### 来源类型

- PostgreSQL 事务、行锁、唯一约束、advisory lock 和 trigger 的公开文档；
- Axum、SQLx、Tokio、Ed25519 Dalek 和 WebSocket 库的公开 API 文档；
- 已在 M0.2 记录的公开密码学标准原则。

### 提取的公开原则

- 一次性 Token 必须在单个数据库事务内锁定、验证并消费；
- IP 地址选择需要每网络串行化和数据库唯一约束共同防止竞争；
- 签名输入需要固定域分离、精确字节和独立用途密钥；
- WebSocket 长连接需要独立 challenge、认证时限、消息上限和心跳；
- 审计数据应追加写入且不得持久化 bearer secret。

### 独立产出

- 项目自有 PostgreSQL schema、迁移、Token/IPAM 事务边界和审计事件模型；
- XS Nexus enrollment、签名配置和控制连接 JSON API；
- XSP/1 固定 200 字节节点凭证实现、Role Set canonical digest 和测试向量；
- 独立域标签 `XS Nexus configuration v1` 与 `XS Nexus control authentication v1`。

### 未使用或拒绝内容

- 未读取或复制任何第三方组网产品的控制器、凭证、IPAM、Relay 或协议实现；
- 未采用第三方私有 wire format、密钥文件布局、数据库 schema 或 API 兼容层；
- 未自行实现 Ed25519、SHA-256 或常量时间比较原语。

### 许可证与安全备注

新增 Rust crate 的版本和许可证记录在 `Cargo.lock` 与 `THIRD_PARTY.md`。控制面仍需 TLS 终止、最终 RBAC、安全审计和生产密钥管理，不得描述为生产就绪。

## 后续记录模板

```markdown
## YYYY-MM-DD / 主题

### 来源类型

### 提取的公开原则

### 独立产出

### 未使用或拒绝内容

### 许可证与安全备注
```
