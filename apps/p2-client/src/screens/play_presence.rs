//! Oyun etkinliği ve Steam tarzı köşe kartları:
//! - Oyun açılınca kendi etkinliğim "oynuyor" olur (`activity:set`, Ayarlar › Kütüphane "etkinliğimi
//!   paylaş") ve çevrimiçi ekibe duyurulur (`library:announce`, "başlattığımda ekibe duyur").
//! - Bir takım arkadaşı oyun başlatınca sağ altta avatarlı "Oyunda" kartı (`library:playing`,
//!   "takım arkadaşı başlatınca bildir").
//! - Bir arkadaşım çevrimiçi olunca sağ altta "çevrimiçi oldu" kartı (kişi başına 10 dk'da bir).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use gpui_kit::Context;
use serde_json::{Value, json};

use super::WorkspaceScreen;

/// Bağlandıktan sonraki ilk saniyelerde gelen durumlar anlık görüntüdür; kart çıkmaz.
const SETTLE: Duration = Duration::from_secs(8);
const ONLINE_EVERY: Duration = Duration::from_secs(600);

static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// Gerçek zamanlı bağlantı kuruldu (yeniden bağlanınca da): anlık görüntü süresi baştan başlar.
pub(super) fn connected() {
    *STARTED.lock().unwrap() = Some(Instant::now());
}
static ONLINE_SEEN: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

impl WorkspaceScreen {
    fn library_settings(&self, cx: &Context<Self>) -> p2_client_core::store::LibraryPrefs {
        self.app.upgrade().map(|a| a.read(cx).settings.library.clone()).unwrap_or_default()
    }

    /// Patcher `Running`: kendi etkinliğim + ekibe duyuru.
    pub(super) fn play_presence(&mut self, project: &str, running: bool, cx: &mut Context<Self>) {
        let lib = self.library_settings(cx);
        if !lib.share_activity {
            return;
        }
        if !running {
            self.realtime.emit("activity:set", json!({ "kind": Value::Null }));
            return;
        }
        let name = self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()).unwrap_or_default();
        if name.is_empty() {
            return;
        }
        self.realtime.emit("activity:set", json!({ "kind": "playing", "projectId": project, "projectName": name }));
        if lib.announce_play {
            self.realtime.emit("library:announce", json!({ "organizationId": self.org.id, "projectId": project, "projectName": name }));
            crate::trace(&format!("kütüphane: oyun ekibe duyuruldu «{name}»"));
        }
    }

    /// `library:playing`: bir takım arkadaşı oyun başlattı.
    pub(super) fn on_library_playing(&mut self, payload: &Value, cx: &mut Context<Self>) {
        let user = &payload["user"];
        let (id, name) = (user["id"].as_str().unwrap_or_default(), user["displayName"].as_str().unwrap_or_default());
        let project = payload["projectName"].as_str().unwrap_or_default();
        if id.is_empty() || id == self.auth.user.id || project.is_empty() || !self.library_settings(cx).play_toasts {
            return;
        }
        crate::avatars::remember(id, user["avatarUrl"].as_str());
        crate::toasts::play(id, if name.is_empty() { user["username"].as_str().unwrap_or_default() } else { name }, &crate::toasts::PlayKind::Playing(project.to_string()));
    }

    /// `presence:update`: arkadaşım çevrimdışıyken çevrimiçi oldu.
    pub(super) fn on_presence_online(&mut self, user_id: &str, before: Option<&str>, status: &str) {
        let settling = STARTED.lock().unwrap().is_none_or(|t| t.elapsed() < SETTLE);
        if settling || user_id == self.auth.user.id || status == "OFFLINE" || before.is_some_and(|b| b != "OFFLINE") {
            return;
        }
        let Some(friend) = self.home.data.friend(user_id) else { return };
        let mut seen = ONLINE_SEEN.lock().unwrap();
        let seen = seen.get_or_insert_with(HashMap::new);
        if seen.get(user_id).is_some_and(|t| t.elapsed() < ONLINE_EVERY) {
            return;
        }
        seen.insert(user_id.to_string(), Instant::now());
        crate::avatars::remember(user_id, friend.avatar_url.as_deref());
        crate::toasts::play(user_id, &friend.display_name, &crate::toasts::PlayKind::Online);
    }
}
