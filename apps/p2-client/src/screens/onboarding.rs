//! App.tsx `OrganizationOnboarding`: kuruluş oluştur ya da davet koduyla katıl.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClipboardItem, Context, Entity, FontWeight, Subscription, WeakEntity, Window, div, px, relative};
use p2_client_core::api::slugify;
use p2_client_core::{Api, ApiError, Organization};
use p2_ui::components::{error_box, eyebrow, glass_card, logo_image, primary_button, secondary_button};
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{KICKER_TRACKING_EM, css_rgba, fs, shadows};
use p2_ui::{P2Fx as _, P2Text as _, P2Transition as _};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::{submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Choose,
    Create,
    Join,
}

pub struct OnboardingScreen {
    app: WeakEntity<ClientApp>,
    api: Arc<Api>,
    mode: Mode,
    created: Option<Organization>,
    name: Entity<InputState>,
    code: Entity<InputState>,
    busy: bool,
    error: Option<(String, usize)>,
    _subs: Vec<Subscription>,
}

impl OnboardingScreen {
    pub fn new(app: WeakEntity<ClientApp>, api: Arc<Api>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = text_input(window, cx, "Örn. Northstar Games");
        let code = text_input(window, cx, "P2-XXXXXXXX");
        let subs = vec![
            submit_on_enter(&name, window, cx, |this, window, cx| this.submit(window, cx)),
            submit_on_enter(&code, window, cx, |this, window, cx| this.submit(window, cx)),
        ];
        Self { app, api, mode: Mode::Choose, created: None, name, code, busy: false, error: None, _subs: subs }
    }

    fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message, epoch));
        cx.notify();
    }

    fn go(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.error = None;
        match mode {
            Mode::Create => self.name.update(cx, |s, cx| s.focus(window, cx)),
            Mode::Join => self.code.update(cx, |s, cx| s.focus(window, cx)),
            Mode::Choose => {}
        }
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.mode == Mode::Choose {
            return;
        }
        let api = self.api.clone();
        self.busy = true;
        self.error = None;
        cx.notify();
        if self.mode == Mode::Create {
            let name = value(&self.name, cx).trim().to_string();
            task::run(window, cx, move || api.create_organization(&name, &slugify(&name)), |this, r: Result<Organization, ApiError>, _, cx| {
                this.busy = false;
                match r {
                    Ok(org) => {
                        this.created = Some(org);
                        cx.notify();
                    }
                    Err(e) => this.set_error(if e.message.is_empty() { "Kuruluş oluşturulamadı.".into() } else { e.message }, cx),
                }
            });
        } else {
            let code = value(&self.code, cx);
            task::run(window, cx, move || api.join_organization(&code), |this, r: Result<Organization, ApiError>, window, cx| {
                this.busy = false;
                match r {
                    Ok(org) => this.complete(org, window, cx),
                    Err(e) => this.set_error(if e.message.is_empty() { "Kuruluşa katılınamadı.".into() } else { e.message }, cx),
                }
            });
        }
    }

    fn complete(&mut self, org: Organization, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let _ = app.update(cx, |app, cx| app.enter_workspace(org, window, cx));
    }
}

/// `.org-page` zemini: iki `radial-gradient(circle at …, renk, transparent N%)` + `bg-0` (farthest-corner).
/// Katmanlar boyuttan bağımsız sabit görseller (`radial_layer`): pencere boyutlandırılırken her karede
/// yeni SVG üretilip rasterlenene dek boş kalmasın (titreme).
fn org_background(w: f32, h: f32, p: &Palette) -> gpui_kit::Div {
    // CSS `transparent` premultiplied karışır: renk sabit, yalnız alfa düşer.
    let layer = |at: (f32, f32), c: gpui_kit::Hsla, stop: f32| p2_ui::components::radial_layer(w, h, at, &[(c, 0.), (gpui_kit::Hsla { a: 0., ..c }, stop)]).absolute().top_0().left_0();
    div()
        .w(px(w))
        .h(px(h))
        .bg(p.bg_0)
        .child(layer((0.82, 0.78), p.accent_soft, 0.28))
        .child(layer((0.22, 0.2), p.accent_soft_2, 0.34))
}

