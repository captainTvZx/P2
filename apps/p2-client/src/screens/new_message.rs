//! "Yeni mesaj" penceresi (Ana sayfa › Direkt mesajlar › ＋): arkadaşlar ve proje paylaştığın kişiler
//! arasından seç. Tek kişi → Ana sayfa birebir sohbeti açılır (yoksa kurulur); birden çok kişi →
//! projeye bağlı olmayan Ana sayfa grubu kurulur (docs/arkadaslar.md).
//!
//! Aynı pencere proje içinde de açılır (proje kenar çubuğu › SOHBETLER / GRUPLAR › ＋, [`ProjectScope`]):
//! listede yalnız o projenin üyeleri durur; tek kişi → o projenin sohbeti, birkaç kişi → proje grubu
//! (`ChatGroup.projectId`). Grup kurmak projede "Grup sohbeti oluştur" (`group.create`), birebir yazmak
//! "Bireysel mesaj gönder" (`direct.send`) yetkisi ister; olmayan seçenek kapalıdır.
//!
//! Görünüm: cam pencere (grup oluşturma penceresiyle aynı aile), seçilenler çip olarak üstte,
//! arama, avatar + çevrimiçi durum, seçim kutusu. Metinler `p2-i18n` (`dm.new.*`).

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, FontWeight, MouseButton, SharedString, Subscription, Window, div, px};
use p2_client_core::{Api, ChatGroup, Member};
use p2_i18n::{t, tv};
use p2_ui::P2Fx as _; // backdrop_blur
use p2_ui::P2Text as _;
use p2_ui::controls::lh;
use p2_ui::form::InputKind;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::{text_input, value};
use crate::task;

pub enum NewMessageEvent {
    /// Tek kişi seçildi: çalışma alanı sohbeti açar (Ana sayfa ya da pencerenin projesi).
    Direct(Member),
    /// Birden çok kişi: grup kuruldu (Ana sayfa grubu ya da proje grubu).
    Group(ChatGroup),
    Cancelled,
}

/// Pencere bir projenin içinden açıldı: kişiler o projenin üyeleri, sonuç o projeye ait.
#[derive(Clone)]
pub struct ProjectScope {
    pub id: String,
    pub name: String,
    /// Projedeki yetkilerim: birebir mesaj (`direct.send`) ve grup kurma (`group.create`).
    pub can_direct: bool,
    pub can_group: bool,
}

/// Seçilebilir kişi: arkadaşım mı (yoksa proje paylaştığım biri).
#[derive(Clone)]
pub struct Candidate {
    pub member: Member,
    pub friend: bool,
}

