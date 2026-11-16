//! App.tsx `AuthScreen`: giriş / kayıt, iki adımlı doğrulama, "Beni hatırla".

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::InputState;
use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, Subscription, WeakEntity, Window, div, px};
use p2_client_core::{Api, ApiError, Auth, RegisterInput};
use p2_ui::P2Text as _;
use p2_ui::components::{glass_card, logo_image, password_strength, radial_background};
use p2_ui::form::{self, InputKind, auth_label, native_checkbox};
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{KICKER_TRACKING_EM, fs, radius, shadows};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::font_size;
use p2_ui::{Entrance, P2Motion as _};

use super::{password_input, submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

pub struct AuthScreen {
    app: WeakEntity<ClientApp>,
    api: Arc<Api>,
    register: bool,
    two_factor: bool,
    remember: bool,
    busy: bool,
    error: Option<(String, usize)>,
    identifier: Entity<InputState>,
    password: Entity<InputState>,
    email: Entity<InputState>,
    username: Entity<InputState>,
    display_name: Entity<InputState>,
    confirm: Entity<InputState>,
    code: Entity<InputState>,
    /// Kayıt: isteğe bağlı proje davet kodu (hesap açılınca o projeye katılınır).
    invite: Entity<InputState>,
    _subs: Vec<Subscription>,
}

const PASSWORD_RULE: &str = "En az 8 karakter; en az bir büyük harf, bir küçük harf ve bir rakam içermeli.";

impl AuthScreen {
    pub fn new(app: WeakEntity<ClientApp>, api: Arc<Api>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let identifier = text_input(window, cx, "");
        let password = password_input(window, cx);
        let email = text_input(window, cx, "");
        let username = text_input(window, cx, "");
        let display_name = text_input(window, cx, "");
        let confirm = password_input(window, cx);
        let code = text_input(window, cx, "123456");
        let invite = text_input(window, cx, "P2-XXXXXXXX (isteğe bağlı)");
        let mut subs = Vec::new();
        for input in [&identifier, &password, &email, &username, &display_name, &confirm, &code, &invite] {
            subs.push(submit_on_enter(input, window, cx, |this, window, cx| this.submit(window, cx)));
            subs.push(cx.observe(input, |_, _, cx| cx.notify()));
        }
        identifier.update(cx, |s, cx| s.focus(window, cx));
        Self { app, api, register: false, two_factor: false, remember: true, busy: false, error: None, identifier, password, email, username, display_name, confirm, code, invite, _subs: subs }
    }

    pub(crate) fn is_register(&self) -> bool {
        self.register
    }

    fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message, epoch));
        cx.notify();
    }

    fn toggle_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.register = !self.register;
        self.two_factor = false;
        self.error = None;
        let first = if self.register { &self.email } else { &self.identifier };
        first.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let password = value(&self.password, cx);
        // HTML formundaki `pattern` kuralı: istemci tarafında da aynı uyarı.
        if !password_strength(&password).ok {
            return self.set_error(PASSWORD_RULE.into(), cx);
        }
        let api = self.api.clone();
        let remember = self.remember;
        self.busy = true;
        self.error = None;
        cx.notify();
        if self.register {
            let input = RegisterInput {
                email: value(&self.email, cx),
                username: value(&self.username, cx),
                display_name: value(&self.display_name, cx),
                password,
                password_confirm: value(&self.confirm, cx),
            };
            let invite = super::join_project::normalize_code(&value(&self.invite, cx));
            task::run(window, cx, move || if invite.is_empty() { api.register(&input) } else { api.register_with_invite(&input, &invite) }, move |this, r, window, cx| this.finish(r, remember, window, cx));
        } else {
            let identifier = value(&self.identifier, cx);
            let code = self.two_factor.then(|| value(&self.code, cx));
            task::run(window, cx, move || api.login(&identifier, &password, remember, code.as_deref()), move |this, r, window, cx| this.finish(r, remember, window, cx));
        }
    }

    /// Parite aracı: "register" kayıt formunu, "2fa" doğrulama kodu adımını gösterir (veri göndermez).
    pub(crate) fn parity_view(&mut self, view: &str, window: &mut Window, cx: &mut Context<Self>) {
        match view {
            "register" => {
                self.register = true;
                self.email.update(cx, |s, cx| s.focus(window, cx));
            }
            // Kayıt formu + davet kodu dolu (yalnız görünüm).
            v if v.starts_with("register:") => {
                self.register = true;
                let code = v.trim_start_matches("register:").to_string();
                self.invite.update(cx, |s, cx| s.set_value(code, window, cx));
            }
            "2fa" => {
                // Electron adımı: kimlik ve parola yazılmış, sunucu 2FA istemiş.
                self.identifier.update(cx, |s, cx| s.set_value("p2test", window, cx));
                self.password.update(cx, |s, cx| s.set_value("Parola123", window, cx));
                self.two_factor = true;
                self.code.update(cx, |s, cx| s.focus(window, cx));
            }
            _ => {}
        }
        cx.notify();
    }

    /// Otomatik pilot (yalnız `P2_AUTOPILOT_USER/PASS` ile): formu doldurup "Giriş yap".
    pub(crate) fn autopilot(&mut self, user: &str, pass: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (user, pass) = (user.to_string(), pass.to_string());
        self.identifier.update(cx, |s, cx| s.set_value(user, window, cx));
        self.password.update(cx, |s, cx| s.set_value(pass, window, cx));
        self.submit(window, cx);
    }

    fn finish(&mut self, result: Result<Auth, ApiError>, remember: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = false;
        match result {
            Ok(auth) => {
                let app = self.app.clone();
                let _ = app.update(cx, |app, cx| app.authenticated(auth, remember, window, cx));
            }
            Err(e) if e.code == "TWO_FACTOR_REQUIRED" => {
                self.two_factor = true;
                self.error = None;
                self.code.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            }
            Err(e) => self.set_error(p2_client_core::community_error_text(&e.code).map(str::to_string).unwrap_or_else(|| if e.message.is_empty() { "Bir hata oluştu.".into() } else { e.message }), cx),
        }
    }
}


