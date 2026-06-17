//! Dashboard 和公开状态页 API。

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::Serialize;

use crate::{
    domain::{check::CheckStatus, monitor::Monitor},
    error::AppError,
    state::AppState,
    storage::{alerts, checks, monitors},
};

#[derive(Debug, Serialize)]
pub struct Dashboard {
    /// 全部监控项。
    monitors: Vec<Monitor>,
    /// 每个监控项最近一次探测结果，key 为 monitor id。
    latest: HashMap<String, crate::domain::check::CheckResult>,
    /// 最近告警事件。
    alerts: Vec<crate::domain::alert::AlertEvent>,
    total: usize,
    success: usize,
    failed: usize,
    unknown: usize,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/status-pages/:slug", get(status_page))
}

async fn dashboard(State(state): State<AppState>) -> Result<Json<Dashboard>, AppError> {
    let monitors = monitors::list(state.pool()).await?;
    let latest: HashMap<_, _> = checks::latest_by_monitor(state.pool())
        .await?
        .into_iter()
        .map(|result| (result.monitor_id.clone(), result))
        .collect();
    let buffered_latest = state.check_buffer().latest_by_monitor().await;
    let latest = merge_latest(latest, buffered_latest);
    let alerts = alerts::list(state.pool(), 10).await?;
    let total = monitors.len();
    // Dashboard 的当前状态只看最新一次探测结果；没有结果的监控项不计入 up/down。
    let failed = latest
        .values()
        .filter(|result| result.status == CheckStatus::Failed)
        .count();
    let success = latest
        .values()
        .filter(|result| result.status == CheckStatus::Success)
        .count();
    let unknown = total.saturating_sub(success + failed);

    Ok(Json(Dashboard {
        monitors,
        latest,
        alerts,
        total,
        success,
        failed,
        unknown,
    }))
}

fn merge_latest(
    mut latest: HashMap<String, crate::domain::check::CheckResult>,
    buffered_latest: HashMap<String, crate::domain::check::CheckResult>,
) -> HashMap<String, crate::domain::check::CheckResult> {
    for (monitor_id, result) in buffered_latest {
        let entry = latest.entry(monitor_id).or_insert_with(|| result.clone());
        if result.checked_at > entry.checked_at {
            *entry = result;
        }
    }
    latest
}

async fn status_page(
    State(state): State<AppState>,
    Path(_slug): Path<String>,
) -> Result<Json<Dashboard>, AppError> {
    dashboard(State(state)).await
}