pub struct NewMessageDialog {
    api: Arc<Api>,
    org: String,
    /// Proje içinden açıldıysa kapsam (yoksa Ana sayfa).
    project: Option<ProjectScope>,
    candidates: Vec<Candidate>,
    /// Seçim sırası (tıklama sırası; çipler bu sırayla dizilir).
    selected: Vec<String>,
    search: Entity<InputState>,
    name: Entity<InputState>,
    busy: bool,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<NewMessageEvent> for NewMessageDialog {}

fn lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'İ' => vec!['i'],
            'I' if p2_i18n::language().id == "tr" => vec!['ı'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

fn status_rank(status: &str) -> u8 {
    match status {
        "ONLINE" => 0,
        "BUSY" => 1,
        "AWAY" => 2,
        _ => 3,
    }
}

impl NewMessageDialog {
    pub fn new(api: Arc<Api>, org: String, project: Option<ProjectScope>, candidates: Vec<Candidate>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = text_input(window, cx, &t(if project.is_some() { "dm.new.project.search" } else { "dm.new.search" }));
        let name = text_input(window, cx, &t("dm.new.groupName.placeholder"));
        search.update(cx, |s, cx| s.focus(window, cx));
        let subs = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&name, window, |_, state, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    let v = state.read(cx).value().to_string();
                    if v.chars().count() > 60 {
                        let cut: String = v.chars().take(60).collect();
                        state.update(cx, |s, cx| s.set_value(cut, window, cx));
                    }
                }
                cx.notify();
            }),
        ];
        let mut this = Self { api, org, project, candidates: Vec::new(), selected: Vec::new(), search, name, busy: false, error: None, _subs: subs };
        this.set_candidates(candidates);
        this
    }

    /// Kişi listesini yeniler (proje üyeleri arka planda gelir); arkadaşlar önce, sonra durum ve ad.
    pub fn set_candidates(&mut self, mut candidates: Vec<Candidate>) {
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.member.id.clone()));
        candidates.sort_by(|a, b| {
            b.friend
                .cmp(&a.friend)
                .then_with(|| status_rank(&a.member.status).cmp(&status_rank(&b.member.status)))
                .then_with(|| lower(&a.member.display_name).cmp(&lower(&b.member.display_name)))
        });
        self.selected.retain(|id| candidates.iter().any(|c| &c.member.id == id));
        self.candidates = candidates;
    }

    /// Çevrimiçi durum değişti (gerçek zamanlı).
    pub fn set_status(&mut self, user_id: &str, status: &str) {
        for c in self.candidates.iter_mut().filter(|c| c.member.id == user_id) {
            c.member.status = status.to_string();
        }
    }

    /// Otomatik pilot (test/görüntü): kullanıcı adlarıyla seç; `go` ise onayla.
    pub fn autopilot(&mut self, usernames: &[&str], name: &str, go: bool, window: &mut Window, cx: &mut Context<Self>) {
        for u in usernames {
            if let Some(c) = self.candidates.iter().find(|c| c.member.username == *u) {
                if !self.selected.contains(&c.member.id) {
                    self.selected.push(c.member.id.clone());
                }
            }
        }
        if !name.is_empty() {
            let name = name.to_string();
            self.name.update(cx, |s, cx| s.set_value(name, window, cx));
        }
        crate::trace(&format!("yeni mesaj: {} kişi seçildi", self.selected.len()));
        if go {
            self.confirm(window, cx);
        }
        cx.notify();
    }

    fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.selected.iter().position(|x| x == id) {
            Some(i) => {
                self.selected.remove(i);
            }
            None => {
                // Projede grup kurma yetkim yoksa tek kişi seçilir (yeni seçim öncekinin yerine geçer).
                if self.project.as_ref().is_some_and(|s| !s.can_group) {
                    self.selected.clear();
                }
                self.selected.push(id.to_string())
            }
        }
        self.error = None;
        cx.notify();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() || self.busy {
            return;
        }
        if self.selected.len() == 1 {
            // Projede birebir mesaj yetkim yoksa (yalnız grup kurabiliyorsam) tek kişiyle sohbet açılmaz.
            if self.project.as_ref().is_some_and(|s| !s.can_direct) {
                self.error = Some(t("dm.new.project.noDirect"));
                cx.notify();
                return;
            }
            if let Some(c) = self.candidates.iter().find(|c| c.member.id == self.selected[0]) {
                cx.emit(NewMessageEvent::Direct(c.member.clone()));
            }
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.clone());
        let name = value(&self.name, cx).trim().to_string();
        let ids = self.selected.clone();
        let project = self.project.as_ref().map(|s| s.id.clone());
        let create = move || match &project {
            // Proje grubu: üyeler o projeden, grup projenin kenar çubuğunda durur.
            Some(project) => api.create_group(&org, project, &name, &ids),
            None => api.create_home_group(&org, &name, &ids),
        };
        task::run(window, cx, create, |this, result, _, cx| {
            this.busy = false;
            match result {
                Ok(group) => cx.emit(NewMessageEvent::Group(group)),
                Err(e) => {
                    crate::trace(&format!("yeni mesaj: grup kurulamadı ({})", e.code));
                    this.error = Some(match p2_client_core::friend_error_key(&e.code) {
                        Some(key) => t(key),
                        None if e.message.is_empty() => t("dm.new.failed"),
                        None => e.message,
                    });
                }
            }
            cx.notify();
        });
    }

    fn avatar(member: &Member, size: f32, radius: f32, ring: Option<gpui_kit::Hsla>, p: &Palette) -> gpui_kit::Div {
        let hue = p.hues[p2_ui::components::avatar_hue(&member.id) % 10];
        let face = match crate::avatars::image(&member.id) {
            Some(img) => gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(radius)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
            None => p2_ui::components::initials_label(p2_ui::components::user_initials(&member.display_name)).into_any_element(),
        };
        div()
            .relative()
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius))
            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
            .text_color(hue.fg)
            .text_size(fs((size * 0.34).max(8.)))
            .line_height(px(size))
            .font_weight(FontWeight::EXTRA_BOLD)
            .tracking(-0.02)
            .child(face)
            .when_some(ring, |s, ring| s.child(p2_ui::controls::presence_ring(p2_ui::controls::Presence::from_status(&member.status), ring, p).absolute().right(px(-3.)).bottom(px(-3.))))
    }
}

