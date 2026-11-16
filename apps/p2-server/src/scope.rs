//! Proje kapsamı çıkarıcısı: proje yönetim uçları hem `/projects/{projectId}/...` hem de eski
//! `/organizations/{organizationId}/...` adreslerinden çağrılır (docs/proje-bazli-yapi.md, "Uyumluluk").
//!
//! Proje seçimi: adreste `projectId` → `?projectId=` → `X-P2-Project` başlığı → kişinin o kuruluşta
//! üye olduğu en eski proje (yalnız eski istemciler için; gereken yetkinin bulunduğu ilk proje seçilir).

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;

use crate::AppState;
use crate::ctx::{Me, not_found};
use crate::error::{ApiError, ApiResult};
use crate::perm;

pub struct Scope {
    pub user: String,
    pub org: String,
    pub project: String,
    params: HashMap<String, String>,
    /// Proje adresten/sorgudan gelmediyse eski istemci için adaylar (en eski önce).
    fallback: Vec<String>,
}

fn project_missing() -> ApiError {
    not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")
}

impl Scope {
    /// Adresteki diğer parametre (`roleId`, `userId`, ...).
    pub fn p(&self, name: &str) -> String {
        self.params.get(name).cloned().unwrap_or_default()
    }

    /// Yetki ister. Proje varsayılan seçildiyse yetkinin bulunduğu ilk projeye geçer.
    pub async fn require(&mut self, s: &AppState, permission: &str) -> ApiResult<perm::Effective> {
        if self.fallback.len() > 1 {
            for candidate in &self.fallback {
                let eff = perm::effective_in(&s.db, candidate, &self.user).await?;
                if eff.has(permission) {
                    self.project = candidate.clone();
                    return Ok(eff);
                }
            }
        }
        perm::require_in(&s.db, &self.project, &self.user, permission).await
    }

    pub async fn effective(&self, s: &AppState) -> ApiResult<perm::Effective> {
        perm::effective_in(&s.db, &self.project, &self.user).await
    }
}

pub async fn instance_owner(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<bool> {
    let role: Option<String> = sqlx::query_scalar(r#"select role::text from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(role.as_deref() == Some("OWNER"))
}

/// Projenin kuruluşu (silinmemiş proje).
pub async fn project_org(db: &sqlx::PgPool, project_id: &str) -> ApiResult<Option<String>> {
    let row: Option<Option<String>> = sqlx::query_scalar(r#"select "organizationId" from "Project" where id = $1 and "deletedAt" is null"#)
        .bind(project_id)
        .fetch_optional(db)
        .await?;
    Ok(row.flatten())
}

pub async fn resolve(s: &AppState, user: &str, params: HashMap<String, String>, wanted: Option<String>) -> ApiResult<Scope> {
    if let Some(project) = params.get("projectId").cloned() {
        let org = project_org(&s.db, &project).await?.ok_or_else(project_missing)?;
        if params.get("organizationId").is_some_and(|o| *o != org) || !perm::is_member(&s.db, &project, user).await? {
            return Err(project_missing());
        }
        return Ok(Scope { user: user.to_string(), org, project, params, fallback: Vec::new() });
    }
    let org = params.get("organizationId").cloned().unwrap_or_default();
    crate::activity::membership(&s.db, &org, user).await?;
    if let Some(project) = wanted.filter(|p| !p.is_empty()) {
        if !perm::can_view_project(&s.db, &org, user, &project).await? {
            return Err(project_missing());
        }
        return Ok(Scope { user: user.to_string(), org, project, params, fallback: Vec::new() });
    }
    let fallback = perm::member_projects(&s.db, &org, user).await?;
    let project = fallback.first().cloned().ok_or_else(project_missing)?;
    Ok(Scope { user: user.to_string(), org, project, params, fallback })
}

impl FromRequestParts<AppState> for Scope {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let me = Me::from_request_parts(parts, state).await?;
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state).await.map_err(|_| project_missing())?;
        let from_query = axum::extract::Query::<HashMap<String, String>>::from_request_parts(parts, state).await.ok().and_then(|q| q.0.get("projectId").cloned());
        let from_header = parts.headers.get("x-p2-project").and_then(|v| v.to_str().ok()).map(str::to_string);
        resolve(state, &me.id, params, from_query.or(from_header)).await
    }
}
