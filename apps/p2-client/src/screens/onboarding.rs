//! Başlangıç ekranı (App.tsx `OrganizationOnboarding`, proje toplulukları için — docs/proje-bazli-yapi.md):
//! henüz hiçbir projede olmayan kişiyi karşılayan tek ekran. Üç yol yan yana durur:
//!   1. **Davet koduyla katıl** — kod alanı ekranın kendisindedir; tam kod yazılınca önizleme kartı açılır.
//!   2. **Projelere göz at** — Keşfet (`discover.rs`): herkese açık projeler ve haberleri, tek tıkla katılma.
//!   3. **Yeni proje oluştur** — sunucu yetkisi `project.create` yoksa soluk ve tıklanamaz (ipucuyla).
//! "Kurulumu tamamla" (kuruluş oluştur) yalnız ilk kurulumda ya da sunucuda henüz kuruluş yokken görünür.
//!
//! Otomatik pilot: `P2_AUTOPILOT_JOIN` (kodlar), `P2_AUTOPILOT_DISCOVER` (Keşfet'i açar; adımlar discover.rs'te),
//! `P2_AUTOPILOT_ONBOARD=project|project-public|create:<ad>[:public[:KATEGORİ]]`.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, FontWeight, SharedString, Subscription, WeakEntity, Window, div, px, relative};
use p2_client_core::api::slugify;
use p2_client_core::{Api, ApiError, DiscoverList, InvitePreview, Organization};
use p2_ui::components::{error_box, eyebrow, glass_card, logo_image, primary_button};
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{FONT_MONO, KICKER_TRACKING_EM, css_rgba, fs, shadows};
use p2_ui::{P2Fx as _, P2Text as _, P2Transition as _};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::discover::{DiscoverEvent, DiscoverView, Host};
use super::discover_ui::{self as dui, glyph, ico, t, tv};
use super::{submit_on_enter, text_input, value};
use crate::app::ClientApp;
use crate::task;

/// Başlangıç ekranından "Arkadaşlar ve mesajlar" ile geçildi: çalışma alanı projesiz açık kalır
/// (proje listesi boşken başlangıç ekranına geri dönmez, Ana sayfayı açar). Başlangıç ekranı açılınca sıfırlanır.
pub(super) static HOME_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Choose,
    /// Kuruluş (kurulum kabı) oluştur — ilk kurulum.
    Create,
    /// Proje davet koduyla katıl.
    Join,
    /// Yeni proje aç.
    Project,
}

pub struct OnboardingScreen {
    app: WeakEntity<ClientApp>,
    api: Arc<Api>,
    mode: Mode,
    created: Option<Organization>,
    /// İlk kurulum (ilk yönetici az önce oluşturuldu): kuruluş oluşturma öne çıkar.
    first_install: bool,
    name: Entity<InputState>,
    code: Entity<InputState>,
    project: Entity<InputState>,
    preview: Option<InvitePreview>,
    previewed: String,
    /// Sunucu yetkisi `project.create` (`GET /auth/me`); yanıt gelene dek bilinmiyor (`None`).
    can_create: Option<bool>,
    /// "Projelere göz at" kartındaki özet: açık proje sayısı ve ilk birkaç projenin yüzü.
    teaser: Option<DiscoverList>,
    discover: Option<(Entity<DiscoverView>, Subscription)>,
    /// "Arkadaşlar ve mesajlar": kişinin kurulum kabı ve okunmamış birebir mesaj sayısı (kabı yoksa giriş gizli).
    home: Option<(Organization, u32)>,
    /// Yeni proje: herkese açık mı ve Keşfet kategorisi.
    public: bool,
    category: Option<&'static str>,
    /// Otomatik pilot: `P2_AUTOPILOT_JOIN` kodları (`;`), `kod|join` önizlemeden sonra katılır.
    steps: Vec<String>,
    busy: bool,
    error: Option<(String, usize)>,
    _subs: Vec<Subscription>,
}

