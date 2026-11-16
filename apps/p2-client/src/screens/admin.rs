//! Yönetim paneli (AdminPanel.tsx + AdminTeam.tsx + AdminAccess.tsx): Genel Bakış, Ekip ve Yetkiler
//! (Üyeler / Roller), Erişim, Davetler, Kütüphane, Kanallar, Geri Dönüşüm, Kuruluş. Her bölüm kendi
//! yetkisine bağlı; yetkisi olmayan sekmeyi hiç görmez (`permissions/me`).
//!
//! Görünüm settings.css'in birebir karşılığıdır (`.settings-layer`, `.settings-modal.admin-modal`,
//! `.settings-block`, `.settings-button`, `.admin-*`, `.team-*`, `.perm-line`). Kanal izinleri penceresi
//! (`channel_perms.rs`) aynı kabuğu kullanır; ortak parçalar bu dosyada `pub(crate)`.

use std::collections::HashMap;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;

use chrono::{DateTime, Datelike, Local, Timelike};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, FontWeight, Hsla, MouseButton, Pixels, SharedString, Stateful, Subscription, Window, div, px, rgb};
use p2_client_core::{AdminChannel, Api, ApiError, DeletedProject, Invite, LibraryProject, MyPermissions, Organization, Overview, PermissionInfo, TeamMember, TeamRole};
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, lh, menu, menu_item, modal, modal_body, modal_foot, modal_head};
use p2_ui::form::{InputKind, native_checkbox};
use p2_ui::icons::icon;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_MONO, Palette, Shadow, css_rgba, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::workspace::status_label;
use crate::task;

/// AdminTeam.tsx `ROLE_COLORS`.
pub const ROLE_COLORS: [&str; 8] = ["#5865f2", "#3ba55d", "#faa61a", "#ed4245", "#9b59b6", "#1abc9c", "#e91e63", "#95a5a6"];
const MONTHS: [&str; 12] = ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Overview,
    Team,
    Access,
    Invites,
    Library,
    Channels,
    Trash,
    Organization,
}

/// (bölüm, ad, ipucu, gereken yetki)
const SECTIONS: [(Section, &str, &str, Option<&str>); 8] = [
    (Section::Overview, "Genel Bakış", "Kuruluşun özeti", None),
    (Section::Team, "Ekip ve Yetkiler", "Rol oluştur, yetki seç, kişiye ata", None),
    (Section::Access, "Erişim", "Proje ve kanal görünürlüğü", Some("member.roles")),
    (Section::Invites, "Davetler", "Davet kodları ve sınırları", Some("member.invite")),
    (Section::Library, "Kütüphane", "Hangi projeler uygulama olarak listelenir", Some("release.publish")),
    (Section::Channels, "Kanallar", "Tüm kanallar ve mesaj sayıları", None),
    (Section::Trash, "Geri Dönüşüm", "Silinen projeleri kalıcı kaldır", Some("organization.manage")),
    (Section::Organization, "Kuruluş", "Ad, adres ve bilgiler", Some("organization.manage")),
];

pub enum AdminEvent {
    Close,
    Renamed(String),
    /// `p2:toast` (kalıcı silme sonucu): çalışma alanı bildirimi gösterir.
    Toast(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TeamTab {
    Members,
    Roles,
}

enum Confirm {
    Remove(TeamMember),
    Transfer(TeamMember),
    Purge(DeletedProject),
}

/// Açık "＋ rol ver" türü seçici (Electron'da yerel `<select>`).
#[derive(Clone, PartialEq)]
enum Picker {
    GiveRole(String),
    GivePerson(String),
}

// ---------------------------------------------------------------- yardımcılar

/// `toLocaleDateString('tr-TR', {day, month:'long', year})`.
fn day(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map(|d| d.with_timezone(&Local)).map(|d| format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())).unwrap_or_else(|_| "—".into())
}

/// `toLocaleDateString('tr-TR', {day, month:'long'})`.
fn day_month(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map(|d| d.with_timezone(&Local)).map(|d| format!("{} {}", d.day(), MONTHS[d.month0() as usize])).unwrap_or_else(|_| "—".into())
}

/// `toLocaleString('tr-TR')`: "13.11.2026 06:12:48".
fn stamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map(|d| d.with_timezone(&Local)).map(|d| format!("{:02}.{:02}.{} {:02}:{:02}:{:02}", d.day(), d.month(), d.year(), d.hour(), d.minute(), d.second())).unwrap_or_default()
}

/// AdminPanel.tsx `bytes`.
pub fn bytes(size: u64) -> String {
    let s = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", s / 1024.)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", s / 1024. / 1024.)
    } else {
        format!("{:.2} GB", s / 1024. / 1024. / 1024.)
    }
}

fn hex(color: Option<&str>, fallback: Hsla) -> Hsla {
    color.and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok()).map(|v| rgb(v).into()).unwrap_or(fallback)
}

/// CSS `text-transform: uppercase` (`lang="tr"`: i → İ).
pub(crate) fn tr_upper(s: &str) -> String {
    s.chars().map(|c| match c {
        'i' => "İ".to_string(),
        'ı' => "I".to_string(),
        c => c.to_uppercase().to_string(),
    }).collect()
}

/// `initials.ts userInitials`.
fn initials(name: &str) -> String {
    p2_ui::components::user_initials(name)
}

/// Satır kutusu `line-height: normal` olan yazı.
pub(crate) fn text(size: Pixels) -> Div {
    div().text_size(size).line_height(lh(size))
}

/// `.settings-button` türleri.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Btn {
    Plain,
    Primary,
    Danger,
}

/// `.settings-button`: h 32, p 0 14, kenarlık `border`, r 8, bg-3, `650 11px`; `.primary` / `.danger`;
/// `:disabled` opaklık .5.
pub(crate) fn settings_button(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: Btn, disabled: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, border_strong, strong, danger) = (p.bg_hover, p.border_strong, p.text_strong, p.danger);
    let base = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(32.))
        .px(px(14.))
        .border_1()
        .rounded(radius::SM)
        .text_size(fs(11.))
        .line_height(lh(fs(11.)))
        .css_weight(650.)
        .whitespace_nowrap()
        .child(label.into());
    let base = match kind {
        Btn::Plain => base.border_color(p.border).bg(p.bg_3).text_color(p.text).when(!disabled, |s| s.hover(move |s| s.bg(hover_bg).border_color(border_strong).text_color(strong))),
        Btn::Danger => base.border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).when(!disabled, |s| s.hover(move |s| s.bg(danger).border_color(danger).text_color(gpui_kit::white()))),
        Btn::Primary => {
            let grad = p.accent_grad.clone();
            let hot = p2_ui::tokens::Gradient { angle: grad.angle, stops: grad.stops.iter().map(|(c, at)| (p2_ui::form::brighten(*c, 1.08), *at)).collect() };
            base.border_color(gpui_kit::transparent_black())
                .bg(grad.background())
                .text_color(p.text_on_accent)
                .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                .when(!disabled, |s| s.hover(move |s| s.bg(hot.background()).text_color(gpui_kit::white())))
        }
    };
    if disabled { base.opacity(0.5) } else { base.cursor_pointer() }
}

/// `.settings-block`: ızgara gap 9, alt boşluk 18 + 16, alt çizgi; son blokta çizgi/boşluk yok.
pub(crate) fn settings_block(title: impl Into<String>, last: bool, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(9.))
        .when(!last, |s| s.pb(px(18.)).mb(px(16.)).border_b_1().border_color(p.border_subtle))
        .child(block_title(title, p))
}

/// `.settings-block h4`: `800 fs-2xs`, aralık .18em, büyük harf, `text-accent`.
pub(crate) fn block_title(title: impl Into<String>, p: &Palette) -> Div {
    text(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(tr_upper(&title.into()))
}

/// `.settings-hint`: 10px, `text-3`, satır 1.5.
pub(crate) fn hint(body: impl IntoElement, p: &Palette) -> Div {
    div().text_size(fs(10.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).child(body)
}

/// `.settings-warn`: 11px, `danger-text`, satır 1.5.
pub(crate) fn warn(body: impl Into<SharedString>, p: &Palette) -> Div {
    div().text_size(fs(11.)).line_height(gpui_kit::relative(1.5)).text_color(p.danger_text).child(body.into())
}

/// `.settings-field`: üstte `650 10px text-3` etiket, 5px boşluk, altta girdi.
pub(crate) fn settings_field(label: &str, control: impl IntoElement, p: &Palette) -> Div {
    div().flex().flex_col().gap(px(5.)).child(text(fs(10.)).css_weight(650.).text_color(p.text_3).child(label.to_string())).child(control)
}

/// `.settings-field input`: h 34, p 0 10, kenarlık `border`, r 8, bg-3, 12px; odakta `border-accent` + halka.
pub(crate) fn field_input(state: &Entity<InputState>, disabled: bool, p: &Palette, window: &Window, cx: &App) -> AnyElement {
    let el = p2_ui::form::input(state, InputKind::P2, p, window, cx).px(px(10.)).py(px(0.)).text_size(fs(12.)).font_weight(FontWeight::NORMAL);
    if disabled { div().opacity(0.5).child(el.disabled(true)).into_any_element() } else { el.into_any_element() }
}

/// `.admin-avatar` (36, r 12, 11px) / `.small` (26, r 9, 9px): kimlik tonu gradyanı, baş harf ya da görsel.
fn admin_avatar(key: &str, name: &str, small: bool, p: &Palette) -> Div {
    let (size, r, f) = if small { (26., 9., fs(9.)) } else { (36., 12., fs(11.)) };
    let face = match crate::avatars::image(key) {
        Some(img) => gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(r)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
        None => initials(name).into_any_element(),
    };
    p2_ui::controls::avatar_box(size, p2_ui::components::avatar_hue(key), false, p).overflow_hidden().rounded(px(r)).text_size(f).tracking(0.).child(face)
}

/// direct.css `.presence` (+ `.online/.away/.busy`); `.admin-avatar > .presence` sağ/alt −2px.
fn presence(status: &str, p: &Palette) -> Div {
    p2_ui::controls::presence_ring(p2_ui::controls::Presence::from_status(status), p.bg_2, p).absolute().right(px(-2.)).bottom(px(-2.))
}

/// ChannelIcons.tsx — `.admin-channel-icon` (15px çizgi ikon, `#` için düz yazı).
pub(crate) fn channel_icon(kind: &str, size: f32, color: Hsla) -> AnyElement {
    match kind {
        "ANNOUNCEMENT" => icon("channel:announcement").el().size(px(size)).text_color(color).into_any_element(),
        "VOICE" => icon("channel:voice").el().size(px(size)).text_color(color).into_any_element(),
        "FORUM" => icon("channel:forum").el().size(px(size)).text_color(color).into_any_element(),
        _ => div().text_size(font_size::ROOT).line_height(lh(font_size::ROOT)).text_color(color).child("#").into_any_element(),
    }
}

fn type_label(kind: &str) -> &'static str {
    match kind {
        "ANNOUNCEMENT" => "Duyuru",
        "VOICE" => "Ses",
        "FORUM" => "Forum",
        _ => "Metin",
    }
}

/// settings.css `.settings-modal::before`: üstte 210px'lik eliptik vurgu parıltısı
/// (`radial-gradient(110% 100% at 22% 0%, accent-soft, rgba(44,123,255,.03) 46%, transparent 72%)`).
pub(crate) fn modal_glow(w: f32, p: &Palette) -> AnyElement {
    let h = 210.;
    let (c, a) = p2_ui::icons::svg_color(p.accent_soft);
    let markup = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({cx} 0) scale({rx} {ry})"><stop offset="0" stop-color="{c}" stop-opacity="{a}"/><stop offset="0.46" stop-color="rgb(44,123,255)" stop-opacity="0.03"/><stop offset="0.72" stop-color="rgb(44,123,255)" stop-opacity="0"/></radialGradient></defs><rect width="{w}" height="{h}" fill="url(#g)"/></svg>"#,
        cx = w * 0.22,
        rx = w * 1.1 * 1.4142,
        ry = h * 1.4142,
    );
    div().absolute().top_0().left_0().w(px(w)).h(px(h)).child(p2_ui::icons::svg_image(markup).w(px(w)).h(px(h))).into_any_element()
}

/// `.settings-layer` + `.settings-modal` kabuğu (ölçüler pencereye göre). Katman çalışma alanının
/// içinde (başlık çubuğunun altında) durduğundan üst boşluk 38 yerine 0'dır; kart aynı yere düşer.
pub(crate) fn modal_size(max_w: f32, max_h: f32, window: &Window) -> (f32, f32) {
    let vp = window.viewport_size();
    let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
    (max_w.min(vw * 0.94).min(vw - 48.), max_h.min(vh * 0.92).min(vh - 62.))
}

pub(crate) fn settings_layer(id: &'static str, card: impl IntoElement, p: &Palette) -> Stateful<Div> {
    div().id(id).absolute().inset_0().flex().items_center().justify_center().px(px(24.)).pb(px(24.)).bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM).occlude().child(card)
}

