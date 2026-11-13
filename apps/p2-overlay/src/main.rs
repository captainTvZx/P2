//! P2 oyun içi overlay host'u (Electron `overlayHost.cjs` + ekran dışı overlay penceresinin
//! Rust karşılığı). İstemci, enjekte edilen her oyun süreci için bir tane başlatır:
//!
//!   p2-overlay --pid <oyun pid>
//!
//! stdin: satır başına JSON `ToHost` (ilk satır `init`), stdout: `FromHost`.
//! Akış: paylaşımlı bellek oturumu (DLL buna bağlanır) → oyun ilk kareyi çizince "hazır"
//! → arayüz ekran dışı GPUI ile çizilir, önceden çarpılmış BGRA olarak yalnız değişen
//! bölge yazılır. Panel kapalıyken yalnız sabitli pencereler/bildirimler/HUD çizilir ve yalnız değişince.
//!
//! Test kancaları (masaüstüne girdi göndermez):
//! - `P2_OVERLAY_AUTOPILOT="open;tool:tasks;hit:tasks:new;type:Merhaba;key:enter;..."` adımları yalnız
//!   oyun penceresine (PostMessage) gider. Tıklama hedefleri çizimde ölçülen öğelerdir (`ui.rs`).
//! - `p2-overlay --render <çıktı.png> --size 1280x720 [--layout <düzen.json>] [--steps "..."]`: oyun ve
//!   DLL olmadan (init yine stdin'den) arayüzü çizip siyah zemine birleştirilmiş PNG yazar (parite aracı).

mod data;
mod frame;
mod icons;
mod keys;
mod perf;
mod toasts;
mod tools;
mod ui;
mod wm;

use std::io::{BufRead as _, Write as _};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, HeadlessAppContext, PlatformHeadlessRenderer, WindowHandle, px, size};
use p2_client_core::Api;
use p2_overlay_ipc::proto::{FromHost, HotkeySpec, Installed, Look, ToHost, Toast};
use p2_overlay_ipc::{DRAW_HUD, DRAW_PINNED, DRAW_TOAST, InputMsg, Session, input};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE,
};
use windows::core::BOOL;

use data::{Data, DataMsg};
use keys::Out;
use ui::{Cmd, Loadable, OverlayRoot, State};
use wm::ToolId;

const FRAME: Duration = Duration::from_millis(33);

fn emit(msg: FromHost) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", msg.line());
    let _ = out.flush();
}

fn log(text: impl Into<String>) {
    emit(FromHost::Log { text: text.into() });
}

/// Sürecin en büyük görünür penceresi (oyunun ana penceresi).
fn find_window(pid: u32) -> Option<(HWND, u32, u32)> {
    struct Find {
        pid: u32,
        best: Option<(HWND, u32, u32)>,
    }
    extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let find = unsafe { &mut *(lp.0 as *mut Find) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == find.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut r = RECT::default();
            if unsafe { GetClientRect(hwnd, &mut r) }.is_ok() {
                let (w, h) = ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32);
                if w * h > find.best.map(|b| b.1 * b.2).unwrap_or(0) {
                    find.best = Some((hwnd, w, h));
                }
            }
        }
        BOOL(1)
    }
    let mut find = Find { pid, best: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut find as *mut Find as isize));
    }
    find.best
}

struct Proc(HANDLE);
impl Proc {
    fn open(pid: u32) -> Option<Self> {
        unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok().map(Proc)
    }
    fn exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}
impl Drop for Proc {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn stdin_reader() -> Receiver<ToHost> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<ToHost>(&line) {
                Ok(msg) => {
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Err(e) => log(format!("geçersiz komut: {e}")),
            }
        }
        let _ = tx.send(ToHost::Stop); // istemci kapandı
    });
    rx
}

// ---------------------------------------------------------------------------- otomatik test

struct Autopilot {
    steps: Vec<String>,
    next_at: Instant,
}

impl Autopilot {
    fn new(raw: &str, delay: Duration) -> Self {
        Self { steps: raw.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).rev().collect(), next_at: Instant::now() + delay }
    }
    fn from_env() -> Option<Self> {
        Some(Self::new(&std::env::var("P2_OVERLAY_AUTOPILOT").ok()?, Duration::from_millis(1500)))
    }
}

fn post_click(hwnd: HWND, (x, y): (i32, i32)) {
    let lp = LPARAM(((y as u32) << 16 | (x as u32 & 0xffff)) as isize);
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), lp);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), lp);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), lp);
    }
}

fn post_text(hwnd: HWND, text: &str) {
    for unit in text.encode_utf16() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CHAR, WPARAM(unit as usize), LPARAM(1));
        }
    }
}

fn post_key(hwnd: HWND, vk: u16) {
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(vk as usize), LPARAM(1));
        let _ = PostMessageW(Some(hwnd), WM_KEYUP, WPARAM(vk as usize), LPARAM(0xC000_0001u32 as isize));
    }
}

// ---------------------------------------------------------------------------- host

struct Init {
    pid: u32,
    api: Arc<Api>,
    org: String,
    project: String,
    project_name: String,
    user_id: String,
    user_name: String,
    hotkeys: Vec<HotkeySpec>,
    light: bool,
    shot_dir: String,
    look: Look,
    layout_path: String,
    installed: Option<Installed>,
}

fn wait_init(rx: &Receiver<ToHost>) -> anyhow::Result<Init> {
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(ToHost::Init { pid, api, access_token, client_version, org, project, project_name, user_id, user_name, hotkeys, light, shot_dir, hud, toast_corner, look, layout_path, installed }) => {
            let client = Api::new(&api, &client_version);
            client.set_access_only(&access_token);
            let look = look.unwrap_or(Look { hud, toast_corner, ..Default::default() }).clamped();
            Ok(Init { pid, api: Arc::new(client), org, project, project_name, user_id, user_name, hotkeys, light, shot_dir, look, layout_path, installed })
        }
        Ok(other) => anyhow::bail!("ilk komut init değil: {other:?}"),
        Err(_) => anyhow::bail!("init gelmedi"),
    }
}

fn hotkey_keys(hotkeys: &[HotkeySpec], id: u8, default: Option<&HotkeySpec>) -> Vec<String> {
    hotkeys.iter().find(|h| h.id == id).or(default).map(|h| h.keys()).unwrap_or_default()
}
fn panel_keys(hotkeys: &[HotkeySpec]) -> Vec<String> {
    hotkey_keys(hotkeys, 1, Some(&HotkeySpec::PANEL_DEFAULT))
}

