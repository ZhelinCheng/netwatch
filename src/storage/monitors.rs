//! 监控项 repository。

use chrono::Utc;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{
    domain::monitor::{Monitor, MonitorKind, UpdateMonitor, validate_monitor_input},
    error::AppError,
    storage::{
        aggregates, alerts, checks,
        time::{from_timestamp_seconds, to_timestamp_seconds},
    },
};

/// 列出所有监控项，按创建时间倒序返回。
pub async fn list(pool: &SqlitePool) -> Result<Vec<Monitor>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, kind, target, config_json, interval_seconds, timeout_seconds,
               enabled, created_at, updated_at
        FROM monitors
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_monitor).collect()
}

/// 获取单个监控项。
pub async fn get(pool: &SqlitePool, id: i64) -> Result<Monitor, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, kind, target, config_json, interval_seconds, timeout_seconds,
               enabled, created_at, updated_at
        FROM monitors
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    row_to_monitor(row)
}

/// 在事务内获取单个监控项。
pub async fn get_tx(tx: &mut Transaction<'_, Sqlite>, id: i64) -> Result<Monitor, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, kind, target, config_json, interval_seconds, timeout_seconds,
               enabled, created_at, updated_at
        FROM monitors
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(AppError::NotFound)?;

    row_to_monitor(row)
}

