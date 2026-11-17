//! Arama balonu — main.cjs `syncCallWidget` / `WIDGET_HTML` / `askWidgetClose` karşılığı.
//! Aramadayken ana pencere simge durumuna küçültülür ya da tepsiye gizlenirse ekranın sağ
//! altında küçük, her zaman üstte bir kontrol penceresi belirir: arama adı, süre ve kişi
//! sayısı; mikrofon, kulaklık, "P2'yi aç", ayrıl düğmeleri ve × (bu seferlik gizle / bir daha
//! gösterme). Sürükleyerek taşınan konum ve "bir daha gösterme" `call-widget.json`'da
//! (Electron'la aynı biçim: `{x, y, hidden}`, mantıksal piksel).

use std::time::{Duration, Instant};

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, FontWeight, Hsla, PathBuilder, Subscription, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, canvas, div, linear_color_stop, linear_gradient, point, px, rgb, rgba, size};
use p2_ui::{Entrance, P2Motion as _};

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::call::CallController;

pub const TITLE: &str = "P2 arama balonu";
/// Pencere boyutu (main.cjs: 404×126; balon üstte 47 px boşlukla — × düğmesi için).
const W: f32 = 404.;
const H: f32 = 126.;

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
const BG_2: u32 = 0x0e1626;
const BG_3: u32 = 0x121c30;
const BG_HOVER: u32 = 0x182440;
const BORDER: u32 = 0x78aaff2e; // rgba(120,170,255,.18)
const BORDER_STRONG: u32 = 0x60aaff61; // rgba(96,170,255,.38)
const TEXT_2: u32 = 0xd6e4ffc7; // rgba(214,228,255,.78)
const SUCCESS: u32 = 0x3ddc84;
const DANGER: u32 = 0xff5c6c;
const DANGER_SOFT: u32 = 0xff5c6c29; // .16
const DANGER_BORDER: u32 = 0xff5c6c6b; // .42
const DANGER_TEXT: u32 = 0xff9aa4;

// ---------------------------------------------------------------- simgeler (WIDGET_HTML SVG yolları)

const MIC: &str = "M12 14.2a3.4 3.4 0 0 0 3.4-3.4V6.2a3.4 3.4 0 1 0-6.8 0v4.6a3.4 3.4 0 0 0 3.4 3.4Z";
const MIC_STAND: &str = "M18.2 10.6a.9.9 0 0 0-1.8 0 4.4 4.4 0 0 1-8.8 0 .9.9 0 0 0-1.8 0 6.2 6.2 0 0 0 5.3 6.1v2.1H9.4a.9.9 0 1 0 0 1.8h5.2a.9.9 0 1 0 0-1.8h-1.7v-2.1a6.2 6.2 0 0 0 5.3-6.1Z";
const SLASH: &str = "M4.6 3.5a.9.9 0 0 0-1.2 1.4l16 14a.9.9 0 0 0 1.2-1.4l-16-14Z";
const HEADPHONES: &str = "M12 3.2A9.2 9.2 0 0 0 2.8 12.4v4.4a3.2 3.2 0 0 0 3.2 3.2h1.1a1.3 1.3 0 0 0 1.3-1.3v-5.3a1.3 1.3 0 0 0-1.3-1.3H4.9v-.7a7.1 7.1 0 0 1 14.2 0v.7h-2.2a1.3 1.3 0 0 0-1.3 1.3v5.3a1.3 1.3 0 0 0 1.3 1.3H18a3.2 3.2 0 0 0 3.2-3.2v-4.4A9.2 9.2 0 0 0 12 3.2Z";
const OPEN_A: &str = "M4 4h7v2H6v5H4V4Zm16 16h-7v-2h5v-5h2v7Z";
const OPEN_B: &str = "M4 20v-7h2v5h5v2H4ZM20 4v7h-2V6h-5V4h7Z";
const HANGUP: &str = "M12 7.4c-3.3 0-6.4.9-8.9 2.5-.9.6-1.2 1.8-.7 2.7l1 1.8c.5.9 1.7 1.3 2.6.8l2.2-1.1c.6-.3 1-1 1-1.7v-1c1.8-.5 3.8-.5 5.6 0v1c0 .7.4 1.4 1 1.7l2.2 1.1c1 .5 2.1.1 2.6-.8l1-1.8c.5-.9.2-2.1-.7-2.7A16.4 16.4 0 0 0 12 7.4Z";

