//! Sunucu tarayıcısı: P2 Server kurulumlarının listesi (kim nereye kurduysa, `[directory] list` açıksa).
//! Açık sunucu seçilip girilir; özel sunucu listede görünür ama katılım kodu ister (kod doğruysa dizin
//! adresini verir, kod kayıtta da sunucuya gider). İlk açılışta "Bir sunucuya katıl"dan ve uygulama
//! içinde menüden açılır. Adresi bilinen sunucuya "Adresle bağlan" ile eskisi gibi bağlanılır.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, ImageSource, ObjectFit, RenderImage, SharedString, Subscription, WeakEntity, Window, div, img, px, relative};
use p2_client_core::directory::{self, DirServer};
use p2_i18n::{t, tv};
use p2_ui::components::error_box;
use p2_ui::text::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::setup_ui::{mode_back, setup_page};
use super::{submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

pub struct ServerBrowser {
    app: WeakEntity<ClientApp>,
    /// Uygulama içinden açıldı (geri = çalışma alanı); değilse geri = başlangıç seçimi.
    in_app: bool,
    directory: String,
    search: Entity<InputState>,
    code: Entity<InputState>,
    servers: Option<Vec<DirServer>>,
    loading: bool,
    error: Option<String>,
    icons: HashMap<String, Option<Arc<RenderImage>>>,
    /// Kod girilen özel sunucu.
    open: Option<String>,
    joining: Option<String>,
    join_error: Option<(String, usize)>,
    epoch: usize,
    _subs: Vec<Subscription>,
}

impl ServerBrowser {
    pub fn new(app: WeakEntity<ClientApp>, directory: String, in_app: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = text_input(window, cx, &t("browser.search"));
        let code = text_input(window, cx, &t("browser.codePlaceholder"));
        let subs = vec![
            submit_on_enter(&search, window, cx, |this, window, cx| this.refresh(window, cx)),
            submit_on_enter(&code, window, cx, |this, window, cx| {
                if let Some(id) = this.open.clone() {
                    this.join(id, window, cx);
                }
            }),
        ];
        let mut this = Self { app, in_app, directory, search, code, servers: None, loading: false, error: None, icons: HashMap::new(), open: None, joining: None, join_error: None, epoch: 0, _subs: subs };
        this.refresh(window, cx);
        this
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.error = None;
        cx.notify();
        let (dir, q) = (self.directory.clone(), value(&self.search, cx));
        task::run(window, cx, move || directory::servers(&dir, &q), |this, r, window, cx| {
            this.loading = false;
            match r {
                Ok(list) => {
                    crate::trace(&format!("tarayıcı: {} sunucu listelendi", list.len()));
                    this.servers = Some(list);
                    this.load_icons(window, cx);
                }
                Err(_) => this.error = Some(t("browser.unreachable")),
            }
            cx.notify();
        });
    }

    fn load_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted: Vec<(String, String)> = self
            .servers
            .iter()
            .flatten()
            .filter(|s| !self.icons.contains_key(&s.id))
            .filter_map(|s| s.icon_url.clone().map(|u| (s.id.clone(), u)))
            .collect();
        for (id, _) in &wanted {
            self.icons.insert(id.clone(), None);
        }
        if wanted.is_empty() {
            return;
        }
        let dir = self.directory.clone();
        task::run(
            window,
            cx,
            move || wanted.into_iter().filter_map(|(id, path)| directory::icon(&dir, &path).and_then(|b| decode_icon(&b)).map(|(i, _, _)| (id, i))).collect::<Vec<_>>(),
            |this, list, _, cx| {
                for (id, image) in list {
                    this.icons.insert(id, Some(image));
                }
                cx.notify();
            },
        );
    }

    fn set_join_error(&mut self, message: String, cx: &mut Context<Self>) {
        let epoch = self.join_error.as_ref().map_or(0, |e| e.1 + 1);
        self.join_error = Some((message, epoch));
        cx.notify();
    }

    /// Kartın düğmesi: açık sunucuya doğrudan, özele kodla.
    fn press(&mut self, server: &DirServer, window: &mut Window, cx: &mut Context<Self>) {
        if server.private() && self.open.as_deref() != Some(server.id.as_str()) {
            self.open = Some(server.id.clone());
            self.join_error = None;
            self.code.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
            cx.notify();
            return;
        }
        self.join(server.id.clone(), window, cx);
    }

    fn join(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.joining.is_some() {
            return;
        }
        let Some(server) = self.servers.iter().flatten().find(|s| s.id == id).cloned() else { return };
        let code = if server.private() { value(&self.code, cx).trim().to_string() } else { String::new() };
        if server.private() && code.is_empty() {
            return self.set_join_error(t("browser.codeRequired"), cx);
        }
        self.joining = Some(id.clone());
        self.join_error = None;
        cx.notify();
        let dir = self.directory.clone();
        task::run(
            window,
            cx,
            move || {
                // Adres: açıkta listede var, özelde kodla dizinden alınır; sonra sunucunun gerçekten yanıt verdiği denetlenir.
                let url = match server.url.clone().filter(|_| !server.private()) {
                    Some(url) => Ok(url),
                    None => directory::join(&dir, &server.id, &code),
                }?;
                p2_client_core::api::resolve_server(&url).ok_or_else(|| p2_client_core::api::ApiError { code: "UNREACHABLE".into(), ..Default::default() }).map(|u| (u, code))
            },
            move |this, r, window, cx| {
                this.joining = None;
                match r {
                    Ok((url, code)) => {
                        crate::trace(&format!("tarayıcı: sunucuya katılınıyor ({})", if code.is_empty() { "açık" } else { "kodla" }));
                        let app = this.app.clone();
                        let _ = app.update(cx, |app, cx| app.join_from_browser(url, (!code.is_empty()).then_some(code), window, cx));
                    }
                    Err(e) if e.code == "BAD_CODE" => this.set_join_error(t("browser.badCode"), cx),
                    Err(e) if e.code == "UNREACHABLE" || e.unreachable => this.set_join_error(t("browser.serverDown"), cx),
                    Err(e) => this.set_join_error(if e.message.is_empty() { t("browser.serverDown") } else { e.message }, cx),
                }
                cx.notify();
            },
        );
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (app, in_app) = (self.app.clone(), self.in_app);
        let _ = app.update(cx, |app, cx| app.close_browser(in_app, window, cx));
    }

    fn by_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let _ = app.update(cx, |app, cx| app.show_address_form(window, cx));
    }
}

