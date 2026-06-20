//! 数据库连接和迁移入口。

use std::str::FromStr;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

/// 建立 SQLite 连接池。
///
/// 默认开启 WAL 和外键约束，适合一个本地 Web 服务加后台任务的并发写入模型。
pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    tracing::debug!(database_url = %database_url, "opening sqlite pool");
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// 执行内嵌迁移。
pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    tracing::info!("running database migrations");
    super::migrations::run(pool).await
}
