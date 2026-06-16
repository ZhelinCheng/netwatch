//! Axum 路由组装。

use axum::Router;
use tower_http::cors::CorsLayer;

use crate::state::AppState;

/// 构建完整 HTTP 应用，包括 REST API、内置 UI 和 CORS。
pub fn build(state: AppState) -> Router {
    Router::new()
        .merge(crate::web::api::router())
        .merge(crate::web::ui::router())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
