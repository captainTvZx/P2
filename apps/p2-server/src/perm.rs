//! Yetki kataloğu ve proje bazlı hesaplama (docs/proje-bazli-yapi.md).

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use sqlx::PgPool;

use crate::ctx::{forbidden, not_found};
use crate::error::ApiResult;

pub struct Perm {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub channel: bool,
}

const fn p(id: &'static str, group: &'static str, label: &'static str, hint: &'static str, channel: bool) -> Perm {
    Perm { id, group, label, hint, channel }
}

pub const PERMISSIONS: &[Perm] = &[
    p("admin.panel", "Yönetim", "Yönetim panelini aç", "Genel bakış, üyeler ve kanal listesi", false),
    p("organization.manage", "Yönetim", "Projeyi düzenle", "Proje adını, görselini ve ayarlarını değiştir; geri dönüşümü yönet", false),
    p("member.roles", "Yönetim", "Rol ve yetki ata", "Rolleri, grup ve kişi yetkilerini düzenle", false),
    p("member.invite", "Üyeler", "Davet kodu yönet", "Projeye davet kodu oluştur ve iptal et", false),
    p("member.kick", "Üyeler", "Üye çıkar ve yasakla", "Projeden üye çıkar, yasakla ya da yasağı kaldır", false),
    p("usergroup.manage", "Üyeler", "Kullanıcı gruplarını yönet", "@devs gibi grupları oluştur ve düzenle", false),
    p("channel.view", "Kanal", "Kanalı görüntüle", "Kanalı listede gör ve mesajlarını oku", true),
    p("channel.manage", "Kanal", "Kanalları yönet", "Kanal ve kategori oluştur, düzenle, sil", true),
    p("channel.permissions", "Kanal", "Kanal izinlerini düzenle", "Bu kanala özel yetkileri değiştir", true),
    p("message.send", "Mesaj", "Mesaj gönder", "Kanala yazı yazabilir", true),
    p("message.attach", "Mesaj", "Dosya ekle", "Görsel, video ve dosya paylaşabilir", true),
    p("message.react", "Mesaj", "Tepki ver", "Mesajlara emoji tepkisi bırakabilir", true),
    p("message.forward", "Mesaj", "Mesaj ilet", "Mesajı başka bir kanala iletebilir", true),
    p("message.delete_any", "Mesaj", "Her mesajı sil", "Başkalarının mesajlarını da silebilir", true),
    p("message.delete_unlimited", "Mesaj", "Her şeyi süresiz sil", "Kanal, forum, thread, grup, özel mesaj ve görev yorumlarını süre sınırı olmadan silebilir", true),
    p("message.mention_all", "Mesaj", "@everyone / @here kullan", "Herkesi tek seferde etiketleyebilir", true),
    p("thread.reply", "Mesaj", "Thread\u{2019}e yanıt yaz", "Konu altında yanıt bırakabilir", true),
    p("call.start", "Sesli", "Arama başlat", "Kanalda ya da kişiyle arama başlatabilir", true),
    p("voice.connect", "Sesli", "Sesli odaya katıl", "Sesli kanala bağlanabilir", true),
    p("voice.speak", "Sesli", "Konuş", "Aramada mikrofonunu açabilir", true),
    p("voice.video", "Sesli", "Kamera aç", "Aramada kamerasını açabilir", true),
    p("screen.share", "Sesli", "Ekran paylaş", "Aramada ekranını yayınlayabilir", true),
    p("screen.annotate", "Sesli", "Yayına çizim yap", "Paylaşılan ekran üzerine kalemle çizebilir", true),
    p("task.create", "Görevler", "Görev oluştur", "Panoda yeni görev açabilir, kendi görevini düzenler", false),
    p("board.manage", "Görevler", "Panoyu yönet", "Sprint açıp kapatır, herkesin görevini düzenler ve siler", false),
    p("release.publish", "Kütüphane", "Sürüm yükle ve yayınla", "Build yükler, taslakları yayınlar ve siler", false),
    p("release.download", "Kütüphane", "Sürüm indir", "Yayınlanan build\u{2019}leri kütüphaneden indirip çalıştırır", false),
    p("direct.send", "Sohbet", "Bireysel mesaj gönder", "Kişilere özelden yazabilir", false),
    p("group.create", "Sohbet", "Grup sohbeti oluştur", "Yeni grup kurup kişi ekleyebilir", false),
];

