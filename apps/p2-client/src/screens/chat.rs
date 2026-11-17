//! Sohbet bölmesi — ChannelContent.tsx, DirectMessages.tsx ve GroupChat.tsx'in ortak çekirdeği:
//! mesaj listesi (sanal, alttan hizalı, üste kaydırınca eski sayfa), gönderme, düzenleme,
//! silme, tepkiler, alıntı gösterimi, arama kayıtları, "yazıyor" satırı, gerçek zamanlı
//! güncellemeler. Kanal, birebir sohbet ve grup aynı uç eklerini kullandığından
//! (`ChatTarget`) tek bileşen üçünü de çizer; farklar başlıkta ve karşılama metnindedir.
//!
//! Önbellekli bir görünüm olarak gömülür (`cached()`): yalnız kendi bildirimiyle yeniden kurulur.
//! Satır görünümü `chat_view.rs`, yazma kutusu bu dosyada (`composer_bar`, sohbetin kardeşi),
//! açılır menüler/çekmeceler/pencereler `chat_float.rs` (`floating`, çalışma alanı kökünde).

use std::rc::Rc;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, EventEmitter, Focusable as _, KeyDownEvent, ListAlignment, ListState, PathPromptOptions, SharedString, Subscription, WeakEntity, Window, div, list, px};
use p2_client_core::{Api, ApiError, Channel, ChannelType, ChatGroup, ChatTarget, Member, Message, UploadedFile, User, UserGroupSummary, group_reactions};
use p2_ui::P2Text as _;
use p2_ui::icons::icon;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_EMOJI, FONT_MONO, Palette, fs};
use p2_ui::{Entrance, P2Motion as _};

use crate::task;

#[path = "chat_extras.rs"]
pub(crate) mod extras;
// Kanal başlığı (B ajanı iskeleti; menüler C).
#[path = "chat_header.rs"]
mod chat_header;
#[path = "chat_view.rs"]
pub(crate) mod view;
#[path = "chat_emoji.rs"]
pub(crate) mod emoji;
#[path = "chat_float.rs"]
mod float;
// D ajanı: DM/grup başlığı, karşılama, ••• menüsü.
#[path = "chat_people.rs"]
mod people;
// L ajanı: medya (PDF, video/ses oynatıcı, YouTube kartı, arşiv/belge önizlemesi).
#[path = "chat_media.rs"]
pub(crate) mod media;
#[path = "chat_media_view.rs"]
mod media_view;
pub(crate) use media_view::media_act;
// L ajanı: canlı biçim, bağlantı çubuğu, yapıştırma, sürükle-bırak.
#[path = "chat_compose.rs"]
mod compose;
use extras::{FileRef, ImageCache, Slot};
pub use view::RowAct;

const PAGE_OLDER: u32 = 50;
/// "yazıyor" en çok bu sıklıkla gönderilir; karşı tarafta bu kadar sonra kendiliğinden düşer.
const TYPING_EVERY: Duration = Duration::from_secs(3);
const TYPING_TTL: Duration = Duration::from_secs(6);
/// ChannelContent.tsx `commands`.
/// İkinci öğe açıklamanın i18n anahtarı (`chat.cmd.*`).
const COMMANDS: [(&str, &str); 3] = [("shrug", "chat.cmd.shrug"), ("me", "chat.cmd.me"), ("spoiler", "chat.cmd.spoiler")];

/// Bölmenin neyi gösterdiği: başlık, karşılama ve yer tutucu buna göre.
#[derive(Clone)]
pub enum ChatSubject {
    Channel(Channel),
    Direct(Member),
    Group(ChatGroup),
}

/// Bölmeden çalışma alanına: profil aç, gruptan ayrıl, yazıyor bildir.
pub enum ChatEvent {
    OpenProfile(String),
    LeaveGroup(String),
    Typing(bool),
    /// DM'de kişiyi, grupta grubu ara.
    StartCall,
    /// İş parçacığını / forum konusunu yan panelde aç.
    OpenThread(Message),
    /// Kanal mesajını başka kanala ilet (çalışma alanı hedef seçtirir).
    Forward(String),
    /// Grup üyelerini yönet (ekle / çıkar).
    ManageGroup(ChatGroup),
    /// Başlıktaki yıldız (kanalı yıldızla / kaldır).
    ToggleStar,
    /// Birebir sohbeti listemden kaldır (mesajlar silinmez; "•••" menüsü).
    HideConversation,
    /// Bildirim tercihi (MENTIONS | MUTED).
    SetNotify(&'static str),
    /// Kanal izinleri penceresi.
    ChannelPermissions,
    /// Kanaldan ayrıl (kenar çubuğundan gizle).
    LeaveChannel,
    /// Kanal adı/konusu değişti (kenar çubuğu yenilensin).
    ChannelUpdated(Channel),
    /// Sesli oda başlığındaki "Sohbeti göster/gizle" (I ajanı).
    ToggleVoiceChat,
    /// L ajanı: "Bölünmüş görünümde aç", "Yeni pencerede aç", bölünmüş bölmeyi kapat (✕).
    OpenSplit,
    OpenWindow,
    CloseSplit,
}

/// Bağlam menüsü: hangi mesaj, nerede (pencere koordinatı; `up` yukarı açılırken üst kenar).
#[derive(Clone)]
pub(super) struct MsgMenu {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub up: f32,
    pub right: bool,
}

/// Emoji panelinin hedefi: yazma kutusu ya da bir mesaja tepki.
pub(super) enum EmojiTarget {
    Composer,
    Reaction(MsgMenu),
}

/// Başlık araç menüleri (ChannelContent.tsx `toolMenu.kind`).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum ToolMenu {
    Members,
    Notify,
    More,
}

/// Ortadaki iletişim kutuları (`.action-modal-layer`).
pub(super) enum Dialog {
    Delete(Message),
    History(Vec<p2_client_core::HistoryItem>, bool),
    Details,
    Leave,
    Settings { name: Entity<InputState>, topic: Entity<InputState>, busy: bool, error: Option<String> },
}

/// L ajanı: ek görüntüleyicinin ağır içerikleri (AttachmentViewer.tsx gövdeleri).
pub(super) enum ViewerDoc {
    Loading,
    Error(String),
    Pdf(media::PdfPages),
    Office(Vec<media::OfficeSection>),
    Archive(Vec<(String, u64)>),
    Hex(String),
}

pub struct ChatPane {
    /// I ajanı kancası: sesli odanın "Oda sohbeti" yan panelinde başlıksız çizilir
    /// (başlık sesli oda görünümünde, `header_element` ile).
    pub embedded: bool,
    api: Arc<Api>,
    pub target: ChatTarget,
    pub subject: ChatSubject,
    me: User,
    messages: Vec<Message>,
    list: ListState,
    loading: bool,
    has_more: bool,
    loading_older: bool,
    /// Bu indeks ve sonrası canlı gelen mesajlar ("rise" ile girer).
    first_live: usize,
    composer: Entity<TextareaState>,
    editing: Option<String>,
    confirm_leave: bool,
    /// Kanal başlığı durumu (çalışma alanı yazar: yıldız, bildirim).
    pub(super) head: chat_header::HeadState,
    hovered: Option<String>,
    sending: bool,
    error: Option<(String, usize)>,
    /// Yazanlar: (kullanıcı, ad, sürüm) — sürüm eskimişse zamanlayıcı satırı düşürmez.
    typists: Vec<(String, String, u64)>,
    typing_epoch: u64,
    typing_sent: Option<Instant>,
    /// Yanıtlanan (alıntılanacak) mesaj.
    replying: Option<Message>,
    /// Gönderilecek ekler ve süren yüklemeler.
    attachments: Vec<UploadedFile>,
    uploading: usize,
    /// Yazma kutusundaki öneriler: seçili sıra, Esc ile kapatıldı mı.
    suggestion_ix: usize,
    suggestions_closed: bool,
    members: Vec<Member>,
    user_groups: Vec<UserGroupSummary>,
    images: ImageCache,
    /// Forum kanalı: üst düzey mesajlar konudur.
    pub forum: bool,
    /// Otomatik pilot: yükleme bitince bu metinle gönder.
    send_after_upload: Option<String>,
    // ---- açılır öğeler (chat_float.rs)
    pub(super) menu: Option<MsgMenu>,
    pub(super) emoji: Option<(emoji::EmojiPanel, EmojiTarget)>,
    pub(super) tool_menu: Option<ToolMenu>,
    pub(super) members_search: Option<Entity<InputState>>,
    pub(super) dialog: Option<Dialog>,
    /// Kanal içi arama çekmecesi: kutu, sonuçlar (None: en az 2 harf), sürüyor mu, sürüm.
    pub(super) search: Option<Entity<InputState>>,
    pub(super) search_hits: Option<Vec<p2_client_core::SearchHit>>,
    pub(super) searching: bool,
    search_epoch: u64,
    /// Ek görüntüleyici: dosya ve yakınlaştırma.
    pub(super) viewer: Option<(FileRef, f32)>,
    /// "Kopyalandı" (kod bloğu anahtarı).
    copied: Option<String>,
    /// `.latest`: yeni gelen (benim olmayan) mesajın kısa vurgusu.
    latest: Option<String>,
    /// `.flash`: aramadan gidilen mesaj.
    flash: Option<String>,
    /// L ajanı: ek oynatıcıları (dosya yolu → oynatıcı) ve görüntüleyici belgesi.
    pub(super) players: media::Players,
    pub(super) viewer_doc: Option<ViewerDoc>,
    /// `message.delete_any` / `message.delete_unlimited` (kanal izinleriyle; DM/grupta yalnız süresiz).
    pub can_delete_any: bool,
    pub can_delete_unlimited: bool,
    play_tick: bool,
    /// L ajanı: bölünmüş görünümün ikinci bölmesi / ayrı pencere (`splitPane`, popout).
    pub split_pane: bool,
    pub popout: bool,
    /// Forum: açık konu (akışın yerine) ve üstteki "Yeni bir konu başlığı yaz…" kutusu.
    forum_topic: Option<Entity<super::thread::ThreadPane>>,
    /// Yazma kutusu: biçim kipi (tür, başlangıç), iç değişiklik koruması, bağlantı çubuğu, sürükleme.
    fmt_pending: Option<(compose::Fmt, usize)>,
    fmt_guard: bool,
    link_bar: Option<(Entity<InputState>, std::ops::Range<usize>)>,
    drag_over: bool,
    bounds: Rc<std::cell::Cell<gpui_kit::Bounds<gpui_kit::Pixels>>>,
    topic_input: Option<Entity<InputState>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ChatEvent> for ChatPane {}

pub fn clock_text(ts: &str) -> String {
    view::tr_time(ts)
}

/// typingText (useTyping.ts): "Cem yazıyor…", "Cem ve Ada yazıyor…", "3 kişi yazıyor…".
#[cfg_attr(not(test), allow(dead_code))]
fn typing_text(names: &[&str]) -> String {
    typing_text_in("tr", names)
}

/// Seçili dilde "yazıyor" satırı.
fn typing_label(names: &[&str]) -> String {
    typing_text_in(p2_i18n::language().id, names)
}

fn typing_text_in(lang: &str, names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [a] => p2_i18n::translate(lang, "chat.typing.one", &[("a", *a)]),
        [a, b] => p2_i18n::translate(lang, "chat.typing.two", &[("a", *a), ("b", *b)]),
        _ => p2_i18n::translate(lang, "chat.typing.many", &[("n", &names.len().to_string())]),
    }
}

