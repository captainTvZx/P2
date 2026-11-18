//! Sunucu veri tipleri (NestJS yanıtlarıyla aynı alan adları, camelCase).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub email: String,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub two_factor_enabled: bool,
    /// Sunucu çapında yetkiler (`GET /auth/me` → `serverPermissions`): `project.create`, `server.*`.
    #[serde(default)]
    pub server_permissions: Vec<String>,
}

impl User {
    /// Sunucu yetkisi var mı (`project.create`, `server.ban`, ...).
    pub fn can_server(&self, permission: &str) -> bool {
        self.server_permissions.iter().any(|p| p == permission)
    }

    /// "Sunucu yönetimi" paneline erişim: herhangi bir `server.*` yetkisi.
    pub fn server_panel(&self) -> bool {
        self.server_permissions.iter().any(|p| p.starts_with("server."))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    pub user: User,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Health {
    pub status: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub default_channel_id: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub library_enabled: bool,
    /// Proje topluluğunun açıklaması (davet önizlemesi, proje ayarları).
    #[serde(default)]
    pub description: Option<String>,
    /// Keşfet: `PUBLIC` (listelenir, kodsuz katılınır) ya da `PRIVATE` (yalnız davet koduyla).
    #[serde(default)]
    pub visibility: String,
    /// Keşfet kategorisi (GAME / DEV / COMMUNITY / EDUCATION / MUSIC / OTHER).
    #[serde(default)]
    pub category: Option<String>,
    /// Keşfet kartındaki tek satırlık tanıtım.
    #[serde(default)]
    pub tagline: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum ChannelType {
    #[default]
    Text,
    Announcement,
    Voice,
    Forum,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: ChannelType,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub unread: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
}

/// Kanal uçları tepkileri özetlenmiş (`{emoji,count,reacted}`), DM ve grup uçları ham
/// (`{emoji,userId}`) döndürür; ikisi de buraya okunur, ham olanlar `Message::normalize`
/// ile özetlenir.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    pub emoji: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub reacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotedMessage {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub author: Author,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ReplyCount {
    #[serde(default)]
    pub replies: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub edited_at: Option<String>,
    #[serde(default)]
    pub forwarded_from_id: Option<String>,
    #[serde(default)]
    pub quoted_message: Option<QuotedMessage>,
    #[serde(default)]
    pub author: Author,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default, rename = "_count")]
    pub count: ReplyCount,
    /// DM'lerde arama kayıtları: `CALL_STARTED` / `CALL_ENDED` / `CALL_MISSED`; yoksa metin.
    #[serde(default)]
    pub kind: Option<String>,
    /// Son yanıt(lar): kanal mesajlarında en yeni 1 (yazar adı + zaman) — iş parçacığı bağlantısı.
    #[serde(default)]
    pub replies: Vec<ReplyPreview>,
}

/// `replies: { take: 1, select: { createdAt, author: { displayName } } }`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyPreview {
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub author: Author,
}

impl Message {
    /// Ham tepki satırlarını (DM/grup) emoji başına özetler; özetliyse dokunmaz.
    pub fn normalize(&mut self, me: &str) {
        if self.reactions.iter().any(|r| r.user_id.is_some()) {
            let rows: Vec<(String, String)> = self.reactions.iter().map(|r| (r.emoji.clone(), r.user_id.clone().unwrap_or_default())).collect();
            self.reactions = group_reactions(&rows, me);
        }
    }

    pub fn is_text(&self) -> bool {
        self.kind.as_deref().is_none_or(|k| k == "TEXT")
    }
}

/// Mesajların nereye ait olduğu: proje kanalı, birebir sohbet ya da grup. Üç kaynağın uçları
/// aynı ekleri kullanır (`/messages`, `/read`, `/messages/:id/reactions`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChatTarget {
    Channel { org: String, project: String, channel: String },
    Direct { org: String, conversation: String },
    Group { org: String, group: String },
}

/// Gerçek zamanlı olayların hangi sohbete düştüğü (yalnız kimlik).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Channel(String),
    Direct(String),
    Group(String),
}

impl ChatTarget {
    pub fn base(&self) -> String {
        match self {
            Self::Channel { org, project, channel } => format!("/organizations/{org}/projects/{project}/channels/{channel}"),
            Self::Direct { org, conversation } => format!("/organizations/{org}/direct/{conversation}"),
            Self::Group { org, group } => format!("/organizations/{org}/groups/{group}"),
        }
    }

    /// İlk sayfa: kanal 100, DM/grup 200 mesaj (sunucu varsayılanları).
    pub fn first_page(&self) -> u32 {
        if matches!(self, Self::Channel { .. }) { 100 } else { 200 }
    }

