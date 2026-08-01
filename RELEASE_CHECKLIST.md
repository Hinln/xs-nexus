# RELEASE_CHECKLIST.md — Release Candidate 检查

---

## 源代码

- [ ] Git 状态干净
- [ ] 固定提交哈希
- [ ] 无秘密
- [ ] 无未知大文件
- [ ] 格式化通过
- [ ] 静态分析通过
- [x] 依赖锁定（Cargo/npm 源码依赖）
- [x] THIRD_PARTY 完整（431 个源码依赖与许可证）
- [x] SBOM 生成（源码 CycloneDX/SPDX 与四个运行镜像 OS 包/逐包许可证闭包均完成）
- [ ] 漏洞扫描完成

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
- [ ] 确定性或可追溯构建信息

## 测试

- [ ] 单元
- [ ] 集成
- [ ] 网络实验
- [ ] 协议负向
- [ ] NAT
- [ ] Relay
- [ ] ACL
- [ ] 子网路由
- [ ] Playwright
- [ ] 视觉
- [x] 安装（Linux M5.1）
- [x] 升级（Linux M5.1）
- [x] 回滚（Linux M5.1）
- [x] 卸载（Linux M5.1）
- [ ] 稳定性
- [ ] 安全检查
- [ ] Driver Verifier，或明确外部阻塞

## 部署

- [x] 使用外部 `1panel-network`
- [ ] 无数据库公网端口
- [x] 容器非 root
- [x] Secret 仓库外
- [x] 健康检查
- [x] 日志轮转
- [x] 备份（M5.2 本机私有目录；加密/异机复制待 `KI-015`）
- [x] 恢复演练（M5.2 测试 schema）
- [ ] 防火墙最小开放
- [x] 开发和 RC 隔离

## 更新

- [x] 清单签名（临时测试 Ed25519 密钥；正式离线签名未完成）
- [x] 哈希（外部归档与包内逐文件 SHA-256）
- [x] 版本防回滚（外部降级拒绝）
- [ ] 分批
- [x] 回滚（只允许已安装且签名/哈希仍有效版本）
- [ ] 离线签名私钥未进入服务器

## 文档

- [ ] README
- [ ] 架构
- [ ] XSP/1
- [ ] API
- [ ] 安全假设
- [ ] 威胁模型
- [x] 安装（Linux）
- [x] 升级（Linux）
- [x] 卸载（Linux）
- [x] 恢复（Linux Agent 生命周期）
- [x] 1Panel（项目部署与恢复；既有公网数据库端口仍由 `BLK-005` 阻塞）
- [ ] 测试报告
- [ ] 性能报告
- [ ] FINAL_REPORT

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
- [ ] 仍为部分完成
- [ ] 不适合生产

必须三选一，并在 `FINAL_REPORT.md` 给出证据。

## Runtime image license closure checkpoint (2026-07-31)

- [x] Exact installed OS package set has one closure record per package in implementation dry runs.
- [x] Referenced license materials are path/size/SHA-256 bound and independently rehashed.
- [x] Missing SPDX text, unsafe rootfs paths and package identity drift fail closed.
- [x] Clean-commit full image build, deterministic regeneration, vulnerability scan and disposition evidence completed at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z`.