impl OnboardingScreen {
    pub fn new(app: WeakEntity<ClientApp>, api: Arc<Api>, first_install: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = text_input(window, cx, &t("onb.setup.placeholder"));
        let code = text_input(window, cx, &t("onb.code.placeholder"));
        let project = text_input(window, cx, &t("onb.project.placeholder"));
        let mut subs = vec![
            submit_on_enter(&name, window, cx, |this, window, cx| this.submit(window, cx)),
            submit_on_enter(&code, window, cx, |this, window, cx| this.submit(window, cx)),
            submit_on_enter(&project, window, cx, |this, window, cx| this.submit(window, cx)),
        ];
        // Kod değişince eski önizleme düşer; tam kod yazılınca/yapıştırılınca kendiliğinden önizlenir
        // (seçim ekranındaysa katılma adımına geçer).
        subs.push(cx.subscribe_in(&code, window, |this, state, ev: &gpui_kit::component::input::InputEvent, window, cx| {
            if matches!(ev, gpui_kit::component::input::InputEvent::Change) {
                let v = super::join_project::normalize_code(state.read(cx).value());
                if v != this.previewed {
                    this.preview = None;
                    if v.len() == 11 && v.starts_with("P2-") && !this.busy && matches!(this.mode, Mode::Choose | Mode::Join) {
                        this.mode = Mode::Join;
                        this.error = None;
                        this.load_preview(window, cx);
                    }
                }
                cx.notify();
            }
        }));
        let steps: Vec<String> = std::env::var("P2_AUTOPILOT_JOIN").map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect()).unwrap_or_default();
        let mut screen = Self {
            app,
            api,
            mode: Mode::Choose,
            created: None,
            first_install,
            name,
            code,
            project,
            preview: None,
            previewed: String::new(),
            can_create: None,
            teaser: None,
            discover: None,
            home: None,
            public: false,
            category: None,
            steps,
            busy: false,
            error: None,
            _subs: subs,
        };
        HOME_ONLY.store(false, std::sync::atomic::Ordering::SeqCst);
        crate::trace(&format!("başlangıç: {}", if first_install { "ilk kurulum" } else { "projeye katıl ya da oluştur" }));
        screen.load_context(window, cx);
        if !screen.steps.is_empty() {
            super::join_project::AUTOPILOT_JOIN_USED.store(true, std::sync::atomic::Ordering::SeqCst);
            screen.go(Mode::Join, window, cx);
            screen.next_step(window, cx);
        } else if std::env::var_os("P2_AUTOPILOT_DISCOVER").is_some() {
            screen.open_discover(window, cx);
        } else if let Ok(spec) = std::env::var("P2_AUTOPILOT_ONBOARD") {
            screen.autopilot_onboard(&spec, window, cx);
        }
        screen
    }

    /// Yetkim (`project.create`) ve "Projelere göz at" kartının özeti.
    fn load_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || api.me(), |this, r, _, cx| {
            if let Ok(me) = r {
                let can = me.can_server("project.create");
                crate::trace(&format!("başlangıç: proje açma yetkisi {}", if can { "var" } else { "yok" }));
                this.can_create = Some(can);
                cx.notify();
            }
        });
        let api = self.api.clone();
        task::run(window, cx, move || api.organizations().map(|orgs| orgs.into_iter().next().map(|org| { let unread = api.conversations(&org.id).map(|c| c.iter().map(|x| x.unread).sum::<u32>()).unwrap_or(0); (org, unread) })), |this, r, window, cx| {
            if let Ok(Some(home)) = r {
                crate::trace(&format!("başlangıç: ana sayfa girişi ({} okunmamış)", home.1));
                this.home = Some(home);
                if std::env::var("P2_AUTOPILOT_ONBOARD").is_ok_and(|v| v == "home") {
                    this.open_home(window, cx);
                }
                cx.notify();
            }
        });
        let api = self.api.clone();
        task::run(window, cx, move || api.discover_projects("", None, "popular"), |this, r, _, cx| {
            if let Ok(list) = r {
                crate::trace(&format!("başlangıç: {} açık proje", list.total));
                this.teaser = Some(list);
                cx.notify();
            }
        });
    }

    /// "Arkadaşlar ve mesajlar": projesi olmayan kişi de Ana sayfaya (arkadaşlar, birebir mesajlar) geçer.
    /// Çalışma alanı projesiz açılır; ray (Ana sayfa, ＋, Keşfet) görünür kalır.
    fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((org, unread)) = self.home.clone() else { return };
        crate::trace(&format!("başlangıç: ana sayfa ({unread} okunmamış)"));
        HOME_ONLY.store(true, std::sync::atomic::Ordering::SeqCst);
        self.complete(org, window, cx);
    }

    /// `P2_AUTOPILOT_ONBOARD`: `home` (Ana sayfaya geç), `project` (form), `project-public` (form, herkese açık seçili),
    /// `create:<ad>[:public[:KATEGORİ]]` (projeyi oluştur).
    fn autopilot_onboard(&mut self, spec: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut parts = spec.split(':');
        match parts.next().unwrap_or_default() {
            "project" => self.go(Mode::Project, window, cx),
            "project-public" => {
                self.go(Mode::Project, window, cx);
                self.public = true;
                self.category = Some("GAME");
            }
            "create" => {
                let name = parts.next().unwrap_or("Proje").to_string();
                self.public = parts.next() == Some("public");
                self.category = parts.next().and_then(|c| p2_client_core::DISCOVER_CATEGORIES.iter().copied().find(|x| *x == c));
                self.go(Mode::Project, window, cx);
                self.project.update(cx, |s, cx| s.set_value(name, window, cx));
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(1800)).await;
                    let _ = this.update_in(cx, |this, window, cx| this.submit(window, cx));
                })
                .detach();
            }
            _ => {}
        }
    }

    /// "Projelere göz at": Keşfet bu ekranın üstünde açılır.
    fn open_discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        let view = cx.new(|cx| DiscoverView::new(api, Host::Onboarding, window, cx));
        let sub = cx.subscribe_in(&view, window, |this, _, event: &DiscoverEvent, window, cx| match event {
            DiscoverEvent::Close => {
                this.discover = None;
                cx.notify();
            }
            DiscoverEvent::Invite => {
                this.discover = None;
                this.go(Mode::Join, window, cx);
            }
            DiscoverEvent::Joined(joined) => {
                crate::trace(&format!("başlangıç: keşfetten {} projesine girildi", joined.project.name));
                this.complete(joined.organization.clone(), window, cx);
            }
        });
        crate::trace("başlangıç: keşfet açıldı");
        self.discover = Some((view, sub));
        cx.notify();
    }

    /// Otomatik pilot: sıradaki kodu yaz ve önizle (`kod|join`: geçerliyse katıl).
    fn next_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.steps.pop() else { return };
        let (code, join) = step.split_once('|').map(|(c, j)| (c.to_string(), j == "join")).unwrap_or((step.clone(), false));
        crate::trace(&format!("başlangıç: adım {code}{}", if join { " (katıl)" } else { "" }));
        // Kod alanının değişiklik dinleyicisi aynı kodu ikinci kez önizlemesin.
        self.previewed = super::join_project::normalize_code(&code);
        self.preview = None;
        self.code.update(cx, |s, cx| s.set_value(code, window, cx));
        if join {
            self.steps.push("@join".into());
        }
        self.load_preview(window, cx);
    }

    /// Adımlar arası bekleme (görüntü alınabilsin).
    fn step_later(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.steps.is_empty() {
            return;
        }
        let ms = std::env::var("P2_AUTOPILOT_JOIN_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(2500u64);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(ms)).await;
            let _ = this.update_in(cx, |this, window, cx| this.next_step(window, cx));
        })
        .detach();
    }

    fn load_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = super::join_project::normalize_code(&value(&self.code, cx));
        if code.len() < 6 {
            self.set_error(t("onb.join.enter"), cx);
            return;
        }
        self.busy = true;
        self.error = None;
        self.previewed = code.clone();
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || api.invite_preview(&code), |this, r, window, cx| {
            this.busy = false;
            let join_next = this.steps.last().is_some_and(|s| s == "@join");
            if join_next {
                this.steps.pop();
            }
            match r {
                Ok(preview) => {
                    crate::trace(&format!("katıl: önizleme {} ({} üye, {})", preview.project.name, preview.project.member_count, if preview.valid { "geçerli".to_string() } else { preview.reason.clone().unwrap_or_default() }));
                    let go = preview.valid && join_next;
                    this.preview = Some(preview);
                    cx.notify();
                    if go {
                        this.submit(window, cx);
                        return;
                    }
                }
                Err(e) => {
                    crate::trace(&format!("katıl: hata {}", e.code));
                    this.preview = None;
                    this.set_error(super::join_project::join_error(&e), cx);
                }
            }
            this.step_later(window, cx);
        });
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
            Mode::Project => self.project.update(cx, |s, cx| s.focus(window, cx)),
            Mode::Choose => {}
        }
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        // Seçim ekranındaki kod alanı: Enter / "Kodu kontrol et" katılma adımına geçirir.
        if self.mode == Mode::Choose {
            self.mode = Mode::Join;
            self.error = None;
        }
        // Katıl: önce önizleme kartı, sonra (geçerliyse) katılma.
        if self.mode == Mode::Join && !self.preview.as_ref().is_some_and(|p| p.valid && p.code == self.previewed) {
            if self.preview.is_none() {
                self.load_preview(window, cx);
            }
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
                    Err(e) => this.set_error(if e.message.is_empty() { t("onb.setup.failed") } else { e.message }, cx),
                }
            });
        } else if self.mode == Mode::Project {
            let name = value(&self.project, cx).trim().to_string();
            if p2_client_core::project_slug(&name).len() < 2 {
                self.busy = false;
                return self.set_error(t("onb.project.short"), cx);
            }
            let (visibility, category) = (if self.public { p2_client_core::VISIBILITY_PUBLIC } else { p2_client_core::VISIBILITY_PRIVATE }, self.category.filter(|_| self.public));
            task::run(
                window,
                cx,
                move || {
                    let project = api.open_project_with(&name, "", None, visibility, category)?;
                    let orgs = api.organizations()?;
                    Ok::<_, ApiError>((project, orgs))
                },
                |this, r, window, cx| {
                    this.busy = false;
                    match r {
                        Ok((project, orgs)) => {
                            crate::trace(&format!("başlangıç: proje açıldı {} ({})", project.name, if project.visibility == p2_client_core::VISIBILITY_PUBLIC { "herkese açık" } else { "özel" }));
                            if let Some(org) = orgs.into_iter().next() {
                                this.complete(org, window, cx);
                            }
                        }
                        // Sunucuda henüz kuruluş yok: önce kurulum (kuruluş) adı.
                        Err(e) if e.code == "NO_ORGANIZATION" => {
                            this.go(Mode::Create, window, cx);
                            this.set_error(t("onb.setup.needed"), cx);
                        }
                        Err(e) => {
                            crate::trace(&format!("başlangıç: proje açılamadı {}", e.code));
                            this.set_error(if e.message.is_empty() { t("onb.project.failed") } else { e.message }, cx)
                        }
                    }
                },
            );
        } else {
            let code = self.previewed.clone();
            task::run(window, cx, move || api.join_project(&code), |this, r, window, cx| {
                this.busy = false;
                match r {
                    Ok(joined) => {
                        crate::trace(&format!("katıl: {} projesine {}", joined.project.name, if joined.joined { "katıldın" } else { "zaten üyesin" }));
                        this.complete(joined.organization, window, cx)
                    }
                    Err(e) => {
                        crate::trace(&format!("katıl: hata {}", e.code));
                        this.set_error(super::join_project::join_error(&e), cx)
                    }
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

/// Seçenek kartının durumu.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Plain,
    /// Vurgulu gradyan + "ÖNERİLEN" rozeti.
    Featured,
    /// Soluk, tıklanamaz (yetki yok).
    Locked,
}

/// `.org-options > button` (+ `.featured`): r 14, p 22; simge kutusu, başlık, açıklama, alt satırda eylem + ok.
/// `extra`: açıklamanın altındaki isteğe bağlı şerit (ör. açık proje yüzleri).
#[allow(clippy::too_many_arguments)]
fn option(id: &'static str, icon: &'static str, title: String, body: String, cta: String, tone: Tone, extra: Option<AnyElement>, width: f32, height: f32, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (accent, soft, lg) = (p.border_accent, p.accent_soft, p.shadow_lg());
    let featured = tone == Tone::Featured;
    let locked = tone == Tone::Locked;
    // `.featured`: linear-gradient(145deg, accent-soft, rgba(14,22,38,.6)) — alfaları farklı iki durak; Chromium
    // premultiplied karıştırır, GPUI karıştırmaz: ara duraklar önceden hesaplanmış SVG katmanı (hover'da gizlenir).
    // `.featured`: linear-gradient(145deg, accent-soft, koyu/açık cam) — yerel iki duraklı gradyan (köşeler yuvarlak kalır).
    let featured_end = if p.dark { css_rgba(14, 22, 38, 0.6) } else { gpui_kit::Hsla { a: 0.6, ..p.bg_2 } };
    let bg: gpui_kit::Background = if featured { gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(p.accent_soft_2, 0.), gpui_kit::linear_color_stop(featured_end, 1.)) } else { p.glass.background() };
    div()
        .id(id)
        .group(id)
        .relative()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .p(px(22.))
        .flex()
        .flex_col()
        .items_start()
        .rounded(radius::LG)
        .border_1()
        .border_color(if featured { p.border_strong } else { p.border })
        .bg(bg)
        .shadow(shadows(&[p.inset_top]))
        .text_color(p.text)
        .map(|s| {
            if locked {
                s.opacity(0.46).cursor_default()
            } else {
                // `transition: transform var(--m-base) var(--ease-pop), …`; hover 3px yukarı + vurgu.
                s.cursor_pointer().transition(p2_ui::motion::M_BASE, p2_ui::motion::EASE_POP).hover(move |s| s.translate(0., -3.).border_color(accent).bg(soft).shadow(lg.clone()))
            }
        })
        .when(featured || locked, |s| {
            s.child(
                div()
                    .absolute()
                    .right(px(15.))
                    .top(px(15.))
                    .px(px(7.))
                    .py(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(5.))
                    .map(|s| if locked { s.bg(p.bg_3).border_1().border_color(p.border).text_color(p.text_3) } else { s.bg(p.accent_grad.background()).shadow(shadows(&[p.accent_glow_strong])).text_color(p.text_on_accent) })
                    .text_size(fs(8.))
                    .line_height(normal_line_height(fs(8.)))
                    .css_weight(900.)
                    .tracking(0.12)
                    .when(locked, |s| s.child(ico(glyph::LOCK, 10., p.text_3)))
                    .child(t(if locked { "onb.create.locked.badge" } else { "onb.recommended" })),
            )
        })
        .child(div().relative().size(px(44.)).mb(px(16.)).flex().items_center().justify_center().border_1().border_color(p.border).rounded(px(13.)).bg(p.accent_soft).child(ico(icon, 20., p.text_accent)))
        .child(div().relative().text_size(fs(17.)).line_height(normal_line_height(fs(17.))).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(title))
        .child(div().relative().mt(px(8.)).text_size(fs(11.5)).line_height(relative(1.5)).text_color(p.text_3).line_clamp(3).child(body))
        .children(extra.map(|e| div().relative().mt(px(12.)).child(e)))
        .child(
            div()
                .relative()
                .w_full()
                .mt_auto()
                .flex()
                .items_center()
                .justify_between()
                .text_size(fs(11.))
                .line_height(normal_line_height(fs(11.)))
                .css_weight(750.)
                .text_color(if locked { p.text_3 } else { p.text_accent })
                .child(cta)
                .child(ico(if locked { glyph::LOCK } else { glyph::ARROW }, 14., if locked { p.text_3 } else { p.text_accent })),
        )
}

impl OnboardingScreen {
    /// Odak halkalı girdi kabuğu (`.org-form input`): solda isteğe bağlı işaret, sağda isteğe bağlı öğe.
    fn field(&self, input: &Entity<InputState>, mono: bool, lead: Option<AnyElement>, trail: Option<AnyElement>, p: &Palette, window: &Window, cx: &mut Context<Self>) -> gpui_kit::Div {
        let focused = gpui_kit::Focusable::focus_handle(input.read(cx), cx).is_focused(window);
        div()
            .h(px(48.))
            .pl(px(14.))
            .pr(px(if trail.is_some() { 6. } else { 14. }))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(radius::MD)
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border_strong })
            .bg(p.bg_3)
            .shadow(if focused { shadows(&[p.ring, p.inset_top]) } else { shadows(&[p.inset_top]) })
            .text_size(fs(13.))
            .css_weight(400.)
            .text_color(p.text)
            .children(lead)
            .child(gpui_kit::component::input::Input::new(input).appearance(false).bordered(false).focus_bordered(false).p(px(0.)).text_size(fs(if mono { 14. } else { 13. })).when(mono, |s| s.font_family(FONT_MONO)).flex_1())
            .children(trail)
    }

    /// Birincil düğme (`.form-actions` sağ): vurgu gradyanı + ok.
    fn primary(&self, id: &'static str, label: String, p: &Palette, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .h(px(36.))
            .px(px(15.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(9.))
            .bg(p.accent_grad.background())
            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
            .text_color(p.text_on_accent)
            .text_size(fs(12.))
            .css_weight(800.)
            .when(self.busy, |s| s.opacity(0.6))
            .when(!self.busy, |s| s.cursor_pointer().hover(|s| s.brightness(1.08).translate(0., -1.)))
            .child(label)
            .child(ico(glyph::ARROW, 14., p.text_on_accent))
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
    }

    /// Seçim ekranı: kod alanı + "ya da" + göz at / oluştur kartları.
    fn choose(&self, flow_w: f32, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let hash = div().flex_none().text_color(p.text_accent).font_family(FONT_MONO).text_size(fs(14.)).font_weight(FontWeight::BOLD).child("#").into_any_element();
        let check = self.primary("org-code-go", if self.busy { t("onb.busy") } else { t("onb.code.go") }, p, cx).into_any_element();
        let code = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().flex().items_center().gap(px(7.)).child(ico(glyph::KEY, 13., p.text_accent)).child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text_2).child(t("onb.code.label"))).child(div().flex_1()).child(div().text_size(fs(10.)).text_color(p.text_4).child(t("onb.code.note"))))
            .child(self.field(&self.code, true, Some(hash), Some(check), p, window, cx))
            .p2_enter("org-code", Entrance::Rise, Duration::from_millis(40));
        let line = || div().flex_1().h(px(1.)).bg(p.border_subtle);
        let or = div().w_full().my(px(16.)).flex().items_center().gap(px(12.)).child(line()).child(div().text_size(fs(9.)).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_4).child(t("onb.or"))).child(line());

        // "Projelere göz at" özeti: ilk dört açık projenin yüzü + sayı.
        let teaser = self.teaser.as_ref().map(|list| {
            let mut row = div().flex().items_center();
            for (i, pr) in list.projects.iter().take(4).enumerate() {
                row = row.child(div().when(i > 0, |s| s.ml(px(-7.))).p(px(2.)).rounded(px(9.)).bg(p.bg_2).child(dui::project_face(&pr.id, &pr.name, pr.icon_url.as_deref(), 24., 7., p)));
            }
            let label = if list.total == 0 { t("onb.browse.none") } else { tv("onb.browse.count", &[("n", &list.total.to_string())]) };
            row.child(div().ml(px(if list.projects.is_empty() { 0. } else { 9. })).text_size(fs(10.5)).css_weight(700.).text_color(p.text_2).child(label)).into_any_element()
        });

        let gap = 14.;
        let card_w = ((flow_w - gap) / 2.).floor();
        let card_h = if self.home.is_some() { 222. } else { 228. };
        // Ana sayfa girişi: arkadaşlar ve birebir mesajlar projelerden bağımsızdır.
        let home_row = self.home.as_ref().map(|(_, unread)| {
            let (accent, soft, strong) = (p.border_accent, p.accent_soft, p.text_strong);
            let unread = *unread;
            div()
                .id("org-home")
                .w_full()
                .mt(px(gap))
                .h(px(54.))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(radius::LG)
                .border_1()
                .border_color(p.border)
                .bg(p.glass.background())
                .shadow(shadows(&[p.inset_top]))
                .cursor_pointer()
                .transition(p2_ui::motion::M_BASE, p2_ui::motion::EASE_POP)
                .hover(move |s| s.translate(0., -2.).border_color(accent).bg(soft).text_color(strong))
                .child(div().size(px(32.)).flex_none().flex().items_center().justify_center().rounded(px(10.)).border_1().border_color(p.border).bg(p.accent_soft).child(ico(glyph::USERS, 16., p.text_accent)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(3.)).child(div().text_size(fs(12.5)).css_weight(750.).text_color(p.text_strong).child(t("onb.home.title"))).child(div().text_size(fs(10.5)).text_color(p.text_3).truncate().child(t("onb.home.body"))))
                .when(unread > 0, |s| s.child(div().h(px(20.)).px(px(8.)).flex_none().flex().items_center().rounded(radius::PILL).bg(p.danger).text_color(gpui_kit::white()).text_size(fs(10.)).css_weight(800.).child(tv("onb.home.unread", &[("n", &unread.to_string())]))))
                .child(ico(glyph::ARROW, 14., p.text_accent))
                .on_click(cx.listener(|this, _, window, cx| this.open_home(window, cx)))
                .p2_enter("org-home", Entrance::Rise, Duration::from_millis(160))
        });
        let browse = option("org-browse", glyph::COMPASS, t("onb.browse.title"), t("onb.browse.body"), t("onb.browse.cta"), if self.first_install { Tone::Plain } else { Tone::Featured }, teaser, card_w, card_h, p)
            .on_click(cx.listener(|this, _, window, cx| this.open_discover(window, cx)))
            .p2_enter("org-browse", Entrance::Rise, Duration::from_millis(if self.first_install { 125 } else { 90 }));
        let second = if self.first_install {
            option("org-create", glyph::SPARK, t("onb.setup.title"), t("onb.setup.body"), t("onb.setup.cta"), Tone::Featured, None, card_w, card_h, p)
                .on_click(cx.listener(|this, _, window, cx| this.go(Mode::Create, window, cx)))
                .p2_enter("org-create", Entrance::Rise, Duration::from_millis(90))
                .into_any_element()
        } else if self.can_create == Some(false) {
            // Yetki yok: soluk, tıklanamaz; ipucu nedenini söyler.
            let tip: SharedString = t("onb.create.locked").into();
            option("org-project", glyph::PLUS, t("onb.create.title"), t("onb.create.body"), t("onb.create.locked.cta"), Tone::Locked, None, card_w, card_h, p)
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                .p2_enter("org-project", Entrance::Rise, Duration::from_millis(125))
                .into_any_element()
        } else {
            option("org-project", glyph::PLUS, t("onb.create.title"), t("onb.create.body"), t("onb.create.cta"), Tone::Plain, None, card_w, card_h, p)
                .on_click(cx.listener(|this, _, window, cx| this.go(Mode::Project, window, cx)))
                .p2_enter("org-project", Entrance::Rise, Duration::from_millis(125))
                .into_any_element()
        };
        let cards = div().w_full().flex().gap(px(gap)).map(|s| if self.first_install { s.child(second).child(browse) } else { s.child(browse).child(second) });
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(code)
            .when_some(self.error.clone(), |s, (m, e)| s.child(div().mt(px(12.)).child(error_box(m, e, p))))
            .child(or)
            .child(cards)
            .children(home_row)
            .into_any_element()
    }

    /// `.org-form`: etiket + girdi + not, (yeni proje: görünürlük + kategori), hata, `.form-actions` (Geri / birincil).
    fn form(&self, flow_w: f32, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (label, input, note) = match self.mode {
            Mode::Create => (t("onb.setup.label"), &self.name, t("onb.setup.note")),
            Mode::Project => (t("onb.project.label"), &self.project, t("onb.project.note")),
            _ => (t("onb.join.label"), &self.code, t("onb.join.note")),
        };
        let join = self.mode == Mode::Join;
        let lead = join.then(|| div().flex_none().text_color(p.text_accent).font_family(FONT_MONO).text_size(fs(14.)).font_weight(FontWeight::BOLD).child("#").into_any_element());
        let field = self.field(input, join, lead, None, p, window, cx);
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let label_text = if self.busy {
            t("onb.busy")
        } else {
            match self.mode {
                Mode::Create => t("onb.setup.submit"),
                Mode::Project => t("onb.project.submit"),
                _ if self.preview.as_ref().is_some_and(|p| p.valid) => t("onb.join.submit"),
                _ => t("onb.code.go"),
            }
        };
        // Kart formun iç genişliğinde (form dolgusu 2×24 + kenarlık).
        let card_w = flow_w - 50.;
        let preview = join.then(|| self.preview.as_ref().map(|pv| super::join_project::preview_card(pv, card_w, p))).flatten();

        // Yeni proje: görünürlük karoları; herkese açıksa Keşfet kategorisi.
        let visibility = (self.mode == Mode::Project).then(|| {
            let tiles = div()
                .flex()
                .gap(px(10.))
                .child(dui::choice_tile("org-vis-private", glyph::LOCK, t("vis.private"), t("vis.private.hint"), !self.public, p).on_click(cx.listener(|this, _, _, cx| {
                    this.public = false;
                    cx.notify();
                })))
                .child(dui::choice_tile("org-vis-public", glyph::GLOBE, t("vis.public"), t("vis.public.hint"), self.public, p).on_click(cx.listener(|this, _, _, cx| {
                    this.public = true;
                    cx.notify();
                })));
            let mut block = div().mt(px(16.)).flex().flex_col().gap(px(9.)).child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text_2).child(t("vis.label"))).child(tiles);
            if self.public {
                let mut chips = div().flex().flex_wrap().gap(px(7.));
                for id in p2_client_core::DISCOVER_CATEGORIES {
                    let on = self.category == Some(*id);
                    chips = chips.child(dui::category_chip(SharedString::from(format!("org-cat-{id}")), dui::category_glyph(id), dui::category_label(id), None, on, p).on_click(cx.listener(move |this, _, _, cx| {
                        this.category = if this.category == Some(*id) { None } else { Some(*id) };
                        cx.notify();
                    })));
                }
                block = block.child(div().mt(px(6.)).flex().flex_col().gap(px(8.)).child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text_2).child(t("vis.category"))).child(chips).p2_enter("org-cats", Entrance::Rise, Duration::ZERO));
            }
            block
        });

        div()
            .w_full()
            .p(px(24.))
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
            .children(visibility)
            .when_some(preview, |s, card| s.child(div().mt(px(16.)).child(card)))
            .when_some(self.error.clone(), |s, (m, e)| s.child(div().mt(px(16.)).child(error_box(m, e, p))))
            .child(
                div()
                    .mt(px(22.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .id("back")
                            .h(px(36.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .rounded(px(9.))
                            .text_size(fs(12.))
                            .css_weight(650.)
                            .text_color(p.text_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_bg).text_color(strong))
                            .child(t("onb.back"))
                            .on_click(cx.listener(|this, _, window, cx| this.go(Mode::Choose, window, cx))),
                    )
                    .child(self.primary("submit", label_text, p, cx)),
            )
            .p2_enter(("org-form", self.mode as usize), Entrance::Rise, Duration::ZERO)
            .into_any_element()
    }
}

