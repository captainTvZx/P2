//! NestJS hata biçimi. `throw new XException({ code, message, ... })` gövdesi nesnenin
//! kendisidir (alanlar dahil, `context: { safe: true }` bile istemciye gider).

use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: Value,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &str, message: &str) -> Self {
        Self { status, body: json!({ "code": code, "message": message }) }
    }
    pub fn unauthorized(code: &str, message: &str) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message)
    }
    pub fn bad_request(code: &str, message: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    pub fn conflict(code: &str, message: &str) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }
    pub fn field(mut self, field: &str) -> Self {
        self.obj().insert("field".into(), json!(field));
        self
    }
    /// `context: { safe: true }` — NestJS sürümü bunu da gövdeye koyuyor.
    pub fn safe(mut self) -> Self {
        self.obj().insert("context".into(), json!({ "safe": true }));
        self
    }
    fn obj(&mut self) -> &mut Map<String, Value> {
        self.body.as_object_mut().expect("hata gövdesi nesne")
    }
    /// Beklenmeyen hata: NestJS'in varsayılan 500 gövdesi.
    pub fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!("{error}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: json!({ "statusCode": 500, "message": "Internal server error" }),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self.body)).into_response()
    }
}

/// NestJS'in bilinmeyen yol cevabı.
pub async fn not_found(method: Method, uri: Uri) -> Response {
    let body = json!({ "message": format!("Cannot {} {}", method, uri.path()), "error": "Not Found", "statusCode": 404 });
    (StatusCode::NOT_FOUND, axum::Json(body)).into_response()
}

pub type ApiResult<T> = Result<T, ApiError>;
