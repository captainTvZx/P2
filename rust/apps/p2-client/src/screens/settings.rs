//! Ayarlar penceresi — `src/SettingsModal.tsx` + `src/settings.css` birebir karşılığı.
//! Kabuk (katman, 880×640 cam kart, sol gezinti, başlık, kaydırma alanı ve kaydırma çubuğu) burada;
//! bölümler alt modüllerde: Ses ve Görüntü (`settings_media`), Görünüm + Dil ve Bölge (`settings_look`),
//! Bildirimler + Uygulama (`settings_app`), Kütüphane + Uygunluk (`settings_library`); Hesap ve Güvenlik
//! ve Kısayollar bu dosyada. Ortak denetimler `settings_ui`.

use std::collections::HashMap;
use p2_ui::P2Fx as _; // backdrop_blur
use p2_ui::P2Transition as _; // kendi kaydırma çubuğu var (otomatik çubuk kapalı)
use std::sync::Arc;

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, FontWeight, ScrollHandle, SharedString, WeakEntity, Window, div, point, px, relative};
use p2_client_core::store;
use p2_client_core::{Api, TwoFactorSetup, User};
use p2_ui::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, Shadow, css_rgba, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::text_input;
use super::workspace::PRESENCE_OPTIONS;
use crate::app::ClientApp;
use crate::call::CallController;
use crate::task;

#[path = "settings_app.rs"]
mod settings_app;
#[path = "settings_library.rs"]
mod settings_library;
#[path = "settings_look.rs"]
pub mod settings_look;
#[path = "settings_media.rs"]
mod settings_media;
#[path = "settings_ui.rs"]
pub mod settings_ui;
#[path = "settings_qr.rs"]
mod settings_qr;
#[path = "settings_crop.rs"]
mod settings_crop;
use settings_media::MediaState;
use settings_ui::{Btn, SliderRegistry, block, button, field, hint, last_block, row, text};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Account,
    Appearance,
    Language,
    Shortcuts,
    Media,
    Notifications,
    App,
    Library,
    Availability,
}

/// SettingsModal.tsx `sections` sırası; başlık/ipucu `sec.<id>` / `sec.<id>.hint` çevirileri.
const SECTIONS: &[(Section, &str)] = &[
    (Section::Media, "media"),
    (Section::Appearance, "appearance"),
    (Section::Language, "language"),
    (Section::Notifications, "notifications"),
    (Section::App, "app"),
    (Section::Library, "library"),
    (Section::Account, "account"),
    (Section::Availability, "availability"),
    (Section::Shortcuts, "shortcuts"),
];

/// SettingsModal.tsx `shortcuts` (tuşlar) + `keys.*` çevirileri.
const SHORTCUTS: [(&str, &str); 6] = [("Ctrl + ,", "keys.settings"), ("Esc", "keys.escape"), ("Enter", "keys.send"), ("Shift + Enter", "keys.newline"), ("Ctrl + B / Ctrl + I", "keys.format"), ("↑ ↓", "keys.arrows")];

pub enum SettingsEvent {
    Close,
    ProfileSaved(User),
    Presence(String),
    /// Kütüphane/overlay/uygunluk tercihleri değişti: çalışma alanı uygular.
    PrefsChanged,
    /// Kütüphane kurulum konumu seçildi.
    LibraryRoot(String),
}

pub struct SettingsModal {
    api: Arc<Api>,
    app: WeakEntity<ClientApp>,
    user: User,
    preference: String,
    section: Section,
    name: Entity<InputState>,
    title: Entity<InputState>,
    code: Entity<InputState>,
    saving: bool,
    /// Görünen/kaydedilecek fotoğraf: `None` = değişmedi (hesaptaki), `Some("")` = kaldırılacak,
    /// `Some(yol)` = seçilen yerel dosya (Profili kaydet ile yüklenir).
    avatar_draft: Option<String>,
    profile_notice: String,
    /// Açık kırpma penceresi (AvatarCropper).
    crop: Option<settings_crop::Crop>,
    setup: Option<TwoFactorSetup>,
    qr: Option<Arc<gpui_kit::Image>>,
    disabling: bool,
    two_factor_busy: bool,
    twofa_error: String,
    twofa_notice: String,
    error: Option<(String, usize)>,
    calls: Entity<CallController>,
    media: MediaState,
    /// Overlay kısayolu yakalanıyor (ayar anahtarı).
    pub capturing_hotkey: Option<&'static str>,
    /// Kütüphane kökü (göstermek için; çalışma alanı verir).
    pub library_root: String,
    library_free: Option<u64>,
    work_start: Option<Entity<InputState>>,
    work_end: Option<Entity<InputState>>,
    server_addr: Option<Entity<InputState>>,
    custom_name: Option<Entity<InputState>>,
    prefs_subs: Vec<gpui_kit::Subscription>,
    /// Bas-konuş tuşu yakalanırken klavye bu pencereye gelsin diye.
    focus: gpui_kit::FocusHandle,
    /// Anahtar (toggle) topuz canlandırması: her tıklamada artar.
    epochs: HashMap<&'static str, usize>,
    pub(crate) sliders: SliderRegistry<SettingsModal>,
    /// Açık seçim listesi (select).
    open_select: Option<&'static str>,
    scroll: ScrollHandle,
    overflow: bool,
    /// Kaydırma alanının içerik genişliği (açılır listelerin genişliği).
    content_w: f32,
    /// Otomatik pilot kaydırması: içerik (zaman uyumsuz bloklar dahil) yerleşince uygulanır.
    pending_scroll: Option<f32>,
    pending_frames: u32,
    /// Son karede ölçülen kaydırma tavanı: tavan değiştikçe sayaç sıfırlanır (geç gelen bloklar).
    pending_max: f32,
    look: settings_look::LookState,
    server: settings_app::ServerState,
    _calls_sub: gpui_kit::Subscription,
}

