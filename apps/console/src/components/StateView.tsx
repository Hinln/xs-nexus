import type { ReactNode } from "react";

export function LoadingState({ label = "正在读取 Controller 数据" }: { label?: string }) {
  return (
    <div className="state-panel" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <h2>正在加载</h2>
      <p>{label}</p>
    </div>
  );
}

export function EmptyState({ title, description }: { title: string; description: string }) {
  return (
    <div className="state-panel state-panel--quiet">
      <span className="state-symbol" aria-hidden="true">○</span>
      <h2>{title}</h2>
      <p>{description}</p>
    </div>
  );
}

export function ErrorState({
  title = "数据读取失败",
  message,
  onRetry,
}: {
  title?: string;
  message: string;
  onRetry?: () => void;
}) {
  return (
    <div className="state-panel state-panel--error" role="alert">
      <span className="state-symbol" aria-hidden="true">!</span>
      <h2>{title}</h2>
      <p>{message}</p>
      {onRetry ? (
        <button className="button button--secondary" type="button" onClick={onRetry}>
          重新加载
        </button>
      ) : null}
    </div>
  );
}

export function ForbiddenState() {
  return (
    <div className="state-panel state-panel--forbidden" role="alert">
      <span className="state-symbol" aria-hidden="true">×</span>
      <h2>没有访问权限</h2>
      <p>当前角色不能查看此数据。权限由 Controller 在服务端验证。</p>
    </div>
  );
}

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description: string;
  actions?: ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        <p className="eyebrow">XS NEXUS · 运维控制台</p>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {actions ? <div className="page-actions">{actions}</div> : null}
    </header>
  );
}

export function StatusPill({ state, label }: { state: string; label: string }) {
  return <span className={`status-pill status-pill--${state}`}>{label}</span>;
}

export function TagList({ values, empty = "无" }: { values: string[]; empty?: string }) {
  if (values.length === 0) return <span className="muted">{empty}</span>;
  return (
    <span className="tag-list">
      {values.map((value) => (
        <span className="tag" key={value} title={value}>{value}</span>
      ))}
    </span>
  );
}

export function Notice({ children, tone = "info" }: { children: ReactNode; tone?: string }) {
  return (
    <div className={`notice notice--${tone}`} role={tone === "danger" ? "alert" : "status"}>
      {children}
    </div>
  );
}
