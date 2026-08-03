# Controller API（M4.1）

状态：已实现并由 PostgreSQL 集成测试覆盖  
日期：2026-07-31
安全状态：控制台用户、服务端会话、CSRF 和角色授权已实现；生产 TLS 与正式 DNS 仍属于部署门禁。

## 1. 边界与传输

- Controller 提供 enrollment、配置同步、健康检查和临时管理员入口；不承载节点业务数据。
- HTTP JSON body 全局上限为 1 MiB；Relay 报告签名输入另限 64 KiB，Agent 控制消息和 Peer 数另有 512 KiB/1024 项边界；未知 JSON 字段拒绝。
- `GET /install` 公开返回固定生产域名和发布公钥指纹的 Linux 一键引导脚本；仅在完整只读发布目录通过启动校验后可用。
- `GET /downloads/linux/stable/{file}` 只允许当前版本的 x86_64/aarch64 归档、清单、64 字节签名和发布公钥精确文件名，不提供目录遍历或任意文件读取。
- enrollment、管理员 API 和 WebSocket 必须部署在 TLS 反向代理之后；明文监听只允许受控 loopback 或容器内部链路。
- `X-Request-Id` 被生成并传播；HTTP tracing 不记录 header 或 body。
- 所有错误使用固定信封，不暴露 SQL、签名、Token 或内部状态。

```json
{"error":{"code":"invalid_request","message":"request validation failed"}}
```

## 2. 运行配置

| 环境变量 | 要求 |
|---|---|
| `CONTROLLER_LISTEN` | 监听地址，例如 `127.0.0.1:8080` |
| `DATABASE_URL` | PostgreSQL URL，不得提交 Git |
| `DATABASE_SCHEMA` | 小写字母或下划线开头，只允许小写字母、数字、下划线，最长 63 |
| `ADMIN_API_TOKEN` | 至少 32 字符；进程只保留 SHA-256 hash |
| `CONSOLE_BOOTSTRAP_USERNAME` | 用户表为空时创建的初始管理员用户名；与密码成对配置 |
| `CONSOLE_BOOTSTRAP_PASSWORD` | 初始管理员临时强密码；只读取后进行 Argon2id 哈希，不持久化明文 |
| `CONSOLE_COOKIE_SECURE` | 默认 `true`；只有 loopback 无 TLS 测试可显式设为 `false` |
| `CONSOLE_SESSION_TTL_SECONDS` | `900..86400`，默认 8 小时 |
| `CREDENTIAL_SIGNING_KEY_PATH` | 权限不宽于 `0600` 的 32 字节原始 Ed25519 seed 文件 |
| `CONFIG_SIGNING_KEY_PATH` | 与凭证密钥不同的 32 字节原始 Ed25519 seed 文件 |
| `UPDATE_SIGNING_PUBLIC_KEY_PATH` | 可选、权限不宽于 `0644` 的 32 字节原始 Ed25519 发布公钥；不配置时更新导入明确不可用 |
| `NODE_CREDENTIAL_TTL_SECONDS` | `3600..31536000`，默认 30 天 |

密钥文件和真实环境配置位于仓库外。Credential 与 Configuration key 复用会导致启动失败。

## 3. 健康检查

### `GET /health/live`

只证明进程可响应，返回 `200`：

```json
{"status":"ok","database":"unchecked"}
```

### `GET /health/ready`

执行实际数据库查询。成功返回 `200` 和 `{"status":"ok","database":"ok"}`；数据库不可用返回 `503 service_unavailable`。

## 4. 控制台认证和授权

### `POST /v1/auth/login`

- 请求为用户名和密码；成功返回用户、CSRF Token 和绝对到期时间，并设置 `xs_nexus_session` HttpOnly、SameSite=Strict Cookie；
- 生产 Cookie 默认带 Secure；数据库只保存会话与 CSRF 的域分离 SHA-256 摘要；
- 每用户/来源 15 分钟最多 5 次失败；未知用户执行等价 Argon2 工作并返回相同错误；
- 每用户最多保留 10 个会话，超出时撤销最早会话。

### `GET /v1/auth/session`

恢复当前会话并轮换 CSRF Token。过期、撤销、禁用用户或无效 Cookie 返回 `401`。

