# XS Nexus 生产部署、验收与运维手册

文档日期：2026-08-02  
适用范围：Linux 服务端、Linux Agent、Docker Compose、1Panel 外部网络、项目自带 Caddy HTTPS Edge  
不适用范围：尚未完成实机门禁的 Windows Agent/`xsnet` 驱动

> 重要结论：本手册描述的是当前代码能够执行的完整部署流程，不是“已经生产就绪”的声明。当前项目仍为部分完成，尚受 Windows VM/WDK/正式驱动签名、真实 NAS、正式离线发布与备份密钥仪式、真实异地恢复、数据库公网暴露整改、生产防火墙和第三方协议/密码学审计等门禁约束。权威状态以 `FINAL_REPORT.md`、`RELEASE_CHECKLIST.md`、`BLOCKERS.md` 和 `KNOWN_ISSUES.md` 为准。

## 1. 部署目标和组件

推荐部署模型如下：

```text
Internet
   |
   | TCP 80/443
   v
Caddy Edge（自动 ACME、HTTPS、WebSocket）
   |
   | 1panel-network 内部 HTTP
   v
Console / NGINX ----------------> Controller
                                      |
                                      | PostgreSQL
                                      v
                              现有 1Panel 数据库

Internet / Agent
   | UDP Discovery
   +---------------------------> Controller
   |
   | UDP Relay（XSP/1 端到端密文）
   +---------------------------> Relay
```

| 组件 | 部署方式 | 责任 | 持久数据 |
|---|---|---|---|
| `xs-controller` | Docker | 用户、网络、Enrollment、IPAM、ACL、路由、签名配置、WebSocket | 外部 PostgreSQL schema |
| `xs-console` | Docker | 管理控制台和 Controller HTTP/WebSocket 反向代理 | 无 |
| `xs-relay` | Docker | 认证租约、限速、有界队列、端到端密文转发 | 无；身份密钥在 Secret 文件 |
| `edge` | Docker | ACME、HTTPS、HTTP 跳转、安全响应头、WebSocket 透传 | Caddy 证书目录 |
| `migration` | 一次性容器 | 部署前数据库迁移 | 外部 PostgreSQL schema |
| `db-tools` | 一次性容器 | age 加密备份、复制、校验、恢复、保留 | 本地密文目录与独立副本挂载 |
| `xs-agent` | 宿主机 systemd | Linux TUN、Netlink、XSP/1、Direct/Relay、ACL、路由 | `/etc/xs-nexus`、`/var/lib/xs-nexus` |

Controller 不承载普通业务数据；Agent 优先建立 Direct UDP，失败后才经 Relay 转发同一份端到端密文。

## 2. 上线前硬性门禁

正式对外开放前逐项确认：

- Git 工作树干净，部署 revision 是经过审核的固定提交，不使用浮动分支作为版本号；
- Docker Engine 和 Compose v2 可用；
- 已存在名为 `1panel-network` 的 bridge 网络，当前预检要求其子网精确为 `172.18.0.0/16`；
- PostgreSQL 位于受控网络，项目使用独立账号和独立 schema，不开放新的公网数据库端口；
- Controller/Console 的宿主 HTTP 只绑定 `127.0.0.1`；
- 仅 TCP `80/443` 和选定的两个 UDP 端口面向公网；
- 每个域名只解析到当前服务器的获批 A/AAAA 地址，不保留旧 IP；
- Caddy 数据目录可持久化，域名证书申请不会因错误 DNS、端口占用或代理拦截失败；
- 所有 Secret 位于仓库外，权限和所有者通过预检；
- 备份副本目录是真实不同设备号的挂载，不是同一磁盘上的另一个普通目录；
- 已完成备份 identity 的离线保管，并至少执行一次深度校验和恢复演练；
- 临时管理员密码、API Token、数据库密码和测试密钥在正式上线前全部轮换；
- 当前镜像漏洞 disposition 在有效期内；基础镜像、二进制导入或扫描结果变化时重新扫描；
- Windows 客户端不得投入使用，直到 `BLK-001`、`BLK-004`、`KI-018`、`KI-020` 对应实机门禁通过。

## 3. 主机、网络和端口要求

### 3.1 主机工具

部署主机至少需要：