pub const EVERYONE_DEFAULTS: &[&str] = &[
    "channel.view", "channel.manage", "message.send", "message.attach", "message.react", "message.forward", "thread.reply",
    "call.start", "voice.connect", "voice.speak", "voice.video", "screen.share", "screen.annotate", "direct.send", "group.create", "task.create", "release.download",
];

pub fn all() -> Vec<String> {
    PERMISSIONS.iter().map(|p| p.id.to_string()).collect()
}

pub fn catalog_json() -> Value {
    Value::Array(PERMISSIONS.iter().map(|p| json!({ "id": p.id, "group": p.group, "label": p.label, "hint": p.hint, "channel": p.channel })).collect())
}

fn known(id: &str) -> bool {
    PERMISSIONS.iter().any(|p| p.id == id)
}

fn is_channel(id: &str) -> bool {
    PERMISSIONS.iter().any(|p| p.id == id && p.channel)
}

/// `cleanPermissions`: diziye çevirir, bilinmeyenleri atar, sırayı koruyarak tekilleştirir.
pub fn clean(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in values {
        if known(v) && !out.contains(v) {
            out.push(v.clone());
        }
    }
    out
}

/// JSON gövdeden (`unknown`) gelen liste: `String(item)` ile dizgiye çevrilir.
pub fn clean_value(value: Option<&Value>) -> Vec<String> {
    let items: Vec<String> = match value {
        Some(Value::Array(list)) => list
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    };
    clean(&items)
}

pub fn clean_channel(values: &[String]) -> Vec<String> {
    clean(values).into_iter().filter(|v| is_channel(v)).collect()
}

pub fn label(id: &str) -> &str {
    PERMISSIONS.iter().find(|p| p.id == id).map(|p| p.label).unwrap_or(id)
}

pub fn refuse(permission: &str) -> crate::error::ApiError {
    forbidden("PERMISSION_DENIED", &format!("Bu işlem için yetkin yok: {}.", label(permission))).safe()
}

