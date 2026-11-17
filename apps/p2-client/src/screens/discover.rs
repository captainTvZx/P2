//! Keşfet: herkese açık projeler ve haberleri tek ekranda. Solda arama + kategori çipli tanıtım alanı,
//! öne çıkan projeler (büyük afiş kartları) ve tüm açık projelerin ızgarası; sağda sürekli görünen
//! "Haberler" sütunu (dar pencerede yatay şerit). Karta tıklayınca sağdan ayrıntı çekmecesi açılır
//! (afiş, açıklama, kanal önizlemesi, üyeler, son haberler, "Katıl"); habere tıklayınca okuma penceresi.
//!
//! Veri sunucudan gelir (`/discover/...`): açık projeye davet kodu olmadan tek tıkla katılınır, özel
//! projeler hiç listelenmez. Başlangıç ekranı (`onboarding.rs`) ve çalışma alanı (raydaki pusula,
//! "＋" menüsü) aynı görünümü barındırır; `DiscoverEvent` ile kapanma / katılma bildirilir.
//!
//! Otomatik pilot (`P2_AUTOPILOT_DISCOVER`, `;` ile ayrılmış adımlar): `search:<metin>`, `category:<KİMLİK|all>`,
//! `sort:<popular|new|active>`, `open:<ad>`, `join`, `news:<sıra>`, `back`, `scroll:<px>`, `close`.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, Focusable as _, MouseButton, ScrollHandle, SharedString, Subscription, Window, div, point, px, relative};
use p2_client_core::{Api, ApiError, DiscoverDetail, DiscoverList, DiscoverProject, JoinResult, NewsItem, community_error_text};
use p2_ui::components::radial_layer;
use p2_ui::controls::lh;
use p2_ui::motion::{EASE_OUT, M_FAST};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius, shadows};
use p2_ui::{Entrance, P2Fx as _, P2Motion as _, P2Text as _, P2Transition as _};

use super::discover_ui::{self as ui, glyph, ico, t, tv};
use crate::task;

#[path = "discover_views.rs"]
mod views;

pub enum DiscoverEvent {
    Close,
    /// Açık projeye katılındı (ya da zaten üye olunan projeye gidiliyor).
    Joined(JoinResult),
    /// "Davet kodum var": barındıran ekran kodla katılma adımını açar.
    Invite,
}

/// Görünümü barındıran ekran (kapat düğmesi ve sol boşluk buna göre).
#[derive(Clone, Copy, PartialEq)]
pub enum Host {
    Onboarding,
    /// Çalışma alanı: proje rayı (72px) görünür kalır.
    Workspace,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Sort {
    Popular,
    New,
    Active,
}

impl Sort {
    fn id(self) -> &'static str {
        match self {
            Sort::Popular => "popular",
            Sort::New => "new",
            Sort::Active => "active",
        }
    }
}

/// Ayrıntı çekmecesi: kart verisi hemen görünür, ayrıntı gelince dolar.
pub(super) struct Detail {
    pub card: DiscoverProject,
    pub data: Option<DiscoverDetail>,
    pub error: Option<String>,
}

/// Pencere genişliğine göre düzen.
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub page_w: f32,
    pub page_h: f32,
    pub pad: f32,
    /// Sağdaki haber sütunu (geniş pencerede).
    pub news_w: Option<f32>,
    pub main_w: f32,
    pub cols: usize,
    pub card_w: f32,
}

pub(super) const GAP: f32 = 14.;
const TOPBAR_H: f32 = 52.;
const HERO_H: f32 = 238.;

impl Layout {
    fn new(page_w: f32, page_h: f32) -> Self {
        let pad = if page_w >= 1400. { 34. } else if page_w >= 1040. { 26. } else { 18. };
        let news_w = (page_w >= 1120.).then_some(if page_w >= 1500. { 372. } else { 328. });
        // Ana sütun kaydırma çubuğu için 8px pay bırakır.
        let main_w = (page_w - 2. * pad - news_w.map_or(0., |w| w + 22.) - 8.).max(320.);
        let cols = if main_w >= 1180. { 4 } else if main_w >= 760. { 3 } else if main_w >= 500. { 2 } else { 1 };
        let card_w = ((main_w - GAP * (cols as f32 - 1.)) / cols as f32).floor();
        Self { page_w, page_h, pad, news_w, main_w, cols, card_w }
    }
}

