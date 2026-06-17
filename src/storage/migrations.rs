//! 简单的内嵌 SQL 迁移执行器。
//!
//! 第一版只有初始化迁移，后续如果迁移增多可以替换为 `sqlx::migrate!`。

use sqlx::SqlitePool;

const INIT_SQL: &str = include_str!("../../migrations/0001_init.sql");
const SIMPLIFY_CHECK_RESULTS_SQL: &str =
    include_str!("../../migrations/0002_simplify_check_results.sql");
const CHECK_AGGREGATES_SQL: &str = include_str!("../../migrations/0003_check_aggregates.sql");

/// 依次执行初始化 SQL 中的语句。
pub async fn run(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _netwatch_migrations (
            name TEXT PRIMARY KEY NOT NULL,
            applied_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    for (name, sql) in [
        ("0001_init", INIT_SQL),
        ("0002_simplify_check_results", SIMPLIFY_CHECK_RESULTS_SQL),
        ("0003_check_aggregates", CHECK_AGGREGATES_SQL),
    ] {
        let applied: Option<(String,)> =
            sqlx::query_as("SELECT name FROM _netwatch_migrations WHERE name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await?;
        if applied.is_some() {
            continue;
        }

        execute_statements(pool, sql).await?;
        sqlx::query("INSERT INTO _netwatch_migrations (name, applied_at) VALUES (?, ?)")
            .bind(name)
            .bind(chrono::Utc::now().timestamp())
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn execute_statements(pool: &SqlitePool, sql: &str) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    for statement in sql.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            sqlx::query(statement).execute(&mut *connection).await?;
        }
    }
    Ok(())
}
