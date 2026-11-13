//! Yetki kataloğu ve hesaplama — `server/src/permissions.ts` birebir karşılığı.

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
    p("organization.manage", "Yönetim", "Kuruluşu düzenle", "Kuruluş adını değiştir", false),
    p("member.roles", "Yönetim", "Rol ve yetki ata", "Rolleri, grup ve kişi yetkilerini düzenle", false),
    p("member.invite", "Üyeler", "Davet kodu yönet", "Yeni davet kodu oluştur ve iptal et", false),
    p("member.kick", "Üyeler", "Üye çıkar", "Kuruluştan üye çıkar, oturumlarını kapat", false),
    p("usergroup.manage", "Üyeler", "Kullanıcı gruplarını yönet", "@devs gibi grupları oluştur ve düzenle", false),
    p("project.create", "Proje", "Proje oluştur", "Kuruluşta yeni proje açabilir", false),
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

pub async fn effective(db: &PgPool, organization_id: &str, user_id: &str) -> ApiResult<Effective> {
    let member: Option<(String, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select role::text, permissions, "deniedPermissions" from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#,
    )
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let Some((role, allow, denied)) = member else {
        return Err(not_found("ORGANIZATION_NOT_FOUND", "Kuruluş bulunamadı."));
    };
    let owner = role == "OWNER";
    let rows: Vec<(String, String, String, Option<String>, bool, bool, Vec<String>)> = sqlx::query_as(
        r#"select r.id, r.handle, r.name, r.color, r.everyone, r.founder, r.permissions from "Role" r
           where r."organizationId" = $1 and (r.everyone
             or exists (select 1 from "RoleMember" m where m."roleId" = r.id and m."userId" = $2)
             or exists (select 1 from "RoleGroup" g join "UserGroupMember" gm on gm."groupId" = g."groupId" where g."roleId" = r.id and gm."userId" = $2))
           order by r.founder desc, r.everyone desc, r.handle asc"#,
    )
    .bind(organization_id)
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
        r#"select gm."groupId" from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where gm."userId" = $1 and g."organizationId" = $2"#,
    )
    .bind(user_id)
    .bind(organization_id)
    .fetch_all(db)
    .await?;
    Ok(Effective { user_id: user_id.to_string(), owner, permissions: granted, role_ids: roles.iter().map(|r| r.id.clone()).collect(), roles, allow, deny, group_ids })
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

pub async fn can_view_project(db: &PgPool, organization_id: &str, user_id: &str, project_id: &str) -> ApiResult<bool> {
    let eff = effective(db, organization_id, user_id).await?;
    if eff.owner {
        return Ok(true);
    }
    project_visible(db, organization_id, user_id, project_id).await
}

async fn project_visible(db: &PgPool, organization_id: &str, user_id: &str, project_id: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "Project" p where p.id = $1 and p."organizationId" = $2 and p."deletedAt" is null
           and (p."ownerId" = $3 or exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $3))"#,
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
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    Ok(())
}