#[derive(Clone, Debug)]
pub struct RoleInfo {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub color: Option<String>,
    pub everyone: bool,
    pub founder: bool,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Effective {
    pub user_id: String,
    pub owner: bool,
    pub permissions: Vec<String>,
    pub roles: Vec<RoleInfo>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub role_ids: Vec<String>,
    pub group_ids: Vec<String>,
}

impl Effective {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
    pub fn roles_json(&self) -> Value {
        Value::Array(
            self.roles
                .iter()
                .map(|r| json!({ "id": r.id, "handle": r.handle, "name": r.name, "color": r.color, "everyone": r.everyone, "founder": r.founder, "permissions": r.permissions }))
                .collect(),
        )
    }
}

/// Sırayı koruyan küme birleşimi (JS `new Set([...])` ekleme sırası).
fn ordered_union<'a>(items: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in items {
        if !out.contains(i) {
            out.push(i.clone());
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Override {
    pub channel_id: String,
    pub target_type: String,
    pub target_id: String,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Kanal kuralları: @everyone rolü → diğer roller → gruplar → kişi. Her katmanda önce
/// kapatmalar, sonra açmalar.
pub fn apply_overrides(base: &[String], eff: &Effective, overrides: &[Override]) -> Vec<String> {
    let mut granted: Vec<String> = ordered_union(base.iter());
    if eff.owner {
        return granted;
    }
    let everyone_ids: Vec<&String> = eff.roles.iter().filter(|r| r.everyone).map(|r| &r.id).collect();
    let mut layer = |rows: Vec<&Override>| {
        let deny = ordered_union(rows.iter().flat_map(|r| clean_channel(&r.deny)).collect::<Vec<_>>().iter());
        let allow = ordered_union(rows.iter().flat_map(|r| clean_channel(&r.allow)).collect::<Vec<_>>().iter());
        granted.retain(|p| !deny.contains(p));
        for a in allow {
            if !granted.contains(&a) {
                granted.push(a);
            }
        }
    };
    layer(overrides.iter().filter(|o| o.target_type == "ROLE" && everyone_ids.contains(&&o.target_id)).collect());
    layer(overrides.iter().filter(|o| o.target_type == "ROLE" && !everyone_ids.contains(&&o.target_id) && eff.role_ids.contains(&o.target_id)).collect());
    layer(overrides.iter().filter(|o| o.target_type == "GROUP" && eff.group_ids.contains(&o.target_id)).collect());
    layer(overrides.iter().filter(|o| o.target_type == "USER" && o.target_id == eff.user_id).collect());
    granted
}


// ------------------------------------------------------------------ proje bazlı hesap
//
// Her proje kendi topluluğu (docs/proje-bazli-yapi.md): üyelik = `ProjectMember` satırı (proje
// silinmemiş). `OWNER` üye her yetkiyi taşır; diğerleri projenin rollerinden, gruplarından ve
// üyeliğe özel aç/kapat listelerinden alır. Kuruluş üyeliği hiçbir şey vermez.

fn project_missing() -> crate::error::ApiError {
    not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")
}

/// Kişi projenin üyesi mi (proje silinmemiş)?
pub async fn is_member(db: &PgPool, project_id: &str, user_id: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "ProjectMember" m join "Project" p on p.id = m."projectId" where m."projectId" = $1 and m."userId" = $2 and p."deletedAt" is null"#,
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(hit.is_some())
}

/// Projedeki etkin yetkiler. Üye değilse `404 PROJECT_NOT_FOUND`.
pub async fn effective_in(db: &PgPool, project_id: &str, user_id: &str) -> ApiResult<Effective> {
    let member: Option<(String, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select m.role::text, m.permissions, m."deniedPermissions" from "ProjectMember" m join "Project" p on p.id = m."projectId"
           where m."projectId" = $1 and m."userId" = $2 and p."deletedAt" is null"#,
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let Some((role, allow, denied)) = member else { return Err(project_missing()) };
    let owner = role == "OWNER";
    let rows: Vec<(String, String, String, Option<String>, bool, bool, Vec<String>)> = sqlx::query_as(
        r#"select r.id, r.handle, r.name, r.color, r.everyone, r.founder, r.permissions from "Role" r
           where r."projectId" = $1 and (r.everyone
             or exists (select 1 from "RoleMember" m where m."roleId" = r.id and m."userId" = $2)
             or exists (select 1 from "RoleGroup" g join "UserGroupMember" gm on gm."groupId" = g."groupId" where g."roleId" = r.id and gm."userId" = $2))
           order by r.founder desc, r.everyone desc, r.handle asc"#,
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let roles: Vec<RoleInfo> = rows
        .into_iter()
        .map(|(id, handle, name, color, everyone, founder, permissions)| RoleInfo { id, handle, name, color, everyone, founder, permissions: clean(&permissions) })
        .collect();
    let allow = clean(&allow);
    let deny = if owner { Vec::new() } else { clean(&denied) };
    let mut granted = if owner { all() } else { ordered_union(roles.iter().flat_map(|r| r.permissions.iter()).chain(allow.iter())) };
    granted.retain(|p| !deny.contains(p));
    let group_ids: Vec<String> = sqlx::query_scalar(
        r#"select gm."groupId" from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where gm."userId" = $1 and g."projectId" = $2"#,
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_all(db)
    .await?;
    Ok(Effective { user_id: user_id.to_string(), owner, permissions: granted, role_ids: roles.iter().map(|r| r.id.clone()).collect(), roles, allow, deny, group_ids })
}

/// Projede yetki ister (üye değilse 404, yetkisi yoksa 403).
pub async fn require_in(db: &PgPool, project_id: &str, user_id: &str, permission: &str) -> ApiResult<Effective> {
    let eff = effective_in(db, project_id, user_id).await?;
    if !eff.has(permission) {
        return Err(refuse(permission));
    }
    Ok(eff)
}

/// Kişinin bu kuruluşta üye olduğu (silinmemiş) projeler, en eski önce.
pub async fn member_projects(db: &PgPool, organization_id: &str, user_id: &str) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        r#"select p.id from "ProjectMember" m join "Project" p on p.id = m."projectId"
           where m."userId" = $1 and p."organizationId" = $2 and p."deletedAt" is null order by p."createdAt" asc, p.id asc"#,
    )
    .bind(user_id)
    .bind(organization_id)
    .fetch_all(db)
    .await?)
}

/// Proje dışı bağlam (DM, DM araması): kişinin üye olduğu projelerden en az birinde yetki.
pub async fn has_any(db: &PgPool, organization_id: &str, user_id: &str, permission: &str) -> ApiResult<bool> {
    for project in member_projects(db, organization_id, user_id).await? {
        if effective_in(db, &project, user_id).await?.has(permission) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// İki kişinin paylaştığı (silinmemiş) projeler.
pub async fn shared_projects(db: &PgPool, a: &str, b: &str) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        r#"select p.id from "Project" p where p."deletedAt" is null
             and exists (select 1 from "ProjectMember" x where x."projectId" = p.id and x."userId" = $1)
             and exists (select 1 from "ProjectMember" y where y."projectId" = p.id and y."userId" = $2)
           order by p."createdAt" asc"#,
    )
    .bind(a)
    .bind(b)
    .fetch_all(db)
    .await?)
}

/// DM / DM araması: hiçbiri diğerini engellememiş olmalı; iki kişi arkadaşsa serbest, değilse en az
/// bir projeyi paylaşmalı ve gönderen o projelerin birinde yetkiyi taşımalı (docs/arkadaslar.md).
pub async fn require_shared(db: &PgPool, user_id: &str, other_id: &str, permission: &str) -> ApiResult<()> {
    if crate::friends::blocked_between(db, user_id, other_id).await? {
        return Err(crate::friends::blocked_error());
    }
    if crate::friends::are_friends(db, user_id, other_id).await? {
        return Ok(());
    }
    let shared = shared_projects(db, user_id, other_id).await?;
    if shared.is_empty() {
        return Err(forbidden("NO_SHARED_PROJECT", "Bu kişiyle arkadaş değilsin ve ortak bir projen yok.").safe());
    }
    for project in &shared {
        if effective_in(db, project, user_id).await?.has(permission) {
            return Ok(());
        }
    }
    Err(refuse(permission))
}

/// Verilen kişilerden, kişiyle en az bir projeyi paylaşanlar ya da arkadaşı olanlar (kişinin kendisi dahil).
pub async fn peers_among(db: &PgPool, user_id: &str, candidates: &[String]) -> ApiResult<Vec<String>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<String> = sqlx::query_scalar(
        r#"select distinct o."userId" from "ProjectMember" me join "Project" p on p.id = me."projectId" and p."deletedAt" is null
           join "ProjectMember" o on o."projectId" = me."projectId" where me."userId" = $1 and o."userId" = any($2)"#,
    )
    .bind(user_id)
    .bind(candidates)
    .fetch_all(db)
    .await?;
    let friends = crate::friends::friends_among(db, user_id, candidates).await?;
    let mut out: Vec<String> = candidates.iter().filter(|c| rows.contains(c) || friends.contains(c)).cloned().collect();
    if candidates.iter().any(|c| c == user_id) && !out.iter().any(|c| c == user_id) {
        out.push(user_id.to_string());
    }
    Ok(out)
}

/// Kanalın projesi (kanal ve proje silinmemiş, kuruluş eşleşiyor).
pub async fn channel_project(db: &PgPool, organization_id: &str, channel_id: &str) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        r#"select c."projectId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and p."organizationId" = $2 and p."deletedAt" is null"#,
    )
    .bind(channel_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await?)
}

