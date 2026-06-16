//! 监控项管理 API。

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};

use crate::{
    domain::monitor::{CreateMonitor, Monitor, UpdateMonitor},
    error::AppError,
    state::AppState,
    storage::monitors,
};

/// 注册监控项 CRUD 和暂停/恢复路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/monitors", get(list).post(create))
        .route(
            "/api/monitors/:id",
            get(get_one).patch(update).delete(delete_one),
        )
        .route("/api/monitors/:id/pause", post(pause))
        .route("/api/monitors/:id/resume", post(resume))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<Monitor>>, AppError> {
    Ok(Json(monitors::list(state.pool()).await?))
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<CreateMonitor>,
) -> Result<Json<Monitor>, AppError> {
    let monitor = input.into_monitor()?;
    monitors::insert(state.pool(), &monitor).await?;
    Ok(Json(monitor))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Monitor>, AppError> {
    Ok(Json(monitors::get(state.pool(), &id).await?))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateMonitor>,
) -> Result<Json<Monitor>, AppError> {
    Ok(Json(monitors::update(state.pool(), &id, input).await?))
}

async fn delete_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<(), AppError> {
    monitors::delete(state.pool(), &id).await
}

async fn pause(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Monitor>, AppError> {
    Ok(Json(monitors::set_enabled(state.pool(), &id, false).await?))
}

async fn resume(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Monitor>, AppError> {
    Ok(Json(monitors::set_enabled(state.pool(), &id, true).await?))
}
