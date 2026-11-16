//! App.tsx `AdminSetup`: sunucuda henüz yönetici yoksa ilk yönetici hesabı.

use std::time::Duration;

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, Subscription, WeakEntity, Window, div, px};
use p2_client_core::{Api, RegisterInput};
use p2_i18n::t;
use p2_ui::components::{error_box, password_meter, password_strength};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::setup_ui::{logo_pop, mode_back, setup_header, setup_page, setup_shell, shell_enter};
use super::{password_input, submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

pub struct SetupAdminScreen {
    app: WeakEntity<ClientApp>,
    email: Entity<InputState>,
    username: Entity<InputState>,
    display_name: Entity<InputState>,
    password: Entity<InputState>,
    confirm: Entity<InputState>,
    busy: bool,
    error: Option<(String, usize)>,
    _subs: Vec<Subscription>,
}

impl SetupAdminScreen {
    pub fn new(app: WeakEntity<ClientApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // AdminSetup: alanlarda yer tutucu ve otomatik odak yok.
        let email = text_input(window, cx, "");
        let username = text_input(window, cx, "");
        let display_name = text_input(window, cx, "");
        let password = password_input(window, cx);
        let confirm = password_input(window, cx);
        let mut subs = Vec::new();
        for input in [&email, &username, &display_name, &password, &confirm] {
            subs.push(submit_on_enter(input, window, cx, |this, window, cx| this.submit(window, cx)));
            subs.push(cx.observe(input, |_, _, cx| cx.notify()));
        }
        Self { app, email, username, display_name, password, confirm, busy: false, error: None, _subs: subs }
    }

    fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message, epoch));
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let password = value(&self.password, cx);
        let confirm = value(&self.confirm, cx);
        if !password_strength(&password).ok {
            return self.set_error(t("setup.passwordRule"), cx);
        }
        if password != confirm {
            return self.set_error(t("auth.err.mismatch"), cx);
        }
        let input = RegisterInput {
            email: value(&self.email, cx),
            username: value(&self.username, cx),
            display_name: value(&self.display_name, cx),
            password,
            password_confirm: confirm,
        };
        let Some(api) = self.app.upgrade().and_then(|a| a.read(cx).api.clone()) else { return };
        self.busy = true;
        self.error = None;
        cx.notify();
        task::run(window, cx, move || (api.setup_admin(&input), api), |this, (result, _api): (Result<_, _>, std::sync::Arc<Api>), window, cx| {
            this.busy = false;
            match result {
                Ok(auth) => {
                    let app = this.app.clone();
                    let _ = app.update(cx, |app, cx| {
                        app.first_install = true;
                        app.authenticated(auth, true, window, cx)
                    });
                }
                Err(e) => this.set_error(if e.message.is_empty() { t("setup.failed") } else { e.message }, cx),
            }
        });
    }
}