fn stamp() -> String {
    // Yerel saat yerine UTC (bağımlılık eklememek için); dosya adı benzersizliği yeterli.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let (d, s) = (secs / 86400, secs % 86400);
    let z = d as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02}_{:02}-{:02}-{:02}", s / 3600, s % 3600 / 60, s % 60)
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let cleaned = cleaned.trim().chars().take(60).collect::<String>();
    if cleaned.is_empty() { "Oyun".into() } else { cleaned }
}

struct Host {
    /// DLL paylaşımlı bellek oturumu (`--render` kipinde yok).
    session: Option<Session>,
    init: Init,
    data: Data,
    cx: HeadlessAppContext,
    window: WindowHandle<OverlayRoot>,
    w: u32,
    h: u32,
    prev: Vec<u8>,
    next: Vec<u8>,
    frames_written: u64,
    dirty: bool,
    last_render: Instant,
    last_input: Instant,
    last_mask: u32,
    keys: keys::InputState,
    perf: Option<perf::Perf>,
    shot: Option<(u32, Instant)>,
    loaded_at: std::collections::HashMap<ToolId, Instant>,
    /// Arayüz ölçeği (Electron `setZoom`): GPUI penceresi `w/ölçek × h/ölçek` mantıksal
    /// boyutta çizilir, kare `w × h`'ye örneklenir; fare koordinatları ölçeğe bölünür.
    scale: f32,
    /// Giriş/çıkış animasyonu sürerken kare kare çizilir.
    animate_until: Option<Instant>,
    /// HUD açıkken sunucu gidiş-dönüş yoklaması (usePing.ts) çalışır.
    ping_on: Arc<std::sync::atomic::AtomicBool>,
}

/// Fiziksel yüzey boyutunun ölçekli mantıksal karşılığı.
fn logical(w: u32, h: u32, scale: f32) -> (u32, u32) {
    (((w as f32 / scale).round() as u32).max(1), ((h as f32 / scale).round() as u32).max(1))
}