fn styled_input(state: &Entity<InputState>, focused: bool, h: f32, p: &Palette) -> impl IntoElement {
    let input = Input::new(state).appearance(false).bordered(false).focus_bordered(false);
    Styled::h(input, px(h))
        .px(px(12.))
        .rounded(radius::SM)
        .bg(p.bg_3)
        .border_1()
        .text_size(fs(12.5))
        .text_color(p.text)
        .map(|s| if focused { s.border_color(p.border_accent).shadow(p2_ui::tokens::shadows(&[p.ring])) } else { s.border_color(p.border) })
}

/// Sunucu simgesi (en çok 128 px, BGRA).
fn decode_icon(bytes: &[u8]) -> Option<(Arc<RenderImage>, u32, u32)> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let decoded = if decoded.width() > 128 || decoded.height() > 128 { decoded.thumbnail(128, 128) } else { decoded };
    let mut buf = decoded.to_rgba8();
    let (w, h) = (buf.width(), buf.height());
    for px in buf.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some((Arc::new(RenderImage::new(vec![image::Frame::new(buf)])), w, h))
}

/// `install_button` ile aynı görünüm, dinamik kimlikli (kart başına).
fn accent_button(id: SharedString, label: String, busy: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .flex_none()
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
        .child(label)
        .child("→")
}

/// Baş harfler (simgesi olmayan sunucu).
fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

fn badge(text: String, fg: gpui_kit::Hsla, bg: gpui_kit::Hsla, border: gpui_kit::Hsla) -> impl IntoElement {
    div().flex_none().h(px(20.)).px(px(8.)).flex().items_center().rounded(px(10.)).border_1().border_color(border).bg(bg).text_color(fg).text_size(fs(10.)).font_weight(FontWeight::BOLD).child(text)
}