- Linux x86_64；当前证据来自 Ubuntu 开发服务器；
- Git、Bash、OpenSSL、Python 3、`ss`；
- Docker Engine 和 `docker compose` 插件；
- 构建镜像所需的 CPU、内存和磁盘空间；
- 为生成 Ed25519 公钥准备 Rust/Cargo，或在受控构建机生成后通过认证渠道传入；
- Linux Agent 节点额外需要 systemd、`/dev/net/tun`、nftables 和受控 `CAP_NET_ADMIN`。

### 3.2 推荐端口表

端口可以调整，但同类端口不能冲突。

| 方向 | 协议 | 示例端口 | 用途 | 公网策略 |
|---|---|---:|---|---|
| Internet → Edge | TCP | 80 | ACME HTTP-01、HTTP→HTTPS | 开放 |
| Internet → Edge | TCP | 443 | Console、Controller API、WebSocket | 开放 |
| Internet/Agent → Controller | UDP | 42000 | 地址发现 | 开放到 Agent 来源范围；无法限制时启用云防火墙限速/监控 |
| Internet/Agent → Relay | UDP | 42001 | Relay 密文转发 | 开放到 Agent 来源范围；无法限制时启用云防火墙限速/监控 |
| Host loopback → Controller | TCP | 28080 | 诊断/本机代理入口 | 仅 `127.0.0.1` |
| Host loopback → Console | TCP | 28081 | 本机 Console 入口 | 仅 `127.0.0.1` |
| XS 容器 → PostgreSQL | TCP | 5432 | 数据库 | 只在 `1panel-network` 或受控私网 |

不要开放 `3306`、`5432` 或 `6379` 到公网。SSH 管理端口不属于项目部署脚本的管理范围，变更防火墙时必须保留现有 SSH 会话和回滚通道。

## 4. 获取并固定发布代码

不要在在线服务器上直接跟随未审核的 `main`。先选择明确分支、标签或提交：

```bash
git clone https://github.com/Hinln/xs-nexus.git /srv/xs-nexus
cd /srv/xs-nexus
git fetch --all --prune
git checkout <approved-branch-or-tag>
git pull --ff-only
git status --short --branch

REVISION=$(git rev-parse HEAD)
test -z "$(git status --porcelain)"
printf 'release_revision=%s\n' "$REVISION"
```

RC 预检会同时要求：

- 工作树干净；
- `XS_RELEASE_REVISION` 等于当前 `HEAD`；
- Controller、Migration、Relay、Console、db-tools 和 Edge 镜像的 OCI revision 标签等于该提交。

## 5. 环境目录和配置文件

生产前建议先在 `rc` 隔离环境完成全流程。复制模板到仓库外：

```bash
cd /srv/xs-nexus

sudo install -d -m 0700 /etc/xs-nexus/deployments
sudo install -m 0600 deploy/docker/rc.compose.env.example \
  /etc/xs-nexus/deployments/rc.compose.env

sudo install -d -o 65532 -g 65532 -m 0700 \
  /etc/xs-nexus/deployments/rc/controller \
  /etc/xs-nexus/deployments/rc/relay \
  /var/backups/xs-nexus/rc

sudo install -d -m 0700 /var/lib/xs-nexus-deploy/rc
sudo install -d -o 65532 -g 65532 -m 0700 \
  /var/lib/xs-nexus-deploy/rc/edge
```

编辑 `/etc/xs-nexus/deployments/rc.compose.env`，至少替换：

```dotenv
XS_DEPLOYMENT=rc
XS_COMPOSE_PROJECT_NAME=xs-nexus-rc
XS_RELEASE_REVISION=<git-commit>
XS_CONTROLLER_IMAGE=xs-nexus/controller:<git-commit>
XS_MIGRATION_IMAGE=xs-nexus/controller:<git-commit>
XS_RELAY_IMAGE=xs-nexus/relay:<git-commit>
XS_CONSOLE_IMAGE=xs-nexus/console:<git-commit>
XS_DB_TOOLS_IMAGE=xs-nexus/db-tools:<git-commit>
XS_EDGE_IMAGE=xs-nexus/edge:<git-commit>

XS_DATABASE_SCHEMA=xs_nexus_rc
XS_BIND_ADDRESS=127.0.0.1
XS_UDP_BIND_ADDRESS=0.0.0.0
XS_CONTROLLER_HTTP_PORT=28080
XS_CONSOLE_HTTP_PORT=28081
XS_DISCOVERY_UDP_PORT=42000
XS_RELAY_UDP_PORT=42001
XS_DISCOVERY_PUBLIC_ENDPOINT=<public-ip>:42000
XS_RELAY_ID_BASE64=<base64url-16-byte-id>

XS_CONSOLE_COOKIE_SECURE=true
XS_EDGE_BIND_ADDRESS=0.0.0.0
XS_EDGE_HTTP_PORT=80
XS_EDGE_HTTPS_PORT=443
```