impl EventEmitter<SettingsEvent> for SettingsModal {}

fn sliders(this: &mut SettingsModal) -> &mut SliderRegistry<SettingsModal> {
    &mut this.sliders
}

impl SettingsModal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(api: Arc<Api>, app: WeakEntity<ClientApp>, calls: Entity<CallController>, user: User, preference: String, section: Section, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = text_input(window, cx, "");
        let title = text_input(window, cx, "Senior Game Developer");
        let code = text_input(window, cx, "123456");
        let (n, t) = (user.display_name.clone(), user.title.clone().unwrap_or_default());
        name.update(cx, |s, cx| s.set_value(n, window, cx));
        title.update(cx, |s, cx| s.set_value(t, window, cx));
        // Mikrofon testi sırasında seviye çubuğu canlı kalsın.
        let calls_sub = cx.observe(&calls, |_, _, cx| cx.notify());
        let mut modal = Self {
            api,
            app,
            user,
            preference,
            section,
            name,
            title,
            code,
            saving: false,
            avatar_draft: None,
            profile_notice: String::new(),
            crop: None,
            setup: None,
            qr: None,
            disabling: false,
            two_factor_busy: false,
            twofa_error: String::new(),
            twofa_notice: String::new(),
            error: None,
            calls,
            media: MediaState::default(),
            capturing_hotkey: None,
            library_root: String::new(),
            library_free: None,
            work_start: None,
            work_end: None,
            server_addr: None,
            custom_name: None,
            prefs_subs: Vec::new(),
            focus: cx.focus_handle(),
            epochs: HashMap::new(),
            sliders: SliderRegistry::default(),
            open_select: None,
            scroll: ScrollHandle::new(),
            overflow: false,
            content_w: 598.,
            pending_scroll: None,
            pending_frames: 0,
            pending_max: -1.,
            look: settings_look::LookState::default(),
            server: settings_app::ServerState::default(),
            _calls_sub: calls_sub,
        };
        modal.look.custom_tab = modal.app.upgrade().is_some_and(|a| a.read(cx).settings.theme.as_deref() == Some("custom"));
        // Otomatik pilot: profil fotoğrafı yükle ("kaldır" ise kaldır).
        if let Ok(path) = std::env::var("P2_AUTOPILOT_AVATAR") {
            if path == "kaldir" {
                modal.set_avatar(None, window, cx);
            } else {
                modal.upload_avatar(path.into(), window, cx);
            }
        }
        // Otomatik pilot (görünüm kancaları, veri yazmaz): `P2_AUTOPILOT_SETTINGS_VIEW` =
        // `custom` (Görünüm > Özel tema sekmesi), `select:<ad>` (seçim listesi açık), `twofa` (2FA kurulum
        // adımı), `voicekey` (bas-konuş tuşu bekleniyor), `scroll:<px>` (kaydırma).
        if let Ok(view) = std::env::var("P2_AUTOPILOT_SETTINGS_VIEW") {
            for step in view.split(';').filter(|s| !s.is_empty()) {
                let (verb, arg) = step.split_once(':').unwrap_or((step, ""));
                match verb {
                    "custom" => modal.look.custom_tab = true,
                    "select" => modal.open_select = settings_ui::select_name(arg),
                    "voicekey" => {
                        modal.media.capturing_key = true;
                        window.focus(&modal.focus, cx);
                    }
                    "twofa" => modal.start_two_factor(window, cx),
                    "scroll" => {
                        modal.pending_scroll = arg.parse::<f32>().ok();
                    }
                    _ => {}
                }
            }
        }
        // Otomatik pilot: tercih adımları (`;`): hotkey:ad:değer, capture:ad, bandwidth:N, overlay:on|off,
        // hud:on|off, corner:..., days:1,2,3, start:SS:DD, end:SS:DD, outside:none|AWAY|BUSY.
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_PREFS") {
            for step in spec.split(';').filter(|s| !s.is_empty()) {
                let Some((verb, arg)) = step.split_once(':') else { continue };
                let arg = arg.to_string();
                match verb {
                    "hotkey" => {
                        if let Some((name, value)) = arg.split_once(':') {
                            modal.set_hotkey(name, value.to_string(), cx);
                        }
                    }
                    "capture" => {
                        modal.capturing_hotkey = settings_library_names().into_iter().find(|n| *n == arg);
                        window.focus(&modal.focus, cx);
                        crate::trace(&format!("ayarlar: kısayol bekleniyor ({arg})"));
                    }
                    "bandwidth" => {
                        let v = arg.parse().unwrap_or(0);
                        modal.update_prefs(move |l, _, _| l.bandwidth_mbps = v, cx);
                    }
                    "overlay" => modal.update_prefs(move |l, _, _| l.overlay = arg == "on", cx),
                    "hud" => modal.update_prefs(move |_, o, _| o.hud = arg == "on", cx),
                    "corner" => modal.update_prefs(move |_, o, _| o.toast_corner = arg, cx),
                    "days" => modal.update_prefs(move |_, _, a| a.work_days = arg.split(',').filter_map(|d| d.parse().ok()).collect(), cx),
                    "start" => modal.update_prefs(move |_, _, a| a.work_start = arg, cx),
                    "end" => modal.update_prefs(move |_, _, a| a.work_end = arg, cx),
                    "outside" => modal.update_prefs(move |_, _, a| a.outside_hours_status = arg, cx),
                    _ => {}
                }
            }
        }
        modal
    }

    /// Anahtarın canlandırma sayacı.
    pub(crate) fn epoch(&self, key: &'static str) -> usize {
        self.epochs.get(key).copied().unwrap_or(0)
    }

    pub(crate) fn bump(&mut self, key: &'static str) {
        *self.epochs.entry(key).or_insert(0) += 1;
    }

    /// Otomatik pilot: görünen adı doldur.
    pub fn set_display_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.to_string();
        self.name.update(cx, |s, cx| s.set_value(name, window, cx));
    }

    pub fn set_preference(&mut self, preference: &str, cx: &mut Context<Self>) {
        self.preference = preference.to_string();
        cx.notify();
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message, epoch));
        cx.notify();
    }

    /// "Profili kaydet": ad + unvan (+ seçilen/kaldırılan fotoğraf). SettingsModal `ProfileEditor.save`.
    pub fn save_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let (name, title) = (super::value(&self.name, cx), super::value(&self.title, cx));
        if name.trim().chars().count() < 2 {
            return;
        }
        self.saving = true;
        self.profile_notice.clear();
        cx.notify();
        let api = self.api.clone();
        let draft = self.avatar_draft.clone();
        task::run(
            window,
            cx,
            move || {
                let mut user = api.update_profile(&name, &title).map_err(|e| e.message)?;
                match draft.as_deref() {
                    Some("") => user = api.set_avatar(None).map_err(|e| e.message)?,
                    Some(path) => user = upload_avatar_file(&api, std::path::Path::new(path))?,
                    None => {}
                }
                Ok::<User, String>(user)
            },
            |this, result, _, cx| {
                this.saving = false;
                match result {
                    Ok(user) => {
                        crate::trace(&format!("ayarlar: profil kaydedildi ({})", user.display_name));
                        crate::avatars::remember(&user.id, user.avatar_url.as_deref());
                        this.user = User { two_factor_enabled: this.user.two_factor_enabled, ..user.clone() };
                        this.avatar_draft = None;
                        this.profile_notice = "Profil kaydedildi.".into();
                        cx.emit(SettingsEvent::ProfileSaved(this.user.clone()));
                    }
                    Err(e) => this.profile_notice = if e.is_empty() { "Profil kaydedilemedi.".into() } else { e },
                }
                cx.notify();
            },
        );
    }

    fn start_two_factor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.two_factor_busy = true;
        self.twofa_error.clear();
        self.twofa_notice.clear();
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || api.two_factor_setup(), |this, result, _, cx| {
            this.two_factor_busy = false;
            match result {
                Ok(setup) => {
                    // Karekod çevrimdışı üretilir; hiçbir dış servise anahtar gönderilmez.
                    this.qr = settings_qr::svg(&setup.otpauth).map(|svg| Arc::new(gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Svg, svg.into_bytes())));
                    this.setup = Some(setup);
                }
                Err(e) => this.twofa_error = if e.message.is_empty() { "Kurulum başlatılamadı.".into() } else { e.message },
            }
            cx.notify();
        });
    }

    fn confirm_two_factor(&mut self, enable: bool, window: &mut Window, cx: &mut Context<Self>) {
        let code: String = super::value(&self.code, cx).chars().filter(char::is_ascii_digit).collect();
        if code.len() != 6 || self.two_factor_busy {
            return;
        }
        self.two_factor_busy = true;
        self.twofa_error.clear();
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || if enable { api.two_factor_enable(&code) } else { api.two_factor_disable(&code) }, move |this, result, window, cx| {
            this.two_factor_busy = false;
            match result {
                Ok(()) => {
                    this.user.two_factor_enabled = enable;
                    this.setup = None;
                    this.disabling = false;
                    this.code.update(cx, |s, cx| s.set_value("", window, cx));
                    this.twofa_notice = if enable { "İki adımlı doğrulama açıldı." } else { "İki adımlı doğrulama kapatıldı." }.into();
                    cx.emit(SettingsEvent::ProfileSaved(this.user.clone()));
                }
                Err(e) => this.twofa_error = if e.message.is_empty() { "Kod doğrulanamadı.".into() } else { e.message },
            }
            cx.notify();
        });
    }

    /// Eski açık/koyu çağrısı (otomatik pilot `P2_AUTOPILOT_THEME`): "dark" → P2 Mavi, "light" → Kâğıt.
    pub fn set_theme(&mut self, dark: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(if dark { "dark" } else { "light" }, cx);
    }

    pub fn open(&mut self, section: Section, cx: &mut Context<Self>) {
        // Electron: .settings-scroll aynı öğe kalır; kaydırma konumu bölüm değişince korunur (taşmaya göre kırpılır).
        self.section = section;
        self.open_select = None;
        self.error = None;
        cx.notify();
    }

    // ------------------------------------------------------------ Hesap ve Güvenlik

    fn account(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let name = super::value(&self.name, cx);
        // Profil fotoğrafı (86×86, köşe 26, gölge) ya da kimlik tonlu baş harfler.
        let hue = p.hues[p2_ui::components::avatar_hue(&self.user.id) % 10];
        let photo_box = || div().size(px(86.)).rounded(px(26.)).shadow(p.shadow_md()).flex().items_center().justify_center().overflow_hidden();
        let photo: AnyElement = match self.avatar_draft.as_deref() {
            Some("") => None,
            Some(path) => Some(gpui_kit::img(std::path::PathBuf::from(path)).size(px(86.)).rounded(px(26.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element()),
            None => crate::avatars::image(&self.user.id).map(|i| gpui_kit::img(gpui_kit::ImageSource::Render(i)).size(px(86.)).rounded(px(26.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element()),
        }
        .map(|img| photo_box().child(img).into_any_element())
        .unwrap_or_else(|| {
            photo_box()
                .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                .text_color(hue.fg)
                .child(text(fs(14.)).font_weight(FontWeight::EXTRA_BOLD).tracking(-0.02).child(p2_ui::components::user_initials(&name)))
                .into_any_element()
        });
        let has_avatar = match self.avatar_draft.as_deref() {
            Some(d) => !d.is_empty(),
            None => self.user.avatar_url.as_deref().is_some_and(|u| !u.is_empty()),
        };
        let (hb, hbd, ht) = (p.bg_hover, p.border_strong, p.text_strong);
        let photo_col = div()
            .id("profile-photo")
            .group("profile-photo")
            .w(px(116.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(9.))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| this.pick_avatar(window, cx)))
            .child(photo)
            .child(
                div()
                    .min_w(px(104.))
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .text_color(p.text)
                    .text_size(fs(10.))
                    .css_weight(650.)
                    .group_hover("profile-photo", move |s| s.bg(hb).border_color(hbd).text_color(ht))
                    .child("Fotoğrafı değiştir"),
            );
        let tall = |label: &'static str, input: AnyElement| div().flex().flex_col().gap(px(5.)).pb(px(4.)).child(text(fs(10.)).h(px(14.)).css_weight(650.).text_color(p.text_3).child(label)).child(input);
        let fields = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(9.))
            .child(tall("Görünen ad", self.input(&self.name, p, window, cx)))
            .child(tall("Unvan", self.input(&self.title, p, window, cx)));
        let can_save = !self.saving && name.trim().chars().count() >= 2;
        let profile = block("Profil", p)
            .child(div().flex().gap(px(14.)).child(photo_col).child(fields))
            .child(
                row()
                    .pl(px(130.))
                    .child(button("profile-save", if self.saving { "Kaydediliyor…" } else { "Profili kaydet" }, Btn::Primary, !can_save, p).when(can_save, |b| b.on_click(cx.listener(|this, _, window, cx| this.save_profile(window, cx)))))
                    .when(has_avatar, |r| {
                        r.child(button("avatar-clear", "Fotoğrafı kaldır", Btn::Default, false, p).on_click(cx.listener(|this, _, _, cx| {
                            this.avatar_draft = Some(String::new());
                            cx.notify();
                        })))
                    }),
            )
            .when(!self.profile_notice.is_empty(), |b| b.child(hint(self.profile_notice.clone(), p)));

        // Durum: 2 sütunlu düğmeler (h 34, p 0 11, r 8), solda 9px durum noktası.
        let mut grid = div().flex().flex_wrap().gap(px(6.));
        for (status, label) in PRESENCE_OPTIONS {
            let on = self.preference == *status;
            let (hb, ht) = (p.bg_hover, p.text_strong);
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("set-presence-{status}")))
                    .w(px((self.content_width(window) - 6.) / 2.))
                    .h(px(34.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_1()
                    .rounded(radius::SM)
                    .text_size(fs(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_strong) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hb).text_color(ht)) })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preference = status.to_string();
                        cx.emit(SettingsEvent::Presence(status.to_string()));
                        cx.notify();
                    }))
                    .child(presence_dot(status, p))
                    .child(*label),
            );
        }
        let visibility = if self.preference == "INVISIBLE" { "Görünmez (herkese çevrimdışı görünürsün)".to_string() } else { super::workspace::status_label(&self.preference).to_string() };
        let presence = block("Durum", p).child(grid).child(hint(format!("Şu anki görünürlüğün: {visibility}. Uygulamayı arka plana alırsan 3 dakika sonra otomatik olarak \"Uzakta\" olursun."), p));

        div()
            .flex()
            .flex_col()
            .child(profile)
            .child(presence)
            .child(self.two_factor(p, window, cx))
            .child(last_block("Oturum", p).child(div().flex().child(button("logout", "Çıkış yap", Btn::Danger, false, p).on_click(cx.listener(|this, _, window, cx| {
                crate::trace("ayarlar: çıkış yap");
                let _ = this.app.update(cx, |app, cx| app.logout(window, cx));
            })))))
            .into_any_element()
    }

    /// `.settings-field input`: h 34, p 0 10, r 8, bg-3, 12px; odakta `border-accent` + halka.
    pub(crate) fn input(&self, state: &Entity<InputState>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> AnyElement {
        self.input_trailing(state, None, p, window, cx)
    }

    /// Sağda ek öğeli girdi (ör. `type=time` saat simgesi).
    pub(crate) fn input_trailing(&self, state: &Entity<InputState>, trailing: Option<AnyElement>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> AnyElement {
        let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
        let input = gpui_kit::component::input::Input::new(state).appearance(false).bordered(false).focus_bordered(false).px(px(0.)).py(px(0.)).text_size(fs(12.));
        settings_ui::input_frame(focused, p).child(div().flex_1().min_w_0().child(input)).children(trailing).into_any_element()
    }

    fn two_factor(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self.user.two_factor_enabled;
        let code_len = super::value(&self.code, cx).chars().filter(char::is_ascii_digit).count();
        let state = div()
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(13.))
            .py(px(11.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .child(div().size(px(9.)).flex_shrink_0().rounded_full().map(|s| if enabled { s.bg(p.success).shadow(shadows(&[Shadow::new(0., 0., 0., 3., p.success_soft)])) } else { s.bg(p.text_4) }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).child(if enabled { "Açık" } else { "Kapalı" }))
                    .child(div().text_size(fs(10.5)).line_height(px(16.)).text_color(p.text_3).child(if enabled { "Girişte parolanın yanında doğrulayıcı kodun da istenir." } else { "Hesabını yalnızca parola koruyor." })),
            );
        let code_field = |this: &Self, window: &Window, cx: &gpui_kit::App| {
            let focused = gpui_kit::Focusable::focus_handle(this.code.read(cx), cx).is_focused(window);
            let input = gpui_kit::component::input::Input::new(&this.code).appearance(false).bordered(false).focus_bordered(false).px(px(0.)).py(px(0.)).text_size(px(15.)).font_family(p2_ui::tokens::FONT_MONO).font_weight(FontWeight::BOLD);
            field("Doğrulama kodu", settings_ui::input_frame(focused, p).max_w(px(150.)).child(div().flex_1().min_w_0().child(input)), p)
        };
        let setup_box = || div().flex().flex_col().gap(px(11.)).p(px(13.)).border_1().border_color(p.border).rounded(radius::MD).bg(p.bg_3).p2_enter("twofa-setup", Entrance::Rise, std::time::Duration::ZERO);
        let mut b = block("İki adımlı doğrulama", p).child(state);
        if !enabled && self.setup.is_none() {
            b = b
                .child(hint("Google Authenticator, Microsoft Authenticator, Authy ya da 1Password gibi bir doğrulayıcı uygulamayla çalışır. Kodlar cihazında üretilir, internete ihtiyaç duymaz.", p))
                .child(div().flex().child(button("2fa-setup", if self.two_factor_busy { "Hazırlanıyor…" } else { "İki adımlı doğrulamayı aç" }, Btn::Primary, self.two_factor_busy, p).on_click(cx.listener(|this, _, window, cx| {
                    if !this.two_factor_busy {
                        this.start_two_factor(window, cx)
                    }
                }))));
        } else if !enabled {
            let setup = self.setup.clone().unwrap_or_else(|| TwoFactorSetup { secret: String::new(), otpauth: String::new() });
            let steps = [("1.", "Doğrulayıcı uygulamanı aç ve karekodu okut."), ("2.", "Okutamıyorsan anahtarı elle gir."), ("3.", "Uygulamadaki 6 haneli kodu aşağıya yaz.")];
            let qr: AnyElement = match &self.qr {
                Some(img) => gpui_kit::img(img.clone()).size(px(118.)).into_any_element(),
                None => div().text_size(fs(10.)).text_color(p.text_4).child("Karekod üretiliyor…").into_any_element(),
            };
            let grouped: String = setup.secret.chars().collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join(" ");
            let secret = setup.secret.clone();
            b = b.child(
                setup_box()
                    .child(div().flex().flex_col().gap(px(5.)).children(steps.iter().map(|(n, s)| div().flex().text_size(fs(11.)).line_height(relative(1.5)).text_color(p.text_2).child(div().mr(px(5.)).font_weight(FontWeight::BOLD).text_color(p.text_accent).child(*n)).child(*s))))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(div().size(px(132.)).flex_shrink_0().p(px(7.)).rounded(radius::MD).bg(gpui_kit::white()).shadow(p.shadow_md()).flex().items_center().justify_center().child(qr))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(7.))
                                    .child(text(font_size_2xs()).css_weight(800.).tracking(0.12).text_color(p.text_accent).child("KURULUM ANAHTARI"))
                                    .child(div().px(px(10.)).py(px(8.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1).text_color(p.text).font_family(p2_ui::tokens::FONT_MONO).text_size(px(12.)).font_weight(FontWeight::BOLD).tracking(0.1).child(grouped))
                                    .child(button("2fa-copy", "Anahtarı kopyala", Btn::Default, false, p).on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(secret.clone()));
                                        this.twofa_notice = "Anahtar kopyalandı.".into();
                                        cx.notify();
                                    }))),
                            ),
                    )
                    .child(code_field(self, window, cx))
                    .child(
                        row()
                            .child(button("2fa-on", if self.two_factor_busy { "Doğrulanıyor…" } else { "Doğrula ve aç" }, Btn::Primary, self.two_factor_busy || code_len != 6, p).on_click(cx.listener(|this, _, window, cx| this.confirm_two_factor(true, window, cx))))
                            .child(button("2fa-cancel", "Vazgeç", Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| {
                                this.setup = None;
                                this.twofa_error.clear();
                                this.code.update(cx, |s, cx| s.set_value("", window, cx));
                                cx.notify();
                            }))),
                    ),
            );
        } else if !self.disabling {
            b = b.child(div().flex().child(button("2fa-off-start", "İki adımlı doğrulamayı kapat", Btn::Danger, false, p).on_click(cx.listener(|this, _, _, cx| {
                this.disabling = true;
                this.twofa_notice.clear();
                cx.notify();
            }))));
        } else {
            b = b.child(
                setup_box()
                    .child(hint("Kapatmak için doğrulayıcı uygulamandaki güncel kodu gir.", p))
                    .child(code_field(self, window, cx))
                    .child(
                        row()
                            .child(button("2fa-off", if self.two_factor_busy { "Kapatılıyor…" } else { "Kapat" }, Btn::Danger, self.two_factor_busy || code_len != 6, p).on_click(cx.listener(|this, _, window, cx| this.confirm_two_factor(false, window, cx))))
                            .child(button("2fa-off-cancel", "Vazgeç", Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| {
                                this.disabling = false;
                                this.twofa_error.clear();
                                this.code.update(cx, |s, cx| s.set_value("", window, cx));
                                cx.notify();
                            }))),
                    ),
            );
        }
        if !self.twofa_error.is_empty() {
            b = b.child(div().text_size(fs(11.)).line_height(relative(1.5)).text_color(p.danger_text).child(self.twofa_error.clone()));
        } else if !self.twofa_notice.is_empty() {
            b = b.child(hint(self.twofa_notice.clone(), p));
        }
        b.into_any_element()
    }

    // ------------------------------------------------------------ Kısayollar

    fn shortcuts(&self, p: &Palette) -> AnyElement {
        let n = SHORTCUTS.len();
        let mut list = div().flex().flex_col().gap(px(2.));
        for (i, (keys, what)) in SHORTCUTS.iter().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(2.))
                    .py(px(8.))
                    .when(i + 1 < n, |s| s.border_b_1().border_color(p.border_subtle))
                    .child(div().w(px(170.)).flex_shrink_0().flex().child(kbd(keys, p)))
                    .child(div().flex_1().min_w_0().text_size(fs(11.)).line_height(relative(1.45)).text_color(p.text_2).child(p2_i18n::t(what))),
            );
        }
        last_block(&p2_i18n::t("keys.title"), p).child(list).into_any_element()
    }
}

