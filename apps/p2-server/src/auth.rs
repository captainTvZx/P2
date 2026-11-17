//! /health, /setup/*, /auth/* — `server/src/auth.ts` ve `app.ts` karşılığı.
//!
//! Uyumluluk notları:
//! - NestJS'te @Post varsayılan olarak 201 döner; aynısı korunur.
//! - Prisma `@default(now())` ve `@updatedAt` değerlerini istemci tarafında UTC olarak
//!   yazar. Şemadaki `CURRENT_TIMESTAMP` varsayılanı (saat dilimsiz sütunda) ise sunucunun
//!   yerel saatini yazar; bu yüzden zaman alanları burada her zaman açıkça doldurulur.
//! - Parolalar node-argon2 varsayılanlarıyla (argon2id, m=64 MiB, t=3, p=4) özetlenir.

use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use base64::Engine as _;
use chrono::{NaiveDateTime, Timelike, Utc};
use jsonwebtoken::{Algorithm as JwtAlg, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use crate::totp;
use crate::validate::{self, Checker, js_len};

const ACCESS_TTL_SECS: i64 = 15 * 60;

// ------------------------------------------------------------------ yardımcılar

/// Prisma'nın yazdığı gibi milisaniye hassasiyetinde UTC.
pub fn now() -> NaiveDateTime {
    let n = Utc::now().naive_utc();
    n.with_nanosecond(n.nanosecond() / 1_000_000 * 1_000_000).unwrap_or(n)
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn client_version(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("x-p2-client-version")?.to_str().ok()?.trim();
    let ok = (1..=32).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b));
    ok.then(|| raw.to_string())
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(65_536, 3, 4, Some(32)).expect("geçerli argon2 parametreleri");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccessClaims {
    pub sub: String,
    pub username: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Serialize, Deserialize)]
struct RefreshClaims {
    sub: String,
    nonce: String,
    iat: i64,
    exp: i64,
}

fn sign<T: Serialize>(claims: &T, secret: &str) -> ApiResult<String> {
    jsonwebtoken::encode(&Header::default(), claims, &EncodingKey::from_secret(secret.as_bytes())).map_err(ApiError::internal)
}

/// node `jsonwebtoken.verify`: dizgi anahtarla HS256/384/512 kabul, saat toleransı yok.
pub fn verify<T: DeserializeOwned>(token: &str, secret: &str) -> Option<T> {
    let mut validation = Validation::new(JwtAlg::HS256);
    validation.algorithms = vec![JwtAlg::HS256, JwtAlg::HS384, JwtAlg::HS512];
    validation.leeway = 0;
    validation.required_spec_claims = Default::default();
    validation.validate_exp = true;
    jsonwebtoken::decode::<T>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation).ok().map(|d| d.claims)
}

fn access_token(state: &AppState, id: &str, username: &str) -> ApiResult<String> {
    let iat = Utc::now().timestamp();
    sign(&AccessClaims { sub: id.into(), username: username.into(), iat, exp: iat + ACCESS_TTL_SECS }, &state.cfg.access_secret)
}

/// AuthGuard: `Authorization: Bearer …`.
pub fn guard(state: &AppState, headers: &HeaderMap) -> ApiResult<AccessClaims> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            let lower = v.to_ascii_lowercase();
            if lower.starts_with("bearer") { v[6..].trim_start().to_string() } else { v.to_string() }
        })
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ApiError::unauthorized("UNAUTHORIZED", "Oturum gerekli."))?;
    let claims = verify::<AccessClaims>(&token, &state.cfg.access_secret).ok_or_else(|| ApiError::unauthorized("UNAUTHORIZED", "Oturum geçersiz."))?;
    // Sunucu yasağı: oturumlar silinse de erişim belirteci 15 dk geçerli kalırdı.
    if crate::server_admin::is_banned(&claims.sub) {
        return Err(ApiError::unauthorized("SERVER_BANNED", "Bu sunucudan yasaklandın."));
    }
    Ok(claims)
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    email: String,
    username: String,
    display_name: String,
    status: String,
    title: Option<String>,
    avatar_url: Option<String>,
    two_factor_enabled: bool,
    two_factor_secret: Option<String>,
    password_hash: String,
}