/// DM arama kaydı satırı (DirectMessages.tsx `callSummary`).
fn call_summary(message: &Message, own: bool, other: &str) -> (bool, String) {
    let seconds: u64 = message.content.trim().parse().unwrap_or(0);
    let length = format!("{:02}:{:02}", seconds / 60, seconds % 60);
    match message.kind.as_deref() {
        Some("CALL_ENDED") => (false, p2_i18n::tv("chat.call.ended", &[("length", &length)])),
        Some("CALL_MISSED") => (true, if own { p2_i18n::tv("chat.call.noAnswer", &[("name", other)]) } else { p2_i18n::t("chat.call.missed") }),
        _ => (false, if own { p2_i18n::tv("chat.call.calling", &[("name", other)]) } else { p2_i18n::t("chat.call.incoming") }),
    }
}

/// ChannelContent.tsx `expandSlashCommand`.
fn expand_slash(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("/shrug") {
        return format!("{} ¯\\_(ツ)_/¯", rest.trim()).trim().to_string();
    }
    if let Some(rest) = value.strip_prefix("/me ") {
        return format!("*{}*", rest.trim());
    }
    if let Some(rest) = value.strip_prefix("/spoiler ") {
        return format!("||{}||", rest.trim());
    }
    value.to_string()
}

/// Öneri satırı: emoji kısa kodu, bahsetme (etiket, açıklama, değer, avatar) ya da komut.
#[derive(Clone)]
pub(super) enum Suggestion {
    Emoji(String, &'static str),
    Mention { label: String, description: String, value: String, avatar: String },
    Command(&'static str, &'static str),
}

impl ChatPane {
    pub fn new(api: Arc<Api>, target: ChatTarget, subject: ChatSubject, me: User, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = match &subject {
            ChatSubject::Channel(c) => if c.kind == ChannelType::Forum { p2_i18n::tv("chat.ph.forum", &[("name", &c.name)]) } else { p2_i18n::tv("chat.ph.channel", &[("name", &c.name)]) },
            ChatSubject::Direct(m) => p2_i18n::tv("chat.ph.direct", &[("name", &m.display_name)]),
            ChatSubject::Group(g) => p2_i18n::tv("chat.ph.group", &[("name", &g.name)]),
        };
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 11).submit_on_enter(true).placeholder(placeholder));
        let subs = vec![cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
            InputEvent::Change => {
                // 4000 karakter sınırı (Electron `value.slice(0,4000)`).
                let value = this.composer.read(cx).value().to_string();
                if value.chars().count() > 4000 {
                    let cut: String = value.chars().take(4000).collect();
                    this.composer.update(cx, |s, cx| s.set_value(cut, window, cx));
                }
                this.suggestions_closed = false;
                this.suggestion_ix = 0;
                this.after_change(window, cx);
                this.typed(cx);
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
            _ => {}
        })];
        let forum_kind = matches!(&subject, ChatSubject::Channel(c) if c.kind == ChannelType::Forum);
        // Forum akışı üstten dizilir (`.forum-feed`), sohbet alttan.
        let list = ListState::new(0, if forum_kind { ListAlignment::Top } else { ListAlignment::Bottom }, px(800.));
        let me_entity = cx.entity().downgrade();
        list.set_scroll_handler(move |event, window, cx| {
            if event.visible_range.start <= 2 {
                let _ = me_entity.update(cx, |this, cx| this.load_older(window, cx));
            }
        });
        let forum = matches!(&subject, ChatSubject::Channel(c) if c.kind == ChannelType::Forum);
        let mut pane = Self {
            api,
            target,
            subject,
            me,
            messages: Vec::new(),
            list,
            loading: true,
            has_more: false,
            loading_older: false,
            first_live: usize::MAX,
            composer,
            editing: None,
            confirm_leave: false,
            embedded: false,
            head: Default::default(),
            hovered: None,
            sending: false,
            error: None,
            typists: Vec::new(),
            typing_epoch: 0,
            typing_sent: None,
            replying: None,
            attachments: Vec::new(),
            uploading: 0,
            suggestion_ix: 0,
            suggestions_closed: false,
            members: Vec::new(),
            user_groups: Vec::new(),
            images: ImageCache::new(),
            forum,
            send_after_upload: None,
            menu: None,
            emoji: None,
            tool_menu: None,
            members_search: None,
            dialog: None,
            search: None,
            search_hits: None,
            searching: false,
            search_epoch: 0,
            viewer: None,
            copied: None,
            latest: None,
            flash: None,
            players: Default::default(),
            viewer_doc: None,
            can_delete_any: false,
            can_delete_unlimited: false,
            play_tick: false,
            split_pane: false,
            popout: false,
            forum_topic: None,
            fmt_pending: None,
            fmt_guard: false,
            link_bar: None,
            drag_over: false,
            bounds: Rc::default(),
            topic_input: None,
            _subs: subs,
        };
        if pane.forum {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("chat.ph.topic")));
            let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.send_topic(window, cx),
                _ => cx.notify(),
            });
            pane._subs.push(sub);
            pane.topic_input = Some(input);
        }
        pane.load(window, cx);
        pane.load_user_groups(window, cx);
        pane
    }

    /// İzlerde ve testlerde kullanılan kısa ad: `#genel`, `@cem`, `grup:Tasarım`.
    pub fn label(&self) -> String {
        match &self.subject {
            ChatSubject::Channel(c) => format!("#{}", c.name),
            ChatSubject::Direct(m) => format!("@{}", m.username),
            ChatSubject::Group(g) => format!("grup:{}", g.name),
        }
    }

    fn base(&self) -> (Arc<Api>, ChatTarget, String) {
        (self.api.clone(), self.target.clone(), self.me.id.clone())
    }

    fn org(&self) -> String {
        match &self.target {
            ChatTarget::Channel { org, .. } | ChatTarget::Direct { org, .. } => org.clone(),
            ChatTarget::Group { org, .. } => org.clone(),
        }
    }

    fn set_error(&mut self, e: ApiError, cx: &mut Context<Self>) {
        // Bildirimler sağ üstte (toasts.rs); sohbetin altında ayrı kırmızı kutu yok.
        crate::toasts::error(if e.message.is_empty() { p2_i18n::t("auth.err.generic") } else { e.message });
        cx.notify();
    }

    /// @grup bahsetme kaynağı: kullanıcı grupları projeye özel (kanalın ya da sohbet grubunun projesi);
    /// bireysel sohbette proje yok → grup bahsetmesi yok.
    fn load_user_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project = match (&self.target, &self.subject) {
            (ChatTarget::Channel { project, .. }, _) => Some(project.clone()),
            (_, ChatSubject::Group(g)) => g.project_id.clone(),
            _ => None,
        };
        let Some(project) = project else { return };
        let api = self.api.clone();
        task::run(window, cx, move || api.project_mention_groups(&project), |this, r, _, cx| {
            this.user_groups = r.unwrap_or_default();
            cx.notify();
        });
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, target, me) = self.base();
        let page = target.first_page();
        task::run(
            window,
            cx,
            move || {
                let messages = api.messages(&target, None, page, &me);
                let _ = api.mark_read(&target);
                messages
            },
            move |this, result, window, cx| {
                this.loading = false;
                match result {
                    Ok(messages) => {
                        this.has_more = messages.len() as u32 >= page;
                        this.first_live = messages.len();
                        this.list.reset(messages.len());
                        this.messages = messages;
                        crate::trace(&format!("sohbet: {} {} mesaj yüklendi", this.label(), this.messages.len()));
                        if let Some(last) = this.messages.last().filter(|m| m.author.id != this.me.id).map(|m| m.id.clone()) {
                            this.mark_latest(last, window, cx);
                        }
                        // Otomatik pilot: süreç başına bir kez mesaj gönder.
                        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                        // Mesaj, otomatik pilotun açtığı sohbete gider: `P2_AUTOPILOT_DM` varsa birebir sohbete,
                        // `P2_AUTOPILOT_GROUP` / `_OPEN_GROUP` varsa gruba. (Eskiden ilk yüklenen sohbete gidiyordu;
                        // varsayılan kanal DM'den önce yüklenirse mesaj kanala düşüyordu.)
                        let wanted = if std::env::var_os("P2_AUTOPILOT_DM").is_some() {
                            matches!(this.subject, ChatSubject::Direct(_))
                        } else if std::env::var_os("P2_AUTOPILOT_GROUP").is_some() || std::env::var_os("P2_AUTOPILOT_OPEN_GROUP").is_some() {
                            matches!(this.subject, ChatSubject::Group(_))
                        } else {
                            true
                        };
                        if let Ok(text) = std::env::var("P2_AUTOPILOT_SAY").ok().filter(|_| wanted).ok_or(()) {
                            if !SAID.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                this.composer.update(cx, |s, cx| s.set_value(text, window, cx));
                                this.submit(window, cx);
                            }
                        }
                        this.autopilot_extras(window, cx);
                        this.autopilot_view(window, cx);
                    }
                    Err(e) => this.set_error(e, cx),
                }
                cx.notify();
            },
        );
    }

    fn load_older(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_more || self.loading_older || self.loading {
            return;
        }
        let Some(oldest) = self.messages.first().map(|m| m.created_at.clone()) else { return };
        self.loading_older = true;
        cx.notify();
        let (api, target, me) = self.base();
        task::run(window, cx, move || api.messages(&target, Some(&oldest), PAGE_OLDER, &me), |this, result, _, cx| {
            this.loading_older = false;
            match result {
                Ok(older) => {
                    this.has_more = older.len() as u32 >= PAGE_OLDER;
                    let older: Vec<Message> = older.into_iter().filter(|m| !this.messages.iter().any(|x| x.id == m.id)).collect();
                    let n = older.len();
                    this.messages.splice(0..0, older);
                    this.list.splice(0..0, n);
                    if this.first_live != usize::MAX {
                        this.first_live += n;
                    }
                }
                Err(e) => this.set_error(e, cx),
            }
            cx.notify();
        });
    }

    /// `useFreshMessage`: 2,8 sn sonra vurgu söner.
    fn mark_latest(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.latest = Some(id.clone());
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2800)).await;
            let _ = this.update(cx, |this, cx| {
                if this.latest.as_deref() == Some(id.as_str()) {
                    this.latest = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ gerçek zamanlı (çalışma alanından)

    pub fn on_created(&mut self, mut message: Message, window: &mut Window, cx: &mut Context<Self>) {
        if self.messages.iter().any(|m| m.id == message.id) {
            return;
        }
        message.normalize(&self.me.id);
        // Mesaj geldiyse yazan artık yazmıyor.
        self.typists.retain(|t| t.0 != message.author.id);
        let mine = message.author.id == self.me.id;
        let id = message.id.clone();
        let at = self.messages.len();
        self.messages.push(message);
        self.list.splice(at..at, 1);
        self.list.scroll_to_end();
        if mine {
            self.latest = None;
        } else {
            self.mark_latest(id, window, cx);
        }
        // Açık sohbette gelen mesaj okunmuş sayılır.
        let (api, target, _) = self.base();
        cx.background_executor().spawn(async move { let _ = api.mark_read(&target); }).detach();
        cx.notify();
    }

    pub fn on_updated(&mut self, mut message: Message, cx: &mut Context<Self>) {
        message.normalize(&self.me.id);
        if let Some(existing) = self.messages.iter_mut().find(|m| m.id == message.id) {
            let reactions = std::mem::take(&mut existing.reactions);
            let count = existing.count.clone();
            let replies = std::mem::take(&mut existing.replies);
            *existing = Message { reactions, count, replies, ..message };
            cx.notify();
        }
    }

    pub fn on_deleted(&mut self, message_id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.messages.iter().position(|m| m.id == message_id) {
            self.messages.remove(ix);
            self.list.splice(ix..ix + 1, 0);
            if self.first_live != usize::MAX && ix < self.first_live {
                self.first_live -= 1;
            }
            if self.editing.as_deref() == Some(message_id) {
                self.editing = None;
            }
            cx.notify();
        }
    }

    pub fn on_reactions(&mut self, message_id: &str, rows: &[(String, String)], cx: &mut Context<Self>) {
        let me = self.me.id.clone();
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == message_id) {
            m.reactions = group_reactions(rows, &me);
            cx.notify();
        }
    }

    /// Karşı tarafın "yazıyor" bildirimi; TYPING_TTL içinde yenilenmezse düşer.
    pub fn on_typing(&mut self, user_id: String, name: String, typing: bool, window: &mut Window, cx: &mut Context<Self>) {
        if user_id == self.me.id {
            return;
        }
        self.typists.retain(|t| t.0 != user_id);
        if typing {
            self.typing_epoch += 1;
            let epoch = self.typing_epoch;
            self.typists.push((user_id, name, epoch));
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(TYPING_TTL).await;
                let _ = this.update(cx, |this, cx| {
                    let before = this.typists.len();
                    this.typists.retain(|t| t.2 != epoch);
                    if this.typists.len() != before {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// DM başlığındaki karşı tarafın durumu.
    pub fn set_peer_status(&mut self, user_id: &str, status: &str, cx: &mut Context<Self>) {
        if let ChatSubject::Direct(m) = &mut self.subject {
            if m.id == user_id && m.status != status {
                m.status = status.to_string();
                cx.notify();
            }
        }
        if let Some(m) = self.members.iter_mut().find(|m| m.id == user_id) {
            m.status = status.to_string();
        }
    }

    pub fn set_group(&mut self, group: ChatGroup, cx: &mut Context<Self>) {
        if let ChatSubject::Group(g) = &mut self.subject {
            if g.id == group.id {
                *g = group;
                cx.notify();
            }
        }
    }

    // ------------------------------------------------------------ eylemler

    /// Yazarken en çok TYPING_EVERY'de bir "yazıyor" (grup sunucuda desteklenmiyor).
    fn typed(&mut self, cx: &mut Context<Self>) {
        if matches!(self.target, ChatTarget::Group { .. }) || self.editing.is_some() {
            return;
        }
        if self.composer.read(cx).value().trim().is_empty() {
            if self.typing_sent.take().is_some() {
                cx.emit(ChatEvent::Typing(false));
            }
            return;
        }
        if self.typing_sent.is_none_or(|at| at.elapsed() >= TYPING_EVERY) {
            self.typing_sent = Some(Instant::now());
            cx.emit(ChatEvent::Typing(true));
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Öneri listesi açıkken Enter seçili öneriyi uygular.
        let items = self.suggestions(cx);
        if !items.is_empty() {
            let ix = self.suggestion_ix.min(items.len() - 1);
            self.pick_suggestion(items[ix].clone(), window, cx);
            return;
        }
        let raw = expand_slash(self.composer.read(cx).value().trim());
        let raw = emoji::expand_shortcodes(&raw);
        let content = if self.editing.is_some() { raw } else { extras::compose_content(&raw, &self.attachments) };
        if content.is_empty() || self.sending || self.uploading > 0 {
            return;
        }
        self.sending = true;
        if self.typing_sent.take().is_some() {
            cx.emit(ChatEvent::Typing(false));
        }
        let (api, target, me) = self.base();
        if let Some(id) = self.editing.clone() {
            task::run(window, cx, move || api.edit_message(&target, &id, &content, &me), |this, r, window, cx| {
                this.sending = false;
                match r {
                    Ok(m) => {
                        this.on_updated(m, cx);
                        this.cancel_edit(window, cx);
                    }
                    Err(e) => this.set_error(e, cx),
                }
            });
        } else {
            let quoted = self.replying.as_ref().map(|m| m.id.clone());
            task::run(window, cx, move || api.send_message(&target, &content, quoted.as_deref(), &me), |this, r, window, cx| {
                this.sending = false;
                match r {
                    Ok(m) => {
                        this.replying = None;
                        this.attachments.clear();
                        this.composer.update(cx, |s, cx| s.clean(window, cx));
                        this.on_created(m, window, cx);
                    }
                    Err(e) => this.set_error(e, cx),
                }
            });
        }
    }

    fn start_edit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.messages.iter().find(|m| m.id == id) else { return };
        let content = m.content.clone();
        self.editing = Some(id.to_string());
        self.replying = None;
        self.composer.update(cx, |s, cx| {
            // RichComposer: düzenlenen markdown biçimli görünür.
            s.set_value(compose::markdown_content(&content), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.composer.update(cx, |s, cx| s.clean(window, cx));
        cx.notify();
    }

    pub(super) fn delete(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        let (api, target, _) = self.base();
        let message = id.clone();
        task::run(window, cx, move || api.delete_message(&target, &message), move |this, r, _, cx| match r {
            Ok(()) => this.on_deleted(&id, cx),
            Err(e) => this.set_error(e, cx),
        });
    }

    pub(super) fn react(&mut self, id: String, emoji: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (api, target, _) = self.base();
        let emoji = emoji.to_string();
        let message = id.clone();
        task::run(window, cx, move || api.toggle_reaction(&target, &message, &emoji), move |this, r, _, cx| match r {
            Ok(reactions) => {
                if let Some(m) = this.messages.iter_mut().find(|m| m.id == id) {
                    m.reactions = reactions;
                    cx.notify();
                }
            }
            Err(e) => this.set_error(e, cx),
        });
    }

    fn open_profile(&mut self, user_id: String, cx: &mut Context<Self>) {
        cx.emit(ChatEvent::OpenProfile(user_id));
    }

    /// Mesajın kalıcı bağlantısı (yalnız kanallar).
    pub(super) fn message_link(&self, id: &str) -> Option<String> {
        match &self.target {
            ChatTarget::Channel { channel, .. } => Some(format!("p2://channels/{channel}/messages/{id}")),
            _ => None,
        }
    }

    /// Satır eylemleri (chat_view.rs `RowAct`).
    fn row_act(&mut self, id: &str, act: RowAct, window: &mut Window, cx: &mut Context<Self>) {
        let Some(message) = self.messages.iter().find(|m| m.id == id).cloned() else { return };
        match act {
            RowAct::Profile => self.open_profile(message.author.id.clone(), cx),
            RowAct::React(e) => self.react(id.to_string(), &e, window, cx),
            RowAct::Thread => cx.emit(ChatEvent::OpenThread(message)),
            RowAct::Quote => self.start_reply(id, window, cx),
            RowAct::Menu(at) => self.open_menu(id, at.x.into(), at.y.into(), at.y.into(), false, cx),
            RowAct::MenuFrom { right, bottom, top } => self.open_menu(id, right.into(), f32::from(bottom) + 6., f32::from(top) - 6., true, cx),
            RowAct::PickerFrom { right, bottom, top } => {
                let at = MsgMenu { id: id.to_string(), x: right.into(), y: f32::from(bottom) + 6., up: f32::from(top) - 6., right: true };
                self.open_emoji(EmojiTarget::Reaction(at), window, cx);
            }
            RowAct::Download(f) => self.download(f, window, cx),
            RowAct::View(f) => {
                crate::trace(&format!("sohbet: görüntüleyici {}", f.name));
                if f.is_image() || extras::preview_kind(&f.name, &f.mime) == extras::Kind::Text {
                    self.load_file(&f, window, cx);
                }
                // L ajanı: PDF/belge/arşiv/ikili gövdeleri.
                self.load_viewer_doc(&f, window, cx);
                self.viewer = Some((f, 1.));
                cx.notify();
            }
            RowAct::Mention(username) => {
                if let Some(m) = self.members.iter().find(|m| m.username.to_lowercase() == username.to_lowercase()) {
                    let id = m.id.clone();
                    self.open_profile(id, cx);
                }
            }
            RowAct::Url(url) => cx.open_url(&url),
            RowAct::CopyCode(key, code) => {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(code));
                self.copied = Some(key.clone());
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(1500)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.copied.as_deref() == Some(key.as_str()) {
                            this.copied = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            RowAct::Play(_) | RowAct::Seek(..) | RowAct::Volume(..) | RowAct::Mute(_) | RowAct::Fullscreen(_) | RowAct::Youtube(_) => self.media_act(act, window, cx),
            RowAct::Hover(over) => {
                // Parite kancası üzerinde-durumu sabitler (gerçek imleç onu bozmasın).
                if std::env::var_os("P2_AUTOPILOT_HOVER").is_some() {
                    return;
                }
                if over {
                    self.hovered = Some(id.to_string());
                } else if self.hovered.as_deref() == Some(id) {
                    self.hovered = None;
                }
                cx.notify();
            }
        }
    }

    pub(super) fn open_menu(&mut self, id: &str, x: f32, y: f32, up: f32, right: bool, cx: &mut Context<Self>) {
        crate::trace(&format!("sohbet: mesaj menüsü açıldı ({})", id));
        self.emoji = None;
        self.tool_menu = None;
        self.menu = Some(MsgMenu { id: id.to_string(), x, y, up, right });
        cx.notify();
    }

    pub(super) fn open_emoji(&mut self, target: EmojiTarget, window: &mut Window, cx: &mut Context<Self>) {
        let panel = emoji::EmojiPanel::new(window, cx);
        let sub = cx.subscribe(&panel.search, |this, input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let q = input.read(cx).value().to_string();
                if let Some((p, _)) = this.emoji.as_mut() {
                    p.set_query(q);
                }
                cx.notify();
            }
        });
        self._subs.push(sub);
        self.menu = None;
        self.emoji = Some((panel, target));
        cx.notify();
    }

    /// Emoji seçildi: yazma kutusuna ekle ya da tepki ver.
    pub(super) fn emoji_picked(&mut self, e: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some((panel, target)) = self.emoji.as_mut() else { return };
        panel.chose(&e);
        match target {
            EmojiTarget::Composer => {
                // Electron: panel açık kalır (insert), yazma kutusuna eklenir.
                self.composer.update(cx, |s, cx| {
                    s.insert(e, window, cx);
                    s.focus(window, cx);
                });
            }
            EmojiTarget::Reaction(m) => {
                let id = m.id.clone();
                self.emoji = None;
                self.react(id, &e, window, cx);
            }
        }
        cx.notify();
    }

    /// Eski parite aracı (Electron ↔ Rust; kaldırıldı): yalnız GÖRÜNÜM kancaları, mesaj göndermez (süreç başına bir kez).
    /// `P2_AUTOPILOT_SCROLL=top` akışı en üste kaydırır, `P2_AUTOPILOT_DRAFT=<metin>` yazma kutusuna yazar,
    /// `P2_AUTOPILOT_EMOJI_PANEL=1` emoji panelini açar, `P2_AUTOPILOT_MENTION_POPUP=<sorgu>` @öneri listesini açar,
    /// `P2_AUTOPILOT_MESSAGE_MENU=last@x,y` son mesajın bağlam menüsünü (x,y pencere noktasında) açar,
    /// `P2_AUTOPILOT_HOVER=last` son mesajı "üzerinde" gösterir, `P2_AUTOPILOT_THREAD_VIEW=last` yanıtı olan son
    /// mesajın iş parçacığını açar (yanıt yazmaz), `P2_AUTOPILOT_TOOL_MENU=members|notify|more` başlık menüsünü açar.
    fn autopilot_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let env = |k: &str| std::env::var(k).ok();
        let keys = ["P2_AUTOPILOT_SCROLL", "P2_AUTOPILOT_DRAFT", "P2_AUTOPILOT_EMOJI_PANEL", "P2_AUTOPILOT_MENTION_POPUP", "P2_AUTOPILOT_MESSAGE_MENU", "P2_AUTOPILOT_HOVER", "P2_AUTOPILOT_THREAD_VIEW", "P2_AUTOPILOT_TOOL_MENU", "P2_AUTOPILOT_LINK_BAR", "P2_AUTOPILOT_DROP_ZONE"];
        if !keys.iter().any(|k| env(k).is_some()) || DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if env("P2_AUTOPILOT_SCROLL").as_deref() == Some("top") {
            self.list.scroll_to(gpui_kit::ListOffset { item_ix: 0, offset_in_item: px(0.) });
        }
        if let Some(text) = env("P2_AUTOPILOT_DRAFT") {
            self.composer.update(cx, |s, cx| {
                s.set_value(text, window, cx);
                let end = s.value().len();
                s.set_selected_range(end..end, cx);
                s.focus(window, cx);
            });
        }
        if env("P2_AUTOPILOT_EMOJI_PANEL").is_some() {
            self.composer.update(cx, |s, cx| s.focus(window, cx));
            self.open_emoji(EmojiTarget::Composer, window, cx);
        }
        if let Some(query) = env("P2_AUTOPILOT_MENTION_POPUP") {
            self.composer.update(cx, |s, cx| {
                s.set_value(format!("@{query}"), window, cx);
                let end = s.value().len();
                s.set_selected_range(end..end, cx);
                s.focus(window, cx);
            });
        }
        if let Some(spec) = env("P2_AUTOPILOT_MESSAGE_MENU") {
            let (_, at) = spec.split_once('@').unwrap_or((spec.as_str(), "797,672"));
            let (x, y) = at.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))).unwrap_or((797., 672.));
            if let Some(id) = self.messages.last().map(|m| m.id.clone()) {
                self.open_menu(&id, x, y, y, false, cx);
            }
        }
        if env("P2_AUTOPILOT_HOVER").is_some() {
            self.hovered = self.messages.last().map(|m| m.id.clone());
        }
        if env("P2_AUTOPILOT_THREAD_VIEW").is_some() {
            if let Some(m) = self.messages.iter().rev().find(|m| m.count.replies > 0).cloned() {
                cx.emit(ChatEvent::OpenThread(m));
            }
        }
        // L ajanı: `P2_AUTOPILOT_LINK_BAR=1` bağlantı çubuğunu, `P2_AUTOPILOT_DROP_ZONE=1` bırakma katmanını açar.
        if env("P2_AUTOPILOT_LINK_BAR").is_some() {
            self.open_link_bar(window, cx);
        }
        if env("P2_AUTOPILOT_DROP_ZONE").is_some() {
            self.drag_over = true;
        }
        if let Some(kind) = env("P2_AUTOPILOT_TOOL_MENU") {
            self.open_tool_menu(match kind.as_str() { "members" => ToolMenu::Members, "notify" => ToolMenu::Notify, _ => ToolMenu::More }, window, cx);
        }
        crate::trace("parite: sohbet görünüm kancaları uygulandı");
        cx.notify();
    }

    /// Uçtan uca test kancaları (süreç başına bir kez; masaüstüne girdi göndermez).
    fn autopilot_extras(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let env = |k: &str| std::env::var(k).ok();
        // Grup üyeleri: ilk açılan sohbet grup olmayabilir; grup açılınca bir kez.
        if let (Some(_), ChatSubject::Group(g)) = (env("P2_AUTOPILOT_GROUP_MEMBERS"), &self.subject) {
            static GROUP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !GROUP.swap(true, std::sync::atomic::Ordering::SeqCst) {
                cx.emit(ChatEvent::ManageGroup(g.clone()));
            }
        }
        let any = ["P2_AUTOPILOT_REPLY", "P2_AUTOPILOT_ATTACH", "P2_AUTOPILOT_MENTION", "P2_AUTOPILOT_EMOJI", "P2_AUTOPILOT_THREAD", "P2_AUTOPILOT_FORWARD", "P2_AUTOPILOT_SEARCH", "P2_AUTOPILOT_HISTORY"].iter().any(|k| env(k).is_some());
        if !any || DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let last = self.messages.iter().rev().find(|m| m.is_text()).cloned();
        if let (Some(text), Some(m)) = (env("P2_AUTOPILOT_REPLY"), last.clone()) {
            self.start_reply(&m.id, window, cx);
            self.composer.update(cx, |s, cx| s.set_value(text, window, cx));
            self.submit(window, cx);
        }
        if let Some(spec) = env("P2_AUTOPILOT_ATTACH") {
            let (path, text) = spec.split_once('|').unwrap_or((spec.as_str(), ""));
            self.send_after_upload = Some(text.to_string());
            self.upload_paths(vec![path.into()], window, cx);
        }
        if let Some(query) = env("P2_AUTOPILOT_MENTION") {
            self.composer.update(cx, |s, cx| s.set_value(format!("@{query}"), window, cx));
            let names: Vec<String> = self.suggestions(cx).iter().filter_map(|s| if let Suggestion::Mention { value, .. } = s { Some(value.clone()) } else { None }).collect();
            crate::trace(&format!("sohbet: bahsetme önerileri {names:?}"));
            self.submit(window, cx); // ilk öneriyi seçer
            let value = self.composer.read(cx).value().to_string();
            self.composer.update(cx, |s, cx| s.set_value(format!("{value}bahsetme testi"), window, cx));
            self.submit(window, cx);
        }
        if let Some(text) = env("P2_AUTOPILOT_EMOJI") {
            self.composer.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.insert(format!("{text} "), window, cx);
            });
            self.open_emoji(EmojiTarget::Composer, window, cx);
            self.emoji_picked("🎉".into(), window, cx);
            self.emoji = None;
            self.submit(window, cx);
        }
        if let (Some(_), Some(m)) = (env("P2_AUTOPILOT_THREAD"), last.clone()) {
            cx.emit(ChatEvent::OpenThread(m));
        }
        if let (Some(_), Some(m)) = (env("P2_AUTOPILOT_FORWARD"), last) {
            cx.emit(ChatEvent::Forward(m.id));
        }
        if let Some(q) = env("P2_AUTOPILOT_SEARCH") {
            self.toggle_search(window, cx);
            if let Some(input) = &self.search {
                input.update(cx, |s, cx| s.set_value(q, window, cx));
            }
            self.run_search(window, cx);
        }
        if env("P2_AUTOPILOT_HISTORY").is_some() {
            if let Some(m) = self.messages.iter().rev().find(|m| m.edited_at.is_some()).cloned() {
                self.show_history(m.id, window, cx);
            }
        }
    }

    // ------------------------------------------------------------ yanıt, ekler, öneriler

    /// Bahsetme önerileri ve başlıktaki üye yığını için kişi listesi (çalışma alanı verir).
    pub fn set_members(&mut self, members: Vec<Member>) {
        self.members = members;
    }

    /// İş parçacığı panelinden gelen yanıt sayısı.
    pub fn set_replies(&mut self, parent: &str, replies: u32, cx: &mut Context<Self>) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == parent) {
            m.count.replies = replies;
            cx.notify();
        }
    }

    /// Gerçek zamanlı gelen yanıt: ana mesajın sayacı bir artar, son yanıt bilgisi yenilenir.
    pub fn bump_replies(&mut self, parent: &str, cx: &mut Context<Self>) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == parent) {
            m.count.replies += 1;
            cx.notify();
        }
    }

    pub fn start_reply(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.messages.iter().find(|m| m.id == id).cloned() else { return };
        crate::trace(&format!("sohbet: yanıtlanıyor «{}»", m.content.lines().next().unwrap_or_default()));
        self.replying = Some(m);
        self.editing = None;
        self.composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.replying = None;
        cx.notify();
    }

    /// Yazma kutusundaki öneriler: emoji kısa kodu, `@` etiketleri, `/` komutları (ChannelContent.tsx).
    pub(super) fn suggestions(&self, cx: &App) -> Vec<Suggestion> {
        if self.suggestions_closed {
            return Vec::new();
        }
        let draft = self.composer.read(cx).value().to_string();
        let mut out: Vec<Suggestion> = Vec::new();
        if let Some(q) = emoji::shortcode_query(&draft) {
            out.extend(emoji::suggest_shortcodes(&q).into_iter().map(|(k, e)| Suggestion::Emoji(k, e)));
        }
        if let Some(q) = extras::mention_query(&draft) {
            let needle = q.to_lowercase();
            let mut all: Vec<Suggestion> = vec![
                Suggestion::Mention { label: "@here".into(), description: p2_i18n::t("chat.mention.here"), value: "here".into(), avatar: "H".into() },
                Suggestion::Mention { label: "@everyone".into(), description: p2_i18n::t("chat.mention.everyone"), value: "everyone".into(), avatar: "E".into() },
            ];
            all.extend(self.user_groups.iter().map(|g| Suggestion::Mention {
                label: format!("@{}", g.handle),
                description: format!("{} · {}", g.name, p2_i18n::tv("chat.peopleCount", &[("n", &g.members.len().to_string())])),
                value: g.handle.clone(),
                avatar: view::tr_upper(&g.handle.chars().take(2).collect::<String>()),
            }));
            all.extend(self.members.iter().map(|m| Suggestion::Mention {
                label: format!("@{}", m.username),
                description: m.display_name.clone(),
                value: m.username.clone(),
                avatar: p2_ui::components::user_initials(&m.display_name),
            }));
            out.extend(all.into_iter().filter(|s| matches!(s, Suggestion::Mention { value, .. } if value.to_lowercase().starts_with(&needle))).take(8));
        }
        if let Some(q) = command_query(&draft) {
            out.extend(COMMANDS.iter().filter(|c| c.0.starts_with(&q.to_lowercase())).map(|c| Suggestion::Command(c.0, c.1)));
        }
        out
    }

    pub(super) fn pick_suggestion(&mut self, s: Suggestion, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.composer.read(cx).value().to_string();
        let next = match s {
            Suggestion::Emoji(_, e) => match value.rfind(':') {
                Some(at) => format!("{}{e} ", &value[..at]),
                None => format!("{value}{e} "),
            },
            Suggestion::Mention { value: v, .. } => extras::apply_mention(&value, &v),
            Suggestion::Command(name, _) => match value.rfind('/') {
                Some(at) => format!("{}/{name} ", &value[..at]),
                None => format!("{value}/{name} "),
            },
        };
        self.composer.update(cx, |s, cx| {
            s.set_value(next, window, cx);
            let end = s.value().len();
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
        self.suggestion_ix = 0;
        cx.notify();
    }

    pub fn insert_emoji(&mut self, emoji: &str, window: &mut Window, cx: &mut Context<Self>) {
        let emoji = emoji.to_string();
        self.composer.update(cx, |s, cx| {
            s.insert(emoji, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn pick_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(p2_i18n::t("chat.attachFile").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.upload_paths(paths, window, cx));
            }
        })
        .detach();
    }

    /// Dosyaları yükler; bitince yazma kutusunda ek olarak görünür.
    pub fn upload_paths(&mut self, paths: Vec<std::path::PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        for path in paths {
            let api = self.api.clone();
            self.uploading += 1;
            cx.notify();
            task::run(
                window,
                cx,
                move || {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dosya".into());
                    let data = std::fs::read(&path).map_err(|e| ApiError { message: p2_i18n::tv("chat.upload.readFailed", &[("name", &name), ("error", &e.to_string())]), ..Default::default() })?;
                    if data.len() > 250 * 1024 * 1024 {
                        return Err(ApiError { message: p2_i18n::tv("chat.upload.tooLarge", &[("name", &name)]), ..Default::default() });
                    }
                    api.upload_file(&name, &data)
                },
                |this, r, window, cx| {
                    this.uploading = this.uploading.saturating_sub(1);
                    match r {
                        Ok(file) => {
                            crate::trace(&format!("sohbet: ek yüklendi {} ({} bayt)", file.name, file.size));
                            this.attachments.push(file);
                        }
                        Err(e) => this.set_error(e, cx),
                    }
                    if this.uploading == 0 {
                        if let Some(text) = this.send_after_upload.take() {
                            this.composer.update(cx, |s, cx| s.set_value(text, window, cx));
                            this.submit(window, cx);
                        }
                    }
                    cx.notify();
                },
            );
        }
    }

    fn remove_attachment(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.attachments.len() {
            self.attachments.remove(ix);
            cx.notify();
        }
    }

    /// Resim önizlemesi ya da metin önizlemesi (ilk 100 000 karakter) için dosyayı indirir.
    fn load_file(&mut self, file: &FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let key = file.path();
        if self.images.contains_key(&key) {
            return;
        }
        self.images.insert(key.clone(), Slot::Loading);
        let api = self.api.clone();
        let path = key.clone();
        let text = !file.is_image();
        task::run(
            window,
            cx,
            move || {
                let bytes = api.get_bytes(&path).ok()?;
                if text {
                    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(100_000)]).into_owned();
                    Some(Slot::Text(s.chars().take(100_000).collect()))
                } else {
                    extras::decode_image(&bytes).map(|(i, w, h)| Slot::Ready(i, w, h))
                }
            },
            move |this, r, _, cx| {
                crate::trace(&format!("sohbet: {} önizlemesi {} {}", if text { "metin" } else { "resim" }, key, if r.is_some() { "hazır" } else { "yok" }));
                this.images.insert(key, r.unwrap_or(Slot::Failed));
                if std::env::var_os("P2_AUTOPILOT_VIEW").is_some() && this.viewer.is_none() {
                    if let Some(f) = this.messages.iter().flat_map(|m| extras::split_content(&m.content).1).find(|f| f.is_image() && matches!(this.images.get(&f.path()), Some(Slot::Ready(..)))) {
                        crate::trace("sohbet: görüntüleyici (otomatik)");
                        this.viewer = Some((f, 1.));
                    }
                }
                cx.notify();
            },
        );
    }

    pub(super) fn download(&mut self, file: FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(
            window,
            cx,
            move || {
                let bytes = api.get_bytes(&file.path())?;
                let target = extras::download_path(&file.name);
                if let Some(dir) = target.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::fs::write(&target, bytes).map_err(|e| ApiError { message: p2_i18n::tv("chat.saveFailed", &[("error", &e.to_string())]), ..Default::default() })?;
                Ok(target)
            },
            |this, r: Result<std::path::PathBuf, ApiError>, _, cx| match r {
                Ok(path) => {
                    crate::trace(&format!("sohbet: ek indirildi {}", path.display()));
                    // Başarı bildirimi (eskiden kırmızı hata kutusunda çıkıyordu).
                    crate::toasts::ok(p2_i18n::tv("chat.downloaded", &[("path", &path.display().to_string())]));
                    cx.notify();
                }
                Err(e) => this.set_error(e, cx),
            },
        );
    }

    // ------------------------------------------------------------ çizim

    fn render_call(&self, message: &Message, p: &Palette) -> AnyElement {
        let other = match &self.subject {
            ChatSubject::Direct(m) => m.display_name.clone(),
            _ => String::new(),
        };
        let (missed, text) = call_summary(message, message.author.id == self.me.id, &other);
        // call.css .call-log: done (yeşil) / missed (kırmızı) / ringing (vurgu) kartı.
        let (border, bg, icon_fg, text_fg) = match message.kind.as_deref() {
            Some("CALL_ENDED") => (p.success_border, p.success_soft, p.success_text, p.success_text),
            Some("CALL_MISSED") => (p.danger_border, p.danger_soft, p.danger_text, p.danger_text),
            _ => (p.border_accent, p.accent_soft, p.text_accent, p.text_accent),
        };
        let phone = icon("composer:call").stroke(1.8).el().size(px(14.)).text_color(icon_fg);
        let phone = if missed { phone.with_transformation(gpui_kit::Transformation::rotate(gpui_kit::radians(135f32.to_radians()))) } else { phone };
        div()
            .mt(px(4.))
            .mb(px(4.))
            .ml(px(6.))
            .mr(px(4.))
            .py(px(8.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(9.))
            .border_1()
            .border_color(border)
            .rounded(px(11.))
            .bg(bg)
            .child(div().size(px(26.)).flex().items_center().justify_center().rounded(px(8.)).bg(p.bg_3).child(phone))
            .child(div().text_size(fs(11.5)).css_weight(600.).text_color(text_fg).child(text))
            .child(div().ml_auto().text_size(fs(9.5)).text_color(p.text_3).child(view::tr_time(&message.created_at)))
            .into_any_element()
    }

    fn render_row(&mut self, ix: usize, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        crate::count_render("satır");
        let message = self.messages[ix].clone();
        let previous = ix.checked_sub(1).map(|i| &self.messages[i]);
        let new_day = previous.is_none_or(|prev| view::day_key(&prev.created_at) != view::day_key(&message.created_at));
        let last = ix + 1 == self.messages.len();
        // `.message-feed` iç boşluğu: 18 20 12.
        let wrap = |row: AnyElement| div().w_full().px(px(20.)).when(ix == 0, |s| s.pt(px(18.))).when(last, |s| s.pb(px(12.))).when(new_day, |s| s.child(view::day_divider(&message.created_at, p))).child(row).into_any_element();
        if !message.is_text() {
            // D ajanı: grup sistem satırı (GroupChat.tsx `.group-system`).
            let row = if message.kind.as_deref() == Some("SYSTEM") { self.group_system_row(&message, p) } else { self.render_call(&message, p) };
            return wrap(row);
        }
        // Önizlemeler: resimler ve metin dosyaları indirilir (önbellekte yoksa).
        let (visible, files) = extras::split_content(&message.content);
        for f in files {
            if f.is_image() || extras::preview_kind(&f.name, &f.mime) == extras::Kind::Text {
                self.load_file(&f, window, cx);
            } else if extras::preview_kind(&f.name, &f.mime) == extras::Kind::Pdf {
                self.load_pdf_thumb(&f, window, cx);
            }
        }
        for yt in media::youtube_ids(&visible) {
            self.load_youtube_thumb(&yt, window, cx);
        }
        let weak = cx.entity().downgrade();
        let mid = message.id.clone();
        let cb: view::RowCb = Rc::new(move |act, window, cx| {
            let _ = weak.update(cx, |this, cx| this.row_act(&mid, act, window, cx));
        });
        // Forum: konu kartları (`.forum-feed`, 22px iç boşluk, gün ayracı yok; üstte konu kutusu).
        if self.forum {
            let card = view::forum_card(&message, self.hovered.as_deref() == Some(message.id.as_str()), p, cb);
            return div().w_full().px(px(22.)).when(ix == 0, |s| s.pt(px(15.))).when(last, |s| s.pb(px(22.))).child(card).into_any_element();
        }
        let channel = matches!(self.target, ChatTarget::Channel { .. });
        let row = view::message_row(
            view::RowOpts {
                message: &message,
                kind: view::RowKind::Feed,
                hovered: self.hovered.as_deref() == Some(message.id.as_str()) && self.menu.is_none(),
                latest: self.latest.as_deref() == Some(message.id.as_str()),
                flash: self.flash.as_deref() == Some(message.id.as_str()),
                quote: true,
                thread: self.target.supports_threads() && (channel || matches!(self.target, ChatTarget::Group { .. })),
                images: &self.images,
                players: Some(&self.players),
                copied: self.copied.as_deref(),
                cb,
            },
            p,
        );
        let row = if ix >= self.first_live { div().w_full().child(row).p2_enter(("live", ix), Entrance::Rise, Duration::ZERO).into_any_element() } else { row };
        wrap(row)
    }

    fn header(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.subject {
            ChatSubject::Channel(channel) => self.channel_header(channel, p, cx),
            // D ajanı: DM/grup başlığı chat_people.rs.
            ChatSubject::Direct(person) => self.direct_header(person, p, cx),
            ChatSubject::Group(group) => self.group_header(group, p, cx),
        }
    }

    /// `.channel-welcome` (kanal) ya da DM/grup karşılaması.
    fn welcome(&self, p: &Palette) -> AnyElement {
        match &self.subject {
            // `.forum-intro` + "Henüz konu açılmadı."
            ChatSubject::Channel(c) if c.kind == ChannelType::Forum => div()
                .flex_1()
                .px(px(22.))
                .pt(px(15.))
                .pb(px(22.))
                .flex()
                .flex_col()
                .child(div().mb(px(5.)).text_size(fs(22.)).css_weight(700.).text_color(p.text_strong).child(c.name.clone()))
                .child(div().mb(px(20.)).text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("chat.forum.lead")))
                .child(div().text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("chat.forum.empty")))
                .into_any_element(),
            ChatSubject::Channel(c) => {
                let glyph: AnyElement = match c.kind {
                    ChannelType::Text => div().text_size(fs(28.)).child("#").into_any_element(),
                    kind => icon(match kind {
                        ChannelType::Announcement => "channel:announcement",
                        ChannelType::Voice => "channel:voice",
                        _ => "channel:forum",
                    })
                    .stroke(1.7)
                    .el()
                    .size(px(24.))
                    .text_color(p.text_accent)
                    .into_any_element(),
                };
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .px(px(24.))
                    .pb(px(36.))
                    .child(
                        div()
                            .size(px(54.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(18.))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.accent_soft)
                            .text_color(p.text_accent)
                            .shadow(p2_ui::tokens::shadows(&[p.accent_glow]))
                            .child(glyph),
                    )
                    .child(div().mt(px(14.)).mb(px(5.)).text_size(fs(24.)).css_weight(700.).text_color(p.text_strong).child(p2_i18n::tv("chat.welcome.title", &[("name", &c.name)])))
                    .child(div().text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("chat.welcome.body")))
                    .p2_enter("welcome", Entrance::Rise, Duration::ZERO)
                    .into_any_element()
            }
            // D ajanı: DM/grup karşılaması chat_people.rs.
            _ => self.people_welcome(p).unwrap_or_else(|| div().into_any_element()),
        }
    }
}

