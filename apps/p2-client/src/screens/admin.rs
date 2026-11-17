//! Yönetim paneli (AdminPanel.tsx + AdminTeam.tsx + AdminAccess.tsx), proje topluluklarına göre
//! (docs/proje-bazli-yapi.md): her projenin kendi paneli vardır — Genel, Üyeler, Roller, Gruplar,
//! İzinler, Davetler, Yasaklılar, Kanallar, Ayarlar. Her bölüm o projedeki yetkime bağlı; yetkisi
//! olmayan sekmeyi hiç görmez (`/projects/:p/permissions/me`). Kurulum sahibinin (kuruluş `OWNER`)
//! ayrı "Sunucu yönetimi" alanı aynı kabuğu kullanır: Kuruluş (ad), Oturumlar, Geri Dönüşüm.
//!
//! Görünüm settings.css'in birebir karşılığıdır (`.settings-layer`, `.settings-modal.admin-modal`,
//! `.settings-block`, `.settings-button`, `.admin-*`, `.team-*`, `.perm-line`). Kanal izinleri penceresi
//! (`channel_perms.rs`) aynı kabuğu kullanır; ortak parçalar bu dosyada `pub(crate)`. Çizim
//! `admin_views.rs`'tedir (alt modül).

use std::collections::HashMap;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;

use chrono::{DateTime, Datelike, Local, Timelike};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, FontWeight, Hsla, Pixels, SharedString, Stateful, Subscription, Window, div, px, rgb};
use p2_client_core::{AccessData, AdminChannel, Api, ApiError, DeletedProject, Member, MyPermissions, Organization, Overview, PermissionInfo, PermissionMember, Project, ProjectBan, ProjectInvite, ProjectUserGroup, TeamMember, TeamRole};
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, lh, modal, modal_body, modal_head};
use p2_ui::form::InputKind;
use p2_ui::icons::icon;
use p2_ui::tokens::{Palette, Shadow, css_rgba, font_size, fs, radius, shadows};

use crate::task;

#[path = "admin_views.rs"]
mod views;

/// AdminTeam.tsx `ROLE_COLORS`.
pub const ROLE_COLORS: [&str; 8] = ["#5865f2", "#3ba55d", "#faa61a", "#ed4245", "#9b59b6", "#1abc9c", "#e91e63", "#95a5a6"];
const MONTHS: [&str; 12] = ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    // Proje paneli
    Overview,
    Members,
    Roles,
    Groups,
    Access,
    Invites,
    Bans,
    Channels,
    Settings,
    // Sunucu yönetimi (kurulum sahibi)
    Organization,
    Sessions,
    Trash,
}

/// Proje paneli: (bölüm, ad, ipucu, gereken proje yetkisi).
const PROJECT_SECTIONS: [(Section, &str, &str, Option<&str>); 9] = [
    (Section::Overview, "Genel", "Projenin özeti", Some("admin.panel")),
    (Section::Members, "Üyeler", "Kişiler, sahiplik, çıkarma", Some("admin.panel")),
    (Section::Roles, "Roller", "Rol oluştur, yetki seç, kişiye ata", Some("admin.panel")),
    (Section::Groups, "Gruplar", "@etiketli kullanıcı grupları", Some("admin.panel")),
    (Section::Access, "İzinler", "Kanal görünürlüğü, kişiye özel yetki", Some("member.roles")),
    (Section::Invites, "Davetler", "Davet kodları ve sınırları", Some("member.invite")),
    (Section::Bans, "Yasaklılar", "Projeden yasaklanan kişiler", Some("member.kick")),
    (Section::Channels, "Kanallar", "Kanallar ve mesaj sayıları", Some("admin.panel")),
    (Section::Settings, "Ayarlar", "Ad, açıklama, görseller, kütüphane", Some("organization.manage")),
];

/// Sunucu yönetimi (yalnız kurulum sahibi açar).
const INSTANCE_SECTIONS: [(Section, &str, &str, Option<&str>); 3] = [
    (Section::Organization, "Kuruluş", "Kurulumun adı ve bilgileri", None),
    (Section::Sessions, "Oturumlar", "Bir kişinin bütün oturumlarını kapat", None),
    (Section::Trash, "Geri Dönüşüm", "Silinen projeleri kalıcı kaldır", None),
];

/// Panelin kapsamı: bir proje topluluğu ya da kurulumun kendisi (kuruluş kabı).
#[derive(Clone)]
pub enum AdminScope {
    Project(Project),
    Instance,
}

