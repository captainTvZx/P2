//! App.tsx `ModePicker`: ilk açılışta "Bir sunucuya katıl" ya da "Bu bilgisayardaki P2 Server".

use std::time::Duration;

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, Subscription, WeakEntity, Window, div, px, relative};
use p2_client_core::api::resolve_server;
use p2_ui::components::error_box;
use p2_ui::text::normal_line_height;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::setup_ui::{logo_pop, setup_header, setup_page, setup_shell, shell_enter};
use super::{submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

pub struct ModeScreen {
    app: WeakEntity<ClientApp>,
    client_form: bool,
    address: Entity<InputState>,
    busy: bool,
    error: Option<(String, usize)>,
    _subs: Vec<Subscription>,
}

impl ModeScreen {
    pub fn new(app: WeakEntity<ClientApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address = text_input(window, cx, "https://sunucu.ornek.com ya da http://192.168.1.20:4000");
        let subs = vec![submit_on_enter(&address, window, cx, |this, window, cx| this.connect(window, cx))];
        Self { app, client_form: false, address, busy: false, error: None, _subs: subs }
    }

    fn set_error(&mut self, message: &str, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message.to_string(), epoch));
        cx.notify();
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let raw = value(&self.address, cx);
        if raw.trim().is_empty() {
            self.set_error("Sunucu adresini yaz.", cx);
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        task::run(window, cx, move || resolve_server(&raw), |this, found, window, cx| {
            this.busy = false;
            match found {
                Some(url) => {
                    let app = this.app.clone();
                    let _ = app.update(cx, |app, cx| app.connected(url, window, cx));
                }
                None => this.set_error("Bu adreste P2 sunucusu bulunamadı. Adresi ve sunucunun açık olduğunu kontrol et.", cx),
            }
        });
    }

    /// Otomatik pilot (yalnız `P2_AUTOPILOT_SERVER` ile): adresi yazıp "Bağlan".
    pub(crate) fn autopilot(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.client_form = true;
        let url = url.to_string();
        self.address.update(cx, |s, cx| s.set_value(url, window, cx));
        self.connect(window, cx);
    }

    fn choose_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let _ = app.update(cx, |app, cx| app.connected("http://127.0.0.1:4000".into(), window, cx));
    }
}

/// `.mode-choices button`: p 14 16, gap 4, r 14, kenarlık `border`, `bg-3`; hover `border-strong` + `bg-hover`
/// + 2px yukarı + `shadow-md`; `p2-rise` (ikincisi 60ms gecikmeli).
fn choice(id: &'static str, title: &'static str, body: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (hover_border, hover_bg, hover_shadow) = (p.border_strong, p.bg_hover, p.shadow_md());
    let (b, small) = (fs(13.5), fs(11.));
    div()
        .id(id)
        .relative()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(16.))
        .py(px(14.))
        .rounded(radius::LG)
        .border_1()
        .border_color(p.border)
        .bg(p.bg_3)
        .text_color(p.text)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).border_color(hover_border).top(px(-2.)).shadow(hover_shadow.clone()))
        .child(div().h(normal_line_height(b)).line_height(normal_line_height(b)).text_size(b).text_color(p.text_strong).font_weight(FontWeight::BOLD).child(title))
        .child(div().text_size(small).line_height(relative(1.45)).text_color(p.text_3).child(body))
}

