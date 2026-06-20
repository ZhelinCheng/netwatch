//! 前端静态资源服务。

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

const DASHBOARD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dashboard");
const DASHBOARD_INDEX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dashboard/index.html");

/// 注册 Web UI 页面路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .fallback_service(ServeDir::new(DASHBOARD_DIR).fallback(ServeFile::new(DASHBOARD_INDEX)))
}