impl OnboardingScreen {
    /// Sağ üstte "Sunucu yönetimi" bağlantısı: yalnız herhangi bir `server.*` yetkisi olana (projesi olmayan
    /// sunucu yöneticisi paneli buradan açar).
    fn server_admin_link(&self, p: &p2_ui::tokens::Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let app = self.app.upgrade()?;
        if !app.read(cx).auth.as_ref().is_some_and(|a| a.user.server_panel()) {
            return None;
        }
        let (hover, strong) = (p.bg_hover, p.text_strong);
        Some(
            div()
                .id("onb-server-admin")
                .absolute()
                .top(px(16.))
                .right(px(16.))
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(7.))
                .border_1()
                .border_color(p.border)
                .rounded(radius::MD)
                .bg(p.glass_strong.background())
                .text_color(p.text_2)
                .text_size(fs(12.))
                .css_weight(650.)
                .cursor_pointer()
                .hover(move |s| s.bg(hover).text_color(strong))
                .child(p2_ui::icons::Icon::raw(super::server_admin::SHIELD_ICON).el().size(px(15.)).text_color(p.text_accent))
                .child(t("srv.title"))
                .on_click(cx.listener(|this, _, window, cx| {
                    let app = this.app.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = app.update(cx, |app, cx| app.open_server_admin(None, window, cx));
                    });
                }))
                .into_any_element(),
        )
    }
}