/// 插入新的监控项。
pub async fn insert(pool: &SqlitePool, monitor: &Monitor) -> Result<Monitor, AppError> {
    let result = sqlx::query(
        r#"
        INSERT INTO monitors (
            name, kind, target, config_json, interval_seconds, timeout_seconds,
            enabled, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&monitor.name)
    .bind(monitor.kind.as_str())
    .bind(&monitor.target)
    .bind(serde_json::to_string(&monitor.config)?)
    .bind(monitor.interval_seconds as i64)
    .bind(monitor.timeout_seconds as i64)
    .bind(monitor.enabled)
    .bind(to_timestamp_seconds(monitor.created_at))
    .bind(to_timestamp_seconds(monitor.updated_at))
    .execute(pool)
    .await?;

    let mut monitor = monitor.clone();
    monitor.id = result.last_insert_rowid();
    Ok(monitor)
}

pub async fn update(pool: &SqlitePool, id: i64, input: UpdateMonitor) -> Result<Monitor, AppError> {
    // 先加载旧值再局部覆盖，确保 PATCH 语义简单且字段默认值不被误改。
    let mut monitor = get(pool, id).await?;
    let old_interval_seconds = monitor.interval_seconds;

    if let Some(name) = input.name {
        monitor.name = name;
    }
    if let Some(target) = input.target {
        monitor.target = target;
    }
    if let Some(config) = input.config {
        monitor.config = config;
    }
    if let Some(interval_seconds) = input.interval_seconds {
        monitor.interval_seconds = interval_seconds;
    }
    if let Some(timeout_seconds) = input.timeout_seconds {
        monitor.timeout_seconds = timeout_seconds;
    }
    if let Some(enabled) = input.enabled {
        monitor.enabled = enabled;
    }

    validate_monitor_input(
        &monitor.name,
        &monitor.target,
        monitor.kind.clone(),
        &monitor.config,
        monitor.interval_seconds,
        monitor.timeout_seconds,
    )?;
    monitor.updated_at = Utc::now();

    if old_interval_seconds != monitor.interval_seconds {
        checks::delete_for_monitor(pool, id).await?;
        aggregates::delete_for_monitor(pool, id).await?;
        alerts::delete_for_monitor(pool, id).await?;
    }

    sqlx::query(
        r#"
        UPDATE monitors
        SET name = ?, target = ?, config_json = ?, interval_seconds = ?,
            timeout_seconds = ?, enabled = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&monitor.name)
    .bind(&monitor.target)
    .bind(serde_json::to_string(&monitor.config)?)
    .bind(monitor.interval_seconds as i64)
    .bind(monitor.timeout_seconds as i64)
    .bind(monitor.enabled)
    .bind(to_timestamp_seconds(monitor.updated_at))
    .bind(id)
    .execute(pool)
    .await?;

    Ok(monitor)
}

/// 删除监控项；关联的探测结果和告警由外键级联删除。
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM monitors WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// 暂停或恢复监控项。
pub async fn set_enabled(pool: &SqlitePool, id: i64, enabled: bool) -> Result<Monitor, AppError> {
    update(
        pool,
        id,
        UpdateMonitor {
            name: None,
            target: None,
            config: None,
            interval_seconds: None,
            timeout_seconds: None,
            enabled: Some(enabled),
        },
    )
    .await
}

fn row_to_monitor(row: sqlx::sqlite::SqliteRow) -> Result<Monitor, AppError> {
    let kind: String = row.try_get("kind")?;
    let config_json: String = row.try_get("config_json")?;
    let created_at: i64 = row.try_get("created_at")?;
    let updated_at: i64 = row.try_get("updated_at")?;

    Ok(Monitor {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: MonitorKind::try_from(kind.as_str())?,
        target: row.try_get("target")?,
        config: serde_json::from_str(&config_json)?,
        interval_seconds: row.try_get::<i64, _>("interval_seconds")? as u64,
        timeout_seconds: row.try_get::<i64, _>("timeout_seconds")? as u64,
        enabled: row.try_get("enabled")?,
        created_at: from_timestamp_seconds(created_at)?,
        updated_at: from_timestamp_seconds(updated_at)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            check::CheckResult,
            monitor::{MonitorConfig, MonitorKind},
        },
        storage::{alerts, checks},
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn monitor_crud_updates_enabled_and_deletes_missing_as_not_found() {
        let pool = test_support::pool("monitors-crud").await;
        let mut monitor = test_support::monitor(MonitorKind::Http);
        monitor.name = "first".into();

        let inserted = insert(&pool, &monitor).await.unwrap();
        assert!(inserted.id > 0);
        assert_eq!(list(&pool).await.unwrap().len(), 1);
        assert_eq!(get(&pool, inserted.id).await.unwrap().name, "first");

        let updated = update(
            &pool,
            inserted.id,
            UpdateMonitor {
                name: Some("second".into()),
                target: Some("https://example.org".into()),
                config: Some(MonitorConfig {
                    expected_status: Some(204),
                    ..MonitorConfig::default()
                }),
                interval_seconds: None,
                timeout_seconds: Some(2),
                enabled: Some(false),
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "second");
        assert_eq!(updated.timeout_seconds, 2);
        assert!(!updated.enabled);

        let resumed = set_enabled(&pool, inserted.id, true).await.unwrap();
        assert!(resumed.enabled);

        delete(&pool, inserted.id).await.unwrap();
        assert!(matches!(
            get(&pool, inserted.id).await,
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            delete(&pool, inserted.id).await,
            Err(AppError::NotFound)
        ));
    }

    #[tokio::test]
    async fn changing_interval_clears_dependent_history() {
        let pool = test_support::pool("monitors-clear-history").await;
        let monitor = insert(&pool, &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        checks::insert_many_tx(&mut tx, &[CheckResult::success(monitor.id, 42)])
            .await
            .unwrap();
        tx.commit().await.unwrap();
        alerts::insert(
            &pool,
            &crate::domain::alert::AlertEvent {
                id: None,
                monitor_id: monitor.id,
                kind: crate::domain::alert::AlertKind::Triggered,
                message: "down".into(),
                delivered: false,
                created_at: monitor.created_at,
            },
        )
        .await
        .unwrap();

        update(
            &pool,
            monitor.id,
            UpdateMonitor {
                name: None,
                target: None,
                config: None,
                interval_seconds: Some(10),
                timeout_seconds: None,
                enabled: None,
            },
        )
        .await
        .unwrap();

        assert!(
            checks::list_for_monitor(&pool, monitor.id, 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            alerts::latest_for_monitor(&pool, monitor.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn get_tx_reads_monitor_inside_transaction() {
        let pool = test_support::pool("monitors-get-tx").await;
        let monitor = insert(&pool, &test_support::monitor(MonitorKind::Dns))
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();

        let loaded = get_tx(&mut tx, monitor.id).await.unwrap();

        assert_eq!(loaded.kind, MonitorKind::Dns);
        tx.commit().await.unwrap();
    }
}
