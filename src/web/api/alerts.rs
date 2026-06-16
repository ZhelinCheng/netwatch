//! 告警事件查询 API。

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;

use crate::{domain::alert::AlertEvent, error::AppError, state::AppState, storage::alerts};

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/alerts", get(list))
}

async fn list(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<AlertEvent>>, AppError> {
    Ok(Json(
        alerts::list(state.pool(), query.limit.unwrap_or(50).clamp(1, 500)).await?,
    ))
}