impl Render for ModeScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let hint_size = fs(11.);
        let hint = |text: &'static str| div().text_size(hint_size).line_height(relative(1.5)).text_color(p.text_3).child(text);

        let shell = setup_shell("mode-shine", 560., 34., 40., &p);
        let shell = if !self.client_form {
            shell
                .child(setup_header(logo_pop("mode-logo", &p), "HOŞ GELDİN", "Nasıl başlamak istersin?", &p))
                .child(
                    div()
                        .mt(px(20.))
                        .mb(px(14.))
                        .flex()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .flex_basis(px(0.))
                                .min_w_0()
                                .flex()
                                .child(choice("join", "Bir sunucuya katıl", "Ekibinin sunucusu zaten varsa adresini yaz ve giriş yap. Kurulum yok.", &p).on_click(cx.listener(|this, _, window, cx| {
                                    this.client_form = true;
                                    this.address.update(cx, |s, cx| s.focus(window, cx));
                                    cx.notify();
                                })))
                                .p2_enter("choice-join", Entrance::Rise, Duration::ZERO),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex_basis(px(0.))
                                .min_w_0()
                                .flex()
                                .child(choice("local", "Bu bilgisayardaki P2 Server", "Ayrı P2 Server uygulaması bu bilgisayarda kuruluysa yerel sunucuya bağlan.", &p).on_click(cx.listener(|this, _, window, cx| this.choose_local(window, cx))))
                                .p2_enter("choice-local", Entrance::Rise, Duration::from_millis(60)),
                        ),
                )
                .child(hint("Seçimini sonradan Ayarlar > Uygulama > Sunucu adresi bölümünden değiştirebilirsin."))
        } else {
            let label = fs(11.5);
            let focused = gpui_kit::Focusable::focus_handle(self.address.read(cx), cx).is_focused(window);
            let address = gpui_kit::component::input::Input::new(&self.address)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false);
            let address = Styled::h(address, px(36.))
                .px(px(11.))
                .py(px(0.))
                .rounded(radius::SM)
                .bg(p.bg_3)
                .border_1()
                .text_size(fs(12.))
                .text_color(p.text)
                .map(|s| if focused { s.border_color(p.border_accent).shadow(p2_ui::tokens::shadows(&[p.ring])) } else { s.border_color(p.border) });
            let back_hover = (p.bg_hover, p.text_strong);
            let busy = self.busy;
            shell
                .child(setup_header(logo_pop("mode-logo", &p), "BİR SUNUCUYA KATIL", "Sunucu adresini yaz", &p))
                .child(
                    div()
                        .mt(px(24.))
                        .flex()
                        .flex_col()
                        .gap(px(20.))
                        .child(div().flex().flex_col().gap(px(6.)).mt(px(18.)).mb(px(8.)).text_size(label).text_color(p.text_2).child("Sunucu adresi").child(address))
                        .child(hint("Sunucuyu kuran kişinin paylaştığı adres. Bu bilgisayara hiçbir şey kurulmaz; sadece bağlanır."))
                        .when_some(self.error.clone(), |s, (m, e)| s.child(error_box(m, e, &p)))
                        .child(
                            div()
                                .mt(px(16.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .id("back")
                                        .h(px(38.))
                                        .px(px(16.))
                                        .flex()
                                        .items_center()
                                        .border_1()
                                        .border_color(p.border)
                                        .rounded(radius::SM)
                                        .text_size(fs(12.))
                                        .text_color(p.text_2)
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(back_hover.0).text_color(back_hover.1))
                                        .child("Geri")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if !this.busy {
                                                this.client_form = false;
                                                this.error = None;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    div()
                                        .id("connect")
                                        .relative()
                                        .h(px(38.))
                                        .px(px(16.))
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .rounded(radius::MD)
                                        .bg(p.accent_grad.background())
                                        .shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong, p.inset_top]))
                                        .text_color(p.text_on_accent)
                                        .text_size(fs(12.5))
                                        .font_weight(FontWeight::BOLD)
                                        .when(busy, |s| s.opacity(0.65))
                                        .when(!busy, |s| s.cursor_pointer().hover(|s| s.top(px(-1.))))
                                        .child(if busy { "Bağlanılıyor…" } else { "Bağlan" })
                                        .child("→")
                                        .on_click(cx.listener(|this, _, window, cx| this.connect(window, cx))),
                                ),
                        ),
                )
        };
        let tint = p.accent_soft;
        setup_page(window, &p, (0.5, 0.2), tint, 0.55, shell_enter(shell, ("mode", self.client_form as usize)))
    }
}
