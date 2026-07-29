# 拾枢（XS Nexus）

拾枢是独立设计和实现的三层安全内网互通系统。当前仓库处于 `M0.1 仓库与环境基线`，尚未实现或声明生产网络能力。

## 当前组件

- `xs-controller`
- `xs-relay`
- `xs-agent`
- `xs-cli`
- `xs-console`

## 开发命令

```bash
make setup
make fmt-check
make lint
make build
make test
make test-network
make security-check
```

`make test-e2e`、`make test-visual` 和 `make release` 会在对应里程碑实现前明确失败，不会伪造通过结果。

## 凭据

真实配置位于仓库外的 `/etc/xs-nexus/controller.env`。仓库中的 `.env` 仅允许作为被 Git 忽略的本地符号链接。

## 1Panel

项目 Compose 配置只引用既有外部网络 `1panel-network`，不得创建、删除或修改该网络及未知 1Panel 资源。
