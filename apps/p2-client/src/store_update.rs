//! Microsoft Store (MSIX) sürümü: paket kimliği ve Store üzerinden uygulama içi güncelleme.
//!
//! MSIX paketi kendi dosyalarının üzerine yazamaz; NSIS güncelleyicisi (`p2_updater::ClientUpdater`)
//! paketli çalışmada kapalıdır. Onun yerine `Windows.Services.Store.StoreContext`:
//! - `check()`: Store'da yeni paket var mı → varsa sessizce indir (ilerleme %), sonra `Ready`.
//!   Sessiz indirme reddedilirse de `Ready` olur; kurulum düğmesi Store penceresiyle indirir.
//! - `install()`: `RegisterApplicationRestart` + sessiz indir-kur (Store uygulamayı kapatır, kurar,
//!   yeniden açar). Sessiz kurulum olmazsa Store'un onay penceresi (ana pencereye bağlı) açılır.
//! - Açılışta ve 4 saatte bir, ayrıca sunucunun `update:published` duyurusunda denetlenir.
//!
//! Durum arayüze `p2_updater::UpdateState` olarak verilir; banner ve ayarlar ekranı değişmez.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use p2_updater::{Status, UpdateState};

/// Store denetim aralığı (açılıştaki ilk denetimden sonra).
const INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);
/// Ayarlar ekranı "kaynak" alanı.
pub const FEED: &str = "Microsoft Store";
/// AppxManifest `uap3:AppExecutionAlias` (packaging/msix/AppxManifest.xml ile aynı olmalı):
/// `%LOCALAPPDATA%\Microsoft\WindowsApps\p2-client.exe` sürümler arasında sabit kalır.
pub const EXECUTION_ALIAS: &str = "p2-client.exe";

/// Uygulama MSIX paketi içinde mi çalışıyor (Store ya da yerel test kurulumu).
pub fn is_packaged() -> bool {
    static P: OnceLock<bool> = OnceLock::new();
    *P.get_or_init(|| {
        use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_SUCCESS};
        use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
        let mut len = 0u32;
        // Paketliyse ERROR_INSUFFICIENT_BUFFER döner (uzunluk 0 verdik); paketsizse NO_PACKAGE.
        let r = unsafe { GetCurrentPackageFullName(&mut len, None) };
        r != APPMODEL_ERROR_NO_PACKAGE && (r != ERROR_SUCCESS || len > 0)
    })
}

/// Paketin yerel veri klasörü (`%LOCALAPPDATA%\Packages\<aile>\LocalState`). Sanallaştırılmaz:
/// paket dışındaki süreçler (Explorer, oyunlar) bu yolu olduğu gibi görür.
pub fn local_state_dir() -> Option<std::path::PathBuf> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
    let mut len = 0u32;
    let _ = unsafe { GetCurrentPackageFamilyName(&mut len, None) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    if unsafe { GetCurrentPackageFamilyName(&mut len, Some(windows::core::PWSTR(buf.as_mut_ptr()))) } != ERROR_SUCCESS {
        return None;
    }
    let family = String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]);
    let dir = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Packages").join(family).join("LocalState");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Paketli kurulumun ilk açılışında masaüstüne "Play Through" kısayolu koyar (NSIS kurulumu gibi).
/// MSIX kurulumu kısayol oluşturamaz (`desktop7:Shortcut` Windows 10'da yok sayılıyor), bu yüzden
/// uygulama yapar. LocalState'teki işaret dosyası yalnız bir kez yapılmasını sağlar: kullanıcı
/// kısayolu silerse geri gelmez; uygulama kaldırılınca LocalState de silinir, yeni kurulumda yine oluşur.
pub fn ensure_desktop_shortcut() -> Option<std::thread::JoinHandle<()>> {
    if !is_packaged() || std::env::var_os("P2_TEST_WINDOW_POS").is_some() {
        return None;
    }
    let dir = local_state_dir()?;
    let mark = dir.join("desktop-shortcut.done");
    if mark.exists() {
        return None;
    }
    Some(std::thread::spawn(move || match desktop_shortcut(&dir) {
        // İşaret yalnız başarıdan sonra: başarısız deneme sonraki açılışta yinelenir.
        Ok(()) => {
            let _ = std::fs::write(&mark, b"1");
            crate::trace("kabuk: masaüstü kısayolu oluşturuldu (ilk açılış)");
        }
        Err(e) => crate::trace(&format!("kabuk: masaüstü kısayolu oluşturulamadı: {e}")),
    }))
}

