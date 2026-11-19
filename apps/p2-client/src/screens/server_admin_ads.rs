//! Sunucu yönetimi › Sponsorlu: uygulama içi sponsorlu kartların yönetimi (`server.ads`). Aç/kapat,
//! toplam gösterim ve tıklama, kart listesi (yayında / durduruldu / planlandı / bitti), yeni kart ve
//! düzenleme formu (canlı önizlemeyle), durdur/yayınla ve sil.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, SharedString, Window, div, px};
use p2_client_core::SponsoredAd;
use p2_i18n::{t, tv};
use p2_ui::P2Text as _;
use p2_ui::tokens::{Palette, font_size, fs, radius};

use super::{Btn, Confirm, ServerAdminPanel, badge, choice, field_input, hint, perm_toggle, settings_block, settings_button, settings_field, stat, stats_grid, text};
use crate::task;

/// Form alanları (sırayla): sponsor, başlık, metin, görsel, bağlantı, düğme, ağırlık, başlangıç, bitiş.
pub(super) const FIELDS: usize = 9;

#[derive(Clone, Default)]
pub(super) struct AdForm {
    /// `None`: yeni kart.
    pub id: Option<String>,
    pub placements: Vec<String>,
    pub active: bool,
}

fn status(ad: &SponsoredAd) -> (&'static str, Btn) {
    let now = chrono::Utc::now();
    let parse = |v: &Option<String>| v.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    if !ad.active {
        ("srv.ads.paused", Btn::Plain)
    } else if parse(&ad.ends_at).is_some_and(|e| e <= now) {
        ("srv.ads.ended", Btn::Plain)
    } else if parse(&ad.starts_at).is_some_and(|s| s > now) {
        ("srv.ads.scheduled", Btn::Plain)
    } else {
        ("srv.ads.live", Btn::Primary)
    }
}

fn ctr(views: u64, clicks: u64) -> String {
    if views == 0 {
        return "—".into();
    }
    let rate = clicks as f64 * 100. / views as f64;
    // Türkçede yüzde işareti önde ve ondalık virgül: %2,5
    if p2_i18n::language().id == "tr" { format!("%{rate:.1}").replace('.', ",") } else { format!("{rate:.1}%") }
}

fn placement_label(p: &str) -> String {
    t(&format!("srv.ads.place.{p}"))
}

impl ServerAdminPanel {
    fn ad_value(&self, i: usize, cx: &Context<Self>) -> String {
        self.ad_fields[i].read(cx).value().trim().to_string()
    }