pub async fn can_view_project(db: &PgPool, organization_id: &str, user_id: &str, project_id: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "Project" p join "ProjectMember" m on m."projectId" = p.id and m."userId" = $3
           where p.id = $1 and p."organizationId" = $2 and p."deletedAt" is null"#,
    )
    .bind(project_id)
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(hit.is_some())
}

pub async fn require_project_access(db: &PgPool, organization_id: &str, user_id: &str, project_id: &str) -> ApiResult<()> {
    if !can_view_project(db, organization_id, user_id, project_id).await? {
        return Err(project_missing());
    }
    Ok(())
}

/// Verilen kişilerden projenin üyeleri.
pub async fn project_audience(db: &PgPool, organization_id: &str, project_id: &str, user_ids: &[String]) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let members: Vec<String> = sqlx::query_scalar(
        r#"select m."userId" from "ProjectMember" m join "Project" p on p.id = m."projectId"
           where m."projectId" = $1 and p."organizationId" = $2 and p."deletedAt" is null and m."userId" = any($3)"#,
    )
    .bind(project_id)
    .bind(organization_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    Ok(ordered_union(user_ids.iter().filter(|id| members.contains(id))))
}

async fn overrides_for(db: &PgPool, channel_ids: &[String], user_id: &str) -> ApiResult<Vec<Override>> {
    let rows: Vec<(String, String, String, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select "channelId", "targetType", "targetId", allow, deny from "ChannelPermission"
           where "channelId" = any($1) and ("targetType" = 'ROLE' or "targetType" = 'GROUP' or ("targetType" = 'USER' and "targetId" = $2))"#,
    )
    .bind(channel_ids)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(channel_id, target_type, target_id, allow, deny)| Override { channel_id, target_type, target_id, allow, deny }).collect())
}

