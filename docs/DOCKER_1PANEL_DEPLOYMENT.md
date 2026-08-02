# Docker / 1Panel 部署与恢复

状态：M5.2 已验证开发部署生命周期  
证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`

## 1. 不可破坏边界

- `1panel-network` 只作为 `external: true` 网络引用；不得创建、删除、重建、改名或修改 IPAM。
- 不创建 PostgreSQL、Redis 或 MySQL 容器，不发布 `3306`、`5432` 或 `6379`。
- 只操作 `xs-nexus-dev` 或 `xs-nexus-rc` 项目命名容器，不执行全局 prune。
- HTTP 管理面固定只绑定 `127.0.0.1` 并置于 TLS 反向代理之后；Discovery/Relay UDP 由独立 `XS_UDP_BIND_ADDRESS` 控制，默认同样绑定回环，只有经过批准的公网测试或生产环境才可显式设为 `0.0.0.0`。公网 DNS、TLS、反向代理和防火墙开放必须经过人工批准。
- Agent、TUN、Netlink 和 nftables 不进入这些容器，仍由宿主 systemd 管理。

## 2. 组件和持久化

| 服务 | 运行用户 | 持久状态 | 说明 |
|---|---:|---|---|
| Controller | `65532:65532` | 外部 PostgreSQL schema | HTTP、WebSocket 和地址发现 |
| Relay | `65532:65532` | 无 | UDP Relay；身份密钥由只读 Secret 提供，并向内部 Controller 推送身份签名的脱敏累计指标 |
| Console | `101:101` | 无 | 静态资源和 Controller 反向代理 |
| migration | `65532:65532` | 外部 PostgreSQL schema | 一次性执行，服务激活前退出 |
| db-tools | `65532:65532` | 本地加密备份目录和独立副本挂载 | 一次性加密、复制、校验、取回、保留和恢复 |

所有服务使用只读根文件系统、丢弃全部 capability、启用 `no-new-privileges`、PID 上限、tmpfs 和有界日志轮转。

Compose 把 Relay 指标目标固定为同一隔离项目中的 Controller `/v1/relay-metrics`。容器内部 HTTP 必须显式设置隔离网络 opt-in；该选择只省略同主机容器链路的 TLS，不关闭 Ed25519 报告签名、Relay 目录公钥验证、重放/回滚拒绝或样本上限。跨主机和生产非隔离链路必须使用 HTTPS。

## 3. 环境隔离

先复制模板到仓库外，不要直接修改模板：

```bash
sudo install -d -m 0700 /etc/xs-nexus/deployments
sudo install -m 0600 deploy/docker/dev.compose.env.example \
  /etc/xs-nexus/deployments/dev.compose.env
sudo install -m 0600 deploy/docker/rc.compose.env.example \
  /etc/xs-nexus/deployments/rc.compose.env
```

开发和 RC 必须分别使用：

- Compose 项目：`xs-nexus-dev` / `xs-nexus-rc`；
- schema：后缀 `_dev` / `_rc`；
- Controller、Console、Discovery、Relay 端口；
- Controller/Relay Secret 目录；
- 本地备份目录、独立副本挂载和部署状态目录；
- 镜像标签。RC 的 `XS_RELEASE_REVISION` 必须等于干净 Git HEAD，镜像 revision 标签必须一致。

## 4. Secret 和目录

环境文件只保存路径和非秘密配置。真实值必须放入仓库外文件：

```text
<controller-secret-dir>/database-url
<controller-secret-dir>/admin-api-token
<controller-secret-dir>/console-bootstrap-password
<controller-secret-dir>/credential-signing-key
<controller-secret-dir>/configuration-signing-key
<controller-secret-dir>/relay-catalog.json
<controller-secret-dir>/backup-recipient
<relay-secret-dir>/controller-credential-public-key
<relay-secret-dir>/identity-key
```

要求：

- Secret、本地备份和副本目录 UID 为 `65532`、模式 `0700`；
- Secret 文件 UID 为 `65532`、模式 `0400` 或 `0600`；
- 状态目录模式不允许 group/other 权限；
- 所有路径必须是绝对路径、非符号链接；
- `database-url`、管理 Token 和 Bootstrap 密码为单行；两个 Controller 私钥、Relay 私钥和 Controller 公钥均为精确 32 个原始字节；
- `relay-catalog.json` 必须使用 Controller 认可的严格格式，不得包含 Relay 私钥。
- `backup-recipient` 只包含一行 age X25519 public recipient；对应 identity 不得存放在数据库主机或 Controller Secret 目录。
- `XS_BACKUP_REPLICA_DIR` 必须是与本地备份目录不同设备号的独立挂载，并含 UID `65532`、模式 `0600` 的 `.xs-nexus-replica` marker；仅创建另一个本机目录不会通过预检。

示例目录初始化：

```bash
sudo install -d -o 65532 -g 65532 -m 0700 \
  /etc/xs-nexus/deployments/dev/controller \
  /etc/xs-nexus/deployments/dev/relay \
  /var/backups/xs-nexus/dev
