//! Açılış (splash) pencereleri — Electron `main.cjs` `showSplash`/`SPLASH_HTML` ve
//! `patcher.cjs` `showLaunchSplash` karşılıkları, P2 Mavi (`p2-mavi.cjs`) görünümüyle.
//!
//! 1. **Açılış penceresi** (300×340, çerçevesiz, ortada, her zaman üstte, sürüklenebilir):
//!    uygulama penceresinden ÖNCE açılır, güncelleme denetimini gösterir ("Başlatılıyor…" →
//!    "Güncellemeler denetleniyor…"; indirme sürerse "Güncelleme indiriliyor · %N" ve dolan
//!    çubuk), bitince kapanır ve ana pencere açılır. Electron'daki gibi yalnız kurulu sürümde ve
//!    daha önce bağlanılmış bir sunucu varken (`P2_UPDATER_TEST=1` testte açar, `P2_SPLASH=0` kapatır).
//! 2. **Oyun başlatma kartı** (440×150, saydam, odak almaz, her zaman üstte): "P2 · Başlatılıyor"
//!    + oyun adı + belirsiz çubuk; overlay "hazır" deyince, düz başlatmada 4 sn sonra, oyun
//!    kapanınca ya da en geç 20 sn'de kapanır (`screens/launch_splash.rs`).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyWindowHandle, App, Bounds, Context, FontWeight, ImageSource, ObjectFit, RenderImage, SharedString, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, hsla, img, linear_color_stop, linear_gradient, point, px, relative, rgb, size,
};
use p2_ui::motion::{CubicBezier, EASE_OUT, EASE_POP, Entrance, P2Motion as _};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT, Palette, radius};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

/// `assets/icon.png` 2× (192 px) küçültülmüş (main.cjs `splashIcon` 96 px'e küçültür).
pub fn icon() -> Option<Arc<RenderImage>> {
    static ICON: OnceLock<Option<Arc<RenderImage>>> = OnceLock::new();
    ICON.get_or_init(|| {
        let img = image::load_from_memory(ICON_PNG).ok()?.resize_exact(192, 192, image::imageops::FilterType::Lanczos3);
        let mut bgra = img.to_rgba8();
        for px in bgra.pixels_mut() {
            px.0.swap(0, 2);
        }
        Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
    })
    .clone()
}

fn c(hex: u32) -> gpui_kit::Hsla {
    rgb(hex).into()
}

/// `.p2-kicker`: 9.5px, 800, harf aralığı .18em, büyük harf, vurgu rengi.
pub fn kicker(text: &str, p: &Palette) -> impl IntoElement {
    // .p2-kicker: 9.5px 800, letter-spacing .18em (yamalı DirectWrite harf aralığı)
    use p2_ui::P2Text as _;
    div().text_size(px(9.5)).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(text.to_uppercase())
}


/// `.p2-bar` (5px, hap, rgba(120,170,255,.14) zemin, `--p2-accent-bar` dolgu). `percent`
/// yoksa `.indeterminate`: %38'lik dolgu 1,3 sn'de -%40 → %105 (ease-in-out, sonsuz).
pub fn bar(id: impl Into<SharedString>, percent: Option<f32>) -> impl IntoElement {
    let id = id.into();
    let fill = || div().absolute().top_0().bottom_0().rounded(radius::PILL).bg(linear_gradient(90., linear_color_stop(c(0x4f9dff), 0.), linear_color_stop(c(0x8fd0ff), 1.)));
    let track = div().relative().w_full().h(px(5.)).overflow_hidden().rounded(radius::PILL).bg(hsla(218. / 360., 1., 0.735, 0.14));
    match percent {
        Some(v) => track.child(fill().left_0().w(relative((v / 100.).clamp(0., 1.)))).into_any_element(),
        None => {
            const EASE_IN_OUT: CubicBezier = CubicBezier::new(0.42, 0., 0.58, 1.);
            track
                .child(fill().w(relative(0.38)).with_animation(SharedString::from(format!("{id}-run")), Animation::new(Duration::from_millis(1300)).repeat(), |el, t| el.left(relative(-0.4 + 1.45 * EASE_IN_OUT.ease(t)))))
                .into_any_element()
        }
    }
}

