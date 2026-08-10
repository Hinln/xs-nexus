# 架构审查任务

首先全面扫描项目。

检查：

- 项目结构
- 后端架构
- 数据模型
- API设计
- 前端结构
- Agent设计
- 部署方式
- 测试覆盖

输出内部分析后直接执行修改。

重点：

## 架构目标

形成清晰分层：

Frontend
|
API Service
|
Business Layer
|
Database

Agent:

Device Agent
|
Control API
|
Management Console


---

发现架构问题直接重构。

不要为了保持旧结构而保留明显缺陷。