    pub fn scope(&self) -> Scope {
        match self {
            Self::Channel { channel, .. } => Scope::Channel(channel.clone()),
            Self::Direct { conversation, .. } => Scope::Direct(conversation.clone()),
            Self::Group { group, .. } => Scope::Group(group.clone()),
        }
    }
}

/// Birebir sohbet listesi satırı (direct.ts `shape`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub user: Member,
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub last_message: Option<LastMessage>,
    /// Son etkinlik (ISO); liste ve ray baloncukları buna göre sıralanır.
    #[serde(default)]
    pub updated_at: String,
    /// Karşı taraf arkadaşım mı / onu engelledim mi (docs/arkadaslar.md).
    #[serde(default)]
    pub friend: bool,
    #[serde(default)]
    pub blocked_by_me: bool,
    /// Kapsam: `None` = Ana sayfa sohbeti, dolu = o projenin içindeki sohbet (ayrı mesajlar/okunmamışlar).
    #[serde(default)]
    pub project_id: Option<String>,
    /// Listemden kaldırdım (mesajlar durur; yeni mesajda ya da yeniden açınca geri gelir).
    #[serde(default)]
    pub hidden: bool,
}

impl Conversation {
    /// Ana sayfa sohbeti mi (projeye bağlı değil).
    pub fn is_home(&self) -> bool {
        self.project_id.is_none()
    }

    /// Verilen kapsamın sohbeti mi (`None` = Ana sayfa).
    pub fn in_scope(&self, project: Option<&str>) -> bool {
        self.project_id.as_deref() == project
    }

    /// Sohbet listelerinde görünür mü: listeden kaldırılmamış ve en az bir etkileşim (mesaj ya da
    /// arama kaydı) var. Kişiye tıklayınca açılan boş sohbet taslaktır; ilk mesajda / aramada listeye girer.
    pub fn listed(&self) -> bool {
        !self.hidden && self.last_message.is_some()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LastMessage {
    pub id: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub author_id: String,
}

/// Proje içi sohbet grubu (groups.ts `shape`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Ana sayfa grubu: projeye bağlı değil, Ana sayfadaki mesaj listesinde durur.
    #[serde(default)]
    pub home: bool,
    #[serde(default)]
    pub created_by_id: Option<String>,
    #[serde(default)]
    pub members: Vec<Member>,
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub last_message: Option<LastMessage>,
}

/// Arama katılımcısı (CallProvider.tsx `CallParticipant`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallParticipant {
    pub user_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub deafened: bool,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub screen: bool,
    /// Yayının izleyicilere açtığı geçmiş (geri sarma) süresi, sn; 0 = kapalı. Eski sunucu göndermez.
    #[serde(default)]
    pub rewind: u32,
}

/// Sunucudaki arama (`publicCall`): `kind` = direct | channel | group.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallState {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub organization_id: String,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    /// Bireysel aramanın kapsamı: proje sohbetinden başlatıldıysa o proje, Ana sayfadan ise `None`.
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub caller: Option<String>,
    #[serde(default)]
    pub callee: Option<String>,
    #[serde(default)]
    pub participants: Vec<CallParticipant>,
    #[serde(default)]
    pub invited: Vec<String>,
}

impl CallState {
    pub fn has(&self, user: &str) -> bool {
        self.participants.iter().any(|p| p.user_id == user)
    }
}

/// 2FA kurulumu: gizli anahtar ve doğrulayıcı uygulamaya eklenecek `otpauth://` adresi.
#[derive(Clone, Debug, Deserialize)]
pub struct TwoFactorSetup {
    pub secret: String,
    pub otpauth: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub role: String,
    /// Profil panelindeki "E-posta" alanı (üye listesi gönderiyorsa).
    #[serde(default)]
    pub email: Option<String>,
}

/// projects.ts `groupReactions`: ham (emoji, userId) satırlarını özetler.
pub fn group_reactions(rows: &[(String, String)], me: &str) -> Vec<Reaction> {
    let mut out: Vec<Reaction> = Vec::new();
    for (emoji, user) in rows {
        let item = match out.iter_mut().find(|r| &r.emoji == emoji) {
            Some(r) => r,
            None => {
                out.push(Reaction { emoji: emoji.clone(), count: 0, reacted: false, user_id: None });
                out.last_mut().unwrap()
            }
        };
        item.count += 1;
        if user == me {
            item.reacted = true;
        }
    }
    out
}

/// Kayıt / ilk yönetici formu.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterInput {
    pub email: String,
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub password_confirm: String,
}
