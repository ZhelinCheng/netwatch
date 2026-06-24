//! 探测结果 repository。

use chrono::{DateTime, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{
    domain::check::{CheckResult, CheckStatus},
    error::AppError,
    storage::time::{from_timestamp_seconds, to_timestamp_seconds},
};

/// 在事务内批量写入探测结果。
pub async fn insert_many_tx(
    tx: &mut Transaction<'_, Sqlite>,
    results: &[CheckResult],
) -> Result<(), AppError> {
    for result in results {
        insert_tx(tx, result).await?;
    }

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
            monitor_id, status, latency_us, message, checked_at
        )
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(result.monitor_id)
    .bind(result.status.as_str())
    .bind(result.latency_us.map(|value| value as i64))
    .bind(&result.message)
    .bind(to_timestamp_seconds(result.checked_at))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn list_for_monitor(
    pool: &SqlitePool,
    monitor_id: i64,
    limit: i64,
) -> Result<Vec<CheckResult>, AppError> {
    // 详情页按最近结果展示，因此这里统一按 checked_at 倒序返回。
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, message, checked_at
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
    monitor_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, message, checked_at
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
    monitor_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CheckResult>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, status, latency_us, message, checked_at
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
        SELECT cr.id, cr.monitor_id, cr.status, cr.latency_us, cr.message, cr.checked_at
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
pub async fn delete_for_monitor(pool: &SqlitePool, monitor_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM check_results WHERE monitor_id = ?")
        .bind(monitor_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 在事务内删除指定时间范围内的原始探测结果。
pub async fn delete_for_monitor_between_tx(
    tx: &mut Transaction<'_, Sqlite>,
    monitor_id: i64,
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
        message: row.try_get("message")?,
        checked_at: from_timestamp_seconds(checked_at)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use crate::{
        domain::{check::CheckStatus, monitor::MonitorKind},
        storage::monitors,
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn check_results_can_be_inserted_listed_latest_and_deleted() {
        let pool = test_support::pool("checks-crud").await;
        let monitor = monitors::insert(&pool, &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let base = Utc.with_ymd_and_hms(2026, 6, 17, 8, 0, 0).unwrap();
        let mut first = CheckResult::success(monitor.id, 10);
        first.checked_at = base;
        let mut second = CheckResult::failed(monitor.id, Some(20));
        second.message = "HTTP 状态码 500 不在期望范围 200-399".to_string();
        second.checked_at = base + Duration::seconds(5);

        let mut tx = pool.begin().await.unwrap();
        insert_many_tx(&mut tx, &[first.clone(), second.clone()])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let recent = list_for_monitor(&pool, monitor.id, 10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].status, CheckStatus::Failed);
        assert_eq!(recent[0].message, "HTTP 状态码 500 不在期望范围 200-399");

        let ranged = list_for_monitor_between(&pool, monitor.id, base, base + Duration::seconds(5))
            .await
            .unwrap();
        assert_eq!(ranged.len(), 2);

        let latest = latest_by_monitor(&pool).await.unwrap();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].status, CheckStatus::Failed);

        let mut tx = pool.begin().await.unwrap();
        let tx_results =
            list_for_monitor_between_tx(&mut tx, monitor.id, base, base + Duration::seconds(6))
                .await
                .unwrap();
        assert_eq!(tx_results.len(), 2);
        delete_for_monitor_between_tx(&mut tx, monitor.id, base, base + Duration::seconds(1))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            list_for_monitor(&pool, monitor.id, 10).await.unwrap().len(),
            1
        );
        delete_for_monitor(&pool, monitor.id).await.unwrap();
        assert!(
            list_for_monitor(&pool, monitor.id, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn virtual_unknown_results_are_rejected() {
        let pool = test_support::pool("checks-unknown").await;
        let monitor = monitors::insert(&pool, &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();

        let error = insert_many_tx(&mut tx, &[CheckResult::unknown(monitor.id, Utc::now())])
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::BadRequest(_)));
    }
}