/// `.auth-card::after`: 38% genişliğinde parlama şeridi, 1,1 sn ease-out, 0,4 sn gecikme;
/// `translateX(-120% → 260%)` (CSS'teki -18° eğiklik GPUI'de yok).
fn shine(card_w: f32) -> impl IntoElement {
    let band = card_w * 0.38;
    let glow = gpui_kit::rgba(0x78b4ff14);
    let clear = gpui_kit::rgba(0x78b4ff00);
    let half = move |from: gpui_kit::Rgba, to: gpui_kit::Rgba| div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(from, 0.), gpui_kit::linear_color_stop(to, 1.)));
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-band * 1.2))
        .w(px(band))
        .flex()
        .child(half(clear, glow))
        .child(half(glow, clear))
        .with_animation("auth-shine", gpui_kit::Animation::new(Duration::from_millis(1500)), move |el, t| {
            // 0,4 sn gecikme + 1,1 sn ease-out; `both`: önce -120%'de, sonra 260%'de kalır (kart taşmayı kırpar).
            let raw = ((t * 1.5 - 0.4) / 1.1).clamp(0., 1.);
            let k = p2_ui::motion::CubicBezier::new(0., 0., 0.58, 1.).ease(raw);
            el.left(px(band * (-1.2 + 3.8 * k)))
        })
}

