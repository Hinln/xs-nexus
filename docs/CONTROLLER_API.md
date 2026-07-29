# Controller API（M1.1）

状态：已实现并由 PostgreSQL 集成测试覆盖  
日期：2026-07-29  
安全状态：bootstrap 控制面，尚未实现最终用户、RBAC 和生产 TLS 部署。

## 1. 边界与传输

- Controller 提供 enrollment、配置同步、健康检查和临时管理员入口；不承载节点业务数据。
- HTTP JSON body 上限为 64 KiB；未知 JSON 字段拒绝。
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
| `CREDENTIAL_SIGNING_KEY_PATH` | 权限不宽于 `0600` 的 32 字节原始 Ed25519 seed 文件 |
| `CONFIG_SIGNING_KEY_PATH` | 与凭证密钥不同的 32 字节原始 Ed25519 seed 文件 |
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

## 4. Bootstrap 管理 API

M1.1 使用 `Authorization: Bearer <ADMIN_API_TOKEN>`。这是部署 bootstrap 机制，不是最终用户认证或 RBAC；后续里程碑必须替换，且 Console 不得持久化该 Token。

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

## 5. 节点 Enrollment

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

## 6. 签名配置

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

## 7. WebSocket 控制连接

### `GET /v1/control`

消息和 frame 上限均为 4096 字节。连接流程：

1. Controller 发送 `{"type":"challenge","challenge_base64":"..."}`。
2. 节点在 10 秒内发送 `authenticate`，携带 Node ID、200 字节凭证和节点签名。
3. 签名输入为 `"XS Nexus control authentication v1" || challenge[32] || node_id[16]`。
4. Controller 验证凭证、数据库身份、公钥、Network ID、有效期和签名后发送 `authenticated` 与最新配置。
5. 节点发送 `{"type":"sync","last_version":2}`；服务端返回 `configuration` 或 `up_to_date`。

challenge 每连接一次性生成。二进制、未知或越序消息被拒绝；连接使用 ping/pong 心跳。控制通道只传签名配置和状态，不传业务数据。

## 8. 数据库与审计

- Controller 只创建和迁移 `DATABASE_SCHEMA` 指定的项目 schema，不修改 `public` 或 1Panel 管理资源。
- `enrollment_tokens` 不存在明文 Token 列；hash 固定 32 字节。
- 活动 IP 由数据库部分唯一索引保护；释放地址可进入 cooling 状态。
- `audit_events` 通过 trigger 拒绝 UPDATE 和 DELETE；应用事件使用允许字段，不记录 bearer Token、Authorization header、私钥或请求 body。

## 9. 验证命令

```bash
make test-controller-db
make test-protocol-vectors
cargo clippy --workspace --all-targets -- -D warnings
```

数据库测试固定使用独立 `XS_TEST_DATABASE_SCHEMA`，只清空项目测试表，不删除 schema、不修改 1Panel 网络或其他容器。
