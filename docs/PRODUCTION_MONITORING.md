# XS Nexus 生产健康守卫

`check-production-health.sh` 是只读主机检查器，覆盖根分区容量、指定 Docker 容器状态、HTTP 健康端点、加密备份新鲜度和 TLS 证书剩余有效期。它不会删除 Docker 资源、修改网络、读取备份明文或保存凭据。

## 安装

发布前先按实际 Compose 项目名和域名编辑独立配置，不能把真实域名之外的密钥或凭据写入该文件：

```bash
sudo install -d -m 0755 /usr/local/libexec/xs-nexus /etc/xs-nexus
sudo install -m 0755 scripts/check-production-health.sh \
  /usr/local/libexec/xs-nexus/check-production-health
sudo install -m 0644 deploy/systemd/xs-nexus-healthcheck.service \
  /etc/systemd/system/xs-nexus-healthcheck.service
sudo install -m 0644 deploy/systemd/xs-nexus-healthcheck.timer \
  /etc/systemd/system/xs-nexus-healthcheck.timer
sudo install -m 0600 deploy/systemd/monitor.env.example /etc/xs-nexus/monitor.env
sudoedit /etc/xs-nexus/monitor.env
sudo systemctl daemon-reload
sudo systemctl enable --now xs-nexus-healthcheck.timer
sudo systemctl start xs-nexus-healthcheck.service
sudo systemctl status xs-nexus-healthcheck.service --no-pager
```

默认磁盘阈值在 80% 记录 warning，在 90% 返回失败；默认要求最新 `.age` 备份不超过 26 小时，并要求 TLS 证书至少剩余 14 天。所有阈值都必须通过 `/etc/xs-nexus/monitor.env` 显式审核。

## 运维与通知

```bash
systemctl list-timers xs-nexus-healthcheck.timer
journalctl -u xs-nexus-healthcheck.service --since today
sudo /usr/local/libexec/xs-nexus/check-production-health
```

systemd 失败状态和 journal 只提供本机检测证据，不等于值班通知。正式发布仍必须由运维负责人接入独立监控平台、通知渠道、升级策略和 on-call；在这些外部条件完成前，生产告警门禁保持 `BLOCKED_EXTERNAL`。

修改配置后必须先手工运行一次服务并确认：

- 真实容器名全部存在且处于 `running healthy` 或无 Docker healthcheck 的 `running none`；
- HTTP 地址只使用预期的本机或正式入口；
- 备份目录位于实际持久化路径；
- TLS 目标是正式 SNI 名称；
- 故意降低临时测试阈值时服务能够失败，测试后立即恢复审核值。