/// Sabit SQL metni üretir: sqlx 0.9 çalışma anında birleştirilen SQL'i (enjeksiyon riski) reddeder.
macro_rules! select_user {
    ($where:literal) => {
        concat!(
            r#"select id, email, username, "displayName" as display_name, status::text as status, title,
               "avatarUrl" as avatar_url, "twoFactorEnabled" as two_factor_enabled,
               "twoFactorSecret" as two_factor_secret, "passwordHash" as password_hash from "User" "#,
            $where
        )
    };
}

async fn user_by_id(state: &AppState, id: &str) -> ApiResult<UserRow> {
    sqlx::query_as::<_, UserRow>(select_user!("where id = $1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        // Prisma findUniqueOrThrow → NestJS 500
        .ok_or_else(|| ApiError::internal(format!("kullanıcı yok: {id}")))
}

/// auth.ts `safeUser`.
fn safe_user(u: &UserRow) -> Value {
    json!({
        "id": u.id, "email": u.email, "username": u.username, "displayName": u.display_name,
        "status": u.status, "title": u.title, "avatarUrl": u.avatar_url, "twoFactorEnabled": u.two_factor_enabled,
    })
}

/// Kullanıcı nesnesine sunucu yetkilerini ekler (`serverPermissions: string[]`).
async fn with_server_permissions(state: &AppState, mut user: Value) -> ApiResult<Value> {
    let id = user["id"].as_str().unwrap_or_default().to_string();
    user["serverPermissions"] = json!(crate::server_admin::permissions_of(&state.db, &id).await?);
    Ok(user)
}

async fn touch_client_version(db: impl sqlx::PgExecutor<'_>, user_id: &str, version: &str) -> Result<(), sqlx::Error> {
    let at = now();
    sqlx::query(r#"update "User" set "lastClientVersion" = $2, "lastClientSeenAt" = $3, "updatedAt" = $3 where id = $1"#)
        .bind(user_id)
        .bind(version)
        .bind(at)
        .execute(db)
        .await
        .map(|_| ())
}

/// auth.ts `tokens()`.
async fn issue_tokens(state: &AppState, user: &UserRow, remember: bool, version: Option<String>) -> ApiResult<Value> {
    let access = access_token(state, &user.id, &user.username)?;
    let iat = Utc::now().timestamp();
    let days: i64 = if remember { 30 } else { 1 };
    let nonce = {
        let a: [u8; 32] = rand::random();
        let b: [u8; 16] = rand::random();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([a.as_slice(), b.as_slice()].concat())
    };
    let refresh = sign(&RefreshClaims { sub: user.id.clone(), nonce, iat, exp: iat + days * 86_400 }, &state.cfg.refresh_secret)?;
    let created = now();
    let expires = created + chrono::Duration::milliseconds(days * 86_400_000);

    let mut tx = state.db.begin().await?;
    sqlx::query(r#"insert into "Session" (id, "tokenHash", "userId", "expiresAt", "lastUsedAt", "createdAt") values ($1, $2, $3, $4, $5, $5)"#)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(digest(&refresh))
        .bind(&user.id)
        .bind(expires)
        .bind(created)
        .execute(&mut *tx)
        .await?;
    if let Some(version) = version {
        touch_client_version(&mut *tx, &user.id, &version).await?;
    }
    tx.commit().await?;
    Ok(json!({ "user": with_server_permissions(state, safe_user(user)).await?, "accessToken": access, "refreshToken": refresh }))
}

fn created(value: Value) -> (StatusCode, Json<Value>) {
    (StatusCode::CREATED, Json(value))
}

// ------------------------------------------------------------------ uç noktalar

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "status": "ok", "version": state.cfg.version }))
}

pub async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let admins: i64 = sqlx::query_scalar(r#"select count(*) from "User" where role = 'ADMIN'"#).fetch_one(&state.db).await?;
    Ok(Json(json!({ "installed": admins > 0 })))
}

