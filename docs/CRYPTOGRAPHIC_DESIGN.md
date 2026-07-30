# XSP/1 密码学设计

状态：M2.3 XSP/1 核心、XSD/1 发现和 XSR/1 Relay 线格式实现  
日期：2026-07-30  
审计状态：未完成独立第三方审计，不适合宣称生产级安全。

## 1. v1 固定密码套件

套件 ID：`0x0001`

- 身份签名：Ed25519；
- 临时密钥协商：X25519；
- 密钥派生：HKDF-SHA-256；
- 数据 AEAD：ChaCha20-Poly1305；
- 哈希：SHA-256；
- 常量时间比较与内存清零：成熟库提供。

v1 只接受套件 `0x0001`，不提供可协商的弱备选。ClientHello 可以编码套件列表以支持未来版本，但 v1 列表必须只包含 `0x0001`，ServerHello 的选择进入签名 transcript。未知或空列表直接拒绝。

## 2. 密钥层级

### 2.1 Controller 密钥

- Credential Signing Key：签发节点凭证；
- Configuration Signing Key：签名配置和策略版本；
- Online Update Delegation Key：只签受限在线发布元数据；
- Offline Update Root：离线保存，不进入 Controller 服务器。

不同用途必须使用不同密钥和 Key ID，不允许复用。

### 2.2 节点长期身份

节点首次安装时本地生成 Ed25519 密钥对。私钥权限为 `0600` 或平台安全存储，Controller 只接收公钥。Node ID 定义为：

```text
node_id = first_16_bytes(
  SHA-256("XSP/1 node id v1" || ed25519_public_key)
)
```

### 2.3 节点凭证

Controller 签名固定 200 字节凭证。签名覆盖前 136 字节并添加域分离前缀：

```text
Ed25519.Sign(
  controller_credential_key,
  "XSP/1 credential v1" || credential_without_signature
)
```

凭证绑定 Network ID、Node ID、Ed25519 公钥、虚拟 IPv4、序列、有效期、角色摘要和 Controller Key ID。

### 2.4 配置签名

Controller 使用与凭证密钥不同的 Ed25519 Configuration Signing Key，对序列化后的精确紧凑 UTF-8 JSON 字节签名：

```text
configuration_signature = Ed25519.Sign(
  controller_configuration_key,
  "XS Nexus configuration v1" || exact_payload_bytes
)

configuration_key_id = first_4_bytes(
  SHA-256(controller_configuration_public_key)
)
```

签名信封携带单调版本、原始 payload 的 Base64URL 编码、签名和 Key ID。验证方必须验证签名后再解析 payload，并拒绝低版本及同版本不同 payload。Configuration Signing Public Key 在 TLS 保护的 enrollment 响应中取得并持久化为网络信任材料；后续轮换必须由已信任控制面授权。

### 2.5 控制连接认证

Controller 为每条 WebSocket 控制连接生成 32 字节 CSPRNG challenge，challenge 只用于该连接且 10 秒后失效。节点签名：

```text
Ed25519.Sign(
  node_identity_key,
  "XS Nexus control authentication v1" ||
  challenge[32] || node_id[16]
)
```

Controller 同时验证节点凭证、数据库中的公钥与 Network ID、节点签名和凭证有效期。认证失败只返回通用错误并关闭连接。控制连接只同步签名配置，不承载业务数据。

上述 Controller 控制面标签使用 `XS Nexus ...` 域；XSP/1 节点凭证和数据面标签使用 `XSP/1 ...` 域。两组标签不得互换。

### 2.6 临时握手密钥

每次完整握手双方生成新的 X25519 临时私钥。任何网络切换都不能复用已经完成握手的临时私钥。握手完成或失败后立即清零。

### 2.7 发现与候选签名

XSD/1 地址发现请求使用节点长期 Ed25519 身份密钥签名，并携带 Controller 签名的节点凭证。签名覆盖固定长度请求头、Network ID、Node ID、随机 Request ID、客户端时间和完整凭证：

```text
discovery_request_signature = Ed25519.Sign(
  node_identity_key,
  "XS Nexus discovery request v1" || request_without_signature
)
```

Controller 只在数据库确认凭证仍活动后返回观察端点，并使用 Configuration Signing Key 签名精确响应。响应绑定请求的 Network ID、Node ID、Request ID 和完整 SHA-256，因此不能移植到其他请求或节点：

```text
discovery_response_signature = Ed25519.Sign(
  controller_configuration_key,
  "XS Nexus discovery response v1" || response_without_signature
)
```

### 2.8 Relay 注册与响应签名

XSR/1 注册请求携带现有 Controller 节点凭证，并由节点长期身份密钥签名：

```text
relay_register_request_signature = Ed25519.Sign(
  node_identity_key,
  "XSR/1 register request v1" || request_without_signature
)
```

Relay 使用独立 Ed25519 身份密钥签发最多 300 秒的短期 Lease，并签名注册响应和 Keepalive Response：

