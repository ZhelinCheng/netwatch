//! 简单的内嵌 SQL 迁移执行器。
//!
//! 第一版只有初始化迁移，后续如果迁移增多可以替换为 `sqlx::migrate!`。

use sqlx::SqlitePool;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");

/// 依次执行初始化 SQL 中的语句。
pub async fn run(pool: &SqlitePool) -> anyhow::Result<()> {
    for statement in INIT_SQL.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            sqlx::query(statement).execute(pool).await?;
        }
    }
    Ok(())
}
