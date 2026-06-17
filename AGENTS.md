# AGENTS.md — Netwatch

## Project

Rust web monitoring service (uptime/latency/DNS/HTTP/TCP). Single binary, SQLite-backed, Axum HTTP server + background scheduler. Edition 2024.

## Commands

```bash
rtk cargo check          # typecheck
rtk cargo clippy         # lint
rtk cargo test           # unit tests (22 tests, no DB needed)
cargo run                # start server on http://127.0.0.1:4311
```

Verification order: `cargo check -> cargo clippy -> cargo test`

## Architecture

- **Entry**: `src/main.rs` — loads config from env, connects SQLite, runs migrations, starts scheduler, binds Axum
- **Config**: `src/config.rs` — all env vars prefixed `NETWATCH_*`, defaults work out-of-box
- **Domain**: `src/domain/` — `monitor`, `check`, `alert` models; no web/DB coupling
- **Probes**: `src/probes/` — `http`, `icmp`, `dns`, `tcp` + `observation.rs` (unified success-rule evaluator)
- **Scheduler**: `src/scheduler/` — three spawned tokio loops in `mod.rs`:
  - `worker.rs`: per-monitor probe dispatch (spawned per-monitor; checks interval before probing)
  - `flush.rs`: batch-write buffered results to SQLite, then call `evaluator`
  - `compact.rs`: aggregate raw checks into minute/hour/day buckets, delete old raw data
  - `evaluator.rs`: alert rule — consecutive `failed` == threshold → `Triggered`; first `success` after `Triggered` → `Recovered`
- **Notify**: `src/notify/` — `webhook.rs` posts JSON `{monitor, event}` to `NETWATCH_WEBHOOK_URL`; empty URL just logs the alert
- **Storage**: `src/storage/` — SQLite via sqlx; `db.rs` connects with WAL+FK; `migrations.rs` is a custom runner (not `sqlx::migrate!`)
- **Web**: `src/web/` — `router.rs` merges API + UI; `api/` has REST handlers; `ui.rs` is inline HTML (no frontend build)
- **State**: `src/state.rs` — `AppState` (Arc-wrapped) holds config, pool, `CheckResultBuffer` (Mutex<Vec>)

## Key Conventions

- Comments and domain names are in Chinese (中文)
- `serde(rename_all = "snake_case")` on all enums for REST API
- Each domain enum has `as_str()` for DB persistence and `From<&str>` for reading back
- Migrations are `include_str!` from `migrations/*.sql`, tracked in `_netwatch_migrations` table
- No `sqlx::migrate!` macro — migrations run via custom `storage::migrations::run()`
- New migration: add SQL file to `migrations/`, add `include_str!` + entry in `migrations.rs` array
- Migration runner splits each `.sql` by `;` and executes statements one-by-one (no transaction wrapping) — avoid `;` inside string literals
- `*.db`, `*.db-shm`, `*.db-wal` are gitignored runtime data
- Ping probe (`icmp.rs`) requires raw socket capability — may fail without privileges on some OSes

## Gotchas

- `CheckResultBuffer` is an in-memory Mutex<Vec>; flush interval (default 60s) controls write frequency
- Scheduler tick (default 5s) is the scan cycle, not the per-monitor probe interval
- `compact.rs` flushes buffer before aggregating — don't skip this if modifying compact logic
- `interval_seconds` minimum is 5 (validated in `domain::monitor::validate_monitor_input`); defaults are interval=60s, timeout=10s
- `timeout_seconds` must be > 0 and < `interval_seconds`
- Aggregation timezone prefers `NETWATCH_AGGREGATION_TIMEZONE`; when unset, it uses the computer's current local offset and finally falls back to `UTC`
