//! 探测结果缓冲批量落库。

use crate::{
    error::AppError,
    scheduler::{compact, evaluator},
    state::AppState,
    storage::{checks, monitors},
};

/// 将内存缓冲中的探测结果批量写入数据库，并逐条触发告警评估。
pub async fn run(state: AppState) -> anyhow::Result<()> {
    let mut results = state.check_buffer().drain_all().await;
    if results.is_empty() {
        tracing::debug!("check result flush skipped because buffer is empty");
        return Ok(());
    }
    tracing::info!(result_count = results.len(), "flushing check results");
    // 按探测时间落库，保证连续失败判断看到的最近结果顺序稳定。
    results.sort_by_key(|result| result.checked_at);

    if let Err(error) = persist_results_and_rollup_minutes(&state, &results).await {
        // 数据库写入失败时把结果放回队首，等待下一轮 flush 重试。
        tracing::warn!(
            ?error,
            result_count = results.len(),
            "check result flush failed; requeueing results"
        );
        state.check_buffer().requeue_front(results).await;
        return Err(error.into());
    }

    for result in results {
        match monitors::get(state.pool(), result.monitor_id).await {
            Ok(monitor) => {
                // 告警失败不影响探测数据持久化，只记录日志等待下一次状态变化。
                if let Err(error) = evaluator::evaluate(&state, &monitor, &result).await {
                    tracing::warn!(?error, monitor_id = %result.monitor_id, "alert evaluation after flush failed");
                }
            }
            Err(AppError::NotFound) => {
                tracing::debug!(
                    monitor_id = result.monitor_id,
                    "skipping alert evaluation because monitor no longer exists"
                );
            }
            Err(error) => {
                tracing::warn!(?error, monitor_id = %result.monitor_id, "failed to load monitor for alert evaluation");
            }
        }
    }

    if let Err(error) = compact::rollup_hour_if_due(&state).await {
        tracing::warn!(?error, "hour rollup after flush failed");
    }
    if let Err(error) = compact::rollup_day_if_due(&state).await {
        tracing::warn!(?error, "day rollup after flush failed");
    }

    tracing::info!("check result flush completed");
    Ok(())
}

async fn persist_results_and_rollup_minutes(
    state: &AppState,
    results: &[crate::domain::check::CheckResult],
) -> Result<(), AppError> {
    let mut tx = state.pool().begin().await?;
    checks::insert_many_tx(&mut tx, results).await?;
    tracing::debug!(result_count = results.len(), "check results inserted");

    sqlx::query("SAVEPOINT minute_rollup")
        .execute(&mut *tx)
        .await?;
    match compact::rollup_minutes_for_results_tx(&mut tx, results).await {
        Ok(()) => {
            sqlx::query("RELEASE SAVEPOINT minute_rollup")
                .execute(&mut *tx)
                .await?;
        }
        Err(error) => {
            tracing::warn!(?error, "minute rollup after check result insert failed");
            sqlx::query("ROLLBACK TO SAVEPOINT minute_rollup")
                .execute(&mut *tx)
                .await?;
            sqlx::query("RELEASE SAVEPOINT minute_rollup")
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use chrono::{TimeZone, Utc};

    use crate::{
        config::Config,
        domain::{
            check::{AggregateBucketSize, CheckResult},
            monitor::{Monitor, MonitorConfig, MonitorKind},
        },
        state::AppState,
        storage::{aggregates, db, monitors},
    };

    use super::*;

    #[tokio::test]
    async fn flush_upserts_minute_rollup_from_persisted_raw_results() {
        let path = temp_db_path("minute-rollup");
        let database_url = format!("sqlite://{}", path.display());
        let pool = db::connect(&database_url).await.unwrap();
        db::migrate(&pool).await.unwrap();
        let created_at = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let monitor = Monitor {
            id: 1,
            name: "m1".into(),
            kind: MonitorKind::Http,
            target: "https://example.com".into(),
            config: MonitorConfig::default(),
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
            created_at,
            updated_at: created_at,
        };
        monitors::insert(&pool, &monitor).await.unwrap();
        let state = AppState::new(test_config(database_url), pool);
        let minute_start = Utc.with_ymd_and_hms(2026, 6, 16, 1, 2, 0).unwrap();

        let mut first = CheckResult::success(1, 100);
        first.checked_at = minute_start + chrono::Duration::seconds(5);
        state.check_buffer().append(first).await;
        run(state.clone()).await.unwrap();

        let mut second = CheckResult::failed(1, None);
        second.checked_at = minute_start + chrono::Duration::seconds(10);
        state.check_buffer().append(second).await;
        run(state.clone()).await.unwrap();

        let aggregates = aggregates::list_for_monitor_between(
            state.pool(),
            1,
            AggregateBucketSize::Minute,
            minute_start,
            minute_start + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();

        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0].success_count, 1);
        assert_eq!(aggregates[0].failed_count, 1);
        assert_eq!(aggregates[0].unknown_count, 10);
        assert_eq!(aggregates[0].latency_sum_us, 100);
    }

    fn test_config(database_url: String) -> Config {
        Config {
            host: "127.0.0.1".into(),
            port: 4311,
            database_url,
            scheduler_tick: Duration::from_secs(5),
            failure_threshold: 3,
            aggregation_timezone: "UTC".into(),
            compact_interval: Duration::from_secs(600),
            // 让 hour/day 测试窗口落在监控项创建时间之前，避免本测试产生额外 rollup。
            check_flush_interval: Duration::from_secs(10 * 365 * 24 * 60 * 60),
            webhook_url: None,
        }
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        path.push(format!("netwatch-{name}-{suffix}.db"));
        path
    }
}