/// `.shortcut-list kbd`: p 3 8, kenarlık `rgba(140,190,255,.35)` (alt 2px .5), r 6, beyaz gradyan, `650 10px` mono.
pub(crate) fn kbd(keys: &str, p: &Palette) -> gpui_kit::Div {
    div()
        .px(px(8.))
        .py(px(3.))
        .border_1()
        .border_b_2()
        .border_color(css_rgba(140, 190, 255, 0.42))
        .rounded(radius::XS)
        .bg(gpui_kit::linear_gradient(180., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.14), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.06), 1.)))
        .text_color(p.white)
        .font_family(p2_ui::tokens::FONT_MONO)
        .text_size(px(10.))
        .line_height(px(11.))
        .css_weight(650.)
        .child(keys.to_string())
}

fn font_size_2xs() -> gpui_kit::Pixels {
    p2_ui::tokens::font_size::XXS
}

/// `.settings-status .presence` (9×9): çevrimiçi yeşil + parıltı, uzakta `bg-2` + 2,5px sarı iç halka,
/// meşgul kırmızı + halka, görünmez `text-4`.
fn presence_dot(status: &str, p: &Palette) -> gpui_kit::Div {
    let d = div().size(px(9.)).flex_shrink_0().rounded_full();
    match status {
        "ONLINE" => d.bg(p.success).shadow(shadows(&[Shadow::new(0., 0., 0., 1., css_rgba(61, 220, 132, 0.35)), Shadow::new(0., 0., 7., 0., css_rgba(61, 220, 132, 0.45))])),
        "AWAY" => d.bg(p.bg_2).border(px(2.5)).border_color(p.warning),
        "BUSY" => d.bg(p.danger).shadow(shadows(&[Shadow::new(0., 0., 0., 1., css_rgba(255, 92, 108, 0.35))])),
        _ => d.bg(p.text_4),
    }
}

