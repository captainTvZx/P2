//! `server-console.html` + satır içi betiği: aynı metinler, yerleşim, renkler ve düğme mantığı.
//!
//! Betikteki akış korunur: `render(s)` her durum geldiğinde (2,5 sn'de bir ve her `broadcast`'te)
//! bütün alanları yazar — `#error` dahil (eylem iletileri bir sonraki durumla `s.error`'a döner).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable as _, Font, FontWeight, Hsla, ImageSource, PathPromptOptions, PromptLevel, RenderImage,
    ScrollHandle, SharedString, Size, Stateful, StyledText, TextRun, Window, canvas, deferred, div, img, px,
};
use p2_server_ops::config::ConfigureInput;
use p2_server_ops::ops::{Ops, OpsOptions, Outcome, ServerState};
use p2_server_ops::update::UpdateState;
use p2_ui::{Entrance, P2Motion as _};

use crate::style::*;

const BACKUP_HINT: &str = "Yedek yükleme mevcut veritabanını değiştirir; eski uploads klasörü geri alınabilir şekilde saklanır.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Host,
    DbPort,
    Username,
    Database,
    Password,
    ApiPort,
    // Sunucu tarayıcısı (p2-server.ini)
    SrvName,
    SrvDescription,
    SrvIcon,
    SrvUrl,
    SrvCode,
    TurnUrls,
    TurnSecret,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Primary,
    Danger,
}

pub struct Console {
    pub ops: Ops,
    pub state: Option<ServerState>,
    pub error_text: String,
    pub backup_text: String,
    pub backup_busy: bool,
    pub include_releases: bool,
    pub ssl: bool,
    pub inputs: Vec<(Field, Entity<InputState>)>,
    /// HTML form doğrulama balonu: (alan, ileti, ne zamandan beri).
    pub bubble: Option<(Field, &'static str, Instant)>,
    last_log: Option<String>,
    log_scroll: ScrollHandle,
    icon: Option<Arc<RenderImage>>,
    bg: Option<(Size<f32>, Arc<RenderImage>)>,
    measured: Rc<Cell<Size<f32>>>,
    last_gen: u64,
    refreshing: bool,
    again: bool,
    last_refresh: Option<Instant>,
    pub update: UpdateState,
    viewport_width: f32,
    pub autopilot: Option<crate::autopilot::Autopilot>,
    /// `p2-server.ini` (sunucu tarayıcısı ve TURN): son okunan hali, Özel mi, listelensin mi, durum metni.
    srv: p2_server_ops::settings::ServerSettings,
    srv_private: bool,
    srv_listed: bool,
    srv_text: String,
    _subs: Vec<gpui_kit::Subscription>,
}

/// Engelleyici işi arka planda çalıştırıp sonucu görünüme geri getirir.
pub fn run<T: Send + 'static>(
    window: &Window,
    cx: &mut Context<Console>,
    job: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut Console, T, &mut Window, &mut Context<Console>) + 'static,
) {
    cx.spawn_in(window, async move |this, cx| {
        let result = cx.background_executor().spawn(async move { job() }).await;
        let _ = this.update_in(cx, move |view, window, cx| done(view, result, window, cx));
    })
    .detach();
}

