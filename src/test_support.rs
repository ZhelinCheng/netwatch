use std::{path::PathBuf, time::Duration};

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;

use crate::{
    config::Config,
    domain::monitor::{Monitor, MonitorConfig, MonitorKind},
    state::AppState,
    storage::db,
};

/// 构造测试专用配置，避免单测读取开发机环境变量。
pub fn config(database_url: String) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 4311,
        database_url,
        scheduler_tick: Duration::from_secs(5),
        failure_threshold: 3,
        aggregation_timezone: "UTC".into(),
        compact_interval: Duration::from_secs(600),
        check_flush_interval: Duration::from_secs(60),
        webhook_url: None,
    }
}

/// 创建带迁移的临时 SQLite 状态。
pub async fn state(name: &str) -> AppState {
    let database_url = temp_database_url(name);
    let pool = db::connect(&database_url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    AppState::new(config(database_url), pool)
}

/// 创建带迁移的临时 SQLite 连接池。
pub async fn pool(name: &str) -> SqlitePool {
    let database_url = temp_database_url(name);
    let pool = db::connect(&database_url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    pool
}

/// 样例监控项，调用方可在插入后使用返回的自增 ID。
pub fn monitor(kind: MonitorKind) -> Monitor {
    let created_at = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
    Monitor {
        id: 0,
        name: "m1".into(),
        kind,
        target: "https://example.com".into(),
        config: MonitorConfig::default(),
        interval_seconds: 5,
        timeout_seconds: 1,
        enabled: true,
        created_at,
        updated_at: created_at,
    }
}

fn temp_database_url(name: &str) -> String {
    let mut path = PathBuf::from(std::env::temp_dir());
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    path.push(format!("netwatch-{name}-{suffix}.db"));
    format!("sqlite://{}", path.display())
}
