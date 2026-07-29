# XSP/1 协议独立设计边界

状态：内部原创性记录  
日期：2026-07-29  
说明：本文件不是法律意见，也不是第三方安全审计。

## 1. 独立设计内容

以下内容由本项目根据任务需求和公开标准原则独立定义：

- `XSP1` magic、版本和消息类型命名；
- 96 字节固定数据包头的字段集合、顺序和长度；
- Network ID、Source Node ID、Destination Node ID、Session ID、Epoch、Sequence 和 Path ID 的组合；
- `ClientHello → ServerHello → ClientFinish → ServerFinish` 状态机；
- `XSP/1 ...` 域分离标签；
- 配置版本、节点凭证、虚拟 IP 和协议身份的绑定关系；
- Direct 与 Relay 使用同一端到端密文、外层 Relay envelope 单独认证的边界；
- 错误码、关闭语义、重放窗口和 Key Epoch 切换规则。

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

Relay envelope 只负责已认证会话的有限路由，不参与 XSP/1 内层握手和数据解密。内层包在 Direct 和 Relay 路径保持一致，Relay 不能替换节点身份、降低算法或生成有效业务包。

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
- 完整签名、X25519、HKDF 和 AEAD 测试向量将在 M1.3 实现时生成并锁定；
- 当前只有规范级 canonical encoding 向量，不代表加密实现已经验证。
