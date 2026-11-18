//! Arama balonu — main.cjs `syncCallWidget` / `WIDGET_HTML` / `askWidgetClose` karşılığı.
//! Aramadayken ana pencere simge durumuna küçültülür ya da tepsiye gizlenirse ekranın sağ
//! altında küçük, her zaman üstte bir kontrol penceresi belirir: karşıdaki kişinin avatarı ve adı
//! (odada / grupta arama adı), süre; yalnız simgeli yuvarlak mikrofon, kulaklık, "P2'yi aç", ayrıl
//! düğmeleri ve kartın içinde × (bu seferlik gizle). Kapalı mikrofon / kulaklık çizgili simge ve
//! yumuşak kırmızı zeminle gösterilir (yazı yok). Sürükleyerek taşınan konum ve "bir daha gösterme" `call-widget.json`'da
//! (Electron'la aynı biçim: `{x, y, hidden}`, mantıksal piksel).

use std::time::{Duration, Instant};

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, FontWeight, Hsla, Subscription, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, canvas, div, linear_color_stop, linear_gradient, point, px, rgb, rgba, size};

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::call::CallController;

pub const TITLE: &str = "P2 arama balonu";
/// Pencere boyutu = balon kartı (saydam kenar yok: Windows'ta saydam açılır pencere beyaz zeminle
/// çizilebiliyordu). Köşeler DWM ile yuvarlanır, kenarlık rengi DWM'den.
/// İçeriğe göre: 10 + avatar 34 + 10 + yazı 112 + 10 + dört düğme (4×32 + 3×6) + 6 + × 22 + 8.
const W: f32 = 358.;
const H: f32 = 56.;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct WidgetState {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub hidden: bool,
}

impl WidgetState {
    fn from_json(v: &serde_json::Value) -> Self {
        let n = |k: &str| v.get(k).and_then(|x| x.as_f64()).map(|f| f.round() as i32);
        Self { x: n("x"), y: n("y"), hidden: v.get("hidden").and_then(|h| h.as_bool()).unwrap_or(false) }
    }

    fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        if let Some(x) = self.x {
            m.insert("x".into(), x.into());
        }
        if let Some(y) = self.y {
            m.insert("y".into(), y.into());
        }
        if self.hidden {
            m.insert("hidden".into(), true.into());
        }
        serde_json::Value::Object(m)
    }
}

fn state_path() -> std::path::PathBuf {
    p2_client_core::store::profile_dir().join("call-widget.json")
}

pub fn load_state() -> WidgetState {
    std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).map(|v| WidgetState::from_json(&v)).unwrap_or_default()
}

pub fn save_state(patch: impl FnOnce(&mut WidgetState)) {
    let mut s = load_state();
    patch(&mut s);
    let _ = std::fs::write(state_path(), s.to_json().to_string());
}

// ---------------------------------------------------------------- P2 Mavi (sabit koyu tema)

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}
fn ca(hex: u32) -> Hsla {
    rgba(hex).into()
}
const TEXT_STRONG: u32 = 0xeef5ff;
const TEXT_3: u32 = 0x7d93b8;
const BG_HOVER: u32 = 0x182440;
const TEXT_2: u32 = 0xd6e4ffc7; // rgba(214,228,255,.78)
const SUCCESS: u32 = 0x3ddc84;
const DANGER: u32 = 0xff5c6c;
const DANGER_SOFT: u32 = 0xff5c6c2e; // .18
const DANGER_HOVER: u32 = 0xff5c6c47; // .28
const DANGER_TEXT: u32 = 0xff9aa4;
/// Düğme zemini: cam üstünde hafif beyaz.
const GLASS_BTN: u32 = 0xffffff12; // rgba(255,255,255,.07)
const GLASS_EDGE: u32 = 0xffffff14; // rgba(255,255,255,.08)

/// Dolgulu arama simgesi (uygulamadaki arama çubuğuyla aynı takım).
fn glyph(name: &str, ink: Hsla) -> gpui_kit::Svg {
    p2_ui::icons::icon(&format!("call:{name}")).stroke(0.).filled().el().size(px(16.)).text_color(ink)
}

// ---------------------------------------------------------------- pencere

pub struct CallWidget {
    calls: Entity<CallController>,
    _observe: Subscription,
}

