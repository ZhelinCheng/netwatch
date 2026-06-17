//! 单个监控项的一次 worker 执行。

use chrono::Utc;

use crate::{
    domain::monitor::Monitor,
    probes,
    state::AppState,
    storage::{checks, monitors},
};

/// 如果监控项已到达探测间隔，则执行探测、落库并触发告警评估。
pub async fn run_once(state: AppState, monitor: Monitor) -> anyhow::Result<()> {
    let recent = checks::list_for_monitor(state.pool(), &monitor.id, 1).await?;
    let buffered = state.check_buffer().latest_for_monitor(&monitor.id).await;
    // 同时参考数据库和未落库缓冲，避免 flush 间隔内重复探测同一个监控项。
    let latest = recent
        .first()
        .cloned()
        .into_iter()
        .chain(buffered)
        .max_by_key(|result| result.checked_at);
    if let Some(last) = latest {
        let elapsed = Utc::now().signed_duration_since(last.checked_at);
        if elapsed.num_seconds() < monitor.interval_seconds as i64 {
            return Ok(());
        }
    }

    // 真正探测前重新读取监控项，处理用户刚刚暂停或修改配置的情况。
    let fresh_monitor = monitors::get(state.pool(), &monitor.id).await?;
    if !fresh_monitor.enabled {
        return Ok(());
    }

    let result = probes::run(&fresh_monitor).await?;
    // 先写入内存缓冲，由 flush 任务批量落库，降低高频探测时的 SQLite 写入压力。
    state.check_buffer().append(result).await;

    Ok(())
}
