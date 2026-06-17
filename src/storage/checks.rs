//! 探测结果 repository。

use chrono::{DateTime, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{
    domain::check::{CheckResult, CheckStatus},
    error::AppError,
    storage::time::{from_timestamp_seconds, to_timestamp_seconds},
};

/// 批量写入探测结果。
pub async fn insert_many(pool: &SqlitePool, results: &[CheckResult]) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    for result in results {
        insert_tx(&mut tx, result).await?;
    }
    tx.commit().await?;

    Ok(())
}

/// 在事务内写入单条探测结果。
async fn insert_tx(tx: &mut Transaction<'_, Sqlite>, result: &CheckResult) -> Result<(), AppError> {
    if result.status == CheckStatus::Unknown {
        return Err(AppError::BadRequest(
            "unknown check results are virtual and must not be persisted".to_string(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO check_results (
            monitor_id, status, latency_us, checked_at
        )
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(&result.monitor_id)
    .bind(result.status.as_str())
    .bind(result.latency_us.map(|value| value as i64))
    .bind(to_timestamp_seconds(result.checked_at))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn list_for_monitor(
    pool: &SqlitePool,
    monitor_id: &str,
    limit: i64,
) -> Result<Vec<CheckResult>, AppError> {
    // 详情页按最近结果展示，因此这里统一按 checked_at 倒序返回。
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, checked_at
        FROM check_results
        WHERE monitor_id = ?
        ORDER BY checked_at DESC
        LIMIT ?
        "#,
    )
    .bind(monitor_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_check).collect()
}

/// 按时间范围列出原始探测结果。
pub async fn list_for_monitor_between(
    pool: &SqlitePool,
    monitor_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, checked_at
        FROM check_results
        WHERE monitor_id = ? AND checked_at >= ? AND checked_at <= ?
        ORDER BY checked_at ASC
        "#,
    )
    .bind(monitor_id)
    .bind(to_timestamp_seconds(from))
    .bind(to_timestamp_seconds(to))
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_check).collect()
}

/// 在事务内按时间范围列出原始探测结果。
pub async fn list_for_monitor_between_tx(
    tx: &mut Transaction<'_, Sqlite>,
    monitor_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, checked_at
        FROM check_results
        WHERE monitor_id = ? AND checked_at >= ? AND checked_at < ?
        ORDER BY checked_at ASC
        "#,
    )
    .bind(monitor_id)
    .bind(to_timestamp_seconds(from))
    .bind(to_timestamp_seconds(to))
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter().map(row_to_check).collect()
}

/// 获取每个监控项最新的一条探测结果，用于 Dashboard 当前状态。
pub async fn latest_by_monitor(pool: &SqlitePool) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT cr.id, cr.monitor_id, cr.status, cr.latency_us, cr.checked_at
        FROM check_results cr
        JOIN (
            SELECT monitor_id, MAX(checked_at) AS checked_at
            FROM check_results
            GROUP BY monitor_id
        ) latest
            ON latest.monitor_id = cr.monitor_id AND latest.checked_at = cr.checked_at
        ORDER BY cr.checked_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_check).collect()
}

/// 删除某个监控项的全部原始探测结果。
pub async fn delete_for_monitor(pool: &SqlitePool, monitor_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM check_results WHERE monitor_id = ?")
        .bind(monitor_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 在事务内删除指定时间范围内的原始探测结果。
pub async fn delete_for_monitor_between_tx(
    tx: &mut Transaction<'_, Sqlite>,
    monitor_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        DELETE FROM check_results
        WHERE monitor_id = ? AND checked_at >= ? AND checked_at < ?
        "#,
    )
    .bind(monitor_id)
    .bind(to_timestamp_seconds(from))
    .bind(to_timestamp_seconds(to))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// 将 SQLx row 转回领域模型。
fn row_to_check(row: sqlx::sqlite::SqliteRow) -> Result<CheckResult, AppError> {
    let status: String = row.try_get("status")?;
    let checked_at: i64 = row.try_get("checked_at")?;
    let latency_us: Option<i64> = row.try_get("latency_us")?;

    Ok(CheckResult {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        status: CheckStatus::from(status.as_str()),
        latency_us: latency_us.map(|value| value as u64),
        checked_at: from_timestamp_seconds(checked_at)?,
    })
}
