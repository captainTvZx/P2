//! Komut satırı: hizmet kipi ve işletim alt komutları (`deploy/linux/migrate.cjs`, `backup-cli.cjs` ve
//! Electron `--p2-server` karşılıkları). Yönlendirici/durum koduna dokunmaz; yalnız ortamı hazırlar.
//!
//!   p2-server                      ortam değişkenleriyle sunar (geliştirme, konsolun başlattığı süreç)
//!   p2-server --p2-server          hizmet kipi: server.json (yoksa ortam) → göçler → sunum
//!   p2-server migrate              göçleri uygular (DATABASE_URL ya da server.json)
//!   p2-server backup [create] [DOSYA]
//!   p2-server restore DOSYA        (ya da `backup restore DOSYA`) [--no-service]

use std::io::IsTerminal as _;
use std::path::PathBuf;

use p2_server_ops::ops::{Ops, OpsOptions};
use p2_server_ops::service::NoService;

const USAGE: &str = "Kullanım: p2-server [--p2-server] | migrate | backup [create] [DOSYA] | restore DOSYA [--no-service]";

/// `Ok(None)`: sunmaya devam; `Ok(Some(kod))`: alt komut bitti, süreç bu kodla çıkar.
pub fn run() -> anyhow::Result<Option<i32>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let positional: Vec<&str> = args.iter().filter(|a| !a.starts_with("--")).map(String::as_str).collect();
    match positional.as_slice() {
        [] => {}
        ["migrate"] => return Ok(Some(migrate())),
        ["backup"] => return Ok(Some(backup_create(None))),
        ["backup", "create"] => return Ok(Some(backup_create(None))),
        ["backup", "create", file] | ["backup", file] if *file != "restore" => return Ok(Some(backup_create(Some(file)))),
        ["restore", file] | ["backup", "restore", file] => return Ok(Some(restore(file, flag("--no-service")))),
        ["help"] => {
            println!("{USAGE}");
            return Ok(Some(0));
        }
        _ => {
            eprintln!("Bilinmeyen komut.\n{USAGE}");
            return Ok(Some(2));
        }
    }
    if flag("--p2-server") || flag("--headless") {
        return headless();
    }
    // Konsolun başlattığı süreçte çıktı server.log'a gider: renk kodları yazılmaz.
    tracing_subscriber::fmt().with_ansi(std::io::stdout().is_terminal()).init();
    Ok(None)
}

fn ops(data_dir: PathBuf) -> Ops {
    Ops::new(OpsOptions { data_dir, headless: true, service: Box::new(NoService), ..Default::default() })
}

/// CLI veri klasörü: `P2_SERVER_DATA_DIR`; Linux'ta backup-cli.cjs'teki `/var/lib/p2-server`; Windows'ta `%ProgramData%\P2`.
fn cli_data_dir() -> PathBuf {
    if std::env::var_os("P2_SERVER_DATA_DIR").is_some_and(|v| !v.is_empty()) || cfg!(windows) {
        p2_server_ops::data_dir()
    } else {
        PathBuf::from("/var/lib/p2-server")
    }
}

/// `DATABASE_URL` ortamdan, yoksa veri klasöründeki server.json'dan.
fn database_url(data_dir: &std::path::Path) -> Option<String> {
    std::env::var("DATABASE_URL").ok().filter(|v| !v.is_empty()).or_else(|| p2_server_ops::config::ServerConfig::read(&data_dir.join("server.json")).and_then(|c| c.database_url).filter(|v| !v.is_empty()))
}

fn migrate() -> i32 {
    let Some(url) = database_url(&cli_data_dir()) else {
        eprintln!("DATABASE_URL ortam değişkeni gerekli");
        return 1;
    };
    match p2_server_ops::migrate::apply_url_blocking(&url) {
        Ok(report) => {
            for name in &report.adopted {
                println!("devralındı: {name}");
            }
            for name in &report.applied {
                println!("uygulandı: {name}");
            }
            0
        }
        Err(error) => {
            eprintln!("{}", p2_server_ops::error_stack(&error));
            1
        }
    }
}

fn backup_create(file: Option<&str>) -> i32 {
    let data_dir = cli_data_dir();
    let Some(url) = database_url(&data_dir) else {
        eprintln!("PostgreSQL yapılandırması bulunamadı.");
        return 1;
    };
    let destination = file.map(PathBuf::from).unwrap_or_else(|| data_dir.join("backups").join(p2_server_ops::backup::cli_backup_name()));
    let version = std::env::var("P2_SERVER_VERSION").unwrap_or_else(|_| p2_server_ops::VERSION.into());
    let options = p2_server_ops::backup::CreateOptions { database_url: &url, data_dir: &data_dir, destination: &destination, version: &version, include_releases: false };
    match p2_server_ops::backup::create_backup(options) {
        Ok(result) => {
            println!("{}", result.path.display());
            0
        }
        Err(error) => {
            eprintln!("{}", p2_server_ops::error_stack(&error));
            1
        }
    }
}

fn systemctl(action: &str) -> p2_server_ops::Outcome {
    match std::process::Command::new("systemctl").args([action, "p2-server.service"]).status() {
        Ok(s) if s.success() => p2_server_ops::Outcome::ok(),
        Ok(s) => p2_server_ops::Outcome::fail(format!("systemctl {action} başarısız ({s})")),
        Err(e) => p2_server_ops::Outcome::fail(e.to_string()),
    }
}