impl CallWidget {
    fn new(calls: Entity<CallController>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&calls, |_, _, cx| cx.notify());
        // Saat her saniye ilerler.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();
        Self { calls, _observe: observe }
    }

    fn act(&mut self, action: &'static str, cx: &mut Context<Self>) {
        crate::trace(&format!("balon: {action}"));
        if action == "close" {
            // × balonu bu arama için gizler (sonraki aramada yine çıkar).
            self.calls.update(cx, |c, cx| c.widget_skip_this_call(cx));
            return;
        }
        let calls = self.calls.clone();
        if action == "leave" {
            // Ayrılma ana pencerenin bağlamında: motorun kapanış zamanlayıcısı oraya bağlı.
            if let Some(main) = calls.read(cx).main_window() {
                let _ = main.update(cx, |_, window, cx| calls.update(cx, |c, cx| c.leave(window, cx)));
            }
            return;
        }
        calls.update(cx, |c, cx| c.widget_action(action, cx));
    }
}

/// Yuvarlak, yalnız simgeli düğme. `off`: kapalı mikrofon / kulaklık (yumuşak kırmızı zemin);
/// `hangup`: dolu kırmızı.
fn button(id: &'static str, tip: String, off: bool, hangup: bool) -> gpui_kit::Stateful<gpui_kit::Div> {
    let tip = gpui_kit::SharedString::from(tip);
    let bg = if hangup { c(DANGER) } else if off { ca(DANGER_SOFT) } else { ca(GLASS_BTN) };
    div()
        .id(id)
        .size(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .cursor_pointer()
        .hover(move |s| if hangup { s.bg(c(0xff7482)) } else if off { s.bg(ca(DANGER_HOVER)) } else { s.bg(c(BG_HOVER)) })
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
        // Düğmeye basmak balonu sürüklemesin.
        .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .relative()
        .child(probe(id))
}

fn clock(at: Instant) -> String {
    let s = at.elapsed().as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl Render for CallWidget {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let calls = self.calls.read(cx);
        let me = calls.me_id().to_string();
        // (ad, alt satır, avatar kişisi, fazladan kişi sayısı, mikrofon, kulaklık)
        let (label, meta, face, extra, muted, deafened) = match &calls.call {
            Some(call) => {
                let others: Vec<_> = call.participants.iter().filter(|x| x.user_id != me).collect();
                let direct = call.kind == "direct";
                let counterpart = if call.caller.as_deref() == Some(me.as_str()) { call.callee.clone() } else { call.caller.clone() };
                let label = match (direct, others.first()) {
                    (true, Some(x)) if !x.display_name.is_empty() => x.display_name.clone(),
                    (true, _) => p2_i18n::t("cwid.voiceCall"),
                    _ if call.kind == "channel" => p2_i18n::t("cwid.voiceRoom"),
                    _ => p2_i18n::t("cwid.voiceCall"),
                };
                let ringing = calls.connected_at.is_none() && direct;
                let time = |at: Option<Instant>| at.map(clock).unwrap_or_else(|| "00:00".into());
                let meta = if ringing {
                    p2_i18n::tv("cwid.ringing", &[("time", &time(calls.started_at))])
                } else if direct {
                    time(calls.connected_at)
                } else {
                    p2_i18n::tv("cwid.durationPeople", &[("time", &time(calls.connected_at)), ("n", &call.participants.len().to_string())])
                };
                let face = others.first().map(|x| (x.user_id.clone(), x.display_name.clone())).or_else(|| direct.then_some(counterpart).flatten().map(|id| (id, String::new())));
                (label, meta, face, others.len().saturating_sub(1), calls.muted, calls.deafened)
            }
            None => (p2_i18n::t("cwid.voiceCall"), "00:00".to_string(), None, 0, false, false),
        };
        let ink_of = |off: bool| if off { c(DANGER_TEXT) } else { ca(TEXT_2) };
        let mute_btn = button("w-mute", if muted { p2_i18n::t("cwid.unmute") } else { p2_i18n::t("cwid.mute") }, muted, false)
            .child(glyph(if muted { "micOff" } else { "mic" }, ink_of(muted)))
            .on_click(cx.listener(|this, _, _, cx| this.act("mute", cx)));
        let deaf_btn = button("w-deafen", if deafened { p2_i18n::t("cwid.soundOn") } else { p2_i18n::t("cwid.soundOff") }, deafened, false)
            .child(glyph(if deafened { "headphonesOff" } else { "headphones" }, ink_of(deafened)))
            .on_click(cx.listener(|this, _, _, cx| this.act("deafen", cx)));
        let open_btn = button("w-open", p2_i18n::t("cwid.openApp"), false, false).child(glyph("expand", ca(TEXT_2))).on_click(cx.listener(|this, _, _, cx| this.act("open", cx)));
        let leave_btn = button("w-leave", p2_i18n::t("cwid.leave"), false, true).child(glyph("hangup", gpui_kit::white())).on_click(cx.listener(|this, _, _, cx| this.act("leave", cx)));

        // Avatar (fotoğraf ya da baş harfler) + sağ altta "aramada" noktası; odada/grupta "+N".
        let photo = face.as_ref().and_then(|(id, _)| crate::avatars::image(id));
        let initials = face.as_ref().map(|(_, name)| p2_ui::components::user_initials(name)).filter(|s| !s.is_empty());
        let avatar = div()
            .relative()
            .flex_none()
            .size(px(34.))
            .child(match photo {
                Some(image) => gpui_kit::img(gpui_kit::ImageSource::Render(image)).size(px(34.)).rounded(px(11.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
                None => div()
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(linear_gradient(145., linear_color_stop(c(0x2f6df2), 0.), linear_color_stop(c(0x1b3f9c), 1.)))
                    .text_color(c(TEXT_STRONG))
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .map(|d| match initials {
                        Some(text) => d.child(text),
                        None => d.child(glyph("phone", c(TEXT_STRONG))),
                    })
                    .into_any_element(),
            })
            .child(
                // `.dot`: `p2-pulse 2.2s` (opaklık 1 → .55 → 1), kart zemini renginde halka.
                div().absolute().right(px(-3.)).bottom(px(-3.)).size(px(13.)).flex().items_center().justify_center().rounded_full().bg(c(0x0c1322)).child(
                    div()
                        .size(px(9.))
                        .rounded_full()
                        .bg(c(SUCCESS))
                        .with_animation("w-dot-pulse", gpui_kit::Animation::new(Duration::from_millis(2200)).repeat(), |el, t| el.opacity(1. - 0.45 * (1. - (t * std::f32::consts::TAU).cos()) / 2.)),
                ),
            )
            .when(extra > 0, |s| {
                s.child(
                    div()
                        .absolute()
                        .left(px(-4.))
                        .top(px(-4.))
                        .h(px(14.))
                        .min_w(px(14.))
                        .px(px(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(c(BG_HOVER))
                        .text_color(c(TEXT_STRONG))
                        .text_size(px(8.5))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("+{extra}")),
                )
            });

        let close_btn = div()
            .id("w-close")
            .relative()
            .child(probe("w-close"))
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(|s| s.bg(ca(GLASS_BTN)))
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(p2_i18n::t("cwid.hideOnce")).build(window, cx))
            .child(p2_ui::icons::icon("composer:close").stroke(2.).el().size(px(12.)).text_color(c(TEXT_3)))
            .on_click(cx.listener(|this, _, _, cx| this.act("close", cx)));
        // Balon kartı pencerenin tamamı (cam görünümlü koyu degrade); boş yerinden sürüklenir.
        div()
            .id("w-bubble")
            .size_full()
            .font_family("Segoe UI")
            .pl(px(10.))
            .pr(px(8.))
            .flex()
            .items_center()
            .bg(linear_gradient(135., linear_color_stop(c(0x121d33), 0.), linear_color_stop(c(0x090e1a), 1.)))
            // İnce iç kenar (cam kenarı); dış kenarlık ve yuvarlak köşe DWM'den.
            .border_1()
            .border_color(ca(GLASS_EDGE))
            // Balonun boş yerinden sürüklenir (-webkit-app-region: drag).
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, _| drag(window))
            .child(avatar)
            .child(
                div()
                    .ml(px(10.))
                    .mr(px(10.))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(c(TEXT_STRONG)).truncate().child(label))
                    // Tek satır: sığmazsa üç nokta (alt satıra kayma yok).
                    .child(div().text_size(px(10.5)).text_color(c(TEXT_3)).truncate().child(meta)),
            )
            .child(div().flex_none().flex().items_center().gap(px(6.)).child(mute_btn).child(deaf_btn).child(open_btn).child(leave_btn))
            .child(div().flex_none().ml(px(6.)).child(close_btn))
    }
}

/// Pencereyi başlıktan tutulmuş gibi sürükle (Windows: WM_NCLBUTTONDOWN + HTCAPTION).
fn drag(window: &Window) {
    if let Some(hwnd) = crate::shell::hwnd_of(window) {
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            let _ = PostMessageW(Some(hwnd), WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
        }
    }
}

/// Ana pencere simge durumunda ya da gizli mi (main.cjs: `!isVisible() || isMinimized()`).
pub fn main_hidden() -> bool {
    let hwnd = HWND(crate::MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed) as _);
    !hwnd.is_invalid() && !crate::shell::is_visible(hwnd)
}

