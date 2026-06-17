//! 内置静态 UI。
//!
//! 第一版直接返回一份 HTML，避免引入前端构建链；
//! 后续如果 UI 复杂起来，可以把这里替换为静态资源目录或 Vite 构建产物。

use axum::{
    Router,
    response::{Html, IntoResponse},
    routing::get,
};

use crate::state::AppState;

/// 注册 Web UI 页面路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/monitors", get(index))
}

async fn index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Netwatch</title>
  <style>
    :root { color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: #f6f7f9; color: #1b1d22; }
    header { height: 56px; display: flex; align-items: center; justify-content: space-between; padding: 0 24px; background: #ffffff; border-bottom: 1px solid #dde1e7; }
    main { max-width: 1180px; margin: 0 auto; padding: 24px; }
    h1 { font-size: 20px; margin: 0; }
    h2 { font-size: 15px; margin: 0 0 12px; }
    button, input, select { height: 36px; border: 1px solid #cfd6df; border-radius: 6px; background: #fff; color: #1b1d22; padding: 0 10px; font-size: 14px; }
    button { cursor: pointer; background: #1264a3; border-color: #1264a3; color: #fff; }
    button.secondary { background: #fff; color: #1b1d22; border-color: #cfd6df; }
    .grid { display: grid; gap: 16px; }
    .summary { grid-template-columns: repeat(4, minmax(0, 1fr)); }
    .panel { background: #fff; border: 1px solid #dde1e7; border-radius: 8px; padding: 16px; }
    .metric { font-size: 28px; font-weight: 700; }
    .muted { color: #687385; font-size: 13px; }
    .layout { display: grid; grid-template-columns: 360px minmax(0, 1fr); gap: 16px; margin-top: 16px; align-items: start; }
    form { display: grid; gap: 10px; }
    table { width: 100%; border-collapse: collapse; font-size: 14px; }
    th, td { padding: 10px 8px; border-bottom: 1px solid #edf0f3; text-align: left; vertical-align: top; }
    th { color: #687385; font-weight: 600; font-size: 12px; text-transform: uppercase; }
    .status { display: inline-flex; align-items: center; min-width: 58px; justify-content: center; height: 24px; border-radius: 999px; font-size: 12px; font-weight: 700; }
    .success { background: #d9f7e7; color: #17663a; }
    .failed { background: #ffe0df; color: #9b1c17; }
    .unknown { background: #eceff3; color: #586172; }
    @media (max-width: 860px) { .summary, .layout { grid-template-columns: 1fr; } header, main { padding-left: 16px; padding-right: 16px; } }
  </style>
</head>
<body>
  <header>
    <h1>Netwatch</h1>
    <button class="secondary" onclick="load()">刷新</button>
  </header>
  <main>
    <section class="grid summary">
      <div class="panel"><div class="muted">监控项</div><div class="metric" id="total">0</div></div>
      <div class="panel"><div class="muted">成功</div><div class="metric" id="success">0</div></div>
      <div class="panel"><div class="muted">失败</div><div class="metric" id="failed">0</div></div>
      <div class="panel"><div class="muted">最近告警</div><div class="metric" id="alerts">0</div></div>
    </section>
    <section class="layout">
      <div class="panel">
        <h2>新增监控</h2>
        <form id="create-form">
          <input name="name" placeholder="名称，例如 Homepage" required />
          <select name="kind">
            <option value="http">HTTP</option>
            <option value="tcp">TCP</option>
            <option value="dns">DNS</option>
            <option value="ping">Ping</option>
          </select>
          <input name="target" placeholder="目标，例如 https://example.com" required />
          <input name="interval_seconds" type="number" min="5" value="60" />
          <input name="timeout_seconds" type="number" min="1" value="10" />
          <button type="submit">创建</button>
        </form>
      </div>
      <div class="panel">
        <h2>监控列表</h2>
        <table>
          <thead><tr><th>状态</th><th>名称</th><th>类型</th><th>目标</th><th>延迟</th><th>操作</th></tr></thead>
          <tbody id="monitors"></tbody>
        </table>
      </div>
    </section>
  </main>
  <script>
    async function api(path, options) {
      const response = await fetch(path, options);
      if (!response.ok) throw new Error(await response.text());
      return response.json();
    }
    function statusClass(result) {
      if (!result) return "unknown";
      return result.status;
    }
    function formatLatency(latencyUs) {
      if (latencyUs === null || latencyUs === undefined) return "-";
      if (latencyUs < 1000) return `${latencyUs} us`;
      return `${(latencyUs / 1000).toFixed(2)} ms`;
    }
    async function load() {
      const data = await api("/api/dashboard");
      document.getElementById("total").textContent = data.total;
      document.getElementById("success").textContent = data.success;
      document.getElementById("failed").textContent = data.failed;
      document.getElementById("alerts").textContent = data.alerts.length;
      document.getElementById("monitors").innerHTML = data.monitors.map(monitor => {
        const latest = data.latest[monitor.id];
        const status = latest ? latest.status : "unknown";
        const latency = latest ? formatLatency(latest.latency_us) : "-";
        return `<tr>
          <td><span class="status ${statusClass(latest)}">${status}</span></td>
          <td>${monitor.name}</td>
          <td>${monitor.kind}</td>
          <td>${monitor.target}</td>
          <td>${latency}</td>
          <td><button class="secondary" onclick="toggle('${monitor.id}', ${monitor.enabled})">${monitor.enabled ? "暂停" : "恢复"}</button></td>
        </tr>`;
      }).join("");
    }
    async function toggle(id, enabled) {
      await api(`/api/monitors/${id}/${enabled ? "pause" : "resume"}`, { method: "POST" });
      await load();
    }
    document.getElementById("create-form").addEventListener("submit", async event => {
      event.preventDefault();
      const form = new FormData(event.target);
      await api("/api/monitors", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: form.get("name"),
          kind: form.get("kind"),
          target: form.get("target"),
          interval_seconds: Number(form.get("interval_seconds")),
          timeout_seconds: Number(form.get("timeout_seconds")),
          config: {}
        })
      });
      event.target.reset();
      await load();
    });
    load();
    setInterval(load, 10000);
  </script>
</body>
</html>"#;
