use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid id type: {0}")]
    InvalidIdType(String),

    #[error("id not found: {0}")]
    IdNotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("API error: {0}")]
    Api(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),

    #[error("database error: {0}")]
    DbError(String),

    #[error("{0}")]
    Other(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::InvalidIdType(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::IdNotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".into()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into()),
        };
        tracing::error!(%status, error = %self);
        (status, axum::Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    use http_body_util::BodyExt;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    async fn body_of(err: AppError) -> serde_json::Value {
        let resp = err.into_response();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn invalid_id_type_is_400() {
        assert_eq!(
            status_of(AppError::InvalidIdType("x".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn id_not_found_is_404() {
        assert_eq!(
            status_of(AppError::IdNotFound("x".into())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn unauthorized_is_401() {
        assert_eq!(status_of(AppError::Unauthorized), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_is_403() {
        assert_eq!(
            status_of(AppError::Forbidden("x".into())),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn bad_request_is_400() {
        assert_eq!(
            status_of(AppError::BadRequest("x".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn db_error_is_500() {
        assert_eq!(
            status_of(AppError::DbError("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn other_is_500() {
        assert_eq!(
            status_of(AppError::Other("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn internal_errors_redact_details() {
        let body = body_of(AppError::DbError("connection refused to 10.0.0.5:5432".into())).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn internal_error_other_redacts_details() {
        let body = body_of(AppError::Other("secret internal info".into())).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn client_errors_preserve_message() {
        let body = body_of(AppError::BadRequest("missing field".into())).await;
        assert_eq!(body["error"], "missing field");
    }
}