```text
relay_register_response_signature = Ed25519.Sign(
  relay_identity_key,
  "XSR/1 register response v1" || response_without_signature
)

relay_keepalive_response_signature = Ed25519.Sign(
  relay_identity_key,
  "XSR/1 keepalive response v1" || response_without_signature
)
```

三个签名域不得互换。Relay 身份公钥只从 Controller 签名配置取得；Lease ID 使用 CSPRNG，作为短期有状态转发能力绑定 Network、Node、Relay、UDP 来源端点、过期时间和重放窗口。Lease ID 不派生、不替代也不能访问 XSP/1 traffic key。

候选广告通过已经完成 challenge 认证的 WebSocket 控制连接发送。节点使用长期身份密钥签名精确紧凑 JSON 字节：

```text
candidate_signature = Ed25519.Sign(
  node_identity_key,
  "XS Nexus candidate advertisement v1" || exact_payload_bytes
)
```

广告绑定 Network ID、Node ID、持久化单调 generation、生成/过期时间和有界候选列表。Controller 验证后将候选放入新的 Controller 签名配置版本；节点之间不直接信任对方提交的原始 JSON 或签名信封。

## 3. transcript

规范中的所有整数使用网络字节序，所有长度包含在被签名内容中。定义：

```text
client_auth_input =
  "XSP/1 client auth v1" || client_hello_without_signature

server_auth_input =
  "XSP/1 server auth v1" ||
  SHA-256(client_hello_full) ||
  server_hello_without_signature

hello_transcript_hash = SHA-256(
  "XSP/1 hello transcript v1" ||
  client_hello_full ||
  server_hello_full
)
```

ClientHello 和 ServerHello 的签名必须在任何 HKDF 派生结果被接受前验证。签名输入绑定版本、Network ID、双方 Node ID、临时公钥、随机数、凭证、套件和 Session ID。

## 4. 握手密钥派生

```text
dh = X25519(local_ephemeral_private, peer_ephemeral_public)

handshake_salt = SHA-256(
  "XSP/1 handshake salt v1" ||
  network_id || client_nonce || server_nonce
)

handshake_prk = HKDF-Extract(handshake_salt, dh)

client_handshake_key = HKDF-Expand(
  handshake_prk,
  "XSP/1 client handshake key v1" || hello_transcript_hash,
  32
)

server_handshake_key = HKDF-Expand(
  handshake_prk,
  "XSP/1 server handshake key v1" || hello_transcript_hash,
  32
)

client_handshake_nonce_salt = HKDF-Expand(
  handshake_prk,
  "XSP/1 client handshake nonce v1" || hello_transcript_hash,
  4
)

server_handshake_nonce_salt = HKDF-Expand(
  handshake_prk,
  "XSP/1 server handshake nonce v1" || hello_transcript_hash,
  4
)
```

X25519 全零共享结果必须拒绝。任何 HKDF 或 AEAD 错误都终止握手，不发送可区分的密码学细节。

## 5. 双向 key confirmation

ClientFinish 使用 `client_handshake_key` 加密 `hello_transcript_hash`，ServerFinish 使用 `server_handshake_key` 加密包含 ClientFinish 后的 transcript hash。两边只有在验证对方 Finish 后才进入 Established。

握手 nonce：

```text
nonce = direction_handshake_nonce_salt || uint64_be(confirm_sequence)
```

v1 的每个方向只允许一个 Finish，`confirm_sequence = 0`。Finish 重复只可作为幂等网络重传处理，不能生成第二个 nonce 使用不同明文。

## 6. 应用密钥

```text
full_transcript_hash = SHA-256(
  "XSP/1 full transcript v1" ||
  client_hello_full || server_hello_full ||
  client_finish_full || server_finish_full
)

application_prk = HKDF-Extract(full_transcript_hash, handshake_prk)

client_to_server_secret = HKDF-Expand(
  application_prk,
  "XSP/1 client traffic secret v1" ||
  network_id || session_id || client_node_id || server_node_id,
  32
)

server_to_client_secret = HKDF-Expand(
  application_prk,
  "XSP/1 server traffic secret v1" ||
  network_id || session_id || server_node_id || client_node_id,
  32
)
```

每个方向再派生：

```text
traffic_key = HKDF-Expand(direction_secret, "XSP/1 traffic key v1" || epoch, 32)
nonce_salt = HKDF-Expand(direction_secret, "XSP/1 nonce salt v1" || epoch, 4)
```

## 7. 数据 nonce 与 AAD

```text
nonce = nonce_salt[4] || uint64_be(sequence)
AAD = exact_96_byte_data_header
```

- 每方向从随机初始序列或 0 开始，但必须在同一方向密钥下单调且唯一；
- v1 选择从 0 开始，唯一性由新 Session ID、方向密钥和 Epoch 保证；
- 崩溃后不得从持久化会话恢复旧密钥并重置序列，必须完整重握手；
- 序列接近上限、计数状态不确定或可能回绕时立即停止发送并重握手。

PathChallenge 和 PathResponse 不派生独立弱密钥，也不接受明文 token。它们使用当前方向 traffic key、标准数据头 AAD、独立序列和重放窗口；Path ID 与 Packet Type 位于 AAD 中，8 字节 challenge token 位于密文中。只有来源端点、Path ID、token 和 AEAD 全部匹配时才可迁移活动路径。

