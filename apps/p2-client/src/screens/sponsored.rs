//! Sponsorlu kartlar (uygulama içi reklam). Sunucu yöneticisinin yayınladığı sade kartlar, rahatsız
//! etmeyen üç yerde en çok birer tane görünür: Keşfet ızgarası (proje kartlarının arasında), Keşfet
//! haber sütunu (alt sıralarda) ve Kütüphane kenar çubuğunun altı. Açılır pencere, ses, video,
//! hareketli dikkat çekme ya da kişisel izleme yoktur; her kartta "Sponsorlu" yazar ve kişi kartı
//! 7 gün gizleyebilir (`sponsored-hidden.json`, profil klasöründe).
//!
//! Akış tek istekle alınır (`GET /sponsored`), 15 dakikada bir yenilenir. Bir yerde hangi kartın
//! görüneceği oturum boyunca sabittir (ağırlığa göre seçilir; çizimde titremesin). Gösterim, kart ilk
//! kez çizildiğinde bir kez bildirilir; sunucu kişi başına günde bir kez sayar.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ClickEvent, SharedString, Window, div, px, relative};
use p2_client_core::{Api, SponsoredAd};
use p2_ui::controls::lh;
use p2_ui::motion::{EASE_OUT, M_FAST};
use p2_ui::tokens::{Palette, fs, radius};
use p2_ui::{Entrance, P2Motion as _, P2Text as _, P2Transition as _};

use super::discover_ui::{self as ui, glyph, ico};

const REFRESH: Duration = Duration::from_secs(15 * 60);
const HIDE_FOR_MS: u64 = 7 * 24 * 3600 * 1000;

#[derive(Default)]
struct State {
    api: Option<Arc<Api>>,
    ads: Vec<SponsoredAd>,
    fetched: Option<Instant>,
    loading: bool,
    /// yer → seçilen kart
    chosen: HashMap<&'static str, String>,
    viewed: HashSet<String>,
    /// kart → gizlenme bitişi (ms)
    hidden: HashMap<String, u64>,
    hidden_loaded: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn hidden_file() -> std::path::PathBuf {
    p2_client_core::store::profile_dir().join("sponsored-hidden.json")
}

/// Oturum açılınca (sunucu değişince akış da sıfırlanır).
pub fn install(api: Arc<Api>) {
    let mut s = state().lock().unwrap();
    let same = s.api.as_ref().is_some_and(|a| a.base() == api.base());
    s.api = Some(api);
    if !same {
        s.ads.clear();
        s.fetched = None;
        s.chosen.clear();
    }
}

fn load_hidden(s: &mut State) {
    if s.hidden_loaded {
        return;
    }
    s.hidden_loaded = true;
    if let Ok(text) = std::fs::read_to_string(hidden_file()) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, u64>>(&text) {
            let now = now_ms();
            s.hidden = map.into_iter().filter(|(_, until)| *until > now).collect();
        }
    }
}

fn refresh_if_needed(s: &mut State) {
    if s.loading || s.fetched.is_some_and(|t| t.elapsed() < REFRESH) {
        return;
    }
    let Some(api) = s.api.clone() else { return };
    s.loading = true;
    std::thread::spawn(move || {
        // Eski sunucuda uç yoksa (404) ya da kapalıysa kart gösterilmez; 15 dk sonra yeniden denenir.
        let ads = api.sponsored("").map(|f| if f.enabled { f.ads } else { Vec::new() }).unwrap_or_default();
        let mut s = state().lock().unwrap();
        if s.api.as_ref().is_some_and(|a| a.base() == api.base()) {
            crate::trace(&format!("sponsorlu: {} kart", ads.len()));
            s.chosen.retain(|_, id| ads.iter().any(|a| &a.id == id));
            s.ads = ads;
        }
        s.fetched = Some(Instant::now());
        s.loading = false;
        drop(s);
        crate::avatars::DIRTY.store(true, Ordering::Relaxed);
    });
}

