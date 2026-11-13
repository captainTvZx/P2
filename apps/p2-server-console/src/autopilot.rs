//! Otomatik pilot (`P2_CONSOLE_AUTOPILOT=1`): konsolu gerçek düğme yollarıyla sürer, her adımı
//! stdout'a yazar ve `P2_CONSOLE_SHOTS` klasörüne kendi penceresinin görüntülerini (p2-wincap) alır.
//! Masaüstüne girdi göndermez; yalnız kendi görünümünün yöntemlerini çağırır.
//!
//! `P2_CONSOLE_AUTOPILOT_DB="host:port:kullanıcı:veritabanı"`, `P2_CONSOLE_AUTOPILOT_PORT` (API, varsayılan 4140).
//! Yedek adımları için `P2_CONSOLE_BACKUP_PATH` ve `P2_CONSOLE_RESTORE_PATH` aynı dosyayı göstermeli.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::{Context, Window};
use p2_server_ops::update::{StubUpdater, UpdateState};

use crate::console::{Console, Field};

pub struct Autopilot {
    step: usize,
    since: Instant,
    shots: Option<PathBuf>,
    db: Vec<String>,
    api_port: String,
    shooting: Arc<AtomicBool>,
}

impl Autopilot {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("P2_CONSOLE_AUTOPILOT")?;
        let db: Vec<String> = std::env::var("P2_CONSOLE_AUTOPILOT_DB").unwrap_or_else(|_| "127.0.0.1:55432:p2:p2_console_e2e".into()).split(':').map(String::from).collect();
        Some(Self {
            step: 0,
            since: Instant::now(),
            shots: std::env::var_os("P2_CONSOLE_SHOTS").map(PathBuf::from),
            db,
            api_port: std::env::var("P2_CONSOLE_AUTOPILOT_PORT").unwrap_or_else(|_| "4140".into()),
            shooting: Arc::new(AtomicBool::new(false)),
        })
    }

    fn next(&mut self) {
        self.step += 1;
        self.since = Instant::now();
    }

    fn elapsed(&self) -> Duration {
        self.since.elapsed()
    }

    /// Kendi penceremizin görüntüsü (arka planda; bitene kadar sonraki adım bekler).
    fn shot(&self, name: &str) {
        let Some(dir) = &self.shots else { return };
        let _ = std::fs::create_dir_all(dir);
        let exe = std::env::var("P2_WINCAP").unwrap_or_else(|_| "C:/p2-rust-target/release/p2-wincap.exe".into());
        let out = dir.join(format!("{name}.png"));
        let flag = self.shooting.clone();
        flag.store(true, Ordering::SeqCst);
        let name = name.to_string();
        std::thread::spawn(move || {
            let ok = std::process::Command::new(exe).arg(std::process::id().to_string()).arg(&out).status().is_ok_and(|s| s.success());
            crate::trace(&format!("[autopilot] görüntü {name}: {}", if ok { out.display().to_string() } else { "ALINAMADI".into() }));
            flag.store(false, Ordering::SeqCst);
        });
    }
}

fn fail(message: &str) -> ! {
    crate::trace(&format!("[autopilot] HATA: {message}"));
    // Başlatılan API arkada kalmasın.
    if let Some(ops) = OPS.get() {
        ops.stop(false);
    }
    std::process::exit(1);
}

static OPS: std::sync::OnceLock<p2_server_ops::Ops> = std::sync::OnceLock::new();

