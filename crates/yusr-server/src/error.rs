use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use yusr_infra::InfraError;
use yusr_payments::PaymentError;

/// API error rendered as `{"error": "<code>", "message": "<text>"}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "authentication required")
    }

    pub fn forbidden(permission: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", format!("missing permission `{permission}`"))
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.code, "message": self.message }))).into_response()
    }
}

impl From<InfraError> for ApiError {
    fn from(e: InfraError) -> Self {
        match &e {
            InfraError::Domain(d) => Self::new(StatusCode::UNPROCESSABLE_ENTITY, d.code(), d.to_string()),
            InfraError::NotFound(_) => Self::new(StatusCode::NOT_FOUND, "not_found", e.to_string()),
            InfraError::Conflict(code, _) => Self::new(StatusCode::CONFLICT, code, e.to_string()),
            _ if e.violation().is_some() => {
                let (code, constraint) = e.violation().expect("checked above");
                Self::new(StatusCode::CONFLICT, code, constraint.unwrap_or_default())
            }
            _ => {
                tracing::error!(error = %e, "internal error");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error")
            }
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        InfraError::from(e).into()
    }
}

impl From<yusr_domain::DomainError> for ApiError {
    fn from(e: yusr_domain::DomainError) -> Self {
        InfraError::Domain(e).into()
    }
}

impl From<PaymentError> for ApiError {
    fn from(e: PaymentError) -> Self {
        match e {
            PaymentError::BadSignature => Self::new(StatusCode::UNAUTHORIZED, "bad_signature", e.to_string()),
            PaymentError::UnknownGateway(_) | PaymentError::Unsupported { .. } => Self::new(StatusCode::BAD_REQUEST, "unsupported_payment", e.to_string()),
            PaymentError::Malformed(_) => Self::bad_request(e.to_string()),
            PaymentError::Rejected(_) | PaymentError::Http(_) => Self::new(StatusCode::BAD_GATEWAY, "gateway_error", e.to_string()),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