/// Basit, oturuma bağlı rastgelelik (ağırlıklı seçim için yeterli).
fn random(seed: u64) -> u64 {
    let mut x = seed ^ (now_ms().wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (std::process::id() as u64) << 17;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Bu yerde gösterilecek kart (yoksa `None`; akış gelmediyse istek başlatır).
pub fn pick(placement: &'static str) -> Option<SponsoredAd> {
    if std::env::var_os("P2_NO_SPONSORED").is_some() {
        return None;
    }
    let mut s = state().lock().unwrap();
    load_hidden(&mut s);
    refresh_if_needed(&mut s);
    let now = now_ms();
    let visible = |s: &State, a: &SponsoredAd| a.placements.iter().any(|p| p == placement) && !s.hidden.get(&a.id).is_some_and(|u| *u > now);
    if let Some(id) = s.chosen.get(placement) {
        if let Some(ad) = s.ads.iter().find(|a| &a.id == id && visible(&s, a)) {
            return Some(ad.clone());
        }
    }
    let candidates: Vec<&SponsoredAd> = s.ads.iter().filter(|a| visible(&s, a)).collect();
    if candidates.is_empty() {
        s.chosen.remove(placement);
        return None;
    }
    // Başka yerde zaten görünen kart, seçenek varsa tekrarlanmaz.
    let elsewhere: HashSet<&String> = s.chosen.iter().filter(|(k, _)| **k != placement).map(|(_, v)| v).collect();
    let fresh: Vec<&SponsoredAd> = candidates.iter().copied().filter(|a| !elsewhere.contains(&a.id)).collect();
    let pool = if fresh.is_empty() { candidates } else { fresh };
    let total: u64 = pool.iter().map(|a| a.weight.max(1) as u64).sum();
    let mut roll = random(placement.len() as u64 * 7919) % total.max(1);
    let mut chosen = pool[0];
    for a in &pool {
        let w = a.weight.max(1) as u64;
        if roll < w {
            chosen = a;
            break;
        }
        roll -= w;
    }
    let ad = chosen.clone();
    s.chosen.insert(placement, ad.id.clone());
    Some(ad)
}

/// Kart ilk kez çizildiğinde bir kez (oturum başına) gösterim bildirir.
fn viewed(ad: &SponsoredAd) {
    let mut s = state().lock().unwrap();
    if !s.viewed.insert(ad.id.clone()) {
        return;
    }
    let (Some(api), id) = (s.api.clone(), ad.id.clone()) else { return };
    drop(s);
    std::thread::spawn(move || {
        let _ = api.sponsored_event(&id, "view");
    });
}

fn open(ad: &SponsoredAd, cx: &mut App) {
    crate::trace(&format!("sponsorlu: tıklandı {}", ad.id));
    cx.open_url(&ad.link_url);
    if let Some(api) = state().lock().unwrap().api.clone() {
        let id = ad.id.clone();
        std::thread::spawn(move || {
            let _ = api.sponsored_event(&id, "click");
        });
    }
}

/// Kartı 7 gün gizler (bu cihazda).
pub fn hide(id: &str) {
    let mut s = state().lock().unwrap();
    load_hidden(&mut s);
    s.hidden.insert(id.to_string(), now_ms() + HIDE_FOR_MS);
    s.chosen.retain(|_, v| v != id);
    let _ = std::fs::write(hidden_file(), serde_json::to_string(&s.hidden).unwrap_or_default());
    drop(s);
    crate::trace(&format!("sponsorlu: gizlendi {id}"));
    crate::avatars::DIRTY.store(true, Ordering::Relaxed);
}

fn cta(ad: &SponsoredAd) -> String {
    if ad.cta.trim().is_empty() { p2_i18n::t("spon.visit") } else { ad.cta.clone() }
}

/// "SPONSORLU" etiketi; üzerine gelince neden gösterildiğini söyler.
fn label(key: &str, on_image: bool, p: &Palette) -> AnyElement {
    let chip = if on_image {
        ui::glass_chip(None, p2_i18n::t("spon.label").to_uppercase(), false, p)
    } else {
        div()
            .h(px(18.))
            .px(px(7.))
            .flex_none()
            .flex()
            .items_center()
            .rounded(radius::PILL)
            .border_1()
            .border_color(p.border)
            .text_color(p.text_3)
            .text_size(fs(8.5))
            .css_weight(800.)
            .tracking(0.1)
            .child(p2_i18n::t("spon.label").to_uppercase())
    };
    div()
        .id(SharedString::from(format!("spon-label-{key}")))
        .child(chip)
        .tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("spon.why"), w, cx))
        .into_any_element()
}