pub enum AdminEvent {
    Close,
    Renamed(String),
    /// `p2:toast` (kalıcı silme sonucu): çalışma alanı bildirimi gösterir.
    Toast(String),
    /// Proje ayarları kaydedildi (ray adı/ikonu güncellenir).
    ProjectUpdated(Project),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TeamTab {
    Members,
    Roles,
    Groups,
}

enum Confirm {
    Remove(TeamMember),
    Ban(TeamMember),
    Transfer(TeamMember),
    Purge(DeletedProject),
    Sessions(Member),
}

/// Açık "＋ rol ver" türü seçici (Electron'da yerel `<select>`).
#[derive(Clone, PartialEq)]
enum Picker {
    GiveRole(String),
    GivePerson(String),
    /// Gruba kişi ekle (grup kimliği).
    AddToGroup(String),
    /// Gruba rol bağla (grup kimliği).
    LinkRole(String),
    /// Role grup bağla (rol kimliği).
    LinkGroup(String),
    /// Proje ayarları: başlangıç kanalı.
    DefaultChannel,
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

/// CSS `text-transform: uppercase` (`lang="tr"`: i → İ; diğer dillerde düz büyük harf).
pub(crate) fn tr_upper(s: &str) -> String {
    if p2_i18n::language().id != "tr" {
        return s.to_uppercase();
    }
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
/// Projenin Keşfet görünürlüğü (eski sunucu alanı göndermezse özel sayılır).
fn visibility_of(p: &Project) -> &str {
    if p.visibility == p2_client_core::VISIBILITY_PUBLIC { p2_client_core::VISIBILITY_PUBLIC } else { p2_client_core::VISIBILITY_PRIVATE }
}

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
pub(crate) fn admin_avatar(key: &str, name: &str, small: bool, p: &Palette) -> Div {
    let (size, r, f) = if small { (26., 9., fs(9.)) } else { (36., 12., fs(11.)) };
    let face = match crate::avatars::image(key) {
        Some(img) => gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(r)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
        None => initials(name).into_any_element(),
    };
    p2_ui::controls::avatar_box(size, p2_ui::components::avatar_hue(key), false, p).overflow_hidden().rounded(px(r)).text_size(f).tracking(0.).child(face)
}

/// direct.css `.presence` (+ `.online/.away/.busy`); `.admin-avatar > .presence` sağ/alt −2px.
pub(crate) fn presence(status: &str, p: &Palette) -> Div {
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
/// Elips merkezi ve yatay yarıçap genişlikle orantılı, dikey sabit: SVG sabit `GLOW_W` genişlikte bir kez
/// rasterlenip yatayda esnetilir (birebir). Pencere boyutlandırılırken kart genişliği değişse de yeni
/// görsel üretilmez (her karede arka planda rasterlenene dek boş kalıp titriyordu).
pub(crate) fn modal_glow(w: f32, p: &Palette) -> AnyElement {
    const GLOW_W: f32 = 600.;
    let (card_w, w) = (w, GLOW_W);
    let h = 210.;
    let (c, a) = p2_ui::icons::svg_color(p.accent_soft);
    let markup = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({cx} 0) scale({rx} {ry})"><stop offset="0" stop-color="{c}" stop-opacity="{a}"/><stop offset="0.46" stop-color="rgb(44,123,255)" stop-opacity="0.03"/><stop offset="0.72" stop-color="rgb(44,123,255)" stop-opacity="0"/></radialGradient></defs><rect width="{w}" height="{h}" fill="url(#g)"/></svg>"#,
        cx = w * 0.22,
        rx = w * 1.1 * 1.4142,
        ry = h * 1.4142,
    );
    div().absolute().top_0().left_0().w(px(card_w)).h(px(h)).child(p2_ui::icons::svg_image(markup).object_fit(gpui_kit::ObjectFit::Fill).w(px(card_w)).h(px(h))).into_any_element()
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
    scope: AdminScope,
    /// Panelin açıldığı proje (otomatik pilot sunucu yönetiminden proje bölümüne dönerken).
    home: Option<Project>,
    me: String,
    pub perms: MyPermissions,
    section: Section,
    overview: Option<Overview>,
    members: Vec<TeamMember>,
    roles: Vec<TeamRole>,
    groups: Vec<ProjectUserGroup>,
    catalog: Vec<PermissionInfo>,
    /// Kişiye özel yetkiler (allow / deny), `admin/permissions` cevabından.
    overrides: HashMap<String, PermissionMember>,
    invites: Vec<ProjectInvite>,
    bans: Vec<ProjectBan>,
    trash: Vec<DeletedProject>,
    channels: Vec<AdminChannel>,
    /// Sunucu yönetimi › Oturumlar: benimle proje paylaşan kişiler.
    people: Vec<Member>,
    access: Option<AccessData>,
    access_user: Option<String>,
    access_error: Option<String>,
    busy: bool,
    error: Option<String>,
    notice: Option<String>,
    tab: TeamTab,
    selected_member: Option<String>,
    selected_role: Option<String>,
    selected_group: Option<String>,
    creating: bool,
    color: &'static str,
    picker: Option<Picker>,
    confirm: Option<Confirm>,
    /// Proje ayarları taslağı: ikon, kapak, başlangıç kanalı, kütüphane; görsel yükleniyor mu.
    draft_icon: Option<String>,
    draft_cover: Option<String>,
    draft_default: Option<String>,
    draft_library: bool,
    /// Keşfet: görünürlük (`PUBLIC` / `PRIVATE`) ve kategori; tanıtım satırı `project_tagline`.
    draft_visibility: String,
    draft_category: Option<String>,
    uploading: Option<bool>,
    search: Entity<InputState>,
    role_name: Entity<InputState>,
    role_desc: Entity<InputState>,
    invite_uses: Entity<InputState>,
    invite_days: Entity<InputState>,
    org_name: Entity<InputState>,
    project_name: Entity<InputState>,
    project_desc: Entity<InputState>,
    project_tagline: Entity<InputState>,
    ban_reason: Entity<InputState>,
    focus: FocusHandle,
    /// Otomatik pilot adımları (testler).
    steps: Vec<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<AdminEvent> for AdminPanel {}

/// Görsel yükle (`/uploads/...` yolu döner): proje ayarları ve proje ikonu/kapak penceresi ortak.
pub(crate) fn upload_image(api: &Api, path: &std::path::Path) -> Result<String, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "gorsel.png".into());
    if !p2_client_core::mime_for(&name).starts_with("image/") {
        return Err("Yalnız görsel dosyası yüklenebilir.".to_string());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("Görsel 8 MB sınırını aşıyor.".into());
    }
    let url = api.upload_file(&name, &data).map(|f| f.url).map_err(|e| if e.message.is_empty() { "Görsel yüklenemedi.".into() } else { e.message })?;
    Ok(url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or(url))
}

/// Hata iletisi: davet/proje kodları için arayüz metni, yoksa sunucunun iletisi.
pub(crate) fn error_text(e: &ApiError, fallback: &str) -> String {
    p2_client_core::community_error_text(&e.code).map(str::to_string).unwrap_or_else(|| if e.message.is_empty() { fallback.to_string() } else { e.message.clone() })
}

impl AdminPanel {
    pub fn new(api: Arc<Api>, org: Organization, scope: AdminScope, me: String, perms: MyPermissions, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let project_name = input("Proje adı", window, cx);
        let project_desc = input("Ekibine projenin amacını kısaca anlat", window, cx);
        let project_tagline = input(&p2_i18n::t("vis.tagline.placeholder"), window, cx);
        let (mut draft_visibility, mut draft_category) = (p2_client_core::VISIBILITY_PRIVATE.to_string(), None);
        let ban_reason = input("isteğe bağlı", window, cx);
        let name = org.name.clone();
        org_name.update(cx, |s, cx| s.set_value(name, window, cx));
        let (mut draft_icon, mut draft_cover, mut draft_default, mut draft_library) = (None, None, None, false);
        if let AdminScope::Project(p) = &scope {
            let (n, d) = (p.name.clone(), p.description.clone().unwrap_or_default());
            project_name.update(cx, |s, cx| s.set_value(n, window, cx));
            project_desc.update(cx, |s, cx| s.set_value(d, window, cx));
            (draft_icon, draft_cover, draft_default, draft_library) = (p.icon_url.clone(), p.cover_url.clone(), p.default_channel_id.clone(), p.library_enabled);
            (draft_visibility, draft_category) = (visibility_of(p).to_string(), p.category.clone());
            let tagline = p.tagline.clone().unwrap_or_default();
            project_tagline.update(cx, |s, cx| s.set_value(tagline, window, cx));
        }
        let mut subs: Vec<Subscription> = [&search, &role_name, &org_name, &role_desc, &project_name, &project_desc, &ban_reason].into_iter().map(|e| cx.subscribe(e, |_, _, _: &InputEvent, cx| cx.notify())).collect();
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
        let home = match &scope {
            AdminScope::Project(p) => Some(p.clone()),
            AdminScope::Instance => None,
        };
        let mut panel = Self {
            api,
            org,
            scope,
            home,
            me,
            perms,
            section: Section::Overview,
            overview: None,
            members: Vec::new(),
            roles: Vec::new(),
            groups: Vec::new(),
            catalog: Vec::new(),
            overrides: HashMap::new(),
            invites: Vec::new(),
            bans: Vec::new(),
            trash: Vec::new(),
            channels: Vec::new(),
            people: Vec::new(),
            access: None,
            access_user: None,
            access_error: None,
            busy: false,
            error: None,
            notice: None,
            tab: TeamTab::Members,
            selected_member: None,
            selected_role: None,
            selected_group: None,
            creating: false,
            color: ROLE_COLORS[0],
            picker: None,
            confirm: None,
            draft_icon,
            draft_cover,
            draft_default,
            draft_library,
            draft_visibility,
            draft_category,
            uploading: None,
            search,
            role_name,
            role_desc,
            invite_uses,
            invite_days,
            org_name,
            project_name,
            project_desc,
            project_tagline,
            ban_reason,
            focus,
            steps,
            _subs: subs,
        };
        crate::trace(&match &panel.scope {
            AdminScope::Project(p) => format!("yönetim: panel açıldı ({})", p.name),
            AdminScope::Instance => "yönetim: sunucu yönetimi açıldı".into(),
        });
        let first = panel.visible().first().map(|s| s.0).unwrap_or(Section::Overview);
        panel.section = first;
        panel.load(first, window, cx);
        panel
    }

    /// Bu panelin kimliği: proje paneli için proje, sunucu yönetimi için kuruluş.
    fn key(&self) -> String {
        match &self.scope {
            AdminScope::Project(p) => p.id.clone(),
            AdminScope::Instance => self.org.id.clone(),
        }
    }

    /// Proje ayarları taslağını projenin kayıtlı Keşfet alanlarına geri al.
    pub(super) fn reset_discover_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.project().cloned() {
            (self.draft_visibility, self.draft_category) = (visibility_of(&p).to_string(), p.category.clone());
            self.project_tagline.update(cx, |s, cx| s.set_value(p.tagline.clone().unwrap_or_default(), window, cx));
        }
    }

    fn project(&self) -> Option<&Project> {
        match &self.scope {
            AdminScope::Project(p) => Some(p),
            AdminScope::Instance => None,
        }
    }

    fn is_instance(&self) -> bool {
        matches!(self.scope, AdminScope::Instance)
    }

    /// Projenin birincil sahibi miyim (sahipliği yalnız o devreder).
    fn primary_owner(&self) -> bool {
        self.project().and_then(|p| p.owner_id.as_deref()) == Some(self.me.as_str()) || self.members.iter().any(|m| m.id == self.me && m.primary_owner)
    }

    fn visible(&self) -> Vec<(Section, &'static str, &'static str)> {
        let list: &[(Section, &str, &str, Option<&str>)] = if self.is_instance() { &INSTANCE_SECTIONS } else { &PROJECT_SECTIONS };
        list.iter().filter(|s| s.3.is_none_or(|need| self.perms.can(need))).map(|s| (s.0, s.1, s.2)).collect()
    }

    fn fail(&mut self, e: ApiError, cx: &mut Context<Self>) {
        crate::trace(&format!("yönetim: hata {} {}", e.code, e.message));
        self.error = Some(error_text(&e, "İşlem tamamlanamadı."));
        self.busy = false;
        cx.notify();
    }

    pub fn open(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        self.error = None;
        self.notice = None;
        self.picker = None;
        self.creating = false;
        match section {
            Section::Members => self.tab = TeamTab::Members,
            Section::Roles => self.tab = TeamTab::Roles,
            Section::Groups => self.tab = TeamTab::Groups,
            _ => {}
        }
        crate::trace(&format!("yönetim: bölüm {section:?}"));
        self.load(section, window, cx);
        cx.notify();
    }

    /// Sunucu yönetimine geç (otomatik pilot / kurulum sahibi).
    fn switch_scope(&mut self, scope: AdminScope, window: &mut Window, cx: &mut Context<Self>) {
        self.scope = scope;
        let first = self.visible().first().map(|s| s.0).unwrap_or(Section::Overview);
        self.open(first, window, cx);
    }

    fn load(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        let (api, key, org) = (self.api.clone(), self.key(), self.org.id.clone());
        match section {
            Section::Overview => task::run(window, cx, move || api.project_overview(&key), |this, r, window, cx| match r {
                Ok(o) => {
                    this.overview = Some(o);
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Organization => task::run(window, cx, move || api.admin_overview(&org), |this, r, window, cx| match r {
                Ok(o) => {
                    this.overview = Some(o);
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Members | Section::Roles | Section::Groups => self.refresh_team(window, cx),
            Section::Access => {
                let can_roles = self.perms.can("member.roles");
                task::run(window, cx, move || (api.project_access(&key), if can_roles { api.project_admin_permissions(&key).ok() } else { None }), |this, (r, perms), window, cx| match r {
                    Ok(data) => {
                        if this.access_user.as_ref().is_none_or(|u| !data.members.iter().any(|m| &m.id == u)) {
                            this.access_user = data.members.iter().find(|m| !m.owner).or(data.members.first()).map(|m| m.id.clone());
                        }
                        if let Some(p) = perms {
                            this.overrides = p.members.into_iter().map(|m| (m.id.clone(), m)).collect();
                            this.catalog = p.catalog;
                        }
                        crate::trace(&format!("yönetim: erişim {} kişi, {} proje", data.members.len(), data.projects.len()));
                        this.access = Some(data);
                        this.busy = false;
                        cx.notify();
                        this.next_step(window, cx);
                    }
                    Err(e) => {
                        this.access_error = Some(error_text(&e, "Erişim bilgileri yüklenemedi."));
                        this.busy = false;
                        cx.notify();
                    }
                })
            }
            Section::Invites => task::run(window, cx, move || api.project_invites(&key, false), |this, r, window, cx| match r {
                Ok(list) => {
                    crate::trace(&format!("yönetim: {} davet kodu", list.len()));
                    this.invites = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Bans => task::run(window, cx, move || api.project_bans(&key), |this, r, window, cx| match r {
                Ok(list) => {
                    crate::trace(&format!("yönetim: {} yasaklı", list.len()));
                    for b in &list {
                        crate::avatars::remember(&b.user.id, b.user.avatar_url.as_deref());
                    }
                    this.bans = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Channels | Section::Settings => task::run(window, cx, move || api.project_admin_channels(&key), |this, r, window, cx| match r {
                Ok(list) => {
                    this.channels = list;
                    cx.notify();
                    this.next_step(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Section::Sessions => task::run(window, cx, move || api.members(&org, None), |this, r, window, cx| match r {
                Ok(list) => {
                    for m in &list {
                        crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                    }
                    this.people = list;
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

    /// AdminPanel.tsx `refreshTeam`: üyeler, roller, gruplar ve (yetkiliyse) yetki kataloğu — projeye göre.
    fn refresh_team(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, key) = (self.api.clone(), self.key());
        let can_roles = self.perms.can("member.roles");
        task::run(
            window,
            cx,
            move || {
                let members = api.project_admin_members(&key)?;
                let roles = api.project_roles(&key)?;
                let groups = api.project_user_groups(&key).unwrap_or_default();
                let permissions = if can_roles { api.project_admin_permissions(&key).ok() } else { None };
                Ok::<_, ApiError>((members, roles, groups, permissions))
            },
            |this, r, window, cx| match r {
                Ok((members, roles, groups, permissions)) => {
                    crate::trace(&format!("yönetim: {} üye, {} rol, {} grup", members.len(), roles.len(), groups.len()));
                    for m in &members {
                        crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                    }
                    this.members = members;
                    this.roles = roles;
                    this.groups = groups;
                    if let Some(p) = permissions {
                        this.overrides = p.members.into_iter().map(|m| (m.id.clone(), m)).collect();
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
    /// (testler), `notice` ekranda görünen metin. `work` panel kimliğini (proje) alır.
    fn team_action(&mut self, trace: String, notice: String, work: impl FnOnce(&Api, &str) -> Result<serde_json::Value, ApiError> + Send + 'static, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.notice = None;
        self.picker = None;
        cx.notify();
        let (api, key) = (self.api.clone(), self.key());
        task::run(window, cx, move || work(&api, &key), move |this, r, window, cx| match r {
            Ok(_) => {
                crate::trace(&format!("yönetim: {trace}"));
                this.notice = Some(notice);
                this.refresh_team(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
    }

    /// Bölüm eylemi (davet, yasak, ayar, kuruluş, oturum, erişim): bitince bölümü yeniden yükler.
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
        let (api, key) = (self.api.clone(), self.key());
        task::run(window, cx, move || work(&api, &key), move |this, r, window, cx| match r {
            Ok(value) => {
                this.busy = false;
                crate::trace(&format!("yönetim: {trace}"));
                if this.section == Section::Organization {
                    if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                        this.org.name = name.to_string();
                        cx.emit(AdminEvent::Renamed(name.to_string()));
                    }
                }
                if this.section == Section::Settings {
                    if let Ok(saved) = serde_json::from_value::<Project>(value.clone()) {
                        if let AdminScope::Project(p) = &mut this.scope {
                            *p = saved.clone();
                        }
                        this.home = Some(saved.clone());
                        cx.emit(AdminEvent::ProjectUpdated(saved));
                    }
                }
                if let Some(n) = notice {
                    this.notice = Some(match (value.get("rotated").and_then(|c| c.as_str()), value.get("closed").and_then(|c| c.as_u64())) {
                        // Varsayılan kodu yenile: "Yeni davet kodu: KOD"
                        (Some(code), _) => format!("Yeni davet kodu: {code}"),
                        // Oturum kapatma: "{ad} için {n} oturum kapatıldı."
                        (_, Some(closed)) if n.ends_with(" oturumları kapatıldı.") => format!("{} için {closed} oturum kapatıldı.", n.trim_end_matches(" oturumları kapatıldı.")),
                        _ => n,
                    });
                }
                this.load(this.section, window, cx);
            }
            Err(e) => {
                if this.section == Section::Access {
                    this.busy = false;
                    this.access_error = Some(error_text(&e, "İşlem tamamlanamadı."));
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

    fn group_by_name(&self, name: &str) -> Option<ProjectUserGroup> {
        self.groups.iter().find(|g| g.name == name || g.handle == name).cloned()
    }

    /// Yeni rol ya da (Gruplar sekmesinde) yeni kullanıcı grubu.
    fn create_role(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.role_name.read(cx).value().trim().to_string();
        let desc = self.role_desc.read(cx).value().trim().to_string();
        if name.chars().count() < 2 || p2_client_core::role_handle(&name).len() < 2 {
            return;
        }
        let color = self.color.to_string();
        self.creating = false;
        if self.tab == TeamTab::Groups {
            let handle = p2_client_core::role_handle(&name);
            let label = format!("{name} grubu oluşturuldu.");
            self.team_action(label, "Grup oluşturuldu.".into(), move |api, p| api.create_project_user_group(p, &handle, &name, &color).map(|_| serde_json::Value::Null), window, cx);
        } else {
            let label = format!("{name} rolü oluşturuldu.");
            self.team_action(label, "Rol oluşturuldu.".into(), move |api, p| api.create_project_role(p, &name, &desc, &color).map(|_| serde_json::Value::Null), window, cx);
        }
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
        self.team_action(label, "Rol yetkileri güncellendi.".into(), move |api, p| api.set_project_role_permissions(p, &rid, &next), window, cx);
    }

    fn assign(&mut self, user: String, role: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let label = if on { "Rol verildi." } else { "Rol geri alındı." }.to_string();
        self.team_action(label.clone(), label, move |api, p| api.assign_project_role(p, &role, &user, on), window, cx);
    }

    fn group_member(&mut self, group: String, user: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let label = if on { "Kişi gruba eklendi." } else { "Kişi gruptan çıkarıldı." }.to_string();
        self.team_action(label.clone(), label, move |api, p| if on { api.add_project_user_group_members(p, &group, &[user]).map(|_| serde_json::Value::Null) } else { api.remove_project_user_group_member(p, &group, &user) }, window, cx);
    }

    fn link_group(&mut self, role: String, group: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let label = if on { "Rol gruba bağlandı." } else { "Rol bağlantısı kaldırıldı." }.to_string();
        self.team_action(label.clone(), label, move |api, p| api.link_project_role_group(p, &role, &group, on), window, cx);
    }

    /// Kişiye özel yetki: `allow` / `deny` / `inherit` (İzinler sekmesi).
    fn set_override(&mut self, user: String, permission: String, state: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.overrides.get(&user).cloned().unwrap_or_default();
        let (mut allow, mut deny) = (current.allow, current.deny);
        allow.retain(|p| p != &permission);
        deny.retain(|p| p != &permission);
        match state {
            "allow" => allow.push(permission.clone()),
            "deny" => deny.push(permission.clone()),
            _ => {}
        }
        let name = self.access.as_ref().and_then(|a| a.members.iter().find(|m| m.id == user)).map(|m| m.display_name.clone()).or_else(|| self.members.iter().find(|m| m.id == user).map(|m| m.display_name.clone())).unwrap_or_default();
        let label = format!("{name} için {permission} → {state}");
        self.section_action(label, Some("Kişiye özel yetkiler güncellendi.".into()), move |api, p| api.set_member_permissions(p, &user, &allow, &deny), window, cx);
    }

    fn create_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uses = self.invite_uses.read(cx).value().trim().parse::<u64>().ok().filter(|n| *n > 0);
        let days = self.invite_days.read(cx).value().trim().parse::<u64>().ok().filter(|n| *n > 0);
        self.invite_uses.update(cx, |s, cx| s.set_value("", window, cx));
        self.invite_days.update(cx, |s, cx| s.set_value("", window, cx));
        let label = "Yeni davet kodu oluşturuldu.".to_string();
        self.section_action(label.clone(), Some(label), move |api, p| api.create_project_invite(p, uses, days).map(|i| serde_json::json!({ "code": i.code })), window, cx);
    }

    fn rename_org(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.org_name.read(cx).value().trim().to_string();
        if name.is_empty() || name == self.org.name {
            return;
        }
        let label = "Kuruluş adı güncellendi.".to_string();
        self.section_action(label.clone(), Some(label), move |api, org| api.rename_organization(org, &name), window, cx);
    }

    /// Proje ayarları taslağında değişen alanlar (boşsa kaydedilecek bir şey yok).
    fn settings_patch(&self, cx: &App) -> serde_json::Map<String, serde_json::Value> {
        let mut patch = serde_json::Map::new();
        let Some(p) = self.project() else { return patch };
        let name = self.project_name.read(cx).value().trim().to_string();
        let desc = self.project_desc.read(cx).value().trim().to_string();
        if !name.is_empty() && name != p.name {
            patch.insert("name".into(), serde_json::json!(name));
        }
        if desc != p.description.clone().unwrap_or_default() {
            patch.insert("description".into(), serde_json::json!(desc));
        }
        if self.draft_icon != p.icon_url {
            patch.insert("iconUrl".into(), serde_json::json!(self.draft_icon));
        }
        if self.draft_cover != p.cover_url {
            patch.insert("coverUrl".into(), serde_json::json!(self.draft_cover));
        }
        if self.draft_default != p.default_channel_id {
            patch.insert("defaultChannelId".into(), serde_json::json!(self.draft_default));
        }
        if self.draft_library != p.library_enabled && self.perms.can("release.publish") {
            patch.insert("libraryEnabled".into(), serde_json::json!(self.draft_library));
        }
        // Keşfet alanları.
        if self.draft_visibility != visibility_of(p) {
            patch.insert("visibility".into(), serde_json::json!(self.draft_visibility));
        }
        if self.draft_category != p.category {
            patch.insert("category".into(), serde_json::json!(self.draft_category.clone().unwrap_or_default()));
        }
        let tagline = self.project_tagline.read(cx).value().trim().to_string();
        if tagline != p.tagline.clone().unwrap_or_default() {
            patch.insert("tagline".into(), serde_json::json!(tagline));
        }
        patch
    }

    fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let patch = self.settings_patch(cx);
        if patch.is_empty() {
            return;
        }
        let keys: Vec<String> = patch.keys().cloned().collect();
        let label = format!("Proje ayarları güncellendi ({}).", keys.join(", "));
        self.section_action(label, Some("Proje ayarları kaydedildi.".into()), move |api, p| api.update_project_settings(p, serde_json::Value::Object(patch)).map(|saved| serde_json::to_value(saved).unwrap_or_default()), window, cx);
    }

    /// Ayarlar › ikon / kapak: dosya seç ve yükle (taslağa yazılır, "Kaydet" ile projeye).
    fn pick_image(&mut self, cover: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(if cover { "Kapak seç".into() } else { "İkon seç".into() }) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| this.upload(cover, path, window, cx));
                }
            }
        })
        .detach();
    }

    fn upload(&mut self, cover: bool, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        self.uploading = Some(cover);
        self.error = None;
        cx.notify();
        task::run(window, cx, move || upload_image(&api, &path), move |this, r, window, cx| {
            this.uploading = None;
            match r {
                Ok(path) => {
                    crate::trace(&format!("yönetim: {} yüklendi {path}", if cover { "kapak" } else { "ikon" }));
                    if cover {
                        this.draft_cover = Some(path);
                    } else {
                        this.draft_icon = Some(path);
                    }
                    this.next_step(window, cx);
                }
                Err(e) => this.error = Some(e),
            }
            cx.notify();
        });
    }

    fn revoke_invite(&mut self, invite: ProjectInvite, window: &mut Window, cx: &mut Context<Self>) {
        let trace = format!("{} davet kodu iptal edildi.", invite.code);
        self.section_action(trace, Some("Davet kodu iptal edildi.".into()), move |api, p| api.revoke_project_invite(p, &invite.id), window, cx);
    }

    fn rotate_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.section_action("Davet kodu yenilendi.".into(), Some("Davet kodu yenilendi.".into()), |api, p| api.rotate_project_invite(p).map(|i| serde_json::json!({ "rotated": i.code })), window, cx);
    }

    fn copy_invite(&mut self, code: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(code.to_string()));
        crate::trace(&format!("yönetim: davet kodu kopyalandı {code}"));
        self.notice = Some(format!("{code} kopyalandı. Bu kodu alan kişi yalnız bu projeye katılır."));
        cx.notify();
    }

    fn unban(&mut self, ban: ProjectBan, window: &mut Window, cx: &mut Context<Self>) {
        let (id, label) = (ban.user_id.clone(), format!("{} yasağı kaldırıldı.", ban.user.display_name));
        self.section_action(label.clone(), Some(label), move |api, p| api.unban_member(p, &id), window, cx);
    }

    fn ask_ban(&mut self, member: TeamMember, window: &mut Window, cx: &mut Context<Self>) {
        self.ban_reason.update(cx, |s, cx| s.set_value("", window, cx));
        self.confirm = Some(Confirm::Ban(member));
        cx.notify();
    }

    fn run_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirm.take() {
            Some(Confirm::Remove(m)) => {
                let (id, label) = (m.id.clone(), format!("{} projeden çıkarıldı.", m.display_name));
                self.team_action(label.clone(), label, move |api, p| api.kick_member(p, &id), window, cx);
            }
            Some(Confirm::Ban(m)) => {
                let reason = self.ban_reason.read(cx).value().trim().to_string();
                let (id, label) = (m.id.clone(), format!("{} projeden yasaklandı.", m.display_name));
                self.team_action(label.clone(), label, move |api, p| api.ban_member(p, &id, &reason), window, cx);
            }
            Some(Confirm::Transfer(m)) => {
                let id = m.id.clone();
                let label = format!("Proje sahipliği {} kişisine devredildi.", m.display_name);
                self.team_action(label.clone(), label, move |api, p| api.transfer_project_owner(p, &id), window, cx);
            }
            Some(Confirm::Sessions(m)) => {
                let (id, label) = (m.id.clone(), format!("{} oturumları kapatıldı.", m.display_name));
                self.section_action(label.clone(), Some(label), move |api, org| api.revoke_sessions(org, &id), window, cx);
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
                        Err(e) => cx.emit(AdminEvent::Toast(error_text(&e, "Proje silinemedi."))),
                    }
                    cx.notify();
                });
            }
            None => {}
        }
        cx.notify();
    }

    // ------------------------------------------------------------ otomatik pilot

    /// Adımlar (`P2_AUTOPILOT_ADMIN`, `;` ile): `open:<bölüm>` (overview, team|members, roles, groups,
    /// access, invites, bans, channels, settings|library, organization, sessions, trash), `instance`
    /// (sunucu yönetimine geç), `roles`, `role:Ad`, `create-role`, `perm:Ad:yetki`, `assign:Ad:kullanıcı`,
    /// `unassign:Ad:kullanıcı`, `delete-role:Ad`, `group:Ad`, `group-add:Grup:kullanıcı`,
    /// `group-remove:Grup:kullanıcı`, `delete-group:Ad`, `invite:kullanım:gün`, `revoke-last`,
    /// `copy-invite`, `library:Proje:on|off`, `project-name:Ad`, `save-settings`, `rename:Ad`,
    /// `member:kullanıcı`, `kick:kullanıcı`, `ban:kullanıcı[:neden]`, `unban:kullanıcı`,
    /// `confirm:remove|transfer|ban|sessions:kullanıcı`, `confirm:purge`, `confirm-close`,
    /// `access-user:k`, `access-project|access-channel:ad:on|off`, `override:k:yetki:allow|deny|inherit`,
    /// `wait:ms`, `close`. Görünüm adımları (`roles`, `create-role`, `member`, `confirm:*`) veri yazmaz.
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
                    "team" | "members" => Section::Members,
                    "roles" => Section::Roles,
                    "groups" => Section::Groups,
                    "access" => Section::Access,
                    "invites" => Section::Invites,
                    "bans" => Section::Bans,
                    "channels" => Section::Channels,
                    "settings" | "library" => Section::Settings,
                    "organization" => Section::Organization,
                    "sessions" => Section::Sessions,
                    "trash" => Section::Trash,
                    _ => Section::Overview,
                };
                // Kuruluş bölümleri sunucu yönetiminde (yalnız kurulum sahibi).
                let instance = matches!(section, Section::Organization | Section::Sessions | Section::Trash);
                if instance && !self.is_instance() {
                    self.scope = AdminScope::Instance;
                } else if !instance && self.is_instance() {
                    if let Some(p) = self.home.clone() {
                        self.scope = AdminScope::Project(p);
                    }
                }
                self.open(section, window, cx);
            }
            ["instance"] => self.switch_scope(AdminScope::Instance, window, cx),
            ["roles"] => {
                self.tab = TeamTab::Roles;
                self.section = Section::Roles;
                cx.notify();
                self.next_step(window, cx);
            }
            ["create-role"] => {
                self.tab = TeamTab::Roles;
                self.section = Section::Roles;
                self.creating = true;
                cx.notify();
                self.next_step(window, cx);
            }
            ["member", user] => {
                self.selected_member = user_id(self, user);
                cx.notify();
                self.next_step(window, cx);
            }
            ["confirm", kind @ ("remove" | "transfer" | "ban"), user] => {
                if let Some(m) = self.members.iter().find(|m| m.username == *user).cloned() {
                    self.selected_member = Some(m.id.clone());
                    self.confirm = Some(match *kind {
                        "remove" => Confirm::Remove(m),
                        "ban" => Confirm::Ban(m),
                        _ => Confirm::Transfer(m),
                    });
                }
                cx.notify();
                self.next_step(window, cx);
            }
            ["confirm", "sessions", user] => {
                if let Some(m) = self.people.iter().find(|m| m.username == *user).cloned() {
                    self.confirm = Some(Confirm::Sessions(m));
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
                self.tab = TeamTab::Roles;
                self.role_name.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
                self.create_role(window, cx);
            }
            ["group", name] => {
                self.tab = TeamTab::Groups;
                self.role_name.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
                self.create_role(window, cx);
            }
            [verb @ ("group-add" | "group-remove"), group, user] => match (self.group_by_name(group), user_id(self, user)) {
                (Some(g), Some(u)) => {
                    self.selected_group = Some(g.id.clone());
                    self.group_member(g.id, u, *verb == "group-add", window, cx);
                }
                _ => self.next_step(window, cx),
            },
            ["delete-group", name] => match self.group_by_name(name) {
                Some(g) => {
                    let (id, label) = (g.id.clone(), format!("{} grubu silindi.", g.name));
                    self.team_action(label.clone(), label, move |api, p| api.delete_project_user_group(p, &id), window, cx);
                }
                None => self.next_step(window, cx),
            },
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
                    self.team_action(label.clone(), label, move |api, p| api.delete_project_role(p, &id), window, cx);
                }
                None => self.next_step(window, cx),
            },
            ["kick", user] => match self.members.iter().find(|m| m.username == *user).cloned() {
                Some(m) => {
                    self.confirm = Some(Confirm::Remove(m));
                    self.run_confirm(window, cx);
                }
                None => self.next_step(window, cx),
            },
            ["ban", user, rest @ ..] => match self.members.iter().find(|m| m.username == *user).cloned() {
                Some(m) => {
                    self.ask_ban(m, window, cx);
                    let reason = rest.join(":");
                    self.ban_reason.update(cx, |s, cx| s.set_value(reason, window, cx));
                    self.run_confirm(window, cx);
                }
                None => self.next_step(window, cx),
            },
            ["unban", user] => match self.bans.iter().find(|b| b.user.username == *user).cloned() {
                Some(b) => self.unban(b, window, cx),
                None => self.next_step(window, cx),
            },
            ["invite", uses, days] => {
                self.invite_uses.update(cx, |s, cx| s.set_value(uses.to_string(), window, cx));
                self.invite_days.update(cx, |s, cx| s.set_value(days.to_string(), window, cx));
                self.create_invite(window, cx);
            }
            ["revoke-last"] => match self.invites.iter().max_by(|a, b| a.created_at.cmp(&b.created_at)).cloned() {
                Some(i) => self.revoke_invite(i, window, cx),
                None => self.next_step(window, cx),
            },
            ["copy-invite"] => {
                if let Some(code) = self.invites.first().map(|i| i.code.clone()) {
                    self.copy_invite(&code, cx);
                }
                self.next_step(window, cx);
            }
            ["library", project, state] => {
                match self.project().cloned() {
                    Some(p) if p.name == *project && p.library_enabled != (*state == "on") => {
                        self.draft_library = *state == "on";
                        let trace = format!("{} kütüphanede {}.", p.name, if self.draft_library { "listeleniyor" } else { "gizlendi" });
                        let on = self.draft_library;
                        self.section_action(trace, Some(if on { format!("{} kütüphaneye eklendi.", p.name) } else { format!("{} kütüphaneden kaldırıldı.", p.name) }), move |api, pid| api.update_project_settings(pid, serde_json::json!({ "libraryEnabled": on })).map(|saved| serde_json::to_value(saved).unwrap_or_default()), window, cx);
                    }
                    Some(p) => {
                        crate::trace(&format!("yönetim: {} kütüphanede {}.", p.name, if p.library_enabled { "listeleniyor" } else { "gizlendi" }));
                        self.next_step(window, cx)
                    }
                    None => self.next_step(window, cx),
                }
            }
            // Keşfet görünürlüğü: `visibility:public|private` (kaydeder).
            ["visibility", state] => match self.project().cloned() {
                Some(p) => {
                    let public = *state == "public";
                    let value = if public { p2_client_core::VISIBILITY_PUBLIC } else { p2_client_core::VISIBILITY_PRIVATE };
                    self.draft_visibility = value.to_string();
                    let label = p2_i18n::translate("tr", if public { "vis.saved.public" } else { "vis.saved.private" }, &[("name", &p.name)]);
                    let notice = p2_i18n::tv(if public { "vis.saved.public" } else { "vis.saved.private" }, &[("name", &p.name)]);
                    self.section_action(label, Some(notice), move |api, pid| api.update_project_settings(pid, serde_json::json!({ "visibility": value })).map(|saved| serde_json::to_value(saved).unwrap_or_default()), window, cx);
                }
                None => self.next_step(window, cx),
            },
            ["project-name", name] => {
                self.project_name.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
                cx.notify();
                self.next_step(window, cx);
            }
            ["save-settings"] => {
                if self.settings_patch(cx).is_empty() {
                    self.next_step(window, cx);
                } else {
                    self.save_settings(window, cx);
                }
            }
            ["rename", name] => {
                if !self.is_instance() {
                    self.scope = AdminScope::Instance;
                    self.section = Section::Organization;
                }
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
            ["override", user, perm, state] => match self.access.as_ref().and_then(|a| a.members.iter().find(|m| m.username == *user)).map(|m| m.id.clone()) {
                Some(u) => {
                    self.access_user = Some(u.clone());
                    self.set_override(u, perm.to_string(), state, window, cx);
                }
                None => self.next_step(window, cx),
            },
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

    /// Erişim: `project` = kişiyi projeye ekle/çıkar (üyelik), değilse kanal görünürlüğü.
    fn set_access(&mut self, project: bool, target: String, user: String, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.access.as_ref().and_then(|a| a.members.iter().find(|m| m.id == user)).map(|m| m.display_name.clone()).unwrap_or_default();
        let label = format!("{name} için {} {}.", if project { "proje" } else { "kanal" }, if on { "görünür" } else { "gizli" });
        self.section_action(label, None, move |api, p| if project { api.set_member_access(p, &user, on) } else { api.set_member_channel_access(p, &target, &user, on) }, window, cx);
    }
}