impl Render for AuthScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let register = self.register;
        let viewport = window.viewport_size();
        let (page_w, page_h) = (f32::from(viewport.width), f32::from(viewport.height) - crate::titlebar::HEIGHT);
        // .auth-page: grid 1.25fr .75fr
        let copy_w = (page_w * 1.25 / 2.).round();
        let vw = page_w / 100.;

        // .auth-copy: padding 7vw; logo absolute top 48; eyebrow, h1 (clamp(54px, 6.5vw, 88px)), p
        let h1_size = (6.5 * vw).clamp(54., 88.);
        // `clamp(54px, 6.5vw, 88px)` düz px değil: Electron yazı ölçeği bu başlığa uygulanmaz. gpui yaması bütün px
        // yazı boylarını ve px satır yüksekliklerini çarptığı için burada geri bölünür (%125'te taşmasın).
        let unscale = gpui_kit::p2_font_scale();
        let h1_line = |t: &'static str| div().h(px(h1_size * 0.94)).line_height(px(h1_size * 0.94 / unscale)).whitespace_nowrap().child(t);
        let kicker = fs(9.5);
        let lead = fs(17.);
        let copy = div()
            .w(px(copy_w))
            .flex_shrink_0()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .p(px(7. * vw))
            .child(div().absolute().top(px(48.)).left(px(7. * vw)).child(logo_image(44., &p).p2_enter("auth-logo", Entrance::Fade, Duration::from_millis(120))))
            .child(div().my(kicker).h(p2_ui::text::scaled(normal_line_height(kicker))).line_height(normal_line_height(kicker)).text_size(kicker).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child("BUILD · SHARE · PLAY"))
            .child(div().my(px(20.)).text_size(px(h1_size / unscale)).font_weight(FontWeight::BOLD).tracking(-0.06).text_color(p.text_strong).child(h1_line("Oyunların")).child(h1_line("buluşma noktası.")))
            .child(div().my(lead).h(p2_ui::text::scaled(normal_line_height(lead))).line_height(normal_line_height(lead)).text_size(lead).text_color(p.text_3).child("Ekibini topla, sürümlerini paylaş ve birlikte test et."))
            .p2_enter("auth-copy", Entrance::SlideLeft, Duration::from_millis(35));

        let mut fields = div().flex().flex_col().gap(px(14.));
        {
            let input = |state: &Entity<InputState>| form::input(state, InputKind::Auth, &p, window, cx);
            if register {
                fields = fields
                    .child(auth_label("E-posta", input(&self.email), &p))
                    .child(auth_label("Kullanıcı adı", input(&self.username), &p))
                    .child(auth_label("Görünen ad", input(&self.display_name), &p));
            } else {
                fields = fields.child(auth_label("E-posta veya kullanıcı adı", input(&self.identifier), &p));
            }
            fields = fields.child(auth_label("Parola", input(&self.password), &p));
            if register {
                fields = fields.child(auth_label("Parola tekrarı", input(&self.confirm), &p)).child(
                    auth_label("Davet kodu", input(&self.invite), &p).child(
                        div().text_size(fs(11.)).font_weight(FontWeight::MEDIUM).text_color(p.text_3).line_height(normal_line_height(fs(11.))).child("Bir projenin davet kodun varsa gir; hesabın açılınca o projeye katılırsın."),
                    ),
                );
            }
            if self.two_factor && !register {
                let note = fs(11.);
                fields = fields.child(
                    auth_label("Doğrulama kodu", input(&self.code), &p)
                        .child(div().text_size(note).font_weight(FontWeight::MEDIUM).text_color(p.text_3).line_height(normal_line_height(note)).child("Doğrulayıcı uygulamandaki 6 haneli kodu gir.")),
                );
            }
        }
        if !register {
            fields = fields.child(
                div()
                    .id("remember")
                    .h(px(19.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(fs(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text_2)
                    .child(div().ml(px(4.)).mr(px(3.)).child(native_checkbox(self.remember, &p)))
                    .child("Beni hatırla")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.remember = !this.remember;
                        cx.notify();
                    })),
            );
        }
        if let Some((message, epoch)) = self.error.clone() {
            // `.error` + motion.css: p2-rise 170ms ease-out
            fields = fields.child(
                div()
                    .p(px(11.))
                    .rounded(radius::SM)
                    .border_1()
                    .border_color(p.danger_border)
                    .bg(p.danger_soft)
                    .text_color(p.danger_text)
                    .text_size(fs(12.))
                    .child(message)
                    .with_animation(("auth-error", epoch), gpui_kit::Animation::new(Duration::from_millis(170)), |el, t| {
                        let k = p2_ui::motion::EASE_OUT.ease(t);
                        el.opacity((t * 1.6).min(1.)).relative().top(px(8. * (1. - k)))
                    }),
            );
        }
        let label = if self.busy {
            "Bağlanıyor…"
        } else if register {
            "Hesap oluştur"
        } else if self.two_factor {
            "Doğrula ve gir"
        } else {
            "Giriş yap"
        };
        // `.auth-card form > button`: metin ortada, "→" sağa yaslı (float: right)
        let arrow_w = 11.73;
        let busy = self.busy;
        let submit = div()
            .id("submit")
            .relative()
            .h(px(46.))
            .rounded(radius::MD)
            .bg(p.accent_grad.background())
            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
            .text_color(p.text_on_accent)
            .text_size(font_size::ROOT)
            .css_weight(800.)
            .line_height(px(18.))
            .when(busy, |s| s.opacity(0.65))
            .when(!busy, |s| s.cursor_pointer().hover(|s| s.top(px(-1.))).active(|s| s.top(px(0.))))
            .child(div().absolute().top(px(14.)).left(px(18.)).right(px(18. + arrow_w)).flex().justify_center().child(label))
            .child(div().absolute().top(px(14.)).right(px(18.)).child("→"))
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));

        let card_w = page_w - copy_w - 34.;
        let pad = (4. * vw).clamp(30., 58.);
        let small = fs(13.);
        let card = glass_card(&p)
            .w(px(card_w))
            .pt(px(pad))
            .px(px(pad))
            .pb(px(pad) + small)
            .flex()
            .flex_col()
            .child(shine(card_w))
            .child(div().h(p2_ui::text::scaled(normal_line_height(fs(30.)))).line_height(normal_line_height(fs(30.))).text_size(fs(30.)).font_weight(FontWeight::BOLD).tracking(-0.02).text_color(p.text_strong).child(if register { "Hesap oluştur" } else { "Tekrar hoş geldin" }))
            .child(div().mt(small).h(p2_ui::text::scaled(normal_line_height(small))).line_height(normal_line_height(small)).text_size(small).text_color(p.text_3).child(if register { "Geliştirmeye başlamak için aramıza katıl." } else { "P2 hesabınla devam et." }))
            .child(div().mt(px(28.)).mb(px(20.)).flex().flex_col().gap(px(14.)).child(fields).child(submit))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .items_center()
                    .h(px(16.))
                    .text_size(small)
                    .line_height(normal_line_height(small))
                    .text_color(p.text_3)
                    .child(if register { "Zaten hesabın var mı?\u{a0}" } else { "P2\u{2019}de yeni misin?\u{a0}" })
                    .child(
                        div()
                            .id("switch")
                            .px(px(6.))
                            .py(px(1.))
                            .text_color(p.text_accent)
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .child(if register { "Giriş yap" } else { "Hesap oluştur" })
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_mode(window, cx))),
                    ),
            );

        div()
            .id("auth-scroll")
            .relative()
            .size_full()
            .overflow_y_scroll()
            .child(div().absolute().top_0().left_0().child(radial_background(page_w, page_h, (0.18, 0.7), &[(p.accent_soft_2, 0.), (p.bg_1, 0.34), (p.bg_0, 0.68)], p.bg_0)))
            .child(
                div()
                    .relative()
                    .flex()
                    .min_h(px(page_h))
                    .child(copy)
                    .child(div().flex().items_center().mr(px(34.)).my(px(34.)).child(card.p2_enter(("auth-card", register as usize), Entrance::SlideRight, Duration::from_millis(55)))),
            )
    }
}