impl Render for NewMessageDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let count = self.selected.len();
        let needle = lower(value(&self.search, cx).trim().trim_start_matches('@'));
        let visible: Vec<&Candidate> = self.candidates.iter().filter(|c| needle.is_empty() || lower(&c.member.display_name).contains(&needle) || lower(&c.member.username).contains(&needle)).collect();
        let (soft, strong, hover) = (p.accent_soft, p.text_strong, p.bg_hover);

        // Seçilenler: çipler (avatar + ad + ×).
        let mut chips = div().flex().flex_wrap().gap(px(6.));
        for id in &self.selected {
            let Some(c) = self.candidates.iter().find(|c| &c.member.id == id) else { continue };
            let uid = id.clone();
            let (danger_soft, danger_text) = (p.danger_soft, p.danger_text);
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("nm-chip-{id}")))
                    .h(px(26.))
                    .pl(px(3.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(p.border_accent)
                    .bg(p.accent_soft_2)
                    .child(Self::avatar(&c.member, 20., 7., None, &p))
                    .child(div().max_w(px(130.)).truncate().text_size(fs(11.)).line_height(lh(fs(11.))).css_weight(650.).text_color(p.text_strong).child(c.member.display_name.clone()))
                    .child(
                        div()
                            .id(SharedString::from(format!("nm-chip-x-{id}")))
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .text_size(fs(12.))
                            .line_height(px(12.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(danger_soft).text_color(danger_text))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle(&uid, cx))),
                    )
                    .p2_enter(SharedString::from(format!("nm-chip-in-{id}")), Entrance::ScaleIn, std::time::Duration::ZERO),
            );
        }

        // Kişi listesi: "Arkadaşlar" ve "Projelerinden" bölümleri.
        let label = |text: String| div().flex_none().h(px(24.)).px(px(8.)).flex().items_center().text_size(fs(9.5)).css_weight(800.).tracking(0.14).text_color(p.text_3).child(text);
        let mut people = div().id("nm-people").flex_1().min_h(px(120.)).max_h(px(300.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).p(px(4.)).border_1().border_color(p.border_subtle).rounded(radius::MD).bg(p.bg_0);
        let mut last_section: Option<bool> = None;
        for c in &visible {
            if last_section != Some(c.friend) {
                last_section = Some(c.friend);
                let n = visible.iter().filter(|x| x.friend == c.friend).count();
                // Proje içinde tek bölüm: o projenin üyeleri.
                let key = if self.project.is_some() { "dm.new.project.members" } else if c.friend { "dm.new.friends" } else { "dm.new.shared" };
                people = people.child(label(tv(key, &[("n", &n.to_string())])));
            }
            let member = &c.member;
            let on = self.selected.contains(&member.id);
            let id = member.id.clone();
            people = people.child(
                div()
                    .id(SharedString::from(format!("nm-pick-{}", member.id)))
                    .flex_none()
                    .h(px(42.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .map(|s| if on { s.bg(p.accent_soft).text_color(p.text_strong) } else { s.text_color(p.text).hover(move |s| s.bg(hover)) })
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle(&id, cx)))
                    .child(Self::avatar(member, 30., 10., Some(p.bg_0), &p))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().truncate().text_size(fs(12.)).line_height(lh(fs(12.))).css_weight(650.).child(member.display_name.clone()))
                            .child(div().truncate().text_size(fs(10.)).line_height(lh(fs(10.))).text_color(p.text_3).child(format!("@{}", member.username))),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .size(px(16.))
                            .flex_none()
                            .rounded(px(5.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|s| if on { s.border_color(p.accent).bg(p.accent).text_color(gpui_kit::white()).text_size(fs(10.)).line_height(px(14.)).child("✓") } else { s.border_color(p.navy_500) }),
                    ),
            );
        }
        if visible.is_empty() {
            people = people.child(div().py(px(40.)).px(px(16.)).flex().justify_center().text_size(fs(11.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).child(t(if !self.candidates.is_empty() { "dm.new.noMatch" } else if self.project.is_some() { "dm.new.project.empty" } else { "dm.new.empty" })));
        }

        let field = |state: &Entity<InputState>| p2_ui::form::input(state, InputKind::P2, &p, window, cx).h(px(34.)).px(px(10.)).py(px(0.)).text_size(fs(11.5)).font_weight(FontWeight::NORMAL);
        let search_field: AnyElement = field(&self.search).into_any_element();
        let name_field: Option<AnyElement> = (count > 1).then(|| field(&self.name).into_any_element());
        let disabled = self.busy || count == 0;
        let action = if self.busy { t("dm.new.creating") } else if count > 1 { t("dm.new.createGroup") } else { t("dm.new.open") };
        let scope = self.project.clone();
        let hint = match (count, &scope) {
            (0, Some(s)) if !s.can_group => t("dm.new.project.hint.noGroup"),
            (0, _) => t("dm.new.hint.none"),
            (1, Some(s)) if !s.can_direct => t("dm.new.project.noDirect"),
            (1, Some(_)) => t("dm.new.project.hint.one"),
            (1, None) => t("dm.new.hint.one"),
            (n, Some(_)) => tv("dm.new.project.hint.many", &[("n", &n.to_string())]),
            (n, None) => tv("dm.new.hint.many", &[("n", &n.to_string())]),
        };
        let (title_text, subtitle_text) = match &scope {
            Some(s) => (t("dm.new.project.title"), tv("dm.new.project.subtitle", &[("name", &s.name)])),
            None => (t("dm.new.title"), t("dm.new.subtitle")),
        };
        let card = div()
            .id("new-message-card")
            .relative()
            .w(px(460f32.min(f32::from(window.viewport_size().width) - 48.)))
            .max_h(px((f32::from(window.viewport_size().height) * 0.82).min(640.)))
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(16.))
            .px(px(16.))
            .pb(px(14.))
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(fs(14.)).line_height(lh(fs(14.))).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title_text))
                            .child(div().mt(px(2.)).text_size(fs(10.5)).line_height(lh(fs(10.5))).text_color(p.text_3).child(subtitle_text)),
                    )
                    .child(
                        div()
                            .id("new-message-close")
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XS)
                            .text_color(p.text_3)
                            .text_size(fs(19.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(NewMessageEvent::Cancelled))),
                    ),
            )
            .when(count > 0, |s| s.child(chips))
            .child(search_field)
            .child(people)
            .when_some(name_field, |s, f| s.child(div().flex().flex_col().gap(px(5.)).text_size(fs(10.)).line_height(lh(fs(10.))).css_weight(650.).text_color(p.text_3).child(t("dm.new.groupName")).child(f)))
            .when_some(self.error.clone(), |s, e| s.child(div().text_size(fs(10.5)).text_color(p.danger_text).child(e)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(div().mr_auto().min_w_0().truncate().text_size(fs(10.)).line_height(lh(fs(10.))).text_color(p.text_3).child(hint))
                    .child(
                        div()
                            .id("new-message-cancel")
                            .h(px(36.))
                            .px(px(16.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .border_1()
                            .border_color(p.border)
                            .rounded(radius::SM)
                            .text_color(p.text_2)
                            .text_size(fs(12.))
                            .css_weight(650.)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover).text_color(strong))
                            .child(t("dm.new.cancel"))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(NewMessageEvent::Cancelled))),
                    )
                    .child(
                        div()
                            .id("new-message-go")
                            .h(px(36.))
                            .px(px(18.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .border_1()
                            .border_color(gpui_kit::transparent_black())
                            .rounded(radius::SM)
                            .bg(p.accent_grad.background())
                            .text_color(p.text_on_accent)
                            .text_size(fs(12.))
                            .font_weight(FontWeight::BOLD)
                            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                            .map(|s| if disabled { s.opacity(0.55) } else { s.cursor_pointer() })
                            .child(action)
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            )
            .p2_enter("new-message", Entrance::PopElastic, std::time::Duration::ZERO);

        div()
            .id("new-message-scrim")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .pb(px(24.))
            .bg(p.overlay)
            .backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(NewMessageEvent::Cancelled)))
            .child(card)
    }
}