/// `p2-logo-pop` (.55 sn, ease-pop, .12 sn gecikme): 0 → %55'te ölçek 1.14 ve 3° → 1. GPUI'de
/// div ölçeklenemediği için kutu `size` içinde ortalanarak büyütülür; döndürme yok.
pub fn logo_pop(id: impl Into<SharedString>, size_px: f32, content: impl Fn(f32) -> gpui_kit::AnyElement + 'static) -> impl IntoElement {
    let id = id.into();
    div().size(px(size_px)).flex().items_center().justify_center().child(div().with_animation(SharedString::from(format!("{id}-pop")), Animation::new(Duration::from_millis(670)), move |el, t| {
        // .12 sn gecikme (toplam .67 sn)
        let raw = ((t * 0.67 - 0.12) / 0.55).clamp(0., 1.);
        let e = EASE_POP.ease(raw);
        let scale = if raw < 0.55 { 0.4 + (1.14 - 0.4) * (e / EASE_POP.ease(0.55)).min(1.) } else { 1.14 + (1. - 1.14) * ((raw - 0.55) / 0.45) };
        let opacity = (raw / 0.55).min(1.);
        el.opacity(opacity).child(content(size_px * scale.max(0.)))
    }))
}

// ============================================================ açılış penceresi

/// Açılış penceresinin durumu (updater.cjs `state` alt kümesi).
pub struct StartupSplash {
    /// Son metin ("Başlatılıyor…", "Güncellemeler denetleniyor…", "Güncelleme indiriliyor").
    label: String,
    /// İndirme yüzdesi (yalnız `downloading` ve > 0 iken).
    percent: Option<u32>,
}

impl StartupSplash {
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.label = text.to_string();
        crate::trace(&format!("açılış: {text}"));
        cx.notify();
    }

    pub fn set_percent(&mut self, percent: Option<u32>, cx: &mut Context<Self>) {
        self.percent = percent.filter(|p| *p > 0);
        cx.notify();
    }

    /// SPLASH_HTML: yüzde varken metin "Güncelleme indiriliyor · %N" (sondaki … düşer).
    pub fn text(&self) -> String {
        match self.percent {
            Some(p) => format!("{} · %{p}", self.label.trim_end_matches('…')),
            None => self.label.clone(),
        }
    }
}

impl Render for StartupSplash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let logo = |s: f32| match icon() {
            Some(icon) => div()
                .size(px(s))
                .rounded(px(s * 0.25))
                .overflow_hidden()
                .shadow(vec![gpui_kit::BoxShadow { color: hsla(218. / 360., 0.83, 0.47, 0.45), offset: point(px(0.), px(6.)), blur_radius: px(18.), spread_radius: px(0.), inset: false }])
                .child(img(ImageSource::Render(icon)).size_full().object_fit(ObjectFit::Cover))
                .into_any_element(),
            None => div()
                .size(px(s))
                .rounded(px(s * 0.25))
                .flex()
                .items_center()
                .justify_center()
                .bg(linear_gradient(135., linear_color_stop(c(0x2c7bff), 0.), linear_color_stop(c(0x173064), 1.)))
                .text_color(c(0xeef5ff))
                .text_size(px(34. * s / 96.))
                .font_weight(FontWeight::BLACK)
                .child("P2")
                .into_any_element(),
        };
        // body: radial-gradient(circle at 50% 40%, accent-soft 0, transparent 55%), bg-0 — SVG radyal gradyan (birebir).
        let vp = window.viewport_size();
        let glow = div().absolute().top_0().left_0().child(p2_ui::components::radial_background(f32::from(vp.width), f32::from(vp.height), (0.5, 0.4), &[(p.accent_soft, 0.), (p.bg_0, 0.55)], p.bg_0));
        div()
            .id("splash")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(p.bg_0)
            .font_family(FONT)
            .text_color(p.text)
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, _| window.start_window_move())
            .child(glow)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .px(px(26.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(logo_pop("splash-logo", 96., move |s| logo(s)))
                    // .wrap gap 18px; .kicker margin -6px
                    .child(div().mt(px(12.)).child(kicker("P2", &p)))
                    .child(
                        div()
                            .mt(px(18.))
                            .min_h(px(17.))
                            .text_size(px(12.))
                            .line_height(px(17.4))
                            .text_color(p.text_3)
                            .text_center()
                            .child(self.text())
                            .p2_enter("splash-text", Entrance::PopElastic, Duration::from_millis(150)),
                    )
                    .child(div().mt(px(18.)).w(px(180.)).child(bar("splash-bar", self.percent.map(|v| v as f32)))),
            )
    }
}