/// Belirli kanaldaki etkin yetkiler (kanal yoksa ya da kişi projenin üyesi değilse boş).
pub async fn channel_permissions(db: &PgPool, organization_id: &str, user_id: &str, channel_id: &str) -> ApiResult<Vec<String>> {
    let Some(project) = channel_project(db, organization_id, channel_id).await? else { return Ok(Vec::new()) };
    if !is_member(db, &project, user_id).await? {
        return Ok(Vec::new());
    }
    let eff = effective_in(db, &project, user_id).await?;
    let rows = overrides_for(db, &[channel_id.to_string()], user_id).await?;
    Ok(apply_overrides(&eff.permissions, &eff, &rows))
}

pub async fn require_channel(db: &PgPool, organization_id: &str, user_id: &str, channel_id: &str, needed: &[&str]) -> ApiResult<Vec<String>> {
    let granted = channel_permissions(db, organization_id, user_id, channel_id).await?;
    for p in needed {
        if !granted.iter().any(|g| g == p) {
            return Err(refuse(p));
        }
    }
    Ok(granted)
}

/// Kanal başına etkin yetkiler: kişinin üye olduğu projelerdeki kanallar (her kanal kendi projesine göre).
pub async fn channel_grants(db: &PgPool, organization_id: &str, user_id: &str, channel_ids: &[String]) -> ApiResult<HashMap<String, Vec<String>>> {
    let mut out = HashMap::new();
    if channel_ids.is_empty() {
        return Ok(out);
    }
    let pairs: Vec<(String, String)> = sqlx::query_as(
        r#"select ch.id, c."projectId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = any($1) and ch."deletedAt" is null and p."organizationId" = $2 and p."deletedAt" is null
           and exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $3)"#,
    )
    .bind(channel_ids)
    .bind(organization_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    if pairs.is_empty() {
        return Ok(out);
    }
    let accessible: Vec<String> = pairs.iter().map(|(c, _)| c.clone()).collect();
    let rows = overrides_for(db, &accessible, user_id).await?;
    let mut by_channel: HashMap<&str, Vec<Override>> = HashMap::new();
    for r in &rows {
        by_channel.entry(r.channel_id.as_str()).or_default().push(r.clone());
    }
    let mut effs: HashMap<String, Effective> = HashMap::new();
    for (channel, project) in &pairs {
        if !effs.contains_key(project) {
            effs.insert(project.clone(), effective_in(db, project, user_id).await?);
        }
        let eff = &effs[project];
        out.insert(channel.clone(), apply_overrides(&eff.permissions, eff, by_channel.get(channel.as_str()).map(Vec::as_slice).unwrap_or(&[])));
    }
    Ok(out)
}

pub async fn visible_channel_ids(db: &PgPool, organization_id: &str, user_id: &str, channel_ids: &[String]) -> ApiResult<HashSet<String>> {
    let grants = channel_grants(db, organization_id, user_id, channel_ids).await?;
    Ok(grants.into_iter().filter(|(_, g)| g.iter().any(|p| p == "channel.view")).map(|(id, _)| id).collect())
}

