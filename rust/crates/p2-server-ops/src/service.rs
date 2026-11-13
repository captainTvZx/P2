//! Windows hizmeti (WinSW `P2ServerService.exe`, hizmet adı `P2Server`).
//!
//! Gerçek uygulama yönetici izni (UAC) isteyen PowerShell `Start-Process -Verb RunAs` çağrıları yapar.
//! Testler ve otomatik pilot [`NoService`] kullanır (`P2_SERVER_NO_SERVICE=1`): hiçbir zaman UAC
//! penceresi açılmaz, `sc` sorgusu yapılmaz.

use std::path::{Path, PathBuf};
use std::process::Command;

pub trait ServiceControl: Send + Sync {
    /// `serviceRegistered()`: `sc.exe query P2Server` başarılı mı.
    fn registered(&self) -> bool;
    /// `elevatedService('start'|'stop')`: WinSW sarmalayıcısını yönetici olarak çalıştırır.
    fn elevated_service(&self, command: &str) -> bool;
    /// `elevatedScript('p2-server-install.bat'|'p2-server-remove.bat')`.
    fn elevated_script(&self, name: &str) -> bool;
    /// `ensureServiceDescriptor()`: kurulum klasöründe `P2ServerService.xml` var mı.
    fn descriptor_ok(&self) -> bool;
}

/// Gerçek Windows hizmeti. `install_dir`: sarmalayıcı ve betiklerin bulunduğu klasör (exe'nin klasörü).
pub struct WindowsService {
    pub install_dir: PathBuf,
    pub name: String,
}

impl WindowsService {
    pub fn beside_current_exe() -> Self {
        let install_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        Self { install_dir, name: "P2Server".into() }
    }
}

fn ps_quote(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

impl ServiceControl for WindowsService {
    fn registered(&self) -> bool {
        if !cfg!(windows) {
            return false;
        }
        crate::hide(Command::new("sc.exe").args(["query", &self.name])).output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn elevated_service(&self, command: &str) -> bool {
        if !self.descriptor_ok() {
            return false;
        }
        let wrapper = self.install_dir.join("P2ServerService.exe");
        if !wrapper.exists() {
            return false;
        }
        let line = format!(
            "$process = Start-Process -FilePath '{}' -ArgumentList '{command}' -Verb RunAs -Wait -PassThru; exit $process.ExitCode",
            ps_quote(&wrapper)
        );
        crate::hide(Command::new("powershell").args(["-NoProfile", "-Command", &line])).status().map(|s| s.success()).unwrap_or(false)
    }

    fn elevated_script(&self, name: &str) -> bool {
        if !cfg!(windows) {
            return false;
        }
        let script = self.install_dir.join(name);
        if !script.exists() {
            return false;
        }
        let line = format!("Start-Process -FilePath '{}' -Verb RunAs -Wait", ps_quote(&script));
        crate::hide(Command::new("powershell").args(["-NoProfile", "-Command", &line])).status().map(|s| s.success()).unwrap_or(false)
    }

    fn descriptor_ok(&self) -> bool {
        // Electron yalnız paketli Windows kurulumunda denetliyordu; Rust'ta kurulum klasörü exe'nin klasörü.
        if !cfg!(windows) {
            return true;
        }
        self.install_dir.join("P2ServerService.xml").exists()
    }
}

/// Hizmet yok: kayıt sorgusu her zaman `false`, yükseltilmiş çağrılar hiç yapılmaz.
pub struct NoService;

impl ServiceControl for NoService {
    fn registered(&self) -> bool {
        false
    }
    fn elevated_service(&self, _: &str) -> bool {
        false
    }
    fn elevated_script(&self, _: &str) -> bool {
        false
    }
    fn descriptor_ok(&self) -> bool {
        true
    }
}

/// Ortama göre seçim: `P2_SERVER_NO_SERVICE=1` → [`NoService`].
pub fn from_env() -> Box<dyn ServiceControl> {
    if std::env::var("P2_SERVER_NO_SERVICE").is_ok_and(|v| !v.is_empty() && v != "0") {
        Box::new(NoService)
    } else {
        Box::new(WindowsService::beside_current_exe())
    }
}
