//! 统一错误类型和 HTTP 错误响应转换。

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 请求的资源不存在。
    #[error("not found")]
    NotFound,
    /// 用户输入不合法，返回 400。
    #[error("bad request: {0}")]
    BadRequest(String),
    /// 数据库访问失败。
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// JSON 序列化或反序列化失败。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// 兜底错误，用于封装第三方库或运行时错误。
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Sqlx(_) | AppError::Json(_) | AppError::Other(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        (
            status,
            Json(ErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}
