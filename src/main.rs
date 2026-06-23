//! Netwatch 的服务入口。
//!
//! 启动流程保持集中：读取配置、初始化 SQLite、启动后台调度器，
//! 最后挂载 Axum 路由并监听 HTTP 端口。

mod config;
mod domain;
mod error;
mod notify;
mod probes;
mod scheduler;
mod state;
mod storage;
#[cfg(test)]
mod test_support;
mod web;

use std::net::SocketAddr;

use config::Config;
use scheduler::Scheduler;
use state::AppState;
use storage::db;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "netwatch=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    tracing::info!(
        host = %config.host,
        port = config.port,
        database_url = %config.database_url,
        scheduler_tick_seconds = config.scheduler_tick.as_secs(),
        check_flush_interval_seconds = config.check_flush_interval.as_secs(),
        compact_interval_seconds = config.compact_interval.as_secs(),
        aggregation_timezone = %config.aggregation_timezone,
        webhook_enabled = config.webhook_url.is_some(),
        "configuration loaded"
    );

    tracing::info!(database_url = %config.database_url, "connecting database");
    let pool = db::connect(&config.database_url).await?;
    tracing::info!("database connected");
    db::migrate(&pool).await?;
    if std::env::args().any(|arg| arg == "--migrate-only") {
        tracing::info!("database migrated");
        return Ok(());
    }

    // AppState 是 Web API、调度器、探测器共享的运行时上下文。
    let state = AppState::new(config.clone(), pool);
    Scheduler::start(state.clone());

    let app = web::router::build(state);
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("netwatch listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