/// Balonu aç: kayıtlı konum ekranlardan birinde değilse birincil çalışma alanının sağ altı.
pub fn open(calls: Entity<CallController>, cx: &mut App) -> Option<WindowHandle<CallWidget>> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds { origin: point(px(0.), px(0.)), size: size(px(W), px(H)) })),
        titlebar: Some(TitlebarOptions { title: Some(TITLE.into()), appears_transparent: true, traffic_light_position: None }),
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            if let Some(hwnd) = crate::shell::hwnd_of(window) {
                place(hwnd, window.scale_factor());
            }
            cx.new(|cx| CallWidget::new(calls, cx))
        })
        .ok()?;
    crate::trace("balon: göründü");
    Some(handle)
}

fn place(hwnd: HWND, scale: f32) {
    let state = load_state();
    let (w, h) = ((W * scale).round() as i32, (H * scale).round() as i32);
    let mut work = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let saved = match (state.x, state.y) {
        (Some(x), Some(y)) => {
            let (px_, py_) = ((x as f32 * scale) as i32, (y as f32 * scale) as i32);
            let probe = RECT { left: px_, top: py_, right: px_ + (W * scale) as i32, bottom: py_ + (H * scale) as i32 };
            let on_screen = unsafe { !windows::Win32::Graphics::Gdi::MonitorFromRect(&probe, windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONULL).is_invalid() };
            on_screen.then_some((px_, py_))
        }
        _ => None,
    };
    let (mut x, mut y) = saved.unwrap_or((work.right - ((W + 16.) * scale) as i32, work.bottom - ((H + 16.) * scale) as i32));
    // Test: P2_TEST_WINDOW_POS verilmişse balon da ekran dışında açılır (kullanıcının ekranına çıkmasın).
    if let Some((tx, ty)) = std::env::var("P2_TEST_WINDOW_POS").ok().and_then(|v| {
        let (a, b) = v.split_once(',')?;
        Some((a.trim().parse::<i32>().ok()?, b.trim().parse::<i32>().ok()?))
    }) {
        (x, y) = (tx, ty);
    }
    unsafe {
        // Windows 11: yuvarlak köşe + P2 Mavi kenarlık (eski sürümlerde yok sayılır).
        use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const _, std::mem::size_of_val(&corner) as u32);
        let border: u32 = 0x00_8a_5a_3c; // COLORREF 0x00BBGGRR = rgb(60,90,138)
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const _, 4);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

