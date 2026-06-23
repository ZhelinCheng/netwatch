//! REST API 路由入口。

pub mod alerts;
pub mod checks;
pub mod monitors;
pub mod status;

use axum::{Router, routing::get};

use crate::state::AppState;

/// 聚合所有 `/api/*` 路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .merge(monitors::router())
        .merge(checks::router())
        .merge(alerts::router())
        .merge(status::router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_router_can_be_constructed() {
        let _router = super::router();
    }
}