impl Host {
    fn open_window(cx: &mut HeadlessAppContext, st: State) -> anyhow::Result<WindowHandle<OverlayRoot>> {
        let (w, h) = (st.w, st.h);
        let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| OverlayRoot::new(st, window, cx)))?;
        cx.run_until_parked();
        Ok(window)
    }

    fn update<R>(&mut self, f: impl FnOnce(&mut OverlayRoot, &mut gpui_kit::Window, &mut gpui_kit::Context<OverlayRoot>) -> R) -> Option<R> {
        self.window.update(&mut self.cx, |view, window, cx| f(view, window, cx)).ok()
    }

    /// Oyunun arka tamponu boyut değiştirdi: pencere yeniden kurulur, durum taşınır.
    fn resize(&mut self, w: u32, h: u32) {
        let (w, h) = (w.clamp(320, p2_overlay_ipc::MAX_W), h.clamp(240, p2_overlay_ipc::MAX_H));
        if (w, h) == (self.w, self.h) {
            return;
        }
        self.rebuild(w, h, self.scale);
    }

    /// Mantıksal arayüz noktası → oyun penceresindeki (fiziksel) nokta.
    fn physical(&self, (x, y): (i32, i32)) -> (i32, i32) {
        ((x as f32 * self.scale).round() as i32, (y as f32 * self.scale).round() as i32)
    }

    /// Yüzeyi (fiziksel boyut ya da ölçek değişince) yeniden kurar; panel durumu taşınır.
    fn rebuild(&mut self, w: u32, h: u32, scale: f32) {
        let empty = || State::new(0., 0., String::new(), String::new(), String::new(), String::new(), Vec::new(), wm::Store::load("", None, 320., 240., 0.92));
        let Some(mut st) = self.update(|v, _, _| std::mem::replace(&mut v.st, empty())) else { return };
        let _ = self.cx.update_window(self.window.into(), |_, window, _| window.remove_window());
        let (lw, lh) = logical(w, h, scale);
        st.w = lw as f32;
        st.h = lh as f32;
        st.store.apply_viewport(lw as f32, lh as f32);
        match Self::open_window(&mut self.cx, st) {
            Ok(window) => {
                self.window = window;
                self.w = w;
                self.h = h;
                self.scale = scale;
                self.prev = vec![0; (w * h * 4) as usize];
                self.next = vec![0; (w * h * 4) as usize];
                self.frames_written = 0;
                self.dirty = true;
                log(format!("overlay yüzeyi {w}x{h} (ölçek {scale:.2}, mantıksal {lw}x{lh})"));
            }
            Err(e) => log(format!("yeniden boyutlandırılamadı: {e}")),
        }
    }

    /// Görünüm ayarları (açılışta ve ayarlar değişince).
    fn apply_look(&mut self, look: Look) {
        let look = look.clamped();
        log(format!(
            "görünüm: ölçek={:.2} saydamlık={:.2} hud={} yer={} ayrıntı={} köşe={} hareket-azalt={}",
            look.scale, look.opacity, look.hud, look.hud_position, look.hud_details, look.toast_corner, look.reduce_motion
        ));
        self.ping_on.store(look.hud, std::sync::atomic::Ordering::Relaxed);
        let scale = look.scale;
        self.update(|v, _, cx| {
            v.st.store.default_pin_opacity = look.opacity;
            v.st.look = look;
            cx.notify();
        });
        if (scale - self.scale).abs() > 0.001 {
            self.rebuild(self.w, self.h, scale);
        }
        self.dirty = true;
    }

    fn set_visible(&mut self, on: bool, from_dll: bool) {
        if !from_dll {
            if let Some(s) = &self.session {
                s.set_visible(on);
            }
        }
        let changed = self
            .update(|v, _, cx| {
                let was = v.st.visible;
                v.st.visible = on;
                if on && !was {
                    v.on_shown();
                } else if !on && was {
                    v.flush_note(cx);
                    v.on_hidden();
                }
                cx.notify();
                was != on
            })
            .unwrap_or(false);
        if !on {
            self.keys.reset();
        }
        self.run_cmds();
        self.dirty = true;
        if changed || !from_dll {
            emit(FromHost::Visible { on });
            log(format!("panel {}", if on { "açıldı" } else { "kapandı" }));
        }
    }

    fn load_tool(&mut self, tool: ToolId, force: bool) {
        let fresh = self.loaded_at.get(&tool).is_some_and(|t| t.elapsed() < Duration::from_secs(20));
        if fresh && !force {
            return;
        }
        self.loaded_at.insert(tool, Instant::now());
        match tool {
            ToolId::Tasks => {
                self.update(|v, _, _| {
                    if !matches!(v.st.tasks, Loadable::Ready(_)) {
                        v.st.tasks = Loadable::Loading;
                    }
                });
                self.data.load_tasks();
                self.data.load_team();
            }
            ToolId::Notes => self.data.load_notes(),
            ToolId::Team => self.data.load_team(),
            ToolId::Voice => {
                self.data.load_voice();
                self.data.load_team();
            }
            ToolId::Summary => {
                let ch = self.init.installed.as_ref().map(|i| i.channel.clone()).unwrap_or_default();
                self.data.load_release(ch);
                self.data.load_hub();
            }
            _ => {}
        }
    }

    fn toast(&mut self, toast: Toast) {
        self.update(|v, _, cx| {
            v.st.toasts.push(toast);
            cx.notify();
        });
        self.animate_until = Some(Instant::now() + p2_ui::motion::M_ELASTIC + Duration::from_millis(120));
        self.dirty = true;
    }

    fn take_shot(&mut self) {
        if self.shot.is_some() {
            return;
        }
        let Some(s) = &self.session else { return };
        let id = s.request_shot();
        self.shot = Some((id, Instant::now()));
        log(format!("ekran görüntüsü istendi (id {id})"));
    }

    fn finish_shot(&mut self) {
        let Some(shot) = self.session.as_ref().and_then(|s| s.read_shot()) else {
            log("ekran görüntüsü alınamadı (DLL)");
            self.toast(Toast { kind: "error".into(), title: "Ekran görüntüsü alınamadı".into(), duration_ms: Some(4000), ..Default::default() });
            return;
        };
        let dir = if self.init.shot_dir.is_empty() {
            std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Pictures").join("P2")
        } else {
            std::path::PathBuf::from(&self.init.shot_dir)
        }
        .join(safe_name(&self.init.project_name));
        let file = dir.join(format!("{}.png", stamp()));
        let (w, h) = (shot.width, shot.height);
        let mut rgba = shot.bgra;
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        let saved = std::fs::create_dir_all(&dir).is_ok() && image::RgbaImage::from_raw(w, h, rgba).is_some_and(|img| img.save(&file).is_ok());
        if saved {
            log(format!("ekran görüntüsü kaydedildi {} ({w}x{h})", file.display()));
            emit(FromHost::Shot { path: file.display().to_string(), width: w, height: h });
            let path = file.display().to_string();
            self.update(|v, _, _| {
                v.st.shots.insert(0, ui::ShotRec { path: path.clone(), width: w, height: h, at: Instant::now() });
                v.st.shots.truncate(12);
            });
            let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // Electron: "Ekran görüntüsü alındı" + Paylaş / Klasörü aç.
            self.toast(Toast {
                kind: "screenshot".into(),
                title: "Ekran görüntüsü alındı".into(),
                text: name,
                duration_ms: Some(6000),
                actions: vec![
                    p2_overlay_ipc::proto::ToastAction { id: "share".into(), label: "Paylaş".into(), primary: true },
                    p2_overlay_ipc::proto::ToastAction { id: "folder".into(), label: "Klasörü aç".into(), primary: false },
                ],
                payload: Some(serde_json::json!({ "path": path })),
                ..Default::default()
            });
        } else {
            self.toast(Toast { kind: "error".into(), title: "Ekran görüntüsü kaydedilemedi".into(), duration_ms: Some(4000), ..Default::default() });
        }
    }

    fn run_cmds(&mut self) {
        loop {
            let cmds = self.update(|v, _, _| std::mem::take(&mut v.pending)).unwrap_or_default();
            if cmds.is_empty() {
                return;
            }
            for cmd in cmds {
                self.dirty = true;
                match cmd {
                    Cmd::Action(name) => {
                        log(format!("eylem: {name}"));
                        emit(FromHost::Action { name });
                    }
                    Cmd::ToastAction { toast_id, tag, action_id, payload } => {
                        log(format!("bildirim eylemi: {action_id} ({toast_id})"));
                        // Ekran görüntüsü kartı: "Paylaş" Ekran görüntüsü penceresini açar (Electron gibi).
                        if action_id == "share" {
                            self.update(|v, _, _| v.open_window("capture"));
                        }
                        emit(FromHost::ToastAction { toast_id, tag, action_id, payload });
                    }
                    Cmd::Log(text) => log(text),
                    Cmd::Shot => self.take_shot(),
                    Cmd::SetVisible(on) => self.set_visible(on, false),
                    Cmd::Load(tool) => {
                        log(format!("araç: {tool:?}"));
                        self.load_tool(tool, false);
                    }
                    Cmd::CreateTask { board, input } => {
                        log(format!("görev ekleniyor: {}", input["title"].as_str().unwrap_or("")));
                        self.data.create_task(board, input);
                    }
                    Cmd::MoveTask { board, task, from, to } => {
                        log(format!("görev durumu: {from} → {to}"));
                        self.data.move_task(board, task, to);
                    }
                    Cmd::CreateNote => {
                        log("not oluşturuluyor");
                        self.data.create_note();
                    }
                    Cmd::SaveNote { id, patch } => self.data.save_note(id, patch),
                    Cmd::DeleteNote(id) => self.data.delete_note(id),
                    Cmd::Look(look) => {
                        let hud = look.hud;
                        let before = self.update(|v, _, _| v.st.look.hud).unwrap_or(false);
                        self.apply_look(look.clone());
                        if hud != before {
                            log(format!("HUD {}", if hud { "sabitlendi" } else { "kaldırıldı" }));
                        }
                        emit(FromHost::Action { name: format!("look:{}", serde_json::to_string(&look).unwrap_or_default()) });
                    }
                }
            }
        }
    }

    fn on_data(&mut self, msg: DataMsg) {
        self.dirty = true;
        match msg {
            DataMsg::Tasks(r) => {
                if let Ok((boards, by_board)) = &r {
                    log(format!("görevler: {} pano ({} görev)", boards.len(), by_board.iter().map(|(_, t)| t.len()).sum::<usize>()));
                }
                self.update(|v, _, cx| {
                    v.st.tasks = match r {
                        Ok((boards, by_board)) => Loadable::Ready(ui::TasksData { boards, by_board }),
                        Err(e) => Loadable::Failed(e),
                    };
                    cx.notify();
                });
            }
            DataMsg::Notes(r) => {
                if let Ok(notes) = &r {
                    log(format!("notlar: {}", notes.len()));
                }
                let empty = matches!(&r, Ok(n) if n.is_empty());
                let create = self
                    .update(|v, window, cx| {
                        v.st.notes = match r {
                            Ok(n) => Loadable::Ready(n),
                            Err(e) => Loadable::Failed(e),
                        };
                        // Geniş görünümde seçili yoksa ilk not (ProjectNotesPanel); overlay ilk açılışta liste boşsa bir not açar.
                        if v.ui.note_sel.as_ref().is_some_and(|id| !v.st.notes.ready().is_some_and(|n| n.iter().any(|x| &x.id == id))) {
                            v.ui.note_sel = None;
                            v.ui.note_bound = None;
                        }
                        v.bind_note(window, cx);
                        cx.notify();
                        let auto = empty && !v.ui.note_auto;
                        if empty {
                            v.ui.note_auto = true;
                        }
                        auto
                    })
                    .unwrap_or(false);
                if create {
                    log("not oluşturuluyor (liste boş)");
                    self.data.create_note();
                }
            }
            DataMsg::NoteCreated(note) => {
                log(format!("not açıldı: {}", note.id));
                self.update(|v, window, cx| {
                    v.ui.note_sel = Some(note.id.clone());
                    v.ui.note_list = false;
                    v.ui.note_bound = None;
                    v.bind_note(window, cx);
                    v.ui.note_title.update(cx, |s, cx| s.focus(window, cx));
                    cx.notify();
                });
            }
            DataMsg::NoteSaving(id, on) => {
                self.update(|v, _, cx| {
                    v.st.saving_notes.retain(|x| x != &id);
                    if on {
                        v.st.saving_notes.push(id);
                    }
                    cx.notify();
                });
            }
            DataMsg::Team(r) => {
                if let Ok(m) = &r {
                    log(format!("ekip: {}", m.len()));
                }
                self.update(|v, _, cx| {
                    v.st.team = match r {
                        Ok(m) => Loadable::Ready(m),
                        Err(e) => Loadable::Failed(e),
                    };
                    cx.notify();
                });
            }
            DataMsg::VoiceChannels(list) => {
                self.update(|v, _, cx| {
                    v.st.voice_channels = list;
                    cx.notify();
                });
            }
            DataMsg::Release(r) => {
                self.update(|v, _, cx| {
                    v.st.live_release = r;
                    cx.notify();
                });
            }
            DataMsg::Hub { mine, team, posts } => {
                self.update(|v, _, cx| {
                    v.st.hub = Some((mine, team, posts));
                    cx.notify();
                });
            }
            DataMsg::Info(text) => log(text),
            DataMsg::Error(text) => {
                log(format!("hata: {text}"));
                self.toast(Toast { kind: "error".into(), title: "İşlem tamamlanamadı".into(), text, duration_ms: Some(5000), ..Default::default() });
            }
            DataMsg::TokenExpired => {
                emit(FromHost::TokenExpired);
                self.toast(Toast { kind: "error".into(), title: "Oturum yenileniyor".into(), text: "Birazdan tekrar dene.".into(), duration_ms: Some(4000), ..Default::default() });
            }
            DataMsg::Ping { ping, loss } => {
                self.update(|v, _, cx| {
                    v.st.ping = ping;
                    v.st.loss = loss;
                    cx.notify();
                });
            }
        }
    }

    fn on_input(&mut self, msg: InputMsg) {
        match msg.kind {
            input::TOGGLE => self.set_visible(msg.flags & 1 == 1, true),
            input::RESIZE => self.resize(msg.x.max(1) as u32, msg.y.max(1) as u32),
            input::FOCUS => {
                if msg.flags & 1 == 0 {
                    self.keys.reset();
                }
            }
            input::HOTKEY => emit(FromHost::Hotkey { id: msg.key as u8, down: msg.flags & 1 == 1 }),
            input::SHOT_READY => {
                if self.shot.is_some_and(|(id, _)| id == msg.key) {
                    self.shot = None;
                    if msg.flags & 1 == 1 {
                        log("ekran görüntüsü DLL tarafında başarısız");
                        self.toast(Toast { kind: "error".into(), title: "Ekran görüntüsü alınamadı".into(), duration_ms: Some(4000), ..Default::default() });
                    } else {
                        self.finish_shot();
                    }
                }
            }
            _ => {
                let visible = self.update(|v, _, _| v.st.visible).unwrap_or(false);
                if !visible {
                    return;
                }
                // Esc zinciri (OverlayApp.tsx + OverlayWindow.tsx).
                if msg.kind == input::KEY && msg.key == 0x1b && msg.flags & input::FLAG_DOWN != 0 {
                    let close = self.update(|v, window, cx| {
                        let r = v.escape(window, cx);
                        cx.notify();
                        r
                    });
                    self.dirty = true;
                    if close == Some(true) {
                        self.set_visible(false, false);
                    }
                    return;
                }
                // DLL fiziksel yüzey koordinatı verir; arayüz ölçekli mantıksal boyutta.
                let mut msg = msg;
                if matches!(msg.kind, input::MOUSE_MOVE | input::MOUSE_BTN | input::MOUSE_WHEEL) && (self.scale - 1.).abs() > 0.001 {
                    msg.x = (msg.x as f32 / self.scale).round() as i32;
                    msg.y = (msg.y as f32 / self.scale).round() as i32;
                }
                let Some(out) = self.keys.translate(&msg) else { return };
                self.last_input = Instant::now();
                self.dirty = true;
                let _ = self.cx.update_window(self.window.into(), |_, window, cx| match out {
                    Out::Event(event) => {
                        window.dispatch_event(event, cx);
                    }
                    Out::Keystroke(k) => {
                        window.dispatch_keystroke(k, cx);
                    }
                });
                // Sürükleme / boyutlandırma GPUI olay zincirinden bağımsız izlenir.
                let (x, y) = (msg.x as f32, msg.y as f32);
                let up = msg.kind == input::MOUSE_BTN && msg.flags & input::FLAG_DOWN == 0;
                if msg.kind == input::MOUSE_MOVE || up {
                    self.update(|v, _, cx| {
                        let a = v.pointer_move(x, y);
                        let b = up && v.pointer_up();
                        if a || b {
                            cx.notify();
                        }
                    });
                }
                self.run_cmds();
            }
        }
    }

    fn on_command(&mut self, cmd: ToHost) -> bool {
        self.dirty = true;
        match cmd {
            ToHost::Stop => return false,
            ToHost::Init { .. } => {}
            ToHost::Token { access_token } => {
                self.init.api.set_access_only(&access_token);
                log("erişim belirteci yenilendi");
            }
            ToHost::Toast(t) => {
                log(format!("bildirim: {}", t.title));
                self.toast(t);
            }
            ToHost::Dismiss { tag } => {
                self.update(|v, _, cx| {
                    v.st.toasts.dismiss_tag(&tag);
                    cx.notify();
                });
                self.animate_until = Some(Instant::now() + toasts::OUT + Duration::from_millis(60));
            }
            ToHost::Visible { on } => self.set_visible(on, false),
            ToHost::Call(info) => {
                self.update(|v, _, cx| {
                    v.st.call = info;
                    cx.notify();
                });
            }
            ToHost::Hotkeys { list } => {
                let ipc: Vec<_> = list.iter().map(HotkeySpec::to_ipc).collect();
                if let Some(s) = &self.session {
                    s.set_hotkeys(&ipc);
                }
                let (panel, shot, mute) = (panel_keys(&list), hotkey_keys(&list, 2, None), hotkey_keys(&list, 3, None));
                self.update(|v, _, _| {
                    v.st.panel_keys = panel;
                    v.st.shot_keys = shot;
                    v.st.mute_keys = mute;
                });
                self.init.hotkeys = list;
            }
            ToHost::Shot => self.take_shot(),
            ToHost::Look(look) => self.apply_look(look),
            ToHost::Open { window, .. } => {
                if wm::base_tool(&window).is_some() {
                    log(format!("pencere açılıyor: {window}"));
                    self.update(|v, _, cx| {
                        v.open_window(&window);
                        cx.notify();
                    });
                    self.set_visible(true, false);
                }
            }
        }
        true
    }

    /// GPUI çizimi → önceden çarpılmış BGRA (`self.next`).
    fn draw(&mut self) -> anyhow::Result<()> {
        let since = self.last_render.elapsed();
        self.last_render = Instant::now();
        self.cx.advance_clock(since.min(Duration::from_millis(500)));
        self.cx.run_until_parked();
        self.run_cmds();
        let image = self.cx.capture_screenshot(self.window.into())?;
        let (w, h) = (self.w, self.h);
        let (lw, lh) = logical(w, h, self.scale);
        if image.width() != lw * 2 || image.height() != lh * 2 {
            anyhow::bail!("beklenmeyen overlay boyutu {}x{}", image.width(), image.height());
        }
        if (lw, lh) == (w, h) {
            frame::downsample_premultiplied_bgra(image.as_raw(), w, h, &mut self.next);
        } else {
            frame::resample_premultiplied_bgra(image.as_raw(), lw * 2, lh * 2, w, h, &mut self.next);
        }
        Ok(())
    }

    fn render(&mut self) -> anyhow::Result<()> {
        self.draw()?;
        let (w, h) = (self.w, self.h);
        let dirty = if self.frames_written == 0 { None } else { frame::dirty_rect(&self.prev, &self.next, w, h) };
        if self.frames_written == 0 || dirty.is_some() {
            if let Some(s) = &self.session {
                s.write_frame(&self.next, w, h, dirty);
            }
            self.frames_written += 1;
            std::mem::swap(&mut self.prev, &mut self.next);
        }
        Ok(())
    }

    /// Son kareyi siyah zemine birleştirip PNG yazar (parite aracı).
    fn dump(&mut self, path: &str) {
        if let Err(e) = self.draw() {
            log(format!("çizim hatası: {e}"));
            return;
        }
        let mut rgba = self.next.clone();
        for px in rgba.chunks_exact_mut(4) {
            // Önceden çarpılmış BGRA, siyah üstüne: renk = c (alfa zaten çarpılmış).
            px.swap(0, 2);
            px[3] = 255;
        }
        match image::RgbaImage::from_raw(self.w, self.h, rgba).map(|img| img.save(path)) {
            Some(Ok(())) => log(format!("kare yazıldı {path}")),
            _ => log(format!("kare yazılamadı {path}")),
        }
    }

    /// Ölçülen öğenin merkezi (mantıksal); `key` tam ya da önek.
    fn hit(&mut self, key: &str) -> Option<(i32, i32)> {
        let found = self.update(|v, _, _| v.hit(key)).flatten();
        if found.is_none() {
            let keys = self.update(|v, _, _| v.hit_keys()).unwrap_or_default();
            log(format!("hedef yok: {key} (bilinen: {})", keys.join(", ")));
        }
        found
    }

    /// Tıklama: oyun penceresine PostMessage (gerçek kip) ya da doğrudan GPUI'ye (`--render`).
    fn click_at(&mut self, hwnd: Option<HWND>, pt: (i32, i32)) {
        match hwnd {
            Some(h) => post_click(h, self.physical(pt)),
            None => {
                let (x, y) = self.physical(pt);
                for (kind, flags) in [(input::MOUSE_MOVE, 0), (input::MOUSE_BTN, input::FLAG_DOWN | input::FLAG_LEFT), (input::MOUSE_BTN, input::FLAG_LEFT)] {
                    self.on_input(InputMsg { kind, flags, x, y, ..Default::default() });
                }
            }
        }
    }

    fn autopilot(&mut self, pilot: &mut Autopilot, hwnd: Option<HWND>) {
        if Instant::now() < pilot.next_at {
            return;
        }
        let Some(step) = pilot.steps.pop() else { return };
        let mut delay = Duration::from_millis(1200);
        let (verb, arg) = step.split_once(':').unwrap_or((step.as_str(), ""));
        log(format!("adım: {step}"));
        match verb {
            "open" => self.set_visible(true, false),
            "close" => self.set_visible(false, false),
            // Araç çubuğu düğmesi (sırayla: ses, ekip, sohbet, notlar, görevler, ekran görüntüsü galerisi,
            // özet, tarayıcı, performans, [ekran görüntüsü al, ayarlar, kapat]).
            "click" => {
                let i: usize = arg.parse().unwrap_or(0);
                let key = match i {
                    0..=8 => format!("tb:{}", wm::meta(ui::BAR_TOOLS[i]).key),
                    9 => "tb:shot".into(),
                    10 => "tb:settings".into(),
                    _ => "tb:close".into(),
                };
                if let Some(pt) = self.hit(&key) {
                    self.click_at(hwnd, pt);
                }
            }
            "tool" => {
                if let Some(pt) = self.hit(&format!("tb:{arg}")) {
                    self.click_at(hwnd, pt);
                }
            }
            "click-close" => {
                if let Some(pt) = self.hit("tb:close") {
                    self.click_at(hwnd, pt);
                }
            }
            // n. görev satırının durum listesi (üstten).
            "click-chip" => {
                let n: usize = arg.parse().unwrap_or(0);
                let pts = self.update(|v, _, _| {
                    let map = v.measured.borrow();
                    let mut list: Vec<_> = map.iter().filter(|(k, _)| k.starts_with("task-status:")).map(|(_, b)| (f32::from(b.origin.x + b.size.width / 2.) as i32, f32::from(b.origin.y + b.size.height / 2.) as i32)).collect();
                    list.sort_by_key(|p| p.1);
                    list
                });
                if let Some(pt) = pts.and_then(|l| l.get(n).copied()) {
                    self.click_at(hwnd, pt);
                }
            }
            "hit" => {
                if let Some(pt) = self.hit(arg) {
                    self.click_at(hwnd, pt);
                }
            }
            "pick" => {
                if let Some(pt) = self.hit(&format!("opt:{arg}")) {
                    self.click_at(hwnd, pt);
                }
            }
            "scrolls" => {
                let info = self.update(|v, _, _| v.scrolls.borrow().iter().map(|(k, h)| format!("{k}: görünüm {:?} en çok {:?}", h.bounds().size.height, h.max_offset().y)).collect::<Vec<_>>().join("; ")).unwrap_or_default();
                log(format!("kaydırma: {info}"));
            }
            "hits" => {
                let keys = self.update(|v, _, _| v.hit_keys()).unwrap_or_default();
                log(format!("hedefler: {}", keys.join(", ")));
            }
            "tasks-view" => {
                self.update(|v, _, cx| {
                    v.ui.tasks_view = if arg == "board" { tools::TasksView::Board } else { tools::TasksView::List };
                    cx.notify();
                });
            }
            "look" => {
                // look:scale=1.25,opacity=0.6,hudpos=top-right,details=off,motion=reduce,hud=on,corner=top-left
                let mut look = self.update(|v, _, _| v.st.look.clone()).unwrap_or_default();
                for kv in arg.split(',') {
                    let (k, val) = kv.split_once('=').unwrap_or((kv, ""));
                    match k {
                        "scale" => look.scale = val.parse().unwrap_or(1.),
                        "opacity" => look.opacity = val.parse().unwrap_or(0.92),
                        "hudpos" => look.hud_position = val.into(),
                        "details" => look.hud_details = val == "on",
                        "motion" => look.reduce_motion = val == "reduce",
                        "hud" => look.hud = val == "on",
                        "corner" => look.toast_corner = val.into(),
                        _ => {}
                    }
                }
                self.apply_look(look);
            }
            "type" => match hwnd {
                Some(h) => post_text(h, arg),
                None => {
                    for unit in arg.encode_utf16() {
                        self.on_input(InputMsg { kind: input::CHAR, key: unit as u32, ..Default::default() });
                    }
                }
            },
            "key" => {
                let vk = match arg {
                    "enter" => 0x0d,
                    "escape" => 0x1b,
                    "backspace" => 0x08,
                    _ => 0,
                };
                match hwnd {
                    Some(h) => post_key(h, vk),
                    None => {
                        self.on_input(InputMsg { kind: input::KEY, key: vk as u32, flags: input::FLAG_DOWN, ..Default::default() });
                        self.on_input(InputMsg { kind: input::KEY, key: vk as u32, flags: 0, ..Default::default() });
                    }
                }
            }
            "wait" => delay = Duration::from_millis(arg.parse().unwrap_or(1000)),
            "toast" => self.toast(Toast { kind: "info".into(), title: arg.to_string(), duration_ms: Some(8000), ..Default::default() }),
            "toast-json" => match serde_json::from_str::<Toast>(arg) {
                Ok(t) => self.toast(t),
                Err(e) => log(format!("bildirim okunamadı: {e}")),
            },
            "shot" => self.take_shot(),
            "dump" => self.dump(arg),
            "state" => {
                let (visible, focus, hud, toasts, open) = self
                    .update(|v, _, _| {
                        let open: Vec<String> = v.st.store.open_windows().iter().map(|w| format!("{}{}", w.id, if w.pinned { "*" } else { "" })).collect();
                        (v.st.visible, v.st.store.layout.focus.clone().unwrap_or_default(), v.st.hud(), v.st.toasts.len(), open.join(","))
                    })
                    .unwrap_or_default();
                let mask = self.session.as_ref().map(|s| s.stats().draw_mask).unwrap_or(self.last_mask);
                let (lw, lh) = logical(self.w, self.h, self.scale);
                log(format!("durum: görünür={visible} odak={focus} pencereler=[{open}] hud={hud} bildirim={toasts} maske={mask} ölçek={:.2} mantıksal={lw}x{lh}", self.scale));
            }
            _ => log(format!("bilinmeyen adım: {step}")),
        }
        pilot.next_at = Instant::now() + delay;
        self.dirty = true;
    }

    /// Saniyelik iş: performans örneği, süresi dolan bildirimler.
    fn second(&mut self, fixed: Option<&perf::Sample>) {
        let sample = match (&mut self.perf, &self.session, fixed) {
            (_, _, Some(s)) => s.clone(),
            (Some(p), Some(s), None) => {
                p.tick(s.stats().frames);
                p.current.clone()
            }
            _ => return,
        };
        let (visible, hud, perf_open) = self.update(|v, _, _| (v.st.visible, v.st.hud(), v.st.store.win("perf").is_some_and(|w| w.open))).unwrap_or_default();
        let show = hud || (visible && perf_open) || perf_open;
        let expired = self
            .update(|v, _, cx| {
                v.st.history.push(sample.fps.max(0.).round());
                if v.st.history.len() > 60 {
                    v.st.history.remove(0);
                }
                v.st.perf = sample;
                let expired = v.st.expire();
                if show || expired {
                    cx.notify();
                }
                expired
            })
            .unwrap_or(false);
        self.dirty |= show || expired;
    }
}

