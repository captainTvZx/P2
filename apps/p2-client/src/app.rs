//! Uygulama kökü — App.tsx'teki açılış akışı:
//! kurulum türü → sağlık denetimi (3 deneme) → kurulum yapılmamışsa ilk yönetici →
//! kayıtlı oturum varsa yenile → yoksa giriş → kuruluş kapısı → çalışma alanı.

use std::sync::Arc;
use p2_ui::P2Fx as _; // backdrop_blur
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, SharedString, Window, div};
use p2_client_core::store::{self, InstallMode, Settings};
use p2_client_core::{Api, ApiError, Auth, Organization};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::font_size;

use crate::screens::{AuthScreen, ModeScreen, OnboardingScreen, ProblemScreen, SetupAdminScreen, WorkspaceScreen};
use crate::task;

pub enum Screen {
    Mode(Entity<ModeScreen>),
    Loading(SharedString),
    Problem(Entity<ProblemScreen>),
    Setup(Entity<SetupAdminScreen>),
    Auth(Entity<AuthScreen>),
    Onboarding(Entity<OnboardingScreen>),
    Workspace(Entity<WorkspaceScreen>),
}

pub struct ClientApp {
    pub settings: Settings,
    pub api: Option<Arc<Api>>,
    pub auth: Option<Auth>,
    pub(crate) screen: Screen,
    /// ☰ uygulama menüsü açık mı; `menu_epoch` her aç/kapa'da artar (animasyonu yeniden başlatır).
    pub(crate) menu_open: bool,
    pub(crate) menu_epoch: usize,
    /// UpdateBanner: "Sonra" ile gizlenen durum ve "Şimdi yeniden başlat" sürüyor mu.
    pub(crate) update_hidden: Option<crate::update_banner::Shown>,
    pub(crate) update_installing: bool,
    /// Kapat düğmesi "sor" iken açılan seçim penceresi (ve "bu seçimi hatırla").
    close_ask: Option<bool>,
    /// İlk yönetici az önce kuruldu: başlangıç ekranında "Kurulumu tamamla" (kuruluş oluştur) öne çıkar.
    pub(crate) first_install: bool,
    /// "Sunucu yönetimi" paneli: her ekranın üstünde bindirme (çalışma alanı ya da başlangıç ekranı).
    pub(crate) server_admin: Option<(Entity<crate::screens::server_admin::ServerAdminPanel>, gpui_kit::Subscription)>,
}

/// Parite kancası (yalnız görünüm, veri yazmaz): `P2_AUTOPILOT_SCREEN=setup` sunucu kurulu olsa da ilk
/// yönetici ekranını, `=onboarding` kuruluşu olan kullanıcıda da kuruluş oluştur/katıl ekranını, `=splash` "Yükleniyor…" ekranını açık tutar.
fn parity_screen(name: &str) -> bool {
    std::env::var("P2_AUTOPILOT_SCREEN").is_ok_and(|v| v == name)
}

/// Ekran geçişlerini konsola yazar (tanılama ve otomatik pilot doğrulaması için).
fn trace_screen(name: &str) {
    crate::trace(&format!("ekran: {name}"));
}

/// Restore akışının sonucu (arka plandan dönen).
enum Restored {
    Unreachable,
    NeedsSetup,
    LoggedIn(Auth),
    NeedsLogin,
}

