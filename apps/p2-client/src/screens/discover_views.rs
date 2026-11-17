//! Keşfet görünümünün bölümleri: öne çıkanlar, proje ızgarası, haberler (sütun / şerit), ayrıntı
//! çekmecesi ve haber okuma penceresi. Durum `discover.rs`'te (`DiscoverView`).

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, MouseButton, SharedString, div, px, relative};
use p2_client_core::{DiscoverProject, NewsItem};
use p2_ui::controls::lh;
use p2_ui::motion::{EASE_OUT, EASE_POP, M_BASE, M_FAST};
use p2_ui::tokens::{Palette, css_rgba, fs, radius, shadows};
use p2_ui::{Entrance, P2Fx as _, P2Motion as _, P2Text as _, P2Transition as _};

use super::super::discover_ui::{self as ui, Pill, glyph, ico, on_img, t, tv};
use super::{DiscoverEvent, DiscoverView, GAP, Layout};

/// Gönderi metni → paragraflar (dosya işaretleri ve Markdown imleri atılır).
fn paragraphs(content: &str) -> Vec<String> {
    let mut text = String::new();
    let mut rest = content;
    while let Some(start) = rest.find("[[p2-") {
        text.push_str(&rest[..start]);
        match rest[start..].find("]]") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    text.push_str(rest);
    text.lines().map(|l| l.trim().trim_start_matches('#').trim().replace("**", "")).filter(|l| !l.is_empty()).collect()
}

impl DiscoverView {
    fn pill_of(&self, project: &DiscoverProject) -> Pill {
        if self.joining.as_deref() == Some(project.id.as_str()) {
            Pill::Busy
        } else if project.joined {
            Pill::Joined
        } else {
            Pill::Join
        }
    }

    /// Katıl hapı: tıklama kartın tıklamasına (ayrıntı) karışmaz.
    fn pill(&self, key: &str, project: &DiscoverProject, large: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (id, name) = (project.id.clone(), project.name.clone());
        ui::join_pill(SharedString::from(format!("{key}-{}", project.id)), self.pill_of(project), large, p)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.join(id.clone(), name.clone(), window, cx);
            }))
            .into_any_element()
    }

    // ---------------------------------------------------------------- öne çıkanlar

    fn featured_card(&self, slot: usize, project: &DiscoverProject, w: f32, h: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let big = h > 200.;
        let group: SharedString = format!("disc-feat-{slot}").into();
        let (accent, lg) = (p.border_accent, p.shadow_lg());
        let card = project.clone();
        let (face, name_size, pad) = if big { (62., 22., 20.) } else { (44., 15., 14.) };
        let (iw, ih) = (w - 2., h - 2.);
        let blurb = project.blurb().map(str::to_string);
        div()
            .id(SharedString::from(format!("disc-feat-card-{}", project.id)))
            .group(group.clone())
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(radius::LG)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.bg_2)
            .shadow(p.shadow_md())
            .cursor_pointer()
            .transition(M_BASE, EASE_POP)
            .hover(move |s| s.translate(0., -4.).border_color(accent).shadow(lg.clone()))
            .child(div().absolute().top_0().left_0().child(ui::banner(&project.id, project.cover_url.as_deref(), iw, ih, (13., 13.), p)))
            .child(ui::scrim(iw, ih, 1.).absolute().top_0().left_0().rounded(px(13.)))
            // Üst rozetler: kategori (cam) + öne çıkan (vurgu).
            .child(
                div()
                    .absolute()
                    .top(px(pad - 4.))
                    .left(px(pad - 4.))
                    .right(px(pad - 4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(ui::glass_chip(Some(ui::category_glyph(project.category_id())), ui::category_label(project.category_id()).to_uppercase(), false, p))
                    .child(div().flex_1())
                    .when(slot == 0, |s| s.child(ui::glass_chip(Some(glyph::SPARK), t("disc.featured.badge"), true, p)))
                    // Üzerine gelince beliren cam "Göz at" çipi (arkası bulanık).
                    .when(slot != 0 || !big, |s| s.child(div().invisible().group_hover(group.clone(), |s| s.visible()).child(ui::glass_chip(Some(glyph::ARROW), t("disc.peek"), false, p)))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(pad))
                    .right(px(pad))
                    .bottom(px(pad - 2.))
                    .flex()
                    .items_end()
                    .gap(px(if big { 14. } else { 11. }))
                    .child(div().p(px(3.)).rounded(px(face * 0.3 + 3.)).bg(on_img(0.14)).backdrop_blur(p2_ui::fx::BLUR_SM).child(ui::project_face(&project.id, &project.name, project.icon_url.as_deref(), face, face * 0.3, p)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(if big { 5. } else { 3. }))
                            .child(div().text_size(fs(name_size)).line_height(lh(fs(name_size))).css_weight(800.).tracking(-0.03).text_color(on_img(1.)).truncate().child(project.name.clone()))
                            .when_some(blurb, |s, b| s.child(div().text_size(fs(if big { 12. } else { 10.5 })).line_height(relative(1.4)).text_color(on_img(0.78)).map(|s| if big { s.line_clamp(2) } else { s.truncate() }).child(b)))
                            .child(div().mt(px(2.)).child(ui::counts(project.online_count, project.member_count, true, p))),
                    )
                    .child(self.pill("disc-feat-join", project, big, p, cx)),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_detail(card.clone(), window, cx)))
            .p2_enter(SharedString::from(format!("disc-feat-in-{}", project.id)), Entrance::Rise, ui::stagger(slot + 1))
            .into_any_element()
    }

    /// Öne çıkanlar: en kalabalık (afişli) üç proje. Arama/kategori süzgeci varken gizlenir.
    pub(super) fn featured(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.query.is_empty() || self.category.is_some() {
            return None;
        }
        let w = l.main_w;
        let Some(list) = self.list.as_ref() else {
            // İlk yükleme: iskelet.
            let big = (w * 0.58).round();
            let row = if w >= 760. {
                div().flex().gap(px(GAP)).child(ui::skeleton("disc-sk-f0", big, 292., 14., p)).child(div().flex().flex_col().gap(px(GAP)).child(ui::skeleton("disc-sk-f1", w - big - GAP, 139., 14., p)).child(ui::skeleton("disc-sk-f2", w - big - GAP, 139., 14., p)))
            } else {
                div().child(ui::skeleton("disc-sk-f0", w, 240., 14., p))
            };
            return Some(div().flex().flex_col().gap(px(14.)).child(ui::section_head(t("disc.featured.kicker"), t("disc.featured.title"), None, p)).child(row).into_any_element());
        };
        let mut picks: Vec<&DiscoverProject> = list.projects.iter().collect();
        picks.sort_by(|a, b| b.cover_url.is_some().cmp(&a.cover_url.is_some()).then(b.member_count.cmp(&a.member_count)));
        picks.truncate(3);
        if picks.is_empty() {
            return None;
        }
        let row = match (picks.len(), w >= 760.) {
            (3, true) => {
                let big = (w * 0.58).round();
                let side = w - big - GAP;
                div()
                    .flex()
                    .gap(px(GAP))
                    .child(self.featured_card(0, picks[0], big, 292., p, cx))
                    .child(div().flex().flex_col().gap(px(GAP)).child(self.featured_card(1, picks[1], side, 139., p, cx)).child(self.featured_card(2, picks[2], side, 139., p, cx)))
            }
            (n, true) if n >= 2 => {
                let half = ((w - GAP) / 2.).floor();
                div().flex().gap(px(GAP)).child(self.featured_card(0, picks[0], half, 250., p, cx)).child(self.featured_card(1, picks[1], half, 250., p, cx))
            }
            _ => div().child(self.featured_card(0, picks[0], w, 240., p, cx)),
        };
        Some(div().flex().flex_col().gap(px(14.)).child(ui::section_head(t("disc.featured.kicker"), t("disc.featured.title"), None, p)).child(row).into_any_element())
    }

    // ---------------------------------------------------------------- tüm projeler

    fn grid_card(&self, i: usize, project: &DiscoverProject, w: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let group: SharedString = format!("disc-card-{i}").into();
        let card = project.clone();
        let banner_h = 86.;
        let blurb = project.blurb().map(str::to_string).unwrap_or_else(|| t("disc.card.noblurb"));
        ui::card_shell(SharedString::from(format!("disc-grid-{}", project.id)), group.clone(), w, p)
            .child(
                div()
                    .relative()
                    .child(ui::banner(&project.id, project.cover_url.as_deref(), w - 2., banner_h, (13., 0.), p))
                    .child(ui::scrim(w - 2., banner_h, 0.55).absolute().top_0().left_0().rounded_t(px(13.)))
                    .child(div().absolute().top(px(9.)).right(px(9.)).child(ui::glass_chip(Some(ui::category_glyph(project.category_id())), ui::category_label(project.category_id()).to_uppercase(), false, p)))
                    .child(div().absolute().top(px(9.)).left(px(9.)).invisible().group_hover(group.clone(), |s| s.visible()).child(ui::glass_chip(Some(glyph::ARROW), t("disc.peek"), false, p))),
            )
            .child(
                div()
                    .relative()
                    .px(px(14.))
                    .pb(px(13.))
                    .flex()
                    .flex_col()
                    .child(div().mt(px(-24.)).size(px(54.)).flex().items_center().justify_center().rounded(px(17.)).bg(p.bg_2).child(ui::project_face(&project.id, &project.name, project.icon_url.as_deref(), 46., 14., p)))
                    .child(div().mt(px(7.)).text_size(fs(14.)).line_height(lh(fs(14.))).css_weight(750.).tracking(-0.01).text_color(p.text_strong).truncate().child(project.name.clone()))
                    .child(div().mt(px(5.)).h(px(34.)).text_size(fs(11.)).line_height(px(17.)).text_color(p.text_3).line_clamp(2).child(blurb))
                    .child(div().mt(px(11.)).pt(px(11.)).border_t_1().border_color(p.border_subtle).flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().overflow_hidden().child(ui::counts(project.online_count, project.member_count, false, p))).child(self.pill("disc-grid-join", project, false, p, cx))),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_detail(card.clone(), window, cx)))
            .p2_enter(SharedString::from(format!("disc-grid-in-{}", project.id)), Entrance::Rise, ui::stagger(i))
            .into_any_element()
    }

    pub(super) fn grid(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let filtered = !self.query.is_empty() || self.category.is_some();
        let title = if !self.query.is_empty() {
            tv("disc.grid.results", &[("q", &self.query)])
        } else if let Some(category) = self.category.as_deref() {
            ui::category_label(category)
        } else {
            t("disc.grid.title")
        };
        let shown = self.list.as_ref().map_or(0, |x| x.projects.len());
        let kick = if self.list.is_some() { tv("disc.grid.kicker", &[("n", &shown.to_string())]) } else { t("disc.grid.loading") };
        let head = ui::section_head(kick, title, Some(self.sort_control(p, cx)), p);
        let mut wrap = div().w(px(l.main_w)).flex().flex_wrap().gap(px(GAP));
        match self.list.as_ref() {
            None => {
                for i in 0..l.cols * 2 {
                    wrap = wrap.child(ui::skeleton(("disc-sk-g", i), l.card_w, 226., 14., p));
                }
            }
            Some(list) if list.projects.is_empty() => {
                let (title, detail, action) = if filtered {
                    let clear = ui::ghost_button("disc-clear-filters", Some(glyph::X), t("disc.empty.clear"), p).on_click(cx.listener(|this, _, window, cx| this.clear_filters(window, cx)));
                    (t("disc.empty.filtered.title"), t("disc.empty.filtered.body"), Some(clear.into_any_element()))
                } else {
                    let invite = ui::ghost_button("disc-empty-invite", Some(glyph::KEY), t("disc.invite"), p).on_click(cx.listener(|_, _, _, cx| cx.emit(DiscoverEvent::Invite)));
                    (t("disc.empty.title"), t("disc.empty.body"), Some(invite.into_any_element()))
                };
                wrap = wrap.child(ui::empty_state(glyph::COMPASS, title, detail, action, p));
            }
            Some(list) => {
                for (i, project) in list.projects.iter().enumerate() {
                    wrap = wrap.child(self.grid_card(i, project, l.card_w, p, cx));
                }
            }
        }
        div().flex().flex_col().gap(px(14.)).child(head).child(wrap.when(self.loading && self.list.is_some(), |s| s.opacity(0.6))).into_any_element()
    }

    // ---------------------------------------------------------------- haberler

    fn news_meta(&self, item: &NewsItem, p: &Palette) -> gpui_kit::Div {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .child(ui::project_face(&item.project.id, &item.project.name, item.project.icon_url.as_deref(), 16., 5., p))
            .child(div().min_w_0().text_size(fs(10.)).line_height(lh(fs(10.))).css_weight(700.).text_color(p.text_2).truncate().child(item.project.name.clone()))
            .child(div().flex_none().text_size(fs(10.)).text_color(p.text_4).child("·"))
            .child(div().flex_none().text_size(fs(10.)).line_height(lh(fs(10.))).text_color(p.text_3).child(ui::ago(&item.created_at)))
    }

    fn kind_chip(&self, item: &NewsItem, p: &Palette) -> gpui_kit::Div {
        let release = item.kind == "RELEASE";
        ui::glass_chip(Some(if release { glyph::SPARK } else { glyph::NEWS }), t(if release { "disc.news.release" } else { "disc.news.post" }), release, p)
    }

    /// Manşet: kapak + karartma üstünde başlık.
    fn news_lead(&self, item: &NewsItem, w: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let h = (w * 0.5).round().clamp(150., 190.);
        let it = item.clone();
        ui::card_shell(SharedString::from(format!("disc-news-lead-{}", item.id)), "disc-news-lead".into(), w, p)
            .child(
                div()
                    .relative()
                    .child(ui::banner(&item.project.id, item.cover_url.as_deref(), w - 2., h, (13., 0.), p))
                    .child(ui::scrim(w - 2., h, 1.).absolute().top_0().left_0().rounded_t(px(13.)))
                    .child(div().absolute().top(px(10.)).left(px(10.)).child(self.kind_chip(item, p)))
                    .child(div().absolute().left(px(14.)).right(px(14.)).bottom(px(11.)).text_size(fs(15.)).line_height(relative(1.25)).css_weight(800.).tracking(-0.02).text_color(on_img(1.)).line_clamp(2).child(item.title.clone())),
            )
            .child(
                div()
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .when(!item.excerpt.is_empty(), |s| s.child(div().text_size(fs(11.)).line_height(px(17.)).text_color(p.text_3).line_clamp(2).child(item.excerpt.clone())))
                    .child(self.news_meta(item, p)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.reading = Some(it.clone());
                cx.notify();
            }))
            .p2_enter(SharedString::from(format!("disc-news-in-{}", item.id)), Entrance::Rise, ui::stagger(1))
            .into_any_element()
    }

    /// Sıkı satır: küçük kapak + başlık + proje · zaman.
    fn news_row(&self, i: usize, item: &NewsItem, w: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let it = item.clone();
        let (hover, border) = (p.bg_hover, p.border);
        div()
            .id(SharedString::from(format!("disc-news-row-{}", item.id)))
            .w(px(w))
            .p(px(8.))
            .flex()
            .items_center()
            .gap(px(11.))
            .rounded(radius::MD)
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .cursor_pointer()
            .transition(M_FAST, EASE_OUT)
            .hover(move |s| s.bg(hover).border_color(border))
            .child(ui::banner(&item.project.id, item.cover_url.as_deref(), 84., 58., (9., 9.), p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(fs(12.)).line_height(px(17.)).css_weight(700.).text_color(p.text_strong).line_clamp(2).child(item.title.clone()))
                    .child(self.news_meta(item, p)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.reading = Some(it.clone());
                cx.notify();
            }))
            .p2_enter(SharedString::from(format!("disc-news-in-{}", item.id)), Entrance::SlideRight, ui::stagger(i + 1))
            .into_any_element()
    }

    /// Geniş pencere: sağda sürekli görünen haber sütunu (kendi kaydırması).
    pub(super) fn news_column(&self, w: f32, _l: &Layout, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let inner = w - 2. - 20.;
        let mut list = div().w(px(inner)).flex().flex_col().gap(px(4.)).pb(px(14.));
        match self.news.as_ref() {
            None => {
                list = list.child(ui::skeleton("disc-sk-n0", inner, 250., 14., p));
                for i in 1usize..5 {
                    list = list.child(div().mt(px(6.)).child(ui::skeleton(("disc-sk-n", i), inner, 70., 11., p)));
                }
            }
            Some(items) if items.is_empty() => {
                list = list.child(ui::empty_state(glyph::NEWS, t("disc.news.empty.title"), t("disc.news.empty.body"), None, p).py(px(30.)).px(px(16.)));
            }
            Some(items) => {
                for (i, item) in items.iter().enumerate() {
                    list = list.child(if i == 0 { div().mb(px(8.)).child(self.news_lead(item, inner, p, cx)).into_any_element() } else { self.news_row(i, item, inner, p, cx) });
                }
            }
        }
        let count = self.news.as_ref().map_or(0, |n| n.len());
        div()
            .w(px(w))
            .h_full()
            .flex_none()
            .pt(px(20.))
            .pb(px(20.))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .rounded(radius::XL)
                    .border_1()
                    .border_color(p.border)
                    .bg(p.glass_strong.background())
                    .shadow(p.shadow_md())
                    .child(
                        div()
                            .flex_none()
                            .px(px(16.))
                            .pt(px(16.))
                            .pb(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().size(px(30.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).border_1().border_color(p.border).bg(p.accent_soft).child(ico(glyph::NEWS, 16., p.text_accent)))
                            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(4.)).child(ui::kicker(t("disc.news.kicker"), p.text_accent)).child(div().text_size(fs(15.)).line_height(lh(fs(15.))).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(t("disc.news.title"))))
                            .when(count > 0, |s| s.child(div().px(px(8.)).h(px(20.)).flex().items_center().rounded(radius::PILL).bg(p.accent_soft).text_color(p.text_accent).text_size(fs(9.5)).css_weight(800.).child(count.to_string()))),
                    )
                    .child(div().mx(px(16.)).h(px(1.)).flex_none().bg(p.border_subtle))
                    .child(div().id("disc-news-scroll").flex_1().min_h_0().px(px(10.)).pt(px(12.)).overflow_y_scroll().track_scroll(&self.news_scroll).child(list)),
            )
            .p2_enter("disc-news-col", Entrance::SlideRight, Duration::from_millis(60))
            .into_any_element()
    }

    /// Dar pencere: öne çıkanlarla ızgara arasında yatay haber şeridi.
    pub(super) fn news_rail(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items = self.news.as_ref()?;
        if items.is_empty() {
            return None;
        }
        let card_w = 264.;
        let mut row = div().flex().gap(px(GAP)).pb(px(6.)).pt(px(4.));
        for (i, item) in items.iter().take(10).enumerate() {
            let it = item.clone();
            row = row.child(
                ui::card_shell(SharedString::from(format!("disc-rail-{}", item.id)), SharedString::from(format!("disc-rail-{i}")), card_w, p)
                    .child(
                        div()
                            .relative()
                            .child(ui::banner(&item.project.id, item.cover_url.as_deref(), card_w - 2., 116., (13., 0.), p))
                            .child(ui::scrim(card_w - 2., 116., 0.7).absolute().top_0().left_0().rounded_t(px(13.)))
                            .child(div().absolute().top(px(9.)).left(px(9.)).child(self.kind_chip(item, p))),
                    )
                    .child(
                        div()
                            .px(px(13.))
                            .pt(px(10.))
                            .pb(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(div().h(px(36.)).text_size(fs(12.5)).line_height(px(18.)).css_weight(750.).text_color(p.text_strong).line_clamp(2).child(item.title.clone()))
                            .child(div().h(px(32.)).text_size(fs(10.5)).line_height(px(16.)).text_color(p.text_3).line_clamp(2).child(item.excerpt.clone()))
                            .child(self.news_meta(item, p)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.reading = Some(it.clone());
                        cx.notify();
                    }))
                    .p2_enter(SharedString::from(format!("disc-rail-in-{}", item.id)), Entrance::Rise, ui::stagger(i)),
            );
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(ui::section_head(t("disc.news.kicker"), t("disc.news.title"), None, p))
                .child(div().id("disc-news-rail").w(px(l.main_w)).overflow_x_scroll().child(row))
                .into_any_element(),
        )
    }

    // ---------------------------------------------------------------- ayrıntı çekmecesi

    pub(super) fn detail_drawer(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let detail = self.detail.as_ref()?;
        let project = &detail.card;
        let w = 468f32.min(l.page_w - 24.);
        let banner_h = 178.;
        let label = |text: String| div().mb(px(9.)).child(ui::kicker(text, p.text_3));
        let section = || div().w_full().flex().flex_col();

        let (hover, strong) = (css_rgba(8, 14, 28, 0.72), on_img(1.));
        let close = div()
            .id("disc-detail-x")
            .size(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(on_img(0.18))
            .bg(css_rgba(8, 14, 28, 0.5))
            .backdrop_blur(p2_ui::fx::BLUR_SM)
            .cursor_pointer()
            .hover(move |s| s.bg(hover).border_color(strong))
            .child(ico(glyph::X, 15., on_img(0.95)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.detail = None;
                cx.notify();
            }));

        let head = div()
            .relative()
            .flex_none()
            .child(ui::banner(&project.id, project.cover_url.as_deref(), w, banner_h, (0., 0.), p))
            .child(ui::scrim(w, banner_h, 0.85).absolute().top_0().left_0())
            .child(div().absolute().top(px(14.)).left(px(16.)).flex().gap(px(6.)).child(ui::glass_chip(Some(ui::category_glyph(project.category_id())), ui::category_label(project.category_id()).to_uppercase(), false, p)).child(ui::glass_chip(Some(glyph::GLOBE), t("disc.public"), false, p)))
            .child(div().absolute().top(px(12.)).right(px(12.)).child(close));

        let identity = div()
            .flex_none()
            .px(px(22.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .child(div().mt(px(-38.)).relative().size(px(84.)).flex().items_center().justify_center().rounded(px(26.)).bg(p.bg_1).shadow(p.shadow_md()).child(ui::project_face(&project.id, &project.name, project.icon_url.as_deref(), 74., 22., p)))
            .child(div().mt(px(12.)).text_size(fs(22.)).line_height(relative(1.15)).css_weight(800.).tracking(-0.03).text_color(p.text_strong).child(project.name.clone()))
            .when_some(project.tagline.clone().filter(|x| !x.trim().is_empty()), |s, tag| s.child(div().mt(px(5.)).text_size(fs(12.5)).line_height(relative(1.45)).text_color(p.text_2).child(tag)))
            .child(div().mt(px(11.)).child(ui::counts(project.online_count, project.member_count, false, p)));

        let mut body = div().w(px(w - 44.)).flex().flex_col().gap(px(22.)).pb(px(20.));
        if let Some(description) = project.description.clone().filter(|d| !d.trim().is_empty()) {
            body = body.child(section().child(label(t("disc.detail.about"))).child(div().text_size(fs(12.)).line_height(relative(1.6)).text_color(p.text_2).child(description)));
        }
        match detail.data.as_ref() {
            None if detail.error.is_none() => {
                body = body.child(ui::skeleton("disc-sk-d0", w - 44., 120., 12., p)).child(ui::skeleton("disc-sk-d1", w - 44., 56., 12., p));
            }
            None => {}
            Some(data) => {
                // Kanal önizlemesi.
                if !data.channels.is_empty() {
                    let mut rows = div().flex().flex_col().rounded(radius::MD).border_1().border_color(p.border_subtle).bg(p.bg_2).py(px(5.));
                    for channel in data.channels.iter().take(7) {
                        rows = rows.child(
                            div()
                                .h(px(30.))
                                .px(px(12.))
                                .flex()
                                .items_center()
                                .gap(px(9.))
                                .child(ico(ui::channel_glyph(&channel.kind), 14., p.text_3))
                                .child(div().flex_1().min_w_0().text_size(fs(12.)).css_weight(600.).text_color(p.text).truncate().child(channel.name.clone()))
                                .child(div().flex_none().text_size(fs(9.)).css_weight(700.).tracking(0.08).text_color(p.text_4).child(channel.category.clone())),
                        );
                    }
                    if data.channels.len() > 7 {
                        rows = rows.child(div().h(px(26.)).px(px(12.)).flex().items_center().text_size(fs(10.5)).css_weight(650.).text_color(p.text_accent).child(tv("disc.detail.more_channels", &[("n", &(data.channels.len() - 7).to_string())])));
                    }
                    body = body.child(section().child(label(tv("disc.detail.channels", &[("n", &data.channels.len().to_string())]))).child(rows));
                }
                // Üyeler: üst üste binen avatarlar + sahip.
                if !data.members.is_empty() {
                    let mut stack = div().flex().items_center();
                    for (i, member) in data.members.iter().take(9).enumerate() {
                        crate::avatars::remember(&member.id, member.avatar_url.as_deref());
                        stack = stack.child(
                            div()
                                .relative()
                                .when(i > 0, |s| s.ml(px(-9.)))
                                .p(px(2.))
                                .rounded_full()
                                .bg(p.bg_1)
                                .child(p2_ui::components::avatar(&member.id, &member.display_name, 32., p))
                                .when(member.online, |s| s.child(div().absolute().right(px(1.)).bottom(px(1.)).size(px(10.)).rounded_full().border_2().border_color(p.bg_1).bg(p.success))),
                        );
                    }
                    let rest = project.member_count.saturating_sub(data.members.len().min(9) as u64);
                    let names = data.members.iter().take(2).map(|m| m.display_name.as_str()).collect::<Vec<_>>().join(", ");
                    let summary = if rest > 0 { tv("disc.detail.members_more", &[("names", &names), ("n", &rest.to_string())]) } else { names };
                    body = body.child(
                        section().child(label(tv("disc.detail.members", &[("n", &ui::count_text(project.member_count))]))).child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(stack)
                                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(3.)).child(div().text_size(fs(11.5)).css_weight(650.).text_color(p.text).truncate().child(summary)).when_some(data.owner.as_ref(), |s, owner| s.child(div().text_size(fs(10.)).text_color(p.text_3).truncate().child(tv("disc.detail.owner", &[("name", &owner.display_name)]))))),
                        ),
                    );
                }
                // Son haberler.
                if !data.news.is_empty() {
                    let mut rows = div().flex().flex_col().gap(px(2.)).mx(px(-8.));
                    for (i, item) in data.news.iter().take(3).enumerate() {
                        rows = rows.child(self.news_row(100 + i, item, w - 44. + 16., p, cx));
                    }
                    body = body.child(section().child(label(t("disc.detail.news"))).child(rows));
                }
            }
        }

        // Alt çubuk: katıl / git / yasaklı.
        let banned = detail.data.as_ref().is_some_and(|d| d.banned);
        let busy = self.joining.as_deref() == Some(project.id.as_str());
        let (text, on) = if banned {
            (t("disc.detail.banned"), false)
        } else if busy {
            (t("disc.joining"), false)
        } else if project.joined {
            (t("disc.detail.goto"), true)
        } else {
            (t("disc.detail.join"), true)
        };
        let (id, name) = (project.id.clone(), project.name.clone());
        let action = div()
            .id("disc-detail-join")
            .w_full()
            .h(px(44.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(radius::MD)
            .text_size(fs(13.))
            .css_weight(800.)
            .map(|s| {
                if banned {
                    s.bg(p.danger_soft).border_1().border_color(p.danger_border).text_color(p.danger_text)
                } else {
                    s.bg(p.accent_grad.background()).shadow(shadows(&[p.accent_glow_strong, p.inset_top])).text_color(p.text_on_accent)
                }
            })
            .when(!on && !banned, |s| s.opacity(0.65))
            .when(on, |s| s.cursor_pointer().transition(M_FAST, EASE_OUT).hover(|s| s.brightness(1.08).translate(0., -1.)))
            .child(text)
            .when(on, |s| s.child(ico(glyph::ARROW, 15., p.text_on_accent)))
            .on_click(cx.listener(move |this, _, window, cx| {
                if on {
                    this.join(id.clone(), name.clone(), window, cx);
                }
            }));
        let foot = div()
            .flex_none()
            .px(px(22.))
            .pt(px(14.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(9.))
            .border_t_1()
            .border_color(p.border_subtle)
            .bg(p.glass.background())
            .when_some(detail.error.clone(), |s, e| s.child(div().px(px(12.)).py(px(9.)).rounded(radius::MD).border_1().border_color(p.danger_border).bg(p.danger_soft).text_size(fs(11.)).line_height(relative(1.45)).text_color(p.danger_text).child(e)))
            .child(action)
            .child(div().flex().items_center().justify_center().gap(px(6.)).child(ico(if banned { glyph::LOCK } else { glyph::GLOBE }, 11., p.text_4)).child(div().text_size(fs(10.)).text_color(p.text_3).child(t(if banned { "disc.detail.banned.hint" } else if project.joined { "disc.detail.member.hint" } else { "disc.detail.open.hint" }))));

        let drawer = div()
            .id("disc-detail")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(w))
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_l_1()
            .border_color(p.border_strong)
            .bg(p.bg_1)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(head)
            .child(identity)
            .child(div().id("disc-detail-scroll").flex_1().min_h_0().px(px(22.)).overflow_y_scroll().track_scroll(&self.detail_scroll).child(body))
            .child(foot)
            .p2_enter(SharedString::from(format!("disc-detail-in-{}", project.id)), Entrance::SlideRight, Duration::ZERO);

        Some(
            div()
                .id("disc-detail-layer")
                .absolute()
                .inset_0()
                .bg(p.overlay)
                .backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.detail = None;
                    cx.notify();
                }))
                .child(drawer)
                .into_any_element(),
        )
    }

    // ---------------------------------------------------------------- haber okuma penceresi

    pub(super) fn reader(&self, l: &Layout, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let item = self.reading.as_ref()?;
        let w = 640f32.min(l.page_w - 48.);
        let cover_h = 210.;
        let max_h = (l.page_h - 64.).max(360.);
        let mut text = div().w(px(w - 56.)).flex().flex_col().gap(px(11.));
        for line in paragraphs(&item.content) {
            // Başlık gövdenin ilk satırıysa yinelenmesin.
            if line == item.title {
                continue;
            }
            text = text.child(div().text_size(fs(13.)).line_height(relative(1.65)).text_color(p.text_2).child(line));
        }
        let it = item.clone();
        let author = item.author.as_ref().map(|a| a.display_name.clone());
        let (hover, strong) = (css_rgba(8, 14, 28, 0.72), on_img(1.));
        let close = div()
            .id("disc-reader-x")
            .size(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(on_img(0.18))
            .bg(css_rgba(8, 14, 28, 0.5))
            .backdrop_blur(p2_ui::fx::BLUR_SM)
            .cursor_pointer()
            .hover(move |s| s.bg(hover).border_color(strong))
            .child(ico(glyph::X, 15., on_img(0.95)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.reading = None;
                cx.notify();
            }));
        let card = div()
            .id("disc-reader")
            .w(px(w))
            .max_h(px(max_h))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.bg_1)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(ui::banner(&item.project.id, item.cover_url.as_deref(), w - 2., cover_h, (17., 0.), p))
                    .child(ui::scrim(w - 2., cover_h, 1.).absolute().top_0().left_0().rounded_t(px(17.)))
                    .child(div().absolute().top(px(14.)).left(px(16.)).child(self.kind_chip(item, p)))
                    .child(div().absolute().top(px(12.)).right(px(12.)).child(close))
                    .child(div().absolute().left(px(28.)).right(px(28.)).bottom(px(16.)).text_size(fs(22.)).line_height(relative(1.2)).css_weight(800.).tracking(-0.03).text_color(on_img(1.)).line_clamp(3).child(item.title.clone())),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(28.))
                    .pt(px(14.))
                    .pb(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(ui::project_face(&item.project.id, &item.project.name, item.project.icon_url.as_deref(), 30., 9., p))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(div().text_size(fs(12.)).css_weight(750.).text_color(p.text_strong).truncate().child(item.project.name.clone()))
                            .child(div().text_size(fs(10.)).text_color(p.text_3).truncate().child(match author {
                                Some(a) => format!("{a} · {}", ui::ago(&item.created_at)),
                                None => ui::ago(&item.created_at),
                            })),
                    )
                    .child(ui::glass_chip(Some(ui::category_glyph(item.project.category.as_deref().unwrap_or("OTHER"))), ui::category_label(item.project.category.as_deref().filter(|c| !c.is_empty()).unwrap_or("OTHER")).to_uppercase(), false, p).bg(p.accent_soft).border_color(p.border).text_color(p.text_accent)),
            )
            .child(div().id("disc-reader-scroll").flex_1().min_h_0().px(px(28.)).py(px(18.)).overflow_y_scroll().child(text))
            .child(
                div()
                    .flex_none()
                    .px(px(20.))
                    .py(px(14.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(10.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .rounded_b(px(17.))
                    .bg(p.glass.background())
                    .child(ui::ghost_button("disc-reader-close", None, t("disc.close"), p).on_click(cx.listener(|this, _, _, cx| {
                        this.reading = None;
                        cx.notify();
                    })))
                    .child(
                        div()
                            .id("disc-reader-project")
                            .h(px(32.))
                            .px(px(15.))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .rounded(radius::MD)
                            .bg(p.accent_grad.background())
                            .shadow(shadows(&[p.accent_glow, p.inset_top]))
                            .text_color(p.text_on_accent)
                            .text_size(fs(11.5))
                            .css_weight(800.)
                            .cursor_pointer()
                            .hover(|s| s.brightness(1.08))
                            .child(t("disc.reader.project"))
                            .child(ico(glyph::ARROW, 14., p.text_on_accent))
                            .on_click(cx.listener(move |this, _, window, cx| this.open_project_of(&it, window, cx))),
                    ),
            );
        Some(
            div()
                .id("disc-reader-layer")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(p.overlay)
                .backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.reading = None;
                    cx.notify();
                }))
                .child(card.p2_enter(SharedString::from(format!("disc-reader-in-{}", item.id)), Entrance::PopElastic, Duration::ZERO))
                .into_any_element(),
        )
    }
}
