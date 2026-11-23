//! İstemci tarafı oyun içi overlay yöneticisi (Electron `overlayHost.cjs`'nin süreç/enjeksiyon
//! kısmı). Overlay arayüzü her oyun süreci için ayrı bir `p2-overlay` sürecinde çizilir; bu
//! modül onları başlatır, komut yollar ve olaylarını istemciye iletir.
//!
//! Süreç ağacı: Unreal gibi motorlarda kök exe yalnız başlatıcıdır; asıl oyun alt süreçte
//! açılır. Kök sürecin alt ağacı saniyede bir taranır, her yeni 64-bit alt sürece de DLL
//! enjekte edilir (`p2inject64 --attach`). Hangisinin oyun olduğunu DLL'in Present sayacı
//! söyler: ilk kareyi çizen sürecin host'u "hazır" bildirir.
//!
//! GÜVENLİ DEVRE DIŞI: ikililer yoksa, anti-cheat bulunursa ya da enjektör başlatamazsa
//! `launch` `None` döner ve oyun düz başlatılır.

pub mod hotkeys;
#[cfg(windows)]
pub mod procs;
#[cfg(not(windows))]
#[path = "procs_other.rs"]
pub mod procs;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub use p2_overlay_ipc::proto::{CallInfo, CallPerson, FromHost, HotkeySpec, Look, ToHost, Toast, ToastAction, ToastAvatar};
use p2_patcher::{LaunchSpec, Launched};

const NO_OVERLAY_AFTER: Duration = Duration::from_secs(90);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows'ta alt süreç konsol penceresi açmasın.
trait Hidden {
    fn hidden(&mut self) -> &mut Self;
}

impl Hidden for Command {
    fn hidden(&mut self) -> &mut Self {
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(self, CREATE_NO_WINDOW);
        self
    }
}

fn skip_child(name: &str) -> bool {
    const SKIP: [&str; 18] = [
        "crashreportclient", "crashreportclienteditor", "unrealcefsubprocess", "epicwebhelper", "unitycrashhandler32", "unitycrashhandler64",
        "conhost", "cmd", "powershell", "pwsh", "werfault", "dxsetup", "msiexec", "rundll32", "dllhost", "explorer", "gameoverlayui", "steamwebhelper",
    ];
    let lower = name.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    SKIP.contains(&stem) || stem.starts_with("vc_redist") || stem.starts_with("vcredist") || stem.starts_with("ue4prereq") || stem.starts_with("ueprereq")
}

/// Anti-cheat tespiti: bu build'lere enjeksiyon yapılmaz.
pub fn has_anti_cheat(dir: &Path) -> bool {
    const MARKERS: [&str; 7] = ["easyanticheat", "easyanticheat_x64", "beservice", "battleye", "gameguard", "vgc", "vanguard"];
    fn walk(dir: &Path, depth: u32) -> bool {
        if depth > 3 {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if MARKERS.iter().any(|m| name.contains(m)) {
                return true;
            }
            if entry.file_type().is_ok_and(|t| t.is_dir()) && walk(&entry.path(), depth + 1) {
                return true;
            }
        }
        false
    }
    walk(dir, 0)
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub injector: PathBuf,
    pub dll: PathBuf,
    pub host: PathBuf,
}

