//! Kütüphane pencereleri (LibraryPage.tsx alt bileşenleri): sağ tık menüsü (`.channel-context-menu.library-menu`),
//! ConfirmDialog, Sürümler, Sürüm yönetimi (ReleaseCard + ReleaseDiff), LibraryUploader,
//! EditReleaseDialog, AchievementDialog, InstallsDialog. Hepsi pencere katmanında (Electron `position: fixed`).

use std::path::{Path, PathBuf};
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, MouseButton, PathPromptOptions, SharedString, Window, div, point, px, relative};
use p2_client_core::{ReleaseDiff, ReleaseView};
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, modal, modal_body, modal_foot, modal_head};
use p2_ui::tokens::{Palette, css_rgba, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::editor::{self, NoteBox};
use super::ui::*;
use super::{AchDialog, Act, EditRelease, LibraryEvent, LibraryPage, Uploader};
use crate::task;

const ACHIEVEMENT_ICONS: [&str; 16] = ["🏆", "🥇", "🎯", "🔥", "⭐", "💎", "🚀", "🧪", "🐛", "🎮", "👑", "🛠️", "🧭", "⚡", "🎉", "🧠"];

#[derive(Clone)]
enum Item {
    Sep,
    Do { label: String, act: MenuAct, danger: bool, disabled: bool },
}

#[derive(Clone)]
enum MenuAct {
    Open(usize),
    Stop(usize),
    Play(usize),
    Install(ReleaseView),
    Versions,
    Folder(String),
    Verify(String, String),
    Shortcut(usize),
    Manage,
    Settings(String),
    Uninstall(String, String),
}

/// library.ts `nextVersion`: son sürümün yaması +1.
fn next_version(latest: Option<&str>) -> String {
    let Some(v) = latest else { return "1.0.0".into() };
    let parts: Vec<&str> = v.splitn(3, '.').collect();
    if parts.len() == 3 {
        let digits: String = parts[2].chars().take_while(char::is_ascii_digit).collect();
        if let (Ok(_), Ok(_), Ok(patch)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>(), digits.parse::<u64>()) {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    v.to_string()
}

/// Build klasörü taraması (pickBuildFolder): .exe'ler (klasöre göre), dosya sayısı, toplam boyut.
fn scan_build(dir: &Path) -> (Vec<String>, u64, u64) {
    let (mut exes, mut count, mut total) = (Vec::new(), 0u64, 0u64);
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                if depth < 6 && path.file_name().is_some_and(|n| n != ".p2") {
                    stack.push((path, depth + 1));
                }
            } else {
                count += 1;
                total += e.metadata().map(|m| m.len()).unwrap_or(0);
                if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) && depth <= 2 {
                    if let Ok(rel) = path.strip_prefix(dir) {
                        exes.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
    }
    exes.sort_by_key(|e| (e.matches('/').count(), e.to_lowercase()));
    (exes, count, total)
}

/// Pencere boyutunda katman (Electron `position: fixed; inset: 0`). Yerleşim tüm pencereye göredir, ama
/// `.library-modal-backdrop` (z-index 120) başlık çubuğunun (`.titlebar` z-index 1000) altında kalır: katman 38px'ten
/// itibaren kırpılır; başlık çubuğu karartılmaz, kartın üstte kalan kısmı (max-height 100vh-64 → üst 32) gizlenir.
fn window_layer(id: &'static str, priority: usize, window: &Window, child: impl IntoElement) -> AnyElement {
    const TITLEBAR: f32 = 38.;
    let size = window.viewport_size();
    let inner = div().absolute().top(px(-TITLEBAR)).left_0().w(size.width).h(size.height).child(child);
    gpui_kit::deferred(
        gpui_kit::anchored()
            .position_mode(gpui_kit::AnchoredPositionMode::Window)
            .position(point(px(0.), px(TITLEBAR)))
            .child(div().id(id).relative().overflow_hidden().w(size.width).h(size.height - px(TITLEBAR)).child(inner)),
    )
    .with_priority(priority)
    .into_any_element()
}

impl LibraryPage {
    pub(super) fn overlays(&mut self, window: &mut Window, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if self.menu.is_some() {
            out.push(self.menu_view(window, p, cx));
        }
        if self.versions_open {
            out.push(self.versions_view(window, p, cx));
        }
        if self.manage_open && self.can_publish() {
            out.push(self.manage_view(window, p, cx));
        }
        if self.uploader.is_some() {
            out.push(self.uploader_view(window, p, cx));
        }
        if self.editing.is_some() {
            out.push(self.edit_view(window, p, cx));
        }
        if self.achievement.is_some() {
            out.push(self.achievement_view(window, p, cx));
        }
        if self.installs.is_some() {
            out.push(self.installs_view(window, p, cx));
        }
        if self.confirm.is_some() {
            out.push(self.confirm_view(window, p, cx));
        }
        out
    }

    // ------------------------------------------------------------ sağ tık menüsü

    fn menu_items(&self, target: usize) -> Vec<Item> {
        let project = &self.projects[target];
        let inst = self.state.projects.get(&project.id);
        let running = self.running.contains(&project.id);
        let busy = self.jobs.get(&project.id).is_some_and(|j| j.active);
        let current = target == self.project;
        let can_use = true;
        let live = if current { self.live().cloned() } else { None };
        let updatable = current && inst.zip(live.as_ref()).is_some_and(|(i, l)| i.release_id != l.id && !i.pinned);
        let mut items = Vec::new();
        let item = |label: String, act: MenuAct, danger: bool, disabled: bool| Item::Do { label, act, danger, disabled };
        if !current {
            items.push(item(p2_i18n::t("libd.menu.openPage"), MenuAct::Open(target), false, false));
        }
        if can_use && inst.is_some() {
            items.push(if running { item(p2_i18n::t("libd.menu.stop"), MenuAct::Stop(target), false, false) } else { item(p2_i18n::t("libd.menu.run"), MenuAct::Play(target), false, busy) });
        }
        if can_use && current && inst.is_none() {
            if let Some(l) = &live {
                items.push(item(p2_i18n::tv("libd.menu.install", &[("size", &fmt_bytes(l.total_size))]), MenuAct::Install(l.clone()), false, busy));
            }
        }
        if can_use && updatable {
            let l = live.clone().unwrap();
            items.push(item(p2_i18n::tv("lib.updateTo", &[("version", &l.version)]), MenuAct::Install(l), false, busy));
        }
        if can_use && current && inst.is_some_and(|i| i.pinned) {
            if let Some(l) = live.as_ref().filter(|l| inst.is_some_and(|i| i.release_id != l.id)) {
                items.push(item(p2_i18n::tv("libd.menu.backToLive", &[("version", &l.version)]), MenuAct::Install(l.clone()), false, busy));
            }
        }
        if can_use && current && !self.published().is_empty() {
            items.push(item(p2_i18n::t("libd.menu.versions"), MenuAct::Versions, false, false));
        }
        if can_use && inst.is_some() {
            items.push(Item::Sep);
            items.push(item(p2_i18n::t("libd.menu.openFolder"), MenuAct::Folder(project.id.clone()), false, false));
            items.push(item(p2_i18n::t("libd.menu.verify"), MenuAct::Verify(project.id.clone(), project.name.clone()), false, busy || running));
            items.push(item(p2_i18n::t("libd.menu.shortcut"), MenuAct::Shortcut(target), false, false));
        }
        let publish = self.can_publish() && current;
        if publish || self.can_edit_project {
            items.push(Item::Sep);
        }
        if publish {
            items.push(item(p2_i18n::t("libd.menu.manage"), MenuAct::Manage, false, false));
        }
        if self.can_edit_project {
            items.push(item(p2_i18n::t("libd.menu.projectSettings"), MenuAct::Settings(project.id.clone()), false, false));
        }
        if can_use && inst.is_some() {
            items.push(Item::Sep);
            items.push(item(p2_i18n::t("libd.menu.uninstall"), MenuAct::Uninstall(project.id.clone(), project.name.clone()), true, busy || running));
        }
        items
    }

    fn run_menu(&mut self, act: MenuAct, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        match act {
            MenuAct::Open(ix) => self.select_project(ix, window, cx),
            MenuAct::Stop(ix) => self.stop(Some(ix), cx),
            MenuAct::Play(ix) => self.play(Some(ix), cx),
            MenuAct::Install(r) => self.install(Some(r), false, window, cx),
            MenuAct::Versions => self.versions_open = true,
            MenuAct::Folder(id) => {
                if let Some(dir) = self.patcher.install_dir(&id) {
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
            }
            MenuAct::Verify(id, _) => self.ask(p2_i18n::t("libd.verify.title"), p2_i18n::t("libd.verify.body"), &p2_i18n::t("libd.verify.confirm"), false, Act::Verify(id), cx),
            MenuAct::Shortcut(ix) => {
                let (id, name) = (self.projects[ix].id.clone(), self.projects[ix].name.clone());
                match create_shortcut(&id, &name) {
                    Ok(()) => crate::trace(&format!("kütüphane: kısayol {name}")),
                    Err(e) => self.fail(e, cx),
                }
            }
            MenuAct::Manage => self.manage_open = true,
            MenuAct::Settings(id) => cx.emit(LibraryEvent::ProjectSettings(id)),
            MenuAct::Uninstall(id, name) => self.ask(p2_i18n::t("libd.uninstall.title"), p2_i18n::tv("libd.uninstall.body", &[("name", &name)]), &p2_i18n::t("libd.menu.uninstall"), true, Act::Uninstall(id), cx),
        }
        cx.notify();
    }

    fn menu_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let m = self.menu.clone().unwrap();
        let items = self.menu_items(m.target);
        let vw = f32::from(window.viewport_size().width);
        let vh = f32::from(window.viewport_size().height);
        // openMenuBelow: düğmenin sol-alt köşesinin 6px altı.
        let bar_top = if self.scrolled() >= 330. { 38. } else { 38. + 330. - self.scrolled() };
        let (x, y) = match (m.anchor, self.caret_bounds.get()) {
            // Electron: oku taşıyan düğmenin kendisi (`.library-play-caret`) — ana düğmenin değil.
            (1, Some(b)) => (f32::from(b.origin.x), f32::from(b.origin.y + b.size.height) + 6.),
            (1, None) => {
                // Ok düğmesi henüz çizilmedi (ilk kare): sınırlar bir sonraki karede gelir.
                window.request_animation_frame();
                (72. + self.side_width() + 28., bar_top + 15. + 46. + 6.)
            }
            (2, _) => (vw - 10. - 28. - 34., bar_top + 21. + 34. + 6.),
            _ => (m.x, m.y),
        };
        let left = x.min(vw - 250.).max(4.);
        let top = y.min(vh - 12. - items.len() as f32 * 34.).max(4.);
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let mut list = div()
            .id("library-menu")
            .w(px(236.))
            .p(px(6.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::MD)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
        for (i, item) in items.into_iter().enumerate() {
            list = match item {
                Item::Sep => list.child(div().h(px(1.)).mx(px(6.)).my(px(4.)).bg(p.border_subtle)),
                Item::Do { label, act, danger, disabled } => list.child(
                    div()
                        .id(("lib-menu-item", i))
                        .w_full()
                        .h(px(32.))
                        .flex_shrink_0()
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(radius::XS)
                        .text_color(if danger { p.danger_text } else { p.text })
                        .text_size(fs(12.))
                        .css_weight(600.)
                        .when(disabled, |s| s.opacity(0.5))
                        .when(!disabled, |s| {
                            s.cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)).on_click(cx.listener(move |this, _, window, cx| this.run_menu(act.clone(), window, cx)))
                        })
                        .child(label),
                ),
            };
        }
        // Konum dolguyla (anchored katmanında mutlak konum çalışmıyor).
        let layer = div()
            .size_full()
            .pl(px(left))
            .pt(px(top))
            .flex()
            .items_start()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .child(list.p2_enter(("library-menu-in", m.epoch), Entrance::ScaleIn, Duration::ZERO));
        window_layer("library-menu-layer", 3, window, layer)
    }

    // ------------------------------------------------------------ ConfirmDialog (Modal sm)

    fn confirm_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let c = self.confirm.clone().unwrap();
        let close = button("lib-confirm-x", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×").on_click(cx.listener(|this, _, _, cx| {
            this.confirm = None;
            cx.notify();
        }));
        let act = c.act.clone();
        let card = modal(ModalSize::Sm, window, p)
            .child(modal_head(c.title.clone(), Some(c.body.clone().into()), Some(close.into_any_element()), p))
            .child(modal_body())
            .child(
                modal_foot(p)
                    .child(button("lib-confirm-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child(p2_i18n::t("lib.cancelEdit")).on_click(cx.listener(|this, _, _, cx| {
                        this.confirm = None;
                        cx.notify();
                    })))
                    .child(button("lib-confirm-yes", if c.danger { BtnVariant::Danger } else { BtnVariant::Primary }, BtnSize::Md, false, false, p).child(c.label.clone()).on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm = None;
                        this.run_confirm(act.clone(), window, cx);
                    }))),
            )
            .p2_enter("lib-confirm-card", Entrance::PopElastic, Duration::ZERO);
        let layer = div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .pt(px(38.))
            .px(px(24.))
            .pb(px(24.))
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.confirm = None;
                cx.notify();
            }))
            .child(div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card))
            .p2_enter("lib-confirm-layer", Entrance::Fade, Duration::ZERO);
        window_layer("lib-confirm", 6, window, layer)
    }

    /// `.library-modal-backdrop` + kart; arka plana tık `close`.
    fn backdrop(&self, id: &'static str, priority: usize, card: impl IntoElement, close: fn(&mut Self), window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let layer = div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                close(this);
                cx.notify();
            }))
            .child(div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card))
            .p2_enter(SharedString::from(format!("{id}-fade")), Entrance::Fade, Duration::ZERO);
        window_layer(id, priority, window, layer)
    }

    fn modal_header(&self, eyebrow: &str, title: String, close_id: &'static str, close: fn(&mut Self), p: &Palette, cx: &mut Context<Self>) -> gpui_kit::Div {
        div().flex().justify_between().items_start().gap(px(12.)).mb(px(16.)).child(modal_title(eyebrow, title, p)).child(close_x(close_id, p).on_click(cx.listener(move |this, _, _, cx| {
            close(this);
            cx.notify();
        })))
    }

    // ------------------------------------------------------------ Sürümler

    fn versions_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let published = self.published();
        let installed = self.state.projects.get(&self.pid()).cloned();
        let blocked = self.jobs.get(&self.pid()).is_some_and(|j| j.active) || self.running.contains(&self.pid());
        let running = self.running.contains(&self.pid());
        let mut channels: Vec<String> = Vec::new();
        for r in &published {
            if !channels.contains(&r.channel) {
                channels.push(r.channel.clone());
            }
        }
        let mut body = div().flex().flex_col().gap(px(14.)).child(hint(p2_i18n::t("libd.versions.hint"), p));
        for name in channels {
            let mut group = div().flex().flex_col().gap(px(6.)).child(div().mt(px(4.)).mb(px(2.)).text_color(p.text_3).text_size(fs(10.5)).css_weight(800.).tracking(0.08).child(tr_upper(&channel_label(&name))));
            for r in published.iter().filter(|r| r.channel == name) {
                let is_live = self.releases.as_ref().and_then(|l| l.latest.get(&name)).and_then(|x| x.as_ref()).is_some_and(|l| l.id == r.id);
                let is_inst = installed.as_ref().is_some_and(|i| i.release_id == r.id);
                let meta = format!("{}{}{}", fmt_bytes(r.total_size), if is_live { p2_i18n::t("libd.versions.liveSuffix") } else { String::new() }, if is_inst { if installed.as_ref().is_some_and(|i| i.pinned) { p2_i18n::t("libd.versions.installedPinnedSuffix") } else { p2_i18n::t("libd.versions.installedSuffix") } } else { String::new() });
                let rel = r.clone();
                let action: AnyElement = if is_inst {
                    badge(&p2_i18n::t("libd.installed"), Tone::Installed, p).into_any_element()
                } else {
                    let label = if is_live { p2_i18n::t("libd.versions.goLive") } else { p2_i18n::t("libd.versions.installThis") };
                    let id = SharedString::from(format!("lib-ver-{}", r.id));
                    let el = if is_live { primary(id, label, blocked, p) } else { secondary(id, label, Sec::Plain, blocked, p) };
                    el.h(px(32.)).min_h(px(32.)).px(px(12.))
                        .when(running, |s| s.tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("libd.versions.runningTip"), w, cx)))
                        .when(!blocked, |s| {
                            s.on_click(cx.listener(move |this, _, _, cx| {
                                let (title, body) = if is_live {
                                    (p2_i18n::t("libd.versions.goLiveTitle"), p2_i18n::tv("libd.versions.goLiveBody", &[("version", &rel.version)]))
                                } else {
                                    (p2_i18n::t("libd.versions.pinTitle"), p2_i18n::tv("libd.versions.pinBody", &[("version", &rel.version)]))
                                };
                                this.ask(title, body, &p2_i18n::t("libd.versions.install"), false, Act::InstallVersion(rel.clone(), !is_live), cx);
                            }))
                        })
                        .into_any_element()
                };
                group = group.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .py(px(9.))
                        .px(px(12.))
                        .border_1()
                        .border_color(if is_inst { p.border_accent } else { p.border })
                        .rounded(radius::MD)
                        .bg(if is_inst { p.accent_soft } else { p.bg_1 })
                        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_color(p.text_strong).text_size(fs(13.)).font_weight(FontWeight::BOLD).font_family(p2_ui::tokens::FONT_MONO).child(r.version.clone())).child(div().text_color(p.text_3).text_size(fs(11.)).child(meta)))
                        .when(is_live, |s| s.child(badge(&p2_i18n::t("libd.live"), Tone::Live, p)))
                        .child(action),
                );
            }
            body = body.child(group);
        }
        let card = modal_card(560., window, p)
            .id("lib-versions")
            .overflow_y_scroll()
            .child(self.modal_header(&p2_i18n::t("libd.versions.eyebrow"), self.pname(), "lib-versions-x", |t| t.versions_open = false, p, cx))
            .child(body)
            .p2_enter("lib-versions-card", Entrance::PopElastic, Duration::ZERO);
        self.backdrop("lib-versions-layer", 4, card, |t| t.versions_open = false, window, p, cx)
    }

    // ------------------------------------------------------------ Sürüm yönetimi

    fn manage_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let list = self.releases.clone().unwrap_or_default();
        let drafts: Vec<&ReleaseView> = list.releases.iter().filter(|r| r.status == "DRAFT").collect();
        let published = self.published();
        let installed = self.state.projects.get(&self.pid()).map(|i| i.release_id.clone());
        let (soft, bs) = (p.accent_soft, p.border_accent);
        let new_release = div()
            .id("lib-new-release")
            .flex()
            .items_center()
            .gap(px(13.))
            .w_full()
            .py(px(13.))
            .px(px(14.))
            .border_1()
            .border_dashed()
            .border_color(bs)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .cursor_pointer()
            .hover(move |s| s.bg(soft))
            .on_click(cx.listener(|this, _, window, cx| this.open_uploader(window, cx)))
            .child(div().size(px(38.)).flex_shrink_0().flex().items_center().justify_center().rounded(radius::MD).bg(p.accent_grad.background()).text_color(p.text_on_accent).shadow(shadows(&[p.accent_glow_strong])).child(ico(PLUS, 2., 18., p.text_on_accent)))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_color(p.text_strong).text_size(fs(12.5)).css_weight(650.).child(p2_i18n::t("libd.manage.newTitle"))).child(div().text_color(p.text_3).text_size(fs(10.5)).line_height(relative(1.45)).child(p2_i18n::t("libd.manage.newBody"))));
        let section = |title: &str| div().flex().flex_col().child(h3(title, p));
        let mut drafts_el = section(&p2_i18n::t("libd.manage.drafts"));
        if drafts.is_empty() {
            drafts_el = drafts_el.child(hint(p2_i18n::t("libd.manage.noDrafts"), p));
        }
        for r in drafts {
            drafts_el = drafts_el.child(self.release_card(r, true, false, installed.as_deref(), p, cx));
        }
        let mut pub_el = section(&p2_i18n::t("libd.manage.published")).child(div().mt(px(-4.)).mb(px(10.)).child(hint(p2_i18n::t("libd.manage.publishedHint"), p).text_size(fs(11.5))));
        if published.is_empty() {
            pub_el = pub_el.child(hint(p2_i18n::t("libd.manage.noPublished"), p));
        }
        for r in &published {
            let live = list.latest.get(&r.channel).and_then(|x| x.as_ref()).is_some_and(|l| l.id == r.id);
            pub_el = pub_el.child(self.release_card(r, false, live, installed.as_deref(), p, cx));
        }
        let card = modal_card(680., window, p)
            .id("lib-manage")
            .overflow_y_scroll()
            .child(self.modal_header(&p2_i18n::t("libd.manage.eyebrow"), self.pname(), "lib-manage-x", |t| t.manage_open = false, p, cx))
            .child(div().flex().flex_col().gap(px(18.)).child(new_release).child(drafts_el).child(pub_el))
            .p2_enter("lib-manage-card", Entrance::PopElastic, Duration::ZERO);
        self.backdrop("lib-manage-layer", 4, card, |t| t.manage_open = false, window, p, cx)
    }

    fn release_card(&self, r: &ReleaseView, draft: bool, live: bool, installed: Option<&str>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let is_inst = installed == Some(r.id.as_str());
        let id = r.id.clone();
        let diff = self.diffs.get(&r.id).cloned();
        let (bs, hover_bg) = (p.border_strong, p.bg_hover);
        let date = r.published_at.as_deref().map(short_date).unwrap_or_else(|| p2_i18n::t("libd.rc.unpublished"));
        let mut actions = div().flex().items_center().gap(px(6.)).flex_wrap();
        {
            let rel = r.clone();
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-inst-{id}")), p2_i18n::tv("libd.rc.installCount", &[("n", &r.install_count.to_string())]), CardBtn::Plain, p).tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("libd.rc.whoInstalled"), w, cx)).on_click(cx.listener(move |this, _, window, cx| this.open_installs(rel.clone(), window, cx))));
        }
        {
            let id2 = id.clone();
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-diff-{id}")), p2_i18n::t("libd.rc.changes"), if diff.is_some() { CardBtn::Active } else { CardBtn::Plain }, p).on_click(cx.listener(move |this, _, window, cx| this.toggle_diff(id2.clone(), window, cx))));
        }
        {
            let rel = r.clone();
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-edit-{id}")), p2_i18n::t("lib.edit"), CardBtn::Plain, p).on_click(cx.listener(move |this, _, window, cx| this.open_edit(rel.clone(), window, cx))));
        }
        if !draft && !live {
            let (rid, v, ch) = (r.id.clone(), r.version.clone(), channel_label(&r.channel));
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-live-{id}")), p2_i18n::t("libd.rc.makeLive"), CardBtn::Ok, p).on_click(cx.listener(move |this, _, _, cx| {
                this.ask(p2_i18n::t("libd.rc.makeLive"), p2_i18n::tv("libd.rc.makeLiveBody", &[("version", &v), ("channel", &ch)]), &p2_i18n::t("libd.rc.makeLive"), false, Act::Live(rid.clone()), cx)
            })));
        }
        if draft {
            let (rid, v, ch) = (r.id.clone(), r.version.clone(), channel_label(&r.channel));
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-pub-{id}")), p2_i18n::t("libd.rc.publish"), CardBtn::Ok, p).on_click(cx.listener(move |this, _, _, cx| {
                this.ask(p2_i18n::t("libd.rc.publishTitle"), p2_i18n::tv("libd.rc.publishBody", &[("version", &v), ("channel", &ch)]), &p2_i18n::t("libd.rc.publish"), false, Act::Publish(rid.clone()), cx)
            })));
        }
        {
            let (rid, v) = (r.id.clone(), r.version.clone());
            actions = actions.child(card_btn(SharedString::from(format!("lib-rc-del-{id}")), p2_i18n::t("lib.delete"), CardBtn::Danger, p).on_click(cx.listener(move |this, _, _, cx| {
                this.ask(if draft { p2_i18n::t("libd.rc.deleteDraftTitle") } else { p2_i18n::t("libd.rc.deleteReleaseTitle") }, p2_i18n::tv("libd.rc.deleteBody", &[("version", &v)]), &p2_i18n::t("lib.delete"), true, Act::DeleteRelease(rid.clone()), cx)
            })));
        }
        let author = r.created_by.clone().unwrap_or_default();
        let notes = r.notes.clone().filter(|n| !n.trim().is_empty());
        let open = self.expanded.contains(&r.id);
        let card_bg = if live { p.success_soft } else { p.bg_3 };
        let notes_el = notes.map(|n| {
            let long = n.chars().count() > 180;
            let rid = r.id.clone();
            div()
                .relative()
                .mt(px(10.))
                .text_color(p.text)
                .text_size(fs(12.5))
                .line_height(relative(1.55))
                .when(!open, |s| s.max_h(px(96.)).overflow_hidden())
                .child(p2_ui::rich::render_message(&n, p))
                .when(!open, |s| s.child(div().absolute().left_0().right_0().bottom_0().h(px(32.)).bg(gpui_kit::linear_gradient(0., gpui_kit::linear_color_stop(card_bg, 0.), gpui_kit::linear_color_stop(gpui_kit::Hsla { a: 0., ..card_bg }, 1.)))))
                .when(!open && long, |s| {
                    s.child(div().id(SharedString::from(format!("lib-rc-more-{rid}"))).relative().py(px(2.)).text_color(p.text_accent).text_size(fs(11.5)).css_weight(650.).cursor_pointer().hover(|s| s.underline()).child(p2_i18n::t("libd.rc.more")).on_click(cx.listener(move |this, _, _, cx| {
                        this.expanded.insert(rid.clone());
                        cx.notify();
                    })))
                })
        });
        div()
            .id(SharedString::from(format!("lib-rc-{id}")))
            .mb(px(8.))
            .py(px(14.))
            .px(px(15.))
            .border_1()
            .when(draft, |s| s.border_dashed())
            .border_color(if live { p.success_border } else { p.border })
            .rounded(radius::LG)
            .bg(card_bg)
            .when(!live, |s| s.hover(move |s| s.border_color(bs).bg(hover_bg)))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_start()
                    .gap(px(12.))
                    .flex_wrap()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .flex_wrap()
                            .child(div().text_color(p.text_strong).text_size(fs(15.)).css_weight(750.).child(r.version.clone()))
                            .child(chip_badge(&r.channel, p))
                            .when(draft, |s| s.child(badge(&p2_i18n::t("libd.rc.draft"), Tone::Draft, p)))
                            .when(live, |s| s.child(badge(&p2_i18n::t("libd.live"), Tone::Live, p)))
                            .when(is_inst, |s| s.child(badge(&p2_i18n::t("libd.installed"), Tone::Installed, p)))
                            .child(div().text_color(p.text_3).text_size(fs(11.)).child(p2_i18n::tv("libd.rc.meta", &[("size", &fmt_bytes(r.total_size)), ("files", &r.file_count.to_string()), ("date", &date)]))),
                    )
                    .child(actions),
            )
            .child(div().h(px(1.)).mt(px(12.)).bg(p.border_subtle))
            .child(
                div()
                    .mt(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(p.text_3)
                    .text_size(fs(11.5))
                    .child(lib_avatar(&author.id, &author.display_name, author.avatar_url.as_deref(), 24., 9., p))
                    .child(author.display_name.clone())
                    .when_some(r.published_at.clone(), |s, at| s.child(div().text_size(fs(11.)).child(format!(" · {}", time_ago(&at))))),
            )
            .children(notes_el)
            .when_some(diff, |s, d| s.child(diff_panel(d, p)))
            .into_any_element()
    }

    fn toggle_diff(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.diffs.remove(&id).is_some() {
            return cx.notify();
        }
        self.diffs.insert(id.clone(), None);
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.release_diff(&org, &project, &id).map(|d| (id, d)), |this, result, _, cx| {
            match result {
                Ok((id, d)) => {
                    if this.diffs.contains_key(&id) {
                        this.diffs.insert(id, Some(Some(d)));
                    }
                }
                Err(_) => {
                    for v in this.diffs.values_mut() {
                        if v.is_none() {
                            *v = Some(None);
                        }
                    }
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn open_installs(&mut self, release: ReleaseView, window: &mut Window, cx: &mut Context<Self>) {
        self.installs = Some((release, None));
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.release_installs(&org, &project), |this, rows, _, cx| {
            if let Some(i) = this.installs.as_mut() {
                i.1 = Some(rows.unwrap_or_default());
            }
            cx.notify();
        });
        cx.notify();
    }

    fn installs_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (release, rows) = self.installs.clone().unwrap();
        let rows = rows.unwrap_or_default();
        let mut list = div().flex().flex_col().gap(px(3.));
        if rows.is_empty() {
            list = list.child(hint(p2_i18n::t("libd.installs.empty"), p));
        }
        for (i, row) in rows.iter().enumerate() {
            let status = match row.status.as_str() {
                "INSTALLED" => p2_i18n::t("libd.installs.installed"),
                "UPDATING" => p2_i18n::t("libd.installs.updating"),
                s => s.to_lowercase(),
            };
            let other = if row.release_id.as_deref() == Some(release.id.as_str()) { String::new() } else { p2_i18n::t("libd.installs.otherVersion") };
            let played = row.play_seconds.filter(|s| *s > 0).map(|s| p2_i18n::tv("libd.installs.played", &[("duration", &fmt_duration(s as f64))])).unwrap_or_default();
            let hover_bg = p.bg_hover;
            list = list.child(
                div()
                    .id(("lib-install-row", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .py(px(7.))
                    .px(px(8.))
                    .rounded(radius::SM)
                    .hover(move |s| s.bg(hover_bg))
                    .child(lib_avatar(&row.user.id, &row.user.display_name, row.user.avatar_url.as_deref(), 24., 9., p))
                    .child(div().flex().flex_col().child(div().text_color(p.text).text_size(fs(12.)).font_weight(FontWeight::BOLD).child(row.user.display_name.clone())).child(div().mt(px(1.)).text_color(p.text_3).text_size(fs(10.5)).child(format!("{}{other} · {status}{played}", row.version)))),
            );
        }
        let card = modal_card(600., window, p).id("lib-installs").overflow_y_scroll().child(self.modal_header(&p2_i18n::t("libd.installs.eyebrow"), p2_i18n::tv("libd.installs.title", &[("version", &release.version)]), "lib-installs-x", |t| t.installs = None, p, cx)).child(list).p2_enter("lib-installs-card", Entrance::PopElastic, Duration::ZERO);
        self.backdrop("lib-installs-layer", 5, card, |t| t.installs = None, window, p, cx)
    }

    // ------------------------------------------------------------ Yükleyici

    pub(super) fn open_uploader(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let channel = self.active_channel().unwrap_or_default();
        let latest = self.releases.as_ref().and_then(|l| l.latest.get(&channel)).and_then(|r| r.as_ref()).map(|r| r.version.clone());
        let ch = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("libd.up.channelPlaceholder")));
        ch.update(cx, |s, cx| s.set_value(channel.clone(), window, cx));
        let version = cx.new(|cx| InputState::new(window, cx).placeholder("1.0.0"));
        let v = next_version(latest.as_deref());
        version.update(cx, |s, cx| s.set_value(v, window, cx));
        let args = cx.new(|cx| InputState::new(window, cx).placeholder("-windowed -log"));
        let notes = NoteBox::new(&p2_i18n::t("libd.up.notesPlaceholder"), false, window, cx);
        let subs = vec![
            cx.subscribe_in(&ch, window, |this, state, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    let c = state.read(cx).value().trim().to_string();
                    let latest = this.releases.as_ref().and_then(|l| l.latest.get(&c)).and_then(|r| r.as_ref()).map(|r| r.version.clone());
                    if let Some(u) = this.uploader.as_ref() {
                        let v = next_version(latest.as_deref());
                        u.version.update(cx, |s, cx| s.set_value(v, window, cx));
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&version, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        self.uploader = Some(Uploader { dir: None, executables: Vec::new(), exe: 0, file_count: 0, total_size: 0, channel: ch, version, args, notes, phase: "form", release: None, diff: None, error: None, busy: false, exe_open: false, _subs: subs });
        self.upload = None;
        cx.notify();
    }

    fn pick_upload_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(p2_i18n::t("libd.up.buildFolder").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(dir) = paths.first().cloned() {
                    let scan_dir = dir.clone();
                    let (exes, count, total) = cx.background_executor().spawn(async move { scan_build(&scan_dir) }).await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(u) = this.uploader.as_mut() {
                            u.dir = Some(dir);
                            u.executables = exes;
                            u.exe = 0;
                            u.file_count = count;
                            u.total_size = total;
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub fn start_upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.uploader.as_mut() else { return };
        let Some(dir) = u.dir.clone() else { return self.fail(p2_i18n::t("libd.up.pickFolderFirst"), cx) };
        let exe = u.executables.get(u.exe).cloned().unwrap_or_default();
        let channel = u.channel.read(cx).value().split_whitespace().collect::<Vec<_>>().join(" ");
        let version = u.version.read(cx).value().trim().to_string();
        let args = u.args.read(cx).value().trim().to_string();
        let notes = u.notes.value(cx).trim().to_string();
        if exe.is_empty() || version.is_empty() || channel.is_empty() {
            u.error = Some(p2_i18n::t("libd.up.required"));
            return cx.notify();
        }
        u.phase = "working";
        u.error = None;
        let (patcher, org, project) = (self.patcher.clone(), self.org.clone(), self.pid());
        let cancel = Arc::new(AtomicBool::new(false));
        self.upload_cancel = cancel.clone();
        cx.notify();
        task::run(window, cx, move || patcher.upload(&org, &project, &dir, &exe, &args, &channel, &version, &notes, true, &cancel), |this, result, window, cx| {
            match result {
                Ok(release) => {
                    crate::trace(&format!("kütüphane: yükleme tamam {release}"));
                    if let Some(u) = this.uploader.as_mut() {
                        u.phase = "ready";
                        u.release = Some(release.clone());
                        u.diff = None;
                    }
                    let (api, org, project) = (this.api.clone(), this.org.clone(), this.pid());
                    task::run(window, cx, move || api.release_diff(&org, &project, &release).ok(), |this, d, _, cx| {
                        if let Some(u) = this.uploader.as_mut() {
                            u.diff = Some(d);
                        }
                        cx.notify();
                    });
                    this.load(window, cx);
                }
                Err(e) => {
                    let canceled = this.upload_cancel.load(Ordering::SeqCst);
                    if let Some(u) = this.uploader.as_mut() {
                        u.phase = "form";
                        if !canceled {
                            u.error = Some(if e.contains("RELEASE_VERSION_EXISTS") { p2_i18n::t("libd.up.versionExists") } else { e.clone() });
                        }
                    }
                    crate::trace(&format!("kütüphane: yükleme başarısız {e}"));
                }
            }
            cx.notify();
        });
    }

    fn cancel_upload(&mut self, cx: &mut Context<Self>) {
        self.upload_cancel.store(true, Ordering::SeqCst);
        self.uploader = None;
        cx.notify();
    }

    fn finish_upload(&mut self, publish: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(release) = self.uploader.as_ref().and_then(|u| u.release.clone()) else { return };
        if !publish {
            self.uploader = None;
            self.load(window, cx);
            return cx.notify();
        }
        if let Some(u) = self.uploader.as_mut() {
            u.busy = true;
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.publish_release(&org, &project, &release), |this, result, window, cx| match result {
            Ok(_) => {
                this.uploader = None;
                this.load(window, cx);
            }
            Err(e) => {
                if let Some(u) = this.uploader.as_mut() {
                    u.busy = false;
                    u.error = Some(if e.message.is_empty() { p2_i18n::t("lib.err.actionFailed") } else { e.message });
                }
                cx.notify();
            }
        });
    }

    fn uploader_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let u = self.uploader.as_ref().unwrap();
        let form = u.phase == "form";
        let close: fn(&mut Self) = |t| {
            if t.uploader.as_ref().is_some_and(|u| u.phase != "form") {
                t.upload_cancel.store(true, Ordering::SeqCst);
            }
            t.uploader = None;
        };
        let mut body = div().flex().flex_col().gap(px(14.));
        match u.phase {
            "form" => {
                let (soft, bs) = (p.accent_soft, p.border_accent);
                let pick = div()
                    .id("lib-up-dir")
                    .min_h(px(44.))
                    .py(px(10.))
                    .px(px(12.))
                    .border_1()
                    .when(u.dir.is_none(), |s| s.border_dashed())
                    .border_color(bs)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .text_color(p.text_3)
                    .text_size(fs(12.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft))
                    .on_click(cx.listener(|this, _, window, cx| this.pick_upload_dir(window, cx)))
                    .map(|s| match &u.dir {
                        Some(d) => s.flex().flex_col().gap(px(2.)).child(div().text_color(p.text).text_size(fs(12.)).font_weight(FontWeight::BOLD).child(d.display().to_string())).child(div().text_color(p.text_3).text_size(fs(10.5)).child(p2_i18n::tv("libd.up.dirMeta", &[("n", &u.file_count.to_string()), ("size", &fmt_bytes(u.total_size))]))),
                        None => s.child(p2_i18n::t("libd.up.pickFolder")),
                    });
                body = body.child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.buildFolder"), p)).child(pick));
                if u.dir.is_some() {
                    let current = u.executables.get(u.exe).cloned().unwrap_or_else(|| p2_i18n::t("libd.up.noExe"));
                    let mut sel = div().relative().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.exe"), p)).child(
                        div()
                            .id("lib-up-exe")
                            .h(px(34.))
                            .pl(px(10.))
                            .pr(px(32.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(p.border)
                            .rounded(radius::SM)
                            .bg(p.bg_3)
                            .text_color(p.text)
                            .text_size(fs(12.))
                            .cursor_pointer()
                            .child(current)
                            .child(div().absolute().right(px(10.)).text_color(p.text_4).child(ico(CARET, 2.2, 15., p.text_4)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(u) = this.uploader.as_mut() {
                                    u.exe_open = !u.exe_open;
                                }
                                cx.notify();
                            })),
                    );
                    if u.exe_open {
                        let mut opts = div().flex().flex_col().p(px(4.)).border_1().border_color(p.border_strong).rounded(radius::SM).bg(p.bg_2).shadow(p.shadow_lg());
                        for (i, e) in u.executables.iter().enumerate() {
                            let hover = p.accent_soft;
                            opts = opts.child(div().id(("lib-up-exe-opt", i)).h(px(28.)).px(px(8.)).flex().items_center().rounded(radius::XS).text_size(fs(12.)).cursor_pointer().hover(move |s| s.bg(hover)).child(e.clone()).on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(u) = this.uploader.as_mut() {
                                    u.exe = i;
                                    u.exe_open = false;
                                }
                                cx.notify();
                            })));
                        }
                        sel = sel.child(opts);
                    }
                    body = body.child(sel);
                }
                let channels = self.releases.as_ref().map(|l| l.channels.clone()).unwrap_or_default();
                let clean = u.channel.read(cx).value().split_whitespace().collect::<Vec<_>>().join(" ");
                body = body.child(div().flex().gap(px(12.)).child(div().flex_1().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("lib.stat.channel"), p)).child(field_input(&u.channel, p))).child(div().flex_1().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.version"), p)).child(field_input(&u.version, p))));
                if !channels.is_empty() {
                    let mut row = div().flex().flex_wrap().gap(px(6.));
                    for c in channels {
                        let cc = c.clone();
                        row = row.child(chip(SharedString::from(format!("lib-up-ch-{c}")), &c, clean == c, p).on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(u) = this.uploader.as_ref() {
                                let v = cc.clone();
                                u.channel.update(cx, |s, cx| s.set_value(v, window, cx));
                            }
                            cx.notify();
                        })));
                    }
                    body = body.child(row);
                }
                body = body
                    .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.argsOptional"), p)).child(field_input(&u.args, p)))
                    .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.notes"), p)).child(editor::note_box("lib-up-notes", &u.notes, self.api.clone(), |v: &mut Self| v.uploader.as_mut().map(|u| &mut u.notes), None, false, p, cx)));
                if let Some(e) = &u.error {
                    body = body.child(error_bar(e.clone(), p));
                }
                let can_start = u.dir.is_some() && !u.executables.is_empty() && !u.version.read(cx).value().trim().is_empty() && !clean.is_empty();
                body = body.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .mt(px(6.))
                        .child(foot_btn("lib-up-cancel", p2_i18n::t("lib.cancelEdit"), false, false, p).on_click(cx.listener(|this, _, _, cx| {
                            this.uploader = None;
                            cx.notify();
                        })))
                        .child(foot_btn("lib-up-go", p2_i18n::t("libd.up.start"), true, !can_start, p).when(can_start, |s| s.on_click(cx.listener(|this, _, window, cx| this.start_upload(window, cx))))),
                );
            }
            "working" => {
                let up = self.upload.clone();
                let percent = up.as_ref().map_or(0, |u| u.percent);
                let label = match up.as_ref().map(|u| u.phase.as_str()) {
                    Some("hashing") | Some("scanning") => p2_i18n::t("libd.up.phasePreparing"),
                    Some("registering") => p2_i18n::t("libd.up.phaseRegistering"),
                    _ => p2_i18n::t("libd.up.phaseUploading"),
                };
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py(px(20.))
                        .child(div().relative().w_full().h(px(9.)).rounded(radius::PILL).bg(p.navy_600).overflow_hidden().child(div().h_full().w(relative(percent as f32 / 100.)).rounded(radius::PILL).bg(p.accent_bar.background()).shadow(shadows(&[p2_ui::tokens::Shadow::new(0., 0., 14., 0., css_rgba(79, 157, 255, 0.5))]))))
                        .child(div().my(px(12.)).text_color(p.text_3).text_size(fs(12.)).child(p2_i18n::tv("libd.up.progress", &[("label", &label), ("percent", &percent.to_string())])))
                        .child(foot_btn("lib-up-stop", p2_i18n::t("lib.cancel"), false, false, p).on_click(cx.listener(|this, _, _, cx| this.cancel_upload(cx)))),
                );
            }
            _ => {
                let version = u.version.read(cx).value().to_string();
                let channel = channel_label(&u.channel.read(cx).value().trim().to_string());
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(div().size(px(52.)).mb(px(10.)).flex().items_center().justify_center().rounded_full().bg(p.success_soft).text_color(p.success_text).text_size(fs(26.)).child("✓").p2_enter("lib-ready-badge", Entrance::PopElastic, Duration::ZERO))
                        .child(div().mb(px(6.)).text_color(p.text_strong).text_size(fs(15.)).font_weight(FontWeight::BOLD).child(p2_i18n::t("libd.up.doneTitle")))
                        .child(div().max_w(px(46. * 7.)).mb(px(8.)).text_center().text_color(p.text_3).text_size(fs(12.)).line_height(relative(1.5)).child(p2_i18n::tv("libd.up.doneBody", &[("version", &version), ("channel", &channel)])))
                        .child(div().w_full().child(diff_panel(u.diff.clone(), p))),
                );
                if let Some(e) = &u.error {
                    body = body.child(error_bar(e.clone(), p));
                }
                let busy = u.busy;
                body = body.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .mt(px(6.))
                        .child(foot_btn("lib-up-draft", p2_i18n::t("libd.up.keepDraft"), false, busy, p).on_click(cx.listener(|this, _, window, cx| this.finish_upload(false, window, cx))))
                        .child(foot_btn("lib-up-publish", if busy { p2_i18n::t("libd.up.publishing") } else { p2_i18n::t("libd.rc.publish") }, true, busy, p).on_click(cx.listener(|this, _, window, cx| this.finish_upload(true, window, cx)))),
                );
            }
        }
        let card = modal_card(600., window, p)
            .id("lib-uploader")
            .overflow_y_scroll()
            .child(self.modal_header(&p2_i18n::t("libd.up.eyebrow"), p2_i18n::tv("libd.up.title", &[("name", &self.pname())]), "lib-up-x", close, p, cx))
            .child(body)
            .p2_enter("lib-uploader-card", Entrance::PopElastic, Duration::ZERO);
        if form {
            self.backdrop("lib-uploader-layer", 5, card, |t| t.uploader = None, window, p, cx)
        } else {
            self.backdrop("lib-uploader-layer", 5, card, |_| {}, window, p, cx)
        }
    }

    // ------------------------------------------------------------ Sürüm düzenleme

    fn open_edit(&mut self, release: ReleaseView, window: &mut Window, cx: &mut Context<Self>) {
        let mk = |v: &str, window: &mut Window, cx: &mut Context<Self>| {
            let v = v.to_string();
            let s = cx.new(|cx| InputState::new(window, cx));
            s.update(cx, |s, cx| s.set_value(v, window, cx));
            s
        };
        let channel = mk(&release.channel, window, cx);
        let version = mk(&release.version, window, cx);
        let args = cx.new(|cx| InputState::new(window, cx).placeholder("-windowed -log"));
        let a = release.launch_args.clone().unwrap_or_default();
        args.update(cx, |s, cx| s.set_value(a, window, cx));
        let mut notes = NoteBox::new(&p2_i18n::t("libd.up.notesPlaceholder"), false, window, cx);
        notes.set(release.notes.as_deref().unwrap_or(""), window, cx);
        self.editing = Some(EditRelease { release, channel, version, args, notes, busy: false, error: None });
        cx.notify();
    }

    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        let Some(e) = self.editing.as_mut() else { return };
        let draft = e.release.status == "DRAFT";
        let mut body = serde_json::json!({ "notes": e.notes.value(cx), "launchArgs": e.args.read(cx).value().to_string(), "overlay": true });
        if draft {
            body["channel"] = e.channel.read(cx).value().split_whitespace().collect::<Vec<_>>().join(" ").into();
            body["version"] = e.version.read(cx).value().trim().to_string().into();
        }
        e.busy = true;
        e.error = None;
        let id = e.release.id.clone();
        task::run(window, cx, move || api.update_release(&org, &project, &id, body), |this, result, window, cx| match result {
            Ok(_) => {
                this.editing = None;
                this.load(window, cx);
            }
            Err(err) => {
                if let Some(e) = this.editing.as_mut() {
                    e.busy = false;
                    e.error = Some(if err.message.is_empty() { p2_i18n::t("lib.err.saveFailed") } else { err.message });
                }
                cx.notify();
            }
        });
        cx.notify();
    }

    fn edit_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let e = self.editing.as_ref().unwrap();
        let draft = e.release.status == "DRAFT";
        let channels = self.releases.as_ref().map(|l| l.channels.clone()).unwrap_or_default();
        let clean = e.channel.read(cx).value().split_whitespace().collect::<Vec<_>>().join(" ");
        let can_save = !clean.is_empty() && !e.version.read(cx).value().trim().is_empty() && !e.busy;
        let mut body = div().flex().flex_col().gap(px(14.)).child(
            div()
                .flex()
                .gap(px(12.))
                .child(div().flex_1().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("lib.stat.channel"), p)).child(field_input(&e.channel, p).when(!draft, |s| s.opacity(0.55))))
                .child(div().flex_1().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.version"), p)).child(field_input(&e.version, p).when(!draft, |s| s.opacity(0.55)))),
        );
        if draft && !channels.is_empty() {
            let mut row = div().flex().flex_wrap().gap(px(6.));
            for c in channels {
                let cc = c.clone();
                row = row.child(chip(SharedString::from(format!("lib-ed-ch-{c}")), &c, clean == c, p).on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(e) = this.editing.as_ref() {
                        let v = cc.clone();
                        e.channel.update(cx, |s, cx| s.set_value(v, window, cx));
                    }
                    cx.notify();
                })));
            }
            body = body.child(row);
        }
        if !draft {
            body = body.child(hint(p2_i18n::t("libd.edit.publishedHint"), p));
        }
        body = body
            .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.args"), p)).child(field_input(&e.args, p)))
            .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.up.notes"), p)).child(editor::note_box("lib-ed-notes", &e.notes, self.api.clone(), |v: &mut Self| v.editing.as_mut().map(|e| &mut e.notes), None, false, p, cx)));
        if let Some(err) = &e.error {
            body = body.child(error_bar(err.clone(), p));
        }
        body = body.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .mt(px(6.))
                .child(foot_btn("lib-ed-cancel", p2_i18n::t("lib.cancelEdit"), false, false, p).on_click(cx.listener(|this, _, _, cx| {
                    this.editing = None;
                    cx.notify();
                })))
                .child(foot_btn("lib-ed-save", if e.busy { p2_i18n::t("lib.saving") } else { p2_i18n::t("lib.save") }, true, !can_save, p).when(can_save, |s| s.on_click(cx.listener(|this, _, window, cx| this.save_edit(window, cx))))),
        );
        let title = format!("{} · {}", e.release.version, channel_label(&e.release.channel));
        let card = modal_card(600., window, p)
            .id("lib-edit")
            .overflow_y_scroll()
            .child(self.modal_header(&if draft { p2_i18n::t("libd.edit.eyebrowDraft") } else { p2_i18n::t("libd.edit.eyebrowRelease") }, title, "lib-ed-x", |t| t.editing = None, p, cx))
            .child(body)
            .p2_enter("lib-edit-card", Entrance::PopElastic, Duration::ZERO);
        self.backdrop("lib-edit-layer", 5, card, |t| t.editing = None, window, p, cx)
    }

    // ------------------------------------------------------------ Başarım

    pub(super) fn open_achievement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let custom = cx.new(|cx| InputState::new(window, cx));
        custom.update(cx, |s, cx| s.set_value("🏆", window, cx));
        let title = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("libd.ach.titlePlaceholder")));
        let description = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("libd.ach.descPlaceholder")));
        title.update(cx, |s, cx| s.focus(window, cx));
        let subs = vec![
            cx.subscribe(&title, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&custom, |this, state, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    let v: String = state.read(cx).value().chars().take(8).collect();
                    if let Some(a) = this.achievement.as_mut() {
                        a.icon = v;
                    }
                }
                cx.notify();
            }),
        ];
        self.achievement = Some(AchDialog { icon: "🏆".into(), custom, title, description, busy: false, error: None, _subs: subs });
        cx.notify();
    }

    fn save_achievement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.achievement.as_mut() else { return };
        let title = a.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            return;
        }
        let (description, icon) = (a.description.read(cx).value().trim().to_string(), a.icon.clone());
        a.busy = true;
        a.error = None;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.create_achievement(&org, &project, &title, &description, &icon), |this, result, window, cx| match result {
            Ok(_) => {
                this.achievement = None;
                this.load_hub(window, cx);
            }
            Err(e) => {
                if let Some(a) = this.achievement.as_mut() {
                    a.busy = false;
                    a.error = Some(if e.message.is_empty() { p2_i18n::t("lib.err.saveFailed") } else { e.message });
                }
                cx.notify();
            }
        });
        cx.notify();
    }

    fn achievement_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let a = self.achievement.as_ref().unwrap();
        let mut icons = div().flex().flex_wrap().gap(px(5.)).items_center();
        for icon in ACHIEVEMENT_ICONS {
            let active = a.icon == icon;
            icons = icons.child(
                div()
                    .id(SharedString::from(format!("lib-ach-icon-{icon}")))
                    .size(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(if active { p.warning } else { p.border })
                    .rounded(radius::SM)
                    .bg(if active { p.warning_soft } else { p.bg_3 })
                    .text_size(fs(18.))
                    .font_family(p2_ui::tokens::FONT_EMOJI)
                    .cursor_pointer()
                    .child(icon)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(a) = this.achievement.as_mut() {
                            a.icon = icon.to_string();
                            let v = icon.to_string();
                            a.custom.update(cx, |s, cx| s.set_value(v, window, cx));
                        }
                        cx.notify();
                    })),
            );
        }
        // .library-field input kuralı .library-icon-choices input'u ezer: tam genişlik (kendi satırına sarar), h 34, 12px; ortalı metin kalır.
        icons = icons.child(field_input_live(&a.custom, p, window, cx).w_full().child(Input::new(&a.custom).appearance(false).text_size(fs(12.)).text_align(gpui_kit::TextAlign::Center)));
        let can = !a.title.read(cx).value().trim().is_empty() && !a.busy;
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.ach.icon"), p)).child(icons))
            .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.ach.name"), p)).child(field_input_live(&a.title, p, window, cx).child(Input::new(&a.title).appearance(false).text_size(fs(12.)))))
            .child(div().flex().flex_col().gap(px(5.)).child(field_label(&p2_i18n::t("libd.ach.descOptional"), p)).child(field_input_live(&a.description, p, window, cx).child(Input::new(&a.description).appearance(false).text_size(fs(12.)))))
            .child(hint(p2_i18n::t("libd.ach.hint"), p));
        if let Some(e) = &a.error {
            body = body.child(error_bar(e.clone(), p));
        }
        body = body.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .mt(px(6.))
                .child(foot_btn("lib-ach-cancel", p2_i18n::t("lib.cancelEdit"), false, false, p).on_click(cx.listener(|this, _, _, cx| {
                    this.achievement = None;
                    cx.notify();
                })))
                .child(foot_btn("lib-ach-save", if a.busy { p2_i18n::t("lib.saving") } else { p2_i18n::t("libd.ach.add") }, true, !can, p).when(can, |s| s.on_click(cx.listener(|this, _, window, cx| this.save_achievement(window, cx))))),
        );
        let card = modal_card(480., window, p).id("lib-ach").child(self.modal_header(&p2_i18n::t("libd.ach.eyebrow"), p2_i18n::t("libd.ach.title"), "lib-ach-x", |t| t.achievement = None, p, cx)).child(body).p2_enter("lib-ach-card", Entrance::PopElastic, Duration::ZERO);
        self.backdrop("lib-ach-layer", 5, card, |t| t.achievement = None, window, p, cx)
    }
}

