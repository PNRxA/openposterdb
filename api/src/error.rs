use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid id type: {0}")]
    InvalidIdType(String),

    #[error("id not found: {0}")]
    IdNotFound(String),

    #[error("API error: {0}")]
    Api(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),

    #[error("{0}")]
    Other(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::InvalidIdType(_) => StatusCode::BAD_REQUEST,
            AppError::IdNotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        tracing::error!(%status, error = %self);
        (status, self.to_string()).into_response()
    }
}
