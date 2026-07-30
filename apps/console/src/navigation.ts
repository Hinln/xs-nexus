export type PageId =
  | "dashboard"
  | "nodes"
  | "topology"
  | "networks"
  | "address-pools"
  | "tokens"
  | "groups"
  | "acl"
  | "routes"
  | "relays"
  | "users"
  | "updates"
  | "audit"
  | "alerts"
  | "settings"
  | "backup"
  | "not-found";

export interface NavigationItem {
  id: PageId;
  label: string;
  group: "概览" | "网络" | "安全" | "系统";
}

export const navigationItems: NavigationItem[] = [
  { id: "dashboard", label: "首页", group: "概览" },
  { id: "nodes", label: "节点", group: "概览" },
  { id: "topology", label: "拓扑", group: "概览" },
  { id: "networks", label: "网络", group: "网络" },
  { id: "address-pools", label: "地址池", group: "网络" },
  { id: "tokens", label: "注册令牌", group: "网络" },
  { id: "groups", label: "分组与标签", group: "网络" },
  { id: "routes", label: "路由审批", group: "网络" },
  { id: "relays", label: "Relay", group: "网络" },
  { id: "acl", label: "ACL", group: "安全" },
  { id: "users", label: "用户权限", group: "安全" },
  { id: "audit", label: "审计日志", group: "安全" },
  { id: "alerts", label: "安全告警", group: "安全" },
  { id: "updates", label: "更新管理", group: "系统" },
  { id: "settings", label: "系统设置", group: "系统" },
  { id: "backup", label: "备份恢复", group: "系统" },
];

export function pageFromHash(hash: string): PageId {
  const requested = hash.replace(/^#\/?/, "");
  if (requested.length === 0) return "dashboard";
  return navigationItems.some((item) => item.id === requested)
    ? (requested as PageId)
    : "not-found";
}

export function pageTitle(page: PageId): string {
  if (page === "not-found") return "页面不存在";
  return navigationItems.find((item) => item.id === page)?.label ?? "首页";
}