若 UDP 不对公网开放，保持 `XS_UDP_BIND_ADDRESS=127.0.0.1`。对公网测试或生产时才改为 `0.0.0.0`，并同步配置云防火墙和系统防火墙。

## 6. Secret 初始化

环境文件只保存路径和非秘密设置。以下文件必须位于仓库外：

```text
/etc/xs-nexus/deployments/rc/controller/database-url
/etc/xs-nexus/deployments/rc/controller/admin-api-token
/etc/xs-nexus/deployments/rc/controller/console-bootstrap-password
/etc/xs-nexus/deployments/rc/controller/credential-signing-key
/etc/xs-nexus/deployments/rc/controller/configuration-signing-key
/etc/xs-nexus/deployments/rc/controller/relay-catalog.json
/etc/xs-nexus/deployments/rc/controller/backup-recipient
/etc/xs-nexus/deployments/rc/relay/controller-credential-public-key
/etc/xs-nexus/deployments/rc/relay/identity-key
```

### 6.1 生成基础 Secret

以下命令只把秘密写入受限文件，不打印秘密：

```bash
sudo -i
umask 077

CONTROLLER=/etc/xs-nexus/deployments/rc/controller
RELAY=/etc/xs-nexus/deployments/rc/relay

printf '%s' '<postgresql-uri-from-secure-source>' > "$CONTROLLER/database-url"
openssl rand -hex 32 > "$CONTROLLER/admin-api-token"
openssl rand -base64 36 | tr -d '\n' > "$CONTROLLER/console-bootstrap-password"
openssl rand 32 > "$CONTROLLER/credential-signing-key"
openssl rand 32 > "$CONTROLLER/configuration-signing-key"
openssl rand 32 > "$RELAY/identity-key"

chown 65532:65532 "$CONTROLLER"/* "$RELAY"/*
chmod 0400 "$CONTROLLER"/* "$RELAY"/*
```

数据库账号应只拥有目标项目 schema 所需权限，不应是 PostgreSQL 超级用户，也不应复用 1Panel 管理账号。

### 6.2 派生 Relay 所需公钥

从 Controller 凭据签名种子派生 Relay 验证公钥；从 Relay 身份种子派生目录公钥：

```bash
cd /srv/xs-nexus
umask 077

cargo run --quiet -p xs-protocol \
  --example derive_ed25519_public -- \
  /etc/xs-nexus/deployments/rc/controller/credential-signing-key \
  /etc/xs-nexus/deployments/rc/relay/controller-credential-public-key

cargo run --quiet -p xs-protocol \
  --example derive_ed25519_public -- \
  /etc/xs-nexus/deployments/rc/relay/identity-key \
  /var/lib/xs-nexus-deploy/rc/relay-public-key

sudo chown 65532:65532 \
  /etc/xs-nexus/deployments/rc/relay/controller-credential-public-key
sudo chmod 0400 \
  /etc/xs-nexus/deployments/rc/relay/controller-credential-public-key
```

如果生产主机不安装 Cargo，应在受控构建机执行同一仓库、同一提交中的示例，并通过认证渠道传输公钥文件；不要传输私钥种子。

### 6.3 创建 Relay ID 和目录

生成 16 字节非零 Relay ID，并把同一值写入环境文件的 `XS_RELAY_ID_BASE64`：

```bash
python3 - <<'PY'
import base64
import os
print(base64.urlsafe_b64encode(os.urandom(16)).rstrip(b'=').decode())
PY
```