pub(crate) fn settings_card(id: &'static str, w: f32, h: f32, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .w(px(w))
        .h(px(h))
        .flex()
        .overflow_hidden()
        .border_1()
        .border_color(p.border_strong)
        .rounded(radius::XL)
        .bg(p.glass_strong.background())
        .shadow(p.shadow_lg())
        .text_color(p.text)
        .occlude()
        .child(modal_glow(w, p))
}

/// `.settings-modal > nav`: 230px, bg rgba(0,0,0,.16), sağ çizgi; başlık (b 14px + small fs-2xs).
pub(crate) fn settings_nav(title: impl Into<SharedString>, subtitle: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .relative()
        .w(px(230.))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(10.))
        .pb(px(14.))
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
                .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title.into()))
                .child(text(font_size::XXS).font_weight(FontWeight::BOLD).tracking(0.08).text_color(p.text_accent).child(subtitle.into())),
        )
}

/// `.settings-body > header`: 52px, p 0 10 0 20, alt çizgi; h3 14px; × 30px (19px, hover accent-soft).
pub(crate) fn settings_header(title: AnyElement, close: Stateful<Div>, p: &Palette) -> Div {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    div()
        .h(px(52.))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .pl(px(20.))
        .pr(px(10.))
        .border_b_1()
        .border_color(p.border_subtle)
        .child(title)
        .child(
            close
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::XS)
                .text_color(p.text_3)
                .text_size(fs(19.))
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child("×"),
        )
}

/// `.settings-modal > nav button`: p 8 10, r 8, b 12px/600 `text-2`, small 9.5px `text-4`;
/// hover bg-hover + 2px sağa; etkin bg-selected + 2px sol `accent-hover` iç gölgesi.
pub(crate) fn nav_button(id: impl Into<ElementId>, label: &str, sub: Option<&str>, active: bool, p: &Palette) -> Stateful<Div> {
    let (hover, strong) = (p.bg_hover, p.text_strong);
    div()
        .id(id)
        .relative()
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(10.))
        .py(px(8.))
        .rounded(radius::SM)
        .overflow_hidden()
        .cursor_pointer()
        .when(active, |s| s.bg(p.bg_selected).child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(p.accent_hover)))
        .when(!active, |s| s.hover(move |s| s.bg(hover).left(px(2.))))
        .group("settings-nav-button")
        .child(text(fs(12.)).css_weight(600.).text_color(if active { p.text_strong } else { p.text_2 }).group_hover("settings-nav-button", move |s| s.text_color(strong)).child(label.to_string()))
        .when_some(sub, |s, sub| s.child(text(fs(9.5)).text_color(if active { p.text_accent } else { p.text_4 }).child(sub.to_string())))
}

/// PermissionRow.tsx `PermissionToggle` (`.perm-line.toggle`): solda ad + ipucu, sağda 36×20 anahtar.
pub(crate) fn perm_toggle(id: impl Into<ElementId>, label: &str, hint_text: &str, on: bool, disabled: bool, p: &Palette) -> Stateful<Div> {
    let hover = p.bg_hover;
    let knob = div()
        .relative()
        .flex_none()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .map(|s| if on { s.bg(gpui_kit::linear_gradient(135., gpui_kit::linear_color_stop(p.success, 0.), gpui_kit::linear_color_stop(p.success_mid, 1.))) } else { s.bg(p.navy_500) })
        .child(div().absolute().top(px(2.)).left(px(if on { 18. } else { 2. })).size(px(16.)).rounded_full().bg(gpui_kit::white()).shadow(shadows(&[Shadow::new(0., 1., 3., 0., css_rgba(0, 0, 0, 0.4))])));
    perm_line(id, label, hint_text, if on { "allow" } else { "inherit" }, disabled, p).when(!disabled, |s| s.cursor_pointer().when(!on, |s| s.hover(move |s| s.bg(hover)))).child(knob)
}

/// `.perm-line` kabuğu (durum renkleri ve metin).
pub(crate) fn perm_line(id: impl Into<ElementId>, label: &str, hint_text: &str, state: &str, locked: bool, p: &Palette) -> Stateful<Div> {
    let (border, bg) = match state {
        "allow" => (p.success_border, p.success_soft),
        "deny" => (p.danger_border, p.danger_soft),
        _ => (p.border, p.bg_3),
    };
    div()
        .id(id)
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(11.))
        .py(px(9.))
        .border_1()
        .border_color(border)
        .rounded(radius::MD)
        .bg(bg)
        .when(locked, |s| s.opacity(0.62))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(text(fs(11.5)).css_weight(600.).map(|s| if state == "deny" { s.text_color(p.text_2).line_through() } else { s.text_color(p.text) }).child(label.to_string()))
                .child(div().text_size(fs(9.5)).line_height(gpui_kit::relative(1.35)).text_color(p.text_3).truncate().child(hint_text.to_string())),
        )
}

/// `.perm-list` ızgarası: `repeat(auto-fit, minmax(min,1fr))` gap 4; ayrıca `.perm-line + .perm-line`
/// 4px üst boşluk (ızgarada satırları 4px kaydırır — Electron'daki gibi korunur).
pub(crate) fn perm_grid(items: Vec<AnyElement>, width: f32, min: f32) -> Div {
    let cols = (((width + 4.) / (min + 4.)).floor() as usize).max(1).min(items.len().max(1));
    let mut grid = div().w_full().flex().flex_col().gap(px(4.));
    let mut row = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        row.push(div().flex_1().min_w_0().flex().when(i > 0, |s| s.mt(px(4.))).child(div().w_full().flex().child(item)).into_any_element());
        if row.len() == cols {
            grid = grid.child(div().w_full().flex().gap(px(4.)).children(std::mem::take(&mut row)));
        }
    }
    if !row.is_empty() {
        while row.len() < cols {
            row.push(div().flex_1().into_any_element());
        }
        grid = grid.child(div().w_full().flex().gap(px(4.)).children(row));
    }
    grid
}