/// Açılış penceresi gösterilsin mi (main.cjs: `app.isPackaged && updates.savedBase()`): kurulu
/// sürüm (ya da `P2_UPDATER_TEST`) ve daha önce bağlanılmış bir sunucu (`update-source.json`).
/// `P2_SPLASH=0` kapatır. Dönüş: denetlenecek sunucu adresi.
pub fn wanted() -> Option<String> {
    if std::env::var("P2_SPLASH").ok().as_deref() == Some("0") || !crate::updates::packaged() {
        return None;
    }
    let base = crate::updates::updater().saved_base();
    if base.is_empty() {
        crate::trace("açılış: kayıtlı güncelleme kaynağı yok, açılış penceresi atlandı");
        return None;
    }
    Some(base)
}

/// Açılış penceresini açar, güncelleme turunu (`ClientUpdater::startup_check`) yürütür, bitince
/// kapatıp `then`'i çağırır — ana pencere ancak o zaman açılır (main.cjs `whenReady`). Kurulum
/// başladıysa uygulama açılmaz (güncelleyicinin `quit` kancası süreci kapatır).
pub fn run(cx: &mut App, base: String, then: impl FnOnce(&mut App) + 'static) {
    let bounds = Bounds::centered(None, size(px(300.), px(340.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("P2".into()), appears_transparent: true, traffic_light_position: None }),
        is_resizable: false,
        is_minimizable: false,
        is_movable: true,
        app_owns_titlebar_drag: true,
        window_min_size: Some(size(px(300.), px(340.))),
        ..Default::default()
    };
    let handle = match cx.open_window(options, |window, cx| {
        // alwaysOnTop: true
        if let Some(hwnd) = crate::shell::hwnd_of(window) {
            crate::shell::set_topmost(hwnd, true);
        }
        cx.new(|_| StartupSplash { label: "Başlatılıyor…".into(), percent: None })
    }) {
        Ok(h) => h,
        Err(e) => {
            crate::trace(&format!("açılış: pencere açılamadı ({e}); doğrudan devam"));
            then(cx);
            return;
        }
    };
    crate::trace("açılış: pencere gösterildi");
    cx.activate(true);
    cx.spawn(async move |cx| {
        // Kendi sunucumuzsa API'nin kalkmasını kısa süre bekle (main.cjs waitForServer 12 sn).
        if base.contains("127.0.0.1") || base.contains("localhost") {
            let api = p2_client_core::Api::new(&base, crate::CLIENT_VERSION);
            cx.background_executor()
                .spawn(async move {
                    let until = std::time::Instant::now() + Duration::from_secs(12);
                    while api.health().is_err() && std::time::Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(400));
                    }
                })
                .await;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let b = base.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::updates::updater().startup_check(&b));
        });
        // SPLASH_HTML `onState`: metin geldikçe etiket; indirmede yüzde ve dolan çubuk.
        let outcome = loop {
            let s = crate::updates::state();
            let _ = handle.update(cx, |v, _, cx| {
                if !s.text.is_empty() && v.label != s.text {
                    v.set_text(&s.text, cx);
                }
                let pct = (s.status == p2_updater::Status::Downloading && s.percent > 0).then_some(s.percent);
                if pct != v.percent {
                    v.set_percent(pct, cx);
                }
            });
            if let Ok(outcome) = rx.try_recv() {
                break outcome;
            }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        };
        crate::trace(&format!("açılış: güncelleme turu bitti ({outcome:?})"));
        if outcome == p2_updater::StartupOutcome::Installing {
            return;
        }
        // Test kancası: ekran görüntüsü için pencereyi biraz daha tut.
        if let Some(ms) = std::env::var("P2_SPLASH_HOLD_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
            cx.background_executor().timer(Duration::from_millis(ms)).await;
        }
        // Önce ana pencere açılır, sonra açılış penceresi kapanır: arada hiç pencere kalmazsa
        // uygulama "son pencere kapandı" diye çıkabiliyor.
        let _ = cx.update(|cx| {
            then(cx);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            crate::trace("açılış: pencere kapandı");
        });
    })
    .detach();
}


// ============================================================ oyun başlatma kartı

pub struct LaunchCardView {
    name: String,
    initials: String,
}