/// Küçük SVG yol çözücü (M L H V C A Z, büyük/küçük harf, örtük tekrar). 24'lük görünüm kutusu
/// `scale` ile büyütülür, `rotate` derece ile (12,12) etrafında döndürülür.
pub fn svg_path(d: &str, origin: gpui_kit::Point<gpui_kit::Pixels>, scale: f32, rotate: f32) -> Option<gpui_kit::Path<gpui_kit::Pixels>> {
    let (sin, cos) = rotate.to_radians().sin_cos();
    let map = |x: f32, y: f32| {
        let (dx, dy) = (x - 12.0, y - 12.0);
        let (rx, ry) = (dx * cos - dy * sin + 12.0, dx * sin + dy * cos + 12.0);
        point(origin.x + px(rx * scale), origin.y + px(ry * scale))
    };
    // Belirteçler: komut harfleri ve sayılar ("1.4-1.4", ".9.9" biçimleri dahil).
    let mut tokens: Vec<Result<char, f32>> = Vec::new();
    let bytes: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch.is_ascii_alphabetic() {
            tokens.push(Ok(ch));
            i += 1;
        } else if ch == '-' || ch == '.' || ch.is_ascii_digit() {
            let start = i;
            let mut dot = false;
            if ch == '-' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == '.' && !dot)) {
                if bytes[i] == '.' {
                    dot = true;
                }
                i += 1;
            }
            tokens.push(Err(bytes[start..i].iter().collect::<String>().parse().ok()?));
        } else {
            i += 1;
        }
    }
    let mut b = PathBuilder::fill();
    let (mut cx, mut cy, mut sx, mut sy) = (0f32, 0f32, 0f32, 0f32);
    let mut cmd = 'M';
    let mut k = 0;
    let num = |k: &mut usize| -> Option<f32> {
        match tokens.get(*k) {
            Some(Err(v)) => {
                *k += 1;
                Some(*v)
            }
            _ => None,
        }
    };
    while k < tokens.len() {
        if let Some(Ok(ch)) = tokens.get(k) {
            cmd = *ch;
            k += 1;
            if cmd == 'Z' || cmd == 'z' {
                b.close();
                cx = sx;
                cy = sy;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let (x, y) = (num(&mut k)? + ox, num(&mut k)? + oy);
                b.move_to(map(x, y));
                (cx, cy, sx, sy) = (x, y, x, y);
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let (x, y) = (num(&mut k)? + ox, num(&mut k)? + oy);
                b.line_to(map(x, y));
                (cx, cy) = (x, y);
            }
            'H' => {
                cx = num(&mut k)? + ox;
                b.line_to(map(cx, cy));
            }
            'V' => {
                cy = num(&mut k)? + oy;
                b.line_to(map(cx, cy));
            }
            'C' => {
                let (x1, y1, x2, y2, x, y) = (num(&mut k)? + ox, num(&mut k)? + oy, num(&mut k)? + ox, num(&mut k)? + oy, num(&mut k)? + ox, num(&mut k)? + oy);
                b.cubic_bezier_to(map(x, y), map(x1, y1), map(x2, y2));
                (cx, cy) = (x, y);
            }
            'A' => {
                let (rx, ry, rot, large, sweep) = (num(&mut k)?, num(&mut k)?, num(&mut k)?, num(&mut k)? != 0.0, num(&mut k)? != 0.0);
                let (x, y) = (num(&mut k)? + ox, num(&mut k)? + oy);
                b.arc_to(point(px(rx * scale), px(ry * scale)), px(rot + rotate), large, sweep, map(x, y));
                (cx, cy) = (x, y);
            }
            _ => return None,
        }
    }
    b.build().ok()
}

fn icon(paths: Vec<&'static str>, ink: Hsla, rotate: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window: &mut Window, _| {
            let scale = f32::from(bounds.size.width) / 24.0;
            for d in &paths {
                if let Some(p) = svg_path(d, bounds.origin, scale, rotate) {
                    window.paint_path(p, ink);
                }
            }
        },
    )
    .size(px(17.))
    .flex_shrink_0()
}

// ---------------------------------------------------------------- pencere

