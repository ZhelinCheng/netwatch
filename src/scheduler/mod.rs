//! 后台调度器。
//!
//! 调度器按固定 tick 扫描启用的监控项，具体是否到达探测间隔由 worker 判断。

pub mod compact;
pub mod evaluator;
pub mod flush;
pub mod worker;

use tokio::time;

use crate::{state::AppState, storage::monitors};

pub struct Scheduler;

impl Scheduler {
    /// 启动一个后台任务，随 Web 服务进程生命周期运行。
    pub fn start(state: AppState) {
        tracing::info!(
            scheduler_tick_seconds = state.config().scheduler_tick.as_secs(),
            compact_interval_seconds = state.config().compact_interval.as_secs(),
            check_flush_interval_seconds = state.config().check_flush_interval.as_secs(),
            "starting scheduler loops"
        );
        let monitor_state = state.clone();
        tokio::spawn(async move {
            let mut ticker = time::interval(monitor_state.config().scheduler_tick);
            loop {
                ticker.tick().await;
                if let Err(error) = tick(monitor_state.clone()).await {
                    tracing::warn!(?error, "scheduler tick failed");
                }
            }
        });

        let compact_state = state.clone();
        tokio::spawn(async move {
            let mut ticker = time::interval(compact_state.config().compact_interval);
            loop {
                ticker.tick().await;
                if let Err(error) = compact::run(compact_state.clone()).await {
                    tracing::warn!(?error, "compact tick failed");
                }
            }
        });

        let flush_state = state.clone();
        tokio::spawn(async move {
            let mut ticker = time::interval(flush_state.config().check_flush_interval);
            loop {
                ticker.tick().await;
                if let Err(error) = flush::run(flush_state.clone()).await {
                    tracing::warn!(?error, "check result flush failed");
                }
            }
        });
    }
}

async fn tick(state: AppState) -> anyhow::Result<()> {
    let monitors = if let Some(monitors) = state.monitor_cache().snapshot().await {
        tracing::debug!(
            monitor_count = monitors.len(),
            "scheduler tick using cached monitors"
        );
        monitors
    } else {
        let monitors = monitors::list(state.pool()).await?;
        tracing::info!(
            monitor_count = monitors.len(),
            "scheduler monitor cache refreshed"
        );
        state.monitor_cache().replace(monitors.clone()).await;
        monitors
    };
    let enabled_count = monitors.iter().filter(|monitor| monitor.enabled).count();
    tracing::debug!(
        monitor_count = monitors.len(),
        enabled_count,
        "scheduler tick loaded monitors"
    );
    for monitor in monitors.into_iter().filter(|monitor| monitor.enabled) {
        let state = state.clone();
        // 每个监控项独立执行，避免慢探测阻塞整个调度循环。
        tokio::spawn(async move {
            if let Err(error) = worker::run_once(state, monitor).await {
                tracing::warn!(?error, "monitor worker failed");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::{
        domain::{check::CheckResult, monitor::MonitorKind},
        storage::{checks, monitors},
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn tick_refreshes_cache_and_uses_cached_monitors() {
        let state = test_support::state("scheduler-tick").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let mut recent = CheckResult::success(monitor.id, 10);
        recent.checked_at = Utc::now();
        let mut tx = state.pool().begin().await.unwrap();
        checks::insert_many_tx(&mut tx, &[recent]).await.unwrap();
        tx.commit().await.unwrap();

        tick(state.clone()).await.unwrap();
        assert!(state.monitor_cache().snapshot().await.is_some());

        tick(state).await.unwrap();
    }
}
