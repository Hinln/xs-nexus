# Docker / 1Panel 部署与恢复

状态：M5.2 已验证开发部署生命周期  
证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`

## 1. 不可破坏边界

- `1panel-network` 只作为 `external: true` 网络引用；不得创建、删除、重建、改名或修改 IPAM。
- 不创建 PostgreSQL、Redis 或 MySQL 容器，不发布 `3306`、`5432` 或 `6379`。
- 只操作 `xs-nexus-dev` 或 `xs-nexus-rc` 项目命名容器，不执行全局 prune。
- 默认端口只绑定 `127.0.0.1`；公网 DNS、TLS、反向代理和防火墙开放必须经过人工批准。
- Agent、TUN、Netlink 和 nftables 不进入这些容器，仍由宿主 systemd 管理。

## 2. 组件和持久化

| 服务 | 运行用户 | 持久状态 | 说明 |
|---|---:|---|---|
| Controller | `65532:65532` | 外部 PostgreSQL schema | HTTP、WebSocket 和地址发现 |
| Relay | `65532:65532` | 无 | UDP Relay；身份密钥由只读 Secret 提供，并向内部 Controller 推送身份签名的脱敏累计指标 |
| Console | `101:101` | 无 | 静态资源和 Controller 反向代理 |
| migration | `65532:65532` | 外部 PostgreSQL schema | 一次性执行，服务激活前退出 |
| db-tools | `65532:65532` | 宿主备份目录 | 一次性备份、校验和恢复 |

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
- 备份目录和部署状态目录；
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
<relay-secret-dir>/controller-credential-public-key
<relay-secret-dir>/identity-key
```

要求：

- Secret 目录和备份目录 UID 为 `65532`、模式 `0700`；
- Secret 文件 UID 为 `65532`、模式 `0400` 或 `0600`；
- 状态目录模式不允许 group/other 权限；
- 所有路径必须是绝对路径、非符号链接；
- `database-url`、管理 Token 和 Bootstrap 密码为单行；两个 Controller 私钥、Relay 私钥和 Controller 公钥均为精确 32 个原始字节；
- `relay-catalog.json` 必须使用 Controller 认可的严格格式，不得包含 Relay 私钥。

示例目录初始化：

```bash
sudo install -d -o 65532 -g 65532 -m 0700 \
  /etc/xs-nexus/deployments/dev/controller \
  /etc/xs-nexus/deployments/dev/relay \
  /var/backups/xs-nexus/dev
sudo install -d -m 0700 /var/lib/xs-nexus-deploy/dev
```

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

`preflight` 会验证环境隔离、Secret 权限、外部网络精确定义、Compose 安全属性、无数据库服务和无数据库端口。`deploy` 的顺序固定为：

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
sudo "$STACK" --env-file "$ENV_FILE" restore before-change-20260731 \
  --confirm-schema xs_nexus_dev
```

备份由 PostgreSQL 18 `pg_dump` 生成自定义格式归档，并写入固定五字段清单：schema、归档名、字节数、SHA-256 和 UTC 时间。校验同时执行大小、SHA-256、时间格式和 `pg_restore --list` 检查。

恢复必须精确确认当前 schema。脚本先停止 Controller，再创建恢复前安全备份、删除目标 schema、创建空 schema 并受限恢复；恢复失败时删除失败 schema 并用安全备份回滚，最后按原状态恢复 Controller。

当前本机备份尚未实现静态加密、异机复制和正式保留策略，见 `KI-015`。任何恢复前先把归档和清单复制到受控离线位置。

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
- 配置备份加密、异机复制、保留/删除策略并再次演练恢复；
- 轮换所有临时密码、Bootstrap 密码、管理 Token 和在线签名密钥。

## 9. 验证

```bash
make test-docker-deployment
make validate-m52
```

验证完成后必须确认没有 `xs-nexus-dev`/`xs-nexus-rc` 容器，`1panel-network` 成员、Docker 网络、默认路由和 nftables 与验证前一致。
