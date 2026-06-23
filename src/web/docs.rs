//! OpenAPI 文档和 Swagger UI 路由。

use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    domain::{
        alert::{AlertEvent, AlertKind},
        check::{
            AggregateBucketSize, AggregatePoint, CheckAggregate, CheckResult, CheckSeriesPoint,
            CheckStatus, LatencyMetrics,
        },
        monitor::{
            CreateMonitor, DnsRecordType, HeaderMatchMode, HttpHeaderMatch, Monitor, MonitorConfig,
            MonitorKind, UpdateMonitor,
        },
    },
    state::AppState,
    web::api::{checks::ChecksResponse, status::Dashboard},
};

#[derive(OpenApi)]
#[openapi(
    info(title = "Netwatch API", version = "0.1.0"),
    paths(
        crate::web::api::monitors::list,
        crate::web::api::monitors::create,
        crate::web::api::monitors::get_one,
        crate::web::api::monitors::update,
        crate::web::api::monitors::delete_one,
        crate::web::api::monitors::pause,
        crate::web::api::monitors::resume,
        crate::web::api::checks::list,
        crate::web::api::alerts::list,
        crate::web::api::status::dashboard,
        crate::web::api::status::status_page
    ),
    components(schemas(
        AlertEvent,
        AlertKind,
        AggregateBucketSize,
        AggregatePoint,
        CheckAggregate,
        CheckResult,
        CheckSeriesPoint,
        CheckStatus,
        ChecksResponse,
        CreateMonitor,
        Dashboard,
        DnsRecordType,
        HeaderMatchMode,
        HttpHeaderMatch,
        LatencyMetrics,
        Monitor,
        MonitorConfig,
        MonitorKind,
        UpdateMonitor
    )),
    tags(
        (name = "monitors", description = "监控项管理"),
        (name = "checks", description = "探测结果查询"),
        (name = "alerts", description = "告警事件查询"),
        (name = "status", description = "Dashboard 和公开状态页")
    )
)]
pub(crate) struct ApiDoc;

/// 注册交互式文档页和 OpenAPI JSON。
pub fn router() -> Router<AppState> {
    SwaggerUi::new("/docs")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}

#[cfg(test)]
mod tests {
    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn openapi_contains_core_paths_and_schemas() {
        let openapi = ApiDoc::openapi();
        let json = serde_json::to_string(&openapi).unwrap();

        assert!(json.contains("\"/api/monitors\""));
        assert!(json.contains("\"/api/monitors/{id}/checks\""));
        assert!(json.contains("\"/api/dashboard\""));

        let schemas = &openapi.components.as_ref().unwrap().schemas;
        assert!(schemas.contains_key("Monitor"));
        assert!(schemas.contains_key("CreateMonitor"));
        assert!(schemas.contains_key("CheckResult"));
        assert!(schemas.contains_key("AlertEvent"));

        let _router = super::router();
    }
}
