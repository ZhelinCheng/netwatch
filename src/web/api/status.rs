//! Dashboard 和公开状态页 API。

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    domain::{check::CheckStatus, monitor::Monitor},
    error::AppError,
    state::AppState,
    storage::{alerts, checks, monitors},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct Dashboard {
    /// 全部监控项。
    monitors: Vec<Monitor>,
    /// 每个监控项最近一次探测结果，key 为 monitor id。
    latest: HashMap<i64, crate::domain::check::CheckResult>,
    /// 每个监控项最近 1 小时可用率，key 为 monitor id。
    availability: HashMap<i64, f64>,
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
        .route("/api/status-pages/{slug}", get(status_page))
}

#[utoipa::path(
    get,
    path = "/api/dashboard",
    operation_id = "get_dashboard",
    tag = "status",
    responses(
        (status = 200, description = "Dashboard 汇总数据", body = Dashboard),
        (status = 500, description = "服务端错误")
    )
)]
pub(crate) async fn dashboard(State(state): State<AppState>) -> Result<Json<Dashboard>, AppError> {
    let monitors = monitors::list(state.pool()).await?;
    let latest: HashMap<_, _> = checks::latest_by_monitor(state.pool())
        .await?
        .into_iter()
        .map(|result| (result.monitor_id, result))
        .collect();
    let buffered_latest = state.check_buffer().latest_by_monitor().await;
    let latest = merge_latest(latest, buffered_latest);
    let availability = availability_by_monitor(&state, &monitors).await?;
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
        availability,
        alerts,
        total,
        success,
        failed,
        unknown,
    }))
}

fn merge_latest(
    mut latest: HashMap<i64, crate::domain::check::CheckResult>,
    buffered_latest: HashMap<i64, crate::domain::check::CheckResult>,
) -> HashMap<i64, crate::domain::check::CheckResult> {
    for (monitor_id, result) in buffered_latest {
        let entry = latest.entry(monitor_id).or_insert_with(|| result.clone());
        if result.checked_at > entry.checked_at {
            *entry = result;
        }
    }
    latest
}

async fn availability_by_monitor(
    state: &AppState,
    monitors: &[Monitor],
) -> Result<HashMap<i64, f64>, AppError> {
    let to = chrono::Utc::now();
    let from = to - chrono::Duration::hours(1);
    let mut counts = checks::status_counts_by_monitor_between(state.pool(), from, to).await?;

    for monitor in monitors {
        for result in state
            .check_buffer()
            .list_for_monitor_between(monitor.id, from, to)
            .await
        {
            counts.entry(monitor.id).or_default().add_result(&result);
        }
    }

    Ok(monitors
        .iter()
        .map(|monitor| {
            (
                monitor.id,
                counts
                    .get(&monitor.id)
                    .map_or(0.0, checks::StatusCounts::availability),
            )
        })
        .collect())
}

#[utoipa::path(
    get,
    path = "/api/status-pages/{slug}",
    operation_id = "get_status_page",
    tag = "status",
    params(("slug" = String, Path, description = "状态页标识")),
    responses(
        (status = 200, description = "公开状态页数据", body = Dashboard),
        (status = 500, description = "服务端错误")
    )
)]
pub(crate) async fn status_page(
    State(state): State<AppState>,
    Path(_slug): Path<String>,
) -> Result<Json<Dashboard>, AppError> {
    dashboard(State(state)).await
}

#[cfg(test)]
mod tests {
    use axum::extract::{Path, State};
    use chrono::{Duration, Utc};

    use crate::{
        domain::{check::CheckResult, monitor::MonitorKind},
        storage::{checks, monitors},
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn dashboard_merges_buffered_latest_and_status_page_reuses_dashboard() {
        let state = test_support::state("api-dashboard").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let base_time = Utc::now() - Duration::seconds(10);
        let mut persisted = CheckResult::failed(monitor.id, None);
        persisted.checked_at = base_time;
        let mut tx = state.pool().begin().await.unwrap();
        checks::insert_many_tx(&mut tx, &[persisted.clone()])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut buffered = CheckResult::success(monitor.id, 10);
        buffered.checked_at = base_time + Duration::seconds(5);
        state.check_buffer().append(buffered).await;

        let Json(data) = dashboard(State(state.clone())).await.unwrap();
        assert_eq!(data.total, 1);
        assert_eq!(data.success, 1);
        assert_eq!(data.failed, 0);
        assert_eq!(data.unknown, 0);
        assert_eq!(data.latest.get(&monitor.id).unwrap().latency_us, Some(10));
        assert_eq!(data.availability.get(&monitor.id), Some(&50.0));

        let Json(status_data) = status_page(State(state), Path("public".to_string()))
            .await
            .unwrap();
        assert_eq!(status_data.total, 1);
    }

    #[test]
    fn merge_latest_keeps_newest_result_per_monitor() {
        let now = Utc::now();
        let mut old = CheckResult::failed(1, None);
        old.checked_at = now;
        let mut new = CheckResult::success(1, 5);
        new.checked_at = now + Duration::seconds(1);

        let merged = merge_latest(HashMap::from([(1, old)]), HashMap::from([(1, new)]));

        assert_eq!(merged.get(&1).unwrap().latency_us, Some(5));
    }
}