impl Paths {
    /// Paketli: `<exe klasörü>\overlay\{p2inject64.exe, p2overlay64.dll}` + `<exe klasörü>\p2-overlay.exe`.
    /// Geliştirme: depodaki `native/overlay/dist`. `P2_OVERLAY_DIR` ile değiştirilebilir.
    pub fn locate() -> Option<Self> {
        // Oyun içi overlay (D3D kancası) yalnız Windows'ta.
        if !cfg!(windows) {
            return None;
        }
        let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(d) = std::env::var("P2_OVERLAY_DIR") {
            dirs.push(d.into());
        }
        dirs.push(exe_dir.join("overlay"));
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native/overlay/dist"));
        let dir = dirs.into_iter().find(|d| d.join("p2inject64.exe").exists() && d.join("p2overlay64.dll").exists())?;
        let mut hosts: Vec<PathBuf> = std::env::var("P2_OVERLAY_HOST").map(|h| vec![PathBuf::from(h)]).unwrap_or_default();
        hosts.extend([exe_dir.join("p2-overlay.exe"), dir.join("p2-overlay.exe")]);
        hosts.extend(exe_dir.parent().map(|p| p.join("p2-overlay.exe"))); // cargo örnekleri (target/release/examples)
        let host = hosts.into_iter().find(|p| p.exists())?;
        Some(Self { injector: dir.join("p2inject64.exe").canonicalize().ok()?, dll: dir.join("p2overlay64.dll").canonicalize().ok()?, host })
    }
}

/// Host'a iletilecek oturum bilgisi (giriş yapıldıktan sonra istemci verir).
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub api: String,
    pub client_version: String,
    pub access_token: String,
    pub org: String,
    pub user_id: String,
    pub user_name: String,
    pub light: bool,
    pub shot_dir: String,
    pub hud: bool,
    pub toast_corner: String,
    /// Overlay pencere düzeni dosyası (profil klasöründe; boşsa saklanmaz).
    pub layout_path: String,
    /// Görünüm ayarlarının tamamı (saydamlık, ölçek, gösterge yeri…).
    pub look: Option<Look>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Bir süreç ilk kareyi çizdi, overlay hazır.
    Ready { project_id: String, pid: u32 },
    /// 90 sn'de hiçbir süreç kare çizmedi (D3D12/Vulkan/OpenGL olabilir).
    NoOverlay { project_id: String },
    Visible { project_id: String, pid: u32, on: bool },
    Hotkey { project_id: String, pid: u32, id: u8, down: bool },
    Action { project_id: String, name: String },
    Shot { project_id: String, path: String, width: u32, height: u32 },
    TokenExpired,
    Log { pid: u32, text: String },
    /// Oyunun süreç ağacı bitti.
    Ended { project_id: String },
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

struct HostProc {
    project_id: String,
    root: u32,
    stdin: Mutex<ChildStdin>,
}

impl HostProc {
    fn send(&self, msg: &ToHost) -> bool {
        let mut stdin = self.stdin.lock().unwrap();
        writeln!(stdin, "{}", msg.line()).and_then(|_| stdin.flush()).is_ok()
    }
}

struct Shared {
    context: Context,
    hotkeys: Vec<HotkeySpec>,
    call: CallInfo,
}

pub struct OverlayManager {
    paths: Option<Paths>,
    sink: Sink,
    /// Ayarlar > Kütüphane > "Oyun içi overlay" kapalıysa düz başlatılır.
    enabled: std::sync::atomic::AtomicBool,
    hosts: Mutex<HashMap<u32, Arc<HostProc>>>,
    shared: Mutex<Shared>,
}

