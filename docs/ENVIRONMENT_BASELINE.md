# M0.1 环境基线报告

采集时间：2026-07-29 09:40 UTC  
工作目录：`/srv/xs-nexus`  
原始证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z`

## 主机

- Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- Google Compute Engine，8 vCPU，7.7 GiB 内存；
- 根文件系统 193 GiB，采集时使用率 3%；
- systemd 失败服务为 0；
- 系统提示需要重启，但本阶段未重启，避免影响 1Panel 和 SSH。

## Docker 与 1Panel

- Docker 服务正常；
- 外部网络 `1panel-network` 的 ID、驱动和 IPAM 已记录；
- 网络为本地 bridge，子网 `172.18.0.0/16`，网关 `172.18.0.1`；
- 采集时网络连接 3 个既有 1Panel 容器；
- PostgreSQL、Redis、MySQL 容器状态已记录；
- 项目未删除、重建、修改或断开任何现有容器、网络、卷或服务；
- `deploy/docker/compose.yaml` 只以 `external: true` 引用 `1panel-network`；
- 基线 profile 只定义非 root、只读、无 Linux capability 的配置探针，本阶段没有启动该容器。

## 现有安全风险

外部 TCP 探测确认 PostgreSQL `5432` 和 Redis `6379` 可从公网到达。该状态在项目开始前已经存在，证据位于：

```text
/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt
```

该段记录的是旧开发服务器基线。项目未修改其 1Panel 端口映射或生产防火墙；迁移到新生产候选服务器后，项目 PostgreSQL 无 host binding 且外部数据库端口不可达，`KI-006` 和 `BLK-005` 已解除。新主机证据见 `ENVIRONMENT.md`，本历史基线不得覆盖当前事实。

## 网络能力

- `/dev/net/tun` 存在；
- root 进程具备所需网络 capability；
- `nftables`、`iproute2`、`tc`、`unshare` 和 `nsenter` 可用；
- 在临时 network namespace 中实际创建 TUN、分配地址并创建 nftables table 成功；
- namespace 退出后测试接口和规则自动消失，宿主机接口、路由和 nftables 未被测试修改；
- IPv4 forwarding 在采集前已经为 1，项目未修改该值。

## 工具链

安装并验证：

- Rust 1.93.1；
- Cargo 1.93.1；
- rustfmt 1.8.0；
- rust-clippy 1.93.1；
- Node.js 22.22.1；
- npm 9.2.0；
- GCC 15.2.0；
- Clang 21.1.8；
- CMake 4.2.3；
- GNU Make 4.4.1；
- ripgrep 15.1.0；
- ShellCheck 0.11.0。

安装只增加开发工具，没有执行系统升级、重启或网络变更。安装日志保存在基线证据目录。

## 凭据

- 真实配置保存在 `/etc/xs-nexus/controller.env`，权限为 `0600`；
- 项目 `.env` 是被 Git 忽略的符号链接；
- 基于外部环境文件的精确值扫描未发现真实凭据进入仓库；
- 通用扫描只识别到 `.env.example` 的占位符，未发现私钥或真实连接串。

## M0.1 验证

验证时间：2026-07-29 09:52–09:56 UTC  
环境：上述临时开发服务器  
证据目录：`/srv/xs-nexus/artifacts/qa/m0.1-20260729T095256Z`

运行命令：

```bash
make fmt-check
make lint
make build
make test-unit
make test-integration
make test-network
make security-check
docker compose --profile baseline -f deploy/docker/compose.yaml config
git check-ignore -v .env
```

结果：

- Rust 与前端格式检查通过；
- Rust Clippy 和 TypeScript 严格检查通过；
- 四个 Rust 组件和 React/Vite 控制台构建通过；
- Rust 单元测试 2 个通过，前端单元测试 1 个通过；
- 四个组件集成启动检查通过；
- 隔离 TUN、namespace、nftables 实测通过；
- 仓库秘密扫描通过；
- Compose 配置确认外部网络引用；
- `.env` 确认被 Git 忽略。

首次执行中发现并保留了三类失败日志：TypeScript CSS 类型入口缺失、测试 TUN 名称超过 Linux 长度限制、秘密扫描规则误报。均按根因修复并复跑通过，未删除或弱化测试。
