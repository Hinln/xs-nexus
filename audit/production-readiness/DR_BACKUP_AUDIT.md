# Disaster Recovery And Backup Audit

## Verified

- age X25519 加密备份、公开 index 和 replica receipt 存在。
- `pre-migration-rc-20260808T104233-1345622` 从精确 `ff9551d` worktree 执行公开完整性校验通过。
- M5.2 隔离环境重新通过 backup、tamper rejection、wrong identity rejection、fetch、restore 和 retention 测试。

## Failed Production Requirements

- `/var/backups/xs-nexus/rc` 与 `/mnt/xs-nexus-offsite/rc` 都在同一主机；后者是 `/dev/loop0`，不构成异地故障域。
- 没有 backup timer/cron，现有文件主要是部署前手工备份。
- 本轮未使用离线 identity 深度验证当前生产备份，未在全新服务器恢复。
- 没有现存节点、撤销、ACL、路由和网络状态在恢复后保持正确的证据。
- RPO、RTO、restore time、失败步骤和人工步骤没有正式演练记录。

## Result

`BLOCKED_EXTERNAL`，正式生产 `NO_GO`。同机加密文件不是异地灾备 PASS。