impl ClientApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = store::load_settings();
        let mut app = Self { settings, api: None, auth: None, screen: Screen::Loading("".into()), close_ask: None, menu_open: false, menu_epoch: 0, update_hidden: None, update_installing: false, first_install: false, server_admin: None };
        // Parite aracı: başlık çubuğundaki ☰ uygulama menüsü açık başlar (yalnız görünüm).
        if std::env::var_os("P2_AUTOPILOT_APP_MENU").is_some() {
            app.menu_open = true;
        }
        // Bildirimler her yerden (başka görünüm, arka plan) gelebilir: kök görünüm uyandırılır.
        {
            use futures::StreamExt as _;
            let mut wake = crate::toasts::wake_channel();
            cx.spawn(async move |this, cx| {
                while wake.next().await.is_some() {
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        crate::toasts::autopilot();
        // Tanı: ana iş parçacığı 300 ms'den uzun takılırsa "donma:" satırı (tanı kaydına da) yazılır.
        {
            cx.spawn(async move |_, cx| {
                let mut last = std::time::Instant::now();
                loop {
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                    let now = std::time::Instant::now();
                    let gap = now.duration_since(last);
                    if gap > Duration::from_millis(300) {
                        crate::trace(&format!("donma: ana iş parçacığı {} ms takıldı", gap.as_millis()));
                    }
                    last = now;
                }
            })
            .detach();
        }
        app.setup_shell(window, cx);
        if app.settings.mode == InstallMode::Unset || app.settings.api_url.is_empty() {
            app.show_mode(window, cx);
        } else {
            app.restore(window, cx);
        }
        app
    }

    fn api(&self) -> Arc<Api> {
        self.api.clone().expect("API kurulmadı")
    }

    pub fn show_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let screen = cx.new(|cx| ModeScreen::new(me, window, cx));
        if let Ok(url) = std::env::var("P2_AUTOPILOT_SERVER") {
            screen.update(cx, |s, cx| s.autopilot(&url, window, cx));
        }
        self.screen = Screen::Mode(screen);
        trace_screen("kurulum türü");
        cx.notify();
    }

    /// ModePicker sonrası: adres kaydedilir, akış baştan çalışır.
    pub fn connected(&mut self, api_url: String, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.mode = InstallMode::Client;
        self.settings.api_url = api_url;
        store::save_settings(&self.settings);
        self.restore(window, cx);
    }

    /// "Adresi değiştir" / "Kurulumu sıfırla".
    pub fn change_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.mode = InstallMode::Unset;
        self.settings.api_url.clear();
        store::save_settings(&self.settings);
        self.api = None;
        self.show_mode(window, cx);
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = Arc::new(Api::new(&self.settings.api_url, crate::CLIENT_VERSION));
        self.api = Some(api.clone());
        crate::avatars::install(api.clone());
        self.screen = Screen::Loading(p2_i18n::t("app.loading").into());
        trace_screen("yükleniyor");
        // Parite kancası: açılış ekranında kal (sunucuya hiç gidilmez).
        if parity_screen("splash") {
            cx.notify();
            return;
        }
        cx.notify();
        task::run(
            window,
            cx,
            move || {
                let mut up = false;
                for _ in 0..3 {
                    if api.health().is_ok() {
                        up = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(400));
                }
                if !up {
                    return Restored::Unreachable;
                }
                match api.setup_installed() {
                    Ok(false) => return Restored::NeedsSetup,
                    Err(e) if e.unreachable => return Restored::Unreachable,
                    _ => {}
                }
                let Some(token) = store::load_session() else { return Restored::NeedsLogin };
                match api.refresh(&token) {
                    Ok(auth) => {
                        api.set_auth(&auth, true);
                        Restored::LoggedIn(auth)
                    }
                    Err(e) => {
                        // Sunucu jetonu reddettiyse (kodlu hata) kayıt silinir; ağ hatasında tutulur.
                        if !e.code.is_empty() {
                            store::clear_session();
                        }
                        Restored::NeedsLogin
                    }
                }
            },
            |this, result, window, cx| match parity_screen("setup").then_some(Restored::NeedsSetup).unwrap_or(result) {
                Restored::Unreachable => this.show_problem(window, cx),
                Restored::NeedsSetup => {
                    let me = cx.entity().downgrade();
                    this.screen = Screen::Setup(cx.new(|cx| SetupAdminScreen::new(me, window, cx)));
                    trace_screen("ilk yönetici");
                    cx.notify();
                }
                Restored::LoggedIn(auth) => this.authenticated(auth, true, window, cx),
                Restored::NeedsLogin => this.show_auth(window, cx),
            },
        );
    }

    fn show_problem(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        trace_screen("sunucuya bağlanamadık");
        let me = cx.entity().downgrade();
        let url = self.settings.api_url.clone();
        self.screen = Screen::Problem(cx.new(|cx| ProblemScreen::new(me, url, window, cx)));
        cx.notify();
    }

    pub fn show_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let api = self.api();
        let screen = cx.new(|cx| AuthScreen::new(me, api, window, cx));
        if let (Ok(user), Ok(pass)) = (std::env::var("P2_AUTOPILOT_USER"), std::env::var("P2_AUTOPILOT_PASS")) {
            screen.update(cx, |s, cx| s.autopilot(&user, &pass, window, cx));
        }
        // Parite aracı: giriş yapmadan kayıt / 2FA görünümü (P2_AUTOPILOT_AUTH_VIEW=register|2fa).
        if let Ok(view) = std::env::var("P2_AUTOPILOT_AUTH_VIEW") {
            screen.update(cx, |s, cx| s.parity_view(&view, window, cx));
        }
        self.screen = Screen::Auth(screen);
        trace_screen("giriş");
        cx.notify();
    }

    /// Giriş / kayıt / ilk yönetici / yenileme sonrası.
    pub fn authenticated(&mut self, auth: Auth, remember: bool, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api();
        api.set_auth(&auth, remember);
        crate::avatars::remember(&auth.user.id, auth.user.avatar_url.as_deref());
        self.auth = Some(auth);
        self.screen = Screen::Loading(p2_i18n::t("app.loadingOrgs").into());
        cx.notify();
        // Kuruluş var ama hiçbir projede değilse de başlangıç (katıl / oluştur) ekranı açılır.
        task::run(
            window,
            cx,
            move || api.organizations().map(|orgs| {
                let has_projects = orgs.is_empty() || api.my_projects().map(|p| !p.is_empty()).unwrap_or(true);
                (orgs, has_projects)
            }),
            |this, result: Result<(Vec<Organization>, bool), ApiError>, window, cx| match result {
                Ok((orgs, true)) if !orgs.is_empty() && !parity_screen("onboarding") => this.enter_workspace(orgs[0].clone(), window, cx),
                Ok(_) => this.show_onboarding(window, cx),
                Err(e) if e.code == "SESSION_EXPIRED" => this.logout(window, cx),
                Err(_) => this.show_problem(window, cx),
            },
        );
    }

    /// Başlangıç ekranı (davet koduyla katıl / proje oluştur): kuruluşu ya da projesi olmayan kişi;
    /// son projesinden çıkan (sildi, ayrıldı, çıkarıldı) kişi de buraya döner.
    pub fn show_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let api = self.api();
        let first = self.first_install;
        self.screen = Screen::Onboarding(cx.new(|cx| OnboardingScreen::new(me, api, first, window, cx)));
        trace_screen("kuruluş");
        cx.notify();
    }

    /// "Sunucu yönetimi" paneli (kurulum sahibi ya da herhangi bir `server.*` yetkisi).
    pub fn open_server_admin(&mut self, tab: Option<crate::screens::server_admin::Tab>, window: &mut Window, cx: &mut Context<Self>) {
        use crate::screens::server_admin::{ServerAdminEvent, ServerAdminPanel};
        let Some(auth) = self.auth.clone() else { return };
        if !auth.user.server_panel() {
            return;
        }
        let api = self.api();
        let panel = cx.new(|cx| ServerAdminPanel::new(api, auth.user.id.clone(), tab, window, cx));
        let sub = cx.subscribe_in(&panel, window, |this, _, event: &ServerAdminEvent, window, cx| match event {
            ServerAdminEvent::Close => {
                this.server_admin = None;
                crate::trace("sunucu yönetimi: kapandı");
                cx.notify();
            }
            ServerAdminEvent::Renamed(name) => {
                if let Screen::Workspace(ws) = &this.screen {
                    let name = name.clone();
                    ws.update(cx, |w, cx| w.instance_renamed(name, cx));
                }
            }
            ServerAdminEvent::Entered(value) => {
                let project = serde_json::from_value::<p2_client_core::Project>(value["project"].clone()).ok();
                let org = serde_json::from_value::<Organization>(value["organization"].clone()).ok();
                match (&this.screen, project) {
                    (Screen::Workspace(ws), Some(project)) if ws.read(cx).org_id() == value["organizationId"].as_str().unwrap_or_default() => {
                        ws.update(cx, |w, cx| w.enter_project(project, window, cx));
                    }
                    (_, _) => {
                        if let Some(org) = org {
                            this.enter_workspace(org, window, cx);
                        }
                    }
                }
                this.server_admin = None;
                cx.notify();
            }
        });
        self.server_admin = Some((panel, sub));
        cx.notify();
    }

    /// `server:banned`: sunucudan yasaklandım — oturum kapanır, giriş ekranında neden gösterilir.
    pub fn server_banned(&mut self, reason: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.logout(window, cx);
        let text = match reason.filter(|r| !r.trim().is_empty()) {
            Some(r) => p2_i18n::tv("auth.err.bannedReason", &[("reason", r.trim())]),
            None => p2_i18n::t("auth.err.banned"),
        };
        if let Screen::Auth(auth) = &self.screen {
            auth.update(cx, |s, cx| s.show_error(text, cx));
        }
    }

    /// `server:permissions` olayı: sunucu yetkilerim değişti (proje oluşturma düğmesi, kalkan).
    pub fn server_permissions_changed(&mut self, permissions: Vec<String>, cx: &mut Context<Self>) {
        if let Some(auth) = self.auth.as_mut() {
            auth.user.server_permissions = permissions.clone();
        }
        if self.auth.as_ref().is_some_and(|a| !a.user.server_panel()) {
            self.server_admin = None;
        }
        if let Screen::Workspace(ws) = &self.screen {
            ws.update(cx, |w, cx| w.set_server_permissions(permissions, cx));
        }
        cx.notify();
    }

    pub fn enter_workspace(&mut self, org: Organization, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let auth = self.auth.clone().expect("oturum yok");
        let api = self.api();
        trace_screen(&format!("çalışma alanı ({} · {})", auth.user.username, org.name));
        self.screen = Screen::Workspace(cx.new(|cx| WorkspaceScreen::new(me, api, auth, org, window, cx)));
        cx.notify();
    }

    /// Kabuk: ana pencere tutamacı, tepsi, ikinci örnek / tepsi olayları, kapatma davranışı.
    fn setup_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(hwnd) = crate::shell::hwnd_of(window) {
            crate::MAIN_HWND.store(hwnd.0 as usize, std::sync::atomic::Ordering::Relaxed);
            // main.cjs pinWindowIcon: görev çubuğu simgesi + AppUserModelID pencereye de yazılır.
            crate::shell::apply_app_details(hwnd);
        }
        let Some((tx, rx)) = crate::SHELL.lock().unwrap().take() else { return };
        if !(std::env::var_os("P2_NO_TRAY").is_some() || (crate::shell::quiet() && std::env::var_os("P2_TRAY").is_none())) && crate::shell::start_tray(tx) {
            crate::trace("kabuk: tepsi simgesi hazır");
        }
        let (ftx, mut frx) = futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            while let Ok(ev) = rx.recv() {
                if ftx.unbounded_send(ev).is_err() {
                    break;
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            use futures::StreamExt as _;
            while let Some(ev) = frx.next().await {
                if this.update_in(cx, |app, window, cx| app.on_shell(ev, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // Profil fotoğrafları arka planda iner: inince pencereler yeniden çizilir.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
            if crate::avatars::DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed) && this.update(cx, |_, cx| cx.refresh_windows()).is_err() {
                break;
            }
        })
        .detach();
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| weak.update(cx, |app, cx| app.should_close(window, cx)).unwrap_or(true));
    }

    fn hwnd(&self) -> windows::Win32::Foundation::HWND {
        windows::Win32::Foundation::HWND(crate::MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed) as _)
    }

    fn show_window(&self) {
        let hwnd = self.hwnd();
        if !hwnd.is_invalid() {
            crate::shell::show(hwnd);
            crate::shell::flash(hwnd, false);
        }
        crate::trace(&format!("pencere: gösterildi (görünür={})", !hwnd.is_invalid() && crate::shell::is_visible(hwnd)));
    }

    fn on_shell(&mut self, ev: crate::shell::ShellEvent, window: &mut Window, cx: &mut Context<Self>) {
        use crate::shell::ShellEvent;
        match ev {
            ShellEvent::Show | ShellEvent::NotificationClicked => self.show_window(),
            ShellEvent::Quit => {
                crate::trace("kabuk: tepsiden çıkılıyor");
                crate::shell::remove_tray();
                cx.quit();
            }
            ShellEvent::Args(args) => {
                crate::trace(&format!("kabuk: ikinci örnek {args:?}"));
                self.show_window();
                if let Some(project) = crate::shell::play_target(&args) {
                    crate::trace(&format!("kabuk: p2://play {project}"));
                    if let Screen::Workspace(ws) = &self.screen {
                        ws.update(cx, |w, cx| w.play_from_link(&project, window, cx));
                    }
                }
            }
        }
    }

    /// Kapat düğmesi (main.cjs `window.on("close")`): true = pencere kapanır.
    /// ☰ "Sayfayı yenile" (Electron `location.reload()`): açılış akışı baştan çalışır.
    pub(crate) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace("kabuk: sayfa yenileniyor");
        if self.settings.mode == InstallMode::Unset || self.settings.api_url.is_empty() {
            self.show_mode(window, cx);
        } else {
            self.restore(window, cx);
        }
    }

    pub(crate) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let action = self.settings.app.close_action.clone();
        crate::trace(&format!("pencere: kapat isteği ({action})"));
        self.apply_close(&action, window, cx)
    }

    fn apply_close(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match action {
            "quit" => {
                crate::shell::remove_tray();
                cx.quit();
                true
            }
            "tray" => {
                let hwnd = self.hwnd();
                if !hwnd.is_invalid() {
                    crate::shell::hide(hwnd);
                }
                crate::trace(&format!("pencere: tepsiye gizlendi (görünür={})", !hwnd.is_invalid() && crate::shell::is_visible(hwnd)));
                false
            }
            "minimize" => {
                window.minimize_window();
                crate::trace("pencere: simge durumuna küçültüldü");
                false
            }
            _ => {
                self.close_ask = Some(true);
                cx.notify();
                if let Ok(choice) = std::env::var("P2_AUTOPILOT_CLOSE_CHOICE") {
                    let choice: &'static str = match choice.as_str() { "quit" => "quit", "minimize" => "minimize", _ => "tray" };
                    crate::trace("pencere: kapatma sorusu açıldı");
                    let weak = cx.entity().downgrade();
                    window.defer(cx, move |window, cx| {
                        let _ = weak.update(cx, |app, cx| app.choose_close(choice, window, cx));
                    });
                }
                false
            }
        }
    }

    fn choose_close(&mut self, action: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let remember = self.close_ask.take().unwrap_or(true);
        crate::trace(&format!("pencere: kapatma seçimi {action}{}", if remember { " (hatırlandı)" } else { "" }));
        if remember {
            self.settings.app.close_action = action.into();
            store::save_settings(&self.settings);
        }
        cx.notify();
        self.apply_close(action, window, cx);
    }

    pub fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(api) = self.api.clone() {
            cx.background_executor().spawn(async move { api.logout() }).detach();
        }
        self.auth = None;
        self.server_admin = None;
        self.show_auth(window, cx);
    }

}

