import { useEffect, useState, type FormEvent, type ReactNode } from "react";

import {
  ConsoleApiError,
  loadConsoleSnapshot,
  loadUsers,
  login,
  logout,
  restoreSession,
} from "./api";
import { Layout } from "./components/Layout";
import {
  ErrorState,
  ForbiddenState,
  LoadingState,
  Notice,
  PageHeader,
} from "./components/StateView";
import { pageFromHash, pageTitle, type PageId } from "./navigation";
import { DashboardPage, NodesPage, TopologyPage } from "./pages/OverviewPages";
import {
  AddressPoolsPage,
  GroupsPage,
  NetworksPage,
  RelaysPage,
  RoutesPage,
  TokensPage,
} from "./pages/NetworkPages";
import { AclPage, AlertsPage, AuditPage, UsersPage } from "./pages/SecurityPages";
import { BackupPage, SettingsPage, UpdatesPage } from "./pages/SystemPages";
import type { ConsoleSnapshot, ConsoleUser, SessionPayload } from "./types";

interface AuthenticatedState {
  user: ConsoleUser;
  csrfToken: string;
  expiresAt: string;
}

interface DataState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  forbidden: boolean;
}

const emptySnapshotState: DataState<ConsoleSnapshot> = {
  data: null,
  loading: true,
  error: null,
  forbidden: false,
};