pub struct DiscoverView {
    api: Arc<Api>,
    host: Host,
    search: Entity<InputState>,
    query: String,
    category: Option<String>,
    sort: Sort,
    list: Option<DiscoverList>,
    loading: bool,
    error: Option<String>,
    news: Option<Vec<NewsItem>>,
    detail: Option<Detail>,
    reading: Option<NewsItem>,
    /// Katılma isteği süren proje.
    joining: Option<String>,
    /// Liste isteği sırası (geç gelen eski yanıt yok sayılır).
    seq: u64,
    typing: u64,
    scroll: ScrollHandle,
    news_scroll: ScrollHandle,
    detail_scroll: ScrollHandle,
    steps: Vec<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<DiscoverEvent> for DiscoverView {}

fn error_text(e: &ApiError) -> String {
    community_error_text(&e.code).map(str::to_string).unwrap_or_else(|| if e.message.is_empty() { t("disc.error.generic") } else { e.message.clone() })
}

impl DiscoverView {
    pub fn new(api: Arc<Api>, host: Host, window: &mut Window, cx: &mut Context<Self>) -> Self {
        super::discover_images::install(api.clone());
        let placeholder = t("disc.search.placeholder");
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subs = vec![cx.subscribe_in(&search, window, |this, state, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                if value.chars().count() > 60 {
                    let cut: String = value.chars().take(60).collect();
                    state.update(cx, |s, cx| s.set_value(cut, window, cx));
                    return;
                }
                if value.trim() != this.query {
                    this.query = value.trim().to_string();
                    // Yazarken her tuşta istek atma: 240 ms durunca ara.
                    this.typing += 1;
                    let n = this.typing;
                    cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor().timer(Duration::from_millis(240)).await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            if this.typing == n {
                                this.load(window, cx);
                            }
                        });
                    })
                    .detach();
                    cx.notify();
                }
            }
        })];
        let steps: Vec<String> = std::env::var("P2_AUTOPILOT_DISCOVER").map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect()).unwrap_or_default();
        let mut view = Self {
            api,
            host,
            search,
            query: String::new(),
            category: None,
            sort: Sort::Popular,
            list: None,
            loading: false,
            error: None,
            news: None,
            detail: None,
            reading: None,
            joining: None,
            seq: 0,
            typing: 0,
            scroll: ScrollHandle::new(),
            news_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            steps,
            _subs: subs,
        };
        crate::trace("keşfet: açıldı");
        view.load(window, cx);
        view.load_news(window, cx);
        view.step_later(window, cx);
        view
    }

    // ---------------------------------------------------------------- veri

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.seq += 1;
        let seq = self.seq;
        self.loading = true;
        self.error = None;
        cx.notify();
        let (api, query, category, sort) = (self.api.clone(), self.query.clone(), self.category.clone(), self.sort);
        task::run(window, cx, move || api.discover_projects(&query, category.as_deref(), sort.id()), move |this, r, _, cx| {
            if this.seq != seq {
                return;
            }
            this.loading = false;
            match r {
                Ok(list) => {
                    crate::trace(&format!(
                        "keşfet: {} proje (toplam {}; q=\"{}\" kategori={} sıra={}) [{}]",
                        list.projects.len(),
                        list.total,
                        this.query,
                        this.category.as_deref().unwrap_or("hepsi"),
                        this.sort.id(),
                        list.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                    this.list = Some(list);
                }
                Err(e) => {
                    crate::trace(&format!("keşfet: hata {}", e.code));
                    this.error = Some(error_text(&e));
                }
            }
            cx.notify();
        });
    }

    fn load_news(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || api.discover_news(24), |this, r, _, cx| {
            let items = r.unwrap_or_default();
            crate::trace(&format!("keşfet: {} haber", items.len()));
            this.news = Some(items);
            cx.notify();
        });
    }

    pub(super) fn set_category(&mut self, category: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.category != category {
            self.category = category;
            self.load(window, cx);
        }
    }

    pub(super) fn set_sort(&mut self, sort: Sort, window: &mut Window, cx: &mut Context<Self>) {
        if self.sort != sort {
            self.sort = sort;
            self.load(window, cx);
        }
    }

    /// Arama metnini doğrudan ayarla (temizle düğmesi, otomatik pilot): girdinin programla değişmesi
    /// `Change` olayı üretmediği için liste burada yenilenir.
    fn set_query(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let value = text.to_string();
        self.search.update(cx, |s, cx| s.set_value(value, window, cx));
        self.typing += 1;
        if self.query != text.trim() {
            self.query = text.trim().to_string();
            self.load(window, cx);
        }
    }

    pub(super) fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.category = None;
        self.query.clear();
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.load(window, cx);
    }

    pub(super) fn open_detail(&mut self, card: DiscoverProject, window: &mut Window, cx: &mut Context<Self>) {
        let id = card.id.clone();
        self.reading = None;
        self.detail = Some(Detail { card, data: None, error: None });
        self.detail_scroll = ScrollHandle::new();
        cx.notify();
        let api = self.api.clone();
        let wanted = id.clone();
        task::run(window, cx, move || api.discover_project(&id), move |this, r, _, cx| {
            let Some(detail) = this.detail.as_mut().filter(|d| d.card.id == wanted) else { return };
            match r {
                Ok(data) => {
                    crate::trace(&format!("keşfet: ayrıntı {} ({} üye, {} kanal{})", data.project.name, data.project.member_count, data.channels.len(), if data.banned { ", yasaklı" } else { "" }));
                    detail.card = data.project.clone();
                    detail.data = Some(data);
                }
                Err(e) => {
                    crate::trace(&format!("keşfet: ayrıntı hatası {}", e.code));
                    detail.error = Some(if e.code == "PROJECT_NOT_FOUND" { t("disc.error.gone") } else { error_text(&e) });
                }
            }
            cx.notify();
        });
    }

    /// Haberin projesini ayrıntı çekmecesinde aç (liste gelmemişse haberdeki özetle).
    pub(super) fn open_project_of(&mut self, item: &NewsItem, window: &mut Window, cx: &mut Context<Self>) {
        let card = self.list.as_ref().and_then(|l| l.projects.iter().find(|p| p.id == item.project.id)).cloned().unwrap_or_else(|| DiscoverProject {
            id: item.project.id.clone(),
            name: item.project.name.clone(),
            icon_url: item.project.icon_url.clone(),
            cover_url: item.project.cover_url.clone(),
            category: item.project.category.clone(),
            joined: item.project.joined,
            ..Default::default()
        });
        self.open_detail(card, window, cx);
    }

    /// Açık projeye kodsuz katıl (üyeysem aynı uç projeye götürür).
    pub(super) fn join(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.joining.is_some() {
            return;
        }
        self.joining = Some(id.clone());
        cx.notify();
        let api = self.api.clone();
        let project = id.clone();
        task::run(window, cx, move || api.discover_join(&project), move |this, r, _, cx| {
            this.joining = None;
            match r {
                Ok(joined) => {
                    crate::trace(&format!("keşfet: {} {}", if joined.joined { "katıldın" } else { "zaten üyesin" }, joined.project.name));
                    if let Some(list) = this.list.as_mut() {
                        for p in list.projects.iter_mut().filter(|p| p.id == id) {
                            p.joined = true;
                        }
                    }
                    cx.emit(DiscoverEvent::Joined(joined));
                }
                Err(e) => {
                    crate::trace(&format!("keşfet: hata {} ({name})", e.code));
                    let text = if e.code == "PROJECT_NOT_FOUND" { t("disc.error.gone") } else { error_text(&e) };
                    if let Some(detail) = this.detail.as_mut().filter(|d| d.card.id == id) {
                        detail.error = Some(text.clone());
                        if e.code == "PROJECT_BANNED" && let Some(data) = detail.data.as_mut() {
                            data.banned = true;
                        }
                    }
                    // Bildirimler sağ üstte (toasts.rs); çekmecedeki satır durumu gösterir.
                    crate::toasts::error(text);
                }
            }
            cx.notify();
        });
    }

    /// Esc: önce okuma penceresi, sonra çekmece, sonra ekran. Bir şey kapandıysa `true`.
    pub fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.reading.take().is_some() || self.detail.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    // ---------------------------------------------------------------- otomatik pilot

    fn step_later(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.steps.is_empty() {
            return;
        }
        let ms = std::env::var("P2_AUTOPILOT_DISCOVER_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(1500u64);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(ms)).await;
            let _ = this.update_in(cx, |this, window, cx| this.next_step(window, cx));
        })
        .detach();
    }

    fn next_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Süren bir yükleme varsa bitmesini bekle.
        let busy = self.loading || self.list.is_none() || self.joining.is_some() || self.detail.as_ref().is_some_and(|d| d.data.is_none() && d.error.is_none());
        if busy {
            return self.step_later(window, cx);
        }
        let Some(step) = self.steps.pop() else { return };
        crate::trace(&format!("keşfet: adım {step}"));
        let (name, arg) = step.split_once(':').map(|(a, b)| (a, b.to_string())).unwrap_or((step.as_str(), String::new()));
        match name {
            "search" => self.set_query(&arg, window, cx),
            "category" => self.set_category(Some(arg.to_uppercase()).filter(|c| c != "ALL"), window, cx),
            "sort" => self.set_sort(match arg.as_str() { "new" => Sort::New, "active" => Sort::Active, _ => Sort::Popular }, window, cx),
            "open" => {
                let hit = self.list.as_ref().and_then(|l| l.projects.iter().find(|p| p.name.contains(&arg))).cloned();
                match hit {
                    Some(card) => self.open_detail(card, window, cx),
                    None => crate::trace(&format!("keşfet: listede yok {arg}")),
                }
            }
            "join" => {
                if let Some((id, name)) = self.detail.as_ref().map(|d| (d.card.id.clone(), d.card.name.clone())) {
                    self.join(id, name, window, cx);
                }
            }
            "news" => {
                let index: usize = arg.parse().unwrap_or(0);
                if let Some(item) = self.news.as_ref().and_then(|n| n.get(index)).cloned() {
                    crate::trace(&format!("keşfet: haber {}", item.title));
                    self.detail = None;
                    self.reading = Some(item);
                }
            }
            "back" => {
                self.escape(cx);
            }
            "scroll" => {
                let y: f32 = arg.parse().unwrap_or(0.);
                self.scroll.set_offset(point(px(0.), px(-y)));
            }
            "close" => cx.emit(DiscoverEvent::Close),
            _ => {}
        }
        cx.notify();
        self.step_later(window, cx);
    }

    // ---------------------------------------------------------------- görünüm: üst çubuk ve tanıtım alanı

    fn topbar(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (icon, label) = if self.host == Host::Onboarding { (glyph::BACK, t("disc.back")) } else { (glyph::X, t("disc.close")) };
        div()
            .w_full()
            .h(px(TOPBAR_H))
            .flex_none()
            .px(px(l.pad))
            .flex()
            .items_center()
            .gap(px(12.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.glass.background())
            .child(ui::ghost_button("disc-close", Some(icon), label, p).on_click(cx.listener(|_, _, _, cx| cx.emit(DiscoverEvent::Close))))
            .child(div().w(px(1.)).h(px(22.)).bg(p.border_subtle))
            .child(div().size(px(30.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).border_1().border_color(p.border).bg(p.accent_soft).child(ico(glyph::COMPASS, 17., p.text_accent)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(ui::kicker("P2", p.text_accent))
                    .child(div().text_size(fs(13.5)).line_height(lh(fs(13.5))).css_weight(750.).text_color(p.text_strong).child(t("disc.title"))),
            )
            .child(div().flex_1())
            .when(l.page_w >= 760., |s| s.child(div().text_size(fs(10.5)).text_color(p.text_3).child(t("disc.invite.hint"))))
            .child(ui::ghost_button("disc-invite", Some(glyph::KEY), t("disc.invite"), p).on_click(cx.listener(|_, _, _, cx| cx.emit(DiscoverEvent::Invite))))
            .into_any_element()
    }

    fn hero(&self, l: &Layout, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = l.main_w;
        let focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        let total = self.list.as_ref().map_or(0, |x| x.total);
        let online: u64 = self.list.as_ref().map_or(0, |x| x.online);
        let news = self.news.as_ref().map_or(0, |n| n.len() as u64);

        // Zemin: lacivert gradyan + sağ üstte vurgu parıltısı + ince halkalar.
        let glow = |at: (f32, f32), c: gpui_kit::Hsla, stop: f32| radial_layer(w, HERO_H, at, &[(c, 0.), (gpui_kit::Hsla { a: 0., ..c }, stop)]).absolute().top_0().left_0();
        let ring = |size: f32, right: f32, top: f32, c: gpui_kit::Hsla| div().absolute().size(px(size)).right(px(right)).top(px(top)).rounded_full().border_1().border_color(c);

        let stat = |value: String, label: String| {
            div()
                .min_w(px(86.))
                .px(px(13.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .rounded(radius::MD)
                .border_1()
                .border_color(p.border)
                .bg(p.glass.background())
                .backdrop_blur(p2_ui::fx::BLUR_SM)
                .shadow(shadows(&[p.inset_top]))
                .child(div().text_size(fs(19.)).line_height(lh(fs(19.))).css_weight(800.).tracking(-0.03).text_color(p.text_strong).child(value))
                .child(ui::kicker(label, p.text_3))
        };

        let has_query = !self.query.is_empty();
        let search_w = (w - 56.).min(540.);
        let search = div()
            .w(px(search_w))
            .h(px(44.))
            .pl(px(14.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(12.))
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border_strong })
            .bg(p.bg_3)
            .shadow(if focused { shadows(&[p.ring, p.inset_top]) } else { shadows(&[p.inset_top]) })
            .child(ico(glyph::SEARCH, 16., if focused { p.text_accent } else { p.text_3 }))
            .child(Input::new(&self.search).appearance(false).bordered(false).focus_bordered(false).p(px(0.)).text_size(fs(13.)).flex_1())
            .map(|s| {
                if has_query {
                    let (hover, strong) = (p.bg_hover, p.text_strong);
                    s.child(
                        div()
                            .id("disc-clear")
                            .size(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover).text_color(strong))
                            .child(ico(glyph::X, 14., p.text_3))
                            .on_click(cx.listener(|this, _, window, cx| this.set_query("", window, cx))),
                    )
                } else {
                    s.child(div().mr(px(6.)).flex_none().text_size(fs(10.)).css_weight(650.).text_color(p.text_4).child(tv("disc.search.count", &[("n", &total.to_string())])))
                }
            });

        // Kategori çipleri: Tümü + sunucudaki kategoriler (sayılarıyla).
        let counts = self.list.as_ref().map(|x| x.categories.clone()).unwrap_or_default();
        // Dar pencerede çipler yatay rayda kayar (altta kaydırma çubuğu; bkz. `p2_ui::components::h_rail`).
        let mut chips = p2_ui::components::h_rail("disc-chips").w(px(w - 56.)).items_center().gap(px(8.));
        chips = chips.child(ui::category_chip("disc-cat-all", ui::category_glyph("ALL"), ui::category_label("ALL"), self.list.as_ref().map(|x| x.total), self.category.is_none(), p).on_click(cx.listener(|this, _, window, cx| this.set_category(None, window, cx))));
        for id in p2_client_core::DISCOVER_CATEGORIES {
            let active = self.category.as_deref() == Some(*id);
            let count = counts.get(*id).copied();
            if count.is_none() && !active && self.list.is_some() {
                continue;
            }
            chips = chips.child(ui::category_chip(SharedString::from(format!("disc-cat-{id}")), ui::category_glyph(id), ui::category_label(id), count, active, p).on_click(cx.listener(move |this, _, window, cx| {
                let next = if this.category.as_deref() == Some(*id) { None } else { Some(id.to_string()) };
                this.set_category(next, window, cx);
            })));
        }

        div()
            .relative()
            .w(px(w))
            .h(px(HERO_H))
            .flex_none()
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(gpui_kit::linear_gradient(112., gpui_kit::linear_color_stop(p.blue_900, 0.), gpui_kit::linear_color_stop(p.bg_1, 1.)))
            .shadow(p.shadow_md())
            .child(glow((0.8, 0.3), p.accent_soft_2, 0.3))
            .child(glow((0.2, 0.75), p.accent_soft, 0.26))
            .child(ring(340., -120., -150., p.border_strong))
            .child(ring(220., -60., -90., p.border))
            .child(ring(120., -10., -40., p.border_subtle))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(px(28.))
                    .pt(px(24.))
                    // Alt dolgu 22px; bunun 14px'i çip rayının kaydırma çubuğu oluğu (`RAIL_GUTTER`).
                    .pb(px(22. - p2_ui::fx::RAIL_GUTTER))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().flex().items_center().gap(px(7.)).child(ico(glyph::COMPASS, 13., p.text_accent)).child(ui::kicker(t("disc.hero.kicker"), p.text_accent)))
                                    .child(div().mt(px(9.)).text_size(fs(if w >= 640. { 31. } else { 24. })).line_height(relative(1.1)).css_weight(750.).tracking(-0.04).text_color(p.text_strong).child(t("disc.hero.title")))
                                    .child(div().mt(px(7.)).max_w(px(520.)).text_size(fs(12.5)).line_height(relative(1.5)).text_color(p.text_3).truncate().child(t("disc.hero.subtitle"))),
                            )
                            .when(w >= 720., |s| {
                                s.child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .gap(px(8.))
                                        .child(stat(ui::count_text(total), t("disc.stat.projects")).p2_enter("disc-stat-0", Entrance::Rise, ui::stagger(1)))
                                        .child(stat(ui::count_text(online), t("disc.stat.online")).p2_enter("disc-stat-1", Entrance::Rise, ui::stagger(2)))
                                        .child(stat(ui::count_text(news), t("disc.stat.news")).p2_enter("disc-stat-2", Entrance::Rise, ui::stagger(3))),
                                )
                            }),
                    )
                    .child(div().flex_1())
                    .child(search)
                    .child(div().mt(px(12.)).child(chips)),
            )
            .p2_enter("disc-hero", Entrance::Rise, Duration::ZERO)
            .into_any_element()
    }

    /// Sıralama seçici (Popüler / Yeni / Etkin).
    pub(super) fn sort_control(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div().h(px(30.)).p(px(2.)).flex_none().flex().items_center().gap(px(2.)).rounded(radius::PILL).border_1().border_color(p.border).bg(p.glass.background());
        for (sort, key) in [(Sort::Popular, "disc.sort.popular"), (Sort::New, "disc.sort.new"), (Sort::Active, "disc.sort.active")] {
            let on = self.sort == sort;
            let (strong, hover) = (p.text_strong, p.bg_hover);
            row = row.child(
                div()
                    .id(SharedString::from(format!("disc-sort-{}", sort.id())))
                    .h(px(24.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .rounded(radius::PILL)
                    .text_size(fs(10.5))
                    .line_height(lh(fs(10.5)))
                    .css_weight(700.)
                    .cursor_pointer()
                    .transition(M_FAST, EASE_OUT)
                    .map(|s| if on { s.bg(p.accent_soft_2).text_color(p.text_strong).shadow(shadows(&[p.inset_top])) } else { s.text_color(p.text_3).hover(move |s| s.bg(hover).text_color(strong)) })
                    .child(t(key))
                    .on_click(cx.listener(move |this, _, window, cx| this.set_sort(sort, window, cx))),
            );
        }
        row.into_any_element()
    }
}

impl Render for DiscoverView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let vp = window.viewport_size();
        let inset = if self.host == Host::Workspace { 72. } else { 0. };
        let l = Layout::new(f32::from(vp.width) - inset, f32::from(vp.height) - crate::titlebar::HEIGHT);

        // Sayfa zemini: bg-0 + iki yumuşak vurgu bulutu (başlangıç ekranıyla aynı dil).
        let cloud = |at: (f32, f32), c: gpui_kit::Hsla, stop: f32| radial_layer(l.page_w, l.page_h, at, &[(c, 0.), (gpui_kit::Hsla { a: 0., ..c }, stop)]).absolute().top_0().left_0();

        let mut main = div().w(px(l.main_w)).flex().flex_col().gap(px(26.)).pt(px(20.)).pb(px(44.));
        main = main.child(self.hero(&l, &p, window, cx));
        if let Some(error) = self.error.clone() {
            let retry = ui::ghost_button("disc-retry", None, t("disc.retry"), &p).on_click(cx.listener(|this, _, window, cx| {
                this.load(window, cx);
                this.load_news(window, cx);
            }));
            main = main.child(ui::empty_state(glyph::GLOBE, t("disc.error.title"), error, Some(retry.into_any_element()), &p));
        } else {
            main = main.children(self.featured(&l, &p, cx));
            if l.news_w.is_none() {
                main = main.children(self.news_rail(&l, &p, cx));
            }
            main = main.child(self.grid(&l, &p, cx));
        }

        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .gap(px(22.))
            .pl(px(l.pad))
            .pr(px(l.pad))
            .child(div().id("disc-main").flex_1().min_w_0().h_full().overflow_y_scroll().track_scroll(&self.scroll).child(main))
            .children(l.news_w.map(|w| self.news_column(w, &l, &p, cx)));

        div()
            .id("discover")
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .left(px(inset))
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .text_color(p.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, ev: &gpui_kit::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    if !this.escape(cx) {
                        cx.emit(DiscoverEvent::Close);
                    }
                    cx.stop_propagation();
                }
            }))
            .child(cloud((0.86, 0.9), p.accent_soft, 0.34))
            .child(cloud((0.12, 0.05), p.accent_soft_2, 0.3))
            .child(self.topbar(&l, &p, cx))
            .child(body)
            .children(self.detail_drawer(&l, &p, cx))
            .children(self.reader(&l, &p, cx))
            .p2_enter("discover-page", Entrance::Page, Duration::ZERO)
    }
}
