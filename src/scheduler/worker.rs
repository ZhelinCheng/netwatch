//! 单个监控项的一次 worker 执行。

use chrono::Utc;

use crate::{
    domain::monitor::Monitor,
    probes,
    scheduler::evaluator,
    state::AppState,
    storage::{checks, monitors},
};

/// 如果监控项已到达探测间隔，则执行探测、落库并触发告警评估。
pub async fn run_once(state: AppState, monitor: Monitor) -> anyhow::Result<()> {
    let recent = checks::list_for_monitor(state.pool(), &monitor.id, 1).await?;
    if let Some(last) = recent.first() {
        let elapsed = Utc::now().signed_duration_since(last.checked_at);
        if elapsed.num_seconds() < monitor.interval_seconds as i64 {
            return Ok(());
        }
    }

    let fresh_monitor = monitors::get(state.pool(), &monitor.id).await?;
    if !fresh_monitor.enabled {
        return Ok(());
    }

    let result = probes::run(&fresh_monitor).await?;
    checks::insert(state.pool(), &result).await?;
    evaluator::evaluate(&state, &fresh_monitor, &result).await?;

    Ok(())
}