impl OverlayManager {
    pub fn new(sink: Sink) -> Arc<Self> {
        let disabled = std::env::var_os("P2_OVERLAY_DISABLE").is_some();
        Arc::new(Self {
            paths: if disabled { None } else { Paths::locate() },
            sink,
            enabled: std::sync::atomic::AtomicBool::new(true),
            hosts: Mutex::new(HashMap::new()),
            shared: Mutex::new(Shared { context: Context::default(), hotkeys: vec![HotkeySpec::PANEL_DEFAULT], call: CallInfo::default() }),
        })
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_shot_dir(&self, dir: &str) {
        self.shared.lock().unwrap().context.shot_dir = dir.to_string();
    }

    /// Görünüm ayarları: yeni açılan oyunlara init ile gider, açık oyunlara hemen iletilir.
    pub fn set_look(&self, look: Look) {
        let look = look.clamped();
        let changed = {
            let mut s = self.shared.lock().unwrap();
            s.context.hud = look.hud;
            s.context.toast_corner = look.toast_corner.clone();
            let changed = s.context.look.as_ref() != Some(&look);
            s.context.look = Some(look.clone());
            changed
        };
        if changed {
            self.broadcast(None, &ToHost::Look(look));
        }
    }

    /// Bu projenin oyununa bağlı bir overlay host'u var mı (başlatma kartı overlay "hazır"ı bekler).
    pub fn has_project(&self, project_id: &str) -> bool {
        self.hosts.lock().unwrap().values().any(|h| h.project_id == project_id)
    }

    pub fn available(&self) -> bool {
        self.paths.is_some()
    }

    pub fn paths(&self) -> Option<&Paths> {
        self.paths.as_ref()
    }

    /// Enjekte edilmiş oyun süreci sayısı (güncelleyici oyun açıkken kurulum yapmaz).
    pub fn active(&self) -> usize {
        self.hosts.lock().unwrap().len()
    }

    pub fn set_context(&self, context: Context) {
        self.shared.lock().unwrap().context = context;
    }

    /// Erişim belirteci değiştiyse açık host'lara iletir.
    pub fn set_token(&self, token: &str) {
        let changed = {
            let mut shared = self.shared.lock().unwrap();
            let changed = shared.context.access_token != token;
            shared.context.access_token = token.to_string();
            changed
        };
        if changed {
            self.broadcast(None, &ToHost::Token { access_token: token.to_string() });
        }
    }

    pub fn set_hotkeys(&self, list: Vec<HotkeySpec>) {
        let mut list: Vec<_> = list.into_iter().filter(|h| h.id > 0 && h.vk > 0).take(8).collect();
        if !list.iter().any(|h| h.id == 1) {
            list.insert(0, HotkeySpec::PANEL_DEFAULT);
        }
        self.shared.lock().unwrap().hotkeys = list.clone();
        self.broadcast(None, &ToHost::Hotkeys { list });
    }

    pub fn set_call(&self, call: CallInfo) {
        {
            let mut shared = self.shared.lock().unwrap();
            if shared.call == call {
                return;
            }
            shared.call = call.clone();
        }
        self.broadcast(None, &ToHost::Call(call));
    }

    /// `project`: yalnız o projenin oyunlarına; `None`: hepsine.
    pub fn toast(&self, project: Option<&str>, toast: Toast) {
        self.broadcast(project, &ToHost::Toast(toast));
    }

    pub fn dismiss(&self, tag: &str) {
        self.broadcast(None, &ToHost::Dismiss { tag: tag.to_string() });
    }

    pub fn set_visible(&self, project: Option<&str>, on: bool) {
        self.broadcast(project, &ToHost::Visible { on });
    }

    pub fn screenshot(&self, project: Option<&str>) {
        self.broadcast(project, &ToHost::Shot);
    }

    fn broadcast(&self, project: Option<&str>, msg: &ToHost) {
        let hosts: Vec<_> = self.hosts.lock().unwrap().values().cloned().collect();
        for host in hosts {
            if project.is_none_or(|p| p == host.project_id) {
                host.send(msg);
            }
        }
    }

    pub fn stop_all(&self) {
        let hosts: Vec<_> = self.hosts.lock().unwrap().drain().map(|(_, h)| h).collect();
        for host in hosts {
            host.send(&ToHost::Stop);
        }
    }

    fn emit(&self, event: Event) {
        (self.sink)(event)
    }

    fn run_injector(&self, args: &[&std::ffi::OsStr]) -> (Option<u32>, i32, String) {
        let Some(paths) = &self.paths else { return (None, -1, "ikili yok".into()) };
        let child = Command::new(&paths.injector).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).hidden().spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return (None, -1, e.to_string()),
        };
        // Yalnız ilk satır (pid): oyun borunun yazma ucunu devralabildiği için EOF gelmeyebilir.
        let mut line = String::new();
        let _ = BufReader::new(child.stdout.take().unwrap()).read_line(&mut line);
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        let mut err = String::new();
        if code != 0 {
            if let Some(mut e) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut e, &mut err);
            }
        }
        (line.trim().parse().ok(), code, err.trim().to_string())
    }

    /// Host sürecini oyun pid'i belli olmadan başlatır: exe'nin soğuk açılışı (ilk çalıştırmada
    /// virüs taraması vb.) DLL'in 5 sn'lik bağlanma penceresini kaçırmasın.
    fn start_host(&self) -> Option<std::process::Child> {
        let paths = self.paths.as_ref()?;
        match Command::new(&paths.host).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).hidden().spawn() {
            Ok(c) => Some(c),
            Err(e) => {
                self.emit(Event::Log { pid: 0, text: format!("overlay host başlatılamadı: {e}") });
                None
            }
        }
    }

    /// Bir oyun süreci için host başlatır; paylaşımlı bellek kurulunca döner.
    fn spawn_host(self: &Arc<Self>, pid: u32, root: u32, project_id: &str, project_name: &str) -> bool {
        match self.start_host() {
            Some(child) => self.bind_host(child, pid, root, project_id, project_name),
            None => false,
        }
    }

    /// Bekleyen host'a oyun pid'ini ve oturumu verir; paylaşımlı bellek kurulunca döner.
    fn bind_host(self: &Arc<Self>, mut child: std::process::Child, pid: u32, root: u32, project_id: &str, project_name: &str) -> bool {
        let (init, call) = {
            let shared = self.shared.lock().unwrap();
            let c = &shared.context;
            (
                ToHost::Init {
                    pid,
                    api: c.api.clone(),
                    access_token: c.access_token.clone(),
                    client_version: c.client_version.clone(),
                    org: c.org.clone(),
                    project: project_id.to_string(),
                    project_name: project_name.to_string(),
                    user_id: c.user_id.clone(),
                    user_name: c.user_name.clone(),
                    hotkeys: shared.hotkeys.clone(),
                    light: c.light,
                    shot_dir: c.shot_dir.clone(),
                    hud: c.hud,
                    toast_corner: c.toast_corner.clone(),
                    look: c.look.clone(),
                    layout_path: c.layout_path.clone(),
                    installed: None,
                    lang: p2_i18n::language().id.to_string(),
                },
                shared.call.clone(),
            )
        };
        let host = Arc::new(HostProc { project_id: project_id.to_string(), root, stdin: Mutex::new(child.stdin.take().unwrap()) });
        host.send(&init);
        if call.active {
            host.send(&ToHost::Call(call));
        }
        self.hosts.lock().unwrap().insert(pid, host);
        let (session_tx, session_rx) = channel::<()>();
        let this = self.clone();
        let project = project_id.to_string();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut session_tx = Some(session_tx);
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(msg) = serde_json::from_str::<FromHost>(&line) else { continue };
                this.on_host(pid, &project, msg, &mut session_tx);
            }
            let _ = child.wait();
            this.hosts.lock().unwrap().remove(&pid);
            this.emit(Event::Log { pid, text: "overlay host kapandı".into() });
        });
        session_rx.recv_timeout(Duration::from_secs(5)).is_ok()
    }

    fn on_host(&self, pid: u32, project: &str, msg: FromHost, session: &mut Option<Sender<()>>) {
        let project_id = project.to_string();
        match msg {
            FromHost::Session => {
                if let Some(tx) = session.take() {
                    let _ = tx.send(());
                }
            }
            FromHost::Ready { width, height } => {
                self.emit(Event::Log { pid, text: format!("hazır ({width}x{height})") });
                self.emit(Event::Ready { project_id, pid });
            }
            FromHost::Visible { on } => self.emit(Event::Visible { project_id, pid, on }),
            FromHost::Hotkey { id, down } => self.emit(Event::Hotkey { project_id, pid, id, down }),
            FromHost::Action { name } => self.emit(Event::Action { project_id, name }),
            FromHost::Shot { path, width, height } => self.emit(Event::Shot { project_id, path, width, height }),
            FromHost::TokenExpired => self.emit(Event::TokenExpired),
            FromHost::Log { text } => self.emit(Event::Log { pid, text }),
            // Bildirim kartı düğmesi: istemciye eylem olarak (`toast:<eylem>:<etiket>`).
            FromHost::ToastAction { action_id, tag, .. } => self.emit(Event::Action { project_id, name: format!("toast:{action_id}:{}", tag.unwrap_or_default()) }),
        }
    }

    /// Oyunu enjeksiyonla başlatır. `None`: overlay mümkün değil, düz başlatılmalı.
    pub fn launch(self: &Arc<Self>, spec: &LaunchSpec) -> Option<Launched> {
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            self.emit(Event::Log { pid: 0, text: "overlay ayarlardan kapalı; düz başlatılıyor".into() });
            return None;
        }
        let paths = self.paths.clone()?;
        if has_anti_cheat(&spec.cwd) {
            self.emit(Event::Log { pid: 0, text: format!("{}: anti-cheat bulundu, enjeksiyon yok", spec.project_name) });
            return None;
        }
        let mut args: Vec<&std::ffi::OsStr> = vec![paths.dll.as_os_str(), spec.exe.as_os_str(), spec.cwd.as_os_str()];
        args.extend(spec.args.iter().map(|a| std::ffi::OsStr::new(a.as_str())));
        let mut standby = self.start_host();
        let (pid, code, err) = self.run_injector(&args);
        if pid.is_none() || code != 0 {
            if let Some(mut c) = standby.take() {
                let _ = c.kill();
            }
        }
        let Some(root) = pid else {
            self.emit(Event::Log { pid: 0, text: format!("{}: enjektör başlatamadı ({err})", spec.project_name) });
            return None;
        };
        let handle = procs::Proc::open(root);
        let done = Arc::new((Mutex::new(None::<i32>), Condvar::new()));
        if code != 0 {
            self.emit(Event::Log { pid: root, text: format!("{}: oyun başladı ama enjeksiyon tutmadı (kod {code}: {err})", spec.project_name) });
        } else {
            self.emit(Event::Log { pid: root, text: format!("{}: kök süreç {root} enjekte edildi; alt süreçler izleniyor", spec.project_name) });
            match standby {
                // Kök süreç host'u kendi iş parçacığında bağlanır: alt süreç takibi (Unreal: asıl oyun
                // alt süreçtir) host'un 5 sn'lik oturum beklemesini beklemeden başlasın.
                Some(child) => {
                    let (this, id, name) = (self.clone(), spec.project_id.clone(), spec.project_name.clone());
                    std::thread::spawn(move || {
                        this.bind_host(child, root, root, &id, &name);
                    });
                }
                None => self.emit(Event::Log { pid: root, text: "overlay host yok".into() }),
            }
        }
        let this = self.clone();
        let (project_id, project_name, d) = (spec.project_id.clone(), spec.project_name.clone(), done.clone());
        let injected = code == 0;
        std::thread::spawn(move || this.track(root, handle, injected, project_id, project_name, d));
        let wait = Box::new(move || {
            let (lock, cv) = &*done;
            let mut code = lock.lock().unwrap();
            while code.is_none() {
                code = cv.wait(code).unwrap();
            }
            code.unwrap_or(0)
        });
        Some(Launched { pid: root, wait })
    }

    /// Kök sürecin alt ağacını izler: yeni 64-bit alt süreçlere enjekte eder; ağaç bitince
    /// host'ları durdurur ve bekleyeni uyandırır.
    fn track(self: Arc<Self>, root: u32, handle: Option<procs::Proc>, injected: bool, project_id: String, project_name: String, done: Arc<(Mutex<Option<i32>>, Condvar)>) {
        let started = Instant::now();
        let mut tree: HashSet<u32> = HashSet::from([root]);
        let mut seen: HashSet<u32> = HashSet::from([root]);
        let mut no_overlay_said = !injected;
        loop {
            let list = procs::processes();
            let alive: HashSet<u32> = list.iter().map(|p| p.pid).collect();
            // Bilinen ağaçtan yaşayanlar tohum: başlatıcı erken çıksa da asıl oyun izlenir.
            let mut desc: HashSet<u32> = tree.iter().copied().filter(|p| alive.contains(p)).collect();
            if desc.is_empty() {
                break;
            }
            let mut grew = true;
            while grew {
                grew = false;
                for p in &list {
                    if desc.contains(&p.ppid) && !desc.contains(&p.pid) {
                        desc.insert(p.pid);
                        grew = true;
                    }
                }
            }
            tree.extend(desc.iter().copied());
            if injected {
                let fresh: Vec<_> = list.iter().filter(|p| desc.contains(&p.pid) && !seen.contains(&p.pid)).cloned().collect();
                for p in &fresh {
                    seen.insert(p.pid);
                    if procs::is_wow64(p.pid) {
                        self.emit(Event::Log { pid: p.pid, text: format!("{project_name}: alt süreç {} 32-bit, atlandı", p.name) });
                        continue;
                    }
                    if skip_child(&p.name) {
                        self.emit(Event::Log { pid: p.pid, text: format!("{project_name}: alt süreç {} yardımcı, atlandı", p.name) });
                        continue;
                    }
                    // Paylaşımlı bellek DLL'den önce hazır olmalı (DLL 5 sn boyunca açmayı dener).
                    if self.spawn_host(p.pid, root, &project_id, &project_name) {
                        let dll = self.paths.as_ref().map(|p| p.dll.clone()).unwrap_or_default();
                        let pid_s = p.pid.to_string();
                        let (_, code, err) = self.run_injector(&["--attach".as_ref(), dll.as_os_str(), pid_s.as_ref()]);
                        if code == 0 {
                            self.emit(Event::Log { pid: p.pid, text: format!("{project_name}: alt süreç {} enjekte edildi", p.name) });
                        } else {
                            self.emit(Event::Log { pid: p.pid, text: format!("{project_name}: alt süreç {} enjekte edilemedi (kod {code}: {err})", p.name) });
                            if let Some(h) = self.hosts.lock().unwrap().remove(&p.pid) {
                                h.send(&ToHost::Stop);
                            }
                        }
                    }
                }
            }
            if !no_overlay_said && started.elapsed() > NO_OVERLAY_AFTER {
                no_overlay_said = true;
                if !self.hosts.lock().unwrap().values().any(|h| h.root == root) {
                    self.emit(Event::NoOverlay { project_id: project_id.clone() });
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        // Ağaç bitti: host'ları durdur.
        let hosts: Vec<_> = {
            let mut map = self.hosts.lock().unwrap();
            let pids: Vec<u32> = map.iter().filter(|(_, h)| h.root == root).map(|(p, _)| *p).collect();
            pids.into_iter().filter_map(|p| map.remove(&p)).collect()
        };
        for h in hosts {
            h.send(&ToHost::Stop);
        }
        let code = handle.and_then(|h| h.exit_code()).unwrap_or(0);
        self.emit(Event::Ended { project_id });
        let (lock, cv) = &*done;
        *lock.lock().unwrap() = Some(code);
        cv.notify_all();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn helpers_skipped() {
        assert!(skip_child("CrashReportClient.exe"));
        assert!(skip_child("UnrealCEFSubProcess.exe"));
        assert!(skip_child("VC_redist.x64.exe"));
        assert!(!skip_child("MyGame-Win64-Shipping.exe"));
    }

    #[test]
    fn process_list_has_self() {
        let me = std::process::id();
        let list = procs::processes();
        assert!(list.iter().any(|p| p.pid == me));
        assert!(!procs::is_wow64(me));
    }
}