### `POST /v1/auth/logout`

要求有效会话和 CSRF，撤销数据库会话并返回清除 Cookie。

### 角色

- `administrator`：全部管理读取和写入，包括创建控制台用户；
- `operator`：读取和网络管理写入，但不能管理控制台用户；
- `auditor`：只读；所有写入在服务端返回 `403`。

### `GET/POST /v1/admin/users`

读取用户需要已认证会话；创建用户只允许 administrator，要求 CSRF，并保存 Argon2id PHC 哈希。响应不返回密码或哈希。

### `GET /v1/admin/console`

返回控制台管理快照，包括网络、节点、Token 元数据、组、ACL、路由建议/审批、Relay 目录、拓扑、审计、告警和能力状态。节点在线只依据当前进程已认证控制连接；路径、流量、延迟和 Relay 健康只来自新鲜且验签通过的报告，缺失或陈旧数据返回 `unavailable`/`stale`，不推断默认健康或路径。任何秘密、Token 明文、会话和 hash 均不返回。

所有会话管理写操作要求 `X-CSRF-Token`。`ADMIN_API_TOKEN` 仍可用于服务端自动化并视为 administrator，但不得交给浏览器。

## 5. Bootstrap 管理 API

服务端自动化可使用 `Authorization: Bearer <ADMIN_API_TOKEN>`。浏览器控制台使用上一节的会话和角色授权，不读取或持久化该 Token。

### `POST /v1/admin/networks`

请求示例：

```json
{
  "name":"development",
  "address_pool":"100.88.0.0/24",
  "reserved_addresses":16
}
```

成功返回 `201`。只接受 IPv4 pool；保留数必须给网络地址、广播地址和后续节点留出有效范围。创建网络时同时发布签名配置版本 1 并写入审计事件。

### `POST /v1/admin/enrollment-tokens`

请求示例：

```json
{
  "network_id":"00000000-0000-0000-0000-000000000000",
  "expires_in_seconds":3600,
  "max_uses":1,
  "default_role_bitmap":1,
  "default_tags":["linux"],
  "requested_virtual_ip":"100.88.0.30"
}
```

- 有效期为 60 秒至 7 天，`max_uses` 为 `1..100`。
- `requested_virtual_ip` 可省略；指定时必须属于网络、位于可分配范围且未活动/冷却占用。
- Token 由 CSPRNG 生成，只在 `201` 响应中返回一次；数据库和审计只保存域分离 SHA-256 hash。
- Token 的角色、标签、网络、固定 IP、有效期和次数在创建后不可扩大。

### 更新发布与灰度策略

- `GET/POST /v1/admin/update-releases`：读取或导入离线签名的不可变 Linux 发布。POST 只接受 Base64URL 清单、64 字节签名和与清单归档名一致的 HTTPS URL；Controller 不接受或保存发布私钥。
- `GET /v1/admin/networks/{network_id}/update-policies`：读取网络的通道/平台/架构策略。
- `PUT /v1/admin/networks/{network_id}/update-policies/{channel}/{platform}/{architecture}`：以 `expected_generation`、发布 ID、可选最低版本、`0..10000` 基点和 pause 状态替换策略；代次冲突返回 409。
- `PUT /v1/admin/networks/{network_id}/nodes/{node_id_base64}/update-channel`：以 `expected_configuration_version` 和 `update_channel` 修改活动节点的分配通道，发布新签名配置并写入审计；配置版本冲突返回 409。

读取接口需要 Read 权限；导入、策略和节点通道写入需要 Manage 权限及有效 CSRF。发布内容不可变，策略 pause 始终覆盖最低版本和灰度资格。完整信任边界见 `docs/UPDATE_SYSTEM.md`。

## 6. 节点 Enrollment

### `POST /v1/enroll`

节点必须本地生成 Ed25519 密钥，只提交 32 字节公钥：

```json
{
  "token":"<one-time enrollment token>",
  "name":"node-a",
  "device_type":"linux",
  "identity_public_key_base64":"<base64url-no-padding public key>"
}
```

成功返回 `201`，包含 Network ID、Node ID、虚拟 IP、固定 200 字节凭证、两个独立 Controller 验证公钥和最新签名配置。所有二进制字段使用无 padding Base64URL。

