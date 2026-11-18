//! Çalışma alanı iskeleti: proje rayı (72px) ve kanal kenar çubuğu (252px).
//!
//! Electron referansı birebir: `src/Workspace.tsx` (`ChannelSidebar`, `.project-rail` JSX'i),
//! `src/project-rail.css`, `src/direct.css` (kişi satırı, durum noktası, okunmamış rozeti),
//! `src/call.css` (`.voice-count`, `.voice-channel-badges`). Boyutlar CSS px'tir (ölçeklenmez);
//! yazı boyları `fs()` ile 0,85 ölçeklidir. Hesaplanmış değerler `dom-styles.mjs --root
//! ".project-rail"` / `".channel-sidebar"` çıktısıyla doğrulandı.
//!
//! Bilinen farklar: CSS geçişleri (`transform`/`border-radius` 140–220ms) GPUI'de anlık;
//! `chevron` dönüşü animasyonsuz; proje başına okunmamış sayısı (`/projects/unread`) Rust
//! çekirdeğinde yok (rozet yalnız seçili olmayan projelerde ve veri gelince çizilir).

use gpui_kit::prelude::*;
use p2_ui::P2Fx as _; // backdrop_blur
use gpui_kit::{AnyElement, Context, Hsla, MouseButton, MouseDownEvent, Pixels, Point, SharedString, div, linear_color_stop, linear_gradient, point, px};
use p2_client_core::{Channel, ChannelType, ChatGroup, Member, Scope};
use p2_ui::{P2Motion as _, P2Text as _};
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::{Icon, icon, svg_color, svg_image};
use p2_ui::tokens::{Hue, Palette, fs};

use super::{ChatSubject, WorkspaceScreen, manage_ui};

/// `.rail-avatar.tone-N` → `--p2-hue-*` (project-rail.css).
const TONES: [usize; 6] = [0, 2, 8, 4, 3, 7];

/// Kenar çubuğu sağ tık menüleri (Workspace.tsx `projectContext` / `personMenu`).
#[derive(Clone)]
pub(super) enum NavMenu {
    Project { index: usize, at: Point<Pixels> },
    /// Kişi menüsü. `dm`: menü bir sohbet satırından açıldıysa o sohbetin kimliği ("Listeden kaldır").
    Person { person: Member, dm: Option<String>, at: Point<Pixels> },
    /// Ana sayfa grubu satırı: üyeler / yeniden adlandır, gruptan ayrıl.
    Group { group: ChatGroup, at: Point<Pixels> },
    /// Rayın "＋" düğmesi: davet koduyla katıl / yeni proje oluştur.
    Add { at: Point<Pixels> },
}

/// CSS `linear-gradient(145deg, bg, bg2)` (avatar zeminleri).
fn hue_grad(h: &Hue) -> gpui_kit::Background {
    linear_gradient(145., linear_color_stop(h.bg, 0.), linear_color_stop(h.bg2, 1.))
}

/// tr-TR küçük harf (arama).
fn tr_lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'İ' => vec!['i'],
            'I' => vec!['ı'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

/// tr-TR büyük harf (`text-transform: uppercase`, html lang="tr").
fn tr_upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

fn count_text(n: u32) -> String {
    if n > 99 { "99+".into() } else { n.to_string() }
}

/// direct.css `.presence` (+ `.online/.away/.busy`): 11×11, 2px halka (`ring`), konumu çağıranda.
pub(super) fn presence_mark(status: &str, ring: Hsla, p: &Palette) -> gpui_kit::Div {
    // Ortak çizim (p2_ui::controls::presence_ring): halka + 7px nokta kesirli koordinatlarla boyanır,
    // uzakta deliği tam ortada kalır.
    p2_ui::controls::presence_ring(p2_ui::controls::Presence::from_status(status), ring, p)
}

/// direct.css `.direct-unread`.
pub(super) fn unread_badge(n: u32, p: &Palette) -> gpui_kit::Div {
    div()
        .flex_none()
        .min_w(px(19.))
        .h(px(19.))
        .px(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.5))
        .bg(p.danger)
        .text_color(gpui_kit::white())
        .text_size(fs(10.))
        .css_weight(700.)
        .line_height(px(10.))
        .tracking(0.01)
        .shadow(vec![
            gpui_kit::BoxShadow { color: gpui_kit::black().opacity(0.4), offset: point(px(0.), px(1.)), blur_radius: px(3.), spread_radius: px(0.), inset: false },
            gpui_kit::BoxShadow { color: gpui_kit::white().opacity(0.18), offset: point(px(0.), px(1.)), blur_radius: px(0.), spread_radius: px(0.), inset: true },
        ])
        .child(count_text(n))
}

/// project-rail.css `.channel-project-header::before` + `::after` (radyal gradyanlar ve
/// eğik elips) — GPUI'de radyal gradyan olmadığından SVG olarak (`w`×100; `w` = kenar çubuğu genişliği − 1px sağ kenarlık).
/// `::after` content-box: 150×100 + 1px kenarlık = 152×102, sağa yaslı (`right: -20px`) → merkez (w−56, 91); kapak kutusuna göre.
/// Kapak zemini (degradeler) 252 px genişlikte BİR KEZ üretilir ve kenar çubuğu genişliğine gerilir:
/// Electron'daki yüzdeli degradeler de genişlikle orantılı ölçeklenir. Genişliğe göre yeniden SVG
/// üretmek sürüklerken her karede yeni bir görsel yükletiyor ve kapak titriyordu.
pub(super) const COVER_BASE_W: f32 = 252.;

/// Sağdaki elips (`::after`): sabit görsel, sağ kenara bağlı. Kutu 180×120, merkez (90, 60).
pub(super) fn cover_ellipse_svg(p: &Palette) -> String {
    let (border, ba) = svg_color(p.border);
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="120" viewBox="0 0 180 120"><ellipse cx="90" cy="60" rx="75.5" ry="50.5" transform="rotate(-12 90 60)" fill="none" stroke="{border}" stroke-opacity="{ba}" stroke-width="1"/></svg>"#)
}

pub(super) fn cover_svg(p: &Palette, w: f32) -> String {
    let (accent2, a2) = svg_color(p.accent_soft_2);
    let (bg3, bg3a) = svg_color(p.bg_3);
    let (blue, bluea) = svg_color(p.blue_900);
    let h = 100f32;
    // linear-gradient(160deg): yön (sin, -cos), çizgi uzunluğu |w sin| + |h cos|.
    let a = 160f32.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let len = (w * a.sin()).abs() + (h * a.cos()).abs();
    let (cx, cy) = (w / 2., h / 2.);
    let (x1, y1, x2, y2) = (cx - dx * len / 2., cy - dy * len / 2., cx + dx * len / 2., cy + dy * len / 2.);
    // radial-gradient(circle at X Y, …): yarıçap = en uzak köşe.
    let far = |x: f32, y: f32| [(0., 0.), (w, 0.), (0., h), (w, h)].iter().map(|(a, b): &(f32, f32)| ((a - x).powi(2) + (b - y).powi(2)).sqrt()).fold(0f32, f32::max);
    let (r1x, r1y) = (w * 0.18, h * 0.18);
    let (r2x, r2y) = (w * 0.85, h * 0.90);
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="100" viewBox="0 0 {w} 100"><defs>"#,
            r#"<linearGradient id="l" gradientUnits="userSpaceOnUse" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"><stop offset="0" stop-color="{bg3}" stop-opacity="{bg3a}"/><stop offset="1" stop-color="{blue}" stop-opacity="{bluea}"/></linearGradient>"#,
            r#"<radialGradient id="r2" gradientUnits="userSpaceOnUse" cx="{r2x}" cy="{r2y}" r="{r2}"><stop offset="0" stop-color="rgb(127,182,255)" stop-opacity="0.12"/><stop offset="0.4" stop-color="rgb(127,182,255)" stop-opacity="0"/></radialGradient>"#,
            r#"<radialGradient id="r1" gradientUnits="userSpaceOnUse" cx="{r1x}" cy="{r1y}" r="{r1}"><stop offset="0" stop-color="{accent2}" stop-opacity="{a2}"/><stop offset="0.42" stop-color="{accent2}" stop-opacity="0"/></radialGradient>"#,
            r#"</defs><rect width="{w}" height="100" fill="url(#l)"/><rect width="{w}" height="100" fill="url(#r2)"/><rect width="{w}" height="100" fill="url(#r1)"/>"#,
            r#"</svg>"#
        ),
        w = w, x1 = x1, y1 = y1, x2 = x2, y2 = y2, bg3 = bg3, bg3a = bg3a, blue = blue, bluea = bluea,
        r2x = r2x, r2y = r2y, r2 = far(r2x, r2y), r1x = r1x, r1y = r1y, r1 = far(r1x, r1y), accent2 = accent2, a2 = a2
    )
}

