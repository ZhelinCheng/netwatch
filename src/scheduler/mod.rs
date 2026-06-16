//! 后台调度器。
//!
//! 调度器按固定 tick 扫描启用的监控项，具体是否到达探测间隔由 worker 判断。

pub mod evaluator;
pub mod worker;

use tokio::time;

use crate::{state::AppState, storage::monitors};

pub struct Scheduler;

impl Scheduler {
    /// 启动一个后台任务，随 Web 服务进程生命周期运行。
    pub fn start(state: AppState) {
        tokio::spawn(async move {
            let mut ticker = time::interval(state.config().scheduler_tick);
            loop {
                ticker.tick().await;
                if let Err(error) = tick(state.clone()).await {
                    tracing::warn!(?error, "scheduler tick failed");
                }
            }
        });
    }
}

async fn tick(state: AppState) -> anyhow::Result<()> {
    let monitors = monitors::list(state.pool()).await?;
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
