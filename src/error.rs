use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    Status(StatusCode, String),
    Internal(anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::BAD_REQUEST, msg.into())
    }
    pub fn unauthorized() -> Self {
        Self::Status(StatusCode::UNAUTHORIZED, "Not logged in".into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::FORBIDDEN, msg.into())
    }
    pub fn not_found() -> Self {
        Self::Status(StatusCode::NOT_FOUND, "Not found".into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::CONFLICT, msg.into())
    }
    pub fn too_large(msg: impl Into<String>) -> Self {
        Self::Status(StatusCode::PAYLOAD_TOO_LARGE, msg.into())
    }
    pub fn too_many() -> Self {
        Self::Status(StatusCode::TOO_MANY_REQUESTS, "Too many attempts, try again later".into())
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Status(s, m) => (s, m),
            Self::Internal(e) => {
                tracing::error!("{e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong".to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}
