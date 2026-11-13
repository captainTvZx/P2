//! Uçtan uca: gerçek Rust p2-server (C:/p2-rust-target/release/p2-server.exe, PORT 4150)
//! geçici bir P2_UPDATE_DIR'den sahte latest.yml + kurulum dosyası sunar; güncelleyici
//! denetler → indirir → doğrular → hazır olur. Hiçbir kurulum programı çalıştırılmaz: başlatıcı
//! sahtedir ve yalnızca çağrıyı kaydeder. Sunucu ikilisi ya da test ortamı yoksa test atlanır.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use p2_updater::*;

const SERVER_EXE: &str = "C:/p2-rust-target/release/p2-server.exe";
const ENV_FILE: &str = "C:/p2-pg/test-env.json";
const PORT: u16 = 4150;
const INSTALLER: &str = "P2-Server-Setup-9.9.9.exe";
const CLIENT_INSTALLER: &str = "P2-Client-Setup-9.9.9.exe";

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn test_env() -> Option<Vec<(String, String)>> {
    let bytes = fs::read(ENV_FILE).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    Some(value.as_object()?.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect())
}

fn start_server(work: &Path, update_dir: &Path) -> Option<Server> {
    if !Path::new(SERVER_EXE).is_file() {
        eprintln!("ATLANDI: {SERVER_EXE} yok");
        return None;
    }
    let Some(env) = test_env() else {
        eprintln!("ATLANDI: {ENV_FILE} okunamadı");
        return None;
    };
    let log = fs::File::create(work.join("server.log")).unwrap();
    let mut command = Command::new(SERVER_EXE);
    command
        .current_dir(work)
        .envs(env)
        .env("HOST", "127.0.0.1")
        .env("PORT", PORT.to_string())
        .env("P2_UPDATE_DIR", update_dir)
        .env("P2_UPLOAD_DIR", work.join("uploads"))
        .env("P2_RELEASE_DIR", work.join("releases"))
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    let server = Server(command.spawn().expect("p2-server başlatılamadı"));
    let http = reqwest::blocking::Client::new();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        if http.get(format!("http://127.0.0.1:{PORT}/health")).send().is_ok_and(|r| r.status().is_success()) {
            return Some(server);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("p2-server hazır olmadı:\n{}", fs::read_to_string(work.join("server.log")).unwrap_or_default());
}

/// Belirlenimci sahte kurulum içeriği (3 MB).
fn installer_bytes() -> Vec<u8> {
    (0..3 * 1024 * 1024u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect()
}

fn latest_yml(file: &str, sha512: &str, size: usize) -> String {
    format!("version: 9.9.9\nfiles:\n  - url: {file}\n    sha512: {sha512}\n    size: {size}\npath: {file}\nsha512: {sha512}\nreleaseDate: '2026-11-13T10:00:00.000Z'\n")
}

#[derive(Default)]
struct FakeLauncher {
    calls: Mutex<Vec<(PathBuf, Vec<String>, bool)>>,
}

impl InstallerLauncher for FakeLauncher {
    fn launch(&self, installer: &Path, args: &[String], elevate: bool) -> std::io::Result<()> {
        self.calls.lock().unwrap().push((installer.to_path_buf(), args.to_vec(), elevate));
        Ok(())
    }
}

fn wait_until(timeout: Duration, mut test: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if test() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    test()
}

fn recorder() -> (Listener, Arc<Mutex<Vec<UpdaterEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    (Arc::new(move |e: &UpdaterEvent| sink.lock().unwrap().push(e.clone())), events)
}

#[test]
fn gercek_sunucuyla_guncelleme_akisi() {
    let root = tempfile::tempdir().unwrap();
    let work = root.path();
    let updates = work.join("updates");
    fs::create_dir_all(updates.join("server")).unwrap();
    fs::create_dir_all(updates.join("client")).unwrap();
    let bytes = installer_bytes();
    let sha = sha512_base64(&bytes);
    fs::write(updates.join("server").join(INSTALLER), &bytes).unwrap();
    fs::write(updates.join("server").join("latest.yml"), latest_yml(INSTALLER, &sha, bytes.len())).unwrap();

    let Some(_server) = start_server(work, &updates) else { return };
    let feed = server_feed_url(PORT);

    // 1) Düşük düzey: denetim → indirme → doğrulama → kurulum komutu.
    let cache = work.join("cache-a");
    let (listener, events) = recorder();
    let updater = Updater::new(UpdaterConfig::new(&feed, "0.9.9", &cache), listener).unwrap();
    let check = updater.check_for_updates().unwrap();
    assert!(check.is_update_available);
    assert_eq!(check.update_info.version, "9.9.9");
    let done = updater.download_update().unwrap();
    assert!(!done.from_cache);
    assert_eq!(done.file, cache.join("pending").join(INSTALLER));
    assert_eq!(sha512_base64_file(&done.file).unwrap(), sha);
    assert!(!cache.join("pending").join(format!("temp-{INSTALLER}")).exists());
    let info: serde_json::Value = serde_json::from_str(&fs::read_to_string(cache.join("pending").join("update-info.json")).unwrap()).unwrap();
    assert_eq!(info["fileName"], INSTALLER);
    assert_eq!(info["sha512"], sha.as_str());
    {
        let events = events.lock().unwrap();
        assert!(matches!(events[0], UpdaterEvent::CheckingForUpdate));
        assert!(matches!(&events[1], UpdaterEvent::UpdateAvailable(i) if i.version == "9.9.9"));
        assert!(events.iter().any(|e| matches!(e, UpdaterEvent::DownloadProgress(p) if p.percent == 100.0 && p.transferred == bytes.len() as u64)));
        assert!(matches!(events.last().unwrap(), UpdaterEvent::UpdateDownloaded(d) if !d.from_cache));
    }
    let launcher = FakeLauncher::default();
    assert!(updater.quit_and_install(true, true, &launcher).unwrap());
    assert!(!updater.quit_and_install(true, true, &launcher).unwrap(), "ikinci çağrı yok sayılmalı");
    {
        let calls = launcher.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, done.file);
        assert_eq!(calls[0].1, vec!["--updated", "/S", "--force-run"]);
        assert!(!calls[0].2);
    }

    // 2) Yeni oturum aynı önbellekle: indirmeden, doğrulanmış dosya yeniden kullanılır.
    let (listener, events) = recorder();
    let again = Updater::new(UpdaterConfig::new(&feed, "0.9.9", &cache), listener).unwrap();
    assert!(again.check_for_updates().unwrap().is_update_available);
    let cached = again.download_update().unwrap();
    assert!(cached.from_cache);
    assert!(!events.lock().unwrap().iter().any(|e| matches!(e, UpdaterEvent::DownloadProgress(_))));

    // Güncel sürümde güncelleme yok.
    let same = Updater::new(UpdaterConfig::new(&feed, "9.9.9", work.join("cache-x")), Arc::new(|_| {})).unwrap();
    assert!(!same.check_for_updates().unwrap().is_update_available);

    // 3) Yarım kalmış indirme Range ile sürdürülür.
    let cache_b = work.join("cache-b");
    let pending = cache_b.join("pending");
    fs::create_dir_all(&pending).unwrap();
    let marker = serde_json::json!({ "url": INSTALLER, "sha512": sha, "sha2": null }).to_string();
    fs::write(pending.join(format!("temp-{INSTALLER}")), &bytes[..1024 * 1024]).unwrap();
    fs::write(pending.join(format!("temp-{INSTALLER}.json")), &marker).unwrap();
    let resumer = Updater::new(UpdaterConfig::new(&feed, "0.9.9", &cache_b), Arc::new(|_| {})).unwrap();
    resumer.check_for_updates().unwrap();
    let resumed = resumer.download_update().unwrap();
    assert_eq!(resumed.resumed_from, 1024 * 1024);
    assert_eq!(sha512_base64_file(&resumed.file).unwrap(), sha);

    // Bozuk yarım dosya sürdürülürse özet tutmaz → hata, yarım dosya silinir.
    let cache_c = work.join("cache-c");
    let pending_c = cache_c.join("pending");
    fs::create_dir_all(&pending_c).unwrap();
    fs::write(pending_c.join(format!("temp-{INSTALLER}")), vec![0u8; 4096]).unwrap();
    fs::write(pending_c.join(format!("temp-{INSTALLER}.json")), &marker).unwrap();
    let (listener, events) = recorder();
    let broken = Updater::new(UpdaterConfig::new(&feed, "0.9.9", &cache_c), listener).unwrap();
    broken.check_for_updates().unwrap();
    let error = broken.download_update().unwrap_err();
    assert_eq!(error.kind, ErrorKind::ChecksumMismatch);
    assert!(!error.is_quiet());
    assert!(!pending_c.join(format!("temp-{INSTALLER}")).exists());
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, UpdaterEvent::Error(err) if err.kind == ErrorKind::ChecksumMismatch)));

    // 4) Sunucu konsolu durum makinesi: idle → checking → downloading → ready → installing,
    //    API durdurulur, kapanış sorusu kapatılır, kurulum başlatılır, uygulama kapanır.
    let statuses = Arc::new(Mutex::new(Vec::<UpdateState>::new()));
    let launcher = Arc::new(FakeLauncher::default());
    let stopped = Arc::new(Mutex::new(0));
    let before_quit = Arc::new(Mutex::new(0));
    let quit = Arc::new(Mutex::new(0));
    let mut options = ServerUpdaterOptions::new(PORT, "0.9.9");
    options.cache_dir = work.join("cache-server");
    options.launcher = launcher.clone();
    options.broadcast = {
        let statuses = statuses.clone();
        Arc::new(move |s: &UpdateState| statuses.lock().unwrap().push(s.clone()))
    };
    options.stop_server = Some({
        let stopped = stopped.clone();
        Arc::new(move || {
            *stopped.lock().unwrap() += 1;
            Ok(())
        })
    });
    options.before_quit = Some({
        let c = before_quit.clone();
        Arc::new(move || *c.lock().unwrap() += 1)
    });
    options.quit = {
        let c = quit.clone();
        Arc::new(move || *c.lock().unwrap() += 1)
    };
    let server_updater = ServerUpdater::start(options).unwrap();
    assert!(wait_until(Duration::from_secs(30), || *quit.lock().unwrap() == 1), "sunucu güncellemesi kurulmadı: {:?}", server_updater.state());
    let seen: Vec<Status> = statuses.lock().unwrap().iter().map(|s| s.status).collect();
    for wanted in [Status::Checking, Status::Downloading, Status::Ready, Status::Installing] {
        assert!(seen.contains(&wanted), "{wanted:?} görülmedi: {seen:?}");
    }
    let last = server_updater.state();
    assert_eq!(last.status, Status::Installing);
    assert_eq!((last.current.as_str(), last.version.as_str(), last.percent, last.error.as_str()), ("0.9.9", "9.9.9", 100, ""));
    assert_eq!((*stopped.lock().unwrap(), *before_quit.lock().unwrap()), (1, 1));
    {
        let calls = launcher.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, work.join("cache-server").join("pending").join(INSTALLER));
        assert_eq!(calls[0].1, vec!["--updated", "/S", "--force-run"]);
    }
    server_updater.stop();

    // 5) API durdurulamazsa kurulum yapılmaz: ready + hata.
    let launcher = Arc::new(FakeLauncher::default());
    let mut options = ServerUpdaterOptions::new(PORT, "0.9.9");
    options.cache_dir = work.join("cache-server");
    options.launcher = launcher.clone();
    options.stop_server = Some(Arc::new(|| Err(String::new())));
    let refusing = ServerUpdater::start(options).unwrap();
    assert!(wait_until(Duration::from_secs(20), || refusing.state().error == "Sunucu durdurulamadı."), "{:?}", refusing.state());
    assert_eq!(refusing.state().status, Status::Ready);
    assert!(launcher.calls.lock().unwrap().is_empty());
    refusing.stop();

    // 6) İstemci: kanalda sürüm yok (404) → sessizce idle.
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let client_statuses = Arc::new(Mutex::new(Vec::<Status>::new()));
    let launcher = Arc::new(FakeLauncher::default());
    let before_install = Arc::new(Mutex::new(0));
    let client_quit = Arc::new(Mutex::new(0));
    let mut options = ClientUpdaterOptions::new("0.9.9", work.join("client-data"));
    options.cache_dir = work.join("cache-client");
    options.auto_install_delay = Duration::from_millis(300);
    options.install_settle = Duration::from_millis(10);
    options.launcher = launcher.clone();
    options.log = {
        let logs = logs.clone();
        Arc::new(move |m: &str| logs.lock().unwrap().push(m.to_string()))
    };
    options.broadcast = {
        let s = client_statuses.clone();
        Arc::new(move |state: &UpdateState| s.lock().unwrap().push(state.status))
    };
    options.before_install = Some({
        let c = before_install.clone();
        Arc::new(move || *c.lock().unwrap() += 1)
    });
    options.quit = {
        let c = client_quit.clone();
        Arc::new(move || *c.lock().unwrap() += 1)
    };
    let client = ClientUpdater::new(options);
    assert_eq!(client.configure("ftp://x"), Err(ConfigureError::NoAddress));
    let feed_url = client.configure(&format!(" http://127.0.0.1:{PORT}/ ")).unwrap();
    assert_eq!(feed_url, format!("http://127.0.0.1:{PORT}/updates/client/"));
    assert_eq!(client.saved_base(), format!("http://127.0.0.1:{PORT}"));
    assert!(wait_until(Duration::from_secs(10), || client_statuses.lock().unwrap().contains(&Status::Checking) && client.state().status == Status::Idle));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(client.state().error, "", "404 sessiz olmalı");
    assert!(logs.lock().unwrap().iter().any(|l| l.contains("404")));

    // 7) İstemci: özet tutmuyor → idle + görünür hata.
    let client_bytes = &bytes[..512 * 1024];
    fs::write(updates.join("client").join(CLIENT_INSTALLER), client_bytes).unwrap();
    fs::write(updates.join("client").join("latest.yml"), latest_yml(CLIENT_INSTALLER, &sha512_base64(b"yanlis"), client_bytes.len())).unwrap();
    client.check_blocking();
    assert!(wait_until(Duration::from_secs(10), || client.state().error.starts_with("sha512 checksum mismatch")), "{:?}", client.state());
    assert_eq!(client.state().status, Status::Idle);

    // 8) İstemci: doğru yayın → ready → 300 ms sonra (boşta) kendiliğinden kurulum.
    fs::write(updates.join("client").join("latest.yml"), latest_yml(CLIENT_INSTALLER, &sha512_base64(client_bytes), client_bytes.len())).unwrap();
    client.check_blocking();
    assert!(wait_until(Duration::from_secs(15), || *client_quit.lock().unwrap() == 1), "istemci kurulmadı: {:?}", client.state());
    assert_eq!(*before_install.lock().unwrap(), 1);
    {
        let calls = launcher.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, work.join("cache-client").join("pending").join(CLIENT_INSTALLER));
        assert_eq!(calls[0].1, vec!["--updated", "/S", "--force-run"]);
    }
    assert!(!client.install_if_ready(), "kurulum zaten başladı");
    client.stop();

    // 9) Kapalı port (ECONNREFUSED) → sessiz ağ hatası (3 yeniden deneme sonrası).
    let refused = Updater::new(UpdaterConfig::new(server_feed_url(PORT + 1), "0.9.9", work.join("cache-r")), Arc::new(|_| {})).unwrap();
    let error = refused.check_for_updates().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Network);
    assert!(error.is_quiet());
}
