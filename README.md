# netwatch
网络可用性 / 延迟 / DNS / HTTP / 出口质量监控工具

Netwatch 是一个面向个人自部署的轻量 Web 监控服务，目标体验参考 Uptime Kuma 和 SmokePing。

## 当前能力

- HTTP/HTTPS 状态码与关键字探测
- DNS 解析耗时与结果校验
- TCP 端口连通性探测
- Ping 探测
- SQLite 持久化
- 定时调度、连续失败告警、恢复告警
- Webhook 通知
- 内置 Web UI 与 REST API

## 运行

```bash
cargo run
```

默认服务地址：

```text
http://127.0.0.1:4311
```

## 环境变量

- `NETWATCH_HOST`：监听地址，默认 `127.0.0.1`
- `NETWATCH_PORT`：监听端口，默认 `4311`
- `NETWATCH_DATABASE_URL`：SQLite 地址，默认 `sqlite://netwatch.db`
- `NETWATCH_SCHEDULER_TICK_SECONDS`：调度扫描间隔，默认 `5`
- `NETWATCH_FAILURE_THRESHOLD`：连续失败多少次触发告警，默认 `3`
- `NETWATCH_AGGREGATION_TIMEZONE`：聚合日历时区，未设置时使用电脑当前时区
- `NETWATCH_WEBHOOK_URL`：Webhook 通知地址，可选

## API

- `GET /api/health`
- `GET /api/dashboard`
- `GET /api/monitors`
- `POST /api/monitors`
- `GET /api/monitors/:id`
- `PATCH /api/monitors/:id`
- `DELETE /api/monitors/:id`
- `POST /api/monitors/:id/pause`
- `POST /api/monitors/:id/resume`
- `GET /api/monitors/:id/checks`
- `GET /api/alerts`
- `GET /api/status-pages/:slug`