impl Render for ClientApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::count_render("kök");
        let p = cx.p2().clone();
        let body = match &self.screen {
            Screen::Mode(v) => v.clone().into_any_element(),
            Screen::Problem(v) => v.clone().into_any_element(),
            Screen::Setup(v) => v.clone().into_any_element(),
            Screen::Auth(v) => v.clone().into_any_element(),
            Screen::Onboarding(v) => v.clone().into_any_element(),
            Screen::Workspace(v) => v.clone().into_any_element(),
            Screen::Loading(text) => crate::splash_view::loading(text.clone(), &p, window),
        };
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .text_color(p.text)
            .font_family(crate::screens::settings::settings_look::font_family())
            // Electron :root — 16px × 0,85 ölçek, line-height: normal (Inter 1,21). GPUI varsayılanı φ≈1,618.
            .text_size(p2_ui::tokens::font_size::ROOT)
            .line_height(gpui_kit::relative(p2_ui::tokens::LINE_HEIGHT_NORMAL))
            .child(self.titlebar(&p, window, cx))
            .child(div().relative().flex_1().min_h_0().child(body).children(self.server_admin.as_ref().map(|s| s.0.clone())))
            .children(self.toast_stack(&p, window, cx))
            .children(self.update_overlay(&p, window, cx))
            .children(self.close_dialog(&p, cx))
    }
}