/// Paket ailesi adı (`THROUGHDUSTADASTRA.PlayThrough_xxxx`).
fn family_name() -> Option<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
    let mut len = 0u32;
    let _ = unsafe { GetCurrentPackageFamilyName(&mut len, None) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    if unsafe { GetCurrentPackageFamilyName(&mut len, Some(windows::core::PWSTR(buf.as_mut_ptr()))) } != ERROR_SUCCESS {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]))
}

/// Kısayol Store uygulamalarının standart yolunu hedefler: `explorer.exe shell:AppsFolder\<aile>!P2`
/// (sürümler arasında sabit; Başlat kutucuğuyla aynı uygulamayı açar). Simge LocalState'e kopyalanır.
fn desktop_shortcut(dir: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    let family = family_name().ok_or("paket ailesi adı alınamadı")?;
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let target = std::path::PathBuf::from(windir).join("explorer.exe");
    let args = format!("shell:AppsFolder\\{family}!P2");
    let src = std::env::current_exe().map_err(|e| e.to_string())?.parent().map(|d| d.join("p2.ico")).ok_or("exe klasörü yok")?;
    let icon = dir.join("p2.ico");
    std::fs::copy(&src, &icon).map_err(|e| e.to_string())?;
    let q = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
    // Masaüstü yolu PowerShell'den: OneDrive yedeklemesi açıksa %USERPROFILE%\Desktop değildir.
    let script = format!(
        "$l=Join-Path ([Environment]::GetFolderPath('Desktop')) 'Play Through.lnk';if(-not(Test-Path $l)){{$s=(New-Object -ComObject WScript.Shell).CreateShortcut($l);$s.TargetPath='{}';$s.Arguments='{}';$s.IconLocation='{},0';$s.Description='Play Through';$s.Save()}};if(-not(Test-Path $l)){{exit 3}}",
        q(&target),
        args.replace('\'', "''"),
        q(&icon)
    );
    let status = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(0x0800_0000).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("powershell çıkış kodu {:?}", status.code())) }
}

fn short_version(v: windows::ApplicationModel::PackageVersion) -> String {
    // Store sürümü dört hanelidir, son hane hep 0 (1.0.8.0 → 1.0.8).
    if v.Revision == 0 { format!("{}.{}.{}", v.Major, v.Minor, v.Build) } else { format!("{}.{}.{}.{}", v.Major, v.Minor, v.Build, v.Revision) }
}

fn cell() -> &'static Mutex<UpdateState> {
    static S: OnceLock<Mutex<UpdateState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(UpdateState { current: crate::updates::product_version().to_string(), feed: FEED.into(), ..Default::default() }))
}

fn set(patch: impl FnOnce(&mut UpdateState)) {
    {
        let mut s = cell().lock().unwrap();
        patch(&mut s);
    }
    crate::updates::bump();
}

pub fn state() -> UpdateState {
    cell().lock().unwrap().clone()
}

static BUSY: AtomicBool = AtomicBool::new(false);

/// İlk bağlanışta çağrılır: hemen ve sonra her `INTERVAL`'da bir denetler (bir kez başlar).
pub fn start() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        loop {
            check_blocking();
            std::thread::sleep(INTERVAL);
        }
    });
}

/// Arka planda denetle (ayarlar düğmesi, `update:published`).
pub fn check() {
    std::thread::spawn(check_blocking);
}

fn check_blocking() {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let before = state().status;
    // İndirilmiş/indiriliyorsa tekrar denetlemeye gerek yok.
    if matches!(before, Status::Idle) {
        set(|s| {
            s.status = Status::Checking;
            s.error.clear();
        });
        match find_and_download() {
            Ok(Some(version)) => {
                crate::trace(&format!("store: güncelleme hazır {version}"));
                set(|s| {
                    s.status = Status::Ready;
                    s.version = version;
                    s.percent = 100;
                });
            }
            Ok(None) => set(|s| s.status = Status::Idle),
            Err(e) => {
                crate::trace(&format!("store: denetim başarısız: {e}"));
                set(|s| {
                    s.status = Status::Idle;
                    s.error = e.to_string();
                });
            }
        }
    }
    BUSY.store(false, Ordering::SeqCst);
}

