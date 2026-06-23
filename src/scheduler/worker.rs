//! 单个监控项的一次 worker 执行。

use chrono::{Duration, Utc};

use crate::{
    domain::monitor::Monitor,
    probes,
    state::AppState,
    storage::{checks, monitors},
};

/// 如果监控项已到达探测间隔，则执行探测、落库并触发告警评估。
pub async fn run_once(state: AppState, monitor: Monitor) -> anyhow::Result<()> {
    let recent = checks::list_for_monitor(state.pool(), monitor.id, 1).await?;
    let buffered = state.check_buffer().latest_for_monitor(monitor.id).await;
    // 同时参考数据库和未落库缓冲，避免 flush 间隔内重复探测同一个监控项。
    let latest = recent
        .first()
        .cloned()
        .into_iter()
        .chain(buffered)
        .max_by_key(|result| result.checked_at);
    if let Some(last) = latest {
        let elapsed = Utc::now().signed_duration_since(last.checked_at);
        if should_skip_before_interval(elapsed, monitor.interval_seconds) {
            tracing::debug!(
                monitor_id = monitor.id,
                elapsed_milliseconds = elapsed.num_milliseconds(),
                interval_seconds = monitor.interval_seconds,
                "monitor probe skipped before interval"
            );
            return Ok(());
        }
    }

    // 真正探测前重新读取监控项，处理用户刚刚暂停或修改配置的情况。
    let fresh_monitor = monitors::get(state.pool(), monitor.id).await?;
    if !fresh_monitor.enabled {
        tracing::debug!(
            monitor_id = monitor.id,
            "monitor probe skipped because disabled"
        );
        return Ok(());
    }

    tracing::debug!(
        monitor_id = fresh_monitor.id,
        name = %fresh_monitor.name,
        kind = fresh_monitor.kind.as_str(),
        target = %fresh_monitor.target,
        "running monitor probe"
    );
    let result = probes::run(&fresh_monitor).await?;
    tracing::info!(
        monitor_id = fresh_monitor.id,
        name = %fresh_monitor.name,
        status = result.status.as_str(),
        latency_us = ?result.latency_us,
        "monitor probe completed"
    );
    // 先写入内存缓冲，由 flush 任务批量落库，降低高频探测时的 SQLite 写入压力。
    state.check_buffer().append(result).await;

    Ok(())
}

/// 判断距离上次探测是否仍未达到间隔，保留 500ms 调度抖动容忍。
fn should_skip_before_interval(elapsed: Duration, interval_seconds: u64) -> bool {
    let interval = Duration::seconds(interval_seconds as i64);
    let scheduler_jitter = Duration::milliseconds(500);
    elapsed + scheduler_jitter < interval
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::{
        domain::{
            check::CheckResult,
            monitor::{MonitorKind, UpdateMonitor},
        },
        storage::{checks, monitors},
        test_support,
    };

    use super::*;

    #[test]
    fn interval_skip_allows_scheduler_jitter() {
        assert!(should_skip_before_interval(Duration::milliseconds(4_400), 5));
        assert!(!should_skip_before_interval(Duration::milliseconds(4_500), 5));
        assert!(!should_skip_before_interval(Duration::seconds(5), 5));
    }

    #[tokio::test]
    async fn run_once_skips_when_recent_result_is_inside_interval() {
        let state = test_support::state("worker-skip-recent").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let mut recent = CheckResult::success(monitor.id, 10);
        recent.checked_at = Utc::now();
        let mut tx = state.pool().begin().await.unwrap();
        checks::insert_many_tx(&mut tx, &[recent]).await.unwrap();
        tx.commit().await.unwrap();

        run_once(state.clone(), monitor).await.unwrap();

        assert!(state.check_buffer().drain_all().await.is_empty());
    }

    #[tokio::test]
    async fn run_once_skips_freshly_disabled_monitor() {
        let state = test_support::state("worker-skip-disabled").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        monitors::update(
            state.pool(),
            monitor.id,
            UpdateMonitor {
                name: None,
                target: None,
                config: None,
                interval_seconds: None,
                timeout_seconds: None,
                enabled: Some(false),
            },
        )
        .await
        .unwrap();

        run_once(state.clone(), monitor).await.unwrap();

        assert!(state.check_buffer().drain_all().await.is_empty());
    }
}
