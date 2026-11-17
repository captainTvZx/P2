//! Keşfet: herkese açık projeler ve onların haberleri. Oturumu olan herkes listeyi görür ve açık bir
//! projeye davet kodu olmadan tek tıkla katılır (proje yasakları geçerlidir). Özel projeler burada
//! hiçbir uçta görünmez: listede yoktur, önizlemesi ve katılması `404 PROJECT_NOT_FOUND` döner.
//!
//! - `GET  /discover/projects?q=&category=&sort=popular|new|active` → `{ projects, categories, total, online }`
//! - `GET  /discover/projects/:id` → kart + sahip, kanal önizlemesi, üye örneği, son haberler, `banned`
//! - `POST /discover/projects/:id/join` → `{ project, organization, projectId, organizationId, joined }`
//! - `GET  /discover/news?limit=&projectId=` → açık projelerin son gönderileri

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::ctx::{Me, Query, Reply, created, many_json, not_found, ok, sql};
use crate::error::ApiResult;
use crate::projects::{iso, user4};

type J = sqlx::types::Json<Value>;

pub const VISIBILITIES: &[&str] = &["PUBLIC", "PRIVATE"];
/// Keşfet kategorileri (arayüzde: Oyun, Geliştirme, Topluluk, Eğitim, Müzik, Diğer).
pub const CATEGORIES: &[&str] = &["GAME", "DEV", "COMMUNITY", "EDUCATION", "MUSIC", "OTHER"];

/// `category` alanı: bilinen bir kategori ya da boş dizgi (kategoriyi kaldırır).
pub fn category_ok(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| s.is_empty() || CATEGORIES.contains(&s))
}

fn missing() -> crate::error::ApiError {
    not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")
}

/// Listelenebilir proje koşulu (alias `p`).
const PUBLIC: &str = r#"p.visibility = 'PUBLIC' and p."deletedAt" is null and p."organizationId" is not null"#;
const MEMBERS: &str = r#"(select count(*) from "ProjectMember" m where m."projectId" = p.id)"#;
/// Çevrimiçi üye: açık bağlantısı var (`$2`) ve görünmez değil.
const ONLINE: &str = r#"(select count(*) from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = p.id and m."userId" = any($2) and u.status <> 'OFFLINE')"#;

/// Keşfet kartı (alias `p`; `$1` ben, `$2` bağlı kullanıcı kimlikleri).
fn card_fields() -> String {
    format!(
        r#"'id', p.id, 'name', p.name, 'slug', p.slug, 'description', p.description, 'tagline', p.tagline, 'category', p.category,
           'iconUrl', p."iconUrl", 'coverUrl', p."coverUrl", 'organizationId', p."organizationId", 'visibility', p.visibility,
           'memberCount', {MEMBERS}, 'onlineCount', {ONLINE},
           'postCount', (select count(*) from "ProjectPost" x where x."projectId" = p.id),
           'joined', exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $1),
           'createdAt', {}"#,
        iso(r#"p."createdAt""#)
    )
}