fn restore(file: &str, no_service: bool) -> i32 {
    let data_dir = cli_data_dir();
    let Some(url) = database_url(&data_dir) else {
        eprintln!("Önce hedef PostgreSQL bağlantısını yapılandır.");
        return 1;
    };
    // Linux: systemd birimi; Windows: konsolun durdur/başlat mantığı (hizmet kayıtlıysa WinSW).
    let windows = (!cfg!(unix) && !no_service).then(|| Ops::new(OpsOptions { data_dir: data_dir.clone(), ..Default::default() }));
    let stop = || match &windows {
        Some(ops) => ops.stop(false),
        None => systemctl("stop"),
    };
    let start = || match &windows {
        Some(ops) => ops.start(),
        None => systemctl("start"),
    };
    let options = p2_server_ops::backup::RestoreOptions {
        database_url: &url,
        data_dir: &data_dir,
        source: std::path::Path::new(file),
        stop: (!no_service).then_some(&stop as &dyn Fn() -> p2_server_ops::Outcome),
        start: (!no_service).then_some(&start as &dyn Fn() -> p2_server_ops::Outcome),
    };
    match p2_server_ops::backup::restore_backup(options) {
        Ok(_) => {
            println!("Yedek başarıyla yüklendi.");
            0
        }
        Err(error) => {
            eprintln!("{}", p2_server_ops::error_stack(&error));
            1
        }
    }
}

/// Hizmet kipi (Electron `--p2-server`): yapılandırma server.json'dan (yoksa ortamdan) okunur, API zaten
/// çalışıyorsa çıkılır, göçler uygulanır, ortam kurulur ve sunuma geçilir. server.json kullanılıyorsa
/// sunucu çıktısı server.log'a yazılır (konsol günlüğü); ortamdan geliyorsa (systemd) stdout'a.
fn headless() -> anyhow::Result<Option<i32>> {
    let data_dir = p2_server_ops::data_dir();
    let ops = ops(data_dir.clone());
    let config = ops.read_config().filter(|c| c.configured());
    let to_file = config.is_some();
    let say = |line: &str| {
        if to_file {
            ops.write_log(line);
        } else {
            println!("{line}");
        }
    };
    let (url, access, refresh, port) = match &config {
        Some(c) => (c.database_url.clone().unwrap_or_default(), c.access_secret.clone().unwrap_or_default(), c.refresh_secret.clone().unwrap_or_default(), c.port()),
        None => match std::env::var("DATABASE_URL") {
            Ok(url) if !url.is_empty() => (
                url,
                std::env::var("JWT_ACCESS_SECRET").unwrap_or_default(),
                std::env::var("JWT_REFRESH_SECRET").unwrap_or_default(),
                std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4000),
            ),
            _ => {
                ops.write_log("Önce PostgreSQL bağlantısını yapılandırın.");
                return Ok(Some(1));
            }
        },
    };
    if p2_server_ops::net::api_alive(port, std::time::Duration::from_millis(900)) {
        return Ok(Some(0));
    }
    if let Err(error) = p2_server_ops::migrate::apply_url_blocking(&url) {
        say(&p2_server_ops::error_stack(&error));
        return Ok(Some(1));
    }
    say(&format!("P2 API başlatılıyor: 0.0.0.0:{port}"));
    let keep = |name: &str, fallback: PathBuf| std::env::var_os(name).filter(|v| !v.is_empty()).unwrap_or_else(|| fallback.into_os_string());
    let upload = keep("P2_UPLOAD_DIR", data_dir.join("uploads"));
    let release = keep("P2_RELEASE_DIR", data_dir.join("releases"));
    let update = if to_file { data_dir.join("updates").into_os_string() } else { keep("P2_UPDATE_DIR", data_dir.join("updates")) };
    let version = if to_file { p2_server_ops::VERSION.to_string() } else { std::env::var("P2_SERVER_VERSION").unwrap_or_else(|_| p2_server_ops::VERSION.into()) };
    // SAFETY: henüz başka iş parçacığı yok (tokio çalışma zamanı bu fonksiyondan sonra kuruluyor;
    // göç için kullanılan ops çalışma zamanının iş parçacıkları ortamı okumaz).
    unsafe {
        std::env::set_var("DATABASE_URL", p2_server_ops::config::clean_database_url(&url).unwrap_or(url));
        std::env::set_var("JWT_ACCESS_SECRET", access);
        std::env::set_var("JWT_REFRESH_SECRET", refresh);
        std::env::set_var("PORT", port.to_string());
        std::env::set_var("P2_SERVER_VERSION", version);
        std::env::set_var("P2_UPDATE_DIR", update);
        std::env::set_var("P2_UPLOAD_DIR", upload);
        std::env::set_var("P2_RELEASE_DIR", release);
    }
    if to_file {
        std::fs::create_dir_all(&data_dir)?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(data_dir.join("server.log"))?;
        tracing_subscriber::fmt().with_ansi(false).with_writer(std::sync::Mutex::new(file)).init();
    } else {
        tracing_subscriber::fmt().with_ansi(std::io::stdout().is_terminal()).init();
    }
    Ok(None)
}