/// Bekleyen güncellemenin sürümü; yoksa `None`. Sessiz indirmeyi dener (ilerleme yayınlanır).
fn find_and_download() -> windows::core::Result<Option<String>> {
    use windows_future::AsyncOperationProgressHandler;
    use windows::Services::Store::{StoreContext, StorePackageUpdateResult, StorePackageUpdateState, StorePackageUpdateStatus};
    let ctx = StoreContext::GetDefault()?;
    let updates = ctx.GetAppAndOptionalStorePackageUpdatesAsync()?.join()?;
    if updates.Size()? == 0 {
        crate::trace("store: güncel");
        return Ok(None);
    }
    let first = updates.GetAt(0)?;
    let version = short_version(first.Package()?.Id()?.Version()?);
    crate::trace(&format!("store: yeni sürüm {version} (zorunlu={}), indiriliyor", first.Mandatory().unwrap_or(false)));
    set(|s| {
        s.status = Status::Downloading;
        s.version = version.clone();
        s.percent = 0;
    });
    let op = ctx.TrySilentDownloadStorePackageUpdatesAsync(&updates)?;
    op.SetProgress(&AsyncOperationProgressHandler::<StorePackageUpdateResult, StorePackageUpdateStatus>::new(|_, progress| {
        let p = &*progress;
        let percent = (p.TotalDownloadProgress.clamp(0.0, 1.0) * 100.0).round() as u32;
        set(|s| s.percent = percent);
        Ok(())
    }))?;
    let result = op.join()?;
    let overall = result.OverallState()?;
    if overall != StorePackageUpdateState::Completed {
        // Sessiz indirmeye izin yok (ör. kullanıcı Store'da otomatik güncellemeyi kapatmış):
        // kurulum düğmesi Store penceresiyle indirir.
        crate::trace(&format!("store: sessiz indirme olmadı (durum {}), kurulumda Store penceresi açılacak", overall.0));
    }
    Ok(Some(version))
}

/// "Yeniden başlat ve kur": Store uygulamayı kapatıp kurar ve yeniden açar.
pub fn install() {
    std::thread::spawn(|| {
        if let Err(e) = install_blocking() {
            crate::trace(&format!("store: kurulum başlatılamadı: {e}"));
            set(|s| {
                s.status = Status::Ready;
                s.error = e.to_string();
            });
        }
    });
}

fn install_blocking() -> windows::core::Result<()> {
    use windows::Services::Store::{StoreContext, StorePackageUpdateState};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Recovery::{REGISTER_APPLICATION_RESTART_FLAGS, RegisterApplicationRestart};
    use windows::Win32::UI::Shell::IInitializeWithWindow;
    use windows::core::Interface;
    let ctx = StoreContext::GetDefault()?;
    let updates = ctx.GetAppAndOptionalStorePackageUpdatesAsync()?.join()?;
    if updates.Size()? == 0 {
        set(|s| s.status = Status::Idle);
        return Ok(());
    }
    set(|s| s.status = Status::Installing);
    // Güncellemeden sonra Windows uygulamayı yeniden açsın (komut satırı boş: normal açılış).
    unsafe {
        let _ = RegisterApplicationRestart(windows::core::PCWSTR::null(), REGISTER_APPLICATION_RESTART_FLAGS(0));
    }
    crate::trace("store: sessiz kurulum başlıyor (uygulama kapanacak)");
    let silent = ctx.TrySilentDownloadAndInstallStorePackageUpdatesAsync(&updates)?.join();
    match silent {
        Ok(r) if r.OverallState()? == StorePackageUpdateState::Completed => return Ok(()),
        Ok(r) => crate::trace(&format!("store: sessiz kurulum olmadı (durum {}), Store penceresi açılıyor", r.OverallState()?.0)),
        Err(e) => crate::trace(&format!("store: sessiz kurulum hatası ({e}), Store penceresi açılıyor")),
    }
    // Masaüstü uygulamasında Store penceresinin sahibi verilmeli.
    let owner = crate::MAIN_HWND.load(Ordering::Relaxed);
    if owner != 0 {
        let init: IInitializeWithWindow = ctx.cast()?;
        unsafe { init.Initialize(HWND(owner as *mut _))? };
    }
    let result = ctx.RequestDownloadAndInstallStorePackageUpdatesAsync(&updates)?.join()?;
    let overall = result.OverallState()?;
    crate::trace(&format!("store: Store penceresiyle kurulum sonucu {}", overall.0));
    if overall != StorePackageUpdateState::Completed {
        set(|s| s.status = Status::Ready);
    }
    Ok(())
}
