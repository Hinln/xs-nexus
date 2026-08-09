# ENVIRONMENT.md — 当前生产候选与测试环境

本文件不包含任何密码。真实秘密只允许通过当前会话或仓库外环境文件提供。

---

## 1. 当前生产候选服务器

- 公网地址：`101.32.170.223`
- SSH 端口：`122`
- 非特权登录用户：`ubuntu`，日常操作使用专用 SSH key；
- 系统：Ubuntu 24.04.4 LTS、Linux 6.8.0-124、x86_64、2 vCPU、3.6 GiB RAM；
- Docker：29.6.2；1Panel 位于 `/opt/1panel`；
- `/dev/net/tun`、network namespace 与 nftables 可用；宿主没有 Rust、Node、CMake、Ninja 或 Clang，构建通过固定 Docker 工具链执行；
- 系统 Chrony 曾被停用并造成系统时钟落后 RTC/NTP 86400.300154 秒，已于 2026-08-08 恢复为 enabled/active 和 NTP synchronized；证据 `/srv/xs-nexus-qa/artifacts/time-sync-precorrect-20260807T063050Z`；
- 初始只读基线位于 `/srv/xs-nexus-qa/baseline/20260807T061137Z`。

旧开发服务器只保留历史证据，不再作为当前部署事实来源。任何网络实验仍必须优先使用独立 namespace，不得直接改生产主机默认路由或 1Panel 资源。

---

## 2. 1Panel Docker 网络

- 外部网络：`1panel-network`
- 子网：`172.18.0.0/16`

Compose 必须：

```yaml
networks:
  1panel-network:
    external: true
    name: 1panel-network
```

不得：

- 重建；
- 删除；
- 修改 IPAM；
- 改名；
- 接管现有容器；
- 使用全局 Docker 清理。

---

## 3. 当前基础服务

- 项目 PostgreSQL：`xs-nexus-rc-postgres:5432`，只加入 `1panel-network`，没有宿主端口映射；
- Controller、Relay、Console：`xs-nexus-rc-*`，HTTP 仅绑定 `127.0.0.1:28080/28081`；
- Discovery 与 Relay UDP：宿主 `42000/42001`；
- 当前主机没有 Redis 或 MySQL 项目依赖；
- Secret 位于 `/etc/xs-nexus/deployments/rc/`，环境文件为 `/etc/xs-nexus/deployments/rc.compose.env`，均不进入仓库；
- 从外部主机验证 TCP `3306`、`5432`、`6379`、`28080`、`28081` 不可达，项目 PostgreSQL 健康且无 host binding；证据 `/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z`。

数据库端口不得发布到公网；历史开发服务器的 PostgreSQL/Redis 暴露结论不外推到当前生产候选服务器。

---

## 4. 部署边界

Docker：

- `xs-controller`
- `xs-relay`
- `xs-console`
- worker
- QA 服务

宿主机：

- `xs-agent`
- TUN
- Netlink
- namespace
- `nftables`
- `tc`
- systemd

---

## 5. NAS

- 局域网地址：`192.168.0.100`
- SSH 端口：`122`
- 用户：`Emotion`
- 位于私有局域网，开发服务器不能直接访问

正确流程：

1. 服务器完成并签名 Linux/NAS Agent；
2. 用户在 NAS 本地执行安装；
3. NAS 主动连接 Controller；
4. 获得虚拟 IP；
5. 完成普通节点测试；
6. 最后审批 `192.168.0.0/24` 子网路由。

不得在服务器仓库保存 NAS 密码。

---

## 6. 控制台认证环境变量

Controller 首次启动且 `console_users` 为空时，可通过以下变量创建唯一的初始管理员：

```text
CONSOLE_BOOTSTRAP_USERNAME=admin
CONSOLE_BOOTSTRAP_PASSWORD=<至少 12 字符的临时强密码>
CONSOLE_COOKIE_SECURE=true
CONSOLE_SESSION_TTL_SECONDS=28800
```

- 密码只从进程环境读取并以 Argon2id PHC 哈希持久化，不得写入仓库或镜像；
- 已存在控制台用户后，Bootstrap 变量不会覆盖用户或密码；首次初始化完成后必须同时清空 `XS_CONSOLE_BOOTSTRAP_USERNAME` 和 `XS_CONSOLE_BOOTSTRAP_PASSWORD_FILE`、删除明文 Secret，并重新部署；
- HTTPS 部署必须保持 `CONSOLE_COOKIE_SECURE=true`；仅本机无 TLS 自动化测试可显式设为 `false`；
- `CONSOLE_SESSION_TTL_SECONDS` 允许 900–86400 秒，默认 28800 秒；
- 浏览器使用 HttpOnly、SameSite=Strict Cookie，写操作还必须提供当前会话的 CSRF 令牌；
- `ADMIN_API_TOKEN` 只用于服务端自动化，不得传递给浏览器或保存到 Web Storage。