/// Sondaki `/komut` (ChannelContent.tsx `commandMatch`).
fn command_query(text: &str) -> Option<String> {
    let at = text.rfind('/')?;
    if text[..at].chars().last().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let q = &text[at + 1..];
    q.chars().all(|c| c.is_ascii_alphabetic()).then(|| q.to_string())
}


impl Render for ChatPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::count_render("sohbet");
        let p = cx.p2().clone();
        let palette = p.clone();
        let header = self.header(&p, cx);
        let content: AnyElement = if self.loading {
            div().flex_1().px(px(20.)).pt(px(18.)).flex().flex_col().justify_end().pb(px(12.)).child(div().text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("chat.loading"))).into_any_element()
        } else if self.messages.is_empty() {
            self.welcome(&p)
        } else {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .when(self.loading_older, |s| s.child(div().px(px(20.)).pt(px(18.)).text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("chat.loadingOlder"))))
                .child(list(self.list.clone(), cx.processor(move |this, ix, window, cx| this.render_row(ix, &palette, window, cx))).flex_1())
                .into_any_element()
        };
        // L ajanı: forum — açık konu akışın yerine; değilse üstte konu açma kutusu.
        let content: AnyElement = match (&self.forum_topic, self.forum) {
            (Some(topic), _) => div().flex_1().min_h_0().flex().flex_col().child(topic.clone()).into_any_element(),
            (None, true) if !self.loading => div().flex_1().min_h_0().flex().flex_col().child(self.topic_composer(&p, _window, cx)).child(content).into_any_element(),
            _ => content,
        };
        // Sürükle-bırak: sohbetin tamamı bırakma alanı; konum `drop_zone` katmanı için ölçülür.
        let probe = self.bounds.clone();
        div()
            .id("chat-pane")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .child(gpui_kit::canvas(move |b, _, _| probe.set(b), |_, _, _, _| {}).absolute().inset_0())
            .on_drag_move::<gpui_kit::ExternalPaths>(cx.listener(|this, e: &gpui_kit::DragMoveEvent<gpui_kit::ExternalPaths>, _, cx| {
                let inside = e.bounds.contains(&e.event.position);
                if inside && !this.drag_over {
                    this.drag_over = true;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(|this, paths: &gpui_kit::ExternalPaths, window, cx| this.drop_files(paths, window, cx)))
            .when(!self.embedded, |s| s.child(header))
            .child(content)
    }
}

impl ChatPane {
    /// I ajanı kancası: sesli oda görünümü kanal başlığını sohbetin dışında çizer.
    pub(crate) fn header_element(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        self.header(p, cx)
    }
}

impl ChatPane {
    /// Yazma kutusu (`.message-composer.slack-composer`). Sohbet görünümünün *dışında* (çalışma
    /// alanında, önbellekli sohbetin kardeşi olarak) çizilir: imleç yanıp söndükçe yalnız atalar
    /// kirlenir; içeride olsaydı her yanıp sönmede bütün görünür mesaj satırları yeniden çizilirdi.
    pub fn composer_bar(&self, p: &Palette, me: WeakEntity<ChatPane>, window: &Window, cx: &App) -> gpui_kit::Div {
        // Forum: alttaki yazma kutusu yok (konu kutusu akışın üstünde, yanıt kutusu konu görünümünde).
        if self.forum {
            return div();
        }
        let editing = self.editing.is_some();
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window) || self.emoji.as_ref().is_some_and(|e| matches!(e.1, EmojiTarget::Composer));
        let draft_empty = self.composer.read(cx).value().trim().is_empty();
        let typing = typing_label(&self.typists.iter().map(|t| t.1.as_str()).collect::<Vec<_>>());
        let suggestions = self.suggestions(cx);
        let esc = me.clone();
        let keys = me.clone();
        let count = suggestions.len();

        let mut form = div()
            .id("composer")
            .relative()
            .flex()
            .flex_col()
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border })
            .rounded(px(14.))
            .bg(p.glass.background())
            .shadow(if focused {
                let mut s = p.shadow_md.clone();
                s.push(p.ring);
                s.push(p.inset_top);
                p2_ui::tokens::shadows(&s)
            } else {
                let mut s = p.shadow_sm.clone();
                s.push(p.inset_top);
                p2_ui::tokens::shadows(&s)
            });

        // Alıntı / düzenleme şeridi (`.composer-reference`).
        let reference = |title: AnyElement, small: String, edit: bool, close: Box<dyn Fn(&mut Window, &mut App)>| {
            let (hover, strong) = (p.bg_hover, p.text_strong);
            div()
                .min_h(px(48.))
                .px(px(10.))
                .py(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .border_b_1()
                .border_color(p.border_subtle)
                .bg(p.accent_soft)
                .rounded_t(px(14.))
                .when(edit, |s| s.border_l(px(3.)).border_color(p.accent))
                .child(div().min_w_0().flex().flex_col().gap(px(3.)).text_size(fs(10.)).text_color(p.text_2).child(title).child(div().truncate().text_size(fs(9.)).text_color(p.text_3).child(small)))
                .child(
                    div()
                        .id("ref-close")
                        .flex_none()
                        .size(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_size(fs(17.))
                        .text_color(p.text_3)
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover).text_color(strong))
                        .child("×")
                        .on_click(move |_, window, cx| close(window, cx)),
                )
                .p2_enter("ref", Entrance::Rise, Duration::ZERO)
        };
        if let Some(r) = &self.replying {
            let who = me.clone();
            form = form.child(reference(
                div().flex().child(p2_i18n::t("chat.replyingTo.pre")).child(div().css_weight(700.).text_color(p.text_strong).child(r.author.display_name.clone())).child(p2_i18n::t("chat.replyingTo.post")).into_any_element(),
                r.content.split_whitespace().collect::<Vec<_>>().join(" "),
                false,
                Box::new(move |_, cx| {
                    let _ = who.update(cx, |this, cx| this.cancel_reply(cx));
                }),
            ));
        }
        if editing {
            let who = me.clone();
            form = form.child(reference(
                div().css_weight(700.).text_color(p.text_strong).child(p2_i18n::t("chat.editing")).into_any_element(),
                p2_i18n::t("chat.editingHint"),
                true,
                Box::new(move |window, cx| {
                    let _ = who.update(cx, |this, cx| this.cancel_edit(window, cx));
                }),
            ));
        }
        // "yazıyor" satırı (`.typing-line`).
        if !typing.is_empty() {
            let dot = |i: u32| div().size(px(4.)).rounded_full().bg(p.accent_hover).p2_enter(("td", i), Entrance::Fade, Duration::from_millis(140 * i as u64));
            form = form.child(div().pt(px(5.)).px(px(12.)).flex().items_center().gap(px(4.)).text_size(fs(10.)).italic().text_color(p.text_3).child(dot(0)).child(dot(1)).child(div().mr(px(4.)).child(dot(2))).child(typing));
        }
        // Ek kuyruğu (`.composer-attachments`).
        if !self.attachments.is_empty() || self.uploading > 0 {
            let mut chips = div().pt(px(8.)).px(px(10.)).pb(px(2.)).flex().flex_wrap().gap(px(7.));
            for (i, f) in self.attachments.iter().enumerate() {
                let who = me.clone();
                let kind = extras::preview_kind(&f.name, &f.mime);
                let tag = match kind {
                    extras::Kind::Pdf => "PDF".to_string(),
                    extras::Kind::Audio => p2_i18n::t("chat.tag.audio"),
                    _ => view::tr_upper(&f.name.rsplit('.').next().unwrap_or("DSY").chars().take(4).collect::<String>()),
                };
                let thumb: AnyElement = match (kind, self.images.get(&format!("/uploads/{}", f.url.rsplit("/uploads/").next().unwrap_or_default()))) {
                    (extras::Kind::Image, Some(Slot::Ready(img, ..))) => gpui_kit::img(gpui_kit::ImageSource::Render(img.clone())).size_full().object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
                    _ => tag.into_any_element(),
                };
                let (danger_soft, danger_text) = (p.danger_soft, p.danger_text);
                chips = chips.child(
                    div()
                        .max_w(px(260.))
                        .h(px(38.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(8.))
                        .bg(p.bg_3)
                        .child(div().flex_none().w(px(28.)).child(div().size(px(34.)).flex().items_center().justify_center().overflow_hidden().border_1().border_color(p.border_subtle).rounded(px(8.)).bg(p.bg_hover).text_size(fs(9.)).css_weight(750.).text_color(p.text_2).child(thumb)))
                        .child(div().min_w_0().truncate().text_size(fs(10.)).css_weight(700.).text_color(p.text).child(f.name.clone()))
                        .child(div().flex_none().text_size(fs(8.)).text_color(p.text_3).child(extras::format_size(f.size)))
                        .child(
                            div()
                                .id(SharedString::from(format!("att-x-{i}")))
                                .flex_none()
                                .size(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .text_color(p.text_3)
                                .cursor_pointer()
                                .hover(move |s| s.bg(danger_soft).text_color(danger_text))
                                .child("×")
                                .tooltip(chat_header::tip_owned(p2_i18n::t("chat.remove")))
                                .on_click(move |_, _, cx| {
                                    let _ = who.update(cx, |this, cx| this.remove_attachment(i, cx));
                                }),
                        ),
                );
            }
            if self.uploading > 0 {
                chips = chips.child(div().my(px(5.)).text_size(fs(10.)).text_color(p.text_3).child(p2_i18n::t("chat.uploading")));
            }
            form = form.child(chips);
        }

        // Giriş satırı: ek düğmesi (29px daire) + yazı alanı.
        let attach = me.clone();
        let (soft2, strong) = (p.accent_soft_2, p.text_strong);
        form = form.child(
            div()
                .min_h(px(43.)).py(px(4.5))
                .px(px(7.))
                .flex()
                .items_center()
                .child(
                    div().flex_none().w(px(35.)).child(
                        div()
                            .id("composer-attach")
                            .group("composer-attach")
                            .size(px(29.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(p.bg_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft2))
                            .child(icon("composer:plus").stroke(2.).el().size(px(15.)).text_color(p.text_2).group_hover("composer-attach", move |s| s.text_color(strong)))
                            .tooltip(chat_header::tip_owned(p2_i18n::t("chat.attachFile")))
                            .on_click(move |_, window, cx| {
                                let _ = attach.update(cx, |this, cx| this.pick_files(window, cx));
                            }),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .py(px(9.))
                        .text_color(p.text)
                        // XSmall: iç boşluk 4px yatay, 0 dikey (Electron .composer-editor 9px 4px).
                        .child({
                            let (tp, paste) = (p.clone(), me.clone());
                            Textarea::new(&self.composer)
                                .appearance(false)
                                .with_size(gpui_kit::component::Size::XSmall)
                                .text_size(fs(13.))
                                .line_height(px(16.0225))
                                .token(move |ctx, _, _| compose::token_element(ctx, &tp))
                                .on_paste(move |item, window, cx| paste.update(cx, |this, cx| this.paste(item, window, cx)).unwrap_or(false))
                        }),
                ),
        );

        // Araç çubuğu: B I bağlantı | emoji @ … gönder.
        let fmt = |id: &'static str, name: &'static str, stroke: f32, tip: String, active: bool| {
            let (soft, soft2, strong) = (p.accent_soft, p.accent_soft_2, p.text_strong);
            div()
                .id(id)
                .group(id)
                .w(px(27.))
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(active, |s| s.bg(soft2))
                .hover(move |s| s.bg(soft))
                .child(icon(name).stroke(stroke).el().size(px(15.)).text_color(if active { strong } else { p.text_3 }).group_hover(id, move |s| s.text_color(strong)))
                .tooltip(chat_header::tip_owned(tip))
        };
        let fmt_state = self.format_state(cx);
        let wrap_with = |f: compose::Fmt| {
            let who = me.clone();
            move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                let _ = who.update(cx, |this, cx| this.format(f, window, cx));
            }
        };
        let (link, emoji_btn, mention) = (me.clone(), me.clone(), me.clone());
        let emoji_open = self.emoji.as_ref().is_some_and(|e| matches!(e.1, EmojiTarget::Composer));
        let can_send = !self.sending && self.uploading == 0 && (!draft_empty || !self.attachments.is_empty());
        let send = me.clone();
        form = form.child(
            div()
                .h(px(34.))
                .px(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(p.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .child(fmt("fmt-bold", "composer:bold", 2.2, p2_i18n::t("chat.fmt.bold"), fmt_state.0).on_click(wrap_with(compose::Fmt::Bold)))
                        .child(fmt("fmt-italic", "composer:italic", 1.8, p2_i18n::t("chat.fmt.italic"), fmt_state.1).on_click(wrap_with(compose::Fmt::Italic)))
                        .child(fmt("fmt-link", "composer:link", 1.8, p2_i18n::t("chat.fmt.link"), fmt_state.2).on_click(move |_, window, cx| {
                            let _ = link.update(cx, |this, cx| this.open_link_bar(window, cx));
                        }))
                        .child(div().w(px(1.)).h(px(17.)).mx(px(4.)).bg(p.border_subtle))
                        .child(fmt("fmt-emoji", "composer:emoji", 1.8, p2_i18n::t("chat.fmt.emoji"), emoji_open).on_click(move |_, window, cx| {
                            let _ = emoji_btn.update(cx, |this, cx| {
                                if this.emoji.as_ref().is_some_and(|e| matches!(e.1, EmojiTarget::Composer)) {
                                    this.emoji = None;
                                    cx.notify();
                                } else {
                                    this.open_emoji(EmojiTarget::Composer, window, cx);
                                }
                            });
                        }))
                        .child(fmt("fmt-mention", "composer:mention", 1.8, p2_i18n::t("chat.fmt.mention"), false).on_click(move |_, window, cx| {
                            let _ = mention.update(cx, |this, cx| {
                                this.composer.update(cx, |s, cx| {
                                    s.insert("@", window, cx);
                                    s.focus(window, cx);
                                });
                            });
                        })),
                )
                .child(
                    div()
                        .id("composer-send")
                        .w(px(30.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .map(|s| if can_send { s.bg(p.accent_grad.background()).shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong])).cursor_pointer().hover(|s| s.top(px(-1.))) } else { s.bg(p.bg_3) })
                        .child(icon("composer:send").stroke(1.8).el().size(px(15.)).text_color(if can_send { gpui_kit::white() } else { p.text_4 }))
                        .on_click(move |_, window, cx| {
                            let _ = send.update(cx, |this, cx| this.submit(window, cx));
                        }),
                ),
        );

        // L ajanı: bağlantı çubuğu (kutunun üstünde).
        if let Some(bar) = self.link_bar_view(p, me.clone()) {
            form = form.child(bar);
        }
        // Öneriler (emoji kısa kodu, @etiket, /komut) ve emoji paneli: kutunun üstünde.
        if !suggestions.is_empty() {
            form = form.child(div().absolute().left(px(7.)).bottom_full().pb(px(7.)).child(self.suggestion_popup(&suggestions, p, me.clone())));
        }
        if let Some((panel, EmojiTarget::Composer)) = &self.emoji {
            let pick = me.clone();
            let pick: emoji::Pick = Rc::new(move |e, window, cx| {
                let _ = pick.update(cx, |this, cx| this.emoji_picked(e, window, cx));
            });
            form = form.child(
                div().absolute().left(px(-1.)).bottom_full().pb(px(8.)).child(
                    div()
                        .id("emoji-popover")
                        .w(px(330.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(14.))
                        .bg(p.bg_2)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .child(panel.render(p, pick))
                        .p2_enter("emoji-pop", Entrance::ScaleIn, Duration::ZERO),
                ),
            );
        }

        div()
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    let _ = esc.update(cx, |this, cx| this.escape(window, cx));
                }
            })
            .capture_key_down({
                let me = me.clone();
                move |event: &KeyDownEvent, window, cx| {
                    // Ctrl+B / Ctrl+I: canlı biçim.
                    let k = &event.keystroke;
                    if k.modifiers.control && !k.modifiers.alt && (k.key == "b" || k.key == "i") {
                        cx.stop_propagation();
                        let f = if k.key == "b" { compose::Fmt::Bold } else { compose::Fmt::Italic };
                        let _ = me.update(cx, |this, cx| this.format(f, window, cx));
                    }
                }
            })
            .on_drag_move::<gpui_kit::ExternalPaths>({
                let me = me.clone();
                move |_, _, cx| {
                    let _ = me.update(cx, |this, cx| {
                        if !this.drag_over {
                            this.drag_over = true;
                            cx.notify();
                        }
                    });
                }
            })
            .on_drop::<gpui_kit::ExternalPaths>({
                let me = me.clone();
                move |paths, window, cx| {
                    let _ = me.update(cx, |this, cx| this.drop_files(paths, window, cx));
                }
            })
            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                if count == 0 {
                    return;
                }
                let key = event.keystroke.key.as_str();
                if !matches!(key, "up" | "down" | "tab" | "escape") {
                    return;
                }
                cx.stop_propagation();
                let _ = keys.update(cx, |this, cx| {
                    match key {
                        "down" => this.suggestion_ix = (this.suggestion_ix.min(count - 1) + 1) % count,
                        "up" => this.suggestion_ix = (this.suggestion_ix.min(count - 1) + count - 1) % count,
                        "tab" => {
                            let items = this.suggestions(cx);
                            if let Some(s) = items.get(this.suggestion_ix.min(count - 1)).cloned() {
                                this.pick_suggestion(s, window, cx);
                            }
                        }
                        _ => this.suggestions_closed = true,
                    }
                    cx.notify();
                });
            })
            .px(px(16.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(form)
    }

    /// `.composer-suggestions` / `.shortcode-suggestions` kutusu.
    fn suggestion_popup(&self, items: &[Suggestion], p: &Palette, me: WeakEntity<ChatPane>) -> AnyElement {
        let active = self.suggestion_ix.min(items.len().saturating_sub(1));
        let emoji_only = items.iter().all(|s| matches!(s, Suggestion::Emoji(..)));
        let head = |title: String| {
            div()
                .h(px(31.))
                .px(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .text_color(p.text_accent)
                .child(div().text_size(fs(9.5)).css_weight(700.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).child(view::tr_upper(&title)))
                .child(div().text_size(fs(8.)).text_color(p.text_4).child(p2_i18n::t("chat.suggest.hint")))
        };
        let mut pop = div()
            .id("suggestions")
            .w(px(if emoji_only { 310. } else { 330. }))
            .max_h(px(350.))
            .overflow_y_scroll()
            .p(px(6.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(11.))
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude();
        let mut last_kind = 0u8;
        for (i, s) in items.iter().enumerate() {
            let kind = match s {
                Suggestion::Emoji(..) => 1,
                Suggestion::Mention { .. } => 2,
                Suggestion::Command(..) => 3,
            };
            if kind != last_kind {
                pop = pop.child(head(match kind {
                    1 => p2_i18n::t("chat.suggest.emoji"),
                    2 => p2_i18n::t("chat.suggest.mentions"),
                    _ => p2_i18n::t("chat.suggest.commands"),
                }));
                last_kind = kind;
            }
            let on = i == active;
            let (soft, soft2, accent_hover) = (p.accent_soft, p.accent_soft_2, p.accent_hover);
            let (who, hover_who, item) = (me.clone(), me.clone(), s.clone());
            let row = div()
                .id(SharedString::from(format!("sg-{i}")))
                .w_full()
                .flex_none()
                .h(px(if kind == 1 { 38. } else { 43. }))
                .px(px(7.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .text_color(if on { p.text_strong } else { p.text })
                .cursor_pointer()
                .when(on, |s| s.bg(soft2).shadow(p2_ui::tokens::shadows(&[p2_ui::tokens::Shadow { x: 2., y: 0., blur: 0., spread: 0., color: accent_hover, inset: true }])))
                .when(!on, |s| s.hover(move |s| s.bg(soft)))
                .on_hover(move |over: &bool, _, cx| {
                    if *over {
                        let _ = hover_who.update(cx, |this, cx| {
                            if this.suggestion_ix != i {
                                this.suggestion_ix = i;
                                cx.notify();
                            }
                        });
                    }
                })
                .on_mouse_down(gpui_kit::MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    let _ = who.update(cx, |this, cx| this.pick_suggestion(item.clone(), window, cx));
                });
            pop = pop.child(match s {
                Suggestion::Emoji(code, e) => row.child(div().w(px(34.)).font_family(FONT_EMOJI).text_size(fs(21.)).child(e.to_string())).child(div().font_family(FONT_MONO).text_size(px(11.)).child(format!(":{code}:"))),
                Suggestion::Mention { label, description, avatar, .. } => row
                    .child(div().w(px(34.)).child(div().size(px(27.)).flex().items_center().justify_center().rounded(px(8.)).bg(p.bg_selected).text_color(p.text_strong).text_size(fs(10.)).css_weight(800.).child(p2_ui::components::initials_label(avatar.clone()))))
                    .child(div().flex().flex_col().gap(px(3.)).child(div().text_size(fs(11.)).css_weight(700.).child(label.clone())).child(div().text_size(fs(9.)).text_color(p.text_3).child(description.clone()))),
                Suggestion::Command(name, desc) => row
                    .child(div().w(px(34.)).child(div().size(px(27.)).flex().items_center().justify_center().rounded(px(8.)).bg(p.bg_selected).text_color(p.text_strong).text_size(fs(10.)).css_weight(800.).child("/")))
                    .child(div().flex().flex_col().gap(px(3.)).child(div().text_size(fs(11.)).css_weight(700.).child(format!("/{name}"))).child(div().text_size(fs(9.)).text_color(p.text_3).child(p2_i18n::t(desc)))),
            });
        }
        pop.p2_enter("sugg", Entrance::ScaleIn, Duration::ZERO).into_any_element()
    }

    /// Seçimi (yoksa imleci) `marker` ile sarar — Electron'da execCommand('bold'/'italic').
    fn wrap_selection(&mut self, marker: &str, window: &mut Window, cx: &mut Context<Self>) {
        let marker = marker.to_string();
        self.composer.update(cx, |s, cx| {
            let value = s.value().to_string();
            let range = s.selected_range();
            let (a, b) = (range.start.min(value.len()), range.end.min(value.len()));
            let next = format!("{}{marker}{}{marker}{}", &value[..a], &value[a..b], &value[b..]);
            s.set_value(next, window, cx);
            let end = s.value().len();
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn insert_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |s, cx| {
            s.insert(format!("[{}](https://)", p2_i18n::t("chat.linkText")), window, cx);
            s.focus(window, cx);
        });
    }

    /// Esc sırası (ChannelContent.tsx `escape`).
    pub(super) fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.link_bar.is_some() {
            self.close_link_bar(window, cx);
        } else if self.menu.take().is_some() || self.tool_menu.take().is_some() {
        } else if self.emoji.take().is_some() {
        } else if self.viewer.take().is_some() {
        } else if self.search.is_some() {
            self.search = None;
            self.search_hits = None;
        } else if self.dialog.take().is_some() {
        } else if self.editing.is_some() {
            self.cancel_edit(window, cx);
        } else if self.replying.is_some() {
            self.replying = None;
        } else if self.confirm_leave {
            self.confirm_leave = false;
        } else {
            return;
        }
        cx.notify();
    }
}

// ---------------------------------------------------------------- geçmiş, arama

impl ChatPane {
    pub fn show_history(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::History(Vec::new(), true));
        cx.notify();
        let (api, target, _) = self.base();
        task::run(window, cx, move || api.message_history(&target, &id), |this, r, _, cx| match r {
            Ok(rows) => {
                crate::trace(&format!("sohbet: düzenleme geçmişi {} sürüm", rows.len()));
                if matches!(this.dialog, Some(Dialog::History(..))) {
                    this.dialog = Some(Dialog::History(rows, false));
                }
                cx.notify();
            }
            Err(_) => {
                this.dialog = None;
                this.set_error(ApiError { message: p2_i18n::t("chat.history.loadFailed"), ..Default::default() }, cx);
            }
        });
    }

    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tool_menu = None;
        if self.search.take().is_some() {
            self.search_hits = None;
            cx.notify();
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("chat.search.placeholder")));
        input.update(cx, |s, cx| s.focus(window, cx));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
            // Electron: yazdıkça 280 ms sonra arar (Enter gerekmez).
            InputEvent::Change => {
                this.search_epoch += 1;
                let epoch = this.search_epoch;
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(280)).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.search_epoch == epoch {
                            this.run_search(window, cx);
                        }
                    });
                })
                .detach();
            }
            InputEvent::PressEnter { .. } => this.run_search(window, cx),
            _ => {}
        });
        self._subs.push(sub);
        self.search = Some(input);
        self.search_hits = None;
        cx.notify();
    }

    pub fn run_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = &self.search else { return };
        let q = input.read(cx).value().trim().to_string();
        if q.chars().count() < 2 {
            self.search_hits = None;
            self.searching = false;
            cx.notify();
            return;
        }
        self.searching = true;
        cx.notify();
        let (api, target, _) = self.base();
        task::run(window, cx, move || api.search_channel(&target, &q).map(|h| (q, h)), |this, r, _, cx| {
            this.searching = false;
            match r {
                Ok((q, hits)) => {
                    crate::trace(&format!("sohbet: arama «{q}» {} sonuç", hits.len()));
                    this.search_hits = Some(hits);
                }
                Err(_) => this.search_hits = Some(Vec::new()),
            }
            cx.notify();
        });
    }

    /// Aramadan "mesaja git": akışta mesaja kaydırır ve 2,8 sn vurgular; yanıtsa kök iş parçacığı açılır.
    pub(super) fn jump_to(&mut self, hit: p2_client_core::SearchHit, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.search_hits = None;
        let target_id = hit.parent_id.clone().unwrap_or(hit.id.clone());
        if let Some(parent) = &hit.parent_id {
            if let Some(root) = self.messages.iter().find(|m| &m.id == parent).cloned() {
                cx.emit(ChatEvent::OpenThread(root));
            }
        }
        if let Some(ix) = self.messages.iter().position(|m| m.id == target_id) {
            self.list.scroll_to_reveal_item(ix);
        }
        self.flash = Some(target_id.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2800)).await;
            let _ = this.update(cx, |this, cx| {
                if this.flash.as_deref() == Some(target_id.as_str()) {
                    this.flash = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn open_tool_menu(&mut self, kind: ToolMenu, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.emoji = None;
        if self.tool_menu == Some(kind) && kind != ToolMenu::Notify {
            self.tool_menu = None;
        } else {
            self.tool_menu = Some(kind);
            if kind == ToolMenu::Members {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("chat.searchPeople")));
                input.update(cx, |s, cx| s.focus(window, cx));
                let sub = cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify());
                self._subs.push(sub);
                self.members_search = Some(input);
            }
        }
        cx.notify();
    }

    pub(super) fn me_id(&self) -> &str {
        &self.me.id
    }

    pub(super) fn members(&self) -> &[Member] {
        &self.members
    }

    pub(super) fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// L ajanı: silme yetkileri (ChannelContent `canDeleteAny`: kanal izni `delete_any` ya da
    /// `delete_unlimited`; DM/grup yalnız kuruluş geneli `delete_unlimited`).
    /// L ajanı: forumda açık konu (akışın yerine çizilir).
    pub fn set_forum_topic(&mut self, topic: Option<Entity<super::thread::ThreadPane>>, cx: &mut Context<Self>) {
        self.forum_topic = topic;
        cx.notify();
    }

    /// `.forum-topic-composer`: 39px kutu + "Konu aç" (88px, vurgu gradyanı); akışın üstünde yapışkan.
    fn topic_composer(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(input) = &self.topic_input else { return div().into_any_element() };
        let empty = input.read(cx).value().trim().is_empty();
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        let disabled = empty || self.sending;
        div()
            .flex_none()
            .pt(px(18.))
            .px(px(18.))
            .child(
                div()
                    .p(px(4.))
                    .flex()
                    .gap(px(7.))
                    .bg(p.bg_0)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(39.))
                            .px(px(11.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(if focused { p.border_accent } else { p.border })
                            .rounded(px(8.))
                            .bg(p.bg_3)
                            .when(focused, |s| s.shadow(p2_ui::tokens::shadows(&[p.ring])))
                            .text_size(fs(16.))
                            .text_color(p.text)
                            .child(gpui_kit::component::input::Input::new(input).appearance(false).with_size(gpui_kit::component::Size::XSmall).text_size(fs(16.))),
                    )
                    .child(
                        div()
                            .id("topic-send")
                            .flex_none()
                            .min_w(px(88.))
                            .h(px(39.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .bg(p.accent_grad.background())
                            .shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong]))
                            .text_size(fs(16.))
                            .css_weight(650.)
                            .text_color(p.text_on_accent)
                            .when(disabled, |s| s.opacity(0.5))
                            .when(!disabled, |s| s.cursor_pointer())
                            .child(if self.sending { "…".to_string() } else { p2_i18n::t("chat.forum.open") })
                            .on_click(cx.listener(|this, _, window, cx| this.send_topic(window, cx))),
                    ),
            )
            .into_any_element()
    }

    /// `sendTopic`: başlık yeni üst düzey mesaj olur.
    fn send_topic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.topic_input.clone() else { return };
        let title = input.read(cx).value().trim().to_string();
        if title.is_empty() || self.sending {
            return;
        }
        self.sending = true;
        let (api, target, me) = self.base();
        task::run(window, cx, move || api.send_message(&target, &title, None, &me), move |this, r, window, cx| {
            this.sending = false;
            match r {
                Ok(m) => {
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                    this.on_created(m, window, cx);
                }
                Err(e) => this.set_error(e, cx),
            }
            cx.notify();
        });
    }

    /// Silme onayı (kanal) — iş parçacığı panelinden de.
    pub fn ask_delete(&mut self, m: Message, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Delete(m));
        cx.notify();
    }

    pub fn apply_perms(&mut self, perms: &p2_client_core::MyPermissions) {
        let (any, unlimited) = match &self.subject {
            ChatSubject::Channel(c) => (perms.can_in(&c.id, "message.delete_any") || perms.can_in(&c.id, "message.delete_unlimited"), perms.can_in(&c.id, "message.delete_unlimited")),
            _ => (perms.can("message.delete_unlimited"), perms.can("message.delete_unlimited")),
        };
        self.can_delete_any = any;
        self.can_delete_unlimited = unlimited;
    }

    pub(super) fn images_mut(&mut self) -> &mut ImageCache {
        &mut self.images
    }

    pub(super) fn api_arc(&self) -> Arc<Api> {
        self.api.clone()
    }

    pub(super) fn images(&self) -> &ImageCache {
        &self.images
    }

    pub(super) fn error(&self) -> Option<&(String, usize)> {
        self.error.as_ref()
    }

    pub(super) fn start_edit_pub(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.start_edit(id, window, cx)
    }

    pub(super) fn api(&self) -> (Arc<Api>, ChatTarget) {
        (self.api.clone(), self.target.clone())
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_line() {
        assert_eq!(typing_text(&[]), "");
        assert_eq!(typing_text(&["Cem"]), "Cem yazıyor…");
        assert_eq!(typing_text(&["Cem", "Ada"]), "Cem ve Ada yazıyor…");
        assert_eq!(typing_text(&["Cem", "Ada", "Efe"]), "3 kişi yazıyor…");
    }

    #[test]
    fn slash_and_commands() {
        assert_eq!(expand_slash("/shrug olur"), "olur ¯\\_(ツ)_/¯");
        assert_eq!(expand_slash("/me koşar"), "*koşar*");
        assert_eq!(command_query("selam /sh"), Some("sh".into()));
        assert_eq!(command_query("a/b"), None);
    }
}