pub struct CallWidget {
    calls: Entity<CallController>,
    asking: bool,
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
        Self { calls, asking: false, _observe: observe }
    }

    fn act(&mut self, action: &'static str, cx: &mut Context<Self>) {
        crate::trace(&format!("balon: {action}"));
        if action == "close" {
            self.asking = true;
            cx.notify();
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

    fn answer(&mut self, choice: &'static str, cx: &mut Context<Self>) {
        crate::trace(&format!("balon: kapatma seçimi {choice}"));
        self.asking = false;
        match choice {
            "once" => self.calls.update(cx, |c, cx| c.widget_skip_this_call(cx)),
            "never" => {
                save_state(|s| s.hidden = true);
                self.calls.update(cx, |c, cx| c.sync_widget(cx));
            }
            _ => cx.notify(),
        }
    }
}

fn button(id: &'static str, tip: String, on: bool, danger: bool) -> gpui_kit::Stateful<gpui_kit::Div> {
    let tip = gpui_kit::SharedString::from(tip);
    let (border, bg, fg) = if on || danger { (ca(DANGER_BORDER), ca(DANGER_SOFT), c(DANGER_TEXT)) } else { (ca(BORDER), c(BG_3), ca(TEXT_2)) };
    div()
        .id(id)
        .h(px(32.))
        .min_w(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(border)
        .rounded(px(8.))
        .bg(bg)
        .text_color(fg)
        .cursor_pointer()
        .hover(move |s| if danger { s.bg(c(DANGER)).border_color(c(DANGER)).text_color(gpui_kit::white()) } else if on { s } else { s.bg(c(BG_HOVER)).border_color(ca(BORDER_STRONG)).text_color(c(TEXT_STRONG)) })
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
        let (label, meta, muted, deafened) = match &calls.call {
            Some(call) => {
                let label = if call.kind == "channel" { p2_i18n::t("cwid.voiceRoom") } else { p2_i18n::t("cwid.voiceCall") };
                let ringing = calls.connected_at.is_none() && call.kind == "direct";
                let meta = if ringing {
                    p2_i18n::tv("cwid.ringing", &[("time", &calls.started_at.map(clock).unwrap_or_else(|| "00:00".into()))])
                } else {
                    p2_i18n::tv("cwid.durationPeople", &[("time", &calls.connected_at.map(clock).unwrap_or_else(|| "00:00".into())), ("n", &call.participants.len().to_string())])
                };
                (label, meta, calls.muted, calls.deafened)
            }
            None => (p2_i18n::t("cwid.voiceCall"), "00:00".to_string(), false, false),
        };
        let ink_of = |on: bool| if on { c(DANGER_TEXT) } else { ca(TEXT_2) };
        let mute_btn = button("w-mute", if muted { p2_i18n::t("cwid.unmute") } else { p2_i18n::t("cwid.mute") }, muted, false)
            .px(px(9.))
            .gap(px(6.))
            .child(icon(if muted { vec![MIC, MIC_STAND, SLASH] } else { vec![MIC, MIC_STAND] }, ink_of(muted), 0.0))
            .child(div().text_size(px(9.)).font_weight(FontWeight::SEMIBOLD).child(if muted { p2_i18n::t("cwid.turnOn") } else { p2_i18n::t("cwid.turnOff") }))
            .on_click(cx.listener(|this, _, _, cx| this.act("mute", cx)));
        let deaf_btn = button("w-deafen", if deafened { p2_i18n::t("cwid.soundOn") } else { p2_i18n::t("cwid.soundOff") }, deafened, false)
            .px(px(9.))
            .gap(px(6.))
            .child(icon(if deafened { vec![HEADPHONES, SLASH] } else { vec![HEADPHONES] }, ink_of(deafened), 0.0))
            .child(div().text_size(px(9.)).font_weight(FontWeight::SEMIBOLD).child(if deafened { p2_i18n::t("cwid.turnOn") } else { p2_i18n::t("cwid.turnOff") }))
            .on_click(cx.listener(|this, _, _, cx| this.act("deafen", cx)));
        let open_btn = button("w-open", p2_i18n::t("cwid.openApp"), false, false).w(px(32.)).child(icon(vec![OPEN_A, OPEN_B], ca(TEXT_2), 0.0)).on_click(cx.listener(|this, _, _, cx| this.act("open", cx)));
        let leave_btn = button("w-leave", p2_i18n::t("cwid.leave"), false, true).w(px(32.)).child(icon(vec![HANGUP], c(DANGER_TEXT), 133.0)).on_click(cx.listener(|this, _, _, cx| this.act("leave", cx)));

        let body: AnyElement = if self.asking {
            // askWidgetClose: "Bu seferlik gizle" / "Bir daha gösterme" / "Vazgeç".
            let choice = |id: &'static str, text: String, primary: bool| {
                div()
                    .id(id)
                    .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .relative()
                    .child(probe(id))
                    .h(px(24.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(if primary { ca(BORDER_STRONG) } else { ca(BORDER) })
                    .rounded(px(8.))
                    .bg(c(BG_3))
                    .text_color(if primary { c(TEXT_STRONG) } else { ca(TEXT_2) })
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|s| s.bg(c(BG_HOVER)).text_color(c(TEXT_STRONG)))
                    .child(text)
            };
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(c(TEXT_STRONG)).child(p2_i18n::t("cwid.askClose")))
                .child(
                    div()
                        .flex()
                        .gap(px(5.))
                        .child(choice("w-once", p2_i18n::t("cwid.hideOnce"), true).on_click(cx.listener(|this, _, _, cx| this.answer("once", cx))))
                        .child(choice("w-never", p2_i18n::t("cwid.never"), false).on_click(cx.listener(|this, _, _, cx| this.answer("never", cx))))
                        .child(choice("w-cancel", p2_i18n::t("act.cancel"), false).on_click(cx.listener(|this, _, _, cx| this.answer("cancel", cx)))),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(9.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(c(TEXT_STRONG)).truncate().child(label))
                        .child(div().text_size(px(10.)).text_color(c(TEXT_3)).child(meta)),
                )
                .child(div().flex().gap(px(5.)).child(mute_btn).child(deaf_btn).child(open_btn).child(leave_btn))
                .into_any_element()
        };

        div()
            .size_full()
            .relative()
            .font_family("Segoe UI")
            .child(
                div()
                    .id("w-close")
                    .child(probe("w-close"))
                    .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .absolute()
                    .top(px(0.))
                    .right(px(4.))
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .pb(px(1.))
                    .border_1()
                    .border_color(ca(BORDER))
                    .rounded(px(6.))
                    .bg(c(BG_2))
                    .text_color(c(TEXT_3))
                    .text_size(px(14.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(BG_HOVER)).text_color(c(TEXT_STRONG)))
                    .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(p2_i18n::t("cwid.closeBubble")).build(window, cx))
                    .child("×")
                    .on_click(cx.listener(|this, _, _, cx| this.act("close", cx))),
            )
            .child(
                div()
                    .absolute()
                    .top(px(47.))
                    .left(px(15.))
                    .right(px(15.))
                    .h(px(64.))
                    .child(
                div()
                    .id("w-bubble")
                    .size_full()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .border_1()
                    .border_color(ca(BORDER_STRONG))
                    .rounded(px(14.))
                    .bg(linear_gradient(135., linear_color_stop(ca(0x0e1626f5), 0.), linear_color_stop(ca(0x080c16f0), 1.)))
                    .shadow_lg()
                    // Balonun boş yerinden sürüklenir (-webkit-app-region: drag).
                    .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, _| drag(window))
                    .p2_enter("call-widget", Entrance::PopElastic, Duration::ZERO)
                    // `.dot`: 9px, `box-shadow: 0 0 8px success`, `p2-pulse 2.2s` (opaklık 1 → .55 → 1).
                    .child(
                        div()
                            .w(px(10.))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .size(px(9.))
                                    .rounded_full()
                                    .bg(c(SUCCESS))
                                    .shadow(vec![gpui_kit::BoxShadow { color: c(SUCCESS), offset: point(px(0.), px(0.)), blur_radius: px(8.), spread_radius: px(0.), inset: false }])
                                    .with_animation("w-dot-pulse", gpui_kit::Animation::new(Duration::from_millis(2200)).repeat(), |el, t| {
                                        el.opacity(1. - 0.45 * (1. - (t * std::f32::consts::TAU).cos()) / 2.)
                                    }),
                            ),
                    )
                    .child(body),
                    ),
            )
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
        window_background: WindowBackgroundAppearance::Transparent,
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
            let probe = RECT { left: px_, top: py_, right: px_ + (380. * scale) as i32, bottom: py_ + (80. * scale) as i32 };
            let on_screen = unsafe { !windows::Win32::Graphics::Gdi::MonitorFromRect(&probe, windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONULL).is_invalid() };
            on_screen.then_some((px_, py_))
        }
        _ => None,
    };
    let (x, y) = saved.unwrap_or((work.right - (424. * scale) as i32, work.bottom - (148. * scale) as i32));
    unsafe {
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
    fn svg_paths_parse() {
        for d in [MIC, MIC_STAND, SLASH, HEADPHONES, OPEN_A, OPEN_B, HANGUP] {
            assert!(svg_path(d, point(px(0.), px(0.)), 1.0, 0.0).is_some(), "{d}");
        }
        assert!(svg_path(HANGUP, point(px(0.), px(0.)), 17. / 24., 133.).is_some());
    }

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