impl ClientApp {
    /// "Kapat düğmesine basınca ne olsun?" (main.cjs `dialog.showMessageBoxSync`).
    fn close_dialog(&self, p: &p2_ui::tokens::Palette, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        let remember = self.close_ask?;
        use p2_ui::components::compact_button;
        use p2_ui::tokens::radius;
        let card = div()
            .id("close-card")
            .w(gpui_kit::px(470.))
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border)
            .bg(p.bg_1)
            .shadow_lg()
            .occlude()
            .child(div().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_accent).child("P2 KAPATILIYOR"))
            .child(div().text_size(font_size::XXL).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("Kapat düğmesine basınca ne olsun?"))
            .child(div().text_color(p.text_2).child("Tepside kalırsa aramalar ve bildirimler çalışmaya devam eder. Bu seçimi daha sonra Ayarlar > Uygulama bölümünden değiştirebilirsin."))
            .child(
                div()
                    .id("close-remember")
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .child(div().size(gpui_kit::px(16.)).rounded(radius::XS).border_1().border_color(if remember { p.accent } else { p.border_strong }).when(remember, |s| s.bg(p.accent)).flex().items_center().justify_center().text_size(font_size::XS).text_color(p.text_on_accent).child(if remember { "✓" } else { "" }))
                    .child(div().text_color(p.text_2).child("Bu seçimi hatırla"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_ask = this.close_ask.map(|r| !r);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    // Büyük yazı boyunda (%110+) üç düğme 470px karta sığmıyor; taşmak yerine alt satıra geçsin.
                    .flex_wrap()
                    .justify_end()
                    .gap_2()
                    .child(compact_button("close-quit", "Uygulamadan çık", false, false, p).on_click(cx.listener(|this, _, window, cx| this.choose_close("quit", window, cx))))
                    .child(compact_button("close-min", "Simge durumuna küçült", false, false, p).on_click(cx.listener(|this, _, window, cx| this.choose_close("minimize", window, cx))))
                    .child(compact_button("close-tray", "Sistem tepsisine küçült", true, false, p).on_click(cx.listener(|this, _, window, cx| this.choose_close("tray", window, cx)))),
            );
        Some(
            div()
                .id("close-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.close_ask = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }
}