/// `.chevron` (6×6, sağ+alt 1,5px kenarlık, `rotate(±45deg) translate(-1px,∓1px)`), 24'lük
/// viewBox'ta 10px kutuya ölçekli (×2,4) elle hesaplanmış köşe noktaları.
const ADD_PLUS: &str = r#"<path d="M6.5 12h11M12 6.5v11"/>"#;
const CHEVRON_OPEN: &str = r#"<path d="M20.909 10.728 12 19.637 3.091 10.728"/>"#;
const CHEVRON_CLOSED: &str = r#"<path d="M10.728 3.091 19.637 12 10.728 20.909"/>"#;

fn chevron(collapsed: bool, group: SharedString, p: &Palette) -> gpui_kit::Div {
    use p2_ui::P2Fx as _;
    let (text_3, accent) = (p.text_3, p.text_accent);
    // `transition: transform var(--m-base) var(--ease-pop)`: açık (0) ↔ kapalı (1) arası döndürme.
    // Kapalı şekil = açık şeklin merkez etrafında −90° döndürülmüşü; kayma da birlikte geçer.
    let key = format!("chevron-{group}");
    let (k, running) = p2_ui::fx::tween_raw(&key, if collapsed { 1. } else { 0. }, p2_ui::motion::M_BASE, p2_ui::motion::EASE_POP);
    let icon = div()
        .absolute()
        .left(px(-2.))
        .top(px(-2. - 1.414 + 2.828 * k))
        .size(px(10.))
        .rotate(-90. * k)
        .child(Icon::raw(CHEVRON_OPEN).stroke(3.6).el().size(px(10.)).text_color(text_3).group_hover(group, move |s| s.text_color(accent)));
    div().relative().flex_none().size(px(6.)).child(p2_ui::fx::keep_animating(icon, &key, running))
}