/// Gizle (×) düğmesi: kartın tıklamasına geçmez.
fn hide_button(key: &str, ad: &SponsoredAd, on_image: bool, p: &Palette) -> AnyElement {
    let id = ad.id.clone();
    let (fg, hover) = if on_image { (ui::on_img(0.9), gpui_kit::Hsla { a: 0.55, ..gpui_kit::black() }) } else { (p.text_3, p.bg_hover) };
    div()
        .id(SharedString::from(format!("spon-hide-{key}")))
        .size(px(22.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius::PILL)
        .when(on_image, |s| s.bg(gpui_kit::Hsla { a: 0.38, ..gpui_kit::black() }))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(ico(glyph::X, 11., fg))
        .tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("spon.hide"), w, cx))
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            hide(&id);
            window.refresh();
        })
        .into_any_element()
}

fn cta_pill(ad: &SponsoredAd, p: &Palette) -> gpui_kit::Div {
    div()
        .h(px(26.))
        .px(px(11.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(radius::PILL)
        .border_1()
        .border_color(p.border_accent)
        .bg(p.accent_soft)
        .text_color(p.text_accent)
        .text_size(fs(10.5))
        .css_weight(750.)
        .whitespace_nowrap()
        .child(cta(ad))
        .child(ico(glyph::ARROW, 11., p.text_accent))
}

/// Keşfet ızgarası: proje kartıyla aynı boyda ve düzende.
pub fn grid_card(ad: &SponsoredAd, i: usize, w: f32, p: &Palette) -> AnyElement {
    viewed(ad);
    let key = format!("grid-{}", ad.id);
    let group: SharedString = format!("spon-card-{}", ad.id).into();
    let banner_h = 86.;
    let a = ad.clone();
    ui::card_shell(SharedString::from(format!("spon-{key}")), group.clone(), w, p)
        .child(
            div()
                .relative()
                .child(ui::banner(&ad.id, ad.image_url.as_deref(), w - 2., banner_h, (13., 0.), p))
                .child(ui::scrim(w - 2., banner_h, 0.55).absolute().top_0().left_0().rounded_t(px(13.)))
                .child(div().absolute().top(px(9.)).right(px(9.)).child(label(&key, true, p)))
                .child(div().absolute().top(px(9.)).left(px(9.)).invisible().group_hover(group.clone(), |s| s.visible()).child(hide_button(&key, ad, true, p))),
        )
        .child(
            div()
                .relative()
                .px(px(14.))
                .pb(px(13.))
                .flex()
                .flex_col()
                .child(div().mt(px(-24.)).size(px(54.)).flex().items_center().justify_center().rounded(px(17.)).bg(p.bg_2).child(ui::project_face(&ad.id, &ad.sponsor, None, 46., 14., p)))
                .child(div().mt(px(7.)).text_size(fs(14.)).line_height(lh(fs(14.))).css_weight(750.).tracking(-0.01).text_color(p.text_strong).truncate().child(ad.title.clone()))
                .child(div().mt(px(5.)).h(px(34.)).text_size(fs(11.)).line_height(px(17.)).text_color(p.text_3).line_clamp(2).child(ad.body.clone()))
                .child(
                    div()
                        .mt(px(11.))
                        .pt(px(11.))
                        .border_t_1()
                        .border_color(p.border_subtle)
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().flex_1().min_w_0().text_size(fs(10.5)).css_weight(650.).text_color(p.text_3).truncate().child(ad.sponsor.clone()))
                        .child(cta_pill(ad, p)),
                ),
        )
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| open(&a, cx))
        .p2_enter(SharedString::from(format!("spon-in-{key}")), Entrance::Rise, ui::stagger(i))
        .into_any_element()
}