pub async fn project_audience(db: &PgPool, organization_id: &str, project_id: &str, user_ids: &[String]) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let owner: Option<String> = sqlx::query_scalar(r#"select "ownerId" from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(project_id)
        .bind(organization_id)
        .fetch_optional(db)
        .await?;
    let Some(owner) = owner else { return Ok(Vec::new()) };
    let members: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ProjectMember" where "projectId" = $1 and "userId" = any($2)"#)
        .bind(project_id)
        .bind(user_ids)
        .fetch_all(db)
        .await?;
    let owners: Vec<String> = sqlx::query_scalar(r#"select "userId" from "OrganizationMember" where "organizationId" = $1 and "userId" = any($2) and role = 'OWNER'"#)
        .bind(organization_id)
        .bind(user_ids)
        .fetch_all(db)
        .await?;
    let all = ordered_union(std::iter::once(&owner).chain(members.iter()).chain(owners.iter()));
    Ok(all.into_iter().filter(|id| user_ids.contains(id)).collect())
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

/// Belirli kanaldaki etkin yetkiler (kanal yoksa ya da proje görünmüyorsa boş).
pub async fn channel_permissions(db: &PgPool, organization_id: &str, user_id: &str, channel_id: &str) -> ApiResult<(Effective, Vec<String>)> {
    let eff = effective(db, organization_id, user_id).await?;
    let project: Option<String> = sqlx::query_scalar(
        r#"select c."projectId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and p."organizationId" = $2 and p."deletedAt" is null"#,
    )
    .bind(channel_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await?;
    let Some(project) = project else { return Ok((eff, Vec::new())) };
    if !eff.owner && !project_visible(db, organization_id, user_id, &project).await? {
        return Ok((eff, Vec::new()));
    }
    let rows = overrides_for(db, &[channel_id.to_string()], user_id).await?;
    let granted = apply_overrides(&eff.permissions, &eff, &rows);
    Ok((eff, granted))
}

pub async fn has_permission(db: &PgPool, organization_id: &str, user_id: &str, permission: &str) -> ApiResult<bool> {
    Ok(effective(db, organization_id, user_id).await?.has(permission))
}

pub async fn require(db: &PgPool, organization_id: &str, user_id: &str, permission: &str) -> ApiResult<Effective> {
    let eff = effective(db, organization_id, user_id).await?;
    if !eff.has(permission) {
        return Err(refuse(permission));
    }
    Ok(eff)
}

pub async fn require_channel(db: &PgPool, organization_id: &str, user_id: &str, channel_id: &str, needed: &[&str]) -> ApiResult<Vec<String>> {
    let (_, granted) = channel_permissions(db, organization_id, user_id, channel_id).await?;
    for p in needed {
        if !granted.iter().any(|g| g == p) {
            return Err(refuse(p));
        }
    }
    Ok(granted)
}

pub async fn visible_channel_ids(db: &PgPool, organization_id: &str, user_id: &str, channel_ids: &[String]) -> ApiResult<HashSet<String>> {
    if channel_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let eff = effective(db, organization_id, user_id).await?;
    if eff.owner {
        return Ok(channel_ids.iter().cloned().collect());
    }
    let accessible: Vec<String> = sqlx::query_scalar(
        r#"select ch.id from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = any($1) and ch."deletedAt" is null and p."organizationId" = $2 and p."deletedAt" is null
           and (p."ownerId" = $3 or exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $3))"#,
    )
    .bind(channel_ids)
    .bind(organization_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let rows = overrides_for(db, &accessible, user_id).await?;
    let mut by_channel: HashMap<&str, Vec<Override>> = HashMap::new();
    for r in &rows {
        by_channel.entry(r.channel_id.as_str()).or_default().push(r.clone());
    }
    let mut visible = HashSet::new();
    for id in channel_ids {
        if !accessible.contains(id) {
            continue;
        }
        let granted = apply_overrides(&eff.permissions, &eff, by_channel.get(id.as_str()).map(Vec::as_slice).unwrap_or(&[]));
        if granted.iter().any(|g| g == "channel.view") {
            visible.insert(id.clone());
        }
    }
    Ok(visible)
}

/// Bir kanalı görebilen kişiler (verilen kişiler arasından).
pub async fn channel_audience(db: &PgPool, organization_id: &str, channel_id: &str, user_ids: &[String]) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let members: Vec<(String, String, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select "userId", role::text, permissions, "deniedPermissions" from "OrganizationMember" where "organizationId" = $1 and "userId" = any($2)"#,
    )
    .bind(organization_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    // (rol, sahibi) çiftleri: doğrudan atanmış ya da grup üzerinden.
    let holders: Vec<(String, Vec<String>, String)> = sqlx::query_as(
        r#"select r.id, r.permissions, m."userId" from "Role" r join "RoleMember" m on m."roleId" = r.id
             where r."organizationId" = $1 and not r.everyone and m."userId" = any($2)
           union
           select r.id, r.permissions, gm."userId" from "Role" r join "RoleGroup" g on g."roleId" = r.id join "UserGroupMember" gm on gm."groupId" = g."groupId"
             where r."organizationId" = $1 and not r.everyone and gm."userId" = any($2)"#,
    )
    .bind(organization_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    let groups: Vec<(String, String)> = sqlx::query_as(
        r#"select gm."userId", gm."groupId" from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where gm."userId" = any($1) and g."organizationId" = $2"#,
    )
    .bind(user_ids)
    .bind(organization_id)
    .fetch_all(db)
    .await?;
    let everyone: Vec<(String, Vec<String>)> = sqlx::query_as(r#"select id, permissions from "Role" where "organizationId" = $1 and everyone"#)
        .bind(organization_id)
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
    let owner: Option<String> = sqlx::query_scalar(
        r#"select p."ownerId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and p."organizationId" = $2 and p."deletedAt" is null"#,
    )
    .bind(channel_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await?;
    let Some(project_owner) = owner else { return Ok(Vec::new()) };
    let project_members: Vec<String> = sqlx::query_scalar(
        r#"select m."userId" from "ProjectMember" m join "ChannelCategory" c on c."projectId" = m."projectId" join "ProjectChannel" ch on ch."categoryId" = c.id
           where ch.id = $1 and m."userId" = any($2)"#,
    )
    .bind(channel_id)
    .bind(user_ids)
    .fetch_all(db)
    .await?;
    let base: Vec<RoleInfo> = everyone
        .into_iter()
        .map(|(id, permissions)| RoleInfo { id, handle: String::new(), name: String::new(), color: None, everyone: true, founder: false, permissions: clean(&permissions) })
        .collect();
    let mut out = Vec::new();
    for (user_id, role, allow, denied) in members {
        let owner = role == "OWNER";
        if !owner && user_id != project_owner && !project_members.contains(&user_id) {
            continue;
        }
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
