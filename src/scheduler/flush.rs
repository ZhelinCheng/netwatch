//! 探测结果缓冲批量落库。

use crate::{
    error::AppError,
    scheduler::evaluator,
    state::AppState,
    storage::{checks, monitors},
};

/// 将内存缓冲中的探测结果批量写入数据库，并逐条触发告警评估。
pub async fn run(state: AppState) -> anyhow::Result<()> {
    let mut results = state.check_buffer().drain_all().await;
    if results.is_empty() {
        return Ok(());
    }
    // 按探测时间落库，保证连续失败判断看到的最近结果顺序稳定。
    results.sort_by_key(|result| result.checked_at);

    if let Err(error) = checks::insert_many(state.pool(), &results).await {
        // 数据库写入失败时把结果放回队首，等待下一轮 flush 重试。
        state.check_buffer().requeue_front(results).await;
        return Err(error.into());
    }

    for result in results {
        match monitors::get(state.pool(), &result.monitor_id).await {
            Ok(monitor) => {
                // 告警失败不影响探测数据持久化，只记录日志等待下一次状态变化。
                if let Err(error) = evaluator::evaluate(&state, &monitor, &result).await {
                    tracing::warn!(?error, monitor_id = %result.monitor_id, "alert evaluation after flush failed");
                }
            }
            Err(AppError::NotFound) => {}
            Err(error) => {
                tracing::warn!(?error, monitor_id = %result.monitor_id, "failed to load monitor for alert evaluation");
            }
        }
    }

    Ok(())
}
