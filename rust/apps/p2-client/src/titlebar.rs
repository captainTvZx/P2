//! Başlık çubuğu — Titlebar.tsx + titlebar.css birebir: 38px, `bg-2 → bg-1` dikey gradyan, alt kenarlık
//! `border-subtle`, `inset-top`; solda ☰ menü düğmesi (34×30) + sayfa başlığı, sağda 46px pencere düğmeleri.
//! ☰ açılınca çizgiler X olur (220ms ease-pop) ve uygulama menüsü (`.app-menu`, p2-scale-in) açılır.
//! Sürükleme/çift tıklama/pencere düğmeleri Windows isabet testiyle (`WindowControlArea`): yerel davranış.

use std::time::Duration;
use p2_ui::P2Fx as _; // backdrop_blur

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{Animation, AnyElement, Context, MouseButton, Transformation, Window, WindowControlArea, div, px, radians};
use p2_ui::P2Text as _;
use p2_ui::motion::{EASE_POP, Entrance, M_BASE, P2Motion as _};
use p2_ui::tokens::{Palette, font_size, radius};

use crate::app::ClientApp;

pub const HEIGHT: f32 = 38.;

static PROJECT_TITLE: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);

/// Çalışma alanı seçili proje değişince çağırır (başlık çubuğu sayfa adı = proje adı). Adlar sızdırılarak
/// `'static` yapılır; aynı ad tekrar gelirse yeniden ayrılmaz.
pub fn set_project_title(name: &str) {
    let mut slot = PROJECT_TITLE.lock().unwrap();
    if slot.is_some_and(|s| s == name) {
        return;
    }
    *slot = Some(Box::leak(name.to_string().into_boxed_str()));
}

fn project_title() -> &'static str {
    PROJECT_TITLE.lock().unwrap().unwrap_or("Proje")
}

/// Uygulama menüsündeki sürüm (`window.p2.version` = Electron `app.getVersion()`): ürün sürümü.
pub fn display_version() -> &'static str {
    crate::updates::product_version()
}

/// ☰ çizgisi: 13×1, `text-3` (hover/açık: `text-strong`). 13×13 SVG kutusunun ortasında çizilir ki
/// döndürme merkezi CSS'teki `transform-origin: center` ile aynı olsun.
fn menu_line(top: f32, angle: f32, scale_x: f32, opacity: f32, color: gpui_kit::Hsla, hot: gpui_kit::Hsla) -> AnyElement {
    let markup: &'static [u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 13 13"><rect x="0" y="6" width="13" height="1" fill="black"/></svg>"#;
    gpui_kit::svg()
        .data(markup)
        .absolute()
        .left(px(9.5 + 6.5 * (1. - scale_x)))
        .top(px(top - 6.))
        .w(px(13. * scale_x.max(0.001)))
        .h(px(13.))
        .opacity(opacity)
        .text_color(color)
        .group_hover("menu-trigger", move |s| s.text_color(hot))
        .with_transformation(Transformation::rotate(radians(angle)))
        .into_any_element()
}

