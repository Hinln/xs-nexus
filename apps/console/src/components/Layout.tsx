import { useState, type ReactNode } from "react";

import { formatDateTime, roleLabel } from "../format";
import { navigationItems, pageTitle, type PageId } from "../navigation";
import type { ConsoleUser } from "../types";

export function Layout({
  user,
  page,
  collectedAt,
  onNavigate,
  onRefresh,
  onLogout,
  children,
}: {
  user: ConsoleUser;
  page: PageId;
  collectedAt: string | null;
  onNavigate: (page: PageId) => void;
  onRefresh: () => void;
  onLogout: () => void;
  children: ReactNode;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const groups = ["概览", "网络", "安全", "系统"] as const;

  const navigate = (nextPage: PageId) => {
    setMenuOpen(false);
    onNavigate(nextPage);
  };

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">跳到主要内容</a>
      <aside id="mobile-navigation" className={`sidebar ${menuOpen ? "sidebar--open" : ""}`} aria-label="主导航">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">拾</span>
          <div>
            <strong>拾枢</strong>
            <span>XS Nexus</span>
          </div>
        </div>
        <nav>
          {groups.map((group) => (
            <section className="nav-group" key={group} aria-labelledby={`nav-${group}`}>
              <h2 id={`nav-${group}`}>{group}</h2>
              {navigationItems
                .filter((item) => item.group === group)
                .map((item) => (
                  <button
                    className="nav-item"
                    type="button"
                    key={item.id}
                    aria-current={page === item.id ? "page" : undefined}
                    onClick={() => navigate(item.id)}
                  >
                    <span className="nav-dot" aria-hidden="true" />
                    {item.label}
                  </button>
                ))}
            </section>
          ))}
        </nav>
        <div className="sidebar-user">
          <span className="avatar" aria-hidden="true">{user.display_name.slice(0, 1)}</span>
          <div>
            <strong title={user.display_name}>{user.display_name}</strong>
            <span>{roleLabel(user.role)}</span>
          </div>
        </div>
      </aside>

      {menuOpen ? (
        <button
          type="button"
          className="menu-backdrop"
          aria-label="关闭导航"
          onClick={() => setMenuOpen(false)}
        />
      ) : null}

      <div className="content-shell">
        <header className="topbar">
          <button
            type="button"
            className="menu-button"
            aria-expanded={menuOpen}
            aria-controls="mobile-navigation"
            onClick={() => setMenuOpen((open) => !open)}
          >
            <span aria-hidden="true">☰</span>
            <span className="sr-only">打开导航</span>
          </button>
          <div className="breadcrumb" aria-label="当前位置">
            <span>控制台</span>
            <strong>{pageTitle(page)}</strong>
          </div>
          <div className="topbar-actions">
            <span className="collection-time">
              数据时间：{collectedAt ? formatDateTime(collectedAt) : "尚未加载"}
            </span>
            <button className="icon-button" type="button" onClick={onRefresh} aria-label="刷新数据">
              ↻
            </button>
            <button className="button button--ghost" type="button" onClick={onLogout}>
              退出
            </button>
          </div>
        </header>
        <main id="main-content" className="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>
    </div>
  );
}