/// Modal zemini: `--p2-glass-strong` (160°, üç durak) + `::before` vurgu ışıması
/// (`radial-gradient(110% 100% at 22% 0%, accent-soft, rgba(44,123,255,.03) 46%, transparent 72%)`, 210px).
/// GPUI'de 3 duraklı/radyal gradyan yok → SVG olarak çizilir (köşe 18 dahil).
fn modal_background(w: f32, h: f32, p: &Palette) -> gpui_kit::Img {
    let color = |c: gpui_kit::Hsla| p2_ui::icons::svg_color(c);
    let a = 160f32.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let len = w * dx.abs() + h * dy.abs();
    let (cx, cy) = (w / 2., h / 2.);
    let (x1, y1, x2, y2) = (cx - dx * len / 2., cy - dy * len / 2., cx + dx * len / 2., cy + dy * len / 2.);
    let mut lin = String::new();
    for (c, at) in &p.glass_strong.stops {
        let (rgb, op) = color(*c);
        lin.push_str(&format!(r#"<stop offset="{at}" stop-color="{rgb}" stop-opacity="{op}"/>"#));
    }
    let (glow, ga) = color(p.accent_soft);
    let rx = w * 1.1;
    let ry = 210f32;
    let gcx = w * 0.22;
    let markup = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="l" gradientUnits="userSpaceOnUse" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">{lin}</linearGradient><radialGradient id="r" gradientUnits="userSpaceOnUse" cx="{gcx}" cy="0" r="{rx}" gradientTransform="translate({gcx} 0) scale(1 {sy}) translate(-{gcx} 0)"><stop offset="0" stop-color="{glow}" stop-opacity="{ga}"/><stop offset="0.46" stop-color="rgb(44,123,255)" stop-opacity="0.03"/><stop offset="0.72" stop-color="rgb(44,123,255)" stop-opacity="0"/></radialGradient><clipPath id="c"><rect width="{w}" height="{h}" rx="18" ry="18"/></clipPath></defs><g clip-path="url(#c)"><rect width="{w}" height="{h}" fill="url(#l)"/><rect width="{w}" height="210" fill="url(#r)"/></g></svg>"#,
        sy = ry / rx
    );
    p2_ui::icons::svg_image(markup).w(px(w)).h(px(h))
}

impl Render for SettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        self.sliders.specs.borrow_mut().clear();

        // Sol gezinti: başlık + bölümler.
        let mut nav = div()
            .id("settings-nav")
            .w(px(230.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(10.))
            .pb(px(14.))
            .overflow_y_scroll()
            .border_r_1()
            .border_color(p.border_subtle)
            .bg(css_rgba(0, 0, 0, 0.16))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .pt(px(16.))
                    .px(px(8.))
                    .pb(px(12.))
                    .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(p2_i18n::t("set.title")))
                    .child(text(font_size_2xs()).font_weight(FontWeight::BOLD).tracking(0.08).text_color(p.text_accent).child(p2_i18n::tv("set.version", &[("version", crate::updates::product_version())]))),
            );
        for (section, key) in SECTIONS {
            let on = self.section == *section;
            let section = *section;
            let (hover_bg, strong) = (p.bg_hover, p.text_strong);
            let group = SharedString::from(format!("sec-{key}"));
            nav = nav.child(
                div()
                    .id(SharedString::from(format!("sec-{key}")))
                    .group(group.clone())
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .map(|s| if on { s.bg(p.bg_selected).shadow(shadows(&[Shadow { inset: true, ..Shadow::new(2., 0., 0., 0., p.accent_hover) }])) } else { s.hover(move |s| s.bg(hover_bg).left(px(2.))) })
                    .on_click(cx.listener(move |this, _, _, cx| this.open(section, cx)))
                    .child(text(fs(12.)).font_weight(FontWeight::SEMIBOLD).text_color(if on { p.text_strong } else { p.text_2 }).when(!on, |s| s.group_hover(group.clone(), move |s| s.text_color(strong))).child(p2_i18n::t(&format!("sec.{key}"))))
                    .child(text(fs(9.5)).text_color(if on { p.text_accent } else { p.text_4 }).child(p2_i18n::t(&format!("sec.{key}.hint")))),
            );
        }

        self.content_w = self.content_width(window);
        let body = match self.section {
            Section::Account => self.account(&p, window, cx),
            Section::Appearance => self.appearance(&p, window, cx),
            Section::Language => self.language(&p, window, cx),
            Section::Media => self.media(&p, window, cx),
            Section::Notifications => self.notifications(&p, cx),
            Section::App => self.application(&p, window, cx),
            Section::Library => self.library(&p, cx),
            Section::Availability => self.availability(&p, window, cx),
            Section::Shortcuts => self.shortcuts(&p),
        };
        let key = SECTIONS.iter().find(|s| s.0 == self.section).map(|s| s.1).unwrap_or("media");

        // Kaydırma: içerik taşıyorsa Chromium 10px'lik çubuğa yer ayırır (içerik 598, değilse 608).
        let max = f32::from(self.scroll.max_offset().y);
        let overflow = max > 0.5;
        if overflow != self.overflow {
            self.overflow = overflow;
            cx.notify();
        }
        if let Some(y) = self.pending_scroll {
            self.scroll.set_offset(point(px(0.), px(-y.min(max))));
            // İçerik yetişince (ya da yüklemeler bitip birkaç kare geçince) bırakılır.
            let ready = self.server.loaded || self.section != Section::App;
            // Tavan sabit kalmalı: "Bu bilgisayardaki sunucu" bloğu yoklamadan sonra eklenir ve tavanı büyütür;
            // sayaç tavan her değiştiğinde sıfırlanır, yoksa kaydırma eksik bir tavana kilitlenip bırakılıyordu.
            if (max - self.pending_max).abs() > 0.5 {
                self.pending_max = max;
                self.pending_frames = 0;
            }
            // "max >= y" tek başına bırakma koşulu değil: açılış canlandırmasında görünür alan küçükken tavan geçici
            // olarak büyük ölçülür, kaydırma erken bırakılınca sonra eksik tavana kırpılıyordu.
            if ready && max > 0. && { self.pending_frames += 1; self.pending_frames > 45 } {
                self.pending_scroll = None;
                crate::trace(&format!("ayarlar: kaydırma uygulandı {:.0}/{:.0}", y.min(max), max));
            }
            // Çizim içindeki cx.notify() yeni kare istemiyor (ekran dışı pencerede hiç kare gelmiyordu):
            // bekleyen kaydırma için kareyi açıkça iste.
            window.request_animation_frame();
            cx.notify();
        }
        let view_h = f32::from(self.scroll.bounds().size.height);
        let scrollbar = (overflow && view_h > 0.).then(|| {
            let content = view_h + max;
            let thumb = (view_h * view_h / content).max(20.);
            let off = -f32::from(self.scroll.offset().y);
            let top = if max > 0. { off / max * (view_h - thumb) } else { 0. };
            let group = "settings-scrollbar";
            div()
                .id("settings-scrollbar")
                .group(group)
                .absolute()
                .top_0()
                .right_0()
                .w(px(10.))
                .h_full()
                .child(div().absolute().top(px(top + 2.)).left(px(2.)).w(px(6.)).h(px(thumb - 4.)).rounded(radius::PILL).bg(css_rgba(120, 170, 255, 0.18)).group_hover(group, |s| s.bg(css_rgba(120, 170, 255, 0.32))))
        });

        let crop = self.crop_dialog(&p, cx);
        let vp = window.viewport_size();
        let modal_w = (f32::from(vp.width) - 48.).min(880.);
        let modal_h = (f32::from(vp.height) - 96.).min(640.).max(200.);

        let card = div()
            .id("settings-card")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, _, cx| {
                if this.capture_hotkey(&event.keystroke, cx) {
                    cx.stop_propagation();
                    return;
                }
                if this.captured_key(&event.keystroke.key, cx) {
                    cx.stop_propagation();
                }
                if event.keystroke.key == "escape" && this.open_select.take().is_some() {
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, e: &gpui_kit::MouseMoveEvent, _, cx| {
                let Some(id) = this.sliders.dragging.clone() else { return };
                if e.pressed_button != Some(gpui_kit::MouseButton::Left) {
                    this.sliders.dragging = None;
                    return;
                }
                if let Some((v, apply)) = this.sliders.value_at(&id, e.position.x) {
                    apply(this, v, cx);
                }
            }))
            .on_mouse_up(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, _| this.sliders.dragging = None))
            .relative()
            .w(px(modal_w))
            .h(px(modal_h))
            .flex()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.open_select.take().is_some() {
                    cx.notify();
                }
            }))
            .child(div().absolute().top(px(-1.)).left(px(-1.)).child(modal_background(modal_w, modal_h, &p)))
            .child(nav)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(52.))
                            .flex_shrink_0()
                            .pl(px(20.))
                            .pr(px(10.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(p2_i18n::t(&format!("sec.{key}"))))
                            .child({
                                let (hb, ht) = (p.accent_soft, p.text_strong);
                                div()
                                    .id("settings-close")
                                    .size(px(30.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(radius::XS)
                                    .cursor_pointer()
                                    .text_color(p.text_3)
                                    .text_size(fs(19.))
                                    .hover(move |s| s.bg(hb).text_color(ht))
                                    .tooltip(|window, cx| p2_ui::controls::tooltip("Kapat (Esc)", window, cx))
                                    .child("×")
                                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)))
                            }),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id("settings-scroll")
                                    .track_scroll(&self.scroll).no_auto_scrollbar()
                                    .overflow_y_scroll()
                                    .size_full()
                                    .pt(px(16.))
                                    .pl(px(20.))
                                    .pr(px(if self.overflow { 30. } else { 20. }))
                                    .pb(px(30.))
                                    .when_some(self.error.clone(), |s, (m, e)| s.child(div().mb(px(12.)).child(p2_ui::components::error_box(m, e, &p))))
                                    .child(body)
                                    .p2_enter("settings-scroll", Entrance::Rise, std::time::Duration::ZERO),
                            )
                            .children(scrollbar),
                    ),
            )
            .p2_enter("settings", Entrance::PopElastic, std::time::Duration::ZERO);

        // Katman: p2_enter göreli konum verdiğinden mutlak kapsayıcının içinde canlandırılır.
        div().absolute().inset_0().child(div()
            .id("settings-scrim")
            .size_full()
            // Electron katmanı tüm pencereyi kaplar (pt 38 = başlık çubuğu); burada çalışma alanı zaten başlığın altında.
            .pt(px(0.))
            .px(px(24.))
            .pb(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)))
            .child(card)
            .children(crop)
            .p2_enter("settings-layer", Entrance::Fade, std::time::Duration::ZERO))
    }
}