impl Render for LaunchCardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let initials = self.initials.clone();
        let icon = move |s: f32| {
            div()
                .size(px(s))
                .rounded(px(18. * s / 78.))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .bg(p2_ui::components::accent_gradient())
                .shadow(vec![gpui_kit::BoxShadow { color: hsla(218. / 360., 0.83, 0.47, 0.4), offset: point(px(0.), px(10.)), blur_radius: px(30.), spread_radius: px(0.), inset: false }])
                .text_size(px(26. * s / 78.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .text_color(c(0xffffff))
                .child(initials.clone())
                .into_any_element()
        };
        // .p2-card: border-strong, r-lg, --p2-glass, gölge; .shine: bir kez geçen parıltı.
        let glass = if p.dark {
            linear_gradient(135., linear_color_stop(hsla(220. / 360., 0.46, 0.102, 0.96), 0.), linear_color_stop(hsla(220. / 360., 0.47, 0.059, 0.94), 1.))
        } else {
            linear_gradient(135., linear_color_stop(p.bg_1, 0.), linear_color_stop(p.bg_2, 1.))
        };
        let shine = div().absolute().top(relative(-0.2)).bottom(relative(-0.2)).w(relative(0.38)).flex().child(div().flex_1().bg(linear_gradient(90., linear_color_stop(hsla(0., 0., 0., 0.), 0.), linear_color_stop(hsla(214. / 360., 1., 0.735, 0.14), 1.)))).child(
            div().flex_1().bg(linear_gradient(90., linear_color_stop(hsla(214. / 360., 1., 0.735, 0.14), 0.), linear_color_stop(hsla(0., 0., 0., 0.), 1.))),
        );
        let ease_out = EASE_OUT;
        div().size_full().font_family(FONT).child(
            div()
                .id("card")
                .size_full()
                .relative()
                .overflow_hidden()
                .rounded(radius::LG)
                .border_1()
                .border_color(p.border_strong)
                .bg(glass)
                .px(px(24.))
                .py(px(18.))
                .flex()
                .items_center()
                .gap(px(18.))
                .child(shine.with_animation("card-shine", Animation::new(Duration::from_millis(1350)), move |el, t| {
                    // p2-shine 1.1 sn ease-out, .25 sn gecikme: -120% → 260% (kendi genişliği)
                    let raw = ((t * 1.35 - 0.25) / 1.1).clamp(0., 1.);
                    el.left(relative(0.38 * (-1.2 + 3.8 * ease_out.ease(raw))))
                }))
                .child(logo_pop("card-icon", 78., icon))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().mb(px(4.)).child(kicker("P2 · Başlatılıyor", &p)))
                        .child(div().text_size(px(19.)).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_strong).truncate().child(self.name.clone()))
                        .child(div().mt(px(12.)).child(bar("card-bar", None))),
                )
                .p2_enter("card-in", Entrance::PopElastic, Duration::ZERO),
        )
    }
}

/// patcher.cjs `initials`: adın ilk iki sözcüğünün baş harfleri, büyük harf.
pub fn initials(name: &str) -> String {
    let s: String = name.split_whitespace().take(2).filter_map(|w| w.chars().next()).collect();
    let s = s.to_uppercase();
    if s.is_empty() { "P".into() } else { s }
}

/// Ekranın ortasında, odak almadan açılan saydam kart.
pub fn open_launch_card(cx: &mut App, name: &str) -> Option<AnyWindowHandle> {
    let bounds = Bounds::centered(None, size(px(440.), px(150.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("P2 · Başlatılıyor".into()), appears_transparent: true, traffic_light_position: None }),
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let (name, ini) = (name.to_string(), initials(name));
    match cx.open_window(options, |_, cx| cx.new(|_| LaunchCardView { name, initials: ini })) {
        Ok(h) => Some(h.into()),
        Err(e) => {
            crate::trace(&format!("başlatma kartı açılamadı: {e}"));
            None
        }
    }
}

pub fn close_window(cx: &mut App, handle: AnyWindowHandle) {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
}

#[cfg(test)]
mod tests {
    #[test]
    fn initials_like_patcher() {
        assert_eq!(super::initials("Spike 2"), "S2");
        assert_eq!(super::initials("aurora borealis game"), "AB");
        assert_eq!(super::initials("  "), "P");
    }

    #[test]
    fn icon_decodes() {
        assert!(super::icon().is_some());
    }
}
