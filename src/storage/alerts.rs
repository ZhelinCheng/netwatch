//! 告警事件 repository。

use sqlx::{Row, SqlitePool};

use crate::{
    domain::alert::{AlertEvent, AlertKind},
    error::AppError,
    storage::time::{from_timestamp_seconds, to_timestamp_seconds},
};

/// 写入告警事件。
pub async fn insert(pool: &SqlitePool, event: &AlertEvent) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO alert_events (monitor_id, kind, message, delivered, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(event.monitor_id)
    .bind(event.kind.as_str())
    .bind(&event.message)
    .bind(event.delivered)
    .bind(to_timestamp_seconds(event.created_at))
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
    monitor_id: i64,
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

pub async fn delete_for_monitor(pool: &SqlitePool, monitor_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM alert_events WHERE monitor_id = ?")
        .bind(monitor_id)
        .execute(pool)
        .await?;

    Ok(())
}

fn row_to_alert(row: sqlx::sqlite::SqliteRow) -> Result<AlertEvent, AppError> {
    let kind: String = row.try_get("kind")?;
    let created_at: i64 = row.try_get("created_at")?;

    Ok(AlertEvent {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        kind: AlertKind::from(kind.as_str()),
        message: row.try_get("message")?,
        delivered: row.try_get("delivered")?,
        created_at: from_timestamp_seconds(created_at)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::{
        domain::{
            alert::AlertKind,
            monitor::{MonitorKind},
        },
        storage::monitors,
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn alert_events_can_be_inserted_listed_latest_and_deleted() {
        let pool = test_support::pool("alerts-crud").await;
        let monitor = monitors::insert(&pool, &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let first = AlertEvent {
            id: None,
            monitor_id: monitor.id,
            kind: AlertKind::Triggered,
            message: "down".into(),
            delivered: false,
            created_at: Utc::now(),
        };
        let second = AlertEvent {
            kind: AlertKind::Recovered,
            message: "up".into(),
            delivered: true,
            created_at: first.created_at + Duration::seconds(1),
            ..first.clone()
        };

        insert(&pool, &first).await.unwrap();
        insert(&pool, &second).await.unwrap();

        let listed = list(&pool, 10).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].kind, AlertKind::Recovered);
        assert!(listed[0].delivered);
        assert_eq!(
            latest_for_monitor(&pool, monitor.id).await.unwrap().unwrap().kind,
            AlertKind::Recovered
        );

        delete_for_monitor(&pool, monitor.id).await.unwrap();
        assert!(latest_for_monitor(&pool, monitor.id).await.unwrap().is_none());
    }
}