sudo install -d -m 0700 /var/lib/xs-nexus-deploy/dev
```

在独立的离线设备生成 identity；只把输出的 public recipient 通过认证渠道写入数据库主机：

```bash
umask 077
age-keygen -o xs-nexus-backup-identity.txt 2>age-keygen.log
# 从 age-keygen.log 取得 Public key: age1...；identity 留在离线设备。

printf '%s\n' 'age1REPLACE_WITH_OFFLINE_PUBLIC_RECIPIENT' |
  sudo install -o 65532 -g 65532 -m 0400 /dev/stdin \
    /etc/xs-nexus/deployments/dev/controller/backup-recipient
```

把异地主机、网络文件系统或受控对象存储网关挂载到环境文件的 `XS_BACKUP_REPLICA_DIR`，然后初始化 marker；`target_id` 必须稳定标识真实故障域：

```bash
REPLICA=/mnt/xs-nexus-offsite/dev
sudo install -d -o 65532 -g 65532 -m 0700 "$REPLICA"
printf '%s\n' \
  'format=xs-nexus-replica-v1' \
  'deployment=dev' \
  'target_id=backup-host-a' |
  sudo install -o 65532 -g 65532 -m 0600 /dev/stdin \
    "$REPLICA/.xs-nexus-replica"
```

环境文件同时设置 `XS_BACKUP_LOCAL_RETENTION_DAYS`、`XS_BACKUP_REPLICA_RETENTION_DAYS` 和 `XS_BACKUP_MIN_RETAINED`。副本保留期不得短于本地；RC 最低分别为 7 天、30 天和 3 份。

不得把数据库 URI、管理 Token、密码或私钥作为命令参数、Compose 环境变量或镜像构建参数。初始化完成后运行仓库秘密扫描。

## 5. 预检、构建和部署

```bash
STACK=./deploy/docker/xs-nexus-stack.sh
ENV_FILE=/etc/xs-nexus/deployments/dev.compose.env

sudo "$STACK" --env-file "$ENV_FILE" preflight
sudo "$STACK" --env-file "$ENV_FILE" build
sudo "$STACK" --env-file "$ENV_FILE" deploy
sudo "$STACK" --env-file "$ENV_FILE" status
```

公网 HTTPS 部署不依赖 1Panel 站点配置。项目提供独立、digest-pinned 的 Caddy Edge；它以非 root UID、只读根文件系统、零 capability 和持久证书目录运行，端口 80/443 只反向代理同一 Compose 项目内的 Console。Console 再代理 `/v1/`、`/health/` 和 WebSocket 到 Controller，因此 Controller 与 Console 的宿主 HTTP 端口始终保持 `127.0.0.1`。把 `Caddyfile.example` 复制到仓库外，逐个域名使用独立站点块，避免一个错误 DNS 记录阻塞其他证书：

```bash
sudo install -o root -g root -m 0444 deploy/docker/Caddyfile.example \
  /etc/xs-nexus/deployments/dev/Caddyfile
sudo install -d -o 65532 -g 65532 -m 0700 \
  /var/lib/xs-nexus-deploy/dev/edge

