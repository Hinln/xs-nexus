# RECOVERY_RUNBOOK.md — 主机、网络和部署恢复

任何会影响 SSH、路由、接口、防火墙、Docker 网络或数据库的操作前，必须先记录恢复步骤。

---

## 1. 基线采集

建议保存到：

```text
/srv/xs-nexus-qa/baseline/<timestamp>/
```

采集：

```bash
ip -br addr
ip route show table all
ip rule show
ss -lntup
nft list ruleset
docker ps -a
docker network ls
docker network inspect 1panel-network
systemctl --failed
df -h
free -h
```

不得在输出中记录秘密环境变量。

---

## 2. 防止 SSH 失联

任何防火墙或路由操作前：

1. 确认当前 SSH 连接；
2. 打开第二个 SSH 会话；
3. 保存规则；
4. 创建定时自动回滚；
5. 应用最小变更；
6. 从第二个会话验证；
7. 验证成功后取消回滚。

不得使用会覆盖全量规则的单行命令。

---

## 3. TUN 和路由清理

项目提供严格可信状态清理命令：

```bash
sudo /usr/local/lib/xs-nexus/current/bin/xs-agent cleanup \
  --config /etc/xs-nexus/agent.json
```

`cleanup` 只加载现有配置、本地身份、签名节点状态和恢复清单，不创建新身份、不 enrollment、不接受任意接口名。可信项目接口仍处于活动状态、身份不匹配或清单无效时失败关闭。正常卸载应优先使用安装器，由安装器停止服务后调用该命令。

至少恢复：

- 项目创建的 TUN；
- 项目路由；
- 项目 rule；
- 项目 nftables chain；
- 项目 namespace；
- 临时 qdisc。

不得删除未知规则。

---

## 4. Docker 恢复

- Compose 只操作项目命名资源；
- 数据卷备份后再迁移；
- 不删除 `1panel-network`；
- 不执行全局 prune；
- 容器失败时回滚到上一个固定镜像摘要；
- 迁移失败时停止新版本并恢复数据库备份。

---

## 5. 数据库恢复

发布前：

- 逻辑备份；
- 记录 schema 版本；
- 验证备份可读取；
- 在临时数据库演练恢复。

迁移：

- 支持事务时使用事务；
- 破坏性迁移拆阶段；
- 先兼容读写，再删除旧字段；
- 失败保留日志和恢复命令。

---

## 6. Agent 升级恢复

升级前保存只读证据：

- `readlink /usr/local/lib/xs-nexus/current` 与 `previous`；
- 版本；
- 配置和节点身份的路径、所有者、权限与哈希，不记录内容；
- `ip -details link`、`ip route show table all`、`ip rule show` 与项目 nftables 表；
- `systemctl status xs-agent` 和 `journalctl -u xs-agent` 的脱敏输出。

安装器激活失败会自动恢复旧版本链接、systemd 单元和先前活动状态，并删除失败版本。需要显式回滚时：

```bash
sudo ./installers/linux/xs-nexus-installer.sh status
sudo ./installers/linux/xs-nexus-installer.sh rollback
sudo ./installers/linux/xs-nexus-installer.sh rollback --version <已安装版本>
```

只允许回滚到仍通过原签名、外部清单、归档和逐文件哈希验证的已安装版本。失败后：

1. 保存安装器与 systemd 脱敏日志；
2. 确认 `current` 指向预期旧版本；
3. 确认配置、身份和签名状态未被替换；
4. 启动或确认安装器已恢复服务；
5. 验证普通网络；
6. 验证虚拟网络；
7. 记录 Bug。

默认卸载保留配置、身份、签名状态、诊断和固定公钥；只有明确销毁节点时才使用 `uninstall --purge`。cleanup 失败时不得手工删除未知路由、接口、规则、Docker 网络或 1Panel 资源。

---

## 7. Windows 驱动恢复

仅在测试 VM 中：

- 安装前快照；
- 准备安全模式；
- 记录设备实例；
- 安装失败自动卸载；
- 蓝屏后回滚快照；
- 不在日常电脑执行首轮测试。

---

## 8. 紧急停止

出现以下情况立即停止相关服务并恢复：

- SSH 不稳定；
- 默认路由改变；
- 1Panel 网络异常；
- 数据库端口公网暴露；
- 明文秘密写入日志；
- TUN 抢占无关流量；
- Relay 出现匿名流量；
- Windows 蓝屏；
- 数据库迁移损坏。

恢复后先创建 P0 Bug，不得直接继续发布。
