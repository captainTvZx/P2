//! Etkinlik akışı — `server/src/activity.ts` karşılığı (etiket/yanıt/tepki/sürüm/atama).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Me, Query, Reply, created, not_found, ok, uuid};
use crate::error::ApiResult;

/// `include: { actor: { select: { id, username, displayName } } }` ile Activity satırı.
pub const ACTIVITY_JSON: &str = concat!(
    r#"json_build_object('id', a.id, 'organizationId', a."organizationId", 'userId', a."userId", 'actorId', a."actorId", 'kind', a.kind,
    'projectId', a."projectId", 'channelId', a."channelId", 'channelName', a."channelName", 'groupId', a."groupId", 'groupName', a."groupName",
    'boardId', a."boardId", 'taskId', a."taskId", 'documentId', a."documentId", 'messageId', a."messageId", 'parentId', a."parentId",
    'emoji', a.emoji, 'preview', a.preview, 'readAt', "#,
    crate::iso!(r#"a."readAt""#),
    ", 'createdAt', ",
    crate::iso!(r#"a."createdAt""#),
    r#", 'actor', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName") from "User" u where u.id = a."actorId"))"#
);

#[derive(Default, Clone)]
pub struct ActivityInput {
    pub organization_id: String,
    pub user_id: String,
    pub actor_id: String,
    pub kind: &'static str,
    pub preview: String,
    pub project_id: Option<String>,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub board_id: Option<String>,
    pub task_id: Option<String>,
    pub document_id: Option<String>,
    pub message_id: Option<String>,
    pub parent_id: Option<String>,
    pub emoji: Option<String>,
}

/// Metindeki @kullanıcı adları (küçük harfe, tr-TR).
pub fn mentioned_usernames(content: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"@([a-zA-Z0-9_.-]+)").unwrap());
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(content) {
        // ASCII dışı karakter yok; tr-TR küçültmesi yalnız 'I' → 'ı' farkı yaratır.
        let name: String = c[1].chars().map(|ch| if ch == 'I' { 'ı' } else { ch.to_ascii_lowercase() }).collect();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Etiketlenen kişiler: @kullanıcı adları + @grup üyeleri.
pub async fn mentioned_user_ids(db: &sqlx::PgPool, organization_id: &str, project_id: &str, content: &str) -> ApiResult<Vec<String>> {
    let names = mentioned_usernames(content);
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let lowered: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let members: Vec<String> = sqlx::query_scalar(
        r#"select m."userId" from "ProjectMember" m join "User" u on u.id = m."userId" join "Project" p on p.id = m."projectId"
           where m."projectId" = $3 and p."organizationId" = $1 and lower(u.username) = any($2)"#,
    )
    .bind(organization_id)
    .bind(&lowered)
    .bind(project_id)
    .fetch_all(db)
    .await?;
    let groups: Vec<String> = sqlx::query_scalar(
        r#"select gm."userId" from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where g."organizationId" = $1 and g."projectId" = $3 and g.handle = any($2)"#,
    )
    .bind(organization_id)
    .bind(&names)
    .bind(project_id)
    .fetch_all(db)
    .await?;
    let mut out: Vec<String> = Vec::new();
    for id in members.into_iter().chain(groups) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

pub fn preview_of(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::validate::js_slice(&collapsed, 140)
}

/// Etkinlikleri yazar ve sahibine canlı bildirir; kişinin kendi eylemi kaydedilmez. Hata yutulur.
pub async fn record(state: &AppState, inputs: Vec<ActivityInput>) {
    for row in inputs.into_iter().filter(|r| !r.user_id.is_empty() && r.user_id != r.actor_id) {
        let id = uuid();
        let q = format!(
            r#"with a as (insert into "Activity" (id, "organizationId", "userId", "actorId", kind, preview, "projectId", "channelId", "channelName", "groupId", "groupName", "boardId", "taskId", "documentId", "messageId", "parentId", emoji, "createdAt")
               values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) returning *) select {ACTIVITY_JSON} from a"#
        );
        let created: Result<sqlx::types::Json<Value>, _> = sqlx::query_scalar(crate::ctx::sql(q))
            .bind(&id)
            .bind(&row.organization_id)
            .bind(&row.user_id)
            .bind(&row.actor_id)
            .bind(row.kind)
            .bind(preview_of(&row.preview))
            .bind(&row.project_id)
            .bind(&row.channel_id)
            .bind(&row.channel_name)
            .bind(&row.group_id)
            .bind(&row.group_name)
            .bind(&row.board_id)
            .bind(&row.task_id)
            .bind(&row.document_id)
            .bind(&row.message_id)
            .bind(&row.parent_id)
            .bind(&row.emoji)
            .bind(crate::ctx::now())
            .fetch_one(&state.db)
            .await;
        match created {
            Ok(activity) => crate::realtime::emit_users(state, &[row.user_id.clone()], "activity:new", &json!({ "activity": activity.0 })).await,
            Err(_) => return,
        }
    }
}

/// Etkinlik yalnız hâlâ üye olunan projelerden (projesiz kayıtlar: DM, grup dışı) görünür.
const VISIBLE: &str = r#"(a."projectId" is null or exists (select 1 from "ProjectMember" pm join "Project" p on p.id = pm."projectId" where pm."projectId" = a."projectId" and pm."userId" = a."userId" and p."deletedAt" is null))"#;

pub async fn membership(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<String> {
    let role: Option<String> = sqlx::query_scalar(r#"select role::text from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    role.ok_or_else(|| not_found("ORGANIZATION_NOT_FOUND", "Kuruluş bulunamadı."))
}

async fn list(State(s): State<AppState>, me: Me, Path(org): Path<String>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let take = q.get("limit").and_then(|l| crate::validate::js_number(l)).filter(|n| *n != 0.0).unwrap_or(60.0).clamp(1.0, 200.0) as i64;
    let items: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(crate::ctx::sql(format!(
        r#"select {ACTIVITY_JSON} from "Activity" a where a."organizationId" = $1 and a."userId" = $2 and {VISIBLE} order by a."createdAt" desc limit $3"#
    )))
    .bind(&org)
    .bind(&me.id)
    .bind(take)
    .fetch_all(&s.db)
    .await?;
    let unread: i64 = sqlx::query_scalar(crate::ctx::sql(format!(r#"select count(*) from "Activity" a where a."organizationId" = $1 and a."userId" = $2 and a."readAt" is null and {VISIBLE}"#)))
        .bind(&org)
        .bind(&me.id)
        .fetch_one(&s.db)
        .await?;
    ok(json!({ "items": items.into_iter().map(|j| j.0).collect::<Vec<_>>(), "unread": unread }))
}

async fn read_all(State(s): State<AppState>, me: Me, Path(org): Path<String>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    sqlx::query(r#"update "Activity" set "readAt" = $3 where "organizationId" = $1 and "userId" = $2 and "readAt" is null"#)
        .bind(&org)
        .bind(&me.id)
        .bind(crate::ctx::now())
        .execute(&s.db)
        .await?;
    created(json!({ "ok": true }))
}

async fn read_one(State(s): State<AppState>, me: Me, Path((org, activity)): Path<(String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    sqlx::query(r#"update "Activity" set "readAt" = $3 where id = $1 and "userId" = $2"#)
        .bind(&activity)
        .bind(&me.id)
        .bind(crate::ctx::now())
        .execute(&s.db)
        .await?;
    created(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organizations/{organizationId}/activity", get(list))
        .route("/organizations/{organizationId}/activity/read", post(read_all))
        .route("/organizations/{organizationId}/activity/{activityId}/read", post(read_one))
}