创建目录时，`endpoint` 必须是 Agent 可访问的公网 IP 和 Relay UDP 端口；`expires_at` 必须处于未来，并建立轮换提醒：

```bash
sudo python3 - \
  /etc/xs-nexus/deployments/rc/controller/relay-catalog.json \
  '<same-relay-id-base64url>' \
  '<public-ip>:42001' \
  /var/lib/xs-nexus-deploy/rc/relay-public-key <<'PY'
import base64
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

output, relay_id, endpoint, public_key_path = sys.argv[1:]
catalog = [{
    'relay_id_base64': relay_id,
    'endpoint': endpoint,
    'identity_public_key_base64': base64.urlsafe_b64encode(
        Path(public_key_path).read_bytes()
    ).rstrip(b'=').decode(),
    'priority': 100,
    'expires_at': (
        datetime.now(timezone.utc) + timedelta(days=365)
    ).isoformat().replace('+00:00', 'Z'),
}]
Path(output).write_text(json.dumps(catalog, separators=(',', ':')), encoding='utf-8')
PY

sudo chown 65532:65532 \
  /etc/xs-nexus/deployments/rc/controller/relay-catalog.json
sudo chmod 0400 \
  /etc/xs-nexus/deployments/rc/controller/relay-catalog.json
sudo rm -f /var/lib/xs-nexus-deploy/rc/relay-public-key
```

Controller 最多接受 16 个 Relay，并拒绝零 ID、重复 ID、重复端点、重复公钥、无效公钥、零优先级和已过期目录项。

### 6.4 备份 recipient 和副本挂载

在离线设备生成 age identity；数据库主机只保存 public recipient：

```bash
umask 077
age-keygen -o xs-nexus-backup-identity.txt 2>age-keygen.log
# 通过认证渠道取得 age1... Public key；identity 文件留在离线介质。
```

把 public recipient 写入数据库主机：

```bash
printf '%s\n' 'age1REPLACE_WITH_OFFLINE_PUBLIC_RECIPIENT' |
  sudo install -o 65532 -g 65532 -m 0400 /dev/stdin \
  /etc/xs-nexus/deployments/rc/controller/backup-recipient
```

把真实异地主机、网络文件系统或受控对象存储网关挂载到 `XS_BACKUP_REPLICA_DIR`。该路径必须与本地备份目录具有不同设备号：

```bash
REPLICA=/mnt/xs-nexus-offsite/rc
sudo install -d -o 65532 -g 65532 -m 0700 "$REPLICA"
printf '%s\n' \
  'format=xs-nexus-replica-v1' \
  'deployment=rc' \
  'target_id=production-backup-target-a' |
  sudo install -o 65532 -g 65532 -m 0600 /dev/stdin \
  "$REPLICA/.xs-nexus-replica"

stat -c '%d %n' /var/backups/xs-nexus/rc "$REPLICA"
```

两个设备号必须不同。RC 保留策略最低为本地 7 天、副本 30 天、至少 3 份；模板使用更保守的 30/180/3。

## 7. DNS、Caddy、HTTPS 和 WebSocket

### 7.1 DNS 检查

在部署 Edge 前检查每个域名：

```bash
dig +short A console.example.com
dig +short AAAA console.example.com
dig +short A controller.example.com
```

删除旧 IP、错误 AAAA 和不再使用的 CDN/代理记录。若使用代理型 DNS 服务，首次签发证书和 WebSocket 测试期间应确认其端口、TLS 模式和回源策略与 Caddy 相容。

### 7.2 Caddyfile

复制模板到仓库外，并将上游改为 RC Console 的网络别名：

```bash
sudo install -o root -g root -m 0444 deploy/docker/Caddyfile.example \
  /etc/xs-nexus/deployments/rc/Caddyfile
```

推荐配置：

```caddyfile
{
    admin off
    http_port 8080
    https_port 8443
    storage file_system /data/caddy
}

(xs_nexus_proxy) {
    encode zstd gzip
    reverse_proxy xs-nexus-rc-console:8080
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
        Referrer-Policy "no-referrer"
        -Server
    }
}

console.example.com {
    import xs_nexus_proxy
}

controller.example.com {
    import xs_nexus_proxy
}
```