/// `.library-field input`: h 34, p 0 10, kenarlık, r 8, `bg-3`, 12px.
pub(super) fn field_input(state: &gpui_kit::Entity<InputState>, p: &Palette) -> gpui_kit::Div {
    div().h(px(34.)).px(px(10.)).flex().items_center().border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_3).text_color(p.text).text_size(fs(12.)).child(Input::new(state).appearance(false))
}

/// `field_input` kabuğu (girdi çağıranda) + `.library-field input:focus`: `border-accent` + `ring`.
fn field_input_live(state: &gpui_kit::Entity<InputState>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> gpui_kit::Div {
    let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
    div().h(px(34.)).px(px(10.)).flex().items_center().border_1().rounded(radius::SM).bg(p.bg_3).text_color(p.text).text_size(fs(12.)).map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
}

/// ReleaseDiff paneli (`.library-diff`). `None` = yükleniyor, `Some(None)` = hesaplanamadı.
fn diff_panel(diff: Option<Option<ReleaseDiff>>, p: &Palette) -> AnyElement {
    let shell = div().flex().flex_col().gap(px(10.)).mt(px(12.)).py(px(12.)).px(px(13.)).border_1().border_color(p.border_subtle).rounded(radius::MD).bg(p.bg_0);
    let d = match diff {
        None => return shell.child(hint(p2_i18n::t("libd.diff.loading"), p)).into_any_element(),
        Some(None) => return shell.child(hint(p2_i18n::t("libd.diff.failed"), p)).into_any_element(),
        Some(Some(d)) => d,
    };
    let Some(against) = &d.against else {
        return shell.child(hint(p2_i18n::tv("libd.diff.first", &[("n", &d.added.to_string()), ("size", &fmt_bytes(d.added_bytes))]), p)).into_any_element();
    };
    let nothing = d.added == 0 && d.changed == 0 && d.removed == 0;
    let stat = |label: &str, value: u64, sub: String, color: gpui_kit::Hsla| {
        div()
            .flex_1()
            .min_w(px(96.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .py(px(8.))
            .px(px(10.))
            .rounded(radius::SM)
            .bg(p.bg_3)
            .child(div().text_color(p.text_accent).text_size(font_size::XXS).css_weight(800.).tracking(0.1).child(tr_upper(label)))
            .child(div().text_color(color).text_size(fs(17.)).css_weight(750.).tracking(-0.02).child(value.to_string()))
            .child(div().text_color(p.text_3).text_size(fs(10.5)).child(sub))
    };
    let mut out = shell.child(
        div()
            .flex()
            .justify_between()
            .items_center()
            .gap(px(10.))
            .flex_wrap()
            .text_color(p.text_2)
            .text_size(fs(12.))
            .child({ let (before, after) = (p2_i18n::t("libd.diff.againstBefore"), p2_i18n::t("libd.diff.againstAfter")); div().flex().gap(px(4.)).when(!before.is_empty(), |s| s.child(before)).child(div().text_color(p.text_strong).font_weight(FontWeight::BOLD).child(against.version.clone())).child(div().text_color(p.text_3).child(format!("({})", against.channel))).when(!after.is_empty(), |s| s.child(after)) })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(4.))
                    .rounded(radius::PILL)
                    .bg(p.accent_soft)
                    .text_color(p.text_accent)
                    .text_size(fs(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(ico(DIFF_DOWN, 2., 13., p.text_accent))
                    .child(p2_i18n::t("libd.diff.download"))
                    .child(div().text_color(p.text_strong).font_weight(FontWeight::BOLD).child(fmt_bytes(d.download_bytes))),
            ),
    );
    if nothing {
        out = out.child(hint(p2_i18n::t("libd.diff.same"), p));
    } else {
        out = out.child(
            div()
                .flex()
                .gap(px(6.))
                .child(stat(&p2_i18n::t("libd.diff.new"), d.added, fmt_bytes(d.added_bytes), p.success_text))
                .child(stat(&p2_i18n::t("libd.diff.changed"), d.changed, fmt_bytes(d.changed_bytes), p.warning_text))
                .child(stat(&p2_i18n::t("libd.diff.removed"), d.removed, fmt_bytes(d.removed_bytes), p.danger_text))
                .child(stat(&p2_i18n::t("libd.diff.same2"), d.unchanged, p2_i18n::t("libd.diff.files"), p.text_strong)),
        );
        let group = |label: &str, rows: Vec<(String, String)>, dot: gpui_kit::Hsla| {
            (!rows.is_empty()).then(|| {
                // <details> kapalı: yalnız özet satırı (ad + adet).
                let n = rows.len();
                div().flex().items_center().gap(px(7.)).py(px(6.)).px(px(8.)).rounded(px(7.)).text_color(p.text_2).text_size(fs(11.5)).font_weight(FontWeight::SEMIBOLD).child(div().size(px(8.)).rounded_full().bg(dot)).child(label.to_string()).child(div().ml_auto().text_color(p.text_3).text_size(fs(10.)).child(n.to_string()))
            })
        };
        out = out
            .children(group(&p2_i18n::t("libd.diff.newFiles"), d.files.added.iter().map(|f| (f.path.clone(), fmt_bytes(f.size))).collect(), p.success))
            .children(group(&p2_i18n::t("libd.diff.changedFiles"), d.files.changed.iter().map(|f| (f.path.clone(), format!("{} → {}", fmt_bytes(f.previous_size), fmt_bytes(f.size)))).collect(), p.warning))
            .children(group(&p2_i18n::t("libd.diff.removedFiles"), d.files.removed.iter().map(|f| (f.path.clone(), fmt_bytes(f.size))).collect(), p.danger));
        if d.truncated {
            out = out.child(hint(p2_i18n::t("libd.diff.truncated"), p));
        }
    }
    out.into_any_element()
}

/// macOS: masaüstüne `<proje>.webloc` → `p2://play/<kimlik>` (Finder bağlantıyı P2 ile açar;
/// protokol Info.plist'te kayıtlı).
#[cfg(not(windows))]
fn create_shortcut(id: &str, name: &str) -> Result<(), String> {
    let safe: String = name.chars().map(|c| if "/:".contains(c) { '_' } else { c }).collect::<String>().trim().to_string();
    let file = format!("{}.webloc", if safe.is_empty() { p2_i18n::t("libd.shortcut.fallbackName") } else { safe });
    let url: String = format!("p2://play/{id}").chars().filter(|c| !"<>&\"'".contains(*c)).collect();
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>URL</key>\n\t<string>{url}</string>\n</dict>\n</plist>\n"
    );
    let path = p2_client_core::paths::home_dir().join("Desktop").join(file);
    std::fs::write(path, plist).map_err(|_| p2_i18n::t("libd.shortcut.failed"))
}

/// patcher.cjs `createShortcut`: masaüstüne `<proje>.lnk` → bu exe `--p2-play=<kimlik>`.
#[cfg(windows)]
fn create_shortcut(id: &str, name: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    let safe: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect::<String>().trim().to_string();
    // Masaüstü klasörü PowerShell'de çözülür: OneDrive yedeklemesi açıksa %USERPROFILE%\Desktop değildir.
    let link = format!("{}.lnk", if safe.is_empty() { p2_i18n::t("libd.shortcut.fallbackName") } else { safe.clone() });
    let mut exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut icon = String::new();
    if crate::store_update::is_packaged() {
        // Store sürümü: exe yolu (WindowsApps\<paket_sürüm>\…) her güncellemede değişir. Kısayol
        // paketin kalıcı çalıştırma takma adını (AppxManifest uap3:AppExecutionAlias) hedefler;
        // simge paketin sanallaştırılmayan LocalState klasörüne kopyalanır.
        let local = std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?;
        exe = PathBuf::from(local).join("Microsoft").join("WindowsApps").join(crate::store_update::EXECUTION_ALIAS);
        if let (Some(dir), Some(src)) = (crate::store_update::local_state_dir(), std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("p2.ico")))) {
            let dst = dir.join("p2.ico");
            if std::fs::copy(&src, &dst).is_ok() {
                icon = format!("{},0", dst.to_string_lossy());
            }
        }
    }
    let q = |v: &str| v.replace('\'', "''");
    let icon_line = if icon.is_empty() { String::new() } else { format!("$s.IconLocation='{}';", q(&icon)) };
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path ([Environment]::GetFolderPath('Desktop')) '{}'));$s.TargetPath='{}';$s.Arguments='--p2-play={}';$s.Description='{} — P2';{icon_line}$s.Save()",
        q(&link),
        q(&exe.to_string_lossy()),
        q(id),
        q(name)
    );
    let status = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(0x0800_0000).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(p2_i18n::t("libd.shortcut.failed")) }
}
