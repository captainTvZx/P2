//! İstemci güncelleyicisi — `electron/updater.cjs` + arayüz kullanımı (`UpdateBanner.tsx`,
//! SettingsModal `UpdateSetting`, `main.cjs` açılış turu) karşılığı, `p2_updater::ClientUpdater`
//! üzerine.
//!
//! - Tek güncelleyici; veri klasörü istemci profili (`P2_PROFILE_DIR`'e uyar): `update-source.json`
//!   (son bağlanılan API) ve `.updaterId` orada.
//! - `configure(api)` her bağlanışta; sunucunun `update:published` duyurusunda `check()`.
//! - Açılışta (ana pencereden önce) `startup_check(saved_base())`, açılış penceresi `state.text`'i
//!   gösterir (splash.rs).
//! - Ayarlar > Uygulama > "Sürüm ve güncelleme": canlı durum, denetle, yeniden başlat ve kur.
//! - Aramadayken `set_busy(true)`; overlay'li oyun açıkken boşta sayılmaz (`set_idle_check`).
//! - Kapanışta `install_if_ready()`.
//!
//! Geliştirme derlemesinde (`p2-rust-target` altındaki exe) `packaged = false`: hiçbir şey
//! yapılmaz, kurulum asla başlamaz. Testler `P2_UPDATER_TEST=1` ile güncelleyiciyi açar; o
//! kipte kurucu çalıştırılmaz (sahte başlatıcı yalnız iz düşer ve işaret dosyası yazar), önbellek
//! profil klasöründe tutulur.
//!
//! TODO(Faz 8): eski birleşik kurulumun `before_install` adımları (schtasks "P2 Server" durdurma,
//! `--p2-server` süreçlerini kapatma, HMAC'li `/updates/stop` isteği, port boşalmasını bekleme).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use p2_updater::{ClientUpdater, ClientUpdaterOptions, InstallerLauncher, NsisLauncher, Status, UpdateState};

/// Ürün sürümü (kök Cargo.toml `[workspace.package] version` ya da P2_PRODUCT_VERSION). build.rs gömer;
/// paketleme `P2_PRODUCT_VERSION` ile değiştirebilir (kurulum paketinin sürümüyle aynı olmalı).
pub fn product_version() -> &'static str {
    env!("P2_PRODUCT_VERSION")
}

/// Durum her değiştiğinde artar; arayüz görünümleri bununla yeniden çizilir.
static REV: AtomicU64 = AtomicU64::new(0);

/// Test kipi: kurucu çalıştırılmaz.
struct TestLauncher;

impl InstallerLauncher for TestLauncher {
    fn launch(&self, installer: &Path, args: &[String], elevate: bool) -> std::io::Result<()> {
        crate::trace(&format!("güncelleyici: [test] kurucu çalıştırılmadı: {} {} (yönetici={elevate})", installer.display(), args.join(" ")));
        let marker = p2_client_core::store::profile_dir().join("update-install.txt");
        std::fs::write(marker, format!("{}\n{}", installer.display(), args.join(" ")))
    }
}

fn test_mode() -> bool {
    std::env::var_os("P2_UPDATER_TEST").is_some()
}

/// Microsoft Store (MSIX) sürümü mü: güncellemeyi Store yapar (`store_update`), NSIS güncelleyicisi kapalı.
pub fn store() -> bool {
    crate::store_update::is_packaged()
}

/// Arayüzü yeniden çizdir (Store güncelleyicisi durum değiştirince).
pub fn bump() {
    REV.fetch_add(1, Ordering::Relaxed);
}

/// NSIS kurulumu (kurulu, paketlenmemiş) sürüm mü: geliştirme derlemesi `p2-rust-target` altında
/// çalışır; Store sürümünde NSIS güncelleyicisi ve açılış turu kapalıdır.
pub fn packaged() -> bool {
    if test_mode() {
        return true;
    }
    if store() {
        return false;
    }
    // macOS: NSIS kurulumu/latest.yml akışı Windows'a özel; Mac güncelleme kanalı ayrı iş.
    if !cfg!(windows) {
        return false;
    }
    std::env::current_exe().map(|e| !e.to_string_lossy().to_lowercase().contains("p2-rust-target")).unwrap_or(false)
}

pub fn updater() -> &'static ClientUpdater {
    static U: OnceLock<ClientUpdater> = OnceLock::new();
    U.get_or_init(|| {
        let profile = p2_client_core::store::profile_dir();
        let mut o = ClientUpdaterOptions::new(product_version(), profile.clone());
        o.packaged = packaged();
        if std::env::var_os("P2_PROFILE_DIR").is_some() {
            // Testler/ayrı profiller kullanıcının gerçek önbelleğine (LOCALAPPDATA) yazmaz.
            o.cache_dir = profile.join("update-cache");
        }
        o.launcher = if test_mode() { Arc::new(TestLauncher) } else { Arc::new(NsisLauncher) };
        let last: Mutex<(Status, String, u32)> = Mutex::new((Status::Idle, String::new(), u32::MAX));
        o.broadcast = Arc::new(move |s: &UpdateState| {
            REV.fetch_add(1, Ordering::Relaxed);
            let mut l = last.lock().unwrap();
            // İz: durum/metin değişince ve indirmede her %10'da bir.
            let step = s.percent / 10;
            if l.0 != s.status || l.1 != s.text || (s.status == Status::Downloading && l.2 != step) {
                crate::trace(&format!("güncelleyici: durum {:?} sürüm={} %{} metin={:?} hata={:?}", s.status, s.version, s.percent, s.text, s.error));
                *l = (s.status, s.text.clone(), step);
            }
        });
        o.log = Arc::new(|line: &str| crate::trace(&format!("güncelleyici: {line}")));
        o.quit = Arc::new(|| {
            crate::trace("güncelleyici: kurulum başladı, uygulama kapanıyor");
            QUITTING.store(true, Ordering::SeqCst);
            std::process::exit(0);
        });
        crate::trace(&format!("güncelleyici: sürüm {} ({})", product_version(), if o.packaged { if test_mode() { "test kipi" } else { "kurulu" } } else if store() { "Microsoft Store — Store güncelleyicisi" } else { "geliştirme — kapalı" }));
        ClientUpdater::new(o)
    })
}

