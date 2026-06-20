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
    tracing::info!(
        name = %monitor.name,
        kind = monitor.kind.as_str(),
        target = %monitor.target,
        interval_seconds = monitor.interval_seconds,
        timeout_seconds = monitor.timeout_seconds,
        enabled = monitor.enabled,
        "creating monitor"
    );
    let monitor = monitors::insert(state.pool(), &monitor).await?;
    state.monitor_cache().mark_dirty().await;
    tracing::info!(monitor_id = monitor.id, name = %monitor.name, "monitor created");
    Ok(Json(monitor))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Monitor>, AppError> {
    Ok(Json(monitors::get(state.pool(), id).await?))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<UpdateMonitor>,
) -> Result<Json<Monitor>, AppError> {
    tracing::info!(monitor_id = id, "updating monitor");
    let monitor = monitors::update(state.pool(), id, input).await?;
    state.monitor_cache().mark_dirty().await;
    tracing::info!(
        monitor_id = monitor.id,
        name = %monitor.name,
        enabled = monitor.enabled,
        interval_seconds = monitor.interval_seconds,
        timeout_seconds = monitor.timeout_seconds,
        "monitor updated"
    );
    Ok(Json(monitor))
}

async fn delete_one(State(state): State<AppState>, Path(id): Path<i64>) -> Result<(), AppError> {
    tracing::info!(monitor_id = id, "deleting monitor");
    monitors::delete(state.pool(), id).await?;
    state.monitor_cache().mark_dirty().await;
    tracing::info!(monitor_id = id, "monitor deleted");
    Ok(())
}

async fn pause(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Monitor>, AppError> {
    tracing::info!(monitor_id = id, "pausing monitor");
    let monitor = monitors::set_enabled(state.pool(), id, false).await?;
    state.monitor_cache().mark_dirty().await;
    tracing::info!(monitor_id = id, "monitor paused");
    Ok(Json(monitor))
}

async fn resume(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Monitor>, AppError> {
    tracing::info!(monitor_id = id, "resuming monitor");
    let monitor = monitors::set_enabled(state.pool(), id, true).await?;
    state.monitor_cache().mark_dirty().await;
    tracing::info!(monitor_id = id, "monitor resumed");
    Ok(Json(monitor))
}