/// `.add-channel` (24×24, iki 11×1,5 çubuktan artı). Bütün kenar çubuğu bölüm başlıklarındaki "＋" budur
/// (proje: kategori / SOHBETLER; Ana sayfa: DİREKT MESAJLAR) — ölçü, çizgi, hover zemini tek yerden.
pub(super) fn add_button(id: SharedString, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let group: SharedString = format!("{id}-g").into();
    let (text_3, accent, soft) = (p.text_3, p.text_accent, p.accent_soft);
    div()
        .id(id)
        .group(group.clone())
        .relative()
        .flex_none()
        .size(px(24.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(soft))
        // İki 11×1,5px çubuk (yarı piksele düşer; Chromium gibi yumuşatılsın diye SVG).
        .child(Icon::raw(ADD_PLUS).stroke(1.5).el().size(px(24.)).text_color(text_3).group_hover(group, move |s| s.text_color(accent)))
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

impl WorkspaceScreen {
    // ------------------------------------------------------------ ray

    /// `.rail-actions .rail-action` (42×42; ikon `currentColor`).
    fn rail_action(&self, id: &'static str, glyph: AnyElement, tip_text: String, active: bool, disabled: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
        let (soft, border, strong) = (p.accent_soft, p.border, p.text_strong);
        div()
            .id(id)
            .group(id)
            .relative()
            .my(px(2.))
            .size(px(42.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if active { 11. } else { 14. }))
            .border_1()
            .border_color(if active { p.border_accent } else { gpui_kit::transparent_black() })
            .when(active, |s| s.bg(p.accent_soft_2))
            .text_color(if active { p.text_strong } else { p.text_3 })
            .text_size(fs(21.))
            .line_height(fs(21.))
            .when(disabled, |s| s.opacity(0.35))
            .when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(soft).border_color(border).text_color(strong).top(px(-1.))))
            .child(glyph)
            .tooltip(tip(tip_text))
    }

    fn rail_icon(&self, id: &'static str, glyph: Icon, size: f32, active: bool, p: &Palette) -> AnyElement {
        let strong = p.text_strong;
        glyph.el().size(px(size)).text_color(if active { p.text_strong } else { p.text_3 }).group_hover(id, move |s| s.text_color(strong)).into_any_element()
    }

    pub(super) fn rail(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let user = self.auth.user.clone();
        let selected_name = self.project.map(|i| self.projects[i].name.clone());
        let tools_open = self.board.is_some() || self.documents.is_some() || self.library.is_some();
        let mut projects = div().id("rail-projects").flex_1().min_h_0().overflow_y_scroll().flex().flex_col();
        // Sürükleme iptal edilirse (ray dışına bırakma) durum kalır; yalnız etkin sürüklemede çizilir.
        let dragging = cx.has_active_drag();
        for (i, project) in self.projects.iter().enumerate() {
            let active = self.project == Some(i) && self.library.is_none() && self.documents.is_none();
            let hue = p.hues[TONES[i % 6]];
            let group: SharedString = format!("rail-p-{i}").into();
            let (border_strong, text_2) = (p.border_strong, p.text_2);
            // Görünen kanalların okunmamışları + bu projenin birebir sohbetlerindeki okunmamışlar (workspace.rs `project_badge`).
            let unread = self.project_badge(i);
            projects = projects.child(
                div()
                    .id(("rail-project", i))
                    .group(group.clone())
                    .relative()
                    .flex_none()
                    .w(px(72.))
                    .h(px(52.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    // Sol gösterge: seçiliyse 36px, üzerindeyken .55 ölçek (ortadan açılır).
                    .child(if active {
                        div().absolute().left_0().top(px(8.)).w(px(3.)).h(px(36.)).rounded_r(px(3.)).bg(p.accent_hover).shadow(vec![gpui_kit::BoxShadow { color: p.accent_soft_2, offset: point(px(0.), px(0.)), blur_radius: px(10.), spread_radius: px(0.), inset: false }])
                    } else {
                        div().absolute().left_0().top(px(16.1)).w(px(3.)).h(px(19.8)).rounded_r(px(3.)).bg(text_2).invisible().group_hover(group.clone(), |s| s.visible())
                    })
                    .child(
                        div()
                            .relative()
                            .size(px(42.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(if active { 11. } else { 14. }))
                            .border_1()
                            .border_color(if active { p.border_accent } else { p.border_subtle })
                            .bg(hue_grad(&hue))
                            .text_color(if active { p.text_strong } else { hue.fg })
                            .text_size(fs(11.))
                            .line_height(px(40.))
                            .css_weight(800.)
                            .tracking(-0.02)
                            .when(active, |s| {
                                let g = p.accent_glow_strong;
                                s.shadow(vec![gpui_kit::BoxShadow { color: g.color, offset: point(px(g.x), px(g.y)), blur_radius: px(g.blur), spread_radius: px(g.spread), inset: g.inset }])
                            })
                            .when(!active, |s| {
                                let sm = p.shadow_sm();
                                s.group_hover(group.clone(), move |s| s.rounded(px(11.)).border_color(border_strong).top(px(-1.)).shadow(sm))
                            })
                            .child(p2_ui::components::initials_label(user_initials_project(&project.name))),
                    )
                    .when(unread > 0, |s| {
                        s.child(
                            div()
                                .absolute()
                                .right(px(9.))
                                .top(px(1.))
                                .p(px(2.))
                                .rounded(px(8.5))
                                .bg(p.bg_1)
                                .child(
                                    div()
                                        .min_w(px(13.))
                                        .h(px(13.))
                                        .px(px(4.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.5))
                                        .bg(p.danger)
                                        .text_color(gpui_kit::white())
                                        .text_size(fs(9.))
                                        .css_weight(750.)
                                        .line_height(fs(9.))
                                        .tracking(0.01)
                                        .child(count_text(unread)),
                                ),
                        )
                    })
                    .tooltip(tip(project.name.clone()))
                    // Sürükle-bırak sırası (Workspace.tsx dragstart/dragover/drop): kaynak .36 saydam,
                    // hedefin üstünde 2px vurgu çizgisi; sıra profil klasöründe saklanır.
                    .on_drag(ProjectDrag { id: project.id.clone(), index: i }, {
                        let (hue, name, active_ui) = (hue, project.name.clone(), active);
                        move |_, _, _, cx| cx.new(|_| DragGhost { hue, label: user_initials_project(&name), active: active_ui })
                    })
                    .when(dragging && self.dragging_project.as_deref() == Some(project.id.as_str()), |s| s.opacity(0.36))
                    .on_drag_move::<ProjectDrag>(cx.listener({
                        let id = project.id.clone();
                        move |this, ev: &gpui_kit::DragMoveEvent<ProjectDrag>, _, cx| {
                            let drag = ev.drag(cx);
                            let (source, inside) = (drag.id.clone(), ev.bounds.contains(&ev.event.position));
                            if this.dragging_project.as_deref() != Some(source.as_str()) {
                                this.dragging_project = Some(source.clone());
                                cx.notify();
                            }
                            let target = (inside && source != id).then(|| id.clone());
                            if inside && this.drag_target != target {
                                this.drag_target = target;
                                cx.notify();
                            }
                        }
                    }))
                    .when(dragging && self.drag_target.as_deref() == Some(project.id.as_str()), |s| {
                        s.child(div().absolute().left(px(12.)).right(px(12.)).top(px(-1.)).h(px(2.)).rounded(px(2.)).bg(p.accent).shadow(vec![gpui_kit::BoxShadow { color: p.accent_soft_2, offset: point(px(0.), px(0.)), blur_radius: px(8.), spread_radius: px(0.), inset: false }]))
                    })
                    .on_drop(cx.listener({
                        let id = project.id.clone();
                        move |this, drag: &ProjectDrag, _, cx| this.drop_project(&drag.id, &id, cx)
                    }))
                    // Raydaki projeye tıklamak her zaman o projenin sohbetine götürür (pano kapanır).
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.board = None;
                        this.documents = None;
                        this.library = None;
                        this.select_project(i, window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                        this.open_project_nav_menu(i, ev.position, window, cx);
                    })),
            );
        }

        let no_project = self.project.is_none();
        let badge = |n: u32, bg: Hsla, max: u32, p: &Palette| {
            div()
                .absolute()
                .right(px(2.))
                .top(px(2.))
                .p(px(2.))
                .rounded(px(8.))
                .bg(p.bg_1)
                .child(
                    div()
                        .min_w(px(12.))
                        .h(px(12.))
                        .px(px(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .bg(bg)
                        .text_color(gpui_kit::white())
                        .text_size(fs(8.5))
                        .css_weight(750.)
                        .line_height(fs(8.5))
                        .tracking(0.01)
                        .child(if n > max { format!("{max}+") } else { n.to_string() }),
                )
        };
        let library_badge = self.library_active_jobs();
        // Workspace.tsx: `activity.filter(item=>!item.readAt).length` (yüklü öğelerden; sunucu toplamı değil).
        let activity_unread = self.activity.iter().filter(|a| a.read_at.is_none()).count() as u32;
        let me_hue = p.hues[avatar_hue(&user.id)];
        let status = if self.preference == "INVISIBLE" { "INVISIBLE".to_string() } else { self.my_status.clone() };
        let actions = div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .child(
                self.rail_action("rail-activity", self.rail_icon("rail-activity", icon("bell"), 19., self.activity_open, p), "Etkinlik".into(), self.activity_open, false, p)
                    .when(activity_unread > 0, |s| s.child(badge(activity_unread, p.danger, 99, p)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.activity_open = !this.activity_open;
                        cx.notify();
                    })),
            )
            .child(
                self.rail_action("rail-board", self.rail_icon("rail-board", icon("board").stroke(1.9), 17., self.board.is_some(), p), selected_name.as_ref().map_or("Görevler (pano)".into(), |n| format!("{n} görev panosu")), self.board.is_some(), no_project, p)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.documents = None;
                        this.library = None;
                        this.open_board(window, cx);
                    })),
            )
            .child(
                self.rail_action("rail-docs", self.rail_icon("rail-docs", Icon::raw(p2_ui::icons::rail::DOCUMENTS), 18., self.documents.is_some(), p), selected_name.as_ref().map_or("Dokümanlar".into(), |n| format!("{n} dokümanları")), self.documents.is_some(), no_project, p)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.board = None;
                        this.library = None;
                        this.open_documents(window, cx);
                    })),
            )
            .child(
                self.rail_action("rail-library", self.rail_icon("rail-library", Icon::raw(p2_ui::icons::rail::LIBRARY), 19., self.library.is_some(), p), selected_name.as_ref().map_or("Kütüphane".into(), |n| format!("{n} kütüphanesi")), self.library.is_some(), no_project, p)
                    .when(library_badge > 0, |s| s.child(badge(library_badge, p.accent, 9, p).right(px(4.)).top(px(4.))))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.board = None;
                        this.documents = None;
                        this.open_library(window, cx);
                    })),
            )
            // Herkes proje açar ya da davet koduyla katılır (proje toplulukları).
            .child({
                let open = matches!(self.nav_menu, Some(NavMenu::Add { .. }));
                self.rail_action("rail-new-project", "＋".into_any_element(), p2_i18n::t(if self.auth.user.can_server("project.create") { "rail.joinOrCreate" } else { "rail.join" }), open, false, p).on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.nav_menu = if matches!(this.nav_menu, Some(NavMenu::Add { .. })) { None } else { Some(NavMenu::Add { at: point(px(66.), ev.position.y - px(20.)) }) };
                    cx.notify();
                }))
            })
            // Keşfet (pusula): herkese açık projeler ve haberleri.
            .child({
                let open = self.discover.is_some();
                self.rail_action("rail-discover", self.rail_icon("rail-discover", Icon::raw(crate::screens::discover_ui::glyph::COMPASS), 20., open, p), p2_i18n::t("disc.rail.tip"), open, false, p).on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_discover(window, cx);
                }))
            })
            // Sunucu yönetimi kalkanı: kurulum sahibi ya da herhangi bir `server.*` yetkisi olan.
            .when(user.server_panel(), |s| {
                s.child(self.rail_action("rail-server", self.rail_icon("rail-server", Icon::raw(crate::screens::server_admin::SHIELD_ICON), 19., false, p), p2_i18n::t("srv.title"), false, false, p).on_click(cx.listener(|this, _, window, cx| this.open_instance_admin(window, cx))))
            })
            .child(div().w(px(32.)).h(px(1.)).my(px(6.)).bg(p.border_subtle))
            .child({
                let (border_strong, soft2, strong_me) = (p.border_strong, p.accent_soft_2, p.text_strong);
                div()
                    .id("me")
                    .relative()
                    .my(px(2.))
                    .size(px(42.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(if self.user_menu { 11. } else { 14. }))
                    .border_1()
                    .border_color(p.border)
                    .bg(hue_grad(&me_hue))
                    .text_color(me_hue.fg)
                    .text_size(fs(12.))
                    .line_height(px(40.))
                    .css_weight(800.)
                    .tracking(-0.02)
                    .cursor_pointer()
                    .when(self.user_menu, |s| s.shadow(vec![gpui_kit::BoxShadow { color: soft2, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }]))
                    // D: `.rail-actions .rail-action.active` zemini (accent-soft-2) ve yazı rengi profil kartı açıkken.
                    .when(self.user_menu, |s| s.bg(soft2).text_color(strong_me))
                    .hover(move |s| s.border_color(border_strong).top(px(-1.)))
                    .child(face(&user.id, &user.display_name, 13.))
                    .child(presence_mark(&status, p.bg_1, p).absolute().right(px(1.)).bottom(px(1.)))
                    .tooltip(tip(format!("{} · @{}", user.display_name, user.username)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.user_menu = !this.user_menu;
                        cx.notify();
                    }))
                    // Profil kartı (D): çalışma alanı kökünde `people_ui::user_menu_layer` (sol 78, alt 12).
            });
        let _ = tools_open;
        div()
            .w(px(72.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .py(px(10.))
            .bg(p.bg_1)
            .border_r_1()
            .border_color(p.border_subtle)
            // Ana sayfa düğmesi + okunmamış DM baloncukları (friends_ui.rs), projelerin üstünde.
            .child(self.rail_home(p, cx))
            .child(projects)
            .child(actions)
    }

    /// Raydaki projeye sağ tık: menü açılır; o projedeki yetkilerim sunucudan yeniden alınır (bilinmiyor ya
    /// da eskimiş olabilir) ve değiştiyse menü güncellenir.
    pub(super) fn open_project_nav_menu(&mut self, index: usize, at: Point<Pixels>, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get(index) else { return };
        let id = project.id.clone();
        crate::trace(&format!("ray: proje menüsü «{}» [{}]", project.name, self.project_menu_items(index).join(", ")));
        self.channel_menu = None;
        self.nav_menu = Some(NavMenu::Project { index, at });
        cx.notify();
        let api = self.api.clone();
        crate::task::run(window, cx, move || (id.clone(), api.project_permissions(&id)), move |this, (id, r), _, cx| {
            if let Ok(perms) = r {
                if this.project_perms.get(&id) != Some(&perms) {
                    this.project_perms.insert(id, perms);
                    if let Some(name) = this.projects.get(index).map(|p| p.name.clone()) {
                        crate::trace(&format!("ray: proje menüsü «{name}» [{}] (yetkiler güncellendi)", this.project_menu_items(index).join(", ")));
                    }
                    cx.notify();
                }
            }
        });
    }

    /// Kütüphane rozetinin sayısı (etkin indirme/kurulum işleri).
    fn library_active_jobs(&self) -> u32 {
        0
    }

    // ------------------------------------------------------------ kenar çubuğu

    /// Kenar çubuğu bölüm başlığı (ok + 9,5px/800 büyük harf başlık + isteğe bağlı rozet ve "＋").
    /// Proje kenar çubuğu ve Ana sayfa aynı başlığı kullanır.
    pub(super) fn category_header(&self, id: SharedString, title: String, collapsed: bool, add: Option<gpui_kit::Stateful<gpui_kit::Div>>, extra: Option<AnyElement>, toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static, p: &Palette, cx: &mut Context<Self>) -> gpui_kit::Div {
        let group: SharedString = format!("{id}-t").into();
        let accent = p.text_accent;
        div()
            .h(px(24.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .id(id)
                    .group(group.clone())
                    .flex_1()
                    .min_w_0()
                    .h(px(24.))
                    .px(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .cursor_pointer()
                    .text_color(p.text_3)
                    .text_size(fs(9.5))
                    .css_weight(800.)
                    .tracking(0.14)
                    .hover(move |s| s.text_color(accent))
                    .child(chevron(collapsed, group, p))
                    .child(div().truncate().child(tr_upper(&title)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        toggle(this, cx);
                        cx.notify();
                    })),
            )
            .children(extra)
            .children(add)
    }

    fn channel_row(&self, channel: &Channel, selected: bool, starred_list: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let group: SharedString = format!("chrow-{}-{}", channel.id, starred_list).into();
        let type_color = if selected { p.text_accent } else { p.text_3 };
        let glyph: AnyElement = match channel.kind {
            ChannelType::Text => div().text_size(fs(17.)).css_weight(500.).text_color(type_color).child("#").into_any_element(),
            ChannelType::Announcement => icon("channel:announcement").el().size(px(16.)).text_color(type_color).into_any_element(),
            ChannelType::Voice => icon("channel:voice").el().size(px(16.)).text_color(type_color).into_any_element(),
            ChannelType::Forum => icon("channel:forum").el().size(px(16.)).text_color(type_color).into_any_element(),
        };
        let is_home = self.project.and_then(|i| self.projects[i].default_channel_id.clone()).as_deref() == Some(channel.id.as_str());
        let voice_count = if channel.kind == ChannelType::Voice { self.voice_count(&channel.id, cx) } else { 0 };
        let unread = if selected { 0 } else { channel.unread };
        let c = channel.clone();
        let name_color = if starred_list && !selected { p.text } else if selected { p.text_strong } else { p.text_2 };
        div()
            .id(SharedString::from(format!("ch-{}-{}", channel.id, starred_list)))
            .group(group.clone())
            .relative()
            .w_full()
            .h(px(29.))
            .flex()
            .items_center()
            .px(px(8.))
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(name_color)
            .when(selected, |s| s.bg(p.bg_selected))
            .hover(move |s| s.bg(hover_bg).text_color(strong).left(px(2.)))
            .when(selected, |s| {
                s.child(div().absolute().left(px(-8.)).top(px(4.5)).w(px(2.)).h(px(18.)).rounded_r(px(2.)).bg(p.accent_hover).shadow(vec![gpui_kit::BoxShadow { color: p.accent_soft_2, offset: point(px(0.), px(0.)), blur_radius: px(8.), spread_radius: px(0.), inset: false }]))
            })
            .child(div().w(px(27.)).flex_none().flex().items_center().child(glyph))
            .child(div().flex_1().min_w_0().truncate().text_size(fs(13.)).css_weight(550.).child(channel.name.clone()))
            .when(!starred_list, |s| {
                s.child(
                    div()
                        .flex_none()
                        .ml_auto()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .when(is_home, |s| s.child(div().id("channel-home").text_size(fs(12.)).line_height(fs(12.)).opacity(0.55).child("⌂").tooltip(tip("Başlangıç kanalı: proje açılınca ilk bu kanal gelir"))))
                        .when(channel.kind == ChannelType::Voice, |s| {
                            if voice_count > 0 {
                                s.child(div().min_w(px(18.)).h(px(18.)).px(px(5.)).flex().items_center().justify_center().rounded(px(9.)).bg(p.success).text_color(p.text_on_success).text_size(fs(9.)).css_weight(750.).child(voice_count.to_string()))
                            } else {
                                s.child(div().px(px(5.)).py(px(2.)).rounded(px(4.)).bg(p.accent_soft).text_color(p.text_accent).text_size(fs(9.)).css_weight(700.).child("SES"))
                            }
                        })
                        .when(unread > 0, |s| s.child(unread_badge(unread, p))),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| this.select_channel(c.clone(), window, cx)))
            .when(!starred_list, |s| {
                let c = channel.clone();
                s.on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.nav_menu = None;
                    this.channel_menu = Some(manage_ui::ChannelMenu::channel(c.clone(), ev.position));
                    cx.notify();
                }))
            })
            .into_any_element()
    }

    /// Workspace.tsx `VoiceMembers` + project-rail.css `.voice-member*`: sesli odadaki kişiler.
    /// Ses düzeyi (`--voice-level`) Rust'ta yok; konuşanlar düzey ≈ .6 ile çizilir.
    fn voice_members(&self, channel_id: &str, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let calls = self.calls.read(cx);
        let list = calls.voice_rooms.get(channel_id).filter(|l| !l.is_empty())?.clone();
        let speaking = calls.speaking.clone();
        let me = self.auth.user.id.clone();
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let mut col = div().mt(px(2.)).mb(px(7.)).pl(px(7.)).flex().flex_col().gap(px(2.));
        for (i, part) in list.iter().enumerate() {
            let level: f32 = if !part.muted && speaking.contains(&part.user_id) { 0.6 } else { 0. };
            let hue = p.hues[avatar_hue(&part.user_id)];
            let ring = gpui_kit::Rgba { r: 79. / 255., g: 157. / 255., b: 1., a: if level > 0.08 { 0.5 + level * 0.5 } else { 0.15 + level * 0.85 } };
            let glow = gpui_kit::Rgba { a: level * 0.65, ..ring };
            let person = self.members.iter().find(|m| m.id == part.user_id).cloned();
            let group: SharedString = format!("vm-{channel_id}-{i}").into();
            col = col.child(
                div()
                    .id(SharedString::from(format!("voice-member-{channel_id}-{}", part.user_id)))
                    .group(group.clone())
                    .w_full()
                    .min_w_0()
                    .h(px(36.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_color(p.text_2)
                    .hover(move |s| s.bg(hover_bg).text_color(strong))
                    .child(
                        div().w(px(28.)).flex_none().child(
                            // 2px halka: dış daire halka rengi, iç kare avatar (GPUI kenarlık halesi yok).
                            div()
                                .size(px(25.))
                                .p(px(2.))
                                .rounded(px(9.))
                                .bg(gpui_kit::Hsla::from(ring))
                                .shadow(vec![gpui_kit::BoxShadow { color: glow.into(), offset: point(px(0.), px(0.)), blur_radius: px(2. + level * 10.), spread_radius: px(0.), inset: false }])
                                .child(
                                    div()
                                        .size_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(7.))
                                        .bg(hue_grad(&hue))
                                        .text_color(hue.fg)
                                        .text_size(fs(9.))
                                        .line_height(px(21.))
                                        .css_weight(800.)
                                        .child(face(&part.user_id, &part.display_name, 7.)),
                                ),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_size(fs(13.))
                            .css_weight(550.)
                            .child(div().truncate().child(part.display_name.clone()))
                            .when(part.user_id == me, |s| s.child(div().flex_none().ml(px(6.)).text_color(p.text_accent).text_size(fs(8.)).css_weight(750.).child("SEN")))
                            .when(part.screen, |s| {
                                s.child(div().flex_none().ml(px(6.)).px(px(5.)).py(px(1.)).rounded(px(5.)).bg(p.danger).text_color(gpui_kit::white()).text_size(fs(8.)).css_weight(800.).tracking(0.04).child("YAYINDA"))
                            }),
                    )
                    .when(part.muted || part.deafened, |s| {
                        s.child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(3.))
                                .text_color(p.danger_text)
                                .child(icon("call:micOff").stroke(0.).filled().el().size(px(15.)).text_color(p.danger_text))
                                .when(part.deafened, |s| s.child(icon("call:headphonesOff").stroke(0.).filled().el().size(px(15.)).text_color(p.danger_text))),
                        )
                    })
                    .tooltip(tip(if person.is_some() { format!("{} profilini aç", part.display_name) } else { part.display_name.clone() }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = person.clone() {
                            this.profile = Some(m);
                            cx.notify();
                        }
                    })),
            );
        }
        Some(col.into_any_element())
    }

    fn voice_count(&self, channel_id: &str, cx: &Context<Self>) -> usize {
        self.calls.read(cx).voice_rooms.get(channel_id).map_or(0, |v| v.len())
    }

    fn person_avatar(member: &Member, p: &Palette) -> gpui_kit::Div {
        let hue = p.hues[avatar_hue(&member.id)];
        div()
            .relative()
            .flex_none()
            .size(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.))
            .bg(hue_grad(&hue))
            .text_color(hue.fg)
            .text_size(fs(9.))
            .line_height(px(26.))
            .css_weight(800.)
            .tracking(-0.02)
            .child(face(&member.id, &member.display_name, 9.))
            .child(presence_mark(&member.status, p.bg_2, p).absolute().right(px(-3.)).bottom(px(-3.)))
    }

    /// Kişi satırı. `dm`: satır bir proje sohbetini gösteriyorsa (SOHBETLER bölümü) sohbet kimliği —
    /// sağ tık menüsüne "Listeden kaldır" eklenir.
    fn person_row(&self, member: &Member, unread: u32, selected: bool, starred_list: bool, dm: Option<&str>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let kind = if dm.is_some() { "dm" } else if starred_list { "star" } else { "person" };
        let group: SharedString = format!("prow-{}-{kind}", member.id).into();
        let person = member.clone();
        let menu_person = member.clone();
        let menu_dm = dm.map(str::to_string);
        let base = if selected || unread > 0 { p.text_strong } else if starred_list { p.text } else { p.text_2 };
        div()
            .id(SharedString::from(if dm.is_some() { format!("pdm-{}", member.id) } else { format!("person-{}-{}", member.id, starred_list) }))
            .group(group.clone())
            .relative()
            .w_full()
            .px(px(8.))
            .py(px(3.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(selected, |s| s.bg(p.bg_selected))
            .hover(move |s| s.bg(hover_bg).left(px(2.)))
            .child(Self::person_avatar(member, p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().text_size(fs(12.)).css_weight(if unread > 0 { 700. } else { 600. }).text_color(base).group_hover(group, move |s| s.text_color(strong)).child(member.display_name.clone())),
            )
            .when(unread > 0, |s| s.child(unread_badge(unread, p)))
            .tooltip(tip(format!("@{}", member.username)))
            .on_click(cx.listener(move |this, _, window, cx| this.open_direct(person.clone(), window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.channel_menu = None;
                this.nav_menu = Some(NavMenu::Person { person: menu_person.clone(), dm: menu_dm.clone(), at: ev.position });
                cx.notify();
            }))
            .into_any_element()
    }

    fn group_row(&self, group: &ChatGroup, selected: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hover_bg, strong, danger_soft, danger_text) = (p.bg_hover, p.text_strong, p.danger_soft, p.danger_text);
        let gname: SharedString = format!("grow-{}", group.id).into();
        let hue = p.hues[8];
        let unread = if selected { 0 } else { group.unread };
        let g = group.clone();
        let gid = group.id.clone();
        let menu_group = group.clone();
        // Üye avatarları (Ana sayfa gruplarındaki gibi üst üste): benden başka ilk iki üye; tek kişi
        // kaldıysa grubun baş harfleri.
        let me = self.auth.user.id.as_str();
        let others: Vec<&Member> = group.members.iter().filter(|m| m.id != me).take(2).collect();
        let ring = if selected { p.bg_selected } else { p.bg_2 };
        let mini = |m: &Member| {
            let h = p.hues[avatar_hue(&m.id)];
            div().size(px(16.)).flex_none().flex().items_center().justify_center().rounded(px(6.)).bg(hue_grad(&h)).text_color(h.fg).text_size(fs(6.5)).line_height(px(16.)).css_weight(800.).child(face(&m.id, &m.display_name, 6.))
        };
        let faces: AnyElement = if others.len() >= 2 {
            div()
                .relative()
                .size(px(24.))
                .child(mini(others[0]).absolute().left_0().top_0())
                .child(div().absolute().right(px(-1.)).bottom(px(-1.)).p(px(1.5)).rounded(px(7.5)).bg(ring).child(mini(others[1])))
                .into_any_element()
        } else {
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(hue_grad(&hue))
                .text_color(hue.fg)
                .text_size(fs(8.5))
                .line_height(px(22.))
                .css_weight(800.)
                .child(p2_ui::components::initials_label(user_initials(&group.name)))
                .into_any_element()
        };
        div()
            .id(SharedString::from(format!("group-{}", group.id)))
            .group(gname.clone())
            .relative()
            .w_full()
            .pl(px(8.))
            .pr(px(26.))
            .py(px(3.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(selected, |s| s.bg(p.bg_selected))
            .hover(move |s| s.bg(hover_bg).left(px(2.)))
            .child(div().w(px(26.)).h(px(24.)).flex_none().flex().items_center().child(faces))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(fs(12.))
                    .css_weight(if unread > 0 { 700. } else { 600. })
                    .text_color(if selected || unread > 0 { p.text_strong } else { p.text_2 })
                    .group_hover(gname.clone(), move |s| s.text_color(strong))
                    .child(group.name.clone()),
            )
            .when(unread > 0, |s| s.child(unread_badge(unread, p)))
            .child(
                div()
                    .id(SharedString::from(format!("group-leave-{}", group.id)))
                    .absolute()
                    .right(px(4.))
                    .top(px(5.))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_color(p.text_3)
                    .text_size(fs(14.))
                    .line_height(fs(14.))
                    .invisible()
                    .group_hover(gname, |s| s.visible())
                    .hover(move |s| s.bg(danger_soft).text_color(danger_text))
                    .child("×")
                    .tooltip(tip("Gruptan ayrıl"))
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        cx.stop_propagation();
                        // D: Electron önce onay sorar (ConfirmDialog "Gruptan ayrıl").
                        this.ask_leave_group(gid.clone(), cx);
                    })),
            )
            .tooltip(tip(group.members.iter().map(|m| m.display_name.clone()).collect::<Vec<_>>().join(", ")))
            .on_click(cx.listener(move |this, _, window, cx| this.select_group(g.clone(), window, cx)))
            // Sağ tık (Ana sayfa gruplarıyla aynı menü): sohbeti aç / üyeler ve ad / gruptan ayrıl.
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.channel_menu = None;
                this.nav_menu = Some(NavMenu::Group { group: menu_group.clone(), at: ev.position });
                cx.notify();
            }))
            .into_any_element()
    }

    fn people_empty(text: &'static str, p: &Palette) -> gpui_kit::Div {
        div().mx(px(8.)).text_size(fs(10.)).text_color(p.text_4).child(text)
    }

    pub(super) fn sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let project_name = self.project.map(|i| self.projects[i].name.clone()).unwrap_or_default();
        let current = self.current_scope(cx);
        let query = tr_lower(self.side_search.read(cx).value().trim());
        let search_empty = self.side_search.read(cx).value().is_empty();
        let me = self.auth.user.id.clone();
        let people: Vec<&Member> = self.members.iter().filter(|m| m.id != me).collect();
        let open_dm_user = match &current {
            Some(Scope::Direct(_)) => self.chat.as_ref().and_then(|c| match &c.read(cx).subject {
                ChatSubject::Direct(m) => Some(m.id.clone()),
                _ => None,
            }),
            _ => None,
        };
        let can_manage = self.my_perms.can("channel.manage");

        // Chromium kaydırma çubuğu 10px yer ayırır (GPUI ayırmaz): liste taşıyorsa sağ dolgu +10,
        // satır rozetleri (kişi sayısı, okunmamış) çubuğa yapışmasın.
        let channel_scroll = CHANNEL_SCROLL.with(Clone::clone);
        let overflows = channel_scroll.max_offset().y > px(0.);
        let mut list = div().id("channel-scroll").track_scroll(&channel_scroll).flex_1().min_h_0().overflow_y_scroll().pt(px(2.)).px(px(8.)).when(overflows, |s| s.pr(px(18.))).pb(px(14.)).flex().flex_col();
        let mut first = true;
        let mut section = |s: gpui_kit::Div, mt: f32| {
            let top = if first { 2. } else { mt };
            first = false;
            s.mt(px(top))
        };

        // Yıldızlılar (Workspace.tsx `starredChannels` / `starredPeople`).
        let starred_channels: Vec<Channel> = self.categories.iter().flat_map(|c| c.channels.iter()).filter(|c| self.starred.contains(&format!("CHANNEL:{}", c.id)) && !self.hidden.contains(&c.id)).cloned().collect();
        // Kişi yıldızları kapsamlıdır: burada yalnız bu projede yıldızlananlar (Ana sayfadakiler değil).
        let starred_people: Vec<&Member> = people.iter().copied().filter(|m| self.person_starred(&m.id)).collect();
        // Bu projenin birebir sohbetleri (listeden kaldırılmamış, engellenmemiş), en yeni önce.
        let project_id = self.project_id();
        let mut project_dms: Vec<&p2_client_core::Conversation> = self
            .conversations
            .iter()
            .filter(|c| project_id.is_some() && c.project_id == project_id && c.listed() &&!c.blocked_by_me && !self.home.data.is_blocked(&c.user.id))
            .collect();
        project_dms.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let dm_unread_of = |user: &str| project_dms.iter().find(|c| c.user.id == user).map_or(0, |c| c.unread);
        if !starred_channels.is_empty() || !starred_people.is_empty() {
            let collapsed = self.collapsed.contains("@starred");
            let mut s = div().flex().flex_col().child(self.category_header("cat-starred".into(), "YILDIZLI".into(), collapsed, None, None, |this, _| toggle_set(&mut this.collapsed, "@starred"), p, cx));
            if !collapsed {
                let mut body = div().flex().flex_col();
                for channel in &starred_channels {
                    let selected = self.stage.is_none() && current == Some(Scope::Channel(channel.id.clone()));
                    body = body.child(self.channel_row(channel, selected, true, p, cx));
                }
                for person in &starred_people {
                    let selected = open_dm_user.as_deref() == Some(person.id.as_str());
                    body = body.child(self.person_row(person, if selected { 0 } else { dm_unread_of(&person.id) }, selected, true, None, p, cx));
                }
                s = s.child(body.p2_enter("starred-body", p2_ui::Entrance::Rise, std::time::Duration::ZERO));
            }
            list = list.child(section(s, 4.));
        }

        for category in &self.categories {
            let channels: Vec<&Channel> = category.channels.iter().filter(|c| !self.hidden.contains(&c.id) && (query.is_empty() || tr_lower(&c.name).contains(&query))).collect();
            if !query.is_empty() && channels.is_empty() {
                continue;
            }
            let collapsed = self.collapsed.contains(&category.id);
            let cid = category.id.clone();
            let add = can_manage.then(|| {
                let cat = category.clone();
                add_button(format!("add-{}", category.id).into(), p).tooltip(tip("Kanal ekle")).on_click(cx.listener(move |this, _, window, cx| this.open_dialog(manage_ui::Dialog::NewChannel(cat.clone()), window, cx)))
            });
            let mut s = div().flex().flex_col().child(self.category_header(format!("cat-{}", category.id).into(), category.name.clone(), collapsed, add, None, move |this, _| toggle_set(&mut this.collapsed, &cid), p, cx));
            // F: boş kategoriye sağ tık → "Sil" (Workspace.tsx `onContextMenu`, yalnız kanalsız kategori).
            if category.channels.is_empty() {
                let cat = category.clone();
                s = s.on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.nav_menu = None;
                    this.channel_menu = Some(manage_ui::ChannelMenu::category(cat.clone(), ev.position));
                    cx.notify();
                }));
            }
            if !collapsed {
                let mut body = div().flex().flex_col();
                for channel in channels {
                    // I ajanı: sesli oda da sohbet olarak açılıyor (call_ui::voice_main) — seçim aynı kapsamla.
                    let selected = self.stage.is_none() && current == Some(Scope::Channel(channel.id.clone()));
                    body = body.child(self.channel_row(channel, selected, false, p, cx));
                    if channel.kind == ChannelType::Voice {
                        if let Some(people) = self.voice_members(&channel.id, p, cx) {
                            body = body.child(people);
                        }
                    }
                }
                s = s.child(body.p2_enter(SharedString::from(format!("cat-body-{}", category.id)), p2_ui::Entrance::Rise, std::time::Duration::ZERO));
            }
            list = list.child(section(s, 4.));
        }
        if self.categories.is_empty() && !self.loading && self.project.is_some() {
            let (grad, glow) = (p2_ui::components::accent_gradient_for(p), p.accent_glow_strong);
            list = list.child(
                div()
                    .my(px(8.))
                    .pt(px(24.))
                    .px(px(14.))
                    .pb(px(20.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .border_1()
                    .border_dashed()
                    .border_color(p.border)
                    .rounded(px(14.))
                    .child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child("İlk kategorini oluştur"))
                    .child(div().mt(px(6.)).mb(px(14.)).text_size(fs(11.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).child("Kanallarını düzenlemek için bir kategoriyle başla."))
                    .child(
                        div()
                            .id("empty-category-create")
                            .h(px(34.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .bg(grad)
                            .text_color(p.text_on_accent)
                            .text_size(fs(11.))
                            .css_weight(700.)
                            .cursor_pointer()
                            .shadow(vec![gpui_kit::BoxShadow { color: glow.color, offset: point(px(glow.x), px(glow.y)), blur_radius: px(glow.blur), spread_radius: px(glow.spread), inset: false }])
                            .child("Kategori oluştur")
                            .on_click(cx.listener(|this, _, window, cx| this.open_dialog(manage_ui::Dialog::NewCategory, window, cx))),
                    ),
            );
        }

        // Sohbetler: bu projenin birebir sohbetleri VE proje grupları tek listede (Ana sayfadaki "Direkt
        // mesajlar" gibi), son etkinliğe göre sıralı, okunmamış rozetleriyle. Ana sayfa sohbetlerinden ayrıdır.
        // Sağ tık: sohbette "Listeden kaldır", grupta sohbeti aç / üyeler ve ad / gruptan ayrıl.
        // Bölüm sohbet yokken de görünür: başlıktaki tek ＋ "Yeni mesaj" penceresini bu projeye bağlı açar
        // (tek kişi → proje sohbeti, birkaç kişi → proje grubu). ＋ yalnız `direct.send` ya da
        // `group.create` yetkisiyle görünür.
        let can_start = self.can_start_project_chat();
        if self.project.is_some() {
            enum Row<'a> {
                Dm(&'a p2_client_core::Conversation),
                Group(&'a ChatGroup),
            }
            let mut rows: Vec<(String, Row)> = project_dms
                .iter()
                .copied()
                .filter(|c| query.is_empty() || tr_lower(&c.user.display_name).contains(&query) || tr_lower(&c.user.username).contains(&query))
                .map(|c| (super::friends_ui::dm_stamp(c).to_string(), Row::Dm(c)))
                .collect();
            for g in self.groups.iter().filter(|g| query.is_empty() || tr_lower(&g.name).contains(&query)) {
                rows.push((g.last_message.as_ref().map(|m| m.created_at.clone()).unwrap_or_default(), Row::Group(g)));
            }
            rows.sort_by(|a, b| b.0.cmp(&a.0));
            if !rows.is_empty() || query.is_empty() {
                let collapsed = self.collapsed.contains("@dms");
                let hidden_unread: u32 = rows.iter().map(|(_, r)| match r { Row::Dm(c) => c.unread, Row::Group(g) => g.unread }).sum();
                let extra = (collapsed && hidden_unread > 0).then(|| unread_badge(hidden_unread, p).mr(px(6.)).into_any_element());
                let add = can_start.then(|| add_button("add-dm".into(), p).tooltip(tip(p2_i18n::t("dm.new.project.tip"))).on_click(cx.listener(|this, _, window, cx| this.open_project_message(window, cx))));
                let mut s = div().flex().flex_col().child(self.category_header("cat-dms".into(), format!("{} — {}", p2_i18n::t("dm.project.section"), rows.len()), collapsed, add, extra, |this, _| toggle_set(&mut this.collapsed, "@dms"), p, cx));
                if !collapsed {
                    let mut body = div().flex().flex_col();
                    if rows.is_empty() {
                        body = body.child(div().mx(px(8.)).text_size(fs(10.)).text_color(p.text_4).child(p2_i18n::t(if can_start { "dm.project.empty" } else { "dm.project.emptyNoPerm" })));
                    }
                    for (_, row) in &rows {
                        match row {
                            Row::Dm(c) => {
                                // Kişi listesindeki durum daha güncel (gerçek zamanlı).
                                let member = people.iter().copied().find(|m| m.id == c.user.id).cloned().unwrap_or_else(|| c.user.clone());
                                let selected = current == Some(Scope::Direct(c.id.clone()));
                                body = body.child(self.person_row(&member, if selected { 0 } else { c.unread }, selected, false, Some(&c.id), p, cx));
                            }
                            Row::Group(g) => body = body.child(self.group_row(g, current == Some(Scope::Group(g.id.clone())), p, cx)),
                        }
                    }
                    s = s.child(body.p2_enter("dms-body", p2_ui::Entrance::Rise, std::time::Duration::ZERO));
                }
                list = list.child(section(s, 10.));
            }
        }

        // Kişiler (projedeki herkes, ben hariç; arama kişi adı/kullanıcı adıyla süzer).
        if self.project.is_some() {
            let collapsed = self.collapsed.contains("@people");
            let visible: Vec<&Member> = people.iter().copied().filter(|m| query.is_empty() || tr_lower(&m.display_name).contains(&query) || tr_lower(&m.username).contains(&query)).collect();
            let extra: Option<AnyElement> = None;
            let mut s = div().flex().flex_col().child(self.category_header("cat-people".into(), format!("KİŞİLER — {}", people.len()), collapsed, None, extra, |this, _| toggle_set(&mut this.collapsed, "@people"), p, cx));
            if !collapsed {
                let mut body = div().flex().flex_col();
                for member in &visible {
                    let selected = open_dm_user.as_deref() == Some(member.id.as_str());
                    // Tıklamak bu projenin sohbetini açar; okunmamış rozeti "Sohbetler" bölümündeki satırdadır.
                    body = body.child(self.person_row(member, 0, selected, false, None, p, cx));
                }
                if people.is_empty() {
                    body = body.child(Self::people_empty("Kuruluşta başka kimse yok.", p));
                } else if visible.is_empty() {
                    body = body.child(Self::people_empty("Eşleşen kişi yok.", p));
                }
                s = s.child(body.p2_enter("people-body", p2_ui::Entrance::Rise, std::time::Duration::ZERO));
            }
            list = list.child(section(s, 10.));
        }

        let (soft, border, strong) = (p.accent_soft, p.border, p.text_strong);
        // Kapak genişliği = canlı kenar çubuğu genişliği − 1px sağ kenarlık (SVG önbelleği için tam piksele yukarı yuvarlı; taşan kısım kırpılır).
        let side_w = crate::resize::value(&crate::resize::SIDEBAR);
        let cover_w = (side_w - 1.).ceil();
        let header = div()
            .relative()
            .flex_none()
            .h(px(150.))
            .pt(px(12.))
            .pr(px(12.))
            .pl(px(16.))
            .flex()
            .items_start()
            .justify_between()
            .overflow_hidden()
            .border_b_1()
            .border_color(p.border_subtle)
            // `::before { left: 0; right: 0; bottom: 0; height: 100px }`: kapak her genişlikte tüm kenar çubuğunu kaplar.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(49.))
                    .w(px(cover_w))
                    .h(px(100.))
                    .overflow_hidden()
                    // object-fit: fill — zemin genişliğe gerilir (oran korunmaz).
                    .child(svg_image(cover_svg(p, COVER_BASE_W)).object_fit(gpui_kit::ObjectFit::Fill).absolute().left_0().top_0().w(px(cover_w)).h(px(100.)))
                    // `::after` sağdan −20px taşar: elips merkezi x = w − 56, y = 91.
                    .child(svg_image(cover_ellipse_svg(p)).absolute().left(px(cover_w - 56. - 90.)).top(px(91. - 60.)).w(px(180.)).h(px(120.))),
            )
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().text_size(fs(15.)).css_weight(700.).text_color(p.text_strong).child(project_name))
                    .child(div().mt(px(4.)).text_size(fs(9.5)).css_weight(800.).tracking(0.18).text_color(p.text_accent).child("PROJE KANALLARI")),
            )
            .child(
                div()
                    .id("project-menu-button")
                    .relative()
                    .flex_none()
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .text_color(p.text_3)
                    .text_size(fs(16.))
                    .tracking_px(1., fs(16.))
                    .cursor_pointer()
                    .when(self.project_menu, |s| s.bg(soft).border_color(border).text_color(strong))
                    .hover(move |s| s.bg(soft).border_color(border).text_color(strong))
                    .child("•••")
                    .tooltip(tip("Proje menüsü"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.project_menu = !this.project_menu;
                        this.project_menu_categories = false;
                        cx.notify();
                    })),
            );
        let search = div().flex_none().h(px(44.)).px(px(8.)).flex().items_center().child(
            div()
                .relative()
                .w_full()
                .h(px(32.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .pl(px(29.))
                .pr(px(9.))
                .child(div().absolute().left(px(9.)).top(px(6.5)).text_size(fs(16.)).text_color(p.text_4).child("⌕"))
                // Yer tutucu elle: gpui-component kendi soluk rengini kullanıyor; CSS `::placeholder` = text-4.
                .when(search_empty, |s| s.child(div().absolute().left(px(29.)).top_0().bottom_0().flex().items_center().text_size(fs(13.)).text_color(p.text_4).child("Kanal ve kişi ara")))
                .child(gpui_kit::component::input::Input::new(&self.side_search).appearance(false).w_full().h(px(30.)).px_0().py_0().text_size(fs(13.))),
        );

        // Workspace.tsx `useResizable('p2:sidebar-width',252,190,430,1)` + `<ResizeHandle edge="right">` (direct.css `.channel-sidebar { position: relative }`).
        div()
            .relative()
            .w(px(side_w))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(p.bg_2)
            .border_r_1()
            .border_color(p.border_subtle)
            .child(header)
            .child(search)
            .child(list)
            .child(crate::resize::handle("sidebar-resize", crate::resize::SIDEBAR, crate::resize::Edge::Right, "Kenar çubuğu genişliği", p))
            // I ajanı: arama paneli (`.call-dock`) kenar çubuğunun içinde değil, kökte sabit (call_ui::call_dock).
            .p2_enter("channel-sidebar", p2_ui::Entrance::SlideLeft, std::time::Duration::ZERO)
    }

    // ------------------------------------------------------------ sağ tık menüleri

    /// project-rail.css `.channel-context-menu` görünümü: konum tıklanan nokta, pencere içinde
    /// 8px boşlukla tutulur (Workspace.tsx `ContextMenuViewportGuard`).
    pub(super) fn nav_menu_view(&self, p: &Palette, window: &gpui_kit::Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.nav_menu.clone()?;
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let entry = |id: &'static str, label: String| {
            div()
                .id(id)
                .w_full()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_color(p.text)
                .text_size(fs(12.))
                .css_weight(600.)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child(label)
        };
        let (width, at, items): (f32, Point<Pixels>, Vec<AnyElement>) = match menu {
            NavMenu::Add { at } => {
                // `.project-menu-action` görünümünde iki satır: simge kutusu + başlık + açıklama.
                // Proje oluşturma sunucu yetkisidir (`project.create`): yoksa satır soluk ve tıklanmaz.
                let can_create = self.auth.user.can_server("project.create");
                let row = |id: &'static str, glyph: &'static str, label: String, sub: String, enabled: bool| {
                    div()
                        .id(id)
                        .w_full()
                        .min_h(px(48.))
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(7.))
                        .text_color(p.text)
                        .when(enabled, |s| s.cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)))
                        .when(!enabled, |s| s.opacity(0.45))
                        .child(div().size(px(28.)).flex_none().flex().items_center().justify_center().rounded(px(8.)).bg(p.bg_3).text_color(p.text_accent).text_size(fs(14.)).css_weight(750.).child(glyph))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(fs(12.)).css_weight(700.).child(label)).child(div().text_size(fs(9.5)).css_weight(600.).text_color(p.text_3).child(sub)))
                };
                let items = vec![
                    row("nm-add-join", "↗", p2_i18n::t("rail.join"), p2_i18n::t("rail.joinHint"), true).on_click(cx.listener(|this, _, window, cx| {
                        this.nav_menu = None;
                        this.open_join(None, window, cx);
                    })).into_any_element(),
                    row("nm-add-browse", "◎", p2_i18n::t("disc.menu.browse"), p2_i18n::t("disc.menu.browse.sub"), true).on_click(cx.listener(|this, _, window, cx| {
                        this.nav_menu = None;
                        this.open_discover(window, cx);
                    })).into_any_element(),
                    row("nm-add-new", "＋", p2_i18n::t("rail.create"), p2_i18n::t(if can_create { "rail.createHint" } else { "rail.createForbidden" }), can_create)
                    .when(!can_create, |s| s.tooltip(tip(p2_i18n::t("rail.createForbidden"))))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !can_create {
                            return;
                        }
                        this.nav_menu = None;
                        this.open_dialog(manage_ui::Dialog::NewProject, window, cx);
                    })).into_any_element(),
                ];
                (236., at, items)
            }
            NavMenu::Project { index, at } => {
                // Yalnız kullanabileceğim satırlar görünür (soluk / tıklanamaz satır yok): yönetim paneli
                // en az bir yönetim yetkisi, ayarlar ve silme "Projeyi düzenle" (organization.manage) ister.
                // Yetkiler o projeye göre (`project_perms`); "Projeden ayrıl" her zaman durur.
                let visible = self.project_menu_items(index);
                let mut items = Vec::new();
                if visible.contains(&"admin") {
                    items.push(entry("nm-project-admin", p2_i18n::t("rail.menu.admin")).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        if this.project != Some(index) {
                            this.select_project(index, window, cx);
                        }
                        this.open_admin(window, cx);
                    })).into_any_element());
                }
                if visible.contains(&"settings") {
                    items.push(entry("nm-project-settings", p2_i18n::t("rail.menu.settings")).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        if this.project != Some(index) {
                            this.select_project(index, window, cx);
                        }
                        this.open_dialog(manage_ui::Dialog::Settings, window, cx);
                    })).into_any_element());
                }
                if visible.contains(&"delete") {
                    items.push(entry("nm-project-delete", p2_i18n::t("rail.menu.delete")).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        if this.project != Some(index) {
                            this.select_project(index, window, cx);
                        }
                        this.open_dialog(manage_ui::Dialog::DeleteProject, window, cx);
                    })).into_any_element());
                }
                let danger = p.danger_text;
                items.push(entry("nm-project-leave", p2_i18n::t("rail.menu.leave")).text_color(danger).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    if this.project != Some(index) {
                        this.select_project(index, window, cx);
                    }
                    this.open_dialog(manage_ui::Dialog::LeaveProject, window, cx);
                })).into_any_element());
                (196., at, items)
            }
            NavMenu::Group { group, at } => {
                let (a, b) = (group.clone(), group.clone());
                let (gid, danger) = (group.id.clone(), p.danger_text);
                let items = vec![
                    entry("nm-group-open", p2_i18n::t("dm.group.open")).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        this.select_group(a.clone(), window, cx);
                    })).into_any_element(),
                    entry("nm-group-manage", p2_i18n::t("dm.group.manage")).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        this.select_group(b.clone(), window, cx);
                        this.open_group_members(b.clone(), window, cx);
                    })).into_any_element(),
                    entry("nm-group-leave", p2_i18n::t("dm.group.leave")).text_color(danger).on_click(cx.listener(move |this, _, _, cx| {
                        this.nav_menu = None;
                        this.ask_leave_group(gid.clone(), cx);
                    })).into_any_element(),
                ];
                (200., at, items)
            }
            NavMenu::Person { person, dm, at } => {
                // Kişi yıldızı kapsamlıdır: Ana sayfada Ana sayfa yıldızı, proje içinde o projenin yıldızı.
                let starred = self.person_starred(&person.id);
                let viewport = window.viewport_size();
                let at = point(at.x.min(viewport.width - px(190.)), at.y.min(viewport.height - px(90.)));
                let (a, b, c, d) = (person.clone(), person.clone(), person.id.clone(), person.id.clone());
                let items = vec![
                    entry("nm-person-profile", "Profili görüntüle".into()).on_click(cx.listener(move |this, _, _, cx| {
                        this.nav_menu = None;
                        this.profile = Some(a.clone());
                        cx.notify();
                    })).into_any_element(),
                    entry("nm-person-dm", "Mesaj gönder".into()).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        this.open_direct(b.clone(), window, cx);
                    })).into_any_element(),
                    entry("nm-person-call", "Sesli ara".into()).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        let (id, scope) = (c.clone(), this.dm_scope());
                        this.start_direct_call(&id, scope, window, cx);
                    })).into_any_element(),
                    entry("nm-person-star", if starred { "Yıldızı kaldır".into() } else { "Yıldıza ekle".into() }).on_click(cx.listener(move |this, _, window, cx| {
                        this.nav_menu = None;
                        this.toggle_star("PERSON", d.clone(), window, cx);
                    })).into_any_element(),
                ];
                let mut items = items;
                // Sohbet satırından açıldıysa: listeden kaldır (mesajlar silinmez).
                if let Some(conversation) = dm.clone() {
                    items.push(entry("nm-person-hide", p2_i18n::t("dm.hide")).on_click(cx.listener(move |this, _, window, cx| {
                        this.hide_conversation(conversation.clone(), window, cx);
                    })).into_any_element());
                }
                // Arkadaşlık satırları (friends_ui.rs): ekle / iptal / kabul / çıkar, engelle / engeli kaldır.
                items.extend(self.friend_menu_items(&person, p, cx));
                (190., at, items)
            }
        };
        // Yükseklik: 6+6 iç boşluk + 32px satırlar (＋ menüsünde 48px) + 2px kenarlık.
        let row_h = if matches!(self.nav_menu, Some(NavMenu::Add { .. })) { 48. } else { 32. };
        let height = 14. + row_h * items.len() as f32;
        let viewport = window.viewport_size();
        let gap = px(8.);
        let x = at.x.min(viewport.width - gap - px(width)).max(gap);
        let y = at.y.min(viewport.height - gap - px(height)).max(gap);
        Some(
            div()
                .id("nav-menu-layer")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.nav_menu = None;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                    this.nav_menu = None;
                    cx.notify();
                }))
                // D: p2_enter öğeyi relative yapar (left/top ezilir) — konum dış sarmalayıcıda;
                // nokta pencere koordinatı, katman başlık çubuğunun (38px) altında.
                .child(div().absolute().left(x).top(y - px(38.)).child(
                    div()
                        .id("nav-menu")
                        .w(px(width))
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .rounded(px(11.))
                        .border_1()
                        .border_color(p.border_strong)
                        .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(items)
                        .p2_enter("nav-menu", p2_ui::Entrance::ScaleIn, std::time::Duration::ZERO),
                ))
                .into_any_element(),
        )
    }
}

