import { bootstrapMessage } from "./status";

export function App() {
  return (
    <main className="shell">
      <section className="card" aria-labelledby="page-title">
        <p className="eyebrow">开发环境基线</p>
        <h1 id="page-title">拾枢 XS Nexus</h1>
        <p className="summary">
          {bootstrapMessage("controller-unconfigured")}
        </p>
        <dl className="facts">
          <div>
            <dt>当前里程碑</dt>
            <dd>M0.1 仓库与环境基线</dd>
          </div>
          <div>
            <dt>运行模式</dt>
            <dd>Development</dd>
          </div>
        </dl>
      </section>
    </main>
  );
}
