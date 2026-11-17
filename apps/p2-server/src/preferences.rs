//! Kanal ve kişi başına yıldız / bildirim / gizleme tercihleri — `server/src/preferences.ts` karşılığı.
//!
//! Tercihler kapsamlıdır (`scope`): kanal tercihlerinde boş (kanal zaten bir projeye aittir); kişi
//! tercihlerinde `home` (Ana sayfa) ya da proje kimliği. Ana sayfada yıldızlanan kişi projede
//! yıldızlı görünmez, bir projede yıldızlanan başka projede görünmez (docs/arkadaslar.md).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;
use serde_json::Value;

use crate::AppState;
use crate::activity::membership;
use crate::ctx::{Body, Me, Reply, many_json, now, ok};
use crate::error::ApiError;
use crate::validate::Checker;

const PREFERENCE_JSON: &str = r#"json_build_object('kind', kind, 'targetId', "targetId", 'scope', scope, 'starred', starred, 'notify', notify, 'hidden', hidden)"#;

async fn list(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    membership(&s.db, &organization_id, &me.id).await?;
    let rows = many_json(
        sqlx::query_scalar(crate::ctx::sql(format!(r#"select {PREFERENCE_JSON} from "UserPreference" where "userId" = $1 and "organizationId" = $2"#)))
            .bind(&me.id)
            .bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn save(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let kind = c.str("kind");
    c.field("kind", &[(kind.is_some_and(|k| k == "CHANNEL" || k == "PERSON"), None), (kind.is_some(), None)]);
    c.field("targetId", &[(c.str("targetId").is_some(), None)]);
    let bool_ok = |name: &str| !c.present(name) || body[name].is_boolean();
    let (starred_ok, hidden_ok) = (bool_ok("starred"), bool_ok("hidden"));
    c.field("starred", &[(starred_ok, None)]);
    let notify = c.str("notify");
    let notify_present = c.present("notify");
    c.field(
        "notify",
        &[(!notify_present || notify.is_some_and(|n| ["ALL", "MENTIONS", "MUTED"].contains(&n)), None), (!notify_present || notify.is_some(), None)],
    );
    c.field("hidden", &[(hidden_ok, None)]);
    let scope_given = c.str("scope");
    c.field("scope", &[(!c.present("scope") || scope_given.is_some_and(|v| v.len() <= 64), None)]);
    c.finish()?;
    membership(&s.db, &organization_id, &me.id).await?;
    // Kapsam: kanalda her zaman boş; kişide `home` ya da üyesi olduğum bir proje (verilmezse Ana sayfa).
    let scope = match kind {
        Some("PERSON") => {
            let scope = scope_given.filter(|v| !v.is_empty()).unwrap_or("home");
            if scope != "home" && !crate::perm::is_member(&s.db, scope, &me.id).await? {
                return Err(crate::ctx::not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
            }
            scope.to_string()
        }
        _ => String::new(),
    };
    // @IsOptional null'u geçirir; Prisma upsert'in `update` kısmında null alan reddedilir (500).
    if ["starred", "notify", "hidden"].iter().any(|k| matches!(body.get(*k), Some(Value::Null))) {
        return Err(ApiError::internal("UserPreference: null alan"));
    }
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(crate::ctx::sql(format!(
        r#"insert into "UserPreference" ("userId", "organizationId", kind, "targetId", starred, notify, hidden, "updatedAt", scope)
           values ($1, $2, $3, $4, coalesce($5, false), coalesce($6, 'MENTIONS'), coalesce($7, false), $8, $9)
           on conflict ("userId", kind, "targetId", scope) do update set
             starred = coalesce($5, "UserPreference".starred), notify = coalesce($6, "UserPreference".notify),
             hidden = coalesce($7, "UserPreference".hidden), "updatedAt" = $8
           returning {PREFERENCE_JSON}"#
    )))
    .bind(&me.id)
    .bind(&organization_id)
    .bind(kind)
    .bind(c_str(&body, "targetId"))
    .bind(body.get("starred").and_then(Value::as_bool))
    .bind(notify)
    .bind(body.get("hidden").and_then(Value::as_bool))
    .bind(now())
    .bind(&scope)
    .fetch_one(&s.db)
    .await?;
    ok(row.0)
}

fn c_str<'a>(body: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    body.get(key).and_then(Value::as_str)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/organizations/{organizationId}/preferences", get(list).put(save))
}
