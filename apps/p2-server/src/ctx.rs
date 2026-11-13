//! İstek bağlamı ve ortak yardımcılar: kimlik çıkarıcısı (AuthGuard), JSON gövde, NestJS
//! durum kodları, Prisma ile aynı zaman biçimi, kimlik üretimi, SQL'den JSON okuma.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde_json::{Map, Value};

use crate::AppState;
use crate::error::{ApiError, ApiResult};

/// Oturumlu kullanıcı (`req.user.sub`).
pub struct Me {
    pub id: String,
    pub username: String,
}

impl FromRequestParts<AppState> for Me {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let claims = crate::auth::guard(state, &parts.headers)?;
        Ok(Me { id: claims.sub, username: claims.username })
    }
}

/// JSON gövde (Express json(): boş gövde `{}`; dizi/değer → `{}`).
pub struct Body(pub Map<String, Value>);

impl<S: Send + Sync> FromRequest<S> for Body {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|_| ApiError::bad_request("INVALID_BODY", "Gövde okunamadı."))?;
        Ok(Body(crate::validate::body(&bytes)?))
    }
}

impl Body {
    pub fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }
    pub fn has(&self, key: &str) -> bool {
        !matches!(self.0.get(key), None | Some(Value::Null))
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

/// NestJS @Post: 201. @Get/@Patch/@Put/@Delete: 200.
pub type Reply = ApiResult<(StatusCode, Json<Value>)>;

pub fn ok(v: Value) -> Reply {
    Ok((StatusCode::OK, Json(v)))
}

pub fn created(v: Value) -> Reply {
    Ok((StatusCode::CREATED, Json(v)))
}

/// NestJS `undefined` dönüşü: boş gövde 200.
pub fn empty() -> Reply {
    Ok((StatusCode::OK, Json(Value::Null)))
}

pub fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub use crate::auth::now;

pub fn not_found(code: &str, message: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, code, message)
}

pub fn forbidden(code: &str, message: &str) -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, code, message)
}

/// Prisma'nın Date → JSON biçimi: `2026-11-12T19:33:00.123Z`. SQL ifadesine çevirir.
#[macro_export]
macro_rules! iso {
    ($col:expr) => {
        concat!("to_char(", $col, r#", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')"#)
    };
}

/// Çalışma anında kurulan SQL (yalnız iç sabitlerden; kullanıcı girdisi her zaman bind ile).
pub fn sql(text: String) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(text)
}

/// Tek satır JSON döndüren sorgu (`select json_build_object(...)`).
pub async fn one_json(q: sqlx::query::QueryScalar<'_, sqlx::Postgres, sqlx::types::Json<Value>, sqlx::postgres::PgArguments>, db: &sqlx::PgPool) -> ApiResult<Option<Value>> {
    Ok(q.fetch_optional(db).await?.map(|j| j.0))
}

/// Çok satırlı JSON (her satır bir nesne) → dizi.
pub async fn many_json(q: sqlx::query::QueryScalar<'_, sqlx::Postgres, sqlx::types::Json<Value>, sqlx::postgres::PgArguments>, db: &sqlx::PgPool) -> ApiResult<Vec<Value>> {
    Ok(q.fetch_all(db).await?.into_iter().map(|j| j.0).collect())
}

/// Sayfalama (paging.ts `pageArgs`): `before` ISO imleci, `limit` 1..max.
pub fn page_args(before: Option<&str>, limit: Option<&str>, take: i64, max: i64) -> (i64, Option<chrono::NaiveDateTime>) {
    let max = max.max(1);
    let fallback = take.max(1).min(max);
    let take = limit.map(str::trim).filter(|l| !l.is_empty()).and_then(|l| l.parse::<f64>().ok()).filter(|f| f.is_finite()).map(|f| (f.floor() as i64).clamp(1, max)).unwrap_or(fallback);
    let cursor = before.and_then(|b| chrono::DateTime::parse_from_rfc3339(b).ok()).map(|d| d.naive_utc());
    (take, cursor)
}

/// Sorgu dizgisi parametreleri.
pub type Query = axum::extract::Query<std::collections::HashMap<String, String>>;

#[cfg(test)]
mod tests {
    // server/src/paging.spec.ts karşılığı.
    use super::page_args;

    #[test]
    fn varsayilanlar() {
        assert_eq!(page_args(None, None, 100, 100), (100, None));
        assert_eq!(page_args(None, None, 200, 200).0, 200);
        assert_eq!(page_args(None, None, 500, 200).0, 200);
    }

    #[test]
    fn limit_kistirma() {
        for (l, want) in [("50", 50), ("0", 1), ("-5", 1), ("999", 100), ("12.9", 12), ("abc", 100), ("", 100)] {
            assert_eq!(page_args(None, Some(l), 100, 100).0, want, "limit {l}");
        }
        assert_eq!(page_args(None, Some("300"), 200, 200).0, 200);
    }

    #[test]
    fn imlec() {
        let (take, cursor) = page_args(Some("2026-10-23T10:00:00.000Z"), Some("50"), 100, 100);
        assert_eq!(take, 50);
        assert_eq!(cursor.unwrap().to_string(), "2026-10-23 10:00:00");
        assert!(page_args(Some("not-a-date"), None, 100, 100).1.is_none());
        assert!(page_args(Some(""), None, 100, 100).1.is_none());
    }
}