impl Console {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ops = Ops::new(OpsOptions::default());
        let make = |window: &mut Window, cx: &mut Context<Self>, value: &str, number: bool, masked: bool| {
            let value = value.to_string();
            cx.new(|cx| {
                let mut s = InputState::new(window, cx).default_value(value);
                if number {
                    // type=number: yalnız rakam girilebilir.
                    s = s.pattern(regex::Regex::new(r"^\d*$").unwrap());
                }
                if masked {
                    s = s.masked(true);
                }
                s
            })
        };
        let srv = p2_server_ops::settings::ServerSettings::load_or_create(&p2_server_ops::data_dir());
        let mut inputs = vec![
            (Field::Host, make(window, cx, "127.0.0.1", false, false)),
            (Field::DbPort, make(window, cx, "5432", true, false)),
            (Field::Username, make(window, cx, "postgres", false, false)),
            (Field::Database, make(window, cx, "p2", false, false)),
            (Field::Password, make(window, cx, "", false, true)),
            (Field::ApiPort, make(window, cx, "4000", true, false)),
        ];
        let db_inputs = inputs.len();
        inputs.extend([
            (Field::SrvName, make(window, cx, &srv.name, false, false)),
            (Field::SrvDescription, make(window, cx, &srv.description, false, false)),
            (Field::SrvIcon, make(window, cx, &srv.icon, false, false)),
            (Field::SrvUrl, make(window, cx, &srv.public_url, false, false)),
            (Field::SrvCode, make(window, cx, &srv.access_code, false, false)),
            (Field::TurnUrls, make(window, cx, &srv.turn_urls.join(", "), false, false)),
            (Field::TurnSecret, make(window, cx, &srv.turn_secret, false, true)),
        ]);
        let mut subs = Vec::new();
        for (_, input) in &inputs[db_inputs..] {
            subs.push(cx.subscribe_in(input, window, |this: &mut Self, _, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_server_settings(cx);
                }
            }));
        }
        for (_, input) in &inputs[..db_inputs] {
            subs.push(cx.subscribe_in(input, window, |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                InputEvent::Change => {
                    if this.bubble.take().is_some() {
                        cx.notify();
                    }
                }
                _ => {}
            }));
        }
        window.on_window_should_close(cx, |_, cx| {
            // Kapat: soru yok; API arka planda sürer (ayrık süreç ya da hizmet).
            crate::trace("pencere kapatıldı; API arka planda bırakıldı");
            cx.quit();
            true
        });
        let mut console = Self {
            ops,
            state: None,
            error_text: String::new(),
            backup_text: BACKUP_HINT.into(),
            backup_busy: false,
            include_releases: false,
            ssl: false,
            inputs,
            bubble: None,
            last_log: None,
            log_scroll: ScrollHandle::new(),
            icon: crate::background::server_icon(),
            bg: None,
            measured: Rc::new(Cell::new(Size { width: 0., height: 0. })),
            last_gen: u64::MAX,
            refreshing: false,
            again: false,
            last_refresh: None,
            update: UpdateState::idle(p2_server_ops::VERSION),
            viewport_width: 1000.,
            autopilot: crate::autopilot::Autopilot::from_env(),
            srv_private: srv.access == p2_server_ops::settings::Access::Private,
            srv_listed: srv.listed,
            srv_text: String::new(),
            srv,
            _subs: subs,
        };
        console.update = console.ops.updater().state();
        console.boot(window, cx);
        console
    }

    /// `app.whenReady`: hizmet tanımı denetimi, public IP, API'yi başlatma, güncelleyici.
    fn boot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ops = self.ops.clone();
        std::thread::spawn(move || ops.resolve_public_ip());
        let ops = self.ops.clone();
        run(
            window,
            cx,
            move || {
                ops.ensure_service_descriptor();
                let started = ops.start();
                crate::updates::start(&ops);
                started
            },
            |this, started, _, cx| {
                crate::trace(&format!("açılış başlatma: ok={} {}", started.ok, started.message.clone().unwrap_or_default()));
                this.refresh_now(cx);
            },
        );
        // 250 ms'lik döngü: yayın sayacı, 2,5 sn yenileme, güncelleme katmanı, tek örnek, kapanış.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                if this.update_in(cx, |view, window, cx| view.tick(window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        self.refresh_now(cx);
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::updates::QUIT.load(std::sync::atomic::Ordering::SeqCst) {
            crate::trace("güncelleme kurulumu için kapanılıyor");
            cx.quit();
            return;
        }
        if crate::instance::SHOW.swap(false, std::sync::atomic::Ordering::SeqCst) {
            window.activate_window();
        }
        let update = self.ops.updater().state();
        if update != self.update {
            self.update = update;
            cx.notify();
        }
        if let Some((_, _, since)) = self.bubble {
            if since.elapsed() > Duration::from_secs(5) {
                self.bubble = None;
                cx.notify();
            }
        }
        // Zemin degradesi belge boyutuna göre.
        let size = self.measured.get();
        let stale = match &self.bg {
            None => size.width > 0.,
            Some((s, _)) => (s.width - size.width).abs() > 1. || (s.height - size.height).abs() > 1.,
        };
        if stale && size.width > 0. {
            if let Some(image) = crate::background::radial(size.width, size.height) {
                self.bg = Some((size, image));
                cx.notify();
            }
        }
        let due = self.last_refresh.is_none_or(|t| t.elapsed() >= Duration::from_millis(2500));
        if !self.refreshing && (due || self.ops.generation() != self.last_gen) {
            self.refresh_now(cx);
        }
        if self.autopilot.is_some() {
            crate::autopilot::step(self, window, cx);
        }
    }

    /// `refresh()`: durum arka planda okunur, `render(s)` uygulanır.
    pub fn refresh_now(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            // Süren okuma bitince yeniden okunur (eylemden sonra gelen istek kaybolmasın).
            self.again = true;
            return;
        }
        self.refreshing = true;
        let ops = self.ops.clone();
        let generation = ops.generation();
        cx.spawn(async move |this, cx| {
            let state = cx.background_executor().spawn(async move { ops.state() }).await;
            let _ = this.update(cx, move |view, cx| {
                view.refreshing = false;
                view.last_refresh = Some(Instant::now());
                view.last_gen = generation;
                view.apply(state, cx);
                if std::mem::take(&mut view.again) {
                    view.refresh_now(cx);
                }
            });
        })
        .detach();
    }

    /// Betikteki `render(s)`.
    fn apply(&mut self, s: ServerState, cx: &mut Context<Self>) {
        self.error_text = s.error.clone();
        self.update = s.update.clone();
        if self.last_log.as_deref() != Some(s.log.as_str()) {
            self.last_log = Some(s.log.clone());
            self.log_scroll.scroll_to_bottom();
        }
        self.state = Some(s);
        cx.notify();
    }

    pub fn input(&self, field: Field) -> &Entity<InputState> {
        &self.inputs.iter().find(|(f, _)| *f == field).unwrap().1
    }

    pub fn value(&self, field: Field, cx: &gpui_kit::App) -> String {
        self.input(field).read(cx).value().to_string()
    }

    pub fn set_value(&mut self, field: Field, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.input(field).clone();
        let value = value.to_string();
        input.update(cx, |s, cx| s.set_value(value, window, cx));
    }

    /// Bir eylemin sonucunu `#error`'a yazıp yeniler (betikteki tıklama işleyicileri).
    fn action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        pending: Option<&str>,
        job: impl FnOnce(&Ops) -> Outcome + Send + 'static,
        message: impl FnOnce(&Outcome) -> Option<String> + 'static,
    ) {
        if let Some(text) = pending {
            self.error_text = text.into();
            cx.notify();
        }
        let ops = self.ops.clone();
        run(window, cx, move || job(&ops), move |this, outcome, _, cx| {
            if let Some(text) = message(&outcome) {
                this.error_text = text;
            }
            crate::trace(&format!("eylem sonucu: ok={} ileti={:?}", outcome.ok, outcome.message));
            this.refresh_now(cx);
            cx.notify();
        });
    }

    pub fn click_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.action(window, cx, None, |ops| ops.start(), |r| (!r.ok).then(|| r.message.clone().unwrap_or_else(|| "Başlatılamadı".into())));
    }

    pub fn click_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.action(window, cx, Some("Sunucu durduruluyor…"), |ops| ops.stop(false), |r| {
            Some(if r.ok { "Sunucu durduruldu.".into() } else { r.message.clone().unwrap_or_else(|| "Durdurulamadı.".into()) })
        });
    }

    pub fn click_restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.action(window, cx, Some("Sunucu yeniden başlatılıyor…"), |ops| ops.restart(), |r| {
            Some(if r.ok { "Sunucu yeniden başladı.".into() } else { r.message.clone().unwrap_or_else(|| "Yeniden başlatılamadı".into()) })
        });
    }

    fn click_service_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.action(window, cx, Some("Yönetici izni bekleniyor…"), |ops| ops.install_service(), |r| {
            Some(if r.ok { "Windows servisi kaydedildi.".into() } else { r.message.clone().unwrap_or_else(|| "Servis kaydedilemedi.".into()) })
        });
    }

    fn click_service_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.action(window, cx, Some("Servis kaldırılıyor…"), |ops| ops.remove_service(), |r| {
            Some(if r.ok { "Windows servisi kaldırıldı.".into() } else { "Servis kaldırılamadı.".into() })
        });
    }

    /// Form gönderimi: önce tarayıcı doğrulaması (required, pattern), sonra `configure`.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pattern = regex::Regex::new(r"^(?:[a-z][a-z0-9_]*)$").unwrap();
        // DOM sırası: host, dbPort, username, database, password, apiPort.
        for (field, input) in self.inputs.clone() {
            let value = input.read(cx).value().to_string();
            let problem = if value.is_empty() {
                Some("Lütfen bu alanı doldurun.")
            } else if field == Field::Database && !pattern.is_match(&value) {
                Some("Lütfen istenen biçimle eşleştirin.")
            } else {
                None
            };
            if let Some(message) = problem {
                input.update(cx, |s, cx| s.focus(window, cx));
                self.bubble = Some((field, message, Instant::now()));
                crate::trace(&format!("form doğrulama: {field:?} — {message}"));
                cx.notify();
                return;
            }
        }
        self.bubble = None;
        let number = |s: String| if s.trim().is_empty() { 0. } else { s.trim().parse::<f64>().unwrap_or(f64::NAN) };
        let input = ConfigureInput {
            host: self.value(Field::Host, cx),
            db_port: number(self.value(Field::DbPort, cx)),
            username: self.value(Field::Username, cx),
            database: self.value(Field::Database, cx),
            password: self.value(Field::Password, cx),
            api_port: number(self.value(Field::ApiPort, cx)),
            ssl: self.ssl,
        };
        self.action(window, cx, Some("Yapılandırılıyor…"), move |ops| ops.configure(&input), |r| {
            Some(if r.ok { "Sunucu hazır.".into() } else { r.message.clone().unwrap_or_else(|| "Yapılandırma başarısız.".into()) })
        });
    }

    fn backup_done(&mut self, create: bool, r: Outcome, cx: &mut Context<Self>) {
        self.backup_text = if r.canceled {
            "İşlem iptal edildi.".into()
        } else if r.ok {
            if create { format!("Yedek hazır: {}", r.path.map(|p| p.display().to_string()).unwrap_or_default()) } else { "Yedek yüklendi; P2 Server yeniden başlatıldı.".into() }
        } else {
            r.message.unwrap_or_else(|| "Yedek işlemi başarısız.".into())
        };
        crate::trace(&format!("yedek: {}", self.backup_text));
        self.backup_busy = false;
        self.refresh_now(cx);
        cx.notify();
    }

    /// `backupAction(kind)`.
    pub fn backup_action(&mut self, create: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.backup_busy {
            return;
        }
        self.backup_busy = true;
        self.backup_text = if create { "Yedek hazırlanıyor…".into() } else { "Yedek doğrulanıyor ve yükleniyor…".into() };
        cx.notify();
        if create {
            if self.ops.read_config().filter(|c| c.configured()).is_none() {
                return self.backup_done(true, Outcome::fail("Önce PostgreSQL bağlantısını yapılandır."), cx);
            }
            let include = self.include_releases;
            if let Ok(path) = std::env::var("P2_CONSOLE_BACKUP_PATH") {
                return self.backup_create_at(PathBuf::from(path), include, window, cx);
            }
            let documents = std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Documents")).unwrap_or_default();
            let name = p2_server_ops::backup::console_backup_name();
            let rx = cx.prompt_for_new_path(&documents, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let chosen = rx.await.ok().and_then(|r| r.ok()).flatten();
                let _ = this.update_in(cx, move |view, window, cx| match chosen {
                    None => view.backup_done(true, Outcome::canceled(), cx),
                    Some(mut path) => {
                        // Electron'daki dosya türü süzgeci uzantıyı ekliyordu.
                        if path.extension().is_none() {
                            path.set_extension("p2backup");
                        }
                        view.backup_create_at(path, include, window, cx)
                    }
                });
            })
            .detach();
        } else {
            if let Some(fail) = self.ops.restore_precheck() {
                return self.backup_done(false, fail, cx);
            }
            if let Ok(path) = std::env::var("P2_CONSOLE_RESTORE_PATH") {
                return self.backup_restore_at(PathBuf::from(path), window, cx);
            }
            let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("P2 Server yedeğini yükle".into()) });
            cx.spawn_in(window, async move |this, cx| {
                let chosen = rx.await.ok().and_then(|r| r.ok()).flatten().and_then(|v| v.into_iter().next());
                let _ = this.update_in(cx, move |view, window, cx| match chosen {
                    None => view.backup_done(false, Outcome::canceled(), cx),
                    Some(path) => {
                        let answer = window.prompt(
                            PromptLevel::Warning,
                            "Mevcut veritabanı seçilen yedekle değiştirilecek.",
                            Some("Sunucu işlem sırasında durdurulur. Mevcut uploads klasörü silinmez; geri alınabilmesi için tarihli bir klasöre taşınır."),
                            &["Yedeği yükle", "Vazgeç"],
                            cx,
                        );
                        cx.spawn_in(window, async move |this, cx| {
                            let index = answer.await.unwrap_or(1);
                            let _ = this.update_in(cx, move |view, window, cx| {
                                if index == 0 { view.backup_restore_at(path, window, cx) } else { view.backup_done(false, Outcome::canceled(), cx) }
                            });
                        })
                        .detach();
                    }
                });
            })
            .detach();
        }
    }

    fn backup_create_at(&mut self, path: PathBuf, include: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ops = self.ops.clone();
        run(window, cx, move || ops.create_backup(&path, include), |this, r, _, cx| this.backup_done(true, r, cx));
    }

    /// Sunucu tarayıcısı ayarlarını `p2-server.ini`'ye yazar; çalışan sunucu dosyayı izler (10 sn içinde uygular).
    fn save_server_settings(&mut self, cx: &mut Context<Self>) {
        use p2_server_ops::settings::Access;
        let v = |this: &Self, f: Field| this.value(f, cx).trim().to_string();
        let mut s = self.srv.clone();
        s.name = v(self, Field::SrvName);
        if s.name.is_empty() {
            s.name = "P2 Server".into();
        }
        s.description = v(self, Field::SrvDescription);
        s.icon = v(self, Field::SrvIcon);
        s.public_url = v(self, Field::SrvUrl).trim_end_matches('/').to_string();
        s.access = if self.srv_private { Access::Private } else { Access::Public };
        s.access_code = v(self, Field::SrvCode);
        s.listed = self.srv_listed;
        s.turn_urls = v(self, Field::TurnUrls).split(',').map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect();
        s.turn_secret = v(self, Field::TurnSecret);
        self.srv_text = if s.access == Access::Private && s.access_code.is_empty() {
            "Özel sunucu için katılım kodu yaz.".into()
        } else if s.listed && !(s.public_url.starts_with("https://") || s.public_url.starts_with("http://")) {
            "Sunucu tarayıcısında görünmek için genel adresi yaz (ör. https://p2.ornek.com).".into()
        } else {
            match s.save(&self.ops.data_dir()) {
                Ok(()) => {
                    self.srv = s;
                    crate::trace("sunucu tarayıcısı ayarları kaydedildi (p2-server.ini)");
                    "Kaydedildi. Çalışan sunucu değişikliği birkaç saniye içinde uygular.".into()
                }
                Err(e) => format!("Kaydedilemedi: {e}"),
            }
        };
        cx.notify();
    }

    /// Simge seç: dosya veri klasörüne `server-icon.<uzantı>` olarak kopyalanır (hizmet hesabı da okuyabilsin).
    fn pick_server_icon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Sunucu simgesi (png, jpg, webp)".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let chosen = rx.await.ok().and_then(|r| r.ok()).flatten().and_then(|v| v.into_iter().next());
            let Some(path) = chosen else { return };
            let _ = this.update_in(cx, move |view, window, cx| {
                let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
                if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
                    view.srv_text = "Simge png, jpg ya da webp olmalı.".into();
                    return cx.notify();
                }
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if size > 256 * 1024 {
                    view.srv_text = "Simge en çok 256 KB olmalı (kare, 256–512 px önerilir).".into();
                    return cx.notify();
                }
                let name = format!("server-icon.{ext}");
                match std::fs::copy(&path, view.ops.data_dir().join(&name)) {
                    Ok(_) => {
                        view.set_value(Field::SrvIcon, &name, window, cx);
                        view.save_server_settings(cx);
                    }
                    Err(e) => {
                        view.srv_text = format!("Simge kopyalanamadı: {e}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn backup_restore_at(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let ops = self.ops.clone();
        run(window, cx, move || ops.restore_backup(&path), |this, r, _, cx| this.backup_done(false, r, cx));
    }

    // ------------------------------------------------------------------ çizim yardımcıları

    fn button(&self, id: &'static str, label: &str, kind: Kind, disabled: bool) -> Stateful<Div> {
        let mut b = div()
            .id(id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .px(px(13.))
            .py(px(9.))
            .border_1()
            .rounded(px(R_SM))
            .text_size(px(14.))
            .line_height(px(20.3))
            .font_weight(W650)
            .child(SharedString::from(label.to_string()));
        b = match kind {
            Kind::Plain => b.border_color(border()).bg(bg_3()).text_color(text()),
            Kind::Primary => b.border_color(gpui_kit::transparent_black()).bg(accent_grad(false)).text_color(c(0xffffff)).shadow(shadow_primary()),
            Kind::Danger => b.border_color(danger_border()).bg(danger_soft()).text_color(danger_text()),
        };
        if disabled {
            b.opacity(0.38).cursor_not_allowed()
        } else {
            b.cursor_pointer().hover(move |s| match kind {
                Kind::Plain => s.bg(bg_hover()).border_color(border_strong()).text_color(text_strong()),
                Kind::Primary => s.bg(accent_grad(true)).top(px(-1.)),
                Kind::Danger => s.bg(danger()).border_color(danger()).text_color(c(0xffffff)),
            })
        }
    }

    fn copy_button(id: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .border_1()
            .border_color(border())
            .rounded(px(R_SM))
            .px(px(11.))
            .py(px(7.))
            .bg(bg_hover())
            .text_color(text())
            .text_size(px(12.))
            .line_height(px(17.4))
            .font_weight(W650)
            .cursor_pointer()
            .hover(|s| s.bg(accent_soft()).border_color(border_strong()).text_color(text_strong()))
            .child("Kopyala")
    }

    /// `.endpoint`: başlık + tek satır kod + Kopyala.
    fn endpoint(label: &str, value: String, button: Stateful<Div>) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .p(px(14.))
            .bg(bg_3())
            .border_1()
            .border_color(border())
            .rounded(px(R_MD))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(spaced(label, 1.33).text_size(px(9.5)).line_height(px(13.8)).font_weight(W800).text_color(text_accent()))
                    .child(div().mt(px(3.)).font_family(MONO).text_color(text_strong()).truncate().child(value)),
            )
            .child(button)
    }

    /// `<p class="hint">` (12px, satır yüksekliği 1.5), içinde `<code>` parçaları olabilir.
    fn hint(parts: &[(&str, bool)]) -> AnyElement {
        let mut text = String::new();
        let mut runs = Vec::new();
        for (part, code) in parts {
            let chunk = if *code { format!("\u{202f}{part}\u{202f}") } else { part.to_string() };
            runs.push(TextRun {
                len: chunk.len(),
                font: Font { family: if *code { MONO.into() } else { FONT.into() }, ..gpui_kit::font(FONT) },
                color: text_3(),
                background_color: code.then(bg_3),
                underline: None,
                strikethrough: None,
            });
            text.push_str(&chunk);
        }
        div().text_size(px(12.)).line_height(px(18.)).text_color(text_3()).child(StyledText::new(text).with_runs(runs)).into_any_element()
    }

    fn checkbox(id: &'static str, checked: bool, color: Hsla) -> Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .size(px(13.))
            .rounded(px(2.))
            .border_1()
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .when(checked, |s| s.bg(color).border_color(color).child(div().text_size(px(10.)).line_height(px(10.)).font_weight(FontWeight::BOLD).text_color(c(0xffffff)).child("✓")))
            .when(!checked, |s| s.bg(c(0xffffff)).border_color(c(0x767676)))
    }

    fn field(&self, label: &str, field: Field, window: &Window, cx: &gpui_kit::App) -> Div {
        let state = self.input(field);
        let focus: FocusHandle = state.read(cx).focus_handle(cx);
        let focused = focus.contains_focused(window, cx);
        let mut boxed = div()
            .mt(px(5.))
            .w_full()
            .px(px(11.))
            .py(px(10.))
            .border_1()
            .rounded(px(R_SM))
            .bg(bg_3())
            .text_color(text())
            // `Input::h` yalnız çok satırlı alanı etkiler; tek satır yüksekliği Styled üzerinden verilir.
            .child(gpui_kit::Styled::h(Input::new(state).appearance(false).p_0(), px(17.4)).text_size(px(12.)).font_weight(W600));
        if focused {
            boxed = boxed.border_color(accent_hover()).shadow(vec![gpui_kit::BoxShadow {
                color: ca(79, 157, 255, 0.35),
                offset: gpui_kit::point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(3.),
                inset: false,
            }]);
        } else {
            boxed = boxed.border_color(border());
        }
        let mut cell = div().relative().flex().flex_col().text_size(px(12.)).line_height(px(17.4)).font_weight(W600).text_color(text_2()).child(SharedString::from(label.to_string())).child(boxed);
        if let Some((f, message, _)) = self.bubble {
            if f == field {
                cell = cell.child(deferred(
                    div()
                        .absolute()
                        .top(px(64.))
                        .left(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(10.))
                        .py(px(7.))
                        .bg(c(0xffffff))
                        .border_1()
                        .border_color(c(0xa0a0a0))
                        .rounded(px(3.))
                        .shadow(vec![gpui_kit::BoxShadow { color: ca(0, 0, 0, 0.3), offset: gpui_kit::point(px(0.), px(2.)), blur_radius: px(6.), spread_radius: px(0.), inset: false }])
                        .text_color(c(0x202020))
                        .font_weight(FontWeight::NORMAL)
                        .text_size(px(13.))
                        .child(div().size(px(18.)).rounded(px(2.)).bg(c(0xf2a000)).flex().items_center().justify_center().text_color(c(0xffffff)).font_weight(FontWeight::BOLD).child("!"))
                        .child(message),
                ));
            }
        }
        cell
    }

    fn grow(mut d: Div, g: f32, min: f32) -> Div {
        d.style().flex_grow = Some(g);
        d.style().flex_shrink = Some(1.);
        d.flex_basis(px(0.)).min_w(px(min))
    }

    fn status_card(&self, cx: &mut Context<Self>) -> Div {
        let s = self.state.as_ref();
        let running = s.is_some_and(|s| s.running);
        let configured = s.is_some_and(|s| s.configured);
        let service = s.is_some_and(|s| s.service);
        let address = s.map(|s| s.address.clone()).unwrap_or_else(|| "Public IP alınıyor…".into());
        let meta = |label: &str, value: String| {
            div()
                .flex_1()
                .p(px(11.))
                .border_1()
                .border_color(border_subtle())
                .rounded(px(R_MD))
                .bg(bg_2())
                .text_color(text_3())
                .text_size(px(12.))
                .line_height(px(17.4))
                .child(SharedString::from(label.to_string()))
                .child(div().mt(px(2.)).text_color(text_strong()).text_size(px(14.)).line_height(px(20.3)).font_weight(FontWeight::BOLD).child(value))
        };
        let dash = || "—".to_string();
        let copy_address = address.clone();
        let mut service_row = div().flex().flex_wrap().gap(px(8.));
        // İlk durum gelmeden HTML'deki gibi iki düğme de görünür.
        if s.is_none() || !service {
            service_row = service_row.child(self.button("serviceInstall", "Windows servisi olarak kaydet", Kind::Primary, false).on_click(cx.listener(|this, _, w, cx| this.click_service_install(w, cx))));
        }
        if s.is_none() || service {
            service_row = service_row.child(self.button("serviceRemove", "Servisi kaldır", Kind::Plain, false).on_click(cx.listener(|this, _, w, cx| this.click_service_remove(w, cx))));
        }
        card()
            .child(card_title("Sunucu durumu").mb(px(14.)))
            .child(
                Self::endpoint("PUBLIC İSTEMCİ BAĞLANTI ADRESİ", address, Self::copy_button("copy").on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_address.clone()))))
                    .mb(px(14.)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(self.button("start", "Başlat", Kind::Primary, s.is_some() && (running || !configured)).on_click(cx.listener(move |this, _, w, cx| {
                        if this.state.as_ref().is_none_or(|s| !(s.running || !s.configured)) {
                            this.click_start(w, cx)
                        }
                    })))
                    .child(self.button("restart", "Yeniden başlat", Kind::Plain, s.is_some() && !running).on_click(cx.listener(move |this, _, w, cx| {
                        if this.state.as_ref().is_none_or(|s| s.running) {
                            this.click_restart(w, cx)
                        }
                    })))
                    .child(self.button("stop", "Durdur", Kind::Danger, s.is_some() && !running).on_click(cx.listener(move |this, _, w, cx| {
                        if this.state.as_ref().is_none_or(|s| s.running) {
                            this.click_stop(w, cx)
                        }
                    })))
                    .child(self.button("folder", "Veri klasörü", Kind::Plain, false).on_click(cx.listener(|this, _, _, _| {
                        this.ops.open_data();
                    })))
                    .child(self.button("updateCheck", "Güncelleme denetle", Kind::Plain, false).on_click(cx.listener(|this, _, _, _| {
                        let updater = this.ops.updater();
                        std::thread::spawn(move || updater.check());
                    }))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .my(px(14.))
                    .child(div().flex().gap(px(9.)).child(meta("P2 Server sürümü", s.map(|s| s.version.clone()).unwrap_or_else(dash))).child(meta("API portu", s.map(|s| s.port.to_string()).unwrap_or_else(dash))))
                    .child(
                        div()
                            .flex()
                            .gap(px(9.))
                            .child(meta("Yapılandırma", s.map(|s| if s.configured { "Hazır" } else { "Bekliyor" }.to_string()).unwrap_or_else(dash)))
                            .child(meta("Windows servisi", s.map(|s| if s.service { "Kayıtlı" } else { "Kayıtlı değil" }.to_string()).unwrap_or_else(dash))),
                    ),
            )
            .child(service_row)
            .child(div().mt(px(12.)).child(Self::hint(&[(
                "Konsolu kapatmak sunucuyu durdurmaz; sunucu arka planda çalışmaya devam eder, durdurmak için \"Durdur\". Servis kaydı yönetici izni ister. Kaydedildiğinde P2 Server, kullanıcı oturumu açılmadan bilgisayar açılışında başlar ve konsol kapalıyken çalışır.",
                false,
            )])))
            .child(div().my(px(10.)).text_color(danger_text()).child(SharedString::from(self.error_text.clone())))
    }

    fn config_card(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let pair = |a: Div, b: Div| div().flex().gap(px(11.)).child(a.flex_1().min_w_0()).child(b.flex_1().min_w_0());
        let ssl = self.ssl;
        let check = div()
            .id("ssl")
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .line_height(px(17.4))
            .font_weight(W600)
            .text_color(text_2())
            .cursor_default()
            .child(Self::checkbox("sslBox", ssl, accent()))
            .child("SSL kullan")
            .on_click(cx.listener(|this, _, _, cx| {
                this.ssl = !this.ssl;
                cx.notify();
            }));
        card()
            .child(card_title("PostgreSQL yapılandırması").mb(px(14.)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(11.))
                    .child(pair(self.field("PostgreSQL adresi", Field::Host, window, cx), self.field("PostgreSQL portu", Field::DbPort, window, cx)))
                    .child(pair(self.field("Kullanıcı", Field::Username, window, cx), self.field("Veritabanı", Field::Database, window, cx)))
                    .child(self.field("PostgreSQL parolası", Field::Password, window, cx))
                    .child(div().flex().gap(px(11.)).child(self.field("API portu", Field::ApiPort, window, cx).flex_1().min_w_0()).child(div().flex_1().min_w_0().flex().items_center().child(check)))
                    .child(div().flex().child(self.button("configure", "Kaydet, tabloları hazırla ve başlat", Kind::Primary, false).on_click(cx.listener(|this, _, w, cx| this.submit(w, cx))))),
            )
            .child(div().mt(px(12.)).child(Self::hint(&[("Parola yalnız bu makinedeki sunucu yapılandırmasında saklanır. İlk yönetici hesabı, P2 Client bu sunucuya ilk bağlandığında oluşturulur.", false)])))
    }

    fn server_card(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let pair = |a: Div, b: Div| div().flex().gap(px(11.)).child(a.flex_1().min_w_0()).child(b.flex_1().min_w_0());
        let (private, listed) = (self.srv_private, self.srv_listed);
        let toggle = |id: &'static str, box_id: &'static str, on: bool, label: &'static str| {
            div().id(id).flex().items_center().gap(px(8.)).text_size(px(12.)).line_height(px(17.4)).font_weight(W600).text_color(text_2()).cursor_default().child(Self::checkbox(box_id, on, accent())).child(label)
        };
        card()
            .child(card_title("Sunucu tarayıcısı").mb(px(6.)))
            .child(Self::hint(&[("P2 kullanıcıları bu sunucuyu sunucu tarayıcısında görür. Açık sunucuya seçip girilir; özel sunucu listede görünür ama katılım kodu ister. Ayarlar ", false), ("p2-server.ini", true), (" dosyasına yazılır; sunucu kapalıyken o dosyadan da düzenlenebilir.", false)]))
            .child(
                div()
                    .mt(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(11.))
                    .child(pair(self.field("Sunucu adı", Field::SrvName, window, cx), self.field("Genel adres (https://…)", Field::SrvUrl, window, cx)))
                    .child(self.field("Açıklama", Field::SrvDescription, window, cx))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(11.))
                            .child(self.field("Simge (veri klasörüne göre yol ya da https adresi)", Field::SrvIcon, window, cx).flex_1().min_w_0())
                            .child(self.button("srvIcon", "Simge seç…", Kind::Plain, false).on_click(cx.listener(|this, _, w, cx| this.pick_server_icon(w, cx)))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(18.))
                            .child(toggle("srvListed", "srvListedBox", listed, "Sunucu tarayıcısında listele").on_click(cx.listener(|this, _, _, cx| {
                                this.srv_listed = !this.srv_listed;
                                cx.notify();
                            })))
                            .child(toggle("srvPrivate", "srvPrivateBox", private, "Özel sunucu (kodla katılım)").on_click(cx.listener(|this, _, _, cx| {
                                this.srv_private = !this.srv_private;
                                cx.notify();
                            }))),
                    )
                    .when(private, |d| d.child(self.field("Katılım kodu", Field::SrvCode, window, cx)))
                    .child(card_title("TURN (sesli arama aktarımı)").mt(px(8.)))
                    .child(Self::hint(&[("İki kişi doğrudan bağlanamadığında ses bu sunucu üzerinden aktarılır. coturn kurulumundaki ", false), ("static-auth-secret", true), (" ile aynı gizli anahtarı yaz. Boş bırakılırsa yalnız doğrudan bağlantı denenir.", false)]))
                    .child(pair(self.field("TURN adresleri (virgülle)", Field::TurnUrls, window, cx), self.field("TURN gizli anahtarı", Field::TurnSecret, window, cx)))
                    .child(div().flex().child(self.button("srvSave", "Kaydet", Kind::Primary, false).on_click(cx.listener(|this, _, _, cx| this.save_server_settings(cx))))),
            )
            .when(!self.srv_text.is_empty(), |d| d.child(div().mt(px(12.)).child(Self::hint(&[(&self.srv_text, false)]))))
    }

    fn backup_card(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.backup_busy;
        let include = self.include_releases;
        card()
            .child(card_title("Yedekleme ve taşıma"))
            .child(div().mt(px(14.)).child(Self::hint(&[
                ("Veritabanını ve yüklenen dosyaları tek bir ", false),
                (".p2backup", true),
                (" dosyasına al. Yeni sunucuda önce PostgreSQL bağlantısını yapılandır, ardından bu yedeği yükle.", false),
            ])))
            .child(
                div()
                    .id("backupReleasesRow")
                    .mt(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(text_3())
                    .child(Self::checkbox("backupReleases", include, c(0x0075ff)).mt(px(3.)).mb(px(3.)).ml(px(4.)).mr(px(3.)))
                    .child(div().flex_1().min_w_0().child("Yedeğe kütüphane sürüm dosyalarını da ekle (çok büyük olabilir)"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.include_releases = !this.include_releases;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .mt(px(14.))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(self.button("backupCreate", "Yedek al", Kind::Primary, busy).on_click(cx.listener(|this, _, w, cx| this.backup_action(true, w, cx))))
                    .child(self.button("backupRestore", "Yedek yükle", Kind::Plain, busy).on_click(cx.listener(|this, _, w, cx| this.backup_action(false, w, cx)))),
            )
            .child(div().mt(px(12.)).child(Self::hint(&[(&self.backup_text, false)])))
    }

    fn console_card(&self) -> Div {
        let s = self.state.as_ref();
        let address = s.map(|s| s.address.clone());
        let update_url = address.map(|a| format!("{}/updates/client/latest.yml", a.trim_end_matches('/'))).unwrap_or_else(|| "Public IP alınıyor…".into());
        let copy = update_url.clone();
        let log = match (&self.last_log, s) {
            (None, _) => "Günlük bekleniyor…".to_string(),
            (Some(l), _) if l.is_empty() => "Henüz günlük kaydı yok.".to_string(),
            (Some(l), _) => l.clone(),
        };
        let lines: Vec<AnyElement> = log.split('\n').map(|l| div().child(SharedString::from(l.to_string())).into_any_element()).collect();
        let updates_dir = s.map(|s| s.updates_dir.clone()).unwrap_or_else(|| "updates".into());
        card()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .mb(px(10.))
                    .child(card_title("Güncelleme dağıtımı ve canlı günlük").mb(px(14.)))
                    .child(div().text_color(text_4()).text_size(px(11.)).child(SharedString::from(s.map(|s| s.data_dir.clone()).unwrap_or_default()))),
            )
            .child(Self::endpoint("CLIENT UPDATE URL", update_url, Self::copy_button("copyUpdate").on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))))
            .child(div().mt(px(14.)).child(Self::hint(&[
                ("P2 Client, bağlandığı API adresini otomatik güncelleme kaynağı olarak kullanır. Alan adı/reverse proxy kullanıyorsan istemcilere IP yerine HTTPS alan adını ver. Yayın dosyaları ", false),
                (&updates_dir, true),
                (" klasöründedir.", false),
            ])))
            .child(
                div()
                    .id("log")
                    .h(px(250.))
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .p(px(14.))
                    .bg(bg_0())
                    .border_1()
                    .border_color(border())
                    .rounded(px(R_MD))
                    .text_color(c(0xb9d2ff))
                    .font_family(MONO)
                    .text_size(px(12.))
                    .line_height(px(18.6))
                    .children(lines),
            )
    }

    fn update_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let u = &self.update;
        if u.status == "idle" || u.status == "checking" {
            return None;
        }
        let title = format!("P2 Server güncellemesi{}", if u.version.is_empty() { String::new() } else { format!(" · v{}", u.version) });
        let body = if u.status == "installing" {
            "Sunucu durduruluyor ve güncelleme kuruluyor; P2 Server kendiliğinden yeniden açılacak.".to_string()
        } else if u.status == "ready" {
            if u.error.is_empty() { "Güncelleme indirildi, kurulum başlıyor…".into() } else { format!("Kurulum başarısız: {} · 5 dakika içinde yeniden denenecek.", u.error) }
        } else {
            format!("Güncelleme indiriliyor · %{}", u.percent)
        };
        let width = (self.viewport_width - 40.).min(420.);
        let percent = (u.percent as f32).max(4.).min(100.) / 100.;
        let retry = u.status == "ready" && !u.error.is_empty();
        let key = format!("{}{}{}{}", u.status, u.version, u.percent, u.error);
        let card = div()
            .w(px(width))
            .p(px(22.))
            .border_1()
            .border_color(border_strong())
            .rounded(px(R_XL))
            .bg(glass_strong())
            .shadow(shadow_lg())
            .relative()
            .child(div().mb(px(14.)).text_size(px(16.38)).line_height(px(23.75)).font_weight(FontWeight::BOLD).text_color(text_strong()).child(title))
            .child(div().text_color(text_2()).line_height(px(21.)).child(body))
            .child(div().my(px(14.)).h(px(7.)).rounded(px(99.)).bg(line()).overflow_hidden().child(div().h_full().w(gpui_kit::relative(percent)).bg(accent_bar())))
            .child(div().flex().justify_end().gap(px(8.)).when(retry, |f| {
                f.child(self.button("installUpdate", "Yeniden dene", Kind::Primary, false).on_click(cx.listener(|this, _, _, _| this.ops.updater().install())))
            }));
        Some(
            div()
                .id("updateLayer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(ca(4, 8, 18, 0.62))
                .occlude()
                .child(card.p2_enter(SharedString::from(key), Entrance::PopElastic, Duration::ZERO))
                .into_any_element(),
        )
    }
}

