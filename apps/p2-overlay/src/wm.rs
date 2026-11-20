//! Overlay pencere düzeni — Electron `src/overlay/layoutStore.ts` + `tools/toolMeta.ts` birebir.
//!
//! Proje başına saklanır; Electron'da `localStorage['p2:overlay:layout:<proje>']`, burada profil
//! klasöründeki `overlay-layout.json` içinde AYNI anahtar ve AYNI JSON biçimiyle (`{ "p2:overlay:layout:<id>": Layout }`).
//! Sürükleme/boyutlandırma sırasında depoya yazılmaz; bırakınca tek `move_window`/`resize_window` olur.
//! Görünüm alanı değişince pencereler kullanıcının son ayarladığı `base` dikdörtgeninden ölçeklenir.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const TOOLBAR_W: f32 = 56.;
pub const EDGE_RIGHT: f32 = 48.;
pub const EDGE_BOTTOM: f32 = 32.;
pub const PIN_OPACITY_MIN: f32 = 0.2;
pub const PIN_OPACITY_MAX: f32 = 1.0;
pub const REFERENCE: (f32, f32) = (1920., 1080.);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    Voice,
    Team,
    Chat,
    Notes,
    Tasks,
    Capture,
    Summary,
    Browser,
    Perf,
    Library,
    /// "Yeni görev" formu: Görevler penceresinden açılan ayrı, taşınabilir pencere.
    NewTask,
}

