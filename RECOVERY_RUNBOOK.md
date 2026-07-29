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

项目必须提供幂等清理命令，例如：

```bash
xs-agent cleanup
```

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

升级前保存：

- 当前二进制；
- 版本；
- 配置；
- 节点身份；
- 路由状态。

失败：

1. 停止新版本；
2. 恢复旧二进制；
3. 恢复配置；
4. 启动；
5. 验证普通网络；
6. 验证虚拟网络；
7. 记录 Bug。

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
