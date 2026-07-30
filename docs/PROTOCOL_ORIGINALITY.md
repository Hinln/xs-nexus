# XSP/1 协议独立设计边界

状态：内部原创性记录  
日期：2026-07-29  
说明：本文件不是法律意见，也不是第三方安全审计。

## 1. 独立设计内容

以下内容由本项目根据任务需求和公开标准原则独立定义：

- `XSP1` magic、版本和消息类型命名；
- `XSD1` 地址发现 magic、固定请求/响应线格式、请求哈希绑定和域标签；
- `XSR1` Relay magic、注册/租约/转发/心跳线格式、短期 Lease 和独立域标签；
- 96 字节固定数据包头的字段集合、顺序和长度；
- Network ID、Source Node ID、Destination Node ID、Session ID、Epoch、Sequence 和 Path ID 的组合；
- `ClientHello → ServerHello → ClientFinish → ServerFinish` 状态机；
- `XSP/1 ...` 域分离标签；
- 配置版本、节点凭证、虚拟 IP 和协议身份的绑定关系；
- Direct 与 Relay 使用同一端到端密文、外层 Relay envelope 单独认证的边界；
- 错误码、关闭语义、重放窗口和 Key Epoch 切换规则。
- 候选种类、generation、短期生命周期、优先级与 AEAD PathChallenge/PathResponse 晋升状态机。

## 2. 公开标准原语

XSP/1 不声称创造下列原语：

- Ed25519；
- X25519；
- HKDF-SHA-256；
- ChaCha20-Poly1305；
- SHA-256；
- 标准 AEAD、重放窗口和临时密钥概念。

这些原语只通过成熟库调用，协议组合本身仍需独立安全审计。

## 3. 明确不兼容

XSP/1 不以兼容以下协议或产品为目标：

- WireGuard；
- Noise Framework；
- QUIC；
- STUN / TURN / ICE 的完整线格式；
- Tailscale、ZeroTier、Nebula、NetBird 或其他私有覆盖网络协议。

即使采用相同公开密码原语或高层概念，也不得复制其字段顺序、常量、密钥文件、状态机、重试语义或封装格式。

## 4. Relay 边界

XSR/1 是项目独立定义的固定 UDP envelope，不兼容 TURN ChannelData、STUN attribute、ICE candidate pair 或其他组网产品协议。它只负责已认证短期 Lease 的有限路由，不参与 XSP/1 内层握手和数据解密。内层包在 Direct 和 Relay 路径逐字节保持一致，Relay 不能替换节点身份、降低算法或生成有效业务包。

v1 独立定义 16 字节公共头、352 字节节点签名注册请求、168 字节 Relay 签名 Lease 响应、104 字节 Data/Keepalive 固定头、152 字节 Relay 签名 Keepalive 响应，以及 `"XSR/1 ... v1"` 三个域分离标签。设计依据仅为本项目的认证、无匿名放大、有界队列和不持有业务密钥要求。

## 5. 独立评审清单

每次协议变更必须回答：

1. 变更是否来自公开标准还是项目独立需求；
2. 是否意外复用了禁止项目的常量、字段顺序或命名；
3. 是否更新规范、威胁模型、测试向量和 Fuzz corpus；
4. 是否影响 transcript、AAD、nonce、Epoch 或兼容性；
5. 是否需要新的安全审计；
6. 是否保留旧版本会导致降级路径。

## 6. 未解决事项

- 尚未完成外部原创性和许可证法律审查；
- 尚未完成第三方协议安全审计；
- M1.1 已锁定节点凭证签名、Role Set 摘要和 200 字节 canonical encoding 向量；
- Role Set 摘要规则在数据面实现前完成澄清，属于 v1 初始定义，不产生兼容性迁移；
- M1.3 已锁定 RFC 7748、RFC 5869、RFC 8439 原语向量和项目独立的四消息握手、Finish、数据 AEAD 向量；
- canonical 与负向 Fuzz seed corpus 由项目生成器复现，覆盖 Magic、版本、类型、flag、长度、保留字段、截断、尾随字节和意外状态；
- M2.1 已锁定项目独立的 `XSD1` 请求/响应向量、发现 Fuzz corpus、签名候选 schema、握手候选回退和认证路径迁移语义；
- M2.3 已锁定项目独立的 `XSR1` 注册、Lease、Data、Keepalive 向量及 Relay Fuzz corpus；当前只证明协议编码边界，不代表 Relay 服务矩阵或第三方安全审计完成；
- 当前自动化已覆盖 Agent UDP/TUN 隔离 namespace 和 NAT 模型端到端链路，但不代表真实公网、运营商 NAT 或 Relay 实网矩阵已经完成。