PUBLIC_STACK=./deploy/docker/xs-nexus-public-deploy.sh
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" build
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" preflight
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" deploy
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" status
```

`XS_EDGE_IMAGE` 必须包含 SHA-256 digest；Edge 脚本拒绝 tag-only 镜像、宽权限配置/证书目录、root 用户、可写根文件系统、额外 capability、错误外部网络和无健康应用上游。Caddy 自动执行 ACME HTTP-01/HTTPS、HTTP 到 HTTPS 跳转、证书续期及 WebSocket 透传。公网部署前必须确保每个域名只有指向当前服务器的批准 A/AAAA 记录，并开放 TCP 80/443；错误或多余地址属于证书和流量一致性失败。

`preflight` 会验证环境隔离、Secret 权限、备份 public recipient、副本设备/marker/保留期、外部网络精确定义、Compose 安全属性、无数据库服务和无数据库端口。`deploy` 的顺序固定为：

1. 验证镜像；
2. schema 已存在时创建迁移前备份；
3. 运行一次性 migration；
4. 记录当前健康服务镜像；
5. 激活 Controller、Relay、Console 并等待健康；
6. 写入私有活动部署记录。

迁移失败不会替换当前服务。激活失败会尝试恢复上一组 Controller、Relay、Console 镜像；没有完整上一组镜像时只停止项目栈，不影响外部网络。

## 6. 备份和恢复

```bash
sudo "$STACK" --env-file "$ENV_FILE" backup before-change-20260731
sudo "$STACK" --env-file "$ENV_FILE" verify-backup before-change-20260731
sudo "$STACK" --env-file "$ENV_FILE" verify-backup-deep before-change-20260731 \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
sudo "$STACK" --env-file "$ENV_FILE" restore before-change-20260731 \
  --confirm-schema xs_nexus_dev \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
```

PostgreSQL 18 `pg_dump` 自定义格式流直接进入 age X25519 加密；明文数据库归档不写入本地或副本目录。每个不可变备份包含 `.dump.age`、`.manifest.age`、公开 `.index` 和复制回执；认证 manifest 绑定 schema、备份名、密文字节数/SHA-256、recipient Key ID 和 UTC 时间。每次 `backup` 自动复制到独立挂载并在两端写入一致回执。

`verify-backup` 不需要私钥，验证两端文件、密文 hash、index 和复制回执一致；它只能证明传输完整性。`verify-backup-deep` 需要临时只读挂载离线 identity，完整认证 manifest 和归档密文，再执行 `pg_restore --list`；错误 identity、任意密文篡改或字段漂移均失败关闭。identity 文件必须是绝对路径、非链接、UID `65532` 且无 group/other 权限，操作结束后立即卸载离线介质。

recipient 轮换后，历史备份仍由其原 recipient Key ID 绑定。跨轮换恢复时，传入的受控 identity 文件必须包含目标历史 key 与当前 public recipient 对应 key；脚本会先证明目标备份可解密，再证明当前 key 创建的恢复前安全备份也可解密，任一缺失都在删除 schema 前失败关闭。

恢复必须精确确认当前 schema 并提供离线 identity。脚本先停止 Controller，再创建并自动复制恢复前加密安全备份、删除目标 schema、创建空 schema并受限流式解密恢复；恢复失败时删除失败 schema 并用安全备份回滚，最后按原状态恢复 Controller。

异地取回、幂等重试和保留清理命令：

```bash
sudo "$STACK" --env-file "$ENV_FILE" replicate-backup before-change-20260731
sudo "$STACK" --env-file "$ENV_FILE" fetch-backup before-change-20260731
sudo "$STACK" --env-file "$ENV_FILE" prune-backups \
  --confirm-before 2026-08-02T00:00:00Z \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
```

`prune-backups` 只处理时间早于显式确认值且通过 identity 认证的备份，始终保留最新的最小份数。本地删除前必须有有效副本；副本销毁前写入包含原始 hashes、创建/销毁时间和 target ID 的不可覆盖墓碑，已销毁名称不能复用。生产 identity 仪式、真实异地主机和生产恢复演练仍由 `BLK-007` 阻塞。

## 7. 手工回滚和停止

```bash
sudo "$STACK" --env-file "$ENV_FILE" rollback
sudo "$STACK" --env-file "$ENV_FILE" down
```

`rollback` 只使用上次部署保存的三个镜像 ID；`down` 只删除当前项目容器，不删除 Secret、备份、外部数据库、`1panel-network` 或任何 1Panel 资源。

## 8. 上线前人工门禁

- 由管理员在 1Panel/反向代理配置正式 HTTPS 和 WebSocket；
- 只开放获批的 Controller/Discovery/Relay 端口，Console 管理面不得裸露 HTTP；
- 修复并复测 `KI-006` 中既有 PostgreSQL/Redis 公网暴露；
- 使用固定摘要和干净提交构建 RC 镜像，生成 SBOM、漏洞报告和来源证明；
- 完成 `BLK-007` 的正式 identity 仪式、真实异地主机挂载和生产恢复演练；
- 轮换所有临时密码、Bootstrap 密码、管理 Token 和在线签名密钥。

## 9. 验证

```bash
make test-docker-deployment
make validate-m52
```

验证完成后必须确认没有 `xs-nexus-dev`/`xs-nexus-rc` 容器，`1panel-network` 成员、Docker 网络、默认路由和 nftables 与验证前一致。