impl Render for ServerBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let search_focused = gpui_kit::Focusable::focus_handle(self.search.read(cx), cx).is_focused(window);
        let code_focused = gpui_kit::Focusable::focus_handle(self.code.read(cx), cx).is_focused(window);
        let (accent_soft, accent_border) = (p.accent_soft, p.border_accent);

        let head = div()
            .flex()
            .items_end()
            .justify_between()
            .gap(px(20.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .min_w_0()
                    .child(div().text_size(fs(10.5)).font_weight(FontWeight::BOLD).tracking(0.18).text_color(p.text_accent).child(t("browser.kicker")))
                    .child(div().text_size(fs(26.)).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_strong).child(t("browser.title")))
                    .child(div().text_size(fs(12.5)).line_height(relative(1.5)).text_color(p.text_3).child(t("browser.lead"))),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(8.))
                    .child(mode_back("browser-back", t("common.back"), &p).on_click(cx.listener(|this, _, window, cx| this.back(window, cx))))
                    .child(mode_back("browser-address", t("browser.byAddress"), &p).on_click(cx.listener(|this, _, window, cx| this.by_address(window, cx)))),
            );

        let loading = self.loading;
        let toolbar = div()
            .mt(px(22.))
            .flex()
            .gap(px(8.))
            .child(div().flex_1().child(styled_input(&self.search, search_focused, 40., &p)))
            .child(mode_back("browser-refresh", t(if loading { "browser.loading" } else { "browser.refresh" }), &p).h(px(40.)).on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))));

        let mut list = div().mt(px(18.)).flex().flex_wrap().gap(px(12.));
        match (&self.servers, &self.error) {
            (_, Some(err)) => {
                list = list.child(div().w_full().child(error_box(err.clone(), 0, &p)));
            }
            (None, None) => {
                list = list.child(div().w_full().py(px(40.)).flex().justify_center().text_color(p.text_3).text_size(fs(12.5)).child(t("browser.loading")));
            }
            (Some(servers), None) if servers.is_empty() => {
                list = list.child(div().w_full().py(px(40.)).flex().flex_col().items_center().gap(px(6.)).child(div().text_color(p.text_2).text_size(fs(13.5)).font_weight(FontWeight::BOLD).child(t("browser.empty"))).child(div().text_color(p.text_3).text_size(fs(12.)).child(t("browser.emptyHint"))));
            }
            (Some(servers), None) => {
                for (ix, s) in servers.iter().enumerate() {
                    let icon = self.icons.get(&s.id).cloned().flatten();
                    let open = self.open.as_deref() == Some(s.id.as_str());
                    let busy = self.joining.as_deref() == Some(s.id.as_str());
                    let media = div()
                        .flex_none()
                        .size(px(54.))
                        .rounded(radius::LG)
                        .overflow_hidden()
                        .bg(p.bg_3)
                        .border_1()
                        .border_color(p.border)
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|d| match icon {
                            Some(image) => d.child(img(ImageSource::Render(image)).size_full().object_fit(ObjectFit::Cover)),
                            None => d.text_color(p.text_accent).text_size(fs(18.)).font_weight(FontWeight::EXTRA_BOLD).child(initials(&s.name)),
                        });
                    let access = if s.private() {
                        badge(t("browser.private"), p.text_2, p.bg_3, p.border)
                    } else {
                        badge(t("browser.public"), p.text_accent, accent_soft, accent_border)
                    };
                    let status = if s.live {
                        div().flex().items_center().gap(px(5.)).child(div().size(px(7.)).rounded_full().bg(p.success)).child(tv("browser.online", &[("n", &s.online.to_string())]))
                    } else {
                        div().flex().items_center().gap(px(5.)).child(div().size(px(7.)).rounded_full().bg(p.text_4)).child(t("browser.offline"))
                    };
                    let meta = div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .text_size(fs(11.))
                        .text_color(p.text_3)
                        .child(status)
                        .child(tv("browser.members", &[("n", &s.members.to_string())]))
                        .when(!s.version.is_empty(), |d| d.child(format!("v{}", s.version)));
                    let server = s.clone();
                    let action_label = if busy { t("browser.joining") } else if s.private() { t(if open { "browser.join" } else { "browser.joinWithCode" }) } else { t("browser.join") };
                    let action = accent_button(SharedString::from(format!("join-{}", s.id)), action_label, busy, &p).on_click(cx.listener(move |this, _, window, cx| this.press(&server, window, cx)));
                    let mut card = div()
                        .id(SharedString::from(format!("server-{}", s.id)))
                        // İki sütun; dar pencerede tek sütun (son kart satırı kaplamaz).
                        .w(relative(0.492))
                        .min_w(px(300.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .p(px(16.))
                        .rounded(radius::LG)
                        .border_1()
                        .border_color(if open { p.border_accent } else { p.border })
                        .bg(p.bg_2)
                        .when(!s.live, |d| d.opacity(0.7))
                        .hover({
                            let (b, sh) = (p.border_strong, p.shadow_md());
                            move |d| d.border_color(b).shadow(sh.clone())
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(14.))
                                .child(media)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.))
                                        .child(div().flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().truncate().text_size(fs(15.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(s.name.clone())).child(access))
                                        .child(div().text_size(fs(12.)).line_height(relative(1.45)).text_color(p.text_2).line_clamp(2).child(if s.description.is_empty() { t("browser.noDescription") } else { s.description.clone() }))
                                        .child(meta),
                                ),
                        );
                    if open {
                        card = card
                            .child(div().text_size(fs(11.5)).text_color(p.text_2).child(t("browser.codeHint")))
                            .child(div().flex().gap(px(8.)).child(div().flex_1().child(styled_input(&self.code, code_focused, 38., &p))).child(action))
                            .when_some(self.join_error.clone(), |d, (m, e)| d.child(error_box(m, e, &p)));
                    } else {
                        card = card.child(div().flex().justify_end().child(action));
                    }
                    list = list.child(card.p2_enter(("server-card", ix), Entrance::Rise, Duration::from_millis((ix as u64).min(8) * 35)));
                }
            }
        }
        // Açık sunucuda katılma hatası (kart kapalıyken): listenin altında.
        let public_error = self.open.is_none().then(|| self.join_error.clone()).flatten();

        let shell = div()
            .w_full()
            .max_w(px(980.))
            .max_h(relative(0.92))
            .flex()
            .flex_col()
            .p(px(28.))
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border)
            .bg(p.bg_1)
            .shadow(p.shadow_lg())
            .child(head)
            .child(toolbar)
            .when_some(public_error, |d, (m, e)| d.child(div().mt(px(12.)).child(error_box(m, e, &p))))
            .child(div().id("browser-list").flex_1().min_h_0().overflow_y_scroll().child(list));
        let tint = p.accent_soft;
        setup_page(window, &p, (0.5, 0.15), tint, 0.5, shell.p2_enter(("browser", self.epoch), Entrance::Rise, Duration::ZERO))
    }
}