/// cosmic-text'in gördüğü Inter ailesi (gömülü değişken TTF yüklendikten sonra).
fn pick_font(cx: &mut HeadlessAppContext) {
    let names = cx.update(|cx| {
        p2_ui::fonts::install(cx);
        cx.text_system().all_font_names()
    });
    let pick = names.iter().find(|n| n.as_str() == p2_ui::tokens::FONT).or_else(|| names.iter().find(|n| n.starts_with("Inter"))).cloned();
    if let Some(name) = pick {
        let _ = ui::FONT.set(Box::leak(name.into_boxed_str()));
    }
}

fn new_cx(light: bool) -> HeadlessAppContext {
    // Ekran dışı GPUI: cosmic-text (DirectWrite ekran dışı çizicide glif üretmiyor, Faz 0).
    let text_system: Arc<dyn gpui_kit::PlatformTextSystem> = Arc::new(gpui_wgpu::CosmicTextSystem::new(p2_ui::tokens::FONT));
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(gpui_kit::assets::Assets), || {
        Ok(Some(Box::new(gpui_wgpu::WgpuHeadlessRenderer::new()?) as Box<dyn PlatformHeadlessRenderer>))
    });
    cx.update(|cx| {
        gpui_kit::init(cx);
        p2_ui::theme::apply(if light { p2_ui::Palette::p2_light() } else { p2_ui::Palette::p2_dark() }, cx);
    });
    pick_font(&mut cx);
    cx
}

