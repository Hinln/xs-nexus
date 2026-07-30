export type PageDataState = "loading" | "empty" | "error" | "forbidden";

export function pageStateMessage(state: PageDataState): string {
  if (state === "loading") return "正在读取 Controller 数据";
  if (state === "empty") return "当前没有可显示的真实数据";
  if (state === "error") return "Controller 请求未能完成";
  return "当前角色没有访问权限";
}
