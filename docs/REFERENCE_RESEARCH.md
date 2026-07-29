# 公开参考研究边界

状态：M0.2  
日期：2026-07-29

本文件只记录公开标准、官方接口和高层架构经验。未复制第三方项目源代码、私有协议、字段顺序、密钥格式、目录结构或品牌元素。

## 1. 公开标准

| 来源 | 可采用的公开原则 | 不采用的内容 |
|---|---|---|
| RFC 5116 AEAD | AEAD 输入、附加认证数据和 nonce 唯一性要求 | 不复刻任何产品包格式 |
| RFC 5869 HKDF | Extract/Expand 分离和上下文标签 | 不自行实现 HKDF |
| RFC 7748 X25519 | 标准椭圆曲线 Diffie-Hellman 原语 | 不自行实现曲线运算 |
| RFC 8032 Ed25519 | 标准数字签名原语 | 不自行实现签名算法 |
| RFC 8439 ChaCha20-Poly1305 | 标准 AEAD 与 96 位 nonce 要求 | 不自行实现 ChaCha20 或 Poly1305 |
| RFC 8489 STUN | 公网映射发现、事务标识和认证探测原则 | 不兼容或复制 STUN 消息格式，不部署 coturn |
| RFC 8445 ICE | 候选、优先级、配对和连通性检查原则 | 不实现完整 ICE 兼容层，不复制 checklist 格式 |
| RFC 8656 TURN | 认证 Relay、配额和生命周期原则 | 不兼容 TURN，不复制 channel/data 格式 |
| RFC 6479 | 滑动抗重放窗口和边界测试原则 | 不复制 IPsec 数据包格式 |
| RFC 9000 QUIC | 包号空间、路径验证和迁移的公开经验 | 不复制 QUIC 包头、TLS 集成或连接 ID 格式 |

标准入口：

- <https://www.rfc-editor.org/rfc/rfc5116>
- <https://www.rfc-editor.org/rfc/rfc5869>
- <https://www.rfc-editor.org/rfc/rfc7748>
- <https://www.rfc-editor.org/rfc/rfc8032>
- <https://www.rfc-editor.org/rfc/rfc8439>
- <https://www.rfc-editor.org/rfc/rfc8489>
- <https://www.rfc-editor.org/rfc/rfc8445>
- <https://www.rfc-editor.org/rfc/rfc8656>
- <https://www.rfc-editor.org/rfc/rfc6479>
- <https://www.rfc-editor.org/rfc/rfc9000>

## 2. 操作系统官方接口

### Linux

- Linux TUN/TAP 文档只用于理解 `/dev/net/tun`、`TUNSETIFF` 和多队列行为；
- rtnetlink 文档只用于接口、地址、路由和 rule 的原生管理；
- network namespace 和 nftables 官方文档用于建立隔离测试与最小规则；
- 运行时核心逻辑不通过 `ip`、`route` 或 `ifconfig` shell 命令完成。

入口：

- <https://docs.kernel.org/networking/tuntap.html>
- <https://man7.org/linux/man-pages/man7/rtnetlink.7.html>
- <https://man7.org/linux/man-pages/man7/network_namespaces.7.html>
- <https://netfilter.org/projects/nftables/>

### Windows

- Microsoft NetAdapterCx、WDF、INF、Driver Verifier 和驱动签名文档用于理解官方生命周期与接口；
- 官方示例只作为 API 行为说明，不复制实现；
- 驱动保持最小化，协议、密码学、ACL、NAT 和 Relay 留在用户态。

入口：

- <https://learn.microsoft.com/windows-hardware/drivers/netcx/>
- <https://learn.microsoft.com/windows-hardware/drivers/wdf/>
- <https://learn.microsoft.com/windows-hardware/drivers/devtest/driver-verifier>
- <https://learn.microsoft.com/windows-hardware/drivers/install/driver-signing>

## 3. 公开架构经验

允许研究以下公开问题域，但不读取或移植其实现：

- 控制面和数据面分离；
- 节点目录与候选交换；
- Direct 优先和 Relay 回退；
- 默认拒绝策略与子网审批；
- 最小内核攻击面；
- 重放、密钥轮换和故障恢复测试方法。

禁止将 Tailscale、Headscale、WireGuard、ZeroTier、NetBird、Nebula、OpenVPN、SoftEther、FRP、rathole、nps、coturn 或 TAP-Windows 的源代码、私有协议、包格式、密钥格式、目录结构和品牌元素引入本项目。

## 4. 研究流程

1. 只从标准、论文、官方文档和公开高层说明提取需求；
2. 在 `CLEAN_ROOM_LOG.md` 记录来源类别和可用原则；
3. 关闭参考材料后独立编写项目设计；
4. 对字段、状态机和标签做项目内评审；
5. 使用差异清单检查是否意外复刻已知私有格式；
6. 对来源或许可证不清楚的内容拒绝引入。

## 5. 当前限制

本记录不是法律意见，也不能证明全世界范围的专利或协议独创性。正式发布前仍需许可证复核、协议安全审计和必要的法律审查。