多个测试域名可以分别建立站点块并代理到同一 Console。不要把多个名称写成一个证书必须同时成功的组合块；独立站点块可避免单个错误 DNS 记录阻塞其他域名。

Caddy 自动处理：

- ACME 证书申请和续期；
- HTTP→HTTPS；
- `/v1/`、`/health/` 和 WebSocket 经 Console 转发到 Controller；
- HSTS、`nosniff`、`DENY` 和 Referrer Policy；
- WebSocket Upgrade/Connection 透传。

## 8. 构建、预检和部署

统一入口：

```bash
cd /srv/xs-nexus
ENV_FILE=/etc/xs-nexus/deployments/rc.compose.env
PUBLIC_STACK=./deploy/docker/xs-nexus-public-deploy.sh

sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" build
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" preflight
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" deploy
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" status
```

`preflight` 会拒绝：

- 脏工作树或 revision 不一致；
- 镜像 revision 标签不一致；
- Controller/Console 不是 loopback HTTP；
- 端口冲突；
- Secret 路径、权限、所有者或文件类型不正确；
- 无效备份 recipient、同设备副本或 marker 错误；
- RC 保留期不足；
- `1panel-network` 名称、driver 或子网与批准基线不一致；
- root 容器、可写根文件系统、额外 capability；
- Compose 中出现数据库容器或数据库公网端口。

部署顺序为：验证镜像 → 迁移前加密备份 → migration → 保存上一组健康镜像 → 激活 Controller/Relay/Console → 激活 Edge → 等待健康。

迁移失败不会替换当前服务。应用激活失败会恢复上一组镜像；Edge 独立失败不会修改外部数据库或 `1panel-network`。

## 9. 部署后验收

### 9.1 容器和安全属性

```bash
sudo "$PUBLIC_STACK" --env-file "$ENV_FILE" status
docker ps --filter label=com.docker.compose.project=xs-nexus-rc
docker inspect xs-nexus-rc-controller-1 --format \
  'user={{.Config.User}} readonly={{.HostConfig.ReadonlyRootfs}} capdrop={{json .HostConfig.CapDrop}} security={{json .HostConfig.SecurityOpt}}'
```

期望：Controller、Relay、Console、Edge 均 healthy；非 root；只读根文件系统；`CapDrop=ALL`；`no-new-privileges`。

### 9.2 HTTP/HTTPS

```bash
curl -fsSI http://console.example.com/
curl -fsSI https://console.example.com/
curl -fsS https://console.example.com/console-health
curl -fsS https://console.example.com/health/ready
```

期望：HTTP 返回 308 或等价永久 HTTPS 跳转；HTTPS 页面 200；Console 和 Controller 健康端点成功；证书链和主机名校验通过。

### 9.3 WebSocket

必须使用真实 WebSocket 客户端检查 Controller 控制通道返回 `101 Switching Protocols`。只用普通 `curl` 访问页面不能证明 WebSocket 可用。浏览器登录后同时检查开发者工具中无未解释的 4xx/5xx、TLS 或 WebSocket 错误。

### 9.4 UDP

从服务器确认监听：

```bash
ss -lunp | grep -E ':(42000|42001)\b'
```

再从外部 Linux Agent 或网络探针分别验证 Discovery 和 Relay UDP。云防火墙与系统防火墙必须同时允许；TCP 端口探测不能替代 UDP 数据面验证。

## 10. 初始登录和管理员处理

默认 Bootstrap 用户名来自 `XS_CONSOLE_BOOTSTRAP_USERNAME`，未设置时为：

```text
admin
```

初始密码不是仓库默认值，而是 Secret 文件：

```text
/etc/xs-nexus/deployments/rc/controller/console-bootstrap-password
```

Controller 只在用户表为空时创建 Bootstrap 管理员，并立即将密码保存为 Argon2id 哈希，不保存明文。首次登录后应：

1. 创建实名管理员账号；
2. 创建 operator/auditor 的最小权限账号；
3. 验证审计日志；
4. 轮换 Bootstrap 密码文件和 `admin-api-token`；
5. 不把 API Token 交给浏览器或普通用户；
6. 在受控变更窗口内验证会话、CSRF、注销和权限边界。

## 11. 创建虚拟网络和加入 Linux Agent

### 11.1 控制台流程