/// ILIKE deseni: kullanıcının yazdığı `%`, `_` ve `\` düz karakter sayılır.
fn like(q: &str) -> String {
    let mut out = String::from("%");
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

fn connected(s: &AppState) -> Vec<String> {
    s.presence.lock().unwrap().connected()
}

async fn projects(State(s): State<AppState>, me: Me, q: Query) -> Reply {
    let text = q.get("q").map(|v| v.trim().to_string()).unwrap_or_default();
    let category = q.get("category").map(|v| v.trim().to_uppercase()).filter(|c| CATEGORIES.contains(&c.as_str())).unwrap_or_default();
    let order = match q.get("sort").map(String::as_str) {
        Some("new") => r#"p."createdAt" desc"#.to_string(),
        Some("active") => format!(r#"{ONLINE} desc, {MEMBERS} desc, p."createdAt" desc"#),
        _ => format!(r#"{MEMBERS} desc, p."createdAt" desc"#),
    };
    let online = connected(&s);
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({}) from "Project" p
               where {PUBLIC}
                 and ($3 = '' or p.name ilike $4 or coalesce(p.description, '') ilike $4 or coalesce(p.tagline, '') ilike $4)
                 and ($5 = '' or coalesce(p.category, 'OTHER') = $5)
               order by {order} limit 120"#,
            card_fields()
        )))
        .bind(&me.id)
        .bind(&online)
        .bind(&text)
        .bind(like(&text))
        .bind(&category),
        &s.db,
    )
    .await?;
    // Kategori sayıları aramadan bağımsızdır (çipler yerinde kalsın).
    let counts: Vec<(String, i64)> = sqlx::query_as(sql(format!(r#"select coalesce(p.category, 'OTHER'), count(*) from "Project" p where {PUBLIC} group by 1"#))).fetch_all(&s.db).await?;
    let mut categories = Map::new();
    let mut total = 0;
    for (name, n) in counts {
        total += n;
        categories.insert(name, json!(n));
    }
    // Açık projelerde şu an çevrimiçi olan tekil kişi sayısı (bir kişi birden çok projede olsa da bir kez).
    let people: i64 = sqlx::query_scalar(sql(format!(
        r#"select count(distinct m."userId") from "ProjectMember" m join "Project" p on p.id = m."projectId" join "User" u on u.id = m."userId"
           where {PUBLIC} and m."userId" = any($1) and u.status <> 'OFFLINE'"#
    )))
    .bind(&online)
    .fetch_one(&s.db)
    .await?;
    ok(json!({ "projects": rows, "categories": categories, "total": total, "online": people }))
}

// ------------------------------------------------------------------ haberler

/// Gönderi metnindeki dosya işaretleri (`[[p2-file|…]]`) ve Markdown başlık/vurgu imleri atılır.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[p2-") {
        out.push_str(&rest[..start]);
        match rest[start..].find("]]") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn clip(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Başlığı olmayan gönderide ilk dolu satır başlık, kalanı özet olur.
pub fn headline(title: Option<&str>, content: &str) -> (String, String) {
    let body = plain(content);
    let clean = |line: &str| line.trim().trim_start_matches('#').trim().trim_matches('*').trim().to_string();
    if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
        return (clip(title, 120), clip(&body, 240));
    }
    let mut lines = body.lines().map(clean).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    let rest = lines.collect::<Vec<_>>().join(" ");
    if rest.is_empty() && first.chars().count() > 90 {
        // Tek paragraf: başı başlık, tamamı özet.
        return (clip(&first, 72), clip(&first, 240));
    }
    (clip(&first, 120), clip(&rest, 240))
}

/// Haber satırı (alias `x` gönderi, `p` proje; `$1` ben).
fn news_json() -> String {
    format!(
        r#"json_build_object('id', x.id, 'projectId', x."projectId", 'kind', x.kind, 'title', x.title, 'content', x.content,
             'coverUrl', coalesce(x."coverUrl", p."coverUrl"), 'ownCover', x."coverUrl" is not null, 'createdAt', {}, 'author', {},
             'project', json_build_object('id', p.id, 'name', p.name, 'iconUrl', p."iconUrl", 'coverUrl', p."coverUrl", 'category', p.category,
               'joined', exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $1)))"#,
        iso(r#"x."createdAt""#),
        user4(r#"x."authorId""#)
    )
}

fn with_headline(mut row: Value) -> Value {
    let (title, excerpt) = headline(row["title"].as_str(), row["content"].as_str().unwrap_or_default());
    if let Some(obj) = row.as_object_mut() {
        obj.insert("title".into(), json!(title));
        obj.insert("excerpt".into(), json!(excerpt));
    }
    row
}

async fn news_rows(s: &AppState, me: &str, project: Option<&str>, limit: i64) -> ApiResult<Vec<Value>> {
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ProjectPost" x join "Project" p on p.id = x."projectId"
               where {PUBLIC} and ($2::text is null or x."projectId" = $2) order by x."createdAt" desc limit $3"#,
            news_json()
        )))
        .bind(me)
        .bind(project)
        .bind(limit),
        &s.db,
    )
    .await?;
    Ok(rows.into_iter().map(with_headline).collect())
}

async fn news(State(s): State<AppState>, me: Me, q: Query) -> Reply {
    let limit = q.get("limit").and_then(|l| l.parse::<i64>().ok()).unwrap_or(24).clamp(1, 60);
    let project = q.get("projectId").map(String::as_str).filter(|p| !p.is_empty());
    ok(Value::Array(news_rows(&s, &me.id, project, limit).await?))
}