export function App() {
  const [auth, setAuth] = useState<AuthenticatedState | null>(null);
  const [restoring, setRestoring] = useState(true);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [snapshotState, setSnapshotState] = useState<DataState<ConsoleSnapshot>>(emptySnapshotState);
  const [usersState, setUsersState] = useState<DataState<ConsoleUser[]>>({
    data: null,
    loading: false,
    error: null,
    forbidden: false,
  });
  const [page, setPage] = useState<PageId>(() => pageFromHash(window.location.hash));

  useEffect(() => {
    const onHashChange = () => setPage(pageFromHash(window.location.hash));
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  useEffect(() => {
    document.title = `${pageTitle(page)} · 拾枢 XS Nexus`;
  }, [page]);

  useEffect(() => {
    let active = true;
    void restoreSession()
      .then((session) => {
        if (!active) return;
        setAuthenticatedSession(session);
        void loadAllData();
      })
      .catch((cause: unknown) => {
        if (!active) return;
        if (!(cause instanceof ConsoleApiError) || cause.status !== 401) {
          setRestoreError(cause instanceof Error ? cause.message : "会话恢复失败");
        }
      })
      .finally(() => {
        if (active) setRestoring(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const setAuthenticatedSession = (session: SessionPayload) => {
    setAuth({
      user: session.user,
      csrfToken: session.csrf_token,
      expiresAt: session.expires_at,
    });
  };

  const loadSnapshot = async () => {
    setSnapshotState((current) => ({ ...current, loading: true, error: null, forbidden: false }));
    try {
      const snapshot = await loadConsoleSnapshot();
      setSnapshotState({ data: snapshot, loading: false, error: null, forbidden: false });
    } catch (cause) {
      handleDataFailure(cause, setSnapshotState);
      throw cause;
    }
  };

  const loadUserList = async () => {
    setUsersState((current) => ({ ...current, loading: true, error: null, forbidden: false }));
    try {
      const users = await loadUsers();
      setUsersState({ data: users, loading: false, error: null, forbidden: false });
    } catch (cause) {
      handleDataFailure(cause, setUsersState);
    }
  };

  const loadAllData = async () => {
    await Promise.allSettled([loadSnapshot(), loadUserList()]);
  };

  const handleLogin = async (username: string, password: string) => {
    const session = await login(username, password);
    setAuthenticatedSession(session);
    setRestoreError(null);
    await loadAllData();
  };

  const handleLogout = async () => {
    if (auth !== null) {
      try {
        await logout(auth.csrfToken);
      } catch {}
    }
    setAuth(null);
    setSnapshotState(emptySnapshotState);
    setUsersState({ data: null, loading: false, error: null, forbidden: false });
  };

  const navigate = (nextPage: PageId) => {
    window.location.hash = `/${nextPage}`;
    setPage(nextPage);
  };

  if (restoring) {
    return <CenteredShell><LoadingState label="正在验证安全会话" /></CenteredShell>;
  }
  if (auth === null) {
    return <LoginPage initialError={restoreError} onLogin={handleLogin} />;
  }

  return (
    <Layout
      user={auth.user}
      page={page}
      collectedAt={snapshotState.data?.collected_at ?? null}
      onNavigate={navigate}
      onRefresh={() => void loadAllData()}
      onLogout={() => void handleLogout()}
    >
      {snapshotState.data === null && snapshotState.loading ? (
        <LoadingState />
      ) : snapshotState.forbidden ? (
        <ForbiddenState />
      ) : snapshotState.error ? (
        <ErrorState message={snapshotState.error} onRetry={() => void loadSnapshot()} />
      ) : snapshotState.data ? (
        <>
          {snapshotState.loading ? <Notice>正在刷新，当前显示上一次成功读取的数据。</Notice> : null}
          <PageRouter
            page={page}
            snapshot={snapshotState.data}
            user={auth.user}
            csrfToken={auth.csrfToken}
            usersState={usersState}
            onSnapshotChanged={loadSnapshot}
            onUsersChanged={async () => {
              await Promise.all([loadSnapshot(), loadUserList()]);
            }}
          />
        </>
      ) : (
        <ErrorState message="控制台没有可显示的数据" onRetry={() => void loadSnapshot()} />
      )}
    </Layout>
  );
}

function PageRouter({
  page,
  snapshot,
  user,
  csrfToken,
  usersState,
  onSnapshotChanged,
  onUsersChanged,
}: {
  page: PageId;
  snapshot: ConsoleSnapshot;
  user: ConsoleUser;
  csrfToken: string;
  usersState: DataState<ConsoleUser[]>;
  onSnapshotChanged: () => Promise<void>;
  onUsersChanged: () => Promise<void>;
}) {
  if (page === "dashboard") return <DashboardPage snapshot={snapshot} />;
  if (page === "nodes") return <NodesPage snapshot={snapshot} user={user} csrfToken={csrfToken} onChanged={onSnapshotChanged} />;
  if (page === "topology") return <TopologyPage snapshot={snapshot} />;
  if (page === "networks") return <NetworksPage snapshot={snapshot} user={user} csrfToken={csrfToken} onChanged={onSnapshotChanged} />;
  if (page === "address-pools") return <AddressPoolsPage snapshot={snapshot} />;
  if (page === "tokens") return <TokensPage snapshot={snapshot} user={user} csrfToken={csrfToken} onChanged={onSnapshotChanged} />;
  if (page === "groups") return <GroupsPage snapshot={snapshot} />;
  if (page === "routes") return <RoutesPage snapshot={snapshot} user={user} csrfToken={csrfToken} onChanged={onSnapshotChanged} />;
  if (page === "relays") return <RelaysPage snapshot={snapshot} />;
  if (page === "acl") return <AclPage snapshot={snapshot} />;
  if (page === "users") return (
    <UsersPage
      currentUser={user}
      users={usersState.data ?? []}
      loading={usersState.loading}
      error={usersState.forbidden ? "当前角色没有读取用户列表的权限" : usersState.error}
      csrfToken={csrfToken}
      onChanged={onUsersChanged}
    />
  );
  if (page === "audit") return <AuditPage snapshot={snapshot} />;
  if (page === "alerts") return <AlertsPage snapshot={snapshot} />;
  if (page === "updates") return (
    <UpdatesPage
      snapshot={snapshot}
      user={user}
      csrfToken={csrfToken}
      onSnapshotChanged={onSnapshotChanged}
    />
  );
  if (page === "settings") return <SettingsPage snapshot={snapshot} />;
  if (page === "backup") return <BackupPage snapshot={snapshot} />;
  return (
    <>
      <PageHeader title="页面不存在" description="该控制台地址不存在或已被移除。" />
      <ErrorState
        title="无法找到页面"
        message="请使用左侧主导航返回已提供的管理页面；当前地址不会自动回退到首页。"
      />
    </>
  );
}

function LoginPage({
  initialError,
  onLogin,
}: {
  initialError: string | null;
  onLogin: (username: string, password: string) => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(initialError);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    setBusy(true);
    setError(null);
    try {
      await onLogin(String(data.get("username") ?? ""), String(data.get("password") ?? ""));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "登录失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login-shell">
      <section className="login-intro" aria-labelledby="login-product-title">
        <div className="brand brand--login">
          <span className="brand-mark" aria-hidden="true">拾</span>
          <div><strong>拾枢</strong><span>XS Nexus</span></div>
        </div>
        <div>
          <p className="eyebrow">自研三层内网互通系统</p>
          <h1 id="login-product-title">把网络运行事实，清晰地交给运维人员。</h1>
          <p>节点、策略、路由与审计均来自 Controller。控制台不会用演示数据伪造健康状态。</p>
        </div>
        <dl className="login-principles">
          <div><dt>默认拒绝</dt><dd>ACL 双端执行</dd></div>
          <div><dt>端到端加密</dt><dd>Relay 不读取业务明文</dd></div>
          <div><dt>可恢复</dt><dd>系统网络变更有生命周期记录</dd></div>
        </dl>
      </section>
      <main className="login-panel">
        <form className="login-card" onSubmit={(event) => void submit(event)}>
          <header><span className="eyebrow">安全登录</span><h2>进入管理控制台</h2><p>使用 Controller 中已启用的控制台用户。</p></header>
          <label>用户名<input name="username" required autoComplete="username" autoFocus maxLength={64} /></label>
          <label>密码<input name="password" type="password" required autoComplete="current-password" maxLength={128} /></label>
          {error ? <Notice tone="danger">{error}</Notice> : null}
          <button className="button button--primary button--wide" type="submit" disabled={busy}>{busy ? "正在验证" : "登录"}</button>
          <p className="login-security">会话使用 HttpOnly Cookie；管理写操作还需当前会话的 CSRF 令牌。</p>
        </form>
      </main>
    </div>
  );
}

function CenteredShell({ children }: { children: ReactNode }) {
  return <main className="centered-shell">{children}</main>;
}

function handleDataFailure<T>(
  cause: unknown,
  setter: (value: DataState<T> | ((current: DataState<T>) => DataState<T>)) => void,
) {
  const forbidden = cause instanceof ConsoleApiError && cause.status === 403;
  setter((current) => ({
    ...current,
    loading: false,
    error: cause instanceof Error ? cause.message : "数据读取失败",
    forbidden,
  }));
}