真实 Bootstrap 密码属于临时凭据，完成首个管理员登录和用户创建后必须从部署环境与 Secret 目录移除。Controller `serve` 使用 `xs_nexus_app`，只校验已应用迁移；一次性 `migrate` 使用独立 `xs_nexus_migrator` 凭据并 `SET ROLE xs_nexus_owner`。Bootstrap/superuser URL 不得挂载到长运行 Controller。

---

## 7. 计划域名与当前状态

当前发布域名：

- `vpn.qinwen.co`：Linux/Windows bootstrap、Controller HTTPS 和下载；当前 A 记录直接指向生产候选服务器，公网健康、引导脚本、Windows manifest/ZIP 精确哈希和未知文件 404 已验证；

任务书计划域名：

- `vpn.xiashikeji.cn`：Web Console 和 HTTPS Controller
- `relay.vpn.xiashikeji.cn`：UDP Relay 和地址发现

`vpn.xiashikeji.cn` 当前解析到 CDN，但应用路径返回 `525 SSL Handshake Failed with Origin Server`；修正生产服务器时钟后仍可复现。它是未来正式域名门禁，不影响当前固定 `vpn.qinwen.co` 下载入口。Codex 未修改 1Panel OpenResty 或 CDN。

---

## 8. 虚拟地址池

首选：

```text
100.88.0.0/16
```

该网段必须在 Agent 启动时检测冲突。发现本地路由或接口已经使用时，不得强行覆盖。

---

## 9. 临时凭据处置

开发和测试完成后统一轮换：

- 服务器 root 密码；
- NAS 密码；
- PostgreSQL 密码；
- Redis 密码；
- MySQL root 密码；
- Web 管理员密码；
- Controller 在线密钥；
- Enrollment Token。

当前服务器已安装项目专用 SSH key；临时密码和全部应用凭据仍须在发布前轮换。是否禁用密码登录属于服务器管理门禁，变更前必须确认备用管理通道。

---

## 10. Linux 发布构建工具链

M5.1 在开发服务器安装并验证以下发行版构建包：

- `rust-src`：与服务器 Rust 工具链匹配的标准库源码；
- `gcc-aarch64-linux-gnu`：aarch64 GNU 交叉编译器和 linker；
- `libc6-dev-arm64-cross`：aarch64 glibc 开发文件；
- OpenSSL CLI、GNU tar/gzip/coreutils、`readelf`：清单签名、确定性归档、哈希和 ELF 架构验证。

这些是构建主机工具，不进入 Agent 运行时产物。实际 x86_64 与 aarch64 构建证据位于 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。正式签名私钥不得安装或持久化在该开发服务器。

M6.1 进一步确认当前 Rust 1.93.1 包含匹配 `rust-src`，Cargo 可在 `RUSTC_BOOTSTRAP=1` 下使用 `-Z build-std`。`crates/windows-transport` 不依赖 `std` 或 SDK C 头，已用 `core,alloc,panic_abort` 为 `x86_64-pc-windows-msvc` 实际 check 和 Clippy。完整 Agent 的 Windows check 会在 TLS 依赖 `ring` 编译时因缺少 Windows SDK `assert.h`、`lib.exe` 等失败；服务器仍没有 Windows SDK、WDK、MSBuild、UMDF/NetAdapterCx targets 或 VM。证据：`/srv/xs-nexus/artifacts/qa/m6.1-win32-transport-20260731T030644Z`。

2026-07-31 为验证使用 Tokio `std` 的最小 Windows 本地 IPC crate，在 root 用户目录安装了官方最小 Rust 1.93.1 rustup 工具链、`x86_64-pc-windows-msvc` 标准库、rust-src、Clippy 和 rustfmt；未替换 `/usr/bin` 的发行版工具链，也未安装或伪造 Windows SDK。`crates/windows-local-ipc` 已使用该目标实际 check 和交叉 Clippy，完整 Agent 仍在 `ring` 查找 `lib.exe` 时失败，和既有 `BLK-001` 一致。旧 transport 脚本现从 `cargo` 所在目录选择匹配的 `rustc` 与 `cargo-clippy`，防止系统 Cargo 和 rustup Clippy 混用 sysroot。

---