impl Render for Console {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport_width = f32::from(window.viewport_size().width);
        let narrow = self.viewport_width <= 850.;
        let s = self.state.clone();
        let running = s.as_ref().is_some_and(|s| s.running);
        let status = div()
            .flex_none()
            .px(px(13.))
            .py(px(9.))
            .rounded(px(999.))
            .border_1()
            .font_weight(W650)
            .when(running, |d| d.border_color(ca(61, 220, 132, 0.4)).bg(success_soft()).text_color(success_text()))
            .when(!running, |d| d.border_color(danger_border()).bg(danger_soft()).text_color(danger_text()))
            .child(match &s {
                None => "Denetleniyor…",
                Some(s) if s.running => "Çalışıyor",
                Some(_) => "Durduruldu",
            });
        let logo = div()
            .size(px(48.))
            .flex_none()
            .rounded(px(12.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(accent_grad(false))
            .shadow(shadow_logo())
            .text_color(c(0xffffff))
            .font_weight(W800)
            .text_size(px(19.))
            .map(|d| match &self.icon {
                Some(icon) => d.child(img(ImageSource::Render(icon.clone())).size_full().rounded(px(12.))),
                None => d.child("P2"),
            });
        let version = s.as_ref().map(|s| format!("v{}", s.version)).unwrap_or_default();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .mb(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(logo.p2_enter("logo", Entrance::PopElastic, Duration::from_millis(100)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(5.))
                                    .child(spaced("P2 Server Console", -0.42).text_size(px(21.)).line_height(px(30.45)).font_weight(FontWeight::BOLD).text_color(text_strong()))
                                    .child(spaced(&version, 0.88).text_size(px(11.)).font_weight(W800).text_color(text_accent())),
                            )
                            .child(div().mt(px(2.)).text_color(text_3()).child("API, dosyalar ve güncelleme dağıtım sunucusu")),
                    ),
            )
            .child(status)
            .p2_enter("top", Entrance::Rise, Duration::ZERO);

        let cards: [Div; 5] = [self.status_card(cx), self.config_card(window, cx), self.backup_card(cx), self.console_card(), self.server_card(window, cx)];
        let [c1, c2, c3, c4, c5] = cards;
        let enter = |d: Div, i: usize| d.p2_enter(("card", i), Entrance::Rise, Duration::from_millis(60 * i as u64));
        let grid = if narrow {
            div().flex().flex_col().gap(px(18.)).child(enter(c1, 0)).child(enter(c2, 1)).child(enter(c3, 2)).child(enter(c5, 4)).child(enter(c4, 3))
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(div().flex().gap(px(18.)).child(Self::grow(div().flex().flex_col(), 0.9, 340.).child(enter(c1.flex_1(), 0))).child(Self::grow(div().flex().flex_col(), 1.35, 390.).child(enter(c2.flex_1(), 1))))
                .child(div().flex().gap(px(18.)).child(Self::grow(div().flex().flex_col(), 0.9, 340.).child(enter(c3, 2))).child(Self::grow(div().flex().flex_col(), 1.35, 390.).child(enter(c5, 4))))
                .child(enter(c4, 3))
        };

        let measured = self.measured.clone();
        let content = div()
            .relative()
            .w_full()
            .pt(px(26.))
            .pb(px(26.))
            .pl(px(26.))
            .pr(px(36.))
            .child(
                canvas(move |bounds, _, _| measured.set(Size { width: f32::from(bounds.size.width) - 10., height: f32::from(bounds.size.height) }), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .when_some(self.bg.clone(), |d, (size, image)| d.child(img(ImageSource::Render(image)).absolute().top_0().left_0().w(px(size.width)).h(px(size.height))))
            .child(div().relative().w_full().max_w(px(1120.)).mx_auto().child(header).child(grid));

        div()
            .size_full()
            .relative()
            .bg(bg_0())
            .font_family(FONT)
            .text_size(px(14.))
            .line_height(px(20.3))
            .text_color(text())
            .child(div().id("page").size_full().child(content).overflow_y_scrollbar())
            .children(self.update_layer(cx))
    }
}