impl Render for OnboardingScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let vp = window.viewport_size();
        // `.org-page` başlık çubuğunun (38px) altında kalan alanı doldurur.
        let (page_w, page_h) = (f32::from(vp.width), f32::from(vp.height) - 38.);

        // Keşfet açıkken ekranın tamamını o kaplar.
        if let Some((view, _)) = self.discover.as_ref() {
            return div().relative().w(px(page_w)).h(px(page_h)).child(view.clone()).into_any_element();
        }

        if let Some(org) = self.created.clone() {
            let card = glass_card(&p)
                .w(px(520.))
                .p(px(40.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(14.))
                .child(div().size(px(56.)).rounded_full().flex().items_center().justify_center().bg(p.success_soft).text_color(p.success).text_size(px(26.)).child("✓").p2_enter("mark", Entrance::PopElastic, Duration::from_millis(80)))
                .child(eyebrow(t("onb.ready.kicker"), &p))
                .child(div().text_size(px(30.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(org.name.clone()))
                .child(div().text_size(font_size::MD).text_color(p.text_3).text_center().child(t("onb.ready.body")))
                .child(div().w_full().child(primary_button("enter", t("onb.ready.enter"), false, &p).on_click(cx.listener(move |this, _, window, cx| this.complete(org.clone(), window, cx)))));
            return div()
                .relative()
                .w(px(page_w))
                .h(px(page_h))
                .child(div().absolute().top_0().left_0().child(org_background(page_w, page_h, &p)))
                .child(div().absolute().inset_0().flex().items_center().justify_center().child(div().child(card).p2_enter("created", Entrance::PopElastic, Duration::ZERO)))
                .into_any_element();
        }

        let (step, title, subtitle) = match self.mode {
            Mode::Choose => (t("onb.choose.step"), t("onb.choose.title"), t("onb.choose.subtitle")),
            Mode::Create => (t("onb.setup.step"), t("onb.setup.head"), t("onb.setup.sub")),
            Mode::Join => (t("onb.join.step"), t("onb.join.head"), t("onb.join.sub")),
            Mode::Project => (t("onb.project.step"), t("onb.project.head"), t("onb.project.sub")),
        };

        // Kabuk: min(1100px, 94vw); dar pencerede tanıtım sütunu gizlenir (akış tam genişlik).
        let shell_w = 1100f32.min(page_w * 0.94);
        let shell_h = 584f32.min(page_h - 32.).max(480.);
        let intro_w = if page_w >= 1000. { 340. } else { 0. };
        let pad_x = if page_w >= 1000. { 50. } else { 36. };
        let flow_w = shell_w - intro_w - 2. - 2. * pad_x;

        // `.org-intro`: 155° koyu mavi gradyan, sağ kenar; sağ altta 260px halka.
        let checks = [t("onb.check.1"), t("onb.check.2"), t("onb.check.3")];
        let mut list = div().mt(px(34.)).flex().flex_col().gap(px(13.)).text_size(fs(12.)).css_weight(600.).text_color(p.text);
        for (i, text) in checks.into_iter().enumerate() {
            list = list.child(
                div()
                    .h(px(19.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().size(px(19.)).flex_none().flex().items_center().justify_center().rounded(px(6.)).bg(p.accent_soft).child(ico(glyph::CHECK, 11., p.text_accent)))
                    .child(text)
                    .p2_enter(("org-li", i), Entrance::SlideLeft, Duration::from_millis(80 * i as u64)),
            );
        }
        let intro = (intro_w > 0.).then(|| {
            div()
                .relative()
                .w(px(intro_w))
                .flex_none()
                .overflow_hidden()
                .py(px(40.))
                .px(px(34.))
                .flex()
                .flex_col()
                .border_r_1()
                .border_color(p.border_subtle)
                // `linear-gradient(155deg, blue-900 0, bg-2 52%, bg-1 100%)`.
                .rounded_l(px(17.))
                .bg(gpui_kit::linear_gradient(155., gpui_kit::linear_color_stop(p.blue_900, 0.), gpui_kit::linear_color_stop(p.bg_1, 1.)))
                // Sağ alt köşede yumuşak vurgu parıltısı (kenarsız radyal katman) ve iki ince halka.
                .child(p2_ui::components::radial_layer(intro_w - 1., shell_h - 2., (1.02, 1.02), &[(p.accent_soft_2, 0.), (gpui_kit::Hsla { a: 0., ..p.accent_soft_2 }, 0.5)]).absolute().top_0().left_0())
                .child(div().absolute().size(px(260.)).right(px(-125.)).bottom(px(-110.)).rounded_full().border_1().border_color(p.border_strong))
                .child(div().absolute().size(px(150.)).right(px(-70.)).bottom(px(-55.)).rounded_full().border_1().border_color(p.border))
                // `.org-brand` 48×48 kutu, içindeki `.p2-logo.lg` 64px taşar.
                .child(div().relative().size(px(48.)).mb(px(56.)).child(logo_image(64., &p).p2_enter("org-logo", Entrance::LogoPop, Duration::from_millis(120))))
                .child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .child(div().mt(px(8.)).h(p2_ui::text::scaled(px(10.))).text_size(font_size::XXS).line_height(px(10.)).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child(t("onb.brand")))
                        .child(div().max_w(px(270.)).mt(px(10.)).mb(px(14.)).text_size(fs(29.)).line_height(relative(1.1)).css_weight(700.).tracking(-0.04).text_color(p.text_strong).child(t("onb.intro.title")))
                        .child(div().max_w(px(270.)).text_size(fs(12.5)).line_height(relative(1.6)).text_color(p.text_3).child(t("onb.intro.body"))),
                )
                .child(list.relative())
                .child(div().relative().mt_auto().max_w(px(230.)).text_size(fs(10.5)).line_height(relative(1.5)).text_color(p.text_4).child(t("onb.intro.foot")))
        });

        let header = div()
            .w_full()
            .mb(px(if self.mode == Mode::Choose { 24. } else { 28. }))
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
            .child(div().mt(px(13.)).mb(px(9.)).text_size(fs(if flow_w >= 560. { 34. } else { 28. })).line_height(relative(1.06)).css_weight(700.).tracking(-0.045).text_color(p.text_strong).child(title))
            .child(div().max_w(px(560.)).text_size(fs(13.)).line_height(relative(1.55)).text_color(p.text_3).child(subtitle));
        let body: AnyElement = if self.mode == Mode::Choose { self.choose(flow_w, &p, window, cx) } else { self.form(flow_w, &p, window, cx) };
        let flow = div().flex_1().min_w_0().py(px(38.)).px(px(pad_x)).flex().flex_col().justify_center().child(header).child(body);

        // `.org-shell`: `glass-strong`, `shadow-lg`, r 18.
        let shell = div()
            .w(px(shell_w))
            .min_h(px(shell_h))
            .flex()
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .children(intro)
            .child(flow);
        div()
            .relative()
            .w(px(page_w))
            .h(px(page_h))
            .child(div().absolute().top_0().left_0().child(org_background(page_w, page_h, &p)))
            .child(div().absolute().inset_0().p(px(16.)).flex().items_center().justify_center().child(div().child(shell).p2_enter("org", Entrance::PopElastic, Duration::ZERO)))
            .children(self.server_admin_link(&p, cx))
            .p2_enter("org-page", Entrance::Page, Duration::ZERO)
            .into_any_element()
    }
}