Token 行锁、每网络 PostgreSQL transaction advisory lock、IP lease、节点、凭证序列、Token 消费、配置版本和审计写入位于同一事务。并发使用单次 Token 时最多一个请求成功。重复公钥或 Node ID 返回 `409 resource_conflict`；所有无效、过期或耗尽 Token 统一返回 `401 invalid_enrollment`。

## 7. 签名配置

配置信封：

```json
{
  "version":2,
  "payload_base64":"<exact compact UTF-8 JSON bytes>",
  "signature_base64":"<Ed25519 signature>",
  "signer_key_id":1234567890
}
```

签名输入为 `"XS Nexus configuration v1" || exact_payload_bytes`。Key ID 是配置公钥 SHA-256 的前 4 字节网络序整数。payload 包含 schema version、Network ID、单调版本、生成时间、地址池、节点目录、Relay 和 policy 数组。验证方必须先验证信封，再解析 payload，并拒绝版本回退或同版本不同 payload。

## 8. WebSocket 控制连接

### `GET /v1/control`

消息和 frame 上限均为 512 KiB；配置、运行时和遥测结构仍分别执行更小的字段、列表与签名输入边界。连接流程：

1. Controller 发送 `{"type":"challenge","challenge_base64":"..."}`。
2. 节点在 10 秒内发送 `authenticate`，携带 Node ID、200 字节凭证和节点签名。
3. 签名输入为 `"XS Nexus control authentication v1" || challenge[32] || node_id[16]`。
4. Controller 验证凭证、数据库身份、公钥、Network ID、有效期和签名后发送 `authenticated` 与最新配置。
5. 节点发送 `{"type":"sync","last_version":2}`；服务端返回 `configuration` 或 `up_to_date`。
6. 节点用身份密钥签名发送 `report_runtime`；Controller 验证身份、当前分配通道、时间和字段后保存单调报告，并只对 eligible/required 节点返回 `update_directive`。
7. 节点用同一身份密钥、独立域标签签名发送 `report_telemetry`；报告绑定 Network/Node、随机非零 boot ID、单调 sequence、当前签名配置的精确 Peer 集合、路径和累计计数。Controller 验证后返回 `telemetry_accepted`；重放、过期、计数回滚、未知 Peer/Relay 或聚合不一致会关闭该控制连接。

challenge 每连接一次性生成。二进制、未知或越序消息被拒绝；连接使用 ping/pong 心跳。控制通道只传签名配置和状态，不传业务数据。

### `POST /v1/relay-metrics`

该端点不使用浏览器会话或管理员 Token；请求必须包含目录中活动 Relay 的 ID、随机非零 boot ID、单调 sequence、生成时间、累计指标和 Ed25519 签名。签名输入为 `"XS Nexus relay telemetry report v1" || compact_report_json`，公钥只来自 Controller 的 Relay 目录。

报告不包含 Network/Node、来源/目标端点、Lease ID 或业务载荷。Controller 拒绝未知/过期 Relay、错误签名、10 分钟以上陈旧报告、未来时间、同 boot 重放/计数回滚、分类丢弃和总丢弃不一致及异常延迟；只保存每 Relay 最新报告和最多 25 小时/9000 个样本，覆盖允许的最快 10 秒周期。生产使用 HTTPS；只有 loopback 或显式批准的隔离容器网络可允许 HTTP，签名验真不因此关闭。

## 9. 数据库与审计

- Controller 只创建和迁移 `DATABASE_SCHEMA` 指定的项目 schema，不修改 `public` 或 1Panel 管理资源。
- `enrollment_tokens` 不存在明文 Token 列；hash 固定 32 字节。
- 活动 IP 由数据库部分唯一索引保护；释放地址可进入 cooling 状态。
- `audit_events` 通过 trigger 拒绝 UPDATE 和 DELETE；应用事件使用允许字段，不记录 bearer Token、Authorization header、私钥或请求 body。

## 10. 验证命令

```bash
make test-controller-db
make test-protocol-vectors
make test-e2e
make test-visual
cargo clippy --workspace --all-targets -- -D warnings
```

数据库测试固定使用独立 `XS_TEST_DATABASE_SCHEMA`，只清空项目测试表，不删除 schema、不修改 1Panel 网络或其他容器。