## 8. Epoch 与完整重握手

- Epoch 为 `uint32`，初始值 0；
- 每个 Epoch 具有独立方向密钥、nonce salt 和重放窗口；
- 单个 Epoch 在 `2^32` 个包、1 小时或实现配置的更低安全阈值首先到达时轮换；
- Key Update 在当前有效会话内加密和认证，接收方先安装单方向接收 Epoch，发起方收到匹配确认后再切换对应发送 Epoch；
- 旧 Epoch 最多保留 30 秒且最多接收 1024 个乱序包；
- 每 24 小时、节点凭证变化、吊销事件、路径身份异常或状态不确定时执行完整 X25519 重握手；
- 仅通过 HKDF 链轮换不能恢复已泄露会话的前向安全，因此不能无限替代完整重握手。

Linux Agent 的生产阈值为每发送方向 `2^20` 个数据包或 1 小时，以先到者为准。`privileged-network-tests` 构建仅为自动化验证把阈值缩短为 4 个数据包或 2 秒，并把旧 Epoch 保留期缩短为 5 秒；这些测试参数不进入默认构建。

## 9. 抗重放

每个接收方向和 Epoch 使用 1024 位滑动窗口：

1. 序列高于当前最大值时向前移动窗口；
2. 超出窗口下界的序列拒绝；
3. 窗口内已标记序列拒绝；
4. 只有 AEAD 成功后才永久标记序列，避免攻击者用伪造 Tag 占位；
5. 为防重复高成本 AEAD，可在验证前执行不改变窗口的廉价范围检查；
6. Epoch 状态彼此隔离，旧 Epoch 到期后全部清零。

## 10. 密钥生命周期

- 长期节点私钥：本机受限存储，可轮换；
- 发现请求与候选广告：复用节点身份签名用途，但由独立域标签隔离，绝不复用 XSP/1 traffic key；
- 发现响应：复用 Controller Configuration Signing Key，但由独立域标签隔离并绑定精确请求哈希；
- Relay 身份私钥：只签 XSR/1 注册和 Keepalive 响应，不签节点凭证、配置或 XSP/1 数据；
- Relay Lease ID：最多 300 秒，仅驻留 Agent/Relay 内存和 UDP 控制消息，不写日志、指标或持久存储；
- Relay Data：不引入业务密钥；完整内层 XSP/1 密文保持逐字节不变；
- 临时 X25519 私钥：单次握手，结束后清零；
- handshake key：Finish 完成后清零；
- traffic secret 和 key：会话/Epoch 内存；
- 日志、指标、错误、诊断、core dump 和前端禁止包含任何私钥或完整密钥材料；
- 调试构建也不得增加秘密日志开关。

## 11. 失败和错误

未认证阶段的错误响应不得大于请求，且只返回通用拒绝或静默丢弃。已认证阶段使用加密错误码。签名、X25519、HKDF、AEAD、重放和凭证错误对公网不得形成可区分 oracle。

## 12. 待审计问题

当前实现证据包括：

- `crates/protocol/tests/primitives.rs` 锁定 RFC 7748、RFC 5869 和 RFC 8439 原语向量；
- `tests/vectors/xsp1/session-v1.json` 与 `generate_session_vector` 锁定双方 Hello、双向 Finish、应用数据包、方向密钥派生和 AAD；
- `crates/protocol/tests/session.rs` 覆盖 transcript/身份篡改、全零 X25519、Finish/Tag 篡改、重放窗口、Epoch 乱序和旧 Epoch 退休；
- `tests/vectors/xsp1/discovery-v1.json` 与 `generate_discovery_vector` 锁定 XSD/1 请求/响应、精确请求哈希、观察端点和双方签名；
- `tests/vectors/xsp1/relay-v1.json` 与 `generate_relay_vector` 锁定 XSR/1 注册、Lease、Data、Keepalive、独立签名域和完整 SHA-256；
- `scripts/test-protocol-vectors.sh` 验证 canonical 向量及有效、非规范和意外状态 Fuzz seed corpus 一致性。

上述自动化证据不替代密码学组合的形式化分析或独立第三方审计。

- transcript 和凭证编码是否存在歧义；
- Ed25519 长期身份与 Controller 凭证组合是否充分绑定；
- key confirmation 和应用密钥派生的形式化安全性；
- Epoch 更新、乱序与重放窗口的竞态；
- XSD/1 使用长期身份签名与配置签名密钥时的跨协议域分离充分性；
- 候选 generation、过期时间和动态配置传播在时钟异常下的安全边界；
- AEAD PathChallenge/PathResponse 与普通数据共享 traffic key、序列空间和重放窗口的状态机正确性；
- Relay 元数据与流量分析；
- XSR/1 Data 使用短期 Lease、来源端点和重放窗口而非逐包外层 MAC 时，对 on-path 注入、流量消耗和故障语义的剩余风险；
- DoS 预检和签名验证成本；
- 时钟异常、吊销和离线窗口。
