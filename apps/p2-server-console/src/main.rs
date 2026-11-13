//! P2 Server Console (Rust + GPUI) — `electron/server-main.cjs` penceresi ve
//! `electron/server-console.html` arayüzünün karşılığı.
//!
//! Pencere 1000×720 (en az 780×560), başlık "P2 Server Console", tek örnek. Kapatınca konsol kapanır,
//! API (konsolun başlattığı süreç ya da Windows hizmeti) arka planda çalışmaya devam eder.
//! İşletim mantığı `p2-server-ops`'ta; API süreci yanındaki `p2-server.exe`.
//!
//! Ortam (testler): `P2_SERVER_DATA_DIR` (veri klasörü), `P2_SERVER_NO_SERVICE=1` (hizmet/UAC yok),
//! `P2_SERVER_EXE`, `P2_PUBLIC_IP`, `P2_CONSOLE_AUTOPILOT=1` (+ `P2_CONSOLE_AUTOPILOT_DB`, `P2_CONSOLE_SHOTS`),
//! `P2_CONSOLE_FAKE_UPDATE`, `P2_CONSOLE_BACKUP_PATH`/`P2_CONSOLE_RESTORE_PATH` (dosya pencereleri yerine).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autopilot;
mod background;
mod console;
mod instance;
mod style;
mod updates;

use gpui_kit::{AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

/// Tanılama izi (otomatik pilot testleri okur).
pub fn trace(line: &str) {
    use std::io::Write as _;
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(std::time::Instant::now).elapsed().as_millis();
    let mut out = std::io::stdout();
    let _ = writeln!(out, "[+{t} ms] {line}");
    let _ = out.flush();
}

/// Electron döneminde kaydedilmiş Windows hizmeti `"<kurulum>\P2 Server.exe" --p2-server`
/// çalıştırır (prepare-service-config.ps1'in eski hâli). Rust kurulumu o adı konsola verdiği için
/// konsol bu argümanla açılırsa işi yanındaki `p2-server.exe`'ye devreder ve onun çıkış koduyla
/// çıkar; hizmet yeniden kaydedilene (ya da kurulumun yazdığı yeni XML okunana) kadar API çalışır.
fn forward_to_server() -> ! {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("p2-server.exe")));
    let code = match exe.filter(|p| p.exists()) {
        Some(server) => {
            use std::os::windows::process::CommandExt as _;
            // Konsol Windows alt sisteminde: alt süreç kendine konsol penceresi açmasın; stdout/stderr
            // (WinSW'nin günlük boruları) yine devralınır.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new(server).args(std::env::args().skip(1)).creation_flags(CREATE_NO_WINDOW).status().ok().and_then(|s| s.code()).unwrap_or(1)
        }
        None => 1,
    };
    std::process::exit(code)
}

fn main() {
    if std::env::args().skip(1).any(|a| a == "--p2-server" || a == "--headless") {
        forward_to_server();
    }
    // Electron `app.setAppUserModelId('com.p2.server')` ve kısayolların kimliği (görev çubuğu sabitlemesi).
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(windows::core::w!("com.p2.server"));
    }
    let autopilot = std::env::var_os("P2_CONSOLE_AUTOPILOT").is_some();
    let multiple = std::env::args().any(|a| a == "--p2-test-multiple");
    if !autopilot && !multiple && !instance::acquire() {
        return;
    }
    trace("konsol açılıyor");
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        p2_ui::theme::apply(p2_ui::Palette::p2_dark(), cx);
        {
            // Kaydırma çubuğu: `::-webkit-scrollbar-thumb` rgba(120,170,255,.18) / üzerinde .32.
            let theme = gpui_kit::component::Theme::global_mut(cx);
            theme.colors.scrollbar_thumb = style::ca(120, 170, 255, 0.18);
            theme.colors.scrollbar_thumb_hover = style::ca(120, 170, 255, 0.32);
            // Chromium'un kaydırma çubuğu her zaman görünür.
            theme.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Always;
            theme.font_family = style::FONT.into();
            theme.font_size = px(14.);
        }
        // `P2_CONSOLE_WINDOW_HEIGHT`: yalnız testler (tam sayfa görüntüsü).
        let height = std::env::var("P2_CONSOLE_WINDOW_HEIGHT").ok().and_then(|h| h.parse::<f32>().ok()).unwrap_or(720.);
        let bounds = Bounds::centered(None, size(px(1000.), px(height)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(780.), px(560.))),
            titlebar: Some(TitlebarOptions { title: Some("P2 Server Console".into()), appears_transparent: false, traffic_light_position: None }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| console::Console::new(window, cx))).expect("pencere açılamadı");
        cx.activate(true);
    });
}