pub struct ToolMeta {
    pub id: ToolId,
    pub key: &'static str,
    /// i18n anahtarı (`p2_i18n::t` ile gösterilir).
    pub title: &'static str,
    pub bounds: Rect,
    pub min: (f32, f32),
    pub pinnable: bool,
    pub ghost_compact: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

/// toolMeta.ts `TOOL_ORDER` + `TOOL_META` (1920×1080 referansına göre).
pub const TOOLS: [ToolMeta; 11] = [
    ToolMeta { id: ToolId::Voice, key: "voice", title: "ovr.tool.voice", bounds: r(1540., 24., 356., 420.), min: (260., 200.), pinnable: true, ghost_compact: true },
    ToolMeta { id: ToolId::Team, key: "team", title: "ovr.tool.team", bounds: r(1540., 468., 356., 420.), min: (260., 200.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Chat, key: "chat", title: "ovr.tool.chat", bounds: r(420., 80., 720., 540.), min: (360., 260.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Notes, key: "notes", title: "ovr.tool.notes", bounds: r(1100., 120., 420., 520.), min: (280., 220.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Tasks, key: "tasks", title: "ovr.tool.tasks", bounds: r(300., 120., 640., 480.), min: (360., 260.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Capture, key: "capture", title: "ovr.tool.capture", bounds: r(360., 140., 640., 460.), min: (320., 240.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Summary, key: "summary", title: "ovr.tool.summary", bounds: r(80., 60., 420., 560.), min: (300., 240.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Browser, key: "browser", title: "ovr.tool.browser", bounds: r(500., 80., 820., 560.), min: (420., 300.), pinnable: false, ghost_compact: false },
    ToolMeta { id: ToolId::Perf, key: "perf", title: "ovr.tool.perf", bounds: r(80., 560., 400., 500.), min: (260., 200.), pinnable: true, ghost_compact: false },
    ToolMeta { id: ToolId::Library, key: "library", title: "ovr.tool.library", bounds: r(80., 60., 380., 440.), min: (280., 220.), pinnable: false, ghost_compact: false },
    ToolMeta { id: ToolId::NewTask, key: "newtask", title: "ovr.tasks.new", bounds: r(660., 200., 600., 480.), min: (420., 380.), pinnable: false, ghost_compact: false },
];

pub fn meta(id: ToolId) -> &'static ToolMeta {
    TOOLS.iter().find(|t| t.id == id).unwrap()
}

/// 'tasks:abc' → Tasks; bilinmeyen kimlik → None.
pub fn base_tool(id: &str) -> Option<ToolId> {
    let key = id.split(':').next().unwrap_or("");
    TOOLS.iter().find(|t| t.key == key).map(|t| t.id)
}
pub fn instance_of(id: &str) -> Option<&str> {
    id.find(':').filter(|i| *i > 0).map(|i| &id[i + 1..])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WinBase {
    pub vw: f32,
    pub vh: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WinState {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub open: bool,
    pub minimized: bool,
    pub pinned: bool,
    pub pin_opacity: f32,
    pub z: f32,
    #[serde(default)]
    pub restore: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<WinBase>,
}

impl WinState {
    pub fn rect(&self) -> Rect {
        r(self.x, self.y, self.w, self.h)
    }
    pub fn maximized(&self) -> bool {
        self.restore.is_some()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Viewport {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub version: u32,
    pub viewport: Viewport,
    pub focus: Option<String>,
    pub windows: BTreeMap<String, WinState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast_corner: Option<String>,
}

fn clamp(v: f32, min: f32, max: f32) -> f32 {
    v.min(max).max(min)
}
fn round(v: f32) -> f32 {
    // JS Math.round (yarım yukarı)
    (v + 0.5).floor()
}

pub struct Store {
    pub project: String,
    pub layout: Layout,
    pub default_pin_opacity: f32,
    path: Option<PathBuf>,
    /// Kalıcı yazım bekliyor (300 ms gecikme: `flush_due`).
    dirty_at: Option<std::time::Instant>,
}

pub fn storage_key(project: &str) -> String {
    format!("p2:overlay:layout:{project}")
}

impl Store {
    pub fn empty(w: f32, h: f32) -> Layout {
        Layout { version: 1, viewport: Viewport { w, h }, focus: None, windows: BTreeMap::new(), toast_corner: None }
    }

    /// initLayout: kayıtlı düzen okunur (yoksa boş), görünüm alanına oranlanır.
    pub fn load(project: &str, path: Option<PathBuf>, w: f32, h: f32, default_pin_opacity: f32) -> Self {
        let project = if project.is_empty() { "demo".to_string() } else { project.to_string() };
        let safe = (w.max(320.), h.max(240.));
        let mut store = Store { project: project.clone(), layout: Self::empty(safe.0, safe.1), default_pin_opacity: clamp(default_pin_opacity, PIN_OPACITY_MIN, PIN_OPACITY_MAX), path, dirty_at: None };
        let raw = store.path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        if let Some(value) = raw.as_ref().and_then(|all| all.get(storage_key(&project))) {
            if let Some(layout) = store.sanitize(value, safe) {
                store.layout = layout;
            }
        }
        store
    }

    fn clamp_rect(&self, rect: Rect, id: &str, vp: Viewport) -> Rect {
        let min = meta(base_tool(id).unwrap_or(ToolId::Notes)).min;
        let w = clamp(round(rect.w), min.0.min(vp.w - TOOLBAR_W), min.0.max(vp.w - TOOLBAR_W));
        let h = clamp(round(rect.h), min.1.min(vp.h), min.1.max(vp.h));
        let x = clamp(round(rect.x), TOOLBAR_W, TOOLBAR_W.max(vp.w - EDGE_RIGHT));
        let y = clamp(round(rect.y), 0., 0f32.max(vp.h - EDGE_BOTTOM));
        r(x, y, w, h)
    }
    fn clamp_loose(rect: Rect, vp: Viewport) -> Rect {
        r(clamp(round(rect.x), TOOLBAR_W, TOOLBAR_W.max(vp.w - EDGE_RIGHT)), clamp(round(rect.y), 0., 0f32.max(vp.h - EDGE_BOTTOM)), round(rect.w).max(1.), round(rect.h).max(1.))
    }
    fn scale_rect(rect: Rect, from: Viewport, to: Viewport) -> Rect {
        let (sx, sy) = (to.w / from.w, to.h / from.h);
        r(rect.x * sx, rect.y * sy, rect.w * sx, rect.h * sy)
    }
    fn base_of(rect: Rect, vp: Viewport) -> WinBase {
        WinBase { vw: vp.w, vh: vp.h, x: rect.x, y: rect.y, w: rect.w, h: rect.h }
    }
    fn from_base(base: WinBase, to: Viewport) -> Rect {
        if base.vw == to.w && base.vh == to.h {
            return r(base.x, base.y, base.w, base.h);
        }
        Self::clamp_loose(Self::scale_rect(r(base.x, base.y, base.w, base.h), Viewport { w: base.vw, h: base.vh }, to), to)
    }
    fn default_window(&self, id: &str) -> WinState {
        let tool = base_tool(id).unwrap_or(ToolId::Notes);
        let vp = self.layout.viewport;
        let rect = self.clamp_rect(Self::scale_rect(meta(tool).bounds, Viewport { w: REFERENCE.0, h: REFERENCE.1 }, vp), id, vp);
        WinState { id: id.into(), x: rect.x, y: rect.y, w: rect.w, h: rect.h, open: false, minimized: false, pinned: false, pin_opacity: self.default_pin_opacity, z: 0., restore: None, base: Some(Self::base_of(rect, vp)) }
    }
    fn max_z(&self) -> f32 {
        self.layout.windows.values().map(|w| w.z).fold(0., f32::max)
    }

    fn sanitize(&self, raw: &serde_json::Value, viewport: (f32, f32)) -> Option<Layout> {
        let vp = Viewport { w: viewport.0, h: viewport.1 };
        if raw.get("version").and_then(|v| v.as_u64()) != Some(1) {
            return None;
        }
        let windows = raw.get("windows")?.as_object()?;
        let num = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_f64()).unwrap_or(0.) as f32;
        let from = raw.get("viewport").map(|v| Viewport { w: num(v.get("w")), h: num(v.get("h")) }).filter(|v| v.w > 0. && v.h > 0.).unwrap_or(vp);
        let mut out = BTreeMap::new();
        for (id, win) in windows {
            let Some(tool) = base_tool(id) else { continue };
            if !win.is_object() {
                continue;
            }
            let saved = win.get("base").filter(|b| num(b.get("vw")) > 0. && num(b.get("vh")) > 0.).map(|b| WinBase { vw: num(b.get("vw")), vh: num(b.get("vh")), x: num(b.get("x")), y: num(b.get("y")), w: num(b.get("w")), h: num(b.get("h")) });
            let saved = saved.unwrap_or(WinBase { vw: from.w, vh: from.h, x: num(win.get("x")), y: num(win.get("y")), w: num(win.get("w")), h: num(win.get("h")) });
            let base = if saved.w > 0. && saved.h > 0. { saved } else { Self::base_of(meta(tool).bounds, Viewport { w: REFERENCE.0, h: REFERENCE.1 }) };
            let rect = Self::from_base(base, vp);
            let flag = |k: &str| win.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            let opacity = num(win.get("pinOpacity"));
            let restore = win.get("restore").filter(|v| v.is_object()).map(|v| Self::clamp_loose(Self::scale_rect(r(num(v.get("x")), num(v.get("y")), num(v.get("w")), num(v.get("h"))), from, vp), vp));
            out.insert(
                id.clone(),
                WinState {
                    id: id.clone(),
                    x: rect.x,
                    y: rect.y,
                    w: rect.w,
                    h: rect.h,
                    base: Some(base),
                    open: flag("open"),
                    minimized: flag("minimized"),
                    pinned: flag("pinned") && meta(tool).pinnable,
                    pin_opacity: clamp(if opacity > 0. { opacity } else { self.default_pin_opacity }, PIN_OPACITY_MIN, PIN_OPACITY_MAX),
                    z: num(win.get("z")).max(0.),
                    restore,
                },
            );
        }
        let focus = raw.get("focus").and_then(|v| v.as_str()).filter(|f| out.contains_key(*f)).map(String::from);
        let toast_corner = raw.get("toastCorner").and_then(|v| v.as_str()).map(String::from);
        Some(Layout { version: 1, viewport: vp, focus, windows: out, toast_corner })
    }

    fn set(&mut self, next: Layout) {
        self.layout = next;
        self.dirty_at = Some(std::time::Instant::now());
    }
    fn patch(&mut self, id: &str, change: impl FnOnce(&mut WinState)) {
        let mut win = self.layout.windows.get(id).cloned().unwrap_or_else(|| self.default_window(id));
        change(&mut win);
        let mut next = self.layout.clone();
        next.windows.insert(id.into(), win);
        self.set(next);
    }
    fn placed(&self, id: &str, rect: Rect) -> (Rect, WinBase) {
        let next = self.clamp_rect(rect, id, self.layout.viewport);
        (next, Self::base_of(next, self.layout.viewport))
    }

    /// 300 ms sonra yazılır (Electron `schedulePersist`); panel kapanınca hemen (`flush`).
    pub fn flush_due(&mut self) {
        if self.dirty_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(300)) {
            self.flush();
        }
    }
    pub fn flush(&mut self) {
        if self.dirty_at.take().is_none() {
            return;
        }
        let Some(path) = &self.path else { return };
        let mut all = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&s).ok()).unwrap_or_default();
        if let Ok(v) = serde_json::to_value(&self.layout) {
            all.insert(storage_key(&self.project), v);
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&all).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    // ---- Okuma ----
    pub fn win(&self, id: &str) -> Option<&WinState> {
        self.layout.windows.get(id)
    }
    /// Açık pencereler z sırasıyla (çizim sırası).
    pub fn open_windows(&self) -> Vec<&WinState> {
        let mut v: Vec<&WinState> = self.layout.windows.values().filter(|w| w.open).collect();
        v.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
    pub fn pinned_windows(&self) -> Vec<&WinState> {
        self.open_windows().into_iter().filter(|w| w.pinned).collect()
    }

    // ---- Eylemler ----
    pub fn open_window(&mut self, id: &str) {
        if base_tool(id).is_none() {
            return;
        }
        let z = self.max_z() + 1.;
        let mut win = self.layout.windows.get(id).cloned().unwrap_or_else(|| self.default_window(id));
        win.open = true;
        win.minimized = false;
        win.z = z;
        let mut next = self.layout.clone();
        next.focus = Some(id.into());
        next.windows.insert(id.into(), win);
        self.set(next);
    }
    pub fn open_instance(&mut self, tool: ToolId, instance: &str) {
        let id = format!("{}:{instance}", meta(tool).key);
        if !self.layout.windows.contains_key(&id) {
            let fallback = self.default_window(&id);
            let rect = match self.layout.windows.get(meta(tool).key) {
                Some(p) => self.clamp_rect(r(p.x + 36., p.y + 36., p.w, p.h), &id, self.layout.viewport),
                None => fallback.rect(),
            };
            let win = WinState { x: rect.x, y: rect.y, w: rect.w, h: rect.h, base: Some(Self::base_of(rect, self.layout.viewport)), ..fallback };
            let mut next = self.layout.clone();
            next.windows.insert(id.clone(), win);
            self.set(next);
        }
        self.open_window(&id);
    }
    fn top_focus(&self, windows: &BTreeMap<String, WinState>, except: Option<&str>, skip_min: bool) -> Option<String> {
        let mut v: Vec<&WinState> = windows.values().filter(|w| w.open && Some(w.id.as_str()) != except && !(skip_min && w.minimized)).collect();
        v.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));
        v.last().map(|w| w.id.clone())
    }
    pub fn close_window(&mut self, id: &str) {
        let Some(win) = self.layout.windows.get(id).cloned() else { return };
        if !win.open {
            return;
        }
        let mut w = WinState { open: false, minimized: false, restore: None, ..win.clone() };
        if let Some(rest) = win.restore {
            w.x = rest.x;
            w.y = rest.y;
            w.w = rest.w;
            w.h = rest.h;
            w.base = Some(Self::base_of(rest, self.layout.viewport));
        }
        let mut next = self.layout.clone();
        next.windows.insert(id.into(), w);
        if next.focus.as_deref() == Some(id) {
            next.focus = self.top_focus(&next.windows, None, false);
        }
        self.set(next);
    }
    pub fn forget_window(&mut self, id: &str) {
        if instance_of(id).is_none() || !self.layout.windows.contains_key(id) {
            return;
        }
        let mut next = self.layout.clone();
        next.windows.remove(id);
        if next.focus.as_deref() == Some(id) {
            next.focus = self.top_focus(&next.windows, None, false);
        }
        self.set(next);
    }
    pub fn toggle_window(&mut self, id: &str) {
        match self.layout.windows.get(id) {
            Some(w) if w.open => {
                if w.minimized || self.layout.focus.as_deref() != Some(id) {
                    self.open_window(id)
                } else {
                    self.close_window(id)
                }
            }
            _ => self.open_window(id),
        }
    }
    pub fn focus_window(&mut self, id: &str) {
        let Some(win) = self.layout.windows.get(id) else { return };
        if !win.open || (self.layout.focus.as_deref() == Some(id) && win.z == self.max_z()) {
            return;
        }
        let z = self.max_z() + 1.;
        let mut next = self.layout.clone();
        next.focus = Some(id.into());
        next.windows.get_mut(id).unwrap().z = z;
        self.set(next);
    }
    pub fn move_window(&mut self, id: &str, x: f32, y: f32) {
        let Some(win) = self.layout.windows.get(id) else { return };
        let (rect, base) = self.placed(id, r(x, y, win.w, win.h));
        self.patch(id, |w| {
            (w.x, w.y, w.w, w.h, w.base, w.restore) = (rect.x, rect.y, rect.w, rect.h, Some(base), None);
        });
    }
    pub fn resize_window(&mut self, id: &str, rect: Rect) {
        if !self.layout.windows.contains_key(id) {
            return;
        }
        let (rect, base) = self.placed(id, rect);
        self.patch(id, |w| {
            (w.x, w.y, w.w, w.h, w.base, w.restore) = (rect.x, rect.y, rect.w, rect.h, Some(base), None);
        });
    }
    pub fn set_pinned(&mut self, id: &str, pinned: bool) {
        let Some(tool) = base_tool(id) else { return };
        if pinned && !meta(tool).pinnable {
            return;
        }
        self.patch(id, |w| w.pinned = pinned);
    }
    pub fn set_pin_opacity(&mut self, id: &str, opacity: f32) {
        let v = clamp(round(opacity * 100.) / 100., PIN_OPACITY_MIN, PIN_OPACITY_MAX);
        self.patch(id, |w| w.pin_opacity = v);
    }
    pub fn minimize_window(&mut self, id: &str, minimized: bool) {
        let Some(win) = self.layout.windows.get(id).cloned() else { return };
        if !win.open {
            return;
        }
        let focus = if minimized && self.layout.focus.as_deref() == Some(id) {
            self.top_focus(&self.layout.windows, Some(id), true)
        } else if minimized {
            self.layout.focus.clone()
        } else {
            Some(id.into())
        };
        let z = if minimized { win.z } else { self.max_z() + 1. };
        let mut next = self.layout.clone();
        next.focus = focus;
        let w = next.windows.get_mut(id).unwrap();
        w.minimized = minimized;
        w.z = z;
        self.set(next);
    }
    pub fn maximize_window(&mut self, id: &str) {
        let Some(win) = self.layout.windows.get(id).cloned() else { return };
        if !win.open {
            return;
        }
        if let Some(rest) = win.restore {
            let (rect, base) = self.placed(id, rest);
            self.patch(id, |w| {
                (w.x, w.y, w.w, w.h, w.base, w.restore, w.minimized) = (rect.x, rect.y, rect.w, rect.h, Some(base), None, false);
            });
            return;
        }
        let vp = self.layout.viewport;
        self.patch(id, |w| {
            w.restore = Some(win.rect());
            (w.x, w.y, w.w, w.h, w.minimized) = (TOOLBAR_W, 0., vp.w - TOOLBAR_W, vp.h, false);
        });
    }
    pub fn apply_viewport(&mut self, w: f32, h: f32) {
        let (w, h) = (round(w).max(320.), round(h).max(240.));
        let from = self.layout.viewport;
        if from.w == w && from.h == h {
            return;
        }
        let to = Viewport { w, h };
        let mut next = self.layout.clone();
        for win in next.windows.values_mut() {
            let base = win.base.unwrap_or(Self::base_of(win.rect(), from));
            let rect = if win.restore.is_some() { r(TOOLBAR_W, 0., w - TOOLBAR_W, h) } else { Self::from_base(base, to) };
            (win.x, win.y, win.w, win.h, win.base) = (rect.x, rect.y, rect.w, rect.h, Some(base));
            win.restore = win.restore.map(|rest| Self::clamp_loose(Self::scale_rect(rest, from, to), to));
        }
        next.viewport = to;
        self.set(next);
    }
}

// ---- Yapışma (OverlayWindow.tsx snapPosition / snapZone) ----
pub const SNAP_PX: f32 = 12.;
pub const ZONE_PX: f32 = 10.;

pub fn snap_position(store: &Store, id: &str, nx: f32, ny: f32, w: f32, h: f32) -> (f32, f32) {
    let (vw, vh) = (store.layout.viewport.w, store.layout.viewport.h);
    let (mut x, mut y) = (nx, ny);
    let mut ex = vec![TOOLBAR_W, vw - w];
    let mut ey = vec![0., vh - h];
    for o in store.open_windows() {
        if o.id == id || o.minimized {
            continue;
        }
        ex.extend([o.x, o.x + o.w, o.x - w, o.x + o.w - w]);
        ey.extend([o.y, o.y + o.h, o.y - h, o.y + o.h - h]);
    }
    if let Some(e) = ex.iter().find(|e| (x - **e).abs() <= SNAP_PX) {
        x = *e;
    }
    if let Some(e) = ey.iter().find(|e| (y - **e).abs() <= SNAP_PX) {
        y = *e;
    }
    (x, y)
}

pub fn snap_zone(store: &Store, px_: f32, py: f32) -> Option<Rect> {
    let (vw, vh) = (store.layout.viewport.w, store.layout.viewport.h);
    let half = round((vw - TOOLBAR_W) / 2.);
    if py <= ZONE_PX {
        Some(r(TOOLBAR_W, 0., vw - TOOLBAR_W, vh))
    } else if px_ <= TOOLBAR_W + ZONE_PX {
        Some(r(TOOLBAR_W, 0., half, vh))
    } else if px_ >= vw - ZONE_PX {
        Some(r(vw - half, 0., half, vh))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varsayilan_dikdortgen_ve_gidis_donus() {
        let mut s = Store::load("p", None, 1280., 720., 0.92);
        s.open_window("tasks");
        let w = s.win("tasks").unwrap().clone();
        // 300×120 640×480 @1920×1080 → 1280×720
        assert_eq!((w.x, w.y, w.w, w.h), (200., 80., 427., 320.));
        assert!(w.open && s.layout.focus.as_deref() == Some("tasks"));
        s.apply_viewport(1920., 1080.);
        s.apply_viewport(1280., 720.);
        assert_eq!(s.win("tasks").unwrap().rect(), w.rect());
        s.maximize_window("tasks");
        assert_eq!(s.win("tasks").unwrap().rect(), Rect { x: 56., y: 0., w: 1224., h: 720. });
        s.maximize_window("tasks");
        assert_eq!(s.win("tasks").unwrap().rect(), w.rect());
        s.toggle_window("tasks");
        assert!(!s.win("tasks").unwrap().open);
    }

    #[test]
    fn electron_json_okunur() {
        let raw = serde_json::json!({ "version": 1, "viewport": { "w": 1920, "h": 1080 }, "focus": "notes",
            "windows": { "notes": { "id": "notes", "x": 1100, "y": 120, "w": 420, "h": 520, "open": true, "minimized": false, "pinned": true, "pinOpacity": 0.5, "z": 3 }, "bilinmez": {} } });
        let s = Store::load("p", None, 1920., 1080., 0.92);
        let l = s.sanitize(&raw, (1920., 1080.)).unwrap();
        let n = &l.windows["notes"];
        assert!(n.pinned && n.open && (n.pin_opacity - 0.5).abs() < 1e-6 && l.windows.len() == 1);
        assert_eq!(l.focus.as_deref(), Some("notes"));
        let text = serde_json::to_string(&l).unwrap();
        assert!(text.contains("\"pinOpacity\":0.5") && text.contains("\"viewport\""), "{text}");
    }
}