## 11. Docker / 1Panel 部署环境

M5.2 使用以下固定边界：

- Compose：`deploy/docker/compose.yaml`；
- 管理脚本：`deploy/docker/xs-nexus-stack.sh`；
- dev 模板：`deploy/docker/dev.compose.env.example`；
- RC 模板：`deploy/docker/rc.compose.env.example`；
- dev 项目/schema：`xs-nexus-dev` / `xs_nexus_dev`；
- RC 项目/schema：`xs-nexus-rc` / `xs_nexus_rc`；
- 外部网络：`1panel-network`，`bridge`，`172.18.0.0/16`；
- PostgreSQL 服务端：18.4；db-tools 使用 PostgreSQL 18 客户端。

环境文件、Secret、备份和状态必须位于仓库外。推荐根路径：

```text
/etc/xs-nexus/deployments/<dev|rc>.compose.env
/etc/xs-nexus/deployments/<dev|rc>/controller
/etc/xs-nexus/deployments/<dev|rc>/relay
/var/backups/xs-nexus/<dev|rc>
/var/lib/xs-nexus-deploy/<dev|rc>
```

默认 Controller/Console/Discovery/Relay 端口只绑定 `127.0.0.1`。正式 DNS、TLS、反向代理和防火墙仍属于人工门禁。完整准备、部署、备份和恢复步骤见 `docs/DOCKER_1PANEL_DEPLOYMENT.md`。

当前生产候选实际为 HTTP 回环、Discovery/Relay UDP 公网监听；数据库无宿主映射。主机 `ufw` inactive，nftables INPUT 默认接受且 1Panel 管理端口仍监听，因此“防火墙最小开放”不得标记完成。

## 12. 生产候选隔离 QA 与最终部署（2026-08-08）

- 产品仓库：`/srv/xs-nexus`；隔离 QA 工作树：`/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo`。
- 生产宿主继续不安装 Rust 或 Node 编译器。仓库外镜像 `xs-nexus/qa-rust:1.93.0` 与 `/srv/xs-nexus-qa/bin` 包装器提供 rustfmt、Clippy、rust-src、ShellCheck、AArch64 GCC 和 `libc6-dev-arm64-cross`；缓存位于 `/srv/xs-nexus-qa/cache`。
- QA Cargo 容器只负责构建和非特权命令；TUN、namespace、nftables、systemd 等真实宿主测试在进入 namespace 前编译目标二进制，再由宿主直接执行，避免 Docker 网络替代被测 namespace。
- 最终验证代码：`ff9551d322067c934d2ac7d55a62af8896660bb3`；证据 `/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z`。
- 当前生产环境文件仍位于仓库外 `/etc/xs-nexus/deployments/rc.compose.env`，权限 `0600 root:root`；活动状态目录 `/var/lib/xs-nexus-deploy/rc`。常驻项目容器为 Controller、Relay、Console、PostgreSQL，全部连接既有外部 `1panel-network`，没有创建项目网络。
- 最终部署证据 `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z`；外部端口证据 `/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z`。不得把 repo 外 QA 镜像、缓存、临时数据库环境文件或测试签名材料当作生产运行依赖或提交到 Git。

## 13. Gate 23 精确 QA 工具链与当前宿主状态（2026-08-09）

- 当前源码工具链为 Rust/Cargo/rustfmt/Clippy `1.94`；仓库外 QA 镜像为 `xs-nexus/qa-rust:1.94.0`，本地 image ID `sha256:9c5046e1f7fd8e27c3185aa09fdf4dcceb383c72fec68a524b6d9c32b58718f9`。Controller/Relay builder 固定 `rust:1.94.0-bookworm@sha256:365468470075493dc4583f47387001854321c5a8583ea9604b297e67f01c5a4f`。
- 旧任务自有 `xs-nexus/qa-rust:1.93.0` 和 `rust:1.93.0-bookworm` 镜像已在确认无容器引用后精确删除；另只删除三个可证明属于 XS Nexus 的 BuildKit cache record，未运行全局 Docker prune。
- 精确提交 `3bf861922c8b3cc62c3bfd1617835565fd86fc6b` 的 clean-checkout 验证根为 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`。验证后临时 worktree、target、QA 容器、QA 网络和 namespace 已清理。
- 当前生产应用仍为 `3d93656cc9ec3ea35d58e453118154b25bcc4e14`；四个项目容器健康，失败 systemd unit 为 0，根分区 `79%`，默认网关 `10.3.0.1`/`eth0`，`1panel-network` ID/子网不变。该 QA 工具链不是生产运行依赖。