static QUITTING: AtomicBool = AtomicBool::new(false);

pub fn rev() -> u64 {
    REV.load(Ordering::Relaxed)
}

pub fn state() -> UpdateState {
    if store() {
        return crate::store_update::state();
    }
    updater().state()
}

/// Her bağlanışta (UpdateBanner `bridge.configure(apiBase)`).
pub fn configure(api_base: &str) {
    if store() {
        crate::store_update::start();
        return;
    }
    let u = updater();
    let base = api_base.to_string();
    let u = u.clone();
    std::thread::spawn(move || match u.configure(&base) {
        Ok(feed) => crate::trace(&format!("güncelleyici: kaynak {feed}")),
        Err(e) => crate::trace(&format!("güncelleyici: yapılandırılmadı ({})", e.reason())),
    });
}

/// Sunucunun `update:published` duyurusu ya da ayarlardaki "Güncellemeleri denetle".
pub fn check() {
    crate::trace("güncelleyici: denetleniyor");
    if store() {
        crate::store_update::check();
        return;
    }
    updater().check();
}

/// "Şimdi yeniden başlat" / "Yeniden başlat ve kur": arka planda (bloklayıcı) kurulum.
pub fn install() {
    if store() {
        crate::store_update::install();
        return;
    }
    let u = updater().clone();
    std::thread::spawn(move || {
        if !u.install() {
            crate::trace("güncelleyici: kurulum başlatılamadı");
        }
    });
}

/// Arama sürüyor mu (CallOverlay.tsx `updates.busy(Boolean(call.call))`).
pub fn set_busy(busy: bool) {
    static LAST: AtomicBool = AtomicBool::new(false);
    if LAST.swap(busy, Ordering::Relaxed) != busy {
        crate::trace(&format!("güncelleyici: meşgul={busy}"));
    }
    updater().set_busy(busy);
}

/// Kapanış: hazır güncelleme sessizce kurulur (main.cjs `before-quit`).
pub fn install_if_ready() {
    // Store sürümünde kapanışta kurulacak bir şey yok: Store kendi zamanında kurar.
    if QUITTING.load(Ordering::SeqCst) || store() {
        return;
    }
    if updater().install_if_ready() {
        crate::trace("güncelleyici: kapanışta kurulum başlatıldı");
    }
}

// ------------------------------------------------------------ ayarlar ekranı durumu

/// Ayarlardaki "Güncellemeleri denetle" düğmesinin durumu (SettingsModal `checking`/`note`).
pub struct CheckUi {
    pub checking: bool,
    pub note: String,
}

pub fn check_ui() -> &'static Mutex<CheckUi> {
    static UI: OnceLock<Mutex<CheckUi>> = OnceLock::new();
    UI.get_or_init(|| Mutex::new(CheckUi { checking: false, note: String::new() }))
}

/// SettingsModal: denetle → 2,5 sn sonra durumu oku, not yaz.
pub fn check_from_settings() {
    {
        let mut ui = check_ui().lock().unwrap();
        if ui.checking {
            return;
        }
        ui.checking = true;
        ui.note.clear();
    }
    REV.fetch_add(1, Ordering::Relaxed);
    let ok = !state().feed.is_empty();
    check();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let fresh = state();
        let mut ui = check_ui().lock().unwrap();
        ui.note = if ok { if fresh.status == Status::Idle { p2_i18n::t("upd.upToDate.note") } else { String::new() } } else { p2_i18n::t("upd.unreachable") };
        ui.checking = false;
        crate::trace(&format!("ayarlar: güncelleme denetimi bitti ({:?}) {}", fresh.status, ui.note));
        REV.fetch_add(1, Ordering::Relaxed);
    });
}

/// SettingsModal `label`.
pub fn status_label(s: &UpdateState) -> String {
    match s.status {
        Status::Downloading => p2_i18n::tv("upd.status.downloading", &[("n", &s.percent.to_string())]),
        Status::Ready => p2_i18n::tv("upd.ready.version", &[("version", s.version.as_str())]),
        Status::Checking => p2_i18n::t("set2.update.checking"),
        _ => p2_i18n::t("upd.status.upToDate"),
    }
}

/// Görünümü güncelleyici durumu değiştikçe yeniden çizer (görünüm yaşadıkça, 250 ms'de bir bakar).
pub fn watch<V: 'static>(cx: &mut gpui_kit::Context<V>) {
    use std::collections::HashSet;
    static WATCHED: OnceLock<Mutex<HashSet<gpui_kit::EntityId>>> = OnceLock::new();
    let id = cx.entity_id();
    if !WATCHED.get_or_init(Default::default).lock().unwrap().insert(id) {
        return;
    }
    cx.spawn(async move |this, cx| {
        let mut seen = rev();
        loop {
            cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            let now = rev();
            if now != seen {
                seen = now;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            } else if this.update(cx, |_, _| ()).is_err() {
                break;
            }
        }
        WATCHED.get().unwrap().lock().unwrap().remove(&id);
    })
    .detach();
}
