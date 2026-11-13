//! NSIS kurulumunu başlatma — electron-updater `NsisUpdater.doInstall` + `BaseUpdater.spawnLog`.

use std::io;
use std::path::Path;

/// `quitAndInstall(isSilent, isForceRunAfter)` için NSIS argümanları, electron-updater ile aynı
/// sırada: `--updated`, sessizse `/S`, kurulumdan sonra açılacaksa `--force-run`, kurulum
/// dizini verilmişse `/D=…` (NSIS kuralı gereği tırnaksız ve sonda), paket dosyası varsa
/// `--package-file=…`. P2 `quitAndInstall(true, true)` kullanır → `--updated /S --force-run`.
pub fn nsis_install_args(silent: bool, force_run_after: bool, install_directory: Option<&str>, package_file: Option<&Path>) -> Vec<String> {
    let mut args = vec!["--updated".to_string()];
    if silent {
        args.push("/S".into());
    }
    if force_run_after {
        args.push("--force-run".into());
    }
    if let Some(dir) = install_directory {
        args.push(format!("/D={dir}"));
    }
    if let Some(package) = package_file {
        args.push(format!("--package-file={}", package.display()));
    }
    args
}

/// Kurulum programını başlatan katman. Testler sahte bir uygulama verir; gerçek uygulama
/// [`NsisLauncher`] kullanır.
pub trait InstallerLauncher: Send + Sync {
    /// `installer`'ı `args` ile ayrık (detached) başlatır ve beklemeden döner. `elevate`:
    /// latest.yml'de `isAdminRightsRequired: true` (perMachine kurulum) — yönetici hakkıyla aç.
    fn launch(&self, installer: &Path, args: &[String], elevate: bool) -> io::Result<()>;
}

/// Varsayılan başlatıcı. electron-updater davranışı:
/// - `elevate` ise doğrudan yükseltilmiş başlatılır (Electron'da `elevate.exe`; burada
///   `ShellExecuteW("runas")`).
/// - Normal başlatma erişim/yükseltme hatası verirse yükseltilmiş yeniden denenir
///   (Electron: EACCES/UNKNOWN → elevate).
/// - Dosya bulunamazsa kabukla açılır (Electron: ENOENT → `shell.openPath`, argümansız).
pub struct NsisLauncher;

impl InstallerLauncher for NsisLauncher {
    fn launch(&self, installer: &Path, args: &[String], elevate: bool) -> io::Result<()> {
        if elevate {
            return shell_execute(installer, args, true);
        }
        match spawn_detached(installer, args) {
            Ok(()) => Ok(()),
            Err(error) if needs_elevation(&error) => shell_execute(installer, args, true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => shell_execute(installer, &[], false),
            Err(error) => Err(error),
        }
    }
}

fn needs_elevation(error: &io::Error) -> bool {
    // 740 = ERROR_ELEVATION_REQUIRED, 5 = ERROR_ACCESS_DENIED
    matches!(error.raw_os_error(), Some(740) | Some(5)) || error.kind() == io::ErrorKind::PermissionDenied
}

/// Node `spawn(cmd, args, { detached: true, stdio: 'ignore' })` + `unref()`.
fn spawn_detached(installer: &Path, args: &[String]) -> io::Result<()> {
    let mut command = std::process::Command::new(installer);
    command.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command.spawn().map(|_child| ())
}

/// Komut satırı için argüman birleştirme (boşluk/tırnak içerenler tırnaklanır; `/D=` NSIS
/// kuralı gereği olduğu gibi kalır).
pub(crate) fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.starts_with("/D=") || (!arg.is_empty() && !arg.contains([' ', '\t', '"'])) {
                arg.clone()
            } else {
                format!("\"{}\"", arg.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn shell_execute(installer: &Path, args: &[String], elevate: bool) -> io::Result<()> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR};

    let verb = HSTRING::from(if elevate { "runas" } else { "open" });
    let file = HSTRING::from(installer.as_os_str());
    let params = HSTRING::from(join_args(args));
    let params_ptr = if args.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) };
    // SAFETY: tüm dizgeler çağrı boyunca yaşıyor; pencere sahibi yok.
    let result = unsafe { ShellExecuteW(None, &verb, &file, params_ptr, PCWSTR::null(), SW_SHOWNORMAL) };
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        // 5: SE_ERR_ACCESSDENIED (kullanıcı UAC'yi reddetti dahil), 2: dosya yok.
        Err(io::Error::from_raw_os_error(if code == 2 { 2 } else { 5 }))
    }
}

#[cfg(not(windows))]
fn shell_execute(installer: &Path, args: &[String], _elevate: bool) -> io::Result<()> {
    spawn_detached(installer, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_updater_arguman_sirasi() {
        assert_eq!(nsis_install_args(true, true, None, None), vec!["--updated", "/S", "--force-run"]);
        assert_eq!(nsis_install_args(false, false, None, None), vec!["--updated"]);
        assert_eq!(
            nsis_install_args(true, false, Some("C:\\Program Files\\P2"), Some(Path::new("C:\\c\\package-1.7z"))),
            vec!["--updated", "/S", "/D=C:\\Program Files\\P2", "--package-file=C:\\c\\package-1.7z"]
        );
        assert_eq!(
            join_args(&["--updated".into(), "/S".into(), "/D=C:\\Program Files\\P2".into(), "a b".into()]),
            "--updated /S /D=C:\\Program Files\\P2 \"a b\""
        );
    }
}
