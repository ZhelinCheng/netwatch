//! 告警事件查询 API。

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{domain::alert::AlertEvent, error::AppError, state::AppState, storage::alerts};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct LimitQuery {
    /// 返回最近 N 条告警，范围 1..=500，默认 50。
    limit: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/alerts", get(list))
}

#[utoipa::path(
    get,
    path = "/api/alerts",
    operation_id = "list_alerts",
    tag = "alerts",
    params(LimitQuery),
    responses(
        (status = 200, description = "告警事件列表", body = Vec<AlertEvent>),
        (status = 500, description = "服务端错误")
    )
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<AlertEvent>>, AppError> {
    Ok(Json(
        alerts::list(state.pool(), query.limit.unwrap_or(50).clamp(1, 500)).await?,
    ))
}