/// ConfirmDialog.tsx: `Modal size="sm"` + başlık/alt yazı, boş gövde, Vazgeç + onay düğmesi.
pub(crate) fn confirm_modal(title: String, body: String, confirm: &str, danger: bool, window: &Window, p: &Palette) -> (Div, Stateful<Div>, Stateful<Div>, Stateful<Div>) {
    let close = button("confirm-x", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×");
    let cancel = button("confirm-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child("Vazgeç");
    let ok = button("confirm-yes", if danger { BtnVariant::Danger } else { BtnVariant::Primary }, BtnSize::Md, false, false, p).child(confirm.to_string());
    let card = modal(ModalSize::Sm, window, p).child(modal_head(title, Some(body.into()), None, p)).child(modal_body());
    (card, close, cancel, ok)
}

// ---------------------------------------------------------------- panel

pub struct AdminPanel {
    api: Arc<Api>,
    org: Organization,
    me: String,
    pub perms: MyPermissions,
    section: Section,
    overview: Option<Overview>,
    members: Vec<TeamMember>,
    roles: Vec<TeamRole>,
    catalog: Vec<PermissionInfo>,
    /// Eski sistemden kalan kişi başına özel ayar sayısı (allow + deny).
    overrides: HashMap<String, usize>,
    invites: Vec<Invite>,
    trash: Vec<DeletedProject>,
    channels: Vec<AdminChannel>,
    library: Vec<LibraryProject>,
    access: Option<p2_client_core::AccessData>,
    access_user: Option<String>,
    access_error: Option<String>,
    busy: bool,
    error: Option<String>,
    notice: Option<String>,
    tab: TeamTab,
    selected_member: Option<String>,
    selected_role: Option<String>,
    creating: bool,
    color: &'static str,
    picker: Option<Picker>,
    confirm: Option<Confirm>,
    search: Entity<InputState>,
    role_name: Entity<InputState>,
    role_desc: Entity<InputState>,
    invite_uses: Entity<InputState>,
    invite_days: Entity<InputState>,
    org_name: Entity<InputState>,
    focus: FocusHandle,
    /// Otomatik pilot adımları (testler).
    steps: Vec<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<AdminEvent> for AdminPanel {}

impl AdminPanel {
    pub fn new(api: Arc<Api>, org: Organization, me: String, perms: MyPermissions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            let placeholder = placeholder.to_string();
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let search = input("Kişi ara", window, cx);
        let role_name = input("Örn. Moderatör", window, cx);
        let role_desc = input("isteğe bağlı", window, cx);
        let invite_uses = input("sınırsız", window, cx);
        let invite_days = input("süresiz", window, cx);
        let org_name = input("", window, cx);
        let name = org.name.clone();
        org_name.update(cx, |s, cx| s.set_value(name, window, cx));
        let mut subs: Vec<Subscription> = [&search, &role_name, &org_name, &role_desc].into_iter().map(|e| cx.subscribe(e, |_, _, _: &InputEvent, cx| cx.notify())).collect();
        // Davet alanları yalnız rakam (4 / 3 hane), Electron'daki `replace(/\D/g,'').slice(..)`.
        for (state, max) in [(&invite_uses, 4usize), (&invite_days, 3usize)] {
            subs.push(cx.subscribe_in(state, window, move |_, state, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    let v = state.read(cx).value().to_string();
                    let clean: String = v.chars().filter(|c| c.is_ascii_digit()).take(max).collect();
                    if clean != v {
                        state.update(cx, |s, cx| s.set_value(clean, window, cx));
                    }
                }
                cx.notify();
            }));
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let steps = std::env::var("P2_AUTOPILOT_ADMIN").map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect()).unwrap_or_default();
        let mut panel = Self {
            api,
            org,
            me,
            perms,
            section: Section::Overview,
            overview: None,
            members: Vec::new(),
            roles: Vec::new(),
            catalog: Vec::new(),
            overrides: HashMap::new(),
            invites: Vec::new(),
            trash: Vec::new(),
            channels: Vec::new(),
            library: Vec::new(),
            access: None,
            access_user: None,
            access_error: None,
            busy: false,
            error: None,
            notice: None,
            tab: TeamTab::Members,
            selected_member: None,
            selected_role: None,
            creating: false,
            color: ROLE_COLORS[0],
            picker: None,
            confirm: None,
            search,
            role_name,
            role_desc,
            invite_uses,
            invite_days,
            org_name,
            focus,
            steps,
            _subs: subs,
        };
        crate::trace("yönetim: panel açıldı");
        panel.load(Section::Overview, window, cx);
        panel
    }

    fn visible(&self) -> Vec<(Section, &'static str, &'static str)> {
        SECTIONS.iter().filter(|s| s.3.is_none_or(|need| self.perms.can(need))).map(|s| (s.0, s.1, s.2)).collect()
    }

    fn fail(&mut self, e: ApiError, cx: &mut Context<Self>) {
        crate::trace(&format!("yönetim: hata {}", e.message));
        self.error = Some(if e.message.is_empty() { "İşlem tamamlanamadı.".into() } else { e.message });
        self.busy = false;
        cx.notify();
    }

    pub fn open(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        self.error = None;
        self.picker = None;
        crate::trace(&format!("yönetim: bölüm {section:?}"));
        self.load(section, window, cx);
        cx.notify();
    }

    fn load(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        match section {
            Section::Overview | Section::Organization => task::run(window, cx, move || api.admin_overview(&org), |this, r, window, cx| match r {
                Ok(o) => {
                    this.overview = Some(o);
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Team => self.refresh_team(window, cx),
            Section::Access => task::run(window, cx, move || api.access(&org), |this, r, window, cx| match r {
                Ok(data) => {
                    if this.access_user.as_ref().is_none_or(|u| !data.members.iter().any(|m| &m.id == u)) {
                        this.access_user = data.members.iter().find(|m| !m.owner).or(data.members.first()).map(|m| m.id.clone());
                    }
                    crate::trace(&format!("yönetim: erişim {} kişi, {} proje", data.members.len(), data.projects.len()));
                    this.access = Some(data);
                    this.busy = false;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => {
                    this.access_error = Some(if e.message.is_empty() { "Erişim bilgileri yüklenemedi.".into() } else { e.message });
                    this.busy = false;
                    cx.notify();
                }
            }),
            Section::Invites => task::run(window, cx, move || api.invites(&org), |this, r, window, cx| match r {
                Ok(list) => {
                    this.invites = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Library => task::run(window, cx, move || api.library_projects(&org), |this, r, window, cx| match r {
                Ok(list) => {
                    for p in &list {
                        crate::avatars::remember(&format!("project:{}", p.id), p.icon_url.as_deref());
                    }
                    this.library = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Channels => task::run(window, cx, move || api.admin_channels(&org), |this, r, window, cx| match r {
                Ok(list) => {
                    this.channels = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Trash => task::run(window, cx, move || api.project_trash(&org), |this, r, window, cx| match r {
                Ok(list) => {
                    this.trash = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
        }
    }

    /// AdminPanel.tsx `refreshTeam`: üyeler, roller ve (yetkiliyse) yetki kataloğu.
    fn refresh_team(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let can_roles = self.perms.can("member.roles");
        task::run(
            window,
            cx,
            move || {
                let members = api.admin_members(&org)?;
                let roles = api.roles(&org)?;
                let permissions = if can_roles { api.admin_permissions(&org).ok() } else { None };
                Ok::<_, ApiError>((members, roles, permissions))
            },
            |this, r, window, cx| match r {
                Ok((members, roles, permissions)) => {
                    crate::trace(&format!("yönetim: {} üye, {} rol", members.len(), roles.len()));
                    for m in &members {
                        crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                    }
                    this.members = members;
                    this.roles = roles;
                    if let Some(p) = permissions {
                        this.overrides = p.members.iter().map(|m| (m.id.clone(), m.allow.len() + m.deny.len())).collect();
                        this.catalog = p.catalog;
                    }
                    this.busy = false;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            },
        );
    }

    /// Ekip eylemi: kaydet, bildir, listeleri tazele (AdminPanel.tsx `teamAction`). `trace` iz satırı
    /// (testler), `notice` ekranda görünen Electron metni.
    fn team_action(&mut self, trace: String, notice: String, work: impl FnOnce(&Api, &str) -> Result<serde_json::Value, ApiError> + Send + 'static, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.notice = None;
        self.picker = None;
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || work(&api, &org), move |this, r, window, cx| match r {
            Ok(value) => {
                crate::trace(&format!("yönetim: {trace}"));
                // Oturum kapatma: "{ad} için {n} oturum kapatıldı."
                this.notice = Some(match value.get("closed").and_then(|n| n.as_u64()) {
                    Some(n) if notice.ends_with("oturumları kapatıldı.") => format!("{} için {n} oturum kapatıldı.", notice.trim_end_matches(" oturumları kapatıldı.")),
                    _ => notice,
                });
                this.refresh_team(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
    }

    /// Bölüm eylemi (davet, kütüphane, kuruluş, erişim): bitince bölümü yeniden yükler.
    fn section_action(&mut self, trace: String, notice: Option<String>, work: impl FnOnce(&Api, &str) -> Result<serde_json::Value, ApiError> + Send + 'static, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.access_error = None;
        if notice.is_some() {
            self.notice = None;
        }
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || work(&api, &org), move |this, r, window, cx| match r {
            Ok(value) => {
                this.busy = false;
                crate::trace(&format!("yönetim: {trace}"));
                if this.section == Section::Organization {
                    if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                        this.org.name = name.to_string();
                        cx.emit(AdminEvent::Renamed(name.to_string()));
                    }
                }
                if let Some(n) = notice {
                    // Varsayılan kodu yenile: "Yeni davet kodu: KOD"
                    this.notice = Some(match value.get("rotated").and_then(|c| c.as_str()) {
                        Some(code) => format!("Yeni davet kodu: {code}"),
                        None => n,
                    });
                }
                this.load(this.section, window, cx);
            }
            Err(e) => {
                if this.section == Section::Access {
                    this.busy = false;
                    this.access_error = Some(e.message);
                    cx.notify();
                } else {
                    this.fail(e, cx)
                }
            }
        });
    }

    fn role_by_name(&self, name: &str) -> Option<TeamRole> {
        self.roles.iter().find(|r| r.name == name || r.handle == name).cloned()
    }

    fn create_role(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.role_name.read(cx).value().trim().to_string();
        let desc = self.role_desc.read(cx).value().trim().to_string();
        if name.chars().count() < 2 || p2_client_core::role_handle(&name).len() < 2 {
            return;
        }
        let color = self.color.to_string();
        self.creating = false;
        let label = format!("{name} rolü oluşturuldu.");
        self.team_action(label, "Rol oluşturuldu.".into(), move |api, org| api.create_role(org, &name, &desc, &color).map(|_| serde_json::Value::Null), window, cx);
        self.role_name.update(cx, |s, cx| s.set_value("", window, cx));
        self.role_desc.update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn toggle_permission(&mut self, role: &TeamRole, permission: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut next = role.permissions.clone();
        let on = if let Some(i) = next.iter().position(|p| p == permission) {
            next.remove(i);
            false
        } else {
            next.push(permission.to_string());
            true
        };
        let (rid, label) = (role.id.clone(), format!("{} yetkisi {} rolünde {}.", permission, role.name, if on { "açıldı" } else { "kapatıldı" }));
        self.team_action(label, "Rol yetkileri güncellendi.".into(), move |api, org| api.set_role_permissions(org, &rid, &next), window, cx);
    }

    fn assign(&mut self, user: String, role: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let label = if on { "Rol verildi." } else { "Rol geri alındı." }.to_string();
        self.team_action(label.clone(), label, move |api, org| api.assign_role(org, &role, &user, on), window, cx);
    }

    fn create_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uses = self.invite_uses.read(cx).value().trim().parse::<u64>().ok().filter(|n| *n > 0);
        let days = self.invite_days.read(cx).value().trim().parse::<u64>().ok().filter(|n| *n > 0);
        self.invite_uses.update(cx, |s, cx| s.set_value("", window, cx));
        self.invite_days.update(cx, |s, cx| s.set_value("", window, cx));
        let label = "Yeni davet kodu oluşturuldu.".to_string();
        self.section_action(label.clone(), Some(label), move |api, org| api.create_invite(org, uses, days).map(|i| serde_json::json!({ "code": i.code })), window, cx);
    }

    fn rename_org(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.org_name.read(cx).value().trim().to_string();
        if name.is_empty() || name == self.org.name {
            return;
        }
        let label = "Kuruluş adı güncellendi.".to_string();
        self.section_action(label.clone(), Some(label), move |api, org| api.rename_organization(org, &name), window, cx);
    }

    fn set_library(&mut self, item: LibraryProject, window: &mut Window, cx: &mut Context<Self>) {
        let on = !item.library_enabled;
        let trace = format!("{} kütüphanede {}.", item.name, if on { "listeleniyor" } else { "gizlendi" });
        let notice = if on { format!("{} kütüphaneye eklendi.", item.name) } else { format!("{} kütüphaneden kaldırıldı.", item.name) };
        let id = item.id.clone();
        self.section_action(trace, Some(notice), move |api, org| api.set_library_enabled(org, &id, on).map(|_| serde_json::Value::Null), window, cx);
    }

    fn revoke_invite(&mut self, invite: Invite, window: &mut Window, cx: &mut Context<Self>) {
        let trace = format!("{} davet kodu iptal edildi.", invite.code);
        self.section_action(trace, Some("Davet kodu iptal edildi.".into()), move |api, org| api.revoke_invite(org, &invite.id), window, cx);
    }

    fn rotate_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.section_action("Davet kodu yenilendi.".into(), Some("Davet kodu yenilendi.".into()), |api, org| api.rotate_invite(org).map(|i| serde_json::json!({ "rotated": i.code })), window, cx);
    }

    fn run_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::Remove(m)) => {
                let (id, label) = (m.id.clone(), format!("{} kuruluştan çıkarıldı.", m.display_name));
                self.team_action(label.clone(), label, move |api, org| api.remove_member(org, &id), window, cx);
            }
            Some(Confirm::Transfer(m)) => {
                let id = m.id.clone();
                let label = "Kuruluş sahipliği devredildi.".to_string();
                self.team_action(label.clone(), label, move |api, org| api.transfer_owner(org, &id), window, cx);
            }
            Some(Confirm::Purge(pr)) => {
                let (api, org) = (self.api.clone(), self.org.id.clone());
                self.busy = true;
                task::run(window, cx, move || api.purge_project(&org, &pr.id).map(|_| pr), |this, r, window, cx| {
                    this.busy = false;
                    match r {
                        Ok(pr) => {
                            let note = format!("“{}” kalıcı olarak silindi; “{}” kodu artık kullanılabilir.", pr.name, pr.slug);
                            crate::trace(&format!("yönetim: {} kalıcı olarak silindi.", pr.name));
                            this.trash.retain(|t| t.id != pr.id);
                            cx.emit(AdminEvent::Toast(note));
                            this.load(Section::Trash, window, cx);
                        }
                        Err(e) => cx.emit(AdminEvent::Toast(if e.message.is_empty() { "Proje silinemedi.".into() } else { e.message })),
                    }
                    cx.notify();
                });
            }
            None => {}
        }
        cx.notify();
    }

    // ------------------------------------------------------------ otomatik pilot

    /// Adımlar (`P2_AUTOPILOT_ADMIN`, `;` ile): `open:team`, `roles`, `role:Ad`, `create-role`,
    /// `perm:Ad:yetki`, `assign:Ad:kullanıcı`, `unassign:Ad:kullanıcı`, `delete-role:Ad`,
    /// `invite:kullanım:gün`, `revoke-last`, `library:Proje:on|off`, `rename:Ad`, `member:kullanıcı`,
    /// `confirm:remove|transfer:kullanıcı`, `confirm:purge`, `confirm-close`, `access-user:k`,
    /// `access-project|access-channel:ad:on|off`, `wait:ms`, `close`. Görünüm adımları (`roles`,
    /// `create-role`, `member`, `confirm:*`) veri yazmaz.
    fn next_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(step) = self.steps.pop() else { return };
        crate::trace(&format!("yönetim: adım {step}"));
        let parts: Vec<&str> = step.split(':').collect();
        let user_id = |this: &Self, name: &str| this.members.iter().find(|m| m.username == name).map(|m| m.id.clone());
        match parts.as_slice() {
            ["open", s] => {
                let section = match *s {
                    "team" => Section::Team,
                    "access" => Section::Access,
                    "invites" => Section::Invites,
                    "library" => Section::Library,
                    "channels" => Section::Channels,
                    "trash" => Section::Trash,
                    "organization" => Section::Organization,
                    _ => Section::Overview,
                };
                self.open(section, window, cx);
            }
            ["roles"] => {
                self.tab = TeamTab::Roles;
                cx.notify();
                self.next_step(window, cx);
            }
            ["create-role"] => {
                self.tab = TeamTab::Roles;
                self.creating = true;
                cx.notify();
                self.next_step(window, cx);
            }
            ["member", user] => {
                self.selected_member = user_id(self, user);
                cx.notify();
                self.next_step(window, cx);
            }
            ["confirm", kind @ ("remove" | "transfer"), user] => {
                if let Some(m) = self.members.iter().find(|m| m.username == *user).cloned() {
                    self.confirm = Some(if *kind == "remove" { Confirm::Remove(m) } else { Confirm::Transfer(m) });
                }
                cx.notify();
                self.next_step(window, cx);
            }
            ["confirm", "purge"] => {
                if let Some(t) = self.trash.first().cloned() {
                    self.confirm = Some(Confirm::Purge(t));
                }
                cx.notify();
                self.next_step(window, cx);
            }
            ["confirm-close"] => {
                self.confirm = None;
                cx.notify();
                self.next_step(window, cx);
            }
            ["role", name] => {
                self.role_name.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
                self.create_role(window, cx);
            }
            ["perm", role, perm] => match self.role_by_name(role) {
                Some(r) => {
                    self.selected_role = Some(r.id.clone());
                    self.toggle_permission(&r, perm, window, cx);
                }
                None => self.next_step(window, cx),
            },
            [verb @ ("assign" | "unassign"), role, user] => match (self.role_by_name(role), user_id(self, user)) {
                (Some(r), Some(u)) => {
                    self.selected_member = Some(u.clone());
                    self.assign(u, r.id, *verb == "assign", window, cx);
                }
                _ => self.next_step(window, cx),
            },
            ["delete-role", role] => match self.role_by_name(role) {
                Some(r) => {
                    let (id, label) = (r.id.clone(), format!("{} silindi.", r.name));
                    self.team_action(label.clone(), label, move |api, org| api.delete_role(org, &id), window, cx);
                }
                None => self.next_step(window, cx),
            },
            ["invite", uses, days] => {
                self.invite_uses.update(cx, |s, cx| s.set_value(uses.to_string(), window, cx));
                self.invite_days.update(cx, |s, cx| s.set_value(days.to_string(), window, cx));
                self.create_invite(window, cx);
            }
            ["revoke-last"] => match self.invites.first().cloned() {
                Some(i) => self.revoke_invite(i, window, cx),
                None => self.next_step(window, cx),
            },
            ["library", project, state] => match self.library.iter().find(|p| p.name == *project).cloned() {
                Some(p) if p.library_enabled != (*state == "on") => self.set_library(p, window, cx),
                Some(p) => {
                    crate::trace(&format!("yönetim: {} kütüphanede {}.", p.name, if p.library_enabled { "listeleniyor" } else { "gizlendi" }));
                    self.next_step(window, cx)
                }
                None => self.next_step(window, cx),
            },
            ["rename", name] => {
                self.org_name.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
                self.rename_org(window, cx);
            }
            ["access-user", user] => {
                self.access_user = self.access.as_ref().and_then(|a| a.members.iter().find(|m| m.username == *user)).map(|m| m.id.clone());
                cx.notify();
                self.next_step(window, cx);
            }
            [kind @ ("access-project" | "access-channel"), name, state] => {
                let on = *state == "on";
                let (user, data) = (self.access_user.clone(), self.access.clone());
                let target = data.as_ref().and_then(|d| if *kind == "access-project" { d.projects.iter().find(|p| p.name == *name).map(|p| p.id.clone()) } else { d.projects.iter().flat_map(|p| p.channels.iter()).find(|c| c.name == *name).map(|c| c.id.clone()) });
                match (user, target) {
                    (Some(u), Some(t)) => self.set_access(*kind == "access-project", t, u, on, window, cx),
                    _ => self.next_step(window, cx),
                }
            }
            ["wait", ms] => {
                let ms = ms.parse::<u64>().unwrap_or(1000);
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
                    let _ = this.update_in(cx, |this, window, cx| this.next_step(window, cx));
                })
                .detach();
            }
            ["close"] => cx.emit(AdminEvent::Close),
            _ => self.next_step(window, cx),
        }
    }

    fn set_access(&mut self, project: bool, target: String, user: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.access.as_ref().and_then(|a| a.members.iter().find(|m| m.id == user)).map(|m| m.display_name.clone()).unwrap_or_default();
        let label = format!("{name} için {} {}.", if project { "proje" } else { "kanal" }, if on { "görünür" } else { "gizli" });
        self.section_action(label, None, move |api, org| if project { api.set_project_access(org, &target, &user, on) } else { api.set_channel_access(org, &target, &user, on) }, window, cx);
    }
}

// ---------------------------------------------------------------- çizim

/// `.admin-stat`: p 12 13, kenarlık `border`, r 11, `glass`, inset-top; small (fs-2xs 800, .12em,
/// büyük harf, accent), b (20px 750, −.02em), span (10px text-3, 1.4). İpucu yoksa ızgara satırları
/// esnetilir (Electron'da hücre yüksekliği satırın en yükseğine uzar).
fn stat(label: &str, value: String, hint_text: Option<String>, p: &Palette) -> Div {
    let has_hint = hint_text.is_some();
    let grow = |d: Div| if has_hint { d } else { d.flex_grow(1.) };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(13.))
        .py(px(12.))
        .border_1()
        .border_color(p.border)
        .rounded(radius::MD)
        .bg(p.glass.background())
        .shadow(shadows(&[p.inset_top]))
        .child(grow(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(label))))
        .child(grow(text(fs(20.)).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(value)))
        .when_some(hint_text, |s, h| s.child(div().text_size(fs(10.)).line_height(gpui_kit::relative(1.4)).text_color(p.text_3).child(h)))
}

/// `.admin-stats`: `repeat(auto-fit, minmax(150px, 1fr))` gap 8.
fn stats_grid(items: Vec<Div>, width: f32) -> Div {
    let cols = (((width + 8.) / 158.).floor() as usize).max(1).min(items.len().max(1));
    let mut col = div().flex().flex_col().gap(px(8.));
    let mut row: Vec<AnyElement> = Vec::new();
    let n = items.len();
    for (i, item) in items.into_iter().enumerate() {
        row.push(div().flex_1().min_w_0().flex().child(item.p2_enter(SharedString::from(format!("stat-{width}-{i}-{n}")), Entrance::Rise, std::time::Duration::ZERO)).into_any_element());
        if row.len() == cols || i + 1 == n {
            while row.len() < cols {
                row.push(div().flex_1().into_any_element());
            }
            col = col.child(div().flex().gap(px(8.)).children(std::mem::take(&mut row)));
        }
    }
    col
}

/// AdminTeam.tsx `Tag` (`.team-tag`): `@handle`, renk verilmişse zemin/kenarlık/yazı o renkten.
fn tag(handle: &str, color: Option<&str>, p: &Palette) -> Div {
    let c = color.map(|c| hex(Some(c), p.text));
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .px(px(9.))
        .py(px(4.))
        .border_1()
        .rounded(radius::PILL)
        .font_family(FONT_MONO)
        .text_size(fs(10.5))
        .line_height(lh(fs(10.5)))
        .css_weight(650.)
        .map(|s| match c {
            Some(c) => s.bg(c.opacity(0x22 as f32 / 255.)).text_color(c).border_color(c.opacity(0x55 as f32 / 255.)),
            None => s.bg(p.bg_3).text_color(p.text).border_color(p.border),
        })
        .child(format!("@{handle}"))
}

/// `.team-tag button`: 14px yuvarlak ×, hover kırmızı.
fn tag_remove(id: impl Into<ElementId>, p: &Palette) -> Stateful<Div> {
    let danger = p.danger;
    div()
        .id(id)
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.bg_hover)
        .font_family(p2_ui::tokens::FONT)
        .text_size(fs(11.))
        .line_height(px(14.))
        .cursor_pointer()
        .hover(move |s| s.bg(danger).text_color(gpui_kit::white()))
        .child("×")
}

/// `.role-colors button`: 22px yuvarlak; etkin `text-strong` 2px kenarlık + 2px `bg-2` halka.
fn color_dot(id: impl Into<ElementId>, color: &str, active: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(22.))
        .flex_none()
        .rounded_full()
        .border_2()
        .bg(hex(Some(color), p.accent))
        .cursor_pointer()
        .map(|s| if active { s.border_color(p.text_strong).shadow(shadows(&[Shadow::new(0., 0., 0., 2., p.bg_2)])) } else { s.border_color(gpui_kit::transparent_black()) })
}

impl AdminPanel {
    fn overview_view(&self, width: f32, p: &Palette) -> AnyElement {
        let Some(o) = &self.overview else { return hint("Yükleniyor…", p).into_any_element() };
        div()
            .flex()
            .flex_col()
            .child(settings_block("Ekip", false, p).child(stats_grid(
                vec![
                    stat("Toplam üye", o.members.total.to_string(), Some(format!("{} kişi çevrimiçi", o.members.online)), p),
                    stat("Rol", o.members.roles.to_string(), Some(format!("{} kuruluş sahibi", o.members.owners)), p),
                    stat("Bu hafta katılan", o.members.joined_this_week.to_string(), None, p),
                    stat("2FA açık", format!("{}/{}", o.members.two_factor, o.members.total), Some(if o.members.two_factor == o.members.total { "Herkes korumalı".into() } else { "Kalanları teşvik et".into() }), p),
                ],
                width,
            )))
            .child(settings_block("İçerik", false, p).child(stats_grid(
                vec![
                    stat("Mesaj", o.content.messages.to_string(), Some(format!("{} kanal · {} bireysel · {} grup", o.content.channel_messages, o.content.direct_messages, o.content.group_messages)), p),
                    stat("Kanal", o.content.channels.to_string(), Some(if o.content.deleted_channels > 0 { format!("{} silinmiş", o.content.deleted_channels) } else { "Silinmiş kanal yok".into() }), p),
                    stat("Proje", o.content.projects.to_string(), None, p),
                    stat("Grup", o.content.groups.to_string(), None, p),
                ],
                width,
            )))
            .child(settings_block("Sistem", true, p).child(stats_grid(
                vec![
                    stat("Dosya alanı", bytes(o.storage.bytes), Some(format!("{} dosya", o.storage.files)), p),
                    stat("Açık oturum", o.sessions.to_string(), None, p),
                    stat("P2 Server", format!("v{}", o.server.version), None, p),
                    stat("Davet kodu", o.invites.to_string(), None, p),
                    stat("Kuruluş yaşı", day(&o.organization.created_at), None, p),
                ],
                width,
            )))
            .into_any_element()
    }

    /// AdminTeam.tsx `Picker` (`.team-picker`, yerel `<select>`): kesikli hap; tıklayınca seçenek menüsü.
    fn picker_view(&self, which: Picker, label: &str, empty: &str, options: Vec<(String, String)>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        if options.is_empty() {
            return div().text_size(fs(10.)).line_height(lh(fs(10.))).italic().text_color(p.text_3).child(empty.to_string()).into_any_element();
        }
        let open = self.picker.as_ref() == Some(&which);
        let w = which.clone();
        let mut el = div()
            .relative()
            .child(
                div()
                    .id(SharedString::from(format!("picker-{label}")))
                    .h(px(26.))
                    .pl(px(8.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .border_1()
                    .border_dashed()
                    .border_color(p.border_accent)
                    .rounded(radius::PILL)
                    .text_color(p.text_accent)
                    .text_size(fs(10.5))
                    .line_height(lh(fs(10.5)))
                    .cursor_pointer()
                    .child(label.to_string())
                    .child(p2_ui::glyphs::ui_stroke("chevron-down", 12., 2.).text_color(p.text_accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picker = if this.picker.as_ref() == Some(&w) { None } else { Some(w.clone()) };
                        cx.notify();
                    })),
            );
        if open {
            let mut list = menu("picker-menu", p).absolute().top(px(30.)).left_0().max_h(px(260.)).overflow_y_scroll();
            for (id, name) in options {
                let which = which.clone();
                list = list.child(menu_item(SharedString::from(format!("pick-{id}")), name, None, None, false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.picker = None;
                    match &which {
                        Picker::GiveRole(user) => this.assign(user.clone(), id.clone(), true, window, cx),
                        Picker::GivePerson(role) => this.assign(id.clone(), role.clone(), true, window, cx),
                    }
                })));
            }
            el = el.child(gpui_kit::deferred(list.p2_enter("picker-menu", Entrance::ScaleIn, std::time::Duration::ZERO)).with_priority(2));
        }
        el.into_any_element()
    }

    /// `.team-head`: ızgara `auto 1fr auto`, p 11 13, kenarlık, r 11, `glass`, inset-top.
    fn team_head(lead: AnyElement, title: AnyElement, lines: Vec<String>, actions: Div, p: &Palette) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(13.))
            .py(px(11.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.glass.background())
            .shadow(shadows(&[p.inset_top]))
            .child(lead)
            .child(div().flex_1().min_w_0().flex().flex_col().child(div().mb(px(2.)).child(title)).children(lines.into_iter().map(|l| div().text_size(fs(10.)).line_height(gpui_kit::relative(1.45)).text_color(p.text_3).child(l))))
            .child(actions.max_w(px(320.)).flex().flex_wrap().items_center().justify_end().gap(px(6.)))
    }

    fn team_view(&self, width: f32, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can_roles = self.perms.can("member.roles");
        let can_kick = self.perms.can("member.kick");
        let owner = self.perms.owner;
        let needle = self.search.read(cx).value().trim().to_lowercase();
        let has = |v: &str| needle.is_empty() || v.to_lowercase().contains(&needle);

        // `.team-tabs > button`: h 32, p 0 14, hap, kenarlık; etkin accent-soft + text-accent; em sayaç.
        let tab = |id: &'static str, label: &str, count: usize, on: bool, which: TeamTab| {
            let (bs, strong) = (p.border_strong, p.text_strong);
            div()
                .id(id)
                .h(px(32.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(6.))
                .border_1()
                .rounded(radius::PILL)
                .text_size(fs(11.5))
                .line_height(lh(fs(11.5)))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_accent) } else { s.border_color(p.border).text_color(p.text_3).hover(move |s| s.border_color(bs).text_color(strong)) })
                .child(label.to_string())
                .child(div().px(px(6.)).py(px(1.)).rounded(radius::PILL).bg(if on { p.accent_soft_2 } else { p.bg_hover }).text_size(fs(9.5)).line_height(lh(fs(9.5))).child(count.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = which;
                    this.creating = false;
                    this.picker = None;
                    cx.notify();
                }))
        };
        let soft = p.accent_soft;
        let tabs = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(tab("tt-members", "Üyeler", self.members.len(), self.tab == TeamTab::Members, TeamTab::Members))
            .child(tab("tt-roles", "Roller", self.roles.len(), self.tab == TeamTab::Roles, TeamTab::Roles))
            .when(self.tab == TeamTab::Roles && can_roles, |s| {
                s.child(
                    div()
                        .id("team-create")
                        .ml_auto()
                        .h(px(32.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .border_1()
                        .border_dashed()
                        .border_color(p.border_accent)
                        .rounded(radius::PILL)
                        .text_color(p.text_accent)
                        .text_size(fs(11.5))
                        .line_height(lh(fs(11.5)))
                        .css_weight(650.)
                        .cursor_pointer()
                        .hover(move |s| s.bg(soft))
                        .child("＋ Yeni rol")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.color = ROLE_COLORS[0];
                            this.role_name.update(cx, |s, cx| s.set_value("", window, cx));
                            this.role_desc.update(cx, |s, cx| s.set_value("", window, cx));
                            this.creating = true;
                            cx.notify();
                        })),
                )
            });

        // Sol liste (`.team-list`, 250px): arama + satırlar.
        let search = div()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::SM)
            .bg(p.bg_3)
            .child(div().w(px(22.)).flex_none().text_color(p.text_4).child("⌕"))
            .child(Input::new(&self.search).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px_0().h(px(30.)));
        let mut list = div().id("team-list").w(px(250.)).flex_none().max_h(px(f32::from(window.viewport_size().height) * 0.58)).overflow_y_scroll().pr(px(4.)).flex().flex_col().gap(px(3.)).child(search);
        let member_list: Vec<&TeamMember> = self.members.iter().filter(|m| has(&m.display_name) || has(&m.username) || has(&m.email)).collect();
        let role_list: Vec<&TeamRole> = self.roles.iter().filter(|r| has(&r.name) || has(&r.handle)).collect();
        let member = self.members.iter().find(|m| Some(&m.id) == self.selected_member.as_ref()).or_else(|| member_list.first().copied()).cloned();
        let role = self.roles.iter().find(|r| Some(&r.id) == self.selected_role.as_ref()).or_else(|| role_list.first().copied()).cloned();
        let row_base = |id: String, active: bool| {
            let hover = p.bg_hover;
            div()
                .id(SharedString::from(id))
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(9.))
                .px(px(9.))
                .py(px(7.))
                .border_1()
                .rounded(radius::MD)
                .cursor_pointer()
                .map(|s| if active { s.border_color(p.border_accent).bg(p.accent_soft) } else { s.border_color(gpui_kit::transparent_black()).hover(move |s| s.bg(hover).left(px(2.))) })
        };
        let name_col = |name: String, sub: String| {
            div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).text_color(p.text).truncate().child(name)).child(text(fs(10.)).text_color(p.text_3).truncate().child(sub))
        };
        match self.tab {
            TeamTab::Members => {
                for m in &member_list {
                    let id = m.id.clone();
                    let sub = if m.owner { "Kurucu".to_string() } else if m.roles.is_empty() { "rol yok".into() } else { m.roles.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ") };
                    list = list.child(
                        row_base(format!("tm-{}", m.id), member.as_ref().is_some_and(|x| x.id == m.id))
                            .child(div().relative().flex_none().child(admin_avatar(&m.id, &m.display_name, true, p)).child(presence(&m.status, p)))
                            .child(name_col(m.display_name.clone(), sub))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_member = Some(id.clone());
                                this.picker = None;
                                cx.notify();
                            })),
                    );
                }
            }
            TeamTab::Roles => {
                for r in &role_list {
                    let id = r.id.clone();
                    let sub = if r.founder { "Kuruluş sahibi".to_string() } else if r.everyone { "Kuruluştaki herkes".into() } else { format!("{} kişi · {} grup", r.members.len(), r.groups.len()) };
                    list = list.child(
                        row_base(format!("tr-{}", r.id), role.as_ref().is_some_and(|x| x.id == r.id))
                            .child(div().size(px(10.)).flex_none().rounded_full().bg(hex(r.color.as_deref(), p.neutral_fallback)))
                            .child(name_col(r.name.clone(), format!("{sub} · {} yetki", r.permissions.len())))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_role = Some(id.clone());
                                this.picker = None;
                                cx.notify();
                            })),
                    );
                }
                if role_list.is_empty() {
                    list = list.child(hint("Rol yok.", p));
                }
            }
        }

        // Sağ ayrıntı (`.team-detail`)
        let detail_w = width - 250. - 14.;
        let mut detail = div().flex_1().min_w_0().flex().flex_col().gap(px(10.));
        let chips = || div().flex().flex_wrap().items_center().gap(px(6.));
        let small = |t: &str| div().text_size(fs(10.5)).line_height(lh(fs(10.5))).text_color(p.text_3).child(t.to_string());
        // `.team-detail .settings-block`: p 0, kenarlık yok (yalnız gap 9).
        let block = |title: &str| div().flex().flex_col().gap(px(9.)).child(block_title(title, p));
        if self.creating {
            let mut colors = div().flex().flex_wrap().gap(px(6.));
            for c in ROLE_COLORS {
                colors = colors.child(color_dot(SharedString::from(format!("nc-{c}")), c, self.color == c, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.color = c;
                    cx.notify();
                })));
            }
            let name = self.role_name.read(cx).value().trim().to_string();
            let name_ok = name.chars().count() >= 2 && p2_client_core::role_handle(&name).len() >= 2;
            let fw = (detail_w - 26. - 18. - 150.) / 2.3;
            detail = detail.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .px(px(13.))
                    .py(px(12.))
                    .border_1()
                    .border_color(p.border_accent)
                    .rounded(radius::MD)
                    .bg(p.accent_soft)
                    .child(block_title("Yeni rol", p))
                    .child(div().flex().gap(px(9.)).child(div().w(px(150.)).flex_none().child(settings_field("Rol adı", field_input(&self.role_name, false, p, window, cx), p))).child(div().w(px(fw * 1.3)).min_w_0().child(settings_field("Açıklama", field_input(&self.role_desc, false, p, window, cx), p))))
                    .child(settings_field("Renk", colors, p))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(settings_button("role-create", "Oluştur", Btn::Primary, self.busy || !name_ok, p).on_click(cx.listener(move |this, _, window, cx| {
                                if name_ok {
                                    this.create_role(window, cx)
                                }
                            })))
                            .child(settings_button("role-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                                this.creating = false;
                                cx.notify();
                            }))),
                    ),
            );
        } else if self.tab == TeamTab::Members {
            if let Some(m) = member {
                let mut actions = div();
                if can_kick {
                    let a = m.clone();
                    actions = actions.child(settings_button("m-sessions", "Oturumları kapat", Btn::Plain, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, window, cx| {
                        let (id, label) = (a.id.clone(), format!("{} oturumları kapatıldı.", a.display_name));
                        this.team_action(label.clone(), label, move |api, org| api.revoke_sessions(org, &id), window, cx);
                    })));
                }
                if owner && !m.owner {
                    let b = m.clone();
                    actions = actions.child(settings_button("m-transfer", "Sahipliği devret", Btn::Plain, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(Confirm::Transfer(b.clone()));
                        cx.notify();
                    })));
                }
                if can_kick {
                    let c = m.clone();
                    let disabled = self.busy || m.owner || m.id == self.me;
                    actions = actions.child(settings_button("m-remove", "Çıkar", Btn::Danger, disabled, p).h(px(28.)).on_click(cx.listener(move |this, _, _, cx| {
                        if !disabled {
                            this.confirm = Some(Confirm::Remove(c.clone()));
                            cx.notify();
                        }
                    })));
                }
                let title = div()
                    .flex()
                    .items_baseline()
                    .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(m.display_name.clone()))
                    .when(m.id == self.me, |s| s.child(text(fs(11.)).css_weight(500.).text_color(p.text_3).child(" · sen")));
                let seen = m.last_client_seen_at.as_deref().map(|s| format!(" · {}", stamp(s))).unwrap_or_default();
                let lines = vec![
                    format!("@{} · {}", m.username, m.email),
                    format!("{} · {} mesaj · {} katıldı · {}", status_label(&m.status), m.channel_messages, day(&m.joined_at), if m.two_factor_enabled { "2FA açık" } else { "2FA kapalı" }),
                    format!("Son P2 Client: {}{seen}", m.last_client_version.as_ref().map(|v| format!("v{v}")).unwrap_or_else(|| "bilinmiyor".into())),
                ];
                detail = detail.child(Self::team_head(div().relative().flex_none().child(admin_avatar(&m.id, &m.display_name, false, p)).child(presence(&m.status, p)).into_any_element(), title.into_any_element(), lines, actions, p));
                let mut row = chips();
                for r in &m.roles {
                    let removable = can_roles && !r.via_group && !r.founder;
                    let (uid, rid) = (m.id.clone(), r.id.clone());
                    let mut t = tag(&r.handle, r.color.as_deref(), p);
                    if removable {
                        t = t.child(tag_remove(SharedString::from(format!("rx-{}", r.id)), p).on_click(cx.listener(move |this, _, window, cx| this.assign(uid.clone(), rid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if m.roles.is_empty() {
                    row = row.child(small("Bu kişide rol yok."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.roles.iter().filter(|r| !r.everyone && !r.founder && !m.roles.iter().any(|x| x.id == r.id)).map(|r| (r.id.clone(), r.name.clone())).collect();
                    let empty = if self.roles.iter().any(|r| !r.everyone && !r.founder) { "Tüm roller verilmiş" } else { "Önce Roller sekmesinden bir rol oluştur" };
                    row = row.child(self.picker_view(Picker::GiveRole(m.id.clone()), "＋ rol ver", empty, options, p, cx));
                }
                detail = detail.child(block("Roller").child(row));
                if can_roles && !m.owner && self.overrides.get(&m.id).copied().unwrap_or(0) > 0 {
                    let id = m.id.clone();
                    detail = detail.child(block("Eski özel ayarlar").child(hint("Bu kişide eski sistemden kalan özel yetki ayarları var. Basit rol sistemine geçmek için temizleyebilirsin.", p)).child(div().flex().child(settings_button("m-reset", "Özel ayarları temizle", Btn::Plain, self.busy, p).on_click(cx.listener(move |this, _, window, cx| {
                        let id = id.clone();
                        let label = "Özel ayarlar temizlendi.".to_string();
                        this.team_action(label.clone(), label, move |api, org| api.reset_member_permissions(org, &id), window, cx);
                    })))));
                }
            }
        } else if let Some(r) = role {
            let mut actions = div();
            if can_roles && !r.founder && !r.everyone {
                let mut colors = div().flex().flex_wrap().gap(px(6.));
                for c in ROLE_COLORS {
                    let id = r.id.clone();
                    colors = colors.child(color_dot(SharedString::from(format!("rc-{c}")), c, r.color.as_deref() == Some(c), p).on_click(cx.listener(move |this, _, window, cx| {
                        let id = id.clone();
                        let label = "Rol güncellendi.".to_string();
                        this.team_action(label.clone(), label, move |api, org| api.update_role(org, &id, serde_json::json!({ "color": c })), window, cx);
                    })));
                }
                let del = r.clone();
                actions = actions.child(colors).child(settings_button("role-delete", "Rolü sil", Btn::Danger, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, window, cx| {
                    let (id, label) = (del.id.clone(), format!("{} silindi.", del.name));
                    this.team_action(label.clone(), label, move |api, org| api.delete_role(org, &id), window, cx);
                })));
            }
            if r.founder || r.everyone {
                actions = actions.child(
                    div().px(px(9.)).py(px(3.)).border_1().border_color(css_rgba(255, 181, 69, 0.5)).rounded(radius::PILL).bg(p.warning_soft).text_color(p.warning_text).text_size(fs(9.5)).line_height(lh(fs(9.5))).font_weight(FontWeight::BOLD).child("temel rol"),
                );
            }
            let mut lines = vec![format!(
                "@{} · {}",
                r.handle,
                if r.founder { "temel rol · kuruluş sahibi".to_string() } else if r.everyone { "temel rol · kuruluştaki herkes".into() } else { format!("{} kişi · {} grup", r.members.len(), r.groups.len()) }
            )];
            if let Some(d) = r.description.clone().filter(|d| !d.is_empty()) {
                lines.push(d);
            }
            detail = detail.child(Self::team_head(div().size(px(20.)).flex_none().rounded_full().bg(hex(r.color.as_deref(), p.neutral_fallback)).into_any_element(), text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(r.name.clone()).into_any_element(), lines, actions, p));
            if !r.founder && !r.everyone {
                let mut row = chips();
                for person in &r.members {
                    let (uid, rid) = (person.id.clone(), r.id.clone());
                    let mut t = tag(&person.username, None, p);
                    if can_roles {
                        t = t.child(tag_remove(SharedString::from(format!("px-{}", person.id)), p).on_click(cx.listener(move |this, _, window, cx| this.assign(uid.clone(), rid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if r.members.is_empty() {
                    row = row.child(small("Bu rol henüz doğrudan kimseye verilmedi."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.members.iter().filter(|m| !r.members.iter().any(|x| x.id == m.id)).map(|m| (m.id.clone(), m.display_name.clone())).collect();
                    row = row.child(self.picker_view(Picker::GivePerson(r.id.clone()), "＋ kişiye ver", "Herkeste var", options, p, cx));
                }
                detail = detail.child(block("Bu role sahip kişiler").child(row));
            }
            let hint_text = if r.founder { "Kurucu rolünde tüm yetkiler her zaman açıktır ve değiştirilemez." } else if r.everyone { "Buradaki her yetki kuruluştaki herkeste varsayılan olarak açılır." } else { "Açtığın yetkiler bu role sahip herkese verilir." };
            let mut perms = block("Yetkiler").child(hint(hint_text, p));
            let mut groups: Vec<&str> = Vec::new();
            for item in &self.catalog {
                if !groups.contains(&item.group.as_str()) {
                    groups.push(&item.group);
                }
            }
            for g in groups {
                let mut items = Vec::new();
                for item in self.catalog.iter().filter(|i| i.group == g) {
                    let on = r.permissions.contains(&item.id);
                    let disabled = self.busy || !can_roles || r.founder;
                    let (role, pid) = (r.clone(), item.id.clone());
                    items.push(
                        perm_toggle(SharedString::from(format!("perm-{}", item.id)), &item.label, &item.hint, on, disabled, p)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !disabled {
                                    this.toggle_permission(&role, &pid, window, cx);
                                }
                            }))
                            .into_any_element(),
                    );
                }
                perms = perms.child(div().mt(px(8.)).flex().flex_col().gap(px(4.)).child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(g))).child(perm_grid(items, detail_w, 320.)));
            }
            detail = detail.child(perms);
        }

        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(tabs)
            .child(
                div()
                    .text_size(fs(10.))
                    .line_height(gpui_kit::relative(1.5))
                    .text_color(p.text_3)
                    .flex()
                    .flex_wrap()
                    .child("Basit akış:\u{a0}")
                    .child(div().font_weight(FontWeight::BOLD).child("Roller"))
                    .child("\u{a0}bölümünde bir rol oluşturup yetkilerini aç; sonra\u{a0}")
                    .child(div().font_weight(FontWeight::BOLD).child("Üyeler"))
                    .child("\u{a0}bölümünden bu rolü kişiye ver."),
            )
            .child(div().flex().items_start().gap(px(14.)).child(list).child(detail.p2_enter(("team-detail", self.tab as usize), Entrance::Rise, std::time::Duration::ZERO)))
            .into_any_element()
    }

    /// AdminAccess.tsx: kişi seç → görebileceği projeler ve kanallar.
    fn access_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = &self.access else {
            return match &self.access_error {
                Some(e) => warn(e.clone(), p).into_any_element(),
                None => hint("Erişim listesi yükleniyor…", p).into_any_element(),
            };
        };
        let selected = data.members.iter().find(|m| Some(&m.id) == self.access_user.as_ref()).cloned();
        let busy = self.busy;
        let mut people = div().flex().flex_col().gap(px(5.));
        for m in &data.members {
            let id = m.id.clone();
            let active = selected.as_ref().is_some_and(|s| s.id == m.id);
            let hover = p.bg_hover;
            people = people.child(
                div()
                    .id(SharedString::from(format!("acc-{}", m.id)))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(7.))
                    .border_1()
                    .rounded(radius::MD)
                    .cursor_pointer()
                    .map(|s| if active { s.border_color(p.border_accent).bg(p.accent_soft) } else { s.border_color(gpui_kit::transparent_black()).hover(move |s| s.bg(hover)) })
                    .child(div().w(px(28.)).flex_none().child(admin_avatar(&m.id, &m.display_name, true, p)))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(m.display_name.clone())).child(text(fs(9.)).text_color(p.text_3).child(format!("@{}{}", m.username, if m.owner { " · Kuruluş sahibi" } else { "" }))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.access_user = Some(id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut projects = div().id("access-projects").flex_1().min_h_0().overflow_y_scroll().pr(px(4.)).flex().flex_col().gap(px(5.));
        if let Some(sel) = &selected {
            for pr in &data.projects {
                let visible = sel.owner || pr.owner_id == sel.id || pr.member_ids.contains(&sel.id);
                let protected = sel.owner || pr.owner_id == sel.id;
                let shown = pr.channels.iter().filter(|c| sel.owner || c.visible_user_ids.contains(&sel.id)).count();
                let (pid, uid) = (pr.id.clone(), sel.id.clone());
                let switch_off = protected || busy;
                // `.admin-access-switch`: 28×16 anahtar + etiket.
                let switch = div()
                    .id(SharedString::from(format!("accp-{}", pr.id)))
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .when(!switch_off, |s| s.cursor_pointer())
                    .child(
                        div()
                            .relative()
                            .w(px(28.))
                            .h(px(16.))
                            .rounded_full()
                            .when(switch_off, |s| s.opacity(0.55))
                            .map(|s| if visible { s.bg(p.accent_grad.background()) } else { s.bg(p.navy_500) })
                            .child(div().absolute().top(px(3.)).left(px(if visible { 15. } else { 3. })).size(px(10.)).rounded_full().bg(gpui_kit::white())),
                    )
                    .child(text(fs(9.5)).css_weight(650.).text_color(p.text_2).whitespace_nowrap().when(switch_off, |s| s.opacity(0.55)).child(if visible { "Projeyi görebilir" } else { "Proje gizli" }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !switch_off {
                            this.set_access(true, pid.clone(), uid.clone(), !visible, window, cx);
                        }
                    }));
                let article = div()
                    .flex_none()
                    .w_full()
                    .overflow_hidden()
                    .border_1()
                    .border_color(if visible { p.border_strong } else { p.border })
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .when(!visible, |s| s.opacity(0.72))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_x(px(10.))
                            .gap_y(px(8.))
                            .px(px(11.))
                            .py(px(10.))
                            .bg(p.bg_2)
                            .child(div().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(pr.name.clone())).child(text(fs(9.)).text_color(p.text_3).child(format!("{} kanal · {}", pr.channels.len(), if visible { format!("{shown} görünür") } else { "erişim kapalı".into() }))))
                            .child(switch),
                    );
                let mut channels = div().flex().flex_col().gap(px(7.)).p(px(7.)).overflow_hidden();
                let mut categories: Vec<&str> = Vec::new();
                for c in &pr.channels {
                    if !categories.contains(&c.category.as_str()) {
                        categories.push(&c.category);
                    }
                }
                for cat in categories {
                    let rows: Vec<AnyElement> = pr
                        .channels
                        .iter()
                        .filter(|c| c.category == cat)
                        .map(|c| {
                            let on = visible && (sel.owner || c.visible_user_ids.contains(&sel.id));
                            let locked = !visible || sel.owner || busy;
                            let (cid, uid) = (c.id.clone(), sel.id.clone());
                            let hover = p.bg_hover;
                            div()
                                .id(SharedString::from(format!("accc-{}", c.id)))
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(7.))
                                .p(px(7.))
                                .rounded(radius::XS)
                                .when(!visible, |s| s.opacity(0.38))
                                .when(visible, |s| s.cursor_pointer().hover(move |s| s.bg(hover)))
                                .child(div().w(px(20.)).flex_none().flex().justify_center().child(channel_icon(&c.kind, 15., p.text_accent)))
                                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(c.name.clone())).child(text(fs(9.)).text_color(p.text_3).child(type_label(&c.kind))))
                                .child(div().when(locked, |s| s.opacity(0.6)).child(native_checkbox(on, p)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if !locked {
                                        this.set_access(false, cid.clone(), uid.clone(), !on, window, cx);
                                    }
                                }))
                                .into_any_element()
                        })
                        .collect();
                    let mut grid = div().flex().flex_col().gap(px(1.));
                    let mut it = rows.into_iter();
                    loop {
                        let Some(a) = it.next() else { break };
                        let b = it.next().unwrap_or_else(|| div().flex_1().into_any_element());
                        grid = grid.child(div().flex().gap(px(1.)).child(a).child(b));
                    }
                    channels = channels.child(div().flex().flex_col().gap(px(3.)).child(div().pt(px(3.)).px(px(7.)).pb(px(1.)).text_size(fs(8.)).line_height(lh(fs(8.))).font_weight(FontWeight::BOLD).tracking(0.12).text_color(p.text_accent).child(tr_upper(cat))).child(grid));
                }
                if pr.channels.is_empty() {
                    channels = channels.child(hint("Bu projede kanal yok.", p));
                }
                projects = projects.child(article.child(channels));
            }
        }
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .when_some(self.access_error.clone(), |s, e| s.child(warn(e, p)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(220.)).flex_none().min_h_0().overflow_hidden().flex().flex_col().gap(px(9.)).child(block_title("Kişi seç", p)).child(hint("Bir kişinin görebileceği projeleri ve o projelerdeki kanalları belirle.", p)).child(people))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap(px(9.))
                            .child(block_title(format!("{} için görünür içerikler", selected.as_ref().map(|s| s.display_name.clone()).unwrap_or_else(|| "Kişi".into())), p))
                            .when(selected.as_ref().is_some_and(|s| s.owner), |s| s.child(hint("Kuruluş sahibi güvenlik gereği bütün projeleri ve kanalları görür.", p)))
                            .child(projects),
                    ),
            )
            .into_any_element()
    }

    fn invites_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can = self.perms.can("member.invite");
        let server = self.api.base().trim_end_matches('/').trim_end_matches("/api").to_string();
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, inv) in self.invites.iter().enumerate() {
            let (code, revoke) = (inv.code.clone(), inv.clone());
            let revoke_off = self.busy || self.invites.len() < 2 || !can;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .px(px(10.))
                    .py(px(9.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .child(div().flex_none().px(px(11.)).py(px(7.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1).font_family(FONT_MONO).text_size(fs(12.)).line_height(lh(fs(12.))).font_weight(FontWeight::BOLD).tracking(0.08).text_color(p.text_strong).child(inv.code.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(fs(10.)).text_color(p.text_3).child(match inv.max_uses {
                                Some(max) => format!("{}/{max} kullanım", inv.use_count),
                                None => format!("{} kullanım · sınırsız", inv.use_count),
                            }))
                            .child(text(fs(10.)).text_color(p.text_3).child(format!(
                                "{} · {} oluşturdu",
                                inv.expires_at.as_deref().map(|e| format!("{} tarihinde biter", day(e))).unwrap_or_else(|| "Süresiz".into()),
                                inv.created_by.display_name
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(settings_button(SharedString::from(format!("inv-copy-{i}")), "Kopyala", Btn::Plain, false, p).h(px(30.)).on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(code.clone()));
                                this.notice = Some("Davet kodu kopyalandı.".into());
                                cx.notify();
                            })))
                            .child(settings_button(SharedString::from(format!("inv-rev-{i}")), "İptal et", Btn::Danger, revoke_off, p).h(px(30.)).on_click(cx.listener(move |this, _, window, cx| {
                                if !revoke_off {
                                    this.revoke_invite(revoke.clone(), window, cx);
                                }
                            }))),
                    )
                    .p2_enter(SharedString::from(format!("inv-{}", inv.id)), Entrance::Rise, std::time::Duration::ZERO),
            );
        }
        if self.invites.is_empty() {
            rows = rows.child(hint("Henüz davet kodu yok.", p));
        }
        const SERVER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/></svg>"#;
        let copy_server = server.clone();
        div()
            .flex()
            .flex_col()
            .child(
                settings_block("Sunucu bağlantısı", false, p).child(hint("Yeni kullanıcı önce bu sunucu adresiyle P2'ye bağlanır, ardından aşağıdaki davet kodlarından birini kullanır.", p)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(12.))
                        .py(px(10.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_1)
                        .child(div().size(px(38.)).flex_none().flex().items_center().justify_center().rounded(radius::SM).bg(p.accent_soft).child(gpui_kit::svg().data(SERVER_SVG).size(px(18.)).text_color(p.text_accent)))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(10.)).font_weight(FontWeight::BOLD).tracking(0.06).text_color(p.text_3).child("SUNUCU ADRESİ")).child(text(fs(13.)).font_family(FONT_MONO).font_weight(FontWeight::BOLD).text_color(p.text_strong).truncate().child(server)))
                        .child(settings_button("inv-server-copy", "Kopyala", Btn::Primary, false, p).h(px(34.)).on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(copy_server.clone()));
                            this.notice = Some("Sunucu bağlantısı kopyalandı.".into());
                            cx.notify();
                        }))),
                ),
            )
            .child(
                settings_block("Yeni davet kodu", false, p).child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(9.))
                        .child(div().flex_1().min_w_0().child(settings_field("Kullanım sınırı", field_input(&self.invite_uses, false, p, window, cx), p)))
                        .child(div().flex_1().min_w_0().child(settings_field("Geçerlilik (gün)", field_input(&self.invite_days, false, p, window, cx), p)))
                        .child(settings_button("inv-create", "Kod oluştur", Btn::Primary, self.busy || !can, p).h(px(34.)).on_click(cx.listener(move |this, _, window, cx| {
                            if can {
                                this.create_invite(window, cx)
                            }
                        }))),
                ),
            )
            .child(
                settings_block(format!("Davet kodları · {}", self.invites.len()), true, p)
                    .child(div().flex().flex_wrap().gap(px(8.)).child(settings_button("inv-rotate", "Varsayılan kodu yenile", Btn::Plain, self.busy || !can, p).on_click(cx.listener(move |this, _, window, cx| {
                        if can {
                            this.rotate_invite(window, cx)
                        }
                    }))))
                    .child(rows),
            )
            .into_any_element()
    }

    fn library_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for item in &self.library {
            let it = item.clone();
            let busy = self.busy;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(admin_avatar(&format!("project:{}", item.id), &item.name, true, p))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).text_color(p.text).truncate().child(item.name.clone())).child(text(fs(10.)).text_color(p.text_3).child(if item.library_enabled { "Kütüphanede listeleniyor" } else { "Kütüphanede görünmüyor" })))
                    .child(div().when(busy, |s| s.opacity(0.5)).child(p2_ui::form::toggle(SharedString::from(format!("lib-{}", item.id)), item.library_enabled, 0, p).on_click(cx.listener(move |this, _, window, cx| {
                        if !busy {
                            this.set_library(it.clone(), window, cx)
                        }
                    })))),
            );
        }
        if self.library.is_empty() {
            rows = rows.child(hint("Proje yok.", p));
        }
        settings_block(format!("Kütüphanede uygulamalar · {}", self.library.iter().filter(|i| i.library_enabled).count()), true, p)
            .child(hint("Açık olan projeler ekibin Kütüphane'sinde oyun/uygulama olarak görünür; sürüm yüklenip çalıştırılabilir. Bir sürüm yayınlandığında proje kendiliğinden açılır. Aynı anahtar proje ayarlarında da vardır.", p))
            .child(rows)
            .into_any_element()
    }

    /// `.admin-channel`: ızgara `26px 1fr auto`, p 9 10, kenarlık, r 11, bg-3.
    fn channel_row(id: String, icon_el: AnyElement, name: String, subs: Vec<String>, trail: Option<u64>, deleted: bool, p: &Palette) -> Stateful<Div> {
        div()
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(10.))
            .py(px(9.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .when(deleted, |s| s.opacity(0.6))
            .child(div().w(px(26.)).flex_none().flex().justify_center().text_color(p.text_accent).child(icon_el))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(name)).children(subs.into_iter().map(|s| text(fs(10.)).text_color(p.text_3).truncate().child(s))))
            .when_some(trail, |s, n| s.child(div().flex().flex_col().items_end().text_color(p.text_strong).child(text(fs(14.)).font_weight(FontWeight::BOLD).child(n.to_string())).child(text(fs(9.)).css_weight(600.).text_color(p.text_3).child("mesaj"))))
            .id(SharedString::from(format!("ach-{id}")))
    }

    fn channels_view(&self, p: &Palette) -> AnyElement {
        let live: Vec<&AdminChannel> = self.channels.iter().filter(|c| c.deleted_at.is_none()).collect();
        let trash: Vec<&AdminChannel> = self.channels.iter().filter(|c| c.deleted_at.is_some()).collect();
        let mut live_rows = div().flex().flex_col().gap(px(4.));
        for c in &live {
            let mut subs = vec![format!("{} · {} · {}", c.project, c.category, type_label(&c.kind))];
            if let Some(t) = c.topic.clone().filter(|t| !t.is_empty()) {
                subs.push(t);
            }
            live_rows = live_rows.child(Self::channel_row(c.id.clone(), channel_icon(&c.kind, 15., p.text_accent), c.name.clone(), subs, Some(c.messages), false, p));
        }
        if live.is_empty() {
            live_rows = live_rows.child(hint("Kanal yok.", p));
        }
        div()
            .flex()
            .flex_col()
            .child(settings_block(format!("Kanallar · {}", live.len()), trash.is_empty(), p).child(live_rows))
            .when(!trash.is_empty(), |s| {
                let mut rows = div().flex().flex_col().gap(px(4.));
                for c in &trash {
                    let sub = format!("{} · {} silindi · {} tarihinde temizlenecek", c.project, c.deleted_at.as_deref().map(day).unwrap_or_else(|| "—".into()), c.purge_at.as_deref().map(day).unwrap_or_else(|| "—".into()));
                    rows = rows.child(Self::channel_row(c.id.clone(), channel_icon(&c.kind, 15., p.text_accent), c.name.clone(), vec![sub], Some(c.messages), true, p));
                }
                s.child(settings_block(format!("Silinmiş kanallar · {}", trash.len()), true, p).child(hint("Silinen kanallar 30 gün saklanır, sonra kalıcı olarak temizlenir.", p)).child(rows))
            })
            .into_any_element()
    }

    fn trash_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for item in &self.trash {
            let it = item.clone();
            let when = format!("{} tarihinde silindi{}", day(&item.deleted_at), item.purge_at.as_deref().map(|d| format!(" · {} tarihinde kendiliğinden kaldırılacak", day_month(d))).unwrap_or_default());
            let busy = self.busy;
            rows = rows.child(
                Self::channel_row(item.id.clone(), text(font_size::ROOT).font_family("Segoe UI Symbol").child("🗑").into_any_element(), item.name.clone(), vec![format!("Kod: {} · Sahip: {}", item.slug, item.owner), when], None, false, p).child(settings_button(SharedString::from(format!("purge-{}", item.id)), "Kalıcı sil", Btn::Danger, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                    if !busy {
                        this.confirm = Some(Confirm::Purge(it.clone()));
                        cx.notify();
                    }
                }))),
            );
        }
        if self.trash.is_empty() {
            // `.admin-empty` (stil tanımı yok: kalıtılan 13,6px metin).
            rows = rows.child(text(font_size::ROOT).text_color(p.text).child("Geri dönüşüm boş."));
        }
        settings_block(format!("Silinen projeler · {}", self.trash.len()), true, p)
            .child(hint("Silinen projeler 30 gün geri dönüşümde bekler. Proje kodu bu süre boyunca kayıtlı kaldığı için aynı adla yeni bir proje açılamaz; kalıcı silmek kodu serbest bırakır ve projenin kanalları, mesajları, panoları geri dönülmez biçimde kaldırılır.", p))
            .child(rows)
            .into_any_element()
    }

    fn organization_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can = self.perms.can("organization.manage");
        let value = self.org_name.read(cx).value().trim().to_string();
        let save_off = !can || self.busy || value.is_empty() || value == self.org.name;
        let mut col = div().flex().flex_col().child(
            settings_block("Kuruluş adı", self.overview.is_none(), p)
                .child(settings_field("Ad", field_input(&self.org_name, !can, p, window, cx), p))
                .when(!can, |s| s.child(hint("Kuruluş adını değiştirmek için “Kuruluşu düzenle” yetkisi gerekir.", p)))
                .child(div().flex().child(settings_button("org-save", "Kaydet", Btn::Primary, save_off, p).on_click(cx.listener(move |this, _, window, cx| {
                    if !save_off {
                        this.rename_org(window, cx)
                    }
                })))),
        );
        if let Some(o) = &self.overview {
            let rows = [
                ("Adres", o.organization.slug.clone()),
                ("Kimlik", o.organization.id.chars().take(8).collect()),
                ("Kuruluş tarihi", day(&o.organization.created_at)),
                ("Toplam üye", format!("{} kişi", o.members.total)),
                ("Dosya alanı", bytes(o.storage.bytes)),
            ];
            let n = rows.len();
            // direct.css `.channel-details`: p 14 17 18; satırlar 9px, alt çizgi.
            let mut details = div().flex().flex_col().gap(px(1.)).pt(px(14.)).px(px(17.)).pb(px(18.));
            for (i, (k, v)) in rows.into_iter().enumerate() {
                details = details.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .py(px(9.))
                        .when(i + 1 < n, |s| s.border_b_1().border_color(p.border_subtle))
                        .child(text(fs(10.)).text_color(p.text_3).child(k))
                        .child(div().max_w(gpui_kit::relative(0.62)).text_size(fs(11.)).line_height(gpui_kit::relative(1.45)).css_weight(650.).text_color(p.text).child(v)),
                );
            }
            col = col.child(settings_block("Bilgiler", true, p).child(details));
        }
        col.into_any_element()
    }

    fn confirm_dialog(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, body, label) = match self.confirm.as_ref()? {
            Confirm::Remove(m) => ("Üyeyi çıkar".to_string(), format!("{} kuruluştan çıkarılacak. Gruplardan da çıkarılır ve kuruluş içeriğine erişemez.", m.display_name), "Kuruluştan çıkar"),
            Confirm::Transfer(m) => ("Sahipliği devret".to_string(), format!("{} kuruluşun sahibi olacak, sen düz üyeye döneceksin. Bu işlemi geri almak için yeni sahibin sahipliği sana devretmesi gerekir.", m.display_name), "Sahipliği devret"),
            Confirm::Purge(pr) => (
                "Proje kalıcı olarak silinsin mi?".to_string(),
                format!("“{}” (kod: {}) ve projeye bağlı tüm kanallar, mesajlar, panolar ve görevler geri dönülmez biçimde silinecek. Silindikten sonra aynı kodla yeni bir proje açabilirsin.", pr.name, pr.slug),
                "Kalıcı olarak sil",
            ),
        };
        let (card, close, cancel, ok) = confirm_modal(title, body, label, true, window, p);
        let dismiss = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.confirm = None;
            cx.notify();
        };
        let close = close.on_click(cx.listener(dismiss));
        // Kapat düğmesi başlığın sağında (modal_head'e `close` verilmediği için mutlak konum).
        let card = card
            .child(modal_foot(p).child(cancel.on_click(cx.listener(dismiss))).child(ok.on_click(cx.listener(|this, _, window, cx| this.run_confirm(window, cx)))))
            .child(div().absolute().top(px(18.)).right(px(16.)).child(close))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("admin-confirm-card", Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            div()
                .id("admin-confirm")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(24.))
                .pb(px(24.))
                .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }
}

