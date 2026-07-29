# ENVIRONMENT.md — 当前临时开发和测试环境

本文件不包含任何密码。真实秘密只允许通过当前会话或仓库外环境文件提供。

---

## 1. 临时开发服务器

- 公网地址：`34.92.139.129`
- SSH 端口：`122`
- 登录用户：`root`
- 用途：
  - Codex 开发；
  - Git；
  - Docker；
  - Controller；
  - Relay；
  - Console；
  - PostgreSQL/Redis 接入；
  - Linux TUN；
  - Network Namespace 实验；
  - Playwright QA。

该服务器是临时开发环境，开始高风险网络实验前应先创建系统盘快照。

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

## 3. 基础服务

### PostgreSQL

- 容器 DNS：`1Panel-postgresql-AVyu`
- 端口：`5432`
- 项目主关系型数据库
- 用户名和密码从 `/etc/xs-nexus/controller.env` 注入

### Redis

- 容器 DNS：`1Panel-redis-7ZP2`
- 端口：`6379`
- 用于缓存、在线状态、任务协调和短期数据
- 密码从仓库外注入

### MySQL

- 容器 DNS：`1Panel-mysql-Rq1B`
- 端口：`3306`
- 首版项目不使用
- 不得使用 MySQL root 作为业务应用账号

数据库端口不得发布到公网。

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

## 6. 计划域名

建议：

- `vpn.xiashikeji.cn`：Web Console 和 HTTPS Controller
- `relay.vpn.xiashikeji.cn`：UDP Relay 和地址发现

域名和防火墙属于人工门禁。在未授权前使用临时端口完成测试。

---

## 7. 虚拟地址池

首选：

```text
100.88.0.0/16
```

该网段必须在 Agent 启动时检测冲突。发现本地路由或接口已经使用时，不得强行覆盖。

---

## 8. 临时凭据处置

开发和测试完成后统一轮换：

- 服务器 root 密码；
- NAS 密码；
- PostgreSQL 密码；
- Redis 密码；
- MySQL root 密码；
- Web 管理员密码；
- Controller 在线密钥；
- Enrollment Token。

正式环境建议切换到 SSH Key，并禁止 root 密码远程登录。