/// Keşfet haber sütunu: haber satırıyla aynı ölçüde, sakin bir satır.
pub fn news_row(ad: &SponsoredAd, w: f32, p: &Palette) -> AnyElement {
    viewed(ad);
    let key = format!("news-{}", ad.id);
    let group: SharedString = format!("spon-news-{}", ad.id).into();
    let a = ad.clone();
    let (hover, border) = (p.bg_hover, p.border);
    div()
        .id(SharedString::from(format!("spon-{key}")))
        .group(group.clone())
        .w(px(w))
        .p(px(8.))
        .flex()
        .items_center()
        .gap(px(11.))
        .rounded(radius::MD)
        .border_1()
        .border_color(p.border_subtle)
        .cursor_pointer()
        .transition(M_FAST, EASE_OUT)
        .hover(move |s| s.bg(hover).border_color(border))
        .child(ui::banner(&ad.id, ad.image_url.as_deref(), 84., 58., (9., 9.), p))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().text_size(fs(12.)).line_height(px(17.)).css_weight(700.).text_color(p.text_strong).line_clamp(2).child(ad.title.clone()))
                .child(div().flex().items_center().gap(px(6.)).min_w_0().child(label(&key, false, p)).child(div().min_w_0().text_size(fs(10.)).css_weight(700.).text_color(p.text_3).truncate().child(ad.sponsor.clone()))),
        )
        .child(div().invisible().group_hover(group, |s| s.visible()).child(hide_button(&key, ad, false, p)))
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| open(&a, cx))
        .into_any_element()
}

/// Kütüphane kenar çubuğunun altı: küçük, sakin kart.
pub fn compact_card(ad: &SponsoredAd, w: f32, p: &Palette) -> AnyElement {
    viewed(ad);
    let key = format!("side-{}", ad.id);
    let a = ad.clone();
    let (hover_border, bg) = (p.border_strong, p.bg_2);
    let image_h = (w * 0.42).round().clamp(70., 110.);
    div()
        .id(SharedString::from(format!("spon-{key}")))
        .w(px(w))
        .flex()
        .flex_col()
        .rounded(radius::MD)
        .border_1()
        .border_color(p.border_subtle)
        .bg(bg)
        .overflow_hidden()
        .cursor_pointer()
        .transition(M_FAST, EASE_OUT)
        .hover(move |s| s.border_color(hover_border))
        .when(ad.image_url.is_some(), |s| s.child(ui::banner(&ad.id, ad.image_url.as_deref(), w - 2., image_h, (9., 0.), p)))
        .child(
            div()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(div().flex().items_center().gap(px(6.)).child(label(&key, false, p)).child(div().flex_1().min_w_0().text_size(fs(10.)).css_weight(700.).text_color(p.text_3).truncate().child(ad.sponsor.clone())).child(hide_button(&key, ad, false, p)))
                .child(div().text_size(fs(12.5)).line_height(relative(1.3)).css_weight(750.).text_color(p.text_strong).line_clamp(2).child(ad.title.clone()))
                .when(!ad.body.trim().is_empty(), |s| s.child(div().text_size(fs(11.)).line_height(px(16.)).text_color(p.text_3).line_clamp(2).child(ad.body.clone())))
                .child(div().mt(px(3.)).flex().child(cta_pill(ad, p))),
        )
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| open(&a, cx))
        .into_any_element()
}