1. 登录 Console；
2. 创建网络并选择地址池，默认可使用 `100.88.0.0/16`；
3. 检查地址池与所有节点本地 LAN、云 VPC、其他 VPN 和 CGNAT 范围冲突；
4. 创建一次性 Enrollment Token，设置有效期、最大次数和默认权限；
5. 创建最小 ACL；默认拒绝，不要先开放全网全端口；
6. 安装第一台 Linux Agent；
7. 安装第二台 Linux Agent；
8. 验证节点在线、虚拟 IP、Direct/Relay 路径和审计事件；
9. 需要子网路由时先让 Agent 提交建议，再由管理员审批，不自动发布整个 LAN。

### 11.2 Linux Agent 安装

正式发布包包含目标 archive、manifest、detached signature 和独立认证渠道取得的 Ed25519 发布公钥：

```bash
sudo ./installers/linux/xs-nexus-installer.sh install \
  --archive ./xs-nexus-<version>-x86_64-unknown-linux-gnu.tar.gz \
  --manifest ./xs-nexus-<version>-x86_64-unknown-linux-gnu.manifest \
  --signature ./xs-nexus-<version>-x86_64-unknown-linux-gnu.manifest.sig \
  --public-key /secure/release-public-key.pem \
  --config /secure/agent.json \
  --enrollment-token-file /secure/enrollment-token
```

Token 只通过受限文件传入，不进入命令行或日志。首次安装固定发布公钥；升级拒绝不同公钥和外部降级。

常用检查：

```bash
sudo systemctl status xs-agent --no-pager
sudo xs status
sudo xs peers
sudo xs netcheck
sudo xs routes
sudo xs diagnostics
sudo xs ping <peer-virtual-ip>
sudo xs path <peer-virtual-ip>
```

`xs ping` 是已认证 XSP/1 路径探测；`xs reconnect` 只重建 Controller WebSocket，不会重置系统默认路由。

## 12. 虚拟网络完整验收

至少使用两台独立 Linux 节点，按顺序验证：

1. 两节点 Enrollment 成功，虚拟 IP 唯一；
2. 双向 `xs ping` 成功；
3. ICMP、TCP、UDP 业务流量按 ACL 通过；
4. 未授权端口和未授权节点被拒绝；
5. Console 显示真实当前路径，不用缺失数据推断 Direct；
6. 正常网络下优先 Direct；
7. 人工阻断 Direct UDP 后自动切换 Relay；
8. 恢复 Direct 后通过认证路径探测回切；
9. Controller 短时停止时，既有会话和最后有效策略继续；
10. Controller 恢复后 WebSocket 自动重连和增量同步；
11. Agent 重启后身份和虚拟 IP 保持；
12. 升级失败自动恢复旧版本；
13. 默认卸载清理项目网络资源但保留身份，`--purge` 才删除身份；
14. 子网路由审批前不可达，审批后只按 ACL 可达；
15. 网关离线、撤销或暂停后路由失效；
16. 宿主默认路由、非项目 nftables、1Panel 网络和无关容器全程不变。

Windows 节点不能用源码门禁或交叉编译结果替代上述实机验收。

## 13. 备份、恢复和灾难演练

```bash
STACK=./deploy/docker/xs-nexus-stack.sh
ENV_FILE=/etc/xs-nexus/deployments/rc.compose.env
NAME=before-release-$(date -u +%Y%m%dT%H%M%SZ)

sudo "$STACK" --env-file "$ENV_FILE" backup "$NAME"
sudo "$STACK" --env-file "$ENV_FILE" verify-backup "$NAME"
sudo "$STACK" --env-file "$ENV_FILE" verify-backup-deep "$NAME" \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
```

`verify-backup` 只证明密文、公开 index 和复制回执一致；`verify-backup-deep` 才会使用离线 identity 认证 manifest、解密并运行 `pg_restore --list`。

恢复必须精确确认 schema：

```bash
sudo "$STACK" --env-file "$ENV_FILE" restore "$NAME" \
  --confirm-schema xs_nexus_rc \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
```

恢复流程会停止 Controller、创建并复制恢复前安全备份、恢复目标 schema；失败时尝试用安全备份回滚。生产上线前必须在隔离环境执行一次完整恢复，并记录 RTO、RPO、操作者、证据目录和回滚结果。