    pub(super) fn load_ads(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || api.server_sponsored(), |this, r, window, cx| match r {
            Ok(feed) => {
                crate::trace(&format!("sunucu yönetimi: {} sponsorlu kart, açık={}", feed.ads.len(), feed.enabled));
                this.sponsored = Some(feed);
                this.loaded(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
    }

    /// Formu açar: `ad` verilirse düzenleme, yoksa yeni kart.
    pub(super) fn edit_ad(&mut self, ad: Option<&SponsoredAd>, window: &mut Window, cx: &mut Context<Self>) {
        let values: [String; FIELDS] = match ad {
            Some(a) => [
                a.sponsor.clone(),
                a.title.clone(),
                a.body.clone(),
                a.image_url.clone().unwrap_or_default(),
                a.link_url.clone(),
                a.cta.clone(),
                a.weight.max(1).to_string(),
                a.starts_at.as_deref().map(|s| s.chars().take(10).collect()).unwrap_or_default(),
                a.ends_at.as_deref().map(|s| s.chars().take(10).collect()).unwrap_or_default(),
            ],
            None => Default::default(),
        };
        for (state, value) in self.ad_fields.clone().iter().zip(values) {
            state.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        self.ad_form = Some(match ad {
            Some(a) => AdForm { id: Some(a.id.clone()), placements: a.placements.clone(), active: a.active },
            None => AdForm { id: None, placements: p2_client_core::SPONSORED_PLACEMENTS.iter().map(|s| s.to_string()).collect(), active: true },
        });
        cx.notify();
    }

    fn form_body(&self, cx: &Context<Self>) -> serde_json::Value {
        let form = self.ad_form.clone().unwrap_or_default();
        let v = |i: usize| self.ad_value(i, cx);
        let date = |s: String| if s.is_empty() { serde_json::Value::Null } else { serde_json::json!(s) };
        serde_json::json!({
            "sponsor": v(0), "title": v(1), "body": v(2), "imageUrl": v(3), "linkUrl": v(4), "cta": v(5),
            "weight": v(6).parse::<i64>().unwrap_or(1), "startsAt": date(v(7)), "endsAt": date(v(8)),
            "placements": form.placements, "active": form.active,
        })
    }

    pub(super) fn save_ad(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.ad_form.clone() else { return };
        let body = self.form_body(cx);
        let title = self.ad_value(1, cx);
        let notice = tv(if form.id.is_some() { "srv.ads.updated" } else { "srv.ads.created" }, &[("name", &title)]);
        self.ad_form = None;
        self.act(format!("sponsorlu kart kaydedildi «{title}»"), notice, move |api| {
            match &form.id {
                Some(id) => api.server_update_sponsored(id, body),
                None => api.server_create_sponsored(body),
            }
            .map(|_| serde_json::json!({}))
        }, window, cx);
    }

    fn set_ad_active(&mut self, ad: &SponsoredAd, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (id, title) = (ad.id.clone(), ad.title.clone());
        let notice = tv(if on { "srv.ads.resumed" } else { "srv.ads.pausedNotice" }, &[("name", &title)]);
        self.act(format!("sponsorlu kart {} «{title}»", if on { "yayında" } else { "durduruldu" }), notice, move |api| api.server_update_sponsored(&id, serde_json::json!({ "active": on })).map(|_| serde_json::json!({})), window, cx);
    }

    pub(super) fn set_ads_enabled(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.act(format!("sponsorlu içerik {}", if on { "açıldı" } else { "kapatıldı" }), t(if on { "srv.ads.enabledNotice" } else { "srv.ads.disabledNotice" }), move |api| api.server_sponsored_enabled(on).map(|_| serde_json::json!({})), window, cx);
    }

    fn pick_ad_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(t("srv.ads.pickImage").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| {
                        let api = this.api.clone();
                        this.ad_uploading = true;
                        cx.notify();
                        task::run(window, cx, move || super::super::admin::upload_image(&api, &path, true), |this, r, window, cx| {
                            this.ad_uploading = false;
                            match r {
                                Ok(url) => this.ad_fields[3].update(cx, |s, cx| s.set_value(url, window, cx)),
                                Err(e) => crate::toasts::error(e),
                            }
                            cx.notify();
                        });
                    });
                }
            }
        })
        .detach();
    }

    /// Formdan önizleme kartı (kaydetmeden nasıl görüneceği).
    fn preview_ad(&self, cx: &Context<Self>) -> SponsoredAd {
        let v = |i: usize| self.ad_value(i, cx);
        SponsoredAd {
            id: self.ad_form.as_ref().and_then(|f| f.id.clone()).unwrap_or_else(|| "preview".into()),
            sponsor: if v(0).is_empty() { t("srv.ads.f.sponsor") } else { v(0) },
            title: if v(1).is_empty() { t("srv.ads.f.title") } else { v(1) },
            body: v(2),
            image_url: Some(v(3)).filter(|s| !s.is_empty()),
            link_url: v(4),
            cta: v(5),
            ..Default::default()
        }
    }

    fn ad_form_view(&self, width: f32, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let form = self.ad_form.clone().unwrap_or_default();
        let field = |i: usize, label: &str, window: &mut Window, cx: &mut Context<Self>| settings_field(&t(label), field_input(&self.ad_fields[i], self.busy, p, window, cx), p);
        let two = |a: AnyElement, b: AnyElement| div().flex().gap(px(10.)).child(div().flex_1().min_w_0().child(a)).child(div().flex_1().min_w_0().child(b));
        let preview_w = 250.;
        let narrow = width < 640.;
        let mut places = div().flex().flex_col().gap(px(6.));
        for place in p2_client_core::SPONSORED_PLACEMENTS {
            let on = form.placements.iter().any(|x| x == place);
            places = places.child(perm_toggle(SharedString::from(format!("srv-ads-place-{place}")), &placement_label(place), &t(&format!("srv.ads.place.{place}.hint")), on, self.busy, p).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(f) = this.ad_form.as_mut() {
                    if let Some(i) = f.placements.iter().position(|x| x == place) {
                        if f.placements.len() > 1 {
                            f.placements.remove(i);
                        }
                    } else {
                        f.placements.push(place.to_string());
                    }
                }
                cx.notify();
            })));
        }
        let required_ok = !self.ad_value(0, cx).is_empty() && !self.ad_value(1, cx).is_empty() && self.ad_value(4, cx).starts_with("http");
        let fields = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(two(field(0, "srv.ads.f.sponsor", window, cx).into_any_element(), field(5, "srv.ads.f.cta", window, cx).into_any_element()))
            .child(field(1, "srv.ads.f.title", window, cx))
            .child(field(2, "srv.ads.f.body", window, cx))
            .child(field(4, "srv.ads.f.link", window, cx))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(field(3, "srv.ads.f.image", window, cx)))
                    .child(settings_button("srv-ads-upload", if self.ad_uploading { t("srv.ads.uploading") } else { t("srv.ads.upload") }, Btn::Plain, self.busy || self.ad_uploading, p).h(px(34.)).on_click(cx.listener(|this, _, window, cx| this.pick_ad_image(window, cx)))),
            )
            .child(div().flex().gap(px(10.)).child(div().w(px(110.)).child(field(6, "srv.ads.f.weight", window, cx))).child(div().flex_1().min_w_0().child(field(7, "srv.ads.f.starts", window, cx))).child(div().flex_1().min_w_0().child(field(8, "srv.ads.f.ends", window, cx))))
            .child(hint(t("srv.ads.f.hint"), p))
            .child(settings_field(&t("srv.ads.f.places"), places, p))
            .child(perm_toggle("srv-ads-active", &t("srv.ads.f.active"), &t("srv.ads.f.activeHint"), form.active, self.busy, p).on_click(cx.listener(|this, _, _, cx| {
                if let Some(f) = this.ad_form.as_mut() {
                    f.active = !f.active;
                }
                cx.notify();
            })))
            .child(
                div()
                    .mt(px(4.))
                    .flex()
                    .gap(px(8.))
                    .child(settings_button("srv-ads-save", t("srv.save"), Btn::Primary, self.busy || !required_ok, p).on_click(cx.listener(|this, _, window, cx| this.save_ad(window, cx))))
                    .child(settings_button("srv-ads-cancel", t("srv.ads.cancel"), Btn::Plain, self.busy, p).on_click(cx.listener(|this, _, _, cx| {
                        this.ad_form = None;
                        cx.notify();
                    }))),
            );
        let preview = div()
            .flex_none()
            .w(px(preview_w))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(super::tr_upper(&t("srv.ads.preview"))))
            .child(super::super::sponsored::compact_card(&self.preview_ad(cx), preview_w, p));
        let title = if form.id.is_some() { t("srv.ads.editTitle") } else { t("srv.ads.newTitle") };
        settings_block(title, false, p)
            .child(div().flex().when(narrow, |s| s.flex_col()).items_start().gap(px(18.)).child(fields).child(preview))
            .into_any_element()
    }

    fn ad_row(&self, i: usize, ad: &SponsoredAd, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (status_key, kind) = status(ad);
        let places = ad.placements.iter().map(|x| placement_label(x)).collect::<Vec<_>>().join(" · ");
        let last14: (u64, u64) = ad.days.iter().fold((0, 0), |a, d| (a.0 + d.impressions, a.1 + d.clicks));
        let (edit, toggle, delete) = (ad.clone(), ad.clone(), ad.clone());
        div()
            .id(SharedString::from(format!("srv-ad-{i}")))
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(12.))
            .py(px(10.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().flex().items_center().gap(px(8.)).min_w_0().child(text(fs(12.5)).css_weight(750.).text_color(p.text_strong).truncate().child(ad.title.clone())).child(badge(t(status_key), kind, p)))
                    .child(div().text_size(fs(10.5)).text_color(p.text_3).truncate().child(format!("{} · {}", ad.sponsor, places)))
                    .child(div().text_size(fs(10.5)).text_color(p.text_2).child(tv("srv.ads.rowStats", &[("views", &ad.impressions.to_string()), ("clicks", &ad.clicks.to_string()), ("ctr", &ctr(ad.impressions, ad.clicks)), ("views14", &last14.0.to_string()), ("clicks14", &last14.1.to_string())]))),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(6.))
                    .child(settings_button(SharedString::from(format!("srv-ad-edit-{i}")), t("srv.ads.edit"), Btn::Plain, self.busy, p).on_click(cx.listener(move |this, _, window, cx| this.edit_ad(Some(&edit), window, cx))))
                    .child(settings_button(SharedString::from(format!("srv-ad-toggle-{i}")), t(if ad.active { "srv.ads.pause" } else { "srv.ads.resume" }), Btn::Plain, self.busy, p).on_click(cx.listener(move |this, _, window, cx| this.set_ad_active(&toggle, !toggle.active, window, cx))))
                    .child(settings_button(SharedString::from(format!("srv-ad-del-{i}")), t("srv.ads.delete"), Btn::Danger, self.busy, p).on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(Confirm::DeleteAd(delete.clone()));
                        cx.notify();
                    }))),
            )
            .into_any_element()
    }

    pub(super) fn sponsored_view(&self, width: f32, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(feed) = &self.sponsored else { return hint(t("srv.loading"), p).into_any_element() };
        let (views, clicks) = feed.ads.iter().fold((0, 0), |a, ad| (a.0 + ad.impressions, a.1 + ad.clicks));
        let live = feed.ads.iter().filter(|a| a.live).count();
        let enabled = feed.enabled;
        let toggle = |id: &'static str, on: bool, title: &str, body: &str, cx: &mut Context<Self>| {
            choice(id.into(), t(title), t(body), enabled == on, self.busy, p).on_click(cx.listener(move |this, _, window, cx| {
                if !this.busy && this.sponsored.as_ref().is_some_and(|f| f.enabled != on) {
                    this.set_ads_enabled(on, window, cx);
                }
            }))
        };
        let mut list = div().flex().flex_col().gap(px(8.));
        if feed.ads.is_empty() {
            list = list.child(hint(t("srv.ads.empty"), p));
        }
        for (i, ad) in feed.ads.iter().enumerate() {
            list = list.child(self.ad_row(i, ad, p, cx));
        }
        div()
            .flex()
            .flex_col()
            .child(
                settings_block(t("srv.ads.title"), false, p)
                    .child(hint(t("srv.ads.hint"), p))
                    .child(div().flex().gap(px(8.)).child(toggle("srv-ads-on", true, "srv.ads.on", "srv.ads.onHint", cx)).child(toggle("srv-ads-off", false, "srv.ads.off", "srv.ads.offHint", cx))),
            )
            .child(settings_block(t("srv.ads.totals"), false, p).child(stats_grid(
                vec![
                    stat(t("srv.ads.liveCount"), live.to_string(), Some(tv("srv.ads.ofTotal", &[("n", &feed.ads.len().to_string())])), p),
                    stat(t("srv.ads.views"), views.to_string(), None, p),
                    stat(t("srv.ads.clicks"), clicks.to_string(), None, p),
                    stat(t("srv.ads.ctr"), ctr(views, clicks), None, p),
                ],
                width,
            )))
            .when_some(self.ad_form.is_some().then(|| self.ad_form_view(width, p, window, cx)), |s, f| s.child(f))
            .child(
                settings_block(t("srv.ads.list"), true, p)
                    .when(self.ad_form.is_none(), |s| s.child(div().flex().child(settings_button("srv-ads-new", t("srv.ads.new"), Btn::Primary, self.busy, p).on_click(cx.listener(|this, _, window, cx| this.edit_ad(None, window, cx))))))
                    .child(list),
            )
            .into_any_element()
    }
}