/// `.org-intro:after` gölgesi `0 0 80px <renk>`: CSS dış gölgesi öğenin içini boyamaz (GPUI boyar), bu yüzden
/// bulanık disk iç daire maskesiyle SVG olarak çizilir (260px halka, σ = 40, çevresinde 160px pay).
fn ring_glow(c: gpui_kit::Hsla) -> gpui_kit::Img {
    let (rgb, a) = p2_ui::icons::svg_color(c);
    p2_ui::icons::svg_image(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="580" height="580" viewBox="0 0 580 580"><defs><filter id="b" x="-1" y="-1" width="3" height="3"><feGaussianBlur stdDeviation="40"/></filter><mask id="m"><rect width="580" height="580" fill="#fff"/><circle cx="290" cy="290" r="130" fill="#000"/></mask></defs><g mask="url(#m)"><circle cx="290" cy="290" r="130" fill="{rgb}" fill-opacity="{a}" filter="url(#b)"/></g></svg>"##
    ))
    .w(px(580.))
    .h(px(580.))
}

/// İki rengin premultiplied karışımı (CSS gradyan ara rengi), düz alfaya geri çevrilmiş.
fn premul_mix(a: gpui_kit::Hsla, b: gpui_kit::Hsla, t: f32) -> gpui_kit::Hsla {
    let (a, b): (gpui_kit::Rgba, gpui_kit::Rgba) = (a.into(), b.into());
    let al = a.a + (b.a - a.a) * t;
    let ch = |x: f32, y: f32| if al > 0. { (x * a.a + (y * b.a - x * a.a) * t) / al } else { 0. };
    gpui_kit::Rgba { r: ch(a.r, b.r), g: ch(a.g, b.g), b: ch(a.b, b.b), a: al }.into()
}

/// Çok duraklı CSS `linear-gradient(<açı>deg, …)` (GPUI yalnız iki durak çizer): boyuttan bağımsız
/// sabit görsel (`p2_ui::components::linear_background`).
fn linear_svg(w: f32, h: f32, deg: f32, stops: &[(gpui_kit::Hsla, f32)]) -> gpui_kit::Img {
    p2_ui::components::linear_background(w, h, deg, stops)
}

