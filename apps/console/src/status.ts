export type BootstrapState = "controller-unconfigured";

export function bootstrapMessage(state: BootstrapState): string {
  if (state === "controller-unconfigured") {
    return "控制台尚未连接 Controller；当前页面不会伪造节点或链路状态。";
  }

  return state satisfies never;
}