保留清理：

```bash
sudo "$STACK" --env-file "$ENV_FILE" prune-backups \
  --confirm-before 2026-08-02T00:00:00Z \
  --identity-file /media/offline/xs-nexus-backup-identity.txt
```

该命令是破坏性操作，只处理显式时间之前、通过 identity 认证且满足最小保留数的备份；副本销毁后写入墓碑，同名备份不可复用。

## 14. 镜像供应链和发布证据

在干净提交上执行：

```bash
make validate-image-supply-chain
make scan-image-vulnerabilities EVIDENCE_DIR=/absolute/evidence/path
make verify-image-vulnerability-disposition EVIDENCE_DIR=/absolute/evidence/path
```

应保存：

- 精确 image ID；
- Dockerfile hash；
- Git revision；
- CycloneDX OS 包 SBOM；
- 逐包许可证闭包；
- in-toto/SLSA provenance；
- Grype JSON 和汇总；
- High/Critical 的明确修复或有界 disposition；
- 容器、Docker 网络、默认路由和 nftables 前后不变证据。

当前已验证证据中没有“当前可修复”的 High/Critical，但仍有 glibc 残余发现；既有 disposition 最迟于 2026-08-31 复核，或在基础镜像、glibc、扫描数据库、扫描结果或二进制导入集合变化时立即失效。

## 15. 更新、回滚和停止

应用栈手工回滚：

```bash
sudo ./deploy/docker/xs-nexus-stack.sh \
  --env-file /etc/xs-nexus/deployments/rc.compose.env rollback
```

只停止 XS Nexus，不删除外部数据库、Secret、备份或 `1panel-network`：

```bash
sudo ./deploy/docker/xs-nexus-public-deploy.sh \
  --env-file /etc/xs-nexus/deployments/rc.compose.env down
```

Linux Agent：

```bash
sudo ./installers/linux/xs-nexus-installer.sh status
sudo ./installers/linux/xs-nexus-installer.sh rollback
sudo ./installers/linux/xs-nexus-installer.sh uninstall
```

不要执行 `docker system prune -a`，不要删除未知容器、网络、卷或 1Panel 资源，也不要用 `git reset --hard` 覆盖服务器专用文件。

## 16. 监控和日常运维

每日：

- 检查 Controller、Relay、Console、Edge health；
- 检查证书到期和 Caddy ACME 错误；
- 检查 Relay 丢弃、限速、I/O 错误和队列延迟；
- 检查 Agent 在线率、Direct/Relay 比例、握手失败和 RTT；
- 检查 PostgreSQL 容量、连接池和备份复制回执；
- 检查磁盘、日志轮转和备份副本挂载是否仍在独立故障域。

每周：

- 执行新的加密备份、公开校验和抽样深度校验；
- 审计管理员、Enrollment Token、ACL、路由审批和异常登录；
- 检查域名是否出现旧 A/AAAA 或未经批准的代理记录；
- 检查云防火墙的 TCP 80/443、UDP Discovery/Relay 和数据库规则。

每次发布：

- 固定提交、干净构建、SBOM、漏洞扫描和 provenance；
- 先备份并深度校验；
- 先在隔离 RC 环境部署；
- 运行 HTTPS、WebSocket、UDP、登录、双 Agent、ACL、Direct/Relay 和恢复抽测；
- 保留上一组镜像和已验证 Agent 版本，确认回滚路径后再扩大范围。

## 17. 常见故障排查

### 17.1 Edge 证书申请失败

- 检查域名是否同时指向旧 IP；
- 检查错误 AAAA；
- 检查 TCP 80/443 是否被 1Panel、其他 NGINX 或旧 Caddy 占用；
- 检查云防火墙和系统防火墙；
- 检查 Caddy 数据目录 UID、模式和持久化；
- 独立验证每个域名，不让单个坏记录掩盖其他域名结果。

### 17.2 页面可开但登录/API/WebSocket 失败