/// `.org-options > button` (+ `.featured`): 235px, p 25, r 14; simge 48, başlık 18px, açıklama 11.5px, alt satır ok.
fn option(id: &'static str, icon: &'static str, title: &'static str, body: &'static str, cta: &'static str, featured: bool, width: f32, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (accent, soft, lg) = (p.border_accent, p.accent_soft, p.shadow_lg());
    // `.featured`: linear-gradient(145deg, accent-soft, rgba(14,22,38,.6)) — alfaları farklı iki durak; Chromium
    // premultiplied karıştırır, GPUI karıştırmaz: ara duraklar önceden hesaplanmış SVG katmanı (hover'da gizlenir).
    let featured_layer = featured.then(|| {
        let (a, b) = (p.accent_soft, css_rgba(14, 22, 38, 0.6));
        let stops: Vec<(gpui_kit::Hsla, f32)> = (0..=16).map(|i| { let t = i as f32 / 16.; (premul_mix(a, b, t), t) }).collect();
        div().absolute().top_0().left_0().rounded(radius::LG).overflow_hidden().group_hover(id, |s| s.opacity(0.)).child(linear_svg(width, 235., 145., &stops))
    });
    let bg: gpui_kit::Background = if featured { gpui_kit::transparent_black().into() } else { p.glass.background() };
    div()
        .id(id)
        .group(id)
        .relative()
        .w(px(width))
        .flex_none()
        .min_h(px(235.))
        .p(px(25.))
        .flex()
        .flex_col()
        .items_start()
        .rounded(radius::LG)
        .border_1()
        .border_color(if featured { p.border_strong } else { p.border })
        .bg(bg)
        .shadow(shadows(&[p.inset_top]))
        .text_color(p.text)
        .cursor_pointer()
        // `transition: transform var(--m-base) var(--ease-pop), …`; hover 3px yukarı + vurgu.
        .transition(p2_ui::motion::M_BASE, p2_ui::motion::EASE_POP)
        .hover(move |s| s.translate(0., -3.).border_color(accent).bg(soft).shadow(lg.clone()))
        .children(featured_layer)
        .when(featured, |s| {
            s.child(
                div()
                    .absolute()
                    .right(px(17.))
                    .top(px(17.))
                    .px(px(7.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .bg(p.accent_grad.background())
                    .shadow(shadows(&[p.accent_glow_strong]))
                    .text_color(p.text_on_accent)
                    .text_size(fs(8.))
                    .line_height(normal_line_height(fs(8.)))
                    .css_weight(900.)
                    .tracking(0.12)
                    .child("ÖNERİLEN"),
            )
        })
        .child(div().size(px(48.)).mb(px(26.)).flex().items_center().justify_center().border_1().border_color(p.border).rounded(radius::LG).bg(p.accent_soft).text_color(p.text_accent).text_size(fs(21.)).child(icon))
        .child(div().text_size(fs(18.)).line_height(normal_line_height(fs(18.))).css_weight(700.).text_color(p.text_strong).child(title))
        .child(div().max_w(px(260.)).mt(px(9.)).text_size(fs(11.5)).line_height(relative(1.55)).text_color(p.text_3).child(body))
        .child(
            div()
                .w_full()
                .mt_auto()
                .flex()
                .justify_between()
                .text_size(fs(11.))
                .line_height(normal_line_height(fs(11.)))
                .css_weight(750.)
                .text_color(p.text_accent)
                .child(cta)
                .child("→"),
        )
}

impl OnboardingScreen {
    /// `.org-form`: etiket + girdi + not, hata, `.form-actions` (Geri / birincil).
    fn form(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (label, input, note) = if self.mode == Mode::Create {
            ("Kuruluş adı", &self.name, "Çalışma alanı adresi addan otomatik hazırlanır.")
        } else {
            ("Davet kodu", &self.code, "Kod büyük/küçük harfe duyarlı değildir.")
        };
        let focused = gpui_kit::Focusable::focus_handle(input.read(cx), cx).is_focused(window);
        let field = div()
            .h(px(47.))
            .px(px(14.))
            .flex()
            .items_center()
            .rounded(radius::MD)
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border })
            .bg(p.bg_3)
            .when(focused, |s| s.shadow(shadows(&[p.ring])))
            .text_size(fs(13.))
            .css_weight(400.)
            .text_color(p.text)
            .child(gpui_kit::component::input::Input::new(input).appearance(false).bordered(false).focus_bordered(false).p(px(0.)).text_size(fs(13.)));
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let label_text = if self.busy { "İşleniyor…" } else if self.mode == Mode::Create { "Kuruluşu oluştur" } else { "Kuruluşa katıl" };
        div()
            .w_full()
            .p(px(26.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::LG)
            .bg(p.glass.background())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .text_size(fs(12.))
                    .css_weight(700.)
                    .text_color(p.text_2)
                    .child(label)
                    .child(field)
                    .child(div().text_size(fs(10.5)).css_weight(400.).text_color(p.text_3).child(note)),
            )
            .when_some(self.error.clone(), |s, (m, e)| s.child(div().mt(px(16.)).child(error_box(m, e, p))))
            .child(
                div()
                    .mt(px(25.))
                    .flex()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .id("back")
                            .h(px(42.))
                            .px(px(17.))
                            .flex()
                            .items_center()
                            .rounded(radius::MD)
                            .text_color(p.text_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_bg).text_color(strong))
                            .child("← Geri")
                            .on_click(cx.listener(|this, _, window, cx| this.go(Mode::Choose, window, cx))),
                    )
                    .child(
                        div()
                            .id("submit")
                            .h(px(42.))
                            .px(px(17.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(radius::MD)
                            .bg(p.accent_grad.background())
                            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                            .text_color(p.text_on_accent)
                            .css_weight(800.)
                            .when(self.busy, |s| s.opacity(0.6))
                            .when(!self.busy, |s| s.cursor_pointer().hover(|s| s.brightness(1.08).translate(0., -1.)))
                            .child(label_text)
                            .child("→")
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
            .p2_enter(("org-form", self.mode as usize), Entrance::Rise, Duration::ZERO)
            .into_any_element()
    }
}

impl Render for OnboardingScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let vp = window.viewport_size();
        // `.org-page` başlık çubuğunun (38px) altında kalan alanı doldurur.
        let (page_w, page_h) = (f32::from(vp.width), f32::from(vp.height) - 38.);

        if let Some(org) = self.created.clone() {
            let invite = org.invite_code.clone().unwrap_or_default();
            let card = glass_card(&p)
                .w(px(520.))
                .p(px(40.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(14.))
                .child(div().size(px(56.)).rounded_full().flex().items_center().justify_center().bg(p.success_soft).text_color(p.success).text_size(px(26.)).child("✓").p2_enter("mark", Entrance::PopElastic, Duration::from_millis(80)))
                .child(eyebrow("ÇALIŞMA ALANI HAZIR", &p))
                .child(div().text_size(px(30.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(org.name.clone()))
                .child(div().text_size(font_size::MD).text_color(p.text_3).child("Kuruluşun oluşturuldu. Ekip arkadaşlarını bu davet koduyla içeri alabilirsin."))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px(px(16.))
                        .py(px(12.))
                        .rounded(radius::LG)
                        .border_1()
                        .border_color(p.border)
                        .bg(p.bg_3)
                        .child(div().flex_1().flex().flex_col().child(div().text_size(font_size::XS).text_color(p.text_4).child("DAVET KODU")).child(div().font_family(p2_ui::tokens::FONT_MONO).text_size(px(18.)).text_color(p.text_strong).child(invite.clone())))
                        .child(secondary_button("copy-invite", "Kopyala", &p).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(invite.clone())))),
                )
                .child(div().w_full().child(primary_button("enter", "Çalışma alanına geç", false, &p).on_click(cx.listener(move |this, _, window, cx| this.complete(org.clone(), window, cx)))));
            return div()
                .relative()
                .w(px(page_w))
                .h(px(page_h))
                .child(div().absolute().top_0().left_0().child(org_background(page_w, page_h, &p)))
                .child(div().absolute().inset_0().flex().items_center().justify_center().child(div().child(card).p2_enter("created", Entrance::PopElastic, Duration::ZERO)))
                .into_any_element();
        }

        let (step, title, subtitle) = match self.mode {
            Mode::Choose => ("BAŞLANGIÇ", "Ekibin için bir alan seç.", "Projeler, build’ler ve ekip üyeleri tek bir çalışma alanında buluşur."),
            Mode::Create => ("YENİ KURULUŞ", "Kuruluşunu oluşturalım.", "İsim vermen yeterli; davet kodunu bir sonraki adımda hazırlayacağız."),
            Mode::Join => ("DAVETLE KATIL", "Ekibine katıl.", "Yöneticinin paylaştığı davet kodunu gir ve kaldığın yerden devam et."),
        };

        // `.org-intro`: 360px, p 42/38, 155° koyu mavi gradyan, sağ kenar; `::after` 260px halka (sağ alt).
        let checks = ["Ortak proje ve kanal yapısı", "Tek kodla güvenli davet", "Ekip rolleri ve izinler"];
        let mut list = div().mt(px(40.)).flex().flex_col().gap(px(13.)).text_size(fs(12.)).css_weight(600.).text_color(p.text);
        for (i, t) in checks.into_iter().enumerate() {
            list = list.child(
                div()
                    .h(px(19.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().size(px(19.)).flex().items_center().justify_center().rounded(px(6.)).bg(p.accent_soft).text_color(p.text_accent).text_size(fs(10.)).child("✓"))
                    .child(t)
                    .p2_enter(("org-li", i), Entrance::SlideLeft, Duration::from_millis(80 * i as u64)),
            );
        }
        let intro = div()
            .relative()
            .w(px(360.))
            .flex_none()
            .overflow_hidden()
            .py(px(42.))
            .px(px(38.))
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border_subtle)
            // `linear-gradient(155deg, blue-900 0, bg-2 52%, bg-1 100%)` (kabuk 570 − 2 kenarlık = 568).
            .child(div().absolute().top_0().left_0().child(linear_svg(359., 568., 155., &[(p.blue_900, 0.), (p.bg_2, 0.52), (p.bg_1, 1.)])))
            .child(
                div().absolute().right(px(-125. - 160.)).bottom(px(-110. - 160.)).child(ring_glow(p.accent_soft_2)),
            )
            .child(div().absolute().size(px(260.)).right(px(-125.)).bottom(px(-110.)).rounded_full().border_1().border_color(p.border_strong))
            // `.org-brand` 48×48 kutu, içindeki `.p2-logo.lg` 64px taşar; 72px alt boşluk.
            .child(div().size(px(48.)).mb(px(72.)).child(logo_image(64., &p).p2_enter("org-logo", Entrance::LogoPop, Duration::from_millis(120))))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().mt(px(8.)).h(px(10.)).text_size(font_size::XXS).line_height(px(10.)).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child("P2 ORGANIZATIONS"))
                    .child(div().max_w(px(275.)).mt(px(10.)).mb(px(14.)).text_size(fs(31.)).line_height(relative(1.08)).css_weight(700.).tracking(-0.04).text_color(p.text_strong).child("Birlikte üretmek için tek bir merkez."))
                    .child(div().max_w(px(275.)).text_size(fs(13.)).line_height(relative(1.65)).text_color(p.text_3).child("Ekibini, projelerini ve sürümlerini dağılmadan yönet.")),
            )
            .child(list)
            .child(div().relative().mt_auto().text_size(fs(10.5)).line_height(normal_line_height(fs(10.5))).text_color(p.text_4).child("Kuruluş ayarlarını daha sonra değiştirebilirsin."));

        let header = div()
            .max_w(px(560.))
            .mb(px(34.))
            .flex()
            .flex_col()
            .items_start()
            .child(
                div()
                    .px(px(8.))
                    .py(px(5.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(px(6.))
                    .bg(p.accent_soft)
                    .text_color(p.text_accent)
                    .text_size(font_size::XXS)
                    .line_height(px(10.))
                    .css_weight(900.)
                    .tracking(KICKER_TRACKING_EM)
                    .child(step),
            )
            .child(div().mt(px(13.)).mb(px(10.)).text_size(fs(38.)).line_height(relative(1.05)).css_weight(700.).tracking(-0.045).text_color(p.text_strong).child(title))
            .child(div().text_size(fs(13.)).line_height(relative(1.6)).text_color(p.text_3).child(subtitle));
        // `.org-options`: 1fr 1fr, gap 15 (iş akışı = kabuk − 360 − 2 kenarlık − 2×60 dolgu).
        let shell_w = 1040f32.min(page_w * 0.94);
        let option_w = (shell_w - 360. - 2. - 120. - 15.) / 2.;
        let body: AnyElement = if self.mode == Mode::Choose {
            div()
                .w_full()
                .flex()
                .gap(px(15.))
                .child(option("org-create", "＋", "Kuruluş oluştur", "Yeni bir ekip kur, projelerini yönet ve üyelerini davet et.", "Oluşturmaya başla", true, option_w, &p).on_click(cx.listener(|this, _, window, cx| this.go(Mode::Create, window, cx))).p2_enter("org-create", Entrance::Rise, Duration::from_millis(55)))
                .child(option("org-join", "↗", "Bir kuruluşa katıl", "Mevcut ekibinden aldığın davet koduyla çalışma alanına gir.", "Davet kodunu gir", false, option_w, &p).on_click(cx.listener(|this, _, window, cx| this.go(Mode::Join, window, cx))).p2_enter("org-join", Entrance::Rise, Duration::from_millis(90)))
                .into_any_element()
        } else {
            self.form(&p, window, cx)
        };
        let flow = div().flex_1().min_w_0().py(px(54.)).px(px(60.)).flex().flex_col().justify_center().child(header).child(body);

        // `.org-shell`: min(1040px, 94vw) × min 570, 360px + 1fr, `glass-strong`, `shadow-lg`, r 18.
        let shell = div()
            .w(px(shell_w))
            .min_h(px(570.))
            .flex()
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .child(intro)
            .child(flow);
        div()
            .relative()
            .w(px(page_w))
            .h(px(page_h))
            .child(div().absolute().top_0().left_0().child(org_background(page_w, page_h, &p)))
            .child(div().absolute().inset_0().p(px(40.)).flex().items_center().justify_center().child(div().child(shell).p2_enter("org", Entrance::PopElastic, Duration::ZERO)))
            .p2_enter("org-page", Entrance::Page, Duration::ZERO)
            .into_any_element()
    }
}
