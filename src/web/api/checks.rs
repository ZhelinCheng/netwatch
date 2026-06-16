//! 探测结果查询 API。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::check::{CheckResult, LatencyMetrics},
    error::AppError,
    state::AppState,
    storage::checks,
};

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ChecksResponse {
    metrics: LatencyMetrics,
    results: Vec<CheckResult>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/monitors/:id/checks", get(list))
}

async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<ChecksResponse>, AppError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    let results = checks::list_for_monitor(state.pool(), &id, limit).await?;
    let metrics = checks::metrics_for_monitor(state.pool(), &id, limit).await?;

    Ok(Json(ChecksResponse { metrics, results }))
}
