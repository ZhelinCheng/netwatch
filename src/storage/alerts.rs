//! 告警事件 repository。

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::{
    domain::alert::{AlertEvent, AlertKind},
    error::AppError,
};

/// 写入告警事件。
pub async fn insert(pool: &SqlitePool, event: &AlertEvent) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO alert_events (monitor_id, kind, message, delivered, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&event.monitor_id)
    .bind(event.kind.as_str())
    .bind(&event.message)
    .bind(event.delivered)
    .bind(event.created_at.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(())
}

/// 列出最近告警事件。
pub async fn list(pool: &SqlitePool, limit: i64) -> Result<Vec<AlertEvent>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, kind, message, delivered, created_at
        FROM alert_events
        ORDER BY created_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_alert).collect()
}

pub async fn latest_for_monitor(
    pool: &SqlitePool,
    monitor_id: &str,
) -> Result<Option<AlertEvent>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, monitor_id, kind, message, delivered, created_at
        FROM alert_events
        WHERE monitor_id = ?
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(monitor_id)
    .fetch_optional(pool)
    .await?;

    row.map(row_to_alert).transpose()
}

fn row_to_alert(row: sqlx::sqlite::SqliteRow) -> Result<AlertEvent, AppError> {
    let kind: String = row.try_get("kind")?;
    let created_at: String = row.try_get("created_at")?;

    Ok(AlertEvent {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        kind: AlertKind::from(kind.as_str()),
        message: row.try_get("message")?,
        delivered: row.try_get("delivered")?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|err| AppError::BadRequest(err.to_string()))?
            .with_timezone(&Utc),
    })
}
