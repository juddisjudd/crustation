use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

/// Every failure the API can report. The wire shape is fixed by docs/API.md.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not signed in")]
    Unauthenticated,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{message}")]
    Validation {
        message: String,
        fields: BTreeMap<String, String>,
    },
    #[error("too many attempts")]
    RateLimited { retry_after: u64 },
    #[allow(dead_code, reason = "returned by provider downloads")]
    #[error("{0}")]
    Unavailable(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn forbidden(what: impl Into<String>) -> Self {
        Self::Forbidden(what.into())
    }

    pub fn not_found(what: impl Into<String>) -> Self {
        Self::NotFound(what.into())
    }

    pub fn conflict(what: impl Into<String>) -> Self {
        Self::Conflict(what.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn field(field: impl Into<String>, message: impl Into<String>) -> Self {
        let mut fields = BTreeMap::new();
        let field = field.into();
        let message = message.into();
        fields.insert(field, message.clone());
        Self::Validation { message, fields }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Validation { .. } => "VALIDATION",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Unavailable(_) => "UNAVAILABLE",
            Self::Internal(_) => "SERVER_ERROR",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound("record".into()),
            other => Self::Internal(other.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(error) = &self {
            tracing::error!(?error, "request failed");
        }
        let mut body = json!({
            "status": "error",
            "error": self.code(),
            "message": match &self {
                // Internal details stay in the log, not in the response.
                Self::Internal(_) => "Something went wrong on the server.".to_string(),
                other => other.to_string(),
            },
        });
        match &self {
            Self::Validation { fields, .. } if !fields.is_empty() => {
                body["fields"] = json!(fields);
            }
            Self::RateLimited { retry_after } => {
                body["retry_after"] = json!(retry_after);
            }
            _ => {}
        }
        (self.status(), Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Wraps a payload as `{"status":"ok","data": …}`.
pub struct Ok<T>(pub T);

impl<T: Serialize> IntoResponse for Ok<T> {
    fn into_response(self) -> Response {
        Json(json!({ "status": "ok", "data": self.0 })).into_response()
    }
}

/// `{"status":"ok"}` for actions with nothing to return.
pub struct Done;

impl IntoResponse for Done {
    fn into_response(self) -> Response {
        Json(json!({ "status": "ok" })).into_response()
    }
}

#[allow(dead_code, reason = "used by create endpoints")]
pub struct Created<T>(pub T);

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::CREATED,
            Json(json!({ "status": "ok", "data": self.0 })),
        )
            .into_response()
    }
}