/// Ray sürükleme yükü.
#[derive(Clone)]
pub(super) struct ProjectDrag {
    pub id: String,
    #[allow(dead_code)]
    pub index: usize,
}

/// Sürüklenen proje avatarı (Chromium'un sürükleme görüntüsü yerine).
struct DragGhost {
    hue: Hue,
    label: String,
    active: bool,
}

impl gpui_kit::Render for DragGhost {
    fn render(&mut self, _: &mut gpui_kit::Window, cx: &mut Context<Self>) -> impl IntoElement {
        use p2_ui::theme::ActiveP2 as _;
        let p = cx.p2().clone();
        div()
            .size(px(42.))
            .flex()
            .items_center()
            .justify_center()
            .opacity(0.7)
            .rounded(px(if self.active { 11. } else { 14. }))
            .border_1()
            .border_color(if self.active { p.border_accent } else { p.border_subtle })
            .bg(hue_grad(&self.hue))
            .text_color(if self.active { p.text_strong } else { self.hue.fg })
            .text_size(fs(11.))
            .css_weight(800.)
            .tracking(-0.02)
            .child(self.label.clone())
    }
}

impl WorkspaceScreen {
    fn order_file(&self) -> std::path::PathBuf {
        // Electron: localStorage `p2:project-order:<kuruluş>:<kullanıcı>`.
        p2_client_core::store::profile_dir().join(format!("project-order-{}-{}.json", self.org.id, self.auth.user.id))
    }