- `curl` 检查 `/console-health` 和 `/health/ready`；
- 检查 Console 的 `CONTROLLER_UPSTREAM` 是否为 `xs-nexus-rc-controller:8080`；
- 检查 Edge 上游是否为 `xs-nexus-rc-console:8080`；
- 检查浏览器 Cookie 是否为 Secure，访问是否确实使用 HTTPS；
- 检查 WebSocket `101`，不要只看首页 200；
- 检查 Controller、Console 日志中的明确错误码，但不得复制 Secret 到工单。

### 17.3 UDP Discovery/Relay 不通

- 确认 `XS_UDP_BIND_ADDRESS=0.0.0.0`；
- 确认环境端口、云防火墙、系统防火墙和公网 endpoint 一致；
- 检查 Relay catalog endpoint、Relay ID、公钥和到期时间；
- 检查 UDP 端口是否被其他程序占用；
- 使用真实外部 Agent 验证，不能用 TCP 检查替代。

### 17.4 `preflight` 拒绝 1Panel 网络

当前脚本要求：

```text
name=1panel-network
driver=bridge
subnet=172.18.0.0/16
```

不要为了通过检查而删除或重建已有 1Panel 网络。若生产环境基线不同，应先审计所有现有容器和网络，再通过独立代码变更扩展批准基线，并完成完整回归。

### 17.5 数据库迁移或恢复失败

- 确认数据库 URL 文件权限和数据库可达性；
- 确认 schema 名仅含小写字母、数字和下划线；
- 保留迁移前备份和失败日志；
- 不手工删除其他 schema；
- 恢复失败时先确认自动安全回滚结果，再决定人工操作。

### 17.6 Agent 在线但无法互通

- `xs status`、`xs peers`、`xs netcheck`、`xs path`、`xs routes`；
- 检查地址池冲突、ACL 双端策略、候选端点和 Relay 状态；
- 检查 TUN、nftables 和项目路由 manifest；
- 检查本地 LAN、云 VPC 或其他 VPN 是否与虚拟池重叠；
- 不修改默认路由来“临时跑通”。

## 18. 最终生产放行清单

只有以下全部完成后，才能由项目所有者重新评估是否标记 Release Candidate 或生产可用：

- [ ] Git 固定提交、工作树干净、构建可追溯；
- [ ] 全部服务和 Edge 健康，非 root、只读根、零 capability；
- [ ] DNS 唯一正确，HTTPS、自动续期、HTTP 跳转和四域名分别通过；
- [ ] WebSocket 全部返回 101；
- [ ] Discovery/Relay UDP 从真实外部网络通过；
- [ ] 两台以上 Linux Agent 完成 Direct、Relay fallback/failover 和回切；
- [ ] 默认拒绝 ACL、子网路由审批、撤销和离线失效通过；
- [ ] PostgreSQL/Redis/MySQL 无公网暴露；
- [ ] 正式发布签名密钥、公钥分发和轮换仪式完成；
- [ ] 正式备份 identity、真实异地主机、深度验证和恢复演练完成；
- [ ] SBOM、provenance、Grype 和残余风险处置在有效期；
- [ ] 临时密码、Token、SSH 和数据库凭据完成轮换；
- [ ] Windows VM/WDK/Driver Verifier、正式签名和安装回滚门禁通过；
- [ ] 真实 NAS/arm64 节点验证通过；
- [ ] 第三方协议、密码学和驱动安全审计完成；
- [ ] `RELEASE_CHECKLIST.md` 和 `FINAL_REPORT.md` 重新审核并如实更新。

## 19. 关联文档

- `README.md`：项目入口和当前状态；
- `docs/ARCHITECTURE.md`：组件、信任边界和数据流；
- `docs/DOCKER_1PANEL_DEPLOYMENT.md`：Compose、1Panel、备份和恢复细节；
- `docs/LINUX_INSTALLATION.md`：Linux Agent 安装、升级、回滚和卸载；
- `docs/UPDATE_SYSTEM.md`：签名更新和灰度策略；
- `docs/IMAGE_SUPPLY_CHAIN.md`：镜像 SBOM、许可证闭包和漏洞证据；
- `docs/CONTROLLER_API.md`：认证、Console、Enrollment 和控制连接；
- `RECOVERY_RUNBOOK.md`：故障恢复；
- `FINAL_REPORT.md`：当前交付结论；
- `BLOCKERS.md`、`KNOWN_ISSUES.md`：不可绕过的外部门禁和开放风险。
