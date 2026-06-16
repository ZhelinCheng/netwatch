//! 探测结果 repository。

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::{
    domain::check::{CheckResult, CheckStatus, LatencyMetrics},
    error::AppError,
};

/// 写入一次探测结果。
pub async fn insert(pool: &SqlitePool, result: &CheckResult) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO check_results (
            monitor_id, status, latency_ms, error, metadata_json, checked_at
        )
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&result.monitor_id)
    .bind(result.status.as_str())
    .bind(result.latency_ms.map(|value| value as i64))
    .bind(&result.error)
    .bind(serde_json::to_string(&result.metadata)?)
    .bind(result.checked_at.to_rfc3339())
    .execute(pool)
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
        SELECT id, monitor_id, status, latency_ms, error, metadata_json, checked_at
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

/// 获取每个监控项最新的一条探测结果，用于 Dashboard 当前状态。
pub async fn latest_by_monitor(pool: &SqlitePool) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT cr.id, cr.monitor_id, cr.status, cr.latency_ms, cr.error, cr.metadata_json, cr.checked_at
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

pub async fn metrics_for_monitor(
    pool: &SqlitePool,
    monitor_id: &str,
    limit: i64,
) -> Result<LatencyMetrics, AppError> {
    let results = list_for_monitor(pool, monitor_id, limit).await?;
    Ok(LatencyMetrics::from_results(&results))
}

fn row_to_check(row: sqlx::sqlite::SqliteRow) -> Result<CheckResult, AppError> {
    let status: String = row.try_get("status")?;
    let metadata_json: String = row.try_get("metadata_json")?;
    let checked_at: String = row.try_get("checked_at")?;
    let latency_ms: Option<i64> = row.try_get("latency_ms")?;

    Ok(CheckResult {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        status: CheckStatus::from(status.as_str()),
        latency_ms: latency_ms.map(|value| value as u64),
        error: row.try_get("error")?,
        metadata: serde_json::from_str(&metadata_json)?,
        checked_at: DateTime::parse_from_rfc3339(&checked_at)
            .map_err(|err| AppError::BadRequest(err.to_string()))?
            .with_timezone(&Utc),
    })
}
