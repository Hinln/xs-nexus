import type { Availability } from "./types";

export function formatDateTime(value: string | null): string {
  if (value === null) return "尚无记录";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "时间格式异常";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(date);
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "数据异常";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function availabilityText<T>(
  availability: Availability<T>,
  format: (value: T) => string = String,
): string {
  if (availability.status === "available" && availability.value !== null) {
    return format(availability.value);
  }
  return "未采集";
}

export function roleLabel(role: string): string {
  if (role === "administrator") return "管理员";
  if (role === "operator") return "运维员";
  if (role === "auditor") return "审计员";
  return "未知角色";
}

export function outcomeLabel(outcome: string): string {
  if (outcome === "success") return "成功";
  if (outcome === "rejected") return "已拒绝";
  if (outcome === "failure") return "失败";
  return "未知";
}

export function nodeStateLabel(state: string): string {
  if (state === "online") return "在线";
  if (state === "offline") return "离线";
  if (state === "revoked") return "已吊销";
  return "未知";
}

export function tokenStateLabel(state: string): string {
  if (state === "active") return "有效";
  if (state === "expired") return "已过期";
  if (state === "exhausted") return "次数耗尽";
  if (state === "revoked") return "已吊销";
  return "未知";
}