pub async fn setup_admin(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let admins: i64 = sqlx::query_scalar(r#"select count(*) from "User" where role = 'ADMIN'"#).fetch_one(&state.db).await?;
    // NestJS'te kurulum denetimi doğrulamadan önce çalışır mı? Hayır: @Dto boru hattı önce
    // çalışır, sonra gövde. Sıra korunur.
    let body = validate::body(&body)?;
    check_register(&body)?;
    if admins > 0 {
        return Err(ApiError::conflict("ALREADY_INSTALLED", "P2 kurulumu zaten tamamlanmış."));
    }
    register_inner(&state, &headers, &body, "ADMIN").await.map(created)
}

/// `inviteCode` verilirse (proje davet kodu) önce doğrulanır, hesap açılınca o projeye katılınır.
pub async fn register(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = validate::body(&body)?;
    check_register(&body)?;
    let code = body.get("inviteCode").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
    // Kayıt kuralı "yalnız davetle": bir projenin davet kodu olmadan hesap açılmaz.
    if code.is_none() && crate::server_admin::invite_only(&state.db).await? {
        return Err(crate::ctx::forbidden("REGISTRATION_INVITE_ONLY", "Bu sunucuda kayıt yalnız davetle açık; bir projenin davet kodunu gir.").field("inviteCode").safe());
    }
    if let Some(code) = &code {
        crate::community::usable_invite(&state.db, code).await.map_err(|e| e.field("inviteCode"))?;
    }
    let mut out = register_inner(&state, &headers, &body, "USER").await?;
    if let Some(code) = code {
        let user_id = out["user"]["id"].as_str().unwrap_or_default().to_string();
        let joined = crate::community::join_with_code(&state, &user_id, &code).await?;
        out["joined"] = json!({ "projectId": joined["projectId"], "organizationId": joined["organizationId"] });
    }
    Ok(created(out))
}

/// RegisterDto. class-validator kısıtları dekoratör sırasının TERSİYLE kaydeder
/// (TypeScript dekoratörleri sağdan sola uygular); güvenli mesaj seçimi buna göre.
fn check_register(body: &serde_json::Map<String, Value>) -> ApiResult<()> {
    let mut c = Checker::new(body);
    let email = c.str("email");
    c.field("email", &[(email.is_some_and(validate::is_email), None)]);
    let username = c.str("username");
    c.field(
        "username",
        &[
            (
                username.is_some_and(|u| !u.is_empty() && u.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')),
                Some("Kullanıcı adı boşluk içeremez; yalnızca harf, rakam, nokta ve alt çizgi kullanılabilir."),
            ),
            (username.is_some_and(|u| (3..=24).contains(&js_len(u))), Some("Kullanıcı adı 3-24 karakter olmalı.")),
            (username.is_some(), None),
        ],
    );
    let display = c.str("displayName");
    c.field("displayName", &[(display.is_some_and(|d| (2..=40).contains(&js_len(d))), None), (display.is_some(), None)]);
    let password = c.str("password");
    c.field(
        "password",
        &[
            (
                password.is_some_and(|p| p.chars().any(char::is_lowercase) && p.chars().any(char::is_uppercase) && p.chars().any(|c| c.is_ascii_digit())),
                Some("Parola en az bir büyük harf, bir küçük harf ve bir rakam içermeli."),
            ),
            (password.is_some_and(|p| js_len(p) >= 8), Some("Parola en az 8 karakter olmalı.")),
            (password.is_some(), None),
        ],
    );
    c.field("passwordConfirm", &[(c.str("passwordConfirm").is_some(), None)]);
    c.finish()
}

async fn register_inner(state: &AppState, headers: &HeaderMap, body: &serde_json::Map<String, Value>, role: &str) -> ApiResult<Value> {
    let s = |k: &str| body.get(k).and_then(Value::as_str).unwrap_or_default();
    if s("password") != s("passwordConfirm") {
        return Err(ApiError::bad_request("PASSWORD_MISMATCH", "Parolalar eşleşmiyor.").field("passwordConfirm"));
    }
    let email = s("email").trim().to_lowercase();
    let username = s("username").trim().to_string();
    let existing: Option<String> = sqlx::query_scalar(r#"select email from "User" where email = $1 or lower(username) = lower($2) limit 1"#)
        .bind(&email)
        .bind(&username)
        .fetch_optional(&state.db)
        .await?;
    if let Some(found) = existing {
        return Err(if found == email {
            ApiError::conflict("EMAIL_TAKEN", "Bu e-posta zaten kullanılıyor.").field("email")
        } else {
            ApiError::conflict("USERNAME_TAKEN", "Bu kullanıcı adı zaten kullanılıyor.").field("username")
        });
    }
    let password = s("password").to_string();
    let hash = tokio::task::spawn_blocking(move || argon2().hash_password(password.as_bytes()).map(|h| h.to_string()))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    let id = uuid::Uuid::new_v4().to_string();
    let at = now();
    sqlx::query(
        r#"insert into "User" (id, email, username, "displayName", "passwordHash", role, "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6::"UserRole", $7, $7)"#,
    )
    .bind(&id)
    .bind(&email)
    .bind(&username)
    .bind(s("displayName").trim())
    .bind(&hash)
    .bind(role)
    .bind(at)
    .execute(&state.db)
    .await?;
    // Hesap kurulumun kuruluş kabına girer (proje erişimi vermez; Ana sayfa ve DM uçları için).
    crate::server_admin::ensure_instance_member(&state.db, &id).await?;
    let user = user_by_id(state, &id).await?;
    issue_tokens(state, &user, true, client_version(headers)).await
}

pub async fn login(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = validate::body(&body)?;
    let mut c = Checker::new(&body);
    c.field("identifier", &[(c.str("identifier").is_some(), None)]);
    c.field("password", &[(c.str("password").is_some(), None)]);
    c.field("rememberMe", &[(!c.present("rememberMe") || body["rememberMe"].is_boolean(), None)]);
    c.field("code", &[(!c.present("code") || c.str("code").is_some(), None)]);
    c.finish()?;

    let key = body["identifier"].as_str().unwrap_or_default().trim().to_string();
    let user = sqlx::query_as::<_, UserRow>(select_user!("where email = $1 or username = $2 limit 1"))
        .bind(key.to_lowercase())
        .bind(&key)
        .fetch_optional(&state.db)
        .await?;
    let invalid = || ApiError::unauthorized("INVALID_CREDENTIALS", "E-posta/kullanıcı adı veya parola hatalı.");
    let Some(user) = user else { return Err(invalid()) };
    let password = body["password"].as_str().unwrap_or_default().to_string();
    let stored = user.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&stored).map(|parsed| argon2().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)?;
    if !ok {
        return Err(invalid());
    }
    if let (true, Some(secret)) = (user.two_factor_enabled, user.two_factor_secret.as_deref()) {
        match body.get("code").and_then(Value::as_str).filter(|c| !c.is_empty()) {
            None => return Err(ApiError::unauthorized("TWO_FACTOR_REQUIRED", "Doğrulama kodunu gir.").field("code").safe()),
            Some(code) if !totp::verify(secret, code) => {
                return Err(ApiError::unauthorized("TWO_FACTOR_INVALID", "Doğrulama kodu geçersiz.").field("code").safe());
            }
            Some(_) => {}
        }
    }
    if let Some(reason) = crate::server_admin::ban_of(&state.db, &user.id).await? {
        return Err(crate::server_admin::banned_error(reason.as_deref()));
    }
    // Eski hesaplar: kuruluş kabında değilse eklenir.
    crate::server_admin::ensure_instance_member(&state.db, &user.id).await?;
    let remember = body.get("rememberMe") != Some(&Value::Bool(false));
    issue_tokens(&state, &user, remember, client_version(&headers)).await.map(created)
}

pub async fn refresh(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = validate::body(&body)?;
    let mut c = Checker::new(&body);
    c.field("refreshToken", &[(c.str("refreshToken").is_some(), None)]);
    c.finish()?;
    let token = body["refreshToken"].as_str().unwrap_or_default();
    let expired = || ApiError::unauthorized("SESSION_EXPIRED", "Oturum süresi doldu.");

    // NestJS sürümü try/catch ile her hatayı SESSION_EXPIRED'a çeviriyor.
    let result: Result<Value, ()> = async {
        let claims: RefreshClaims = verify(token, &state.cfg.refresh_secret).ok_or(())?;
        let session: (String, NaiveDateTime) = sqlx::query_as(r#"select "userId", "expiresAt" from "Session" where "tokenHash" = $1"#)
            .bind(digest(token))
            .fetch_optional(&state.db)
            .await
            .map_err(|_| ())?
            .ok_or(())?;
        if session.0 != claims.sub || session.1 < now() {
            return Err(());
        }
        let user = user_by_id(&state, &session.0).await.map_err(|_| ())?;
        if crate::server_admin::is_banned(&user.id) {
            return Err(());
        }
        crate::server_admin::ensure_instance_member(&state.db, &user.id).await.map_err(|_| ())?;
        let access = access_token(&state, &user.id, &user.username).map_err(|_| ())?;
        if let Some(version) = client_version(&headers) {
            touch_client_version(&state.db, &user.id, &version).await.map_err(|_| ())?;
        }
        let user_json = with_server_permissions(&state, safe_user(&user)).await.map_err(|_| ())?;
        Ok(json!({ "user": user_json, "accessToken": access, "refreshToken": token }))
    }
    .await;
    result.map(created).map_err(|_| expired())
}

/// NestJS: `@Body() dto: Partial<RefreshDto>` — doğrulama yok.
pub async fn logout(State(state): State<AppState>, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = validate::body(&body)?;
    if let Some(token) = body.get("refreshToken").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        sqlx::query(r#"delete from "Session" where "tokenHash" = $1"#).bind(digest(token)).execute(&state.db).await?;
    }
    Ok(created(json!({ "success": true })))
}

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let claims = guard(&state, &headers)?;
    let u = user_by_id(&state, &claims.sub).await?;
    Ok(Json(json!({
        "id": u.id, "email": u.email, "username": u.username, "displayName": u.display_name, "status": u.status,
        "title": u.title, "avatarUrl": u.avatar_url, "twoFactorEnabled": u.two_factor_enabled,
        "serverPermissions": crate::server_admin::permissions_of(&state.db, &u.id).await?,
    })))
}

pub async fn two_factor_setup(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<(StatusCode, Json<Value>)> {
    let claims = guard(&state, &headers)?;
    let user = user_by_id(&state, &claims.sub).await?;
    if user.two_factor_enabled {
        return Err(ApiError::conflict("TWO_FACTOR_ALREADY_ON", "İki adımlı doğrulama zaten açık."));
    }
    let secret = totp::generate_secret();
    sqlx::query(r#"update "User" set "twoFactorSecret" = $2, "twoFactorEnabled" = false, "updatedAt" = $3 where id = $1"#)
        .bind(&user.id)
        .bind(&secret)
        .bind(now())
        .execute(&state.db)
        .await?;
    let otpauth = totp::otpauth_uri(&secret, &user.email);
    Ok(created(json!({ "secret": secret, "otpauth": otpauth })))
}

fn two_factor_code(body: &Bytes) -> ApiResult<String> {
    let body = validate::body(body)?;
    let mut c = Checker::new(&body);
    let code = c.str("code");
    c.field("code", &[(code.is_some_and(|v| js_len(v) == 6), None), (code.is_some(), None)]);
    c.finish()?;
    Ok(body["code"].as_str().unwrap_or_default().to_string())
}

pub async fn two_factor_enable(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let claims = guard(&state, &headers)?;
    let code = two_factor_code(&body)?;
    let user = user_by_id(&state, &claims.sub).await?;
    let Some(secret) = user.two_factor_secret.as_deref() else {
        return Err(ApiError::bad_request("TWO_FACTOR_NOT_STARTED", "Önce kurulumu başlat."));
    };
    if !totp::verify(secret, &code) {
        return Err(ApiError::bad_request("TWO_FACTOR_INVALID", "Doğrulama kodu geçersiz.").field("code").safe());
    }
    sqlx::query(r#"update "User" set "twoFactorEnabled" = true, "updatedAt" = $2 where id = $1"#).bind(&user.id).bind(now()).execute(&state.db).await?;
    Ok(created(json!({ "twoFactorEnabled": true })))
}

pub async fn two_factor_disable(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<(StatusCode, Json<Value>)> {
    let claims = guard(&state, &headers)?;
    let code = two_factor_code(&body)?;
    let user = user_by_id(&state, &claims.sub).await?;
    let (true, Some(secret)) = (user.two_factor_enabled, user.two_factor_secret.as_deref()) else {
        return Ok(created(json!({ "twoFactorEnabled": false })));
    };
    if !totp::verify(secret, &code) {
        return Err(ApiError::bad_request("TWO_FACTOR_INVALID", "Doğrulama kodu geçersiz.").field("code").safe());
    }
    sqlx::query(r#"update "User" set "twoFactorEnabled" = false, "twoFactorSecret" = null, "updatedAt" = $2 where id = $1"#)
        .bind(&user.id)
        .bind(now())
        .execute(&state.db)
        .await?;
    Ok(created(json!({ "twoFactorEnabled": false })))
}

pub async fn profile(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    let claims = guard(&state, &headers)?;
    let body = validate::body(&body)?;
    let mut c = Checker::new(&body);
    let opt_len = |name: &str, min: usize, max: usize| -> (bool, bool) {
        match body.get(name) {
            None | Some(Value::Null) => (true, true),
            Some(Value::String(s)) => ((min..=max).contains(&js_len(s)), true),
            Some(_) => (false, false),
        }
    };
    let (len_ok, str_ok) = opt_len("displayName", 2, 40);
    c.field("displayName", &[(len_ok, None), (str_ok, None)]);
    let (len_ok, str_ok) = opt_len("title", 0, 60);
    c.field("title", &[(len_ok, None), (str_ok, None)]);
    c.field("avatarUrl", &[(!c.present("avatarUrl") || c.str("avatarUrl").is_some(), None)]);
    c.finish()?;

    let text = |k: &str| body.get(k).and_then(Value::as_str);
    sqlx::query(
        r#"update "User" set
             "displayName" = coalesce($2, "displayName"),
             title = case when $3 then $4 else title end,
             "avatarUrl" = case when $5 then $6 else "avatarUrl" end,
             "updatedAt" = $7
           where id = $1"#,
    )
    .bind(&claims.sub)
    .bind(text("displayName").map(str::trim))
    .bind(text("title").is_some())
    .bind(text("title").map(str::trim).filter(|t| !t.is_empty()))
    .bind(text("avatarUrl").is_some())
    .bind(text("avatarUrl").filter(|t| !t.is_empty()))
    .bind(now())
    .execute(&state.db)
    .await?;
    let u = user_by_id(&state, &claims.sub).await?;
    Ok(Json(json!({
        "id": u.id, "email": u.email, "username": u.username, "displayName": u.display_name, "status": u.status,
        "title": u.title, "avatarUrl": u.avatar_url, "twoFactorEnabled": u.two_factor_enabled,
    })))
}

#[cfg(test)]
mod tests {
    // server/src/auth.spec.ts karşılığı (RegisterDto doğrulaması).
    use super::check_register;
    use serde_json::{Value, json};

    fn run(v: Value) -> Result<(), Value> {
        check_register(v.as_object().unwrap()).map_err(|e| e.body)
    }

    #[test]
    fn gecerli_kayit() {
        assert!(run(json!({ "email": "dev@p2.local", "username": "developer", "displayName": "Developer", "password": "Secure-pass1", "passwordConfirm": "Secure-pass1" })).is_ok());
        assert!(run(json!({ "email": "dev@p2.local", "username": "Hasan.Ozoglu", "displayName": "Hasan Özoğlu", "password": "Parola123", "passwordConfirm": "Parola123" })).is_ok());
    }

    #[test]
    fn bosluklu_ad_ve_zayif_parola() {
        let e = run(json!({ "email": "dev@p2.local", "username": "hasan ozoglu", "displayName": "Dev", "password": "sadecekucuk", "passwordConfirm": "sadecekucuk" })).unwrap_err();
        assert_eq!(e["code"], "VALIDATION_ERROR");
        // Kullanıcı adı ve parola hatalı; parolanın güvenli mesajı gösterilir.
        let e2 = run(json!({ "email": "dev@p2.local", "username": "hasanozoglu", "displayName": "Dev", "password": "sadecekucuk", "passwordConfirm": "sadecekucuk" })).unwrap_err();
        assert_eq!(e2["field"], "password");
        assert!(e2["message"].as_str().unwrap().contains("büyük harf"));
    }

    #[test]
    fn gecersiz_eposta_kisa_parola() {
        let e = run(json!({ "email": "wrong", "username": "ok_user", "displayName": "Dev", "password": "short", "passwordConfirm": "short" })).unwrap_err();
        assert_eq!(e["code"], "VALIDATION_ERROR");
        let e2 = run(json!({ "email": "wrong", "username": "ok_user", "displayName": "Dev", "password": "Parola123", "passwordConfirm": "Parola123" })).unwrap_err();
        assert_eq!(e2["field"], "email");
    }
}