// ------------------------------------------------------------------ önizleme ve katılma

async fn preview(State(s): State<AppState>, me: Me, Path(id): Path<String>) -> Reply {
    let online = connected(&s);
    let row: Option<J> = sqlx::query_scalar(sql(format!(
        r#"select json_build_object({card},
             'owner', {owner},
             'banned', exists (select 1 from "ProjectBan" b where b."projectId" = p.id and b."userId" = $1),
             'channels', coalesce((select json_agg(json_build_object('id', ch.id, 'name', ch.name, 'type', ch.type::text, 'category', c.name) order by c.position, ch.position)
                from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId"
                where c."projectId" = p.id and c."deletedAt" is null and ch."deletedAt" is null
                  and not exists (select 1 from "ChannelPermission" cp where cp."channelId" = ch.id and 'channel.view' = any(cp.deny))), '[]'::json),
             'members', coalesce((select json_agg(t.j) from (
                select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl",
                         'online', (u.id = any($2) and u.status <> 'OFFLINE'), 'owner', m.role = 'OWNER') j
                from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = p.id
                order by (m.role = 'OWNER') desc, (u.id = any($2) and u.status <> 'OFFLINE') desc, m."joinedAt" asc limit 14) t), '[]'::json))
           from "Project" p where p.id = $3 and {PUBLIC}"#,
        card = card_fields(),
        owner = user4(r#"p."ownerId""#),
    )))
    .bind(&me.id)
    .bind(&online)
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    let mut project = row.ok_or_else(missing)?.0;
    let posts = news_rows(&s, &me.id, Some(&id), 4).await?;
    if let Some(obj) = project.as_object_mut() {
        obj.insert("news".into(), Value::Array(posts));
    }
    ok(project)
}

/// Açık projeye kodsuz katıl. Özel proje yokmuş gibi davranır; yasaklıysa `403 PROJECT_BANNED`.
async fn join(State(s): State<AppState>, me: Me, Path(id): Path<String>) -> Reply {
    let org: Option<String> = sqlx::query_scalar(sql(format!(r#"select p."organizationId" from "Project" p where p.id = $1 and {PUBLIC}"#))).bind(&id).fetch_optional(&s.db).await?;
    let org = org.ok_or_else(missing)?;
    let joined = crate::community::add_member(&s, &org, &id, &me.id, None).await?;
    let project = crate::community::project_json(&s.db, &id).await?;
    if joined {
        crate::realtime::emit_users(&s, &[me.id.clone()], "project:created", &json!({ "project": project })).await;
    }
    created(json!({
        "project": project,
        "organization": crate::community::organization_json(&s.db, &org, &me.id).await?,
        "projectId": id,
        "organizationId": org,
        "joined": joined,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/discover/projects", get(projects))
        .route("/discover/projects/{projectId}", get(preview))
        .route("/discover/projects/{projectId}/join", post(join))
        .route("/discover/news", get(news))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kategori_dogrulama() {
        assert!(category_ok(Some(&json!("GAME"))));
        assert!(category_ok(Some(&json!(""))));
        assert!(!category_ok(Some(&json!("OYUN"))));
        assert!(!category_ok(Some(&json!(3))));
    }

    #[test]
    fn arama_deseni_joker_kacirir() {
        assert_eq!(like("a%b_c"), r"%a\%b\_c%");
        assert_eq!(like(""), "%%");
    }

    #[test]
    fn haber_basligi() {
        assert_eq!(headline(Some(" Sürüm 1.2 "), "Notlar\nburada"), ("Sürüm 1.2".to_string(), "Notlar burada".to_string()));
        assert_eq!(headline(None, "# Yeni sezon\n\nHarita **yenilendi**.\n[[p2-file|a.png|image%2Fpng|10|/uploads/x.png]]"), ("Yeni sezon".to_string(), "Harita **yenilendi**.".to_string()));
        let long = "a".repeat(200);
        let (title, excerpt) = headline(None, &long);
        assert_eq!(title.chars().count(), 72);
        assert_eq!(excerpt.chars().count(), 200);
        assert_eq!(headline(None, ""), (String::new(), String::new()));
    }
}