/// Bir kanalı görebilen kişiler (verilen kişiler arasından): kanalın projesinin üyeleri, projenin
/// rolleri ve kanal kurallarıyla.
pub async fn channel_audience(db: &PgPool, organization_id: &str, channel_id: &str, user_ids: &[String]) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(project_id) = channel_project(db, organization_id, channel_id).await? else { return Ok(Vec::new()) };
    let members: Vec<(String, String, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select "userId", role::text, permissions, "deniedPermissions" from "ProjectMember" where "projectId" = $1 and "userId" = any($2)"#,
    )
    .bind(&project_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    // (rol, sahibi) çiftleri: doğrudan atanmış ya da grup üzerinden.
    let holders: Vec<(String, Vec<String>, String)> = sqlx::query_as(
        r#"select r.id, r.permissions, m."userId" from "Role" r join "RoleMember" m on m."roleId" = r.id
             where r."projectId" = $1 and not r.everyone and m."userId" = any($2)
           union
           select r.id, r.permissions, gm."userId" from "Role" r join "RoleGroup" g on g."roleId" = r.id join "UserGroupMember" gm on gm."groupId" = g."groupId"
             where r."projectId" = $1 and not r.everyone and gm."userId" = any($2)"#,
    )
    .bind(&project_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    let groups: Vec<(String, String)> = sqlx::query_as(
        r#"select gm."userId", gm."groupId" from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where gm."userId" = any($1) and g."projectId" = $2"#,
    )
    .bind(user_ids)
    .bind(&project_id)
    .fetch_all(db)
    .await?;
    let everyone: Vec<(String, Vec<String>)> = sqlx::query_as(r#"select id, permissions from "Role" where "projectId" = $1 and everyone"#)
        .bind(&project_id)
        .fetch_all(db)
        .await?;
    let overrides: Vec<Override> = sqlx::query_as::<_, (String, String, String, Vec<String>, Vec<String>)>(
        r#"select "channelId", "targetType", "targetId", allow, deny from "ChannelPermission" where "channelId" = $1"#,
    )
    .bind(channel_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .map(|(channel_id, target_type, target_id, allow, deny)| Override { channel_id, target_type, target_id, allow, deny })
    .collect();
    let base: Vec<RoleInfo> = everyone
        .into_iter()
        .map(|(id, permissions)| RoleInfo { id, handle: String::new(), name: String::new(), color: None, everyone: true, founder: false, permissions: clean(&permissions) })
        .collect();
    let mut out = Vec::new();
    for (user_id, role, allow, denied) in members {
        let owner = role == "OWNER";
        let mut roles = base.clone();
        for (rid, perms, holder) in &holders {
            if holder == &user_id && !roles.iter().any(|r| &r.id == rid) {
                roles.push(RoleInfo { id: rid.clone(), handle: String::new(), name: String::new(), color: None, everyone: false, founder: false, permissions: clean(perms) });
            }
        }
        let mut granted = if owner { all() } else { ordered_union(roles.iter().flat_map(|r| r.permissions.iter()).chain(clean(&allow).iter()).collect::<Vec<_>>()) };
        if !owner {
            let deny = clean(&denied);
            granted.retain(|p| !deny.contains(p));
        }
        let eff = Effective {
            user_id: user_id.clone(),
            owner,
            permissions: granted,
            role_ids: roles.iter().map(|r| r.id.clone()).collect(),
            roles,
            allow: Vec::new(),
            deny: Vec::new(),
            group_ids: groups.iter().filter(|(u, _)| u == &user_id).map(|(_, g)| g.clone()).collect(),
        };
        if apply_overrides(&eff.permissions, &eff, &overrides).iter().any(|g| g == "channel.view") {
            out.push(user_id);
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ proje tohumu

/// Yeni projenin temel rolleri (herkes + kurucu) ve kurucu rolünün sahibi. Eksik olanı ekler.
pub async fn seed_project_roles(tx: &mut sqlx::PgConnection, organization_id: &str, project_id: &str, owner_id: &str, at: chrono::NaiveDateTime) -> ApiResult<()> {
    let insert_role = r#"insert into "Role" (id, "organizationId", "projectId", handle, name, description, color, everyone, founder, permissions, "createdById", "createdAt", "updatedAt")
                         select $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12
                         where not exists (select 1 from "Role" where "projectId" = $3 and (handle = $4 or (everyone and $8) or (founder and $9)))"#;
    let everyone: Vec<String> = EVERYONE_DEFAULTS.iter().map(|p| p.to_string()).collect();
    sqlx::query(insert_role)
        .bind(crate::ctx::uuid())
        .bind(organization_id)
        .bind(project_id)
        .bind("everyone")
        .bind("Herkes")
        .bind("Projedeki herkes bu rolün yetkilerini alır.")
        .bind("#79818c")
        .bind(true)
        .bind(false)
        .bind(&everyone)
        .bind(owner_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    sqlx::query(insert_role)
        .bind(crate::ctx::uuid())
        .bind(organization_id)
        .bind(project_id)
        .bind("kurucu")
        .bind("Kurucu")
        .bind("Projeyi kuran kişi: tüm yetkiler her zaman açıktır.")
        .bind("#faa61a")
        .bind(false)
        .bind(true)
        .bind(all())
        .bind(owner_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"insert into "RoleMember" ("roleId", "userId", "addedAt") select id, $2, $3 from "Role" where "projectId" = $1 and founder on conflict do nothing"#,
    )
    .bind(project_id)
    .bind(owner_id)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eff(owner: bool, perms: &[&str], roles: &[(&str, bool)], groups: &[&str]) -> Effective {
        Effective {
            user_id: "u".into(),
            owner,
            permissions: perms.iter().map(|p| p.to_string()).collect(),
            roles: roles
                .iter()
                .map(|(id, everyone)| RoleInfo { id: id.to_string(), handle: String::new(), name: String::new(), color: None, everyone: *everyone, founder: false, permissions: vec![] })
                .collect(),
            allow: vec![],
            deny: vec![],
            role_ids: roles.iter().map(|(id, _)| id.to_string()).collect(),
            group_ids: groups.iter().map(|g| g.to_string()).collect(),
        }
    }

    fn ov(t: &str, id: &str, allow: &[&str], deny: &[&str]) -> Override {
        Override {
            channel_id: "c".into(),
            target_type: t.into(),
            target_id: id.into(),
            allow: allow.iter().map(|p| p.to_string()).collect(),
            deny: deny.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[test]
    fn kanal_katmanlari() {
        let e = eff(false, &["channel.view", "message.send"], &[("ev", true), ("mod", false)], &["g1"]);
        // everyone kapatır, rol açar → açık.
        let out = apply_overrides(&e.permissions, &e, &[ov("ROLE", "ev", &[], &["channel.view"]), ov("ROLE", "mod", &["channel.view"], &[])]);
        assert!(out.contains(&"channel.view".to_string()));
        // kişi kuralı en son: kapatır.
        let out = apply_overrides(&e.permissions, &e, &[ov("GROUP", "g1", &["message.attach"], &[]), ov("USER", "u", &[], &["message.attach"])]);
        assert!(!out.contains(&"message.attach".to_string()));
        // başka projenin rol kimliği etkisiz.
        let out = apply_overrides(&e.permissions, &e, &[ov("ROLE", "baska", &[], &["message.send"])]);
        assert!(out.contains(&"message.send".to_string()));
    }

    #[test]
    fn sahip_kurallardan_etkilenmez() {
        let e = eff(true, &["channel.view"], &[("ev", true)], &[]);
        let out = apply_overrides(&e.permissions, &e, &[ov("ROLE", "ev", &[], &["channel.view"])]);
        assert!(out.contains(&"channel.view".to_string()));
    }

    #[test]
    fn katalog() {
        assert!(!all().contains(&"project.create".to_string()), "proje açmak artık yetki istemez");
        assert!(clean(&["project.create".into(), "member.kick".into(), "member.kick".into()]) == vec!["member.kick".to_string()]);
        for p in EVERYONE_DEFAULTS {
            assert!(known(p), "{p}");
        }
    }
}