    /// Kayıtlı sırayı uygular (bilinmeyenler sona, gelen sırayla).
    pub(super) fn apply_project_order(&mut self) {
        let saved: Vec<String> = std::fs::read(self.order_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if saved.is_empty() {
            return;
        }
        let rank = |id: &str, i: usize| saved.iter().position(|s| s == id).unwrap_or(saved.len() + i);
        let mut rows: Vec<(usize, p2_client_core::Project)> = std::mem::take(&mut self.projects).into_iter().enumerate().map(|(i, p)| (rank(&p.id, i), p)).collect();
        rows.sort_by_key(|(r, _)| *r);
        self.projects = rows.into_iter().map(|(_, p)| p).collect();
    }

    fn drop_project(&mut self, source: &str, target: &str, cx: &mut Context<Self>) {
        self.dragging_project = None;
        self.drag_target = None;
        let (Some(from), Some(to)) = (self.projects.iter().position(|p| p.id == source), self.projects.iter().position(|p| p.id == target)) else {
            cx.notify();
            return;
        };
        if from != to {
            let current = self.project.map(|i| self.projects[i].id.clone());
            let moved = self.projects.remove(from);
            self.projects.insert(to, moved);
            self.project = current.and_then(|id| self.projects.iter().position(|p| p.id == id));
            let ids: Vec<&str> = self.projects.iter().map(|p| p.id.as_str()).collect();
            if let Ok(json) = serde_json::to_vec(&ids) {
                let _ = std::fs::write(self.order_file(), json);
            }
            crate::trace(&format!("ray: proje sırası {from} → {to}"));
        }
        cx.notify();
    }
}

/// Avatar içi: fotoğraf varsa (`img`, `border-radius: inherit`, cover) yoksa baş harfler.
pub(super) fn face(user_id: &str, name: &str, radius: f32) -> AnyElement {
    match crate::avatars::image(user_id) {
        Some(image) => gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().rounded(px(radius)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
        None => p2_ui::components::initials_label(user_initials(name)).into_any_element(),
    }
}

pub(super) fn toggle_set(set: &mut std::collections::HashSet<String>, key: &str) {
    if !set.remove(key) {
        set.insert(key.to_string());
    }
}

/// Workspace.tsx `projectInitials`: ilk üç kelimenin baş harfleri (tr-TR büyük).
fn user_initials_project(name: &str) -> String {
    let out: String = name.split_whitespace().take(3).filter_map(|w| w.chars().next()).collect();
    let out = tr_upper(&out);
    if out.is_empty() { "P".into() } else { out }
}

thread_local! {
    /// Kanal listesinin kaydırma tutamağı (taşma ölçümü; kaydırma çubuğu payı için).
    static CHANNEL_SCROLL: gpui_kit::ScrollHandle = gpui_kit::ScrollHandle::new();
}
