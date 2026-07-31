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

M5.2 项目栈命令：

```bash
STACK=./deploy/docker/xs-nexus-stack.sh
ENV_FILE=/etc/xs-nexus/deployments/dev.compose.env

sudo "$STACK" --env-file "$ENV_FILE" status
sudo "$STACK" --env-file "$ENV_FILE" rollback
sudo "$STACK" --env-file "$ENV_FILE" down
```

部署激活前会把当前三个服务镜像 ID 写入私有状态目录并创建项目专用 rollback tag。`rollback` 只在三个旧镜像均可用时执行；若没有完整集合则失败关闭。`down` 只删除当前 Compose 项目容器，不删除 `1panel-network`、数据库、Secret、备份或 1Panel 资源。

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

已验证命令：

```bash
sudo "$STACK" --env-file "$ENV_FILE" backup incident-before-change
sudo "$STACK" --env-file "$ENV_FILE" verify-backup incident-before-change
sudo "$STACK" --env-file "$ENV_FILE" restore incident-before-change \
  --confirm-schema xs_nexus_dev
```

恢复前必须：

1. 保存数据库、Controller 和部署脚本脱敏日志；
2. 校验 `.dump` 和 `.manifest` 同时存在且均非符号链接；
3. 执行 `verify-backup`；
4. 将归档和清单复制到受控离线位置；
5. 精确核对环境文件中的 schema，不得恢复到其他环境。

恢复脚本停止 Controller、创建恢复前安全备份、删除并重建目标 schema，再受限恢复。恢复失败会删除失败状态并从安全备份回滚；若安全回滚也失败，立即按 P0 处理，不得启动写流量或手工修改其他 schema。备份静态加密和异机复制见 `KI-015`。

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
- 使用 `installers/windows/install-xsnet-test.ps1`，显式传入测试 signer thumbprint 和本机 WDK Microsoft-signed DevGen；DevGen 不随项目分发；
- 安装失败脚本只回滚本次发现的 `Root\XSNET` 设备和精确 `oem#.inf`；
- 正常卸载使用 `installers/windows/uninstall-xsnet-test.ps1`，残留设备或 driver-store package 会保留状态并返回失败；
- 完整 VM 流程使用 `scripts/windows/invoke-xsnet-test-vm-stage.ps1`，同一运行目录依次执行 Initialize、Install、EnableVerifier、重启、CollectVerifier、DisableVerifier、重启、Uninstall；脚本不自动重启，也不验证快照是否真实存在；
- Verifier 启用后若蓝屏、无法启动或普通网络异常，不继续执行安装脚本；保存 dump/事件后从已声明快照恢复。若能登录但测试中止，先执行 `verifier.exe /reset` 并重启，再使用精确状态卸载脚本；
- 不手工删除未知设备、驱动包、网卡、路由或注册表项；自动卸载返回残留时保存运行目录和 `%ProgramData%\XS Nexus` 状态，恢复快照并登记 P0/P1；
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