/// Taşındıysa yeni konumu (mantıksal piksel) kaydeder.
pub fn remember_position(handle: &WindowHandle<CallWidget>, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| {
        let Some(hwnd) = crate::shell::hwnd_of(window) else { return };
        let mut r = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
            return;
        }
        let scale = window.scale_factor();
        let (x, y) = ((r.left as f32 / scale).round() as i32, (r.top as f32 / scale).round() as i32);
        let s = load_state();
        if s.x != Some(x) || s.y != Some(y) {
            save_state(|s| {
                s.x = Some(x);
                s.y = Some(y);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_json_like_electron() {
        let s = WidgetState::from_json(&serde_json::from_str(r#"{"x":10,"y":20,"hidden":true}"#).unwrap());
        assert_eq!((s.x, s.y, s.hidden), (Some(10), Some(20), true));
        assert_eq!(WidgetState { x: Some(1), y: Some(2), hidden: false }.to_json().to_string(), r#"{"x":1,"y":2}"#);
    }
}

/// Testler (`P2_TRACE_WIDGET`): düğmenin pencere içi konumunu (mantıksal px) değişince izine
/// yazar; test kendi penceresine o noktaya PostMessage ile tıklar.
fn probe(id: &'static str) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            if std::env::var_os("P2_TRACE_WIDGET").is_none() {
                return;
            }
            thread_local!(static SEEN: std::cell::RefCell<std::collections::HashMap<&'static str, (i32, i32, i32, i32, i32)>> = Default::default());
            let r = (f32::from(bounds.origin.x) as i32, f32::from(bounds.origin.y) as i32, f32::from(bounds.size.width) as i32, f32::from(bounds.size.height) as i32, (window.scale_factor() * 100.0) as i32);
            let changed = SEEN.with(|s| s.borrow_mut().insert(id, r) != Some(r));
            if changed {
                crate::trace(&format!("balon: düğme {id} {},{},{},{} ölçek {:.2}", r.0, r.1, r.2, r.3, window.scale_factor()));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}