/// Seçilen görseli yükle ve hesaba bağla (en çok 8 MB, yalnız görsel).
fn upload_avatar_file(api: &Api, path: &std::path::Path) -> Result<User, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "avatar.png".into());
    if !p2_client_core::mime_for(&name).starts_with("image/") {
        return Err("Geçerli bir görsel seç.".to_string());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("Görsel 8 MB sınırını aşıyor.".into());
    }
    let file = api.upload_file(&name, &data).map_err(|e| e.message)?;
    let path = file.url.find("/uploads/").map(|i| file.url[i..].to_string()).unwrap_or(file.url);
    api.set_avatar(Some(&path)).map_err(|e| e.message)
}

impl SettingsModal {
    /// "Fotoğrafı değiştir": dosya seç → önizlemede göster, "Profili kaydet" ile yüklenir.
    fn pick_avatar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Profil fotoğrafı".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| {
                        let name = path.to_string_lossy().to_string();
                        match p2_client_core::mime_for(&name).starts_with("image/").then(|| settings_crop::Crop::open(&path)).flatten() {
                            Some(crop) => this.crop = Some(crop),
                            None => this.profile_notice = "Geçerli bir görsel seç.".into(),
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Otomatik pilot: profil fotoğrafını hemen yükle ve hesaba bağla.
    pub fn upload_avatar(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || upload_avatar_file(&api, &path), |this, r, _, cx| match r {
            Ok(user) => this.avatar_saved(user, cx),
            Err(e) => this.fail(e, cx),
        });
    }

    fn set_avatar(&mut self, url: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || api.set_avatar(url.as_deref()), |this, r, _, cx| match r {
            Ok(user) => this.avatar_saved(user, cx),
            Err(e) => this.fail(e.message, cx),
        });
    }

    fn avatar_saved(&mut self, user: User, cx: &mut Context<Self>) {
        crate::trace(&format!("ayarlar: profil fotoğrafı {}", user.avatar_url.clone().unwrap_or_else(|| "kaldırıldı".into())));
        crate::avatars::remember(&user.id, user.avatar_url.as_deref());
        self.user = user.clone();
        cx.emit(SettingsEvent::ProfileSaved(user));
        cx.notify();
    }
}

fn settings_library_names() -> [&'static str; 5] {
    ["panel", "screenshot", "mute", "deafen", "pushToTalk"]
}

#[allow(dead_code)]
fn _store_used(_: &store::Settings) {}