fn build_host(init: Init, session: Option<Session>, w: u32, h: u32, pid: u32) -> anyhow::Result<(Host, Receiver<DataMsg>)> {
    let t0 = Instant::now();
    let mut cx = new_cx(init.light);
    let scale = init.look.scale;
    let (lw, lh) = logical(w, h, scale);
    let path = (!init.layout_path.is_empty()).then(|| std::path::PathBuf::from(&init.layout_path));
    let store = wm::Store::load(&init.project, path, lw as f32, lh as f32, init.look.opacity);
    let hotkeys: Vec<_> = if init.hotkeys.is_empty() { vec![HotkeySpec::PANEL_DEFAULT] } else { init.hotkeys.clone() };
    let mut st = State::new(lw as f32, lh as f32, init.project.clone(), init.project_name.clone(), init.user_id.clone(), init.user_name.clone(), panel_keys(&hotkeys), store);
    let shot = hotkey_keys(&hotkeys, 2, None);
    if !shot.is_empty() {
        st.shot_keys = shot;
    }
    let mute = hotkey_keys(&hotkeys, 3, None);
    if !mute.is_empty() {
        st.mute_keys = mute;
    }
    st.look = init.look.clone();
    st.installed = init.installed.clone();
    let window = Host::open_window(&mut cx, st)?;
    log(format!("hazır: {w}x{h}, GPUI {} ms, yazı {}", t0.elapsed().as_millis(), ui::font()));
    let l = &init.look;
    log(format!(
        "görünüm: ölçek={:.2} saydamlık={:.2} hud={} yer={} ayrıntı={} köşe={} hareket-azalt={} (mantıksal {lw}x{lh})",
        l.scale, l.opacity, l.hud, l.hud_position, l.hud_details, l.toast_corner, l.reduce_motion
    ));
    let (data_tx, data_rx) = channel();
    let ping_on = Arc::new(std::sync::atomic::AtomicBool::new(init.look.hud && session.is_some()));
    if session.is_some() {
        data::start_ping(init.api.clone(), ping_on.clone(), data_tx.clone());
    }
    let data = Data { api: init.api.clone(), org: init.org.clone(), project: init.project.clone(), tx: data_tx };
    let perf = session.as_ref().map(|s| perf::Perf::new(pid, s.stats().frames));
    let host = Host {
        session,
        init,
        data,
        cx,
        window,
        w,
        h,
        prev: vec![0; (w * h * 4) as usize],
        next: vec![0; (w * h * 4) as usize],
        frames_written: 0,
        dirty: true,
        last_render: Instant::now(),
        last_input: Instant::now() - Duration::from_secs(10),
        last_mask: u32::MAX,
        keys: keys::InputState::default(),
        perf,
        shot: None,
        loaded_at: Default::default(),
        scale,
        animate_until: None,
        ping_on,
    };
    Ok((host, data_rx))
}