/// `.form-grid label`: 12px×0,85 600 `text-3`, gap 7; alan p 12 13, r 8, bg-3, kenarlık `border` (odak halkası).
fn grid_field(label: String, state: &Entity<InputState>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> gpui_kit::Div {
    let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
    let input = gpui_kit::component::input::Input::new(state).appearance(false).bordered(false).focus_bordered(false);
    let input = Styled::h(input, px(38.))
        .px(px(13.))
        .py(px(12.))
        .rounded(radius::SM)
        .bg(p.bg_3)
        .border_1()
        .text_size(fs(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text)
        .map(|s| if focused { s.border_color(p.border_accent).shadow(p2_ui::tokens::shadows(&[p.ring])) } else { s.border_color(p.border) });
    div().flex_1().min_w_0().flex().flex_col().gap(px(7.)).text_size(fs(12.)).font_weight(FontWeight::SEMIBOLD).text_color(p.text_3).child(label).child(input)
}

impl Render for SetupAdminScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let password = value(&self.password, cx);
        let confirm = value(&self.confirm, cx);
        // `.setup-back`: başlığın sağında dikey ortalı, h 32, p 0 12, 11,5px×0,85
        let back = mode_back("back", format!("‹ {}", t("common.back")), &p).h(px(32.)).px(px(12.)).text_size(fs(11.5)).on_click(cx.listener(|this, _, window, cx| {
            if this.busy {
                return;
            }
            let app = this.app.clone();
            let _ = app.update(cx, |app, cx| app.change_address(window, cx));
        }));
        let header = setup_header(logo_pop("setup-logo", &p), t("setup.kicker"), t("setup.title"), &p)
            // Ölçüm: düğme başlık kutusunun 18,5px altından başlar (h 32).
            .child(div().absolute().right_0().top(px(18.5)).child(back));
        // `fieldset` + `legend` (1 · Yönetici hesabı) + `.form-grid` (iki sütun, gap 14)
        // `legend` çerçevenin üst çizgisinin üstüne oturur (çizgiyi keser): zemin rengiyle örter.
        let legend = div()
            .absolute()
            .top(px(-12.5))
            .left(px(20.))
            .h(px(23.))
            .bg(p.bg_2)
            .flex()
            .items_center()
            .gap(px(7.))
            .px(px(10.))
            .text_color(p.text_strong)
            .text_size(fs(13.))
            .font_weight(FontWeight::BOLD)
            .child(div().size(px(23.)).rounded(px(7.)).flex().items_center().justify_center().bg(p.accent_grad.background()).shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong])).text_color(p.text_on_accent).child("1"))
            .child(t("setup.account"));
        // Ölçüm: fieldset kutusu legend'in ortasından (11,5px) başlar; ızgara legend altından 20px sonra.
        let fieldset = div()
            .relative()
            .mt(px(11.5))
            .px(px(20.))
            .pt(px(30.5))
            .pb(px(20.))
            .rounded(radius::LG)
            .border_1()
            .border_color(p.border)
            .bg(p.bg_2)
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(legend)
            .child(div().flex().gap(px(14.)).child(grid_field(t("auth.email"), &self.email, &p, window, cx)).child(grid_field(t("auth.username"), &self.username, &p, window, cx)))
            .child(div().flex().child(grid_field(t("auth.displayName"), &self.display_name, &p, window, cx)))
            .child(div().flex().gap(px(14.)).child(grid_field(t("auth.password"), &self.password, &p, window, cx)).child(grid_field(t("auth.confirm"), &self.confirm, &p, window, cx)))
            // `<div className="wide"><PasswordMeter/></div>` hep ızgarada: boşken 0 yükseklik + 14px aralık.
            .child(if password.is_empty() { div().into_any_element() } else { password_meter(&password, Some(&confirm), &p).into_any_element() })
            .p2_enter("fieldset", Entrance::Rise, Duration::ZERO);
        let busy = self.busy;
        let submit = div()
            .id("submit")
            .relative()
            .h(px(46.))
            .rounded(radius::MD)
            .bg(p.accent_grad.background())
            .shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong, p.inset_top]))
            .text_color(p.text_on_accent)
            .font_weight(FontWeight::EXTRA_BOLD)
            .line_height(px(18.))
            .when(busy, |s| s.opacity(0.65))
            .when(!busy, |s| s.cursor_pointer().hover(|s| s.top(px(-1.))))
            .child(div().absolute().top(px(14.)).left(px(18.)).right(px(30.)).flex().justify_center().child(t(if busy { "setup.creating" } else { "setup.finish" })))
            .child(div().absolute().top(px(14.)).right(px(18.)).child("→"))
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));
        let shell = setup_shell("setup-shine", 760., 34., 40., &p).child(header).child(
            div()
                .mt(px(24.))
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(fieldset)
                .child(div().text_size(fs(11.5)).line_height(gpui_kit::relative(1.55)).text_color(p.text_3).child(t("setup.consoleHint")))
                .when_some(self.error.clone(), |s, (m, e)| s.child(error_box(m, e, &p)))
                .child(submit),
        );
        setup_page(window, &p, (0.5, 0.2), p.accent_soft, 0.55, shell_enter(shell, ("setup", 0)))
    }
}