pub fn step(view: &mut Console, window: &mut Window, cx: &mut Context<Console>) {
    let _ = OPS.set(view.ops.clone());
    let Some(mut ap) = view.autopilot.take() else { return };
    if ap.shooting.load(Ordering::SeqCst) {
        view.autopilot = Some(ap);
        return;
    }
    if ap.elapsed() > Duration::from_secs(120) {
        fail(&format!("adım {} zaman aşımı", ap.step));
    }
    let state = view.state.clone();
    let running = state.as_ref().is_some_and(|s| s.running);
    match ap.step {
        0 => {
            if state.is_some() && ap.elapsed() > Duration::from_millis(1800) {
                let s = state.unwrap();
                crate::trace(&format!("[autopilot] ilk durum: configured={} running={} address={}", s.configured, s.running, s.address));
                ap.shot("01-baslangic");
                ap.next();
            }
        }
        1 => {
            let db = ap.db.clone();
            view.set_value(Field::Host, &db[0], window, cx);
            view.set_value(Field::DbPort, &db[1], window, cx);
            view.set_value(Field::Username, &db[2], window, cx);
            view.set_value(Field::Database, &db[3], window, cx);
            view.set_value(Field::Password, "parola", window, cx);
            view.set_value(Field::ApiPort, &ap.api_port.clone(), window, cx);
            crate::trace("[autopilot] form dolduruldu; Kaydet");
            view.submit(window, cx);
            ap.next();
        }
        2 => {
            if running && state.as_ref().is_some_and(|s| s.configured) {
                let port: u16 = ap.api_port.parse().unwrap_or(4140);
                let health = p2_server_ops::net::local_request(port, "GET", "/health", None, Duration::from_secs(2));
                crate::trace(&format!("[autopilot] /health: {:?}", health.map(|h| h.1)));
                if !p2_server_ops::net::api_alive(port, Duration::from_secs(2)) {
                    fail("/health yanıt vermiyor");
                }
                ap.next();
            }
        }
        3 => {
            if ap.elapsed() > Duration::from_millis(1500) {
                ap.shot("02-calisiyor");
                ap.next();
            }
        }
        4 => {
            crate::trace("[autopilot] Durdur");
            view.click_stop(window, cx);
            ap.next();
        }
        5 => {
            if state.is_some() && !running && ap.elapsed() > Duration::from_millis(1500) {
                crate::trace(&format!("[autopilot] durdu; #error = {:?}", view.error_text));
                ap.shot("03-durduruldu");
                ap.next();
            }
        }
        6 => {
            crate::trace("[autopilot] Başlat");
            view.click_start(window, cx);
            ap.next();
        }
        7 => {
            if running {
                crate::trace("[autopilot] Yedek al");
                view.backup_action(true, window, cx);
                ap.next();
            }
        }
        8 => {
            if view.backup_text.starts_with("Yedek hazır: ") {
                crate::trace("[autopilot] Yedek yükle");
                view.backup_action(false, window, cx);
                ap.next();
            } else if !view.backup_busy && ap.elapsed() > Duration::from_secs(1) {
                fail(&format!("yedek alınamadı: {}", view.backup_text));
            }
        }
        9 => {
            if view.backup_text.starts_with("Yedek yüklendi") && running && ap.elapsed() > Duration::from_millis(1500) {
                ap.shot("04-yedek");
                ap.next();
            } else if !view.backup_busy && !view.backup_text.starts_with("Yedek yüklendi") && ap.elapsed() > Duration::from_secs(1) {
                fail(&format!("yedek yüklenemedi: {}", view.backup_text));
            }
        }
        10 => {
            view.set_value(Field::Database, "", window, cx);
            view.submit(window, cx);
            ap.next();
        }
        11 => {
            if ap.elapsed() > Duration::from_millis(600) {
                if view.bubble.is_none() {
                    fail("doğrulama balonu çıkmadı");
                }
                ap.shot("05-dogrulama");
                ap.next();
            }
        }
        12 => {
            let db = ap.db[3].clone();
            view.set_value(Field::Database, &db, window, cx);
            view.bubble = None;
            let stub = Arc::new(StubUpdater::new(view.ops.version()));
            stub.set(UpdateState { status: "downloading".into(), current: view.ops.version().into(), version: "0.9.10".into(), percent: 42, error: String::new() });
            view.ops.set_updater(stub);
            ap.next();
        }
        13 => {
            if ap.elapsed() > Duration::from_millis(1200) {
                ap.shot("06-guncelleme");
                ap.next();
            }
        }
        14 => {
            let stub = Arc::new(StubUpdater::new(view.ops.version()));
            stub.set(UpdateState { status: "ready".into(), current: view.ops.version().into(), version: "0.9.10".into(), percent: 100, error: "Sunucu durdurulamadı.".into() });
            view.ops.set_updater(stub);
            ap.next();
        }
        15 => {
            if ap.elapsed() > Duration::from_millis(1200) {
                ap.shot("07-guncelleme-hata");
                ap.next();
            }
        }
        16 => {
            view.ops.set_updater(Arc::new(StubUpdater::new(view.ops.version())));
            crate::trace("[autopilot] Durdur (son)");
            view.click_stop(window, cx);
            ap.next();
        }
        17 => {
            if state.is_some() && !running && ap.elapsed() > Duration::from_millis(1000) {
                crate::trace("[autopilot] bitti");
                cx.quit();
                ap.next();
            }
        }
        _ => {}
    }
    view.autopilot = Some(ap);
}