/// `--render`: oyun ve DLL olmadan çizim (parite aracı). Sabit performans örneği, ping 31 ms.
fn render_mode(out: String, size: (u32, u32), steps: String, layout: Option<String>, commands: Receiver<ToHost>) -> anyhow::Result<()> {
    let mut init = wait_init(&commands)?;
    if let Some(src) = layout {
        // Verilen düzen geçici bir dosyaya kopyalanır (asıl profile dokunulmaz).
        let tmp = std::env::temp_dir().join(format!("p2-overlay-render-{}.json", std::process::id()));
        let raw = std::fs::read_to_string(&src)?;
        let layout: serde_json::Value = serde_json::from_str(&raw)?;
        std::fs::write(&tmp, serde_json::to_vec(&serde_json::json!({ wm::storage_key(&init.project): layout }))?)?;
        init.layout_path = tmp.display().to_string();
    } else {
        init.layout_path.clear();
    }
    let (mut host, data_rx) = build_host(init, None, size.0, size.1, 0)?;
    host.update(|v, _, _| v.st.ping = Some(31));
    let fixed = perf::Sample { fps: 144., game_cpu: Some(14.), game_ram_mb: Some(2300), sys_cpu: Some(23.), sys_ram_used_mb: 11800, sys_ram_total_mb: 32768 };
    let mut pilot = Autopilot::new(&format!("{steps};wait:1500;dump:{out}"), Duration::from_millis(200));
    let mut sec = Instant::now() - Duration::from_secs(2);
    let mut seconds = 0;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        while let Ok(msg) = data_rx.try_recv() {
            host.on_data(msg);
        }
        while let Ok(cmd) = commands.try_recv() {
            if !host.on_command(cmd) {
                return Ok(());
            }
        }
        if sec.elapsed() >= Duration::from_millis(950) && seconds < 3 {
            sec = Instant::now();
            seconds += 1;
            host.second(Some(&fixed));
        }
        // İlk 3 örnek gelene dek adımlar beklemez ama `dump` en az 3 sn sonra.
        if seconds >= 3 || pilot.steps.len() > 1 {
            host.autopilot(&mut pilot, None);
        }
        host.update(|v, _, cx| {
            v.tick(cx);
        });
        host.run_cmds();
        if pilot.steps.is_empty() && Instant::now() >= pilot.next_at {
            break;
        }
        let _ = host.draw();
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut pid, mut render, mut size_arg, mut steps, mut layout) = (0u32, None, (1280u32, 720u32), String::new(), None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--pid" => pid = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--render" => render = args.next(),
            "--size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))) {
                    size_arg = (w, h);
                }
            }
            "--steps" => steps = args.next().unwrap_or_default(),
            "--layout" => layout = args.next(),
            _ => {}
        }
    }
    let commands = stdin_reader();
    if let Some(out) = render {
        return render_mode(out, size_arg, steps, layout, commands);
    }
    let mut init = wait_init(&commands)?;
    if pid == 0 {
        pid = init.pid;
    }
    anyhow::ensure!(pid > 0, "oyun pid'i yok (--pid ya da init.pid)");
    let hotkeys: Vec<_> = if init.hotkeys.is_empty() { vec![HotkeySpec::PANEL_DEFAULT] } else { init.hotkeys.clone() };
    let session = Session::create(pid, &hotkeys.iter().map(HotkeySpec::to_ipc).collect::<Vec<_>>())?;
    emit(FromHost::Session);
    let game = Proc::open(pid);

    // DLL bağlanıp oyun kare çizene kadar bekle (bu sırada gelen boyut mesajı saklanır).
    let started = Instant::now();
    let mut backbuffer: Option<(u32, u32)> = None;
    let mut early = Vec::new();
    loop {
        while let Ok(msg) = session.input.try_recv() {
            if msg.kind == input::RESIZE {
                backbuffer = Some((msg.x.max(1) as u32, msg.y.max(1) as u32));
            } else {
                early.push(msg);
            }
        }
        if session.stats().frames > 0 && find_window(pid).is_some() {
            break;
        }
        if started.elapsed().as_millis() % 5000 < 100 {
            let st = session.stats();
            log(format!("bekleniyor: frames={} pencere={}", st.frames, find_window(pid).is_some()));
        }
        if game.as_ref().is_some_and(Proc::exited) {
            log("oyun overlay hazır olmadan kapandı");
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(90) {
            log("90 sn içinde kare yok (D3D12/Vulkan/OpenGL olabilir); overlay kapatılıyor");
            return Ok(());
        }
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(ToHost::Stop) => return Ok(()),
            // Oyun hazır olmadan değişen görünüm ayarı kaybolmasın.
            Ok(ToHost::Look(look)) => init.look = look.clamped(),
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
    let (hwnd, cw, ch) = find_window(pid).unwrap();
    let (w, h) = backbuffer.unwrap_or((cw, ch));
    let (w, h) = (w.clamp(320, p2_overlay_ipc::MAX_W), h.clamp(240, p2_overlay_ipc::MAX_H));

    let (mut host, data_rx) = build_host(init, Some(session), w, h, pid)?;
    emit(FromHost::Ready { width: w, height: h });
    let name = host.init.project_name.clone();
    let keys = panel_keys(&hotkeys);
    host.toast(Toast { kind: "info".into(), title: format!("{} başlatıldı", if name.is_empty() { "Oyun" } else { &name }), text: "P2 overlay hazır".into(), keys, duration_ms: Some(9000), ..Default::default() });
    for msg in early {
        host.on_input(msg);
    }
    let mut pilot = Autopilot::from_env();
    let mut perf_at = Instant::now();

    loop {
        let tick = Instant::now();
        loop {
            match commands.try_recv() {
                Ok(cmd) => {
                    if !host.on_command(cmd) {
                        log("durduruldu");
                        host.update(|v, _, _| v.st.store.flush());
                        return Ok(());
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
        while let Some(msg) = host.session.as_ref().and_then(|s| s.input.try_recv().ok()) {
            host.on_input(msg);
        }
        while let Ok(msg) = data_rx.try_recv() {
            host.on_data(msg);
        }
        if let Some(p) = pilot.as_mut() {
            host.autopilot(p, Some(hwnd));
        }
        if let Some((_, at)) = host.shot {
            if at.elapsed() > Duration::from_millis(1500) {
                host.shot = None;
                log("ekran görüntüsü zaman aşımı; doğrudan okunuyor");
                host.finish_shot();
            }
        }
        if game.as_ref().is_some_and(Proc::exited) {
            log("oyun kapandı");
            host.update(|v, _, _| v.st.store.flush());
            return Ok(());
        }
        if perf_at.elapsed() >= Duration::from_secs(1) {
            perf_at = Instant::now();
            host.second(None);
        }
        // Zamanlayıcılar: not otomatik kaydı, düzen yazımı (300 ms), bildirim çıkışları.
        let (busy, toasts, pinned, hud, visible) = host
            .update(|v, _, cx| {
                let a = v.tick(cx);
                v.st.store.flush_due();
                let b = v.st.toasts.animating();
                if v.st.expire() {
                    cx.notify();
                }
                (a || b, !v.st.toasts.is_empty(), !v.st.store.pinned_windows().is_empty(), v.st.hud(), v.st.visible)
            })
            .unwrap_or_default();
        host.run_cmds();
        if busy {
            host.animate_until = Some(Instant::now() + Duration::from_millis(100));
        }

        // Çizim maskesi: panel kapalıyken DLL yalnız bu katmanları çizer.
        let mask = if toasts { DRAW_TOAST } else { 0 } | if hud { DRAW_HUD } else { 0 } | if pinned { DRAW_PINNED } else { 0 };
        if mask != host.last_mask {
            host.last_mask = mask;
            if let Some(s) = &host.session {
                s.set_draw_mask(mask);
            }
        }

        // Çizim: değişiklik varsa; panel açıkken etkileşimden sonra 30 fps (hover, imleç, sürükleme),
        // aksi halde 2 Hz (imleç yanıp sönmesi). Hiçbir katman görünmüyorsa hiç çizilmez.
        let shown = visible || toasts || hud || pinned;
        host.ping_on.store(hud, std::sync::atomic::Ordering::Relaxed);
        let animating = host.animate_until.is_some_and(|t| Instant::now() < t);
        if !animating {
            host.animate_until = None;
        }
        let interactive = (visible && host.last_input.elapsed() < Duration::from_millis(800)) || animating;
        let due = host.dirty || (visible && host.last_render.elapsed() >= Duration::from_millis(500)) || (interactive && host.last_render.elapsed() >= FRAME);
        if due && (shown || host.dirty) {
            host.dirty = false;
            if let Err(e) = host.render() {
                log(format!("çizim hatası: {e}"));
            }
        }
        let spent = tick.elapsed();
        let budget = if interactive { FRAME } else { Duration::from_millis(16) };
        if spent < budget {
            std::thread::sleep(budget - spent);
        }
    }
}