impl Render for AdminPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let (w, h) = modal_size(1080., 760., window);
        let mut nav = settings_nav("Yönetim", format!("{} · {}", self.org.name, if self.perms.owner { "Kuruluş sahibi" } else { "Üye" }), &p);
        for (section, label, sub) in self.visible() {
            nav = nav.child(nav_button(SharedString::from(format!("adm-{label}")), label, Some(sub), self.section == section, &p).on_click(cx.listener(move |this, _, window, cx| this.open(section, window, cx))));
        }
        let content_w = w - 2. - 230. - 40.;
        let body = match self.section {
            Section::Overview => self.overview_view(content_w, &p),
            Section::Team => self.team_view(content_w, &p, window, cx),
            Section::Access => self.access_view(&p, cx),
            Section::Invites => self.invites_view(&p, window, cx),
            Section::Library => self.library_view(&p, cx),
            Section::Channels => self.channels_view(&p),
            Section::Trash => self.trash_view(&p, cx),
            Section::Organization => self.organization_view(&p, window, cx),
        };
        let title = SECTIONS.iter().find(|s| s.0 == self.section).map(|s| s.1).unwrap_or("Yönetim");
        let access = self.section == Section::Access;
        let scroll = div()
            .id(("admin-scroll", self.section as usize))
            .flex_1()
            .min_h_0()
            .pt(px(16.))
            .px(px(20.))
            .pb(px(30.))
            .flex()
            .flex_col()
            .map(|s| if access { s.overflow_hidden() } else { s.overflow_y_scroll() })
            .when_some(self.error.clone(), |s, m| s.child(warn(m, &p)))
            .when_some(self.notice.clone().filter(|_| self.error.is_none()), |s, n| s.child(hint(n, &p)))
            .child(body)
            .p2_enter(("admin-body", self.section as usize), Entrance::Rise, std::time::Duration::ZERO);
        let card = settings_card("admin-card", w, h, &p)
            .track_focus(&self.focus)
            .child(nav)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(settings_header(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title).into_any_element(), div().id("admin-close").on_click(cx.listener(|_, _, _, cx| cx.emit(AdminEvent::Close))), &p))
                    .child(scroll),
            )
            .p2_enter("admin", Entrance::PopElastic, std::time::Duration::ZERO);
        let confirm = self.confirm_dialog(&p, window, cx);
        settings_layer("admin-scrim", card, &p)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.confirm.is_none() {
                    cx.emit(AdminEvent::Close)
                }
            }))
            .children(confirm)
    }
}
