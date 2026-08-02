# RELEASE_CHECKLIST.md — Release Candidate 检查

---

## 源代码

- [x] Git 状态干净（当前检查点）
- [x] 固定提交哈希（Git 检查点）
- [x] 无秘密（秘密扫描通过）
- [x] 无未知大文件（仓库清单审计）
- [x] 格式化通过
- [x] 静态分析通过
- [x] 依赖锁定（Cargo/npm 源码依赖）
- [x] THIRD_PARTY 完整（431 个源码依赖与许可证）
- [x] SBOM 生成（源码 CycloneDX/SPDX 与四个运行镜像 OS 包/逐包许可证闭包均完成）
- [x] 漏洞扫描完成（Grype 0.116.1；`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z`；处置门禁通过）

## 构建

- [x] Linux x86_64（M5.1 测试签名构建）
- [x] Linux arm64（M5.1 真实交叉构建与 ELF 验证，目标运行待门禁）
- [x] Controller（M5.2 开发镜像构建与健康验证）
- [x] Relay（M5.2 开发镜像构建与健康验证）
- [x] Console（M5.2 开发镜像构建与健康验证）
- [x] CLI（Linux x86_64/arm64 包）
- [ ] Windows Agent
- [ ] Windows Driver
- [x] 安装包（M5.1 测试签名范围）
- [x] 确定性或可追溯构建信息（image digest、Dockerfile hash、revision、provenance）

## 测试

- [x] 单元
- [x] 集成
- [x] 网络实验
- [x] 协议负向
- [x] NAT
- [x] Relay
- [x] Agent/Relay 身份签名遥测、重放/回滚拒绝与 Console 有界 24 小时聚合
- [x] ACL
- [x] 子网路由
- [x] Playwright
- [x] 视觉
- [x] 安装（Linux M5.1）
- [x] 升级（Linux M5.1）
- [x] 回滚（Linux M5.1）
- [x] 卸载（Linux M5.1）
- [x] 稳定性（24 小时长样本与修正重启门禁回归已审计）
- [x] 安全检查
- [x] Driver Verifier，或明确外部阻塞（BLK-001）

## 部署

- [x] 使用外部 `1panel-network`
- [ ] 无数据库公网端口
- [x] 容器非 root
- [x] Secret 仓库外
- [x] 健康检查
- [x] 日志轮转
- [x] 备份（age 流式认证加密、独立文件系统自动复制、公开/深度校验、保留和销毁墓碑）
- [x] 恢复演练（真实 PostgreSQL 测试 schema、错误 identity/篡改拒绝、异地取回和失败回滚）
- [ ] 正式离线备份 identity 与真实异地主机（产品边界已完成，外部门禁 `BLK-007`）
- [ ] 防火墙最小开放
- [x] 开发和 RC 隔离

## 更新

- [x] 清单签名（临时测试 Ed25519 密钥；正式离线签名未完成）
- [x] 哈希（外部归档与包内逐文件 SHA-256）
- [x] 版本防回滚（外部降级拒绝）
- [x] 分批（网络/通道/平台/架构策略、基点分桶、暂停和最低版本）
- [x] 回滚（只允许已安装且签名/哈希仍有效版本）
- [ ] 离线签名私钥未进入服务器（产品接口只接受公钥，但正式离线签名仪式和密钥分发仍是门禁）

## 文档

- [x] README
- [x] 架构
- [x] XSP/1
- [x] API
- [x] 安全假设
- [x] 威胁模型
- [x] 安装（Linux）
- [x] 升级（Linux）
- [x] 卸载（Linux）
- [x] 恢复（Linux Agent 生命周期）
- [x] 1Panel（项目部署与恢复；既有公网数据库端口仍由 `BLK-005` 阻塞）
- [x] 测试报告（`QA_MATRIX.md` 与各阶段证据目录）
- [x] 性能报告（`docs/PERFORMANCE_REPORT.md`；含 24 小时稳定性审计）
- [x] FINAL_REPORT（如实标记部分完成和生产不适用）

## 实机门禁

- [x] Linux 核心通过（开发服务器与隔离 namespace；真实 NAS/arm64 仍待门禁）
- [ ] Windows 测试 VM 通过
- [ ] NAS 由用户手动接入
- [ ] 日常 Windows 仅在 VM 通过后接入
- [ ] DNS 由用户批准
- [ ] 正式驱动签名状态明确
- [ ] 所有临时密码待发布后轮换

## 结论

- [ ] 可标记 Release Candidate
- [x] 仍为部分完成
- [ ] 不适合生产

必须三选一，并在 `FINAL_REPORT.md` 给出证据。

## Runtime image license closure checkpoint (2026-07-31)

- [x] Exact installed OS package set has one closure record per package in implementation dry runs.
- [x] Referenced license materials are path/size/SHA-256 bound and independently rehashed.
- [x] Missing SPDX text, unsafe rootfs paths and package identity drift fail closed.
- [x] Clean-commit full image build, deterministic regeneration, vulnerability scan and disposition evidence completed at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z`.