impl ClientApp {
    pub(crate) fn page_title(&self, cx: &gpui_kit::App) -> &'static str {
        use crate::app::Screen;
        match &self.screen {
            // Titlebar.tsx: `.mode-shell` (sunucu sorunu kartı da bu sınıfı taşır) → "Kurulum Türü"
            Screen::Mode(_) | Screen::Problem(_) => "Kurulum Türü",
            Screen::Setup(_) => "Kurulum",
            Screen::Auth(a) => {
                if a.read(cx).is_register() {
                    "Hesap Oluştur"
                } else {
                    "Giriş"
                }
            }
            Screen::Onboarding(_) => "Kuruluş",
            // Workspace.tsx: `.project-blank[data-project-title]` = seçili proje adı (yoksa "Proje").
            Screen::Workspace(_) => return project_title(),
            Screen::Loading(_) => "P2",
        }
    }

    pub(crate) fn toggle_app_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        self.menu_epoch += 1;
        cx.notify();
    }

    pub(crate) fn titlebar(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let open = self.menu_open;
        let animate = self.menu_epoch > 0;
        let title = self.page_title(cx);
        let line_rest = p.text_3;
        let line_hot = p.text_strong;

        // ☰ → X: 220ms ease-pop (CSS `transform var(--m-base) var(--ease-pop)`).
        let lines = div()
            .id("menu-lines")
            .absolute()
            .inset_0()
            .with_animation(("menu-lines", self.menu_epoch), Animation::new(if animate { M_BASE } else { Duration::from_millis(1) }), move |el, t| {
                let e = EASE_POP.ease(t);
                let k = if open { e } else { 1. - e };
                let color = if open { line_hot } else { line_rest };
                let deg = std::f32::consts::PI / 4.;
                el.child(menu_line(10. + 4. * k, deg * k, 1., 1., color, line_hot))
                    .child(menu_line(14., 0., 1. - k, 1. - k, color, line_hot))
                    .child(menu_line(18. - 4. * k, -deg * k, 1., 1., color, line_hot))
            });

        let trigger = div()
            .id("menu-trigger")
            .group("menu-trigger")
            .relative()
            .w(px(34.))
            .h(px(30.))
            .rounded(radius::XS)
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .cursor_pointer()
            .when(open, |s| s.bg(p.accent_soft).border_color(p.border))
            .hover(|s| s.bg(p.accent_soft).border_color(p.border))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle_app_menu(cx)))
            .child(lines);

        let navigation = div()
            .relative()
            .h_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(trigger)
            .child(div().text_color(p.text_2).text_size(font_size::SM).css_weight(650.).tracking(0.025).child(title))
            .when(open, |s| {
                // Titlebar.tsx: menü dışına (düğme hariç) basılınca kapanır.
                s.child(self.app_menu(p, cx)).on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    if this.menu_open {
                        this.toggle_app_menu(cx);
                    }
                }))
            });

        let control = |id: &'static str, area: WindowControlArea, icon: &'static str, close: bool| {
            let (hover_bg, hover_fg) = if close { (p.danger, gpui_kit::white()) } else { (p.bg_hover, p.text_strong) };
            div()
                .id(id)
                .w(px(46.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .group(id)
                .window_control_area(area)
                .hover(move |s| s.bg(hover_bg))
                // GPUI svg rengi kalıtılmaz: renk ve hover rengi svg'nin kendisinde.
                .child(p2_ui::glyphs::ui(icon, 10.).text_color(p.text_3).group_hover(id, move |s| s.text_color(hover_fg)))
        };
        let maximized = window.is_maximized();
        let controls = div()
            .h_full()
            .flex()
            .child(control("win-min", WindowControlArea::Min, "win-min", false))
            .child(control("win-max", WindowControlArea::Max, if maximized { "win-restore" } else { "win-max" }, false))
            .child(control("win-close", WindowControlArea::Close, "win-close", true));

        div()
            .id("titlebar")
            .relative()
            .flex_shrink_0()
            .h(px(HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(12.))
            .bg(gpui_kit::linear_gradient(180., gpui_kit::linear_color_stop(p.bg_2, 0.), gpui_kit::linear_color_stop(p.bg_1, 1.)))
            .border_b_1()
            .border_color(p.border_subtle)
            .shadow(p2_ui::tokens::shadows(&[p.inset_top]))
            .child(div().id("titlebar-drag").absolute().inset_0().window_control_area(WindowControlArea::Drag))
            .child(navigation)
            .child(controls)
            .into_any_element()
    }

    /// `.app-menu`: 220px cam kart (üstte "P2 Desktop" + sürüm), "Sayfayı yenile", "Uygulamadan çık".
    fn app_menu(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let item = |id: &'static str, label: &'static str, quit: bool| {
            let (hover_bg, hover_fg) = if quit { (p.danger_soft, p.danger_text) } else { (p.accent_soft, p.text_strong) };
            div()
                .id(id)
                .w_full()
                .h(px(32.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .rounded(radius::XS)
                .text_color(if quit { p.danger_text } else { p.text })
                .text_size(font_size::MD)
                .css_weight(550.)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
                .child(label)
        };
        let card = div()
            .id("app-menu")
            .w(px(220.))
            .p(px(6.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::MD)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(8.))
                    .px(px(10.))
                    .pb(px(10.))
                    .mb(px(4.))
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(div().text_color(p.text_strong).text_size(font_size::SM).font_weight(gpui_kit::FontWeight::BOLD).child("P2 Desktop"))
                    .child(div().text_color(p.text_4).text_size(font_size::XXS).font_weight(gpui_kit::FontWeight::BOLD).tracking(0.06).child(format!("v{}", display_version()))),
            )
            .child(item("menu-reload", "Sayfayı yenile", false).on_click(cx.listener(|this, _, window, cx| {
                this.menu_open = false;
                this.reload(window, cx);
            })))
            .child(item("menu-quit", "Uygulamadan çık", true).on_click(cx.listener(|this, _, window, cx| {
                this.menu_open = false;
                cx.notify();
                if this.should_close(window, cx) {
                    window.remove_window();
                }
            })));
        // Sonra çizilsin (gövdenin üstünde kalır; CSS z-index 1200).
        gpui_kit::deferred(div().absolute().top(px(35.)).left(px(0.)).child(card.p2_enter(("app-menu", self.menu_epoch), Entrance::ScaleIn, Duration::ZERO)))
            .with_priority(p2_ui::tokens::z::MENU)
            .into_any_element()
    }
}
