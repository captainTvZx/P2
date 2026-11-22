//! Oyun içi overlay arayüzü — Electron `src/OverlayApp.tsx` + `src/overlay/wm/*` + `overlay-wm.css` birebir.
//!
//! Panel açık: scrim + sol 56 px dikey araç çubuğu + açık araç pencereleri (sürükle, 8 tutamaçla
//! boyutlandır, küçült/büyüt/kapat, sabitle + saydamlık) + bildirimler + HUD. Panel kapalı: yalnız
//! sabitli pencereler ("hayalet", girdisiz, pencere saydamlığıyla) + bildirimler + HUD.
//! Pencere düzeni `wm::Store` (layoutStore.ts). Araç içerikleri `tools.rs`.
//!
//! Otomatik test koordinatları sabit sayı değil, çizimde ölçülen öğe sınırlarıdır (`OverlayRoot::hit`):
//! `tb:<araç>`, `tb:shot`, `tb:settings`, `tb:close`, `win:<id>:pin|min|max|close|head`, araç
//! öğeleri (`tasks:new`, `task-status:<başlık>`, `opt:<etiket>` …).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, BoxShadow, Bounds, Context, CursorStyle, Entity, FontWeight, Hsla, MouseButton, MouseDownEvent, Pixels, SharedString, Subscription,
    Window, canvas, deferred, div, hsla, point, px,
};
use p2_client_core::{BoardInfo, Member, ProjectNote, Task};
use p2_overlay_ipc::proto::{CallInfo, Installed, Look, Toast};
use p2_ui::components::logo_image;
use p2_ui::controls::{BadgeTone, badge, kicker};
use p2_ui::motion::{Entrance, P2Motion as _};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_MONO, Palette, font_size, radius};

use crate::icons::{self, ic};
use crate::perf::Sample;
use crate::toasts::Toasts;
use crate::wm::{self, Rect, Store, TOOLBAR_W, ToolId, WinState};

/// Overlay yazı ailesi (ana süreçte cosmic-text'in gördüğü Inter adıyla değiştirilir).
pub static FONT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
pub fn font() -> &'static str {
    FONT.get().copied().unwrap_or(p2_ui::tokens::FONT)
}

/// Araç çubuğundaki araçlar (TOOL_LIST, 'library' hariç).
pub const BAR_TOOLS: [ToolId; 9] = [ToolId::Voice, ToolId::Team, ToolId::Chat, ToolId::Notes, ToolId::Tasks, ToolId::Capture, ToolId::Summary, ToolId::Browser, ToolId::Perf];

pub enum Cmd {
    /// Tanılama satırı (host günlüğüne).
    Log(String),
    /// İstemciye eylem (çağrı, ana pencerede aç…).
    Action(String),
    ToastAction { toast_id: String, tag: Option<String>, action_id: String, payload: Option<serde_json::Value> },
    Shot,
    SetVisible(bool),
    /// Araç penceresi açıldı/göründü: verisi (yeniden) çekilsin.
    Load(ToolId),
    CreateTask { board: String, input: serde_json::Value },
    MoveTask { board: String, task: String, from: String, to: String },
    CreateNote,
    SaveNote { id: String, patch: serde_json::Value },
    DeleteNote(String),
    /// Sohbet: kanal seçildi / mesaj gönder.
    ChatSelect(String),
    ChatSend { channel: String, text: String },
    /// Oyun içi tarayıcıya komut (gezinme, fare, klavye).
    Browser(crate::browser::BrowserCmd),
    /// Ayar değişti (Performans penceresindeki anahtarlar): host uygular ve istemciye bildirir.
    Look(Look),
}

pub enum Loadable<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        if let Loadable::Ready(v) = self { Some(v) } else { None }
    }
    pub fn loading(&self) -> bool {
        matches!(self, Loadable::Idle | Loadable::Loading)
    }
}

#[derive(Default)]
pub struct TasksData {
    pub boards: Vec<BoardInfo>,
    pub by_board: Vec<(String, Vec<Task>)>,
}

impl TasksData {
    pub fn all(&self) -> impl Iterator<Item = &Task> {
        self.by_board.iter().flat_map(|(_, t)| t.iter())
    }
    pub fn board_tasks(&self, board: &str) -> Vec<&Task> {
        if board == "*" { self.all().collect() } else { self.by_board.iter().filter(|(b, _)| b == board).flat_map(|(_, t)| t.iter()).collect() }
    }
}

/// Vitrin gönderisi (Oyun özeti).
#[derive(Clone)]
pub struct HubPostView {
    pub author_id: String,
    pub author: String,
    pub release: bool,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone)]
pub struct ShotRec {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub at: Instant,
}

pub struct State {
    pub w: f32,
    pub h: f32,
    pub visible: bool,
    pub project_id: String,
    pub project_name: String,
    pub user_id: String,
    pub user_name: String,
    /// Kısayol tuş çipleri: panel, ekran görüntüsü, sustur.
    pub panel_keys: Vec<String>,
    pub shot_keys: Vec<String>,
    pub mute_keys: Vec<String>,
    pub toasts: Toasts,
    pub call: CallInfo,
    pub perf: Sample,
    /// Saniyelik FPS geçmişi (tam sayı, perfStore.ts gibi en çok 60).
    pub history: Vec<f32>,
    pub look: Look,
    pub ping: Option<u32>,
    pub loss: Option<f32>,
    pub store: Store,
    pub tasks: Loadable<TasksData>,
    pub notes: Loadable<Vec<ProjectNote>>,
    pub saving_notes: Vec<String>,
    pub team: Loadable<Vec<Member>>,
    pub voice_channels: Vec<(String, String)>,
    /// Sohbet: yazılı kanallar, seçili kanal, mesajları.
    pub chat_channels: Vec<(String, String)>,
    pub chat_sel: Option<String>,
    pub chat: Loadable<Vec<crate::data::ChatLine>>,
    pub live_release: Option<(String, String, String)>,
    pub hub: Option<(u64, u64, Vec<HubPostView>)>,
    pub installed: Option<Installed>,
    pub shots: Vec<ShotRec>,
    /// Görevler penceresindeki geçici uyarı/hata (pencere içi bildirim kartına da gider).
    pub flash: Option<(String, bool, Instant)>,
}

impl State {
    pub fn new(w: f32, h: f32, project_id: String, project_name: String, user_id: String, user_name: String, panel_keys: Vec<String>, store: Store) -> Self {
        Self {
            w,
            h,
            visible: false,
            project_id,
            project_name,
            user_id,
            user_name,
            panel_keys,
            shot_keys: vec!["F12".into()],
            mute_keys: vec!["Shift".into(), "M".into()],
            toasts: Toasts::default(),
            call: CallInfo::default(),
            perf: Sample::default(),
            history: Vec::new(),
            look: Look::default(),
            ping: None,
            loss: None,
            store,
            tasks: Loadable::Idle,
            notes: Loadable::Idle,
            saving_notes: Vec::new(),
            team: Loadable::Idle,
            voice_channels: Vec::new(),
            chat_channels: Vec::new(),
            chat_sel: None,
            chat: Loadable::Idle,
            live_release: None,
            hub: None,
            installed: None,
            shots: Vec::new(),
            flash: None,
        }
    }

    pub fn hud(&self) -> bool {
        self.look.hud
    }

    /// Süresi dolanlar; değişiklik varsa true.
    pub fn expire(&mut self) -> bool {
        let reduce = self.look.reduce_motion;
        let a = self.toasts.tick(reduce);
        let gone = self.flash.as_ref().is_some_and(|f| f.2.elapsed() > Duration::from_secs(5));
        if gone {
            self.flash = None;
        }
        a || gone
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Edge {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}
const EDGES: [Edge; 8] = [Edge::N, Edge::S, Edge::E, Edge::W, Edge::NE, Edge::NW, Edge::SE, Edge::SW];

pub struct Drag {
    pub id: String,
    pub resize: Option<Edge>,
    pub start: (f32, f32),
    pub rect: Rect,
    pub cur: Rect,
    pub zone: Option<Rect>,
    pub moved: bool,
}

/// `<select>` yerine DOM içi liste (selectShim.ts `.ow-select-pop`).
pub struct SelectPop {
    pub key: String,
    pub options: Vec<(String, String)>,
    pub selected: String,
    pub anchor: Bounds<Pixels>,
}

pub type Measured = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

pub struct OverlayRoot {
    pub st: State,
    pub pending: Vec<Cmd>,
    pub drag: Option<Drag>,
    pub tip: Option<usize>,
    pub pin_menu: Option<String>,
    pub select: Option<SelectPop>,
    /// Klavye odağı olan pencere (Esc onu kapatır); yoksa Esc paneli kapatır.
    pub key_window: Option<String>,
    pub measured: Measured,
    /// Pencere gövdelerinin kaydırma durumu (taşma → kaydırma çubuğu).
    pub scrolls: RefCell<HashMap<String, gpui_kit::ScrollHandle>>,
    /// Açılış animasyonu (ow-enter, p2-pop-elastic) — pencere kimliği → başlangıç sırası.
    pub entering: HashMap<String, u64>,
    /// Durum listesi açılan görev (açılır liste seçimi bu göreve uygulanır).
    pub ui_task_target: Option<String>,
    pub enter_seq: u64,
    pub ui: crate::tools::ToolsUi,
    pub _subs: Vec<Subscription>,
}

pub fn input(window: &mut Window, cx: &mut Context<OverlayRoot>, placeholder: &str) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

impl OverlayRoot {
    pub fn new(st: State, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (ui, subs) = crate::tools::ToolsUi::new(window, cx);
        Self {
            st,
            pending: Vec::new(),
            drag: None,
            tip: None,
            pin_menu: None,
            select: None,
            key_window: None,
            measured: Rc::new(RefCell::new(HashMap::new())),
            scrolls: RefCell::new(HashMap::new()),
            entering: HashMap::new(),
            ui_task_target: None,
            enter_seq: 0,
            ui,
            _subs: subs,
        }
    }

    /// Ölçülen öğe sınırı (otomatik test tıklamaları için; mantıksal koordinat).
    pub fn hit(&self, key: &str) -> Option<(i32, i32)> {
        let map = self.measured.borrow();
        let b = map.get(key).or_else(|| map.iter().find(|(k, _)| k.starts_with(key)).map(|(_, b)| b))?;
        Some((f32::from(b.origin.x + b.size.width / 2.) as i32, f32::from(b.origin.y + b.size.height / 2.) as i32))
    }
    pub fn hit_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.measured.borrow().keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Öğenin sınırını kaydeden görünmez katman (öğe `relative` olmalı).
    pub fn mark(&self, key: impl Into<String>) -> impl IntoElement {
        let key = key.into();
        let map = self.measured.clone();
        canvas(move |bounds, _, _| {
            map.borrow_mut().insert(key, bounds);
        }, |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    // ---- Pencere eylemleri ----
    pub fn open_window(&mut self, id: &str) {
        let was_open = self.st.store.win(id).is_some_and(|w| w.open);
        self.st.store.open_window(id);
        self.on_opened(id, was_open);
    }
    fn on_opened(&mut self, id: &str, was_open: bool) {
        if !was_open && !self.st.look.reduce_motion {
            self.enter_seq += 1;
            self.entering.insert(id.to_string(), self.enter_seq);
        }
        if let Some(tool) = wm::base_tool(id) {
            self.pending.push(Cmd::Load(tool));
        }
    }
    pub fn toggle_window(&mut self, id: &str) {
        let was_open = self.st.store.win(id).is_some_and(|w| w.open);
        self.st.store.toggle_window(id);
        let open = self.st.store.win(id).is_some_and(|w| w.open);
        if open {
            self.on_opened(id, was_open);
        } else {
            self.after_close(id);
        }
    }
    pub fn close_window(&mut self, id: &str) {
        self.st.store.close_window(id);
        if wm::instance_of(id).is_some() {
            self.st.store.forget_window(id);
        }
        self.after_close(id);
    }
    fn after_close(&mut self, id: &str) {
        if id == "newtask" {
            self.ui.creating = false;
        }
        if self.key_window.as_deref() == Some(id) {
            self.key_window = None;
        }
        if self.pin_menu.as_deref() == Some(id) {
            self.pin_menu = None;
        }
        self.entering.remove(id);
    }

    /// Panel açıldı: görünür pencerelerin verisi tazelenir (Electron: `visible` değişince refresh).
    pub fn on_shown(&mut self) {
        let tools: Vec<ToolId> = self.st.store.open_windows().iter().filter_map(|w| wm::base_tool(&w.id)).collect();
        for t in tools {
            self.pending.push(Cmd::Load(t));
        }
    }
    pub fn on_hidden(&mut self) {
        self.drag = None;
        self.select = None;
        self.pin_menu = None;
        self.tip = None;
        self.key_window = None;
        self.ui.on_hidden();
        self.st.store.flush();
    }

    /// Esc zinciri (OverlayApp.tsx + OverlayWindow.tsx): açılır liste / sabitleme kartı → kapanır;
    /// odaklı metin alanı → odak bırakılır; odaklı pencere → kapanır; hiçbiri yoksa panel kapanır.
    /// Paneli kapatması gerekiyorsa true.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.select.take().is_some() || self.pin_menu.take().is_some() {
            return false;
        }
        if self.ui.escape(&mut self.st, window, cx) {
            return false;
        }
        if let Some(id) = self.key_window.take() {
            self.close_window(&id);
            return false;
        }
        true
    }

    // ---- İşaretçi (host her fare olayından sonra çağırır; sürükleme GPUI olay zincirinden bağımsız) ----
    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        let Some(d) = self.drag.as_mut() else { return false };
        let (dx, dy) = (x - d.start.0, y - d.start.1);
        if !d.moved && dx.abs() < 2. && dy.abs() < 2. {
            return false;
        }
        d.moved = true;
        let (vw, vh) = (self.st.store.layout.viewport.w, self.st.store.layout.viewport.h);
        let win = d.rect;
        match d.resize {
            None => {
                let zone = wm::snap_zone(&self.st.store, x, y);
                let raw = (clamp(win.x + dx, TOOLBAR_W, vw - wm::EDGE_RIGHT), clamp(win.y + dy, 0., vh - wm::EDGE_BOTTOM));
                let (nx, ny) = if zone.is_some() { raw } else { wm::snap_position(&self.st.store, &d.id, raw.0, raw.1, win.w, win.h) };
                d.zone = zone;
                d.cur = Rect { x: nx.round(), y: ny.round(), w: win.w, h: win.h };
            }
            Some(edge) => {
                let min = wm::meta(wm::base_tool(&d.id).unwrap_or(ToolId::Notes)).min;
                let (mut nx, mut ny, mut nw, mut nh) = (win.x, win.y, win.w, win.h);
                let has = |c: &str| format!("{edge:?}").to_lowercase().contains(c);
                if has("e") {
                    nw = clamp(win.w + dx, min.0, vw - win.x);
                }
                if has("s") {
                    nh = clamp(win.h + dy, min.1, vh - win.y);
                }
                if has("w") {
                    let right = win.x + win.w;
                    nx = clamp(win.x + dx, TOOLBAR_W, right - min.0);
                    nw = right - nx;
                }
                if has("n") {
                    let bottom = win.y + win.h;
                    ny = clamp(win.y + dy, 0., bottom - min.1);
                    nh = bottom - ny;
                }
                d.cur = Rect { x: nx.round(), y: ny.round(), w: nw.round(), h: nh.round() };
            }
        }
        true
    }
    pub fn pointer_up(&mut self) -> bool {
        let Some(d) = self.drag.take() else { return false };
        if !d.moved {
            return true;
        }
        match (d.resize, d.zone) {
            (None, Some(z)) => self.st.store.resize_window(&d.id, z),
            (None, None) => self.st.store.move_window(&d.id, d.cur.x, d.cur.y),
            (Some(_), _) => self.st.store.resize_window(&d.id, d.cur),
        }
        true
    }

    fn start_drag(&mut self, id: &str, ev: &MouseDownEvent, resize: Option<Edge>) {
        let Some(win) = self.st.store.win(id) else { return };
        if win.maximized() || (resize.is_some() && win.minimized) {
            return;
        }
        let rect = win.rect();
        self.drag = Some(Drag { id: id.into(), resize, start: (f32::from(ev.position.x), f32::from(ev.position.y)), rect, cur: rect, zone: None, moved: false });
    }

    pub fn focus_window(&mut self, id: &str) {
        self.st.store.focus_window(id);
        self.key_window = Some(id.to_string());
    }

    pub fn open_select(&mut self, key: impl Into<String>, options: Vec<(String, String)>, selected: impl Into<String>) {
        let key = key.into();
        if self.select.as_ref().is_some_and(|s| s.key == key) {
            self.pending.push(Cmd::Log(format!("liste kapandı: {key}")));
            self.select = None;
            return;
        }
        let Some(anchor) = self.measured.borrow().get(&key).copied() else {
            self.pending.push(Cmd::Log(format!("liste açılamadı (ölçü yok): {key}")));
            return;
        };
        self.pending.push(Cmd::Log(format!("liste açıldı: {key}")));
        self.select = Some(SelectPop { key, options, selected: selected.into(), anchor });
    }
}

fn clamp(v: f32, min: f32, max: f32) -> f32 {
    v.min(max).max(min)
}

/// CSS `rgba()` (p2-ui `css_rgba` ile aynı).
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    gpui_kit::Rgba { r: r as f32 / 255., g: g as f32 / 255., b: b as f32 / 255., a }.into()
}

/// `.p2-kbd` / `.p2-keys kbd` (toast, ipucu): min-w 20, p 2 7, alt kenarlık 2px.
pub fn key_cap(text: &str, p: &Palette) -> gpui_kit::Div {
    p2_ui::controls::kbd(text.to_string(), p)
}

/// `.p2-keys`: tuşlar arasında "+".
pub fn keys_row(keys: &[String], p: &Palette) -> gpui_kit::Div {
    let mut row = div().flex().items_center().gap(px(3.));
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            row = row.child(div().mx(px(1.)).text_size(font_size::XS).text_color(rgba(255, 255, 255, 0.4)).child("+"));
        }
        row = row.child(key_cap(k, p));
    }
    row
}

/// `.p2-empty`: ortalı başlık + açıklama.
pub fn empty(title: Option<&str>, detail: Option<&str>, p: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.))
        .py(px(34.))
        .px(px(18.))
        .text_color(p.text_3)
        .when_some(title, |d, t| d.child(div().text_size(font_size::LG).font_weight(FontWeight::BOLD).text_color(p.text).child(t.to_string())))
        .when_some(detail, |d, t| d.child(div().text_size(font_size::SM).text_center().child(t.to_string())))
}

impl OverlayRoot {
    // ======================================================================== araç çubuğu
    /// `.ow-toolbar`: 56 px, cam zemin, sağ kenarlık, araçlar 40×40 (r 11, aralık 4), altta ekran görüntüsü / ayarlar / kapat.
    fn toolbar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = &self.st.store.layout;
        let mut tools = div().flex().flex_col().gap(px(4.)).flex_1().min_h_0();
        for (i, tool) in BAR_TOOLS.iter().enumerate() {
            let meta = wm::meta(*tool);
            let win = layout.windows.get(meta.key);
            let open = win.is_some_and(|w| w.open);
            let pinned = open && win.is_some_and(|w| w.pinned);
            let focused = open && layout.focus.as_deref() == Some(meta.key);
            let hotkey = match tool {
                ToolId::Voice => Some((p2_i18n::t("ovr.toolbar.mute"), self.st.mute_keys.clone())),
                ToolId::Capture => Some((p2_i18n::t("ovr.toolbar.capture"), self.st.shot_keys.clone())),
                _ => None,
            };
            let key = meta.key;
            tools = tools.child(
                self.tool_button(i, ("tb", i), icons::tool_icon(*tool), open, pinned, focused, false, p2_i18n::t(meta.title), hotkey.map(|(l, k)| (Some(l), k)), p, cx)
                    .child(self.mark(format!("tb:{key}")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.key_window = None;
                        this.toggle_window(key);
                        cx.notify();
                    })),
            );
        }
        let n = BAR_TOOLS.len();
        let bottom = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .pt(px(8.))
            .border_t_1()
            .border_color(p.border_subtle)
            .child(
                self.tool_button(n, "tb-shot", icons::CAPTURE, false, false, false, false, p2_i18n::t("ovr.takeScreenshot"), Some((None, self.st.shot_keys.clone())), p, cx)
                    .child(self.mark("tb:shot"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pending.push(Cmd::Shot);
                        cx.notify();
                    })),
            )
            .child(
                self.tool_button(n + 1, "tb-settings", icons::SETTINGS, false, false, false, false, p2_i18n::t("ovr.tool.library"), None, p, cx)
                    .child(self.mark("tb:settings"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.key_window = None;
                        this.toggle_window("library");
                        cx.notify();
                    })),
            )
            .child(
                self.tool_button(n + 2, "tb-close", icons::CLOSE, false, false, false, true, p2_i18n::t("ovr.toolbar.closePanel"), Some((None, self.st.panel_keys.clone())), p, cx)
                    .child(self.mark("tb:close"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pending.push(Cmd::SetVisible(false));
                        cx.notify();
                    })),
            );
        let logo_tip = self.tip == Some(usize::MAX);
        let name = self.st.project_name.clone();
        div()
            .id("ow-toolbar")
            .occlude()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(px(TOOLBAR_W))
            .py(px(10.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .border_r_1()
            .border_color(p.border_strong)
            .bg(p.glass.background())
            .shadow(p.shadow_md())
            .child(
                div()
                    .id("ow-logo")
                    .relative()
                    .size(px(40.))
                    .mb(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(logo_image(28., p))
                    .on_hover(cx.listener(|this, h: &bool, _, cx| {
                        this.tip = if *h { Some(usize::MAX) } else { None };
                        cx.notify();
                    }))
                    .when(logo_tip, |d| d.child(tip_box("P2 Overlay", Some(div().child(name).into_any_element()), p))),
            )
            .child(tools)
            .child(bottom)
    }

    #[allow(clippy::too_many_arguments)]
    fn tool_button(
        &self,
        index: usize,
        id: impl Into<gpui_kit::ElementId>,
        body: &'static str,
        active: bool,
        pinned: bool,
        focused: bool,
        close: bool,
        title: String,
        hotkey: Option<(Option<String>, Vec<String>)>,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let hovered = self.tip == Some(index);
        let color = if active { p.text_accent } else if hovered { if close { p.danger_text } else { p.text_strong } } else { p.text_2 };
        let bg = if active { Some(p.accent_soft_2) } else if hovered { Some(if close { p.danger_soft } else { p.bg_hover }) } else { None };
        let small = hotkey.filter(|(_, k)| !k.is_empty()).map(|(label, keys)| {
            div().flex().items_center().gap(px(6.)).when_some(label, |d, l| d.child(l)).child(keys_row(&keys, p)).into_any_element()
        });
        div()
            .id(id)
            .relative()
            .size(px(40.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::MD)
            .cursor_pointer()
            .when_some(bg, |d, b| d.bg(b))
            .when(active, |d| d.shadow(vec![BoxShadow { color: p.border_accent, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: true }]))
            .child(ic(body, 1.8, 21., color))
            // Odaklı pencerenin aracı: solda 3 px vurgu çubuğu (::before, left -8 / top-bottom 10).
            .when(focused, |d| d.child(div().absolute().left(px(-8.)).top(px(10.)).bottom(px(10.)).w(px(3.)).rounded(radius::PILL).bg(p.accent)))
            // Sabitli + açık: sağ üstte nokta.
            .when(pinned, |d| {
                d.child(div().absolute().top(px(5.)).right(px(5.)).size(px(8.)).rounded_full().bg(p.accent).shadow(vec![BoxShadow { color: p.bg_1, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }]))
            })
            .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                if *h {
                    this.tip = Some(index);
                } else if this.tip == Some(index) {
                    this.tip = None;
                }
                cx.notify();
            }))
            .when(hovered, |d| d.child(tip_box(&title, small, p)))
    }

    // ======================================================================== pencere
    fn window_el(&self, win: &WinState, ghost: bool, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = win.id.clone();
        let Some(tool) = wm::base_tool(&id) else { return div().into_any_element() };
        let meta = wm::meta(tool);
        let dragging = self.drag.as_ref().filter(|d| d.id == id && d.moved);
        let rect = dragging.map(|d| d.cur).unwrap_or(win.rect());
        let focused = !ghost && self.st.store.layout.focus.as_deref() == Some(id.as_str());
        let maximized = win.maximized();
        let compact = ghost && meta.ghost_compact;
        let title = self.tool_title(tool);
        let body = self.tool_body(tool, win, ghost, p, window, cx);
        let pct = (win.pin_opacity * 100.).round() as i32;

        let mut frame = div().relative().flex_1().flex().flex_col().min_w_0().min_h_0().overflow_hidden().rounded(px(13.));
        let mut head = div()
            .id(SharedString::from(format!("head-{id}")))
            .relative()
            .flex_none()
            .h(px(36.))
            .pl(px(12.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.bg_2)
            // GPUI içeriği yuvarlak köşeye göre kırpmaz: zemini olan her katman köşe yarıçapını kendisi taşır.
            .rounded_t(px(13.))
            .when(win.minimized, |d| d.rounded_b(px(13.)))
            .child(div().flex().items_center().justify_center().child(ic(icons::tool_icon(tool), 1.8, 15., p.text_accent)))
            .child(div().flex_1().min_w_0().truncate().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title));
        if ghost {
            head = head.child(div().flex().items_center().child(ic(icons::PIN, 1.8, 13., p.text_accent)));
        } else {
            head = head.child(self.mark(format!("win:{id}:head")));
            if win.pinned {
                head = head.child(badge(format!("%{pct}"), BadgeTone::Soft, p));
            }
            let act = |key: &str, body: &'static str, size: f32, on: bool, close: bool| {
                let (hb, hc) = if close { (p.danger_soft, p.danger_text) } else { (p.bg_hover, p.text_strong) };
                let group = SharedString::from(format!("act-{id}-{key}"));
                div()
                    .id(SharedString::from(format!("{id}-{key}")))
                    .relative()
                    .group(group.clone())
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::XS)
                    .cursor_pointer()
                    .when(on, |d| d.bg(p.accent_soft))
                    // GPUI svg rengi kalıtmaz: renk ve hover rengi doğrudan ikona.
                    .hover(move |s| s.bg(hb))
                    .child(p2_ui::icons::Icon::raw(body).stroke(1.8).el().size(px(size)).text_color(if on { p.text_accent } else { p.text_3 }).group_hover(group, move |s| s.text_color(hc)))
                    .child(self.mark(format!("win:{id}:{key}")))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            };
            let mut actions = div().flex().gap(px(2.)).flex_none();
            if meta.pinnable {
                let pid = id.clone();
                actions = actions.child(act("pin", icons::PIN, 15., win.pinned, false).on_click(cx.listener(move |this, _, _, cx| {
                    let pinned = this.st.store.win(&pid).is_some_and(|w| w.pinned);
                    if !pinned {
                        this.st.store.set_pinned(&pid, true);
                        this.pin_menu = Some(pid.clone());
                    } else {
                        this.pin_menu = if this.pin_menu.as_deref() == Some(pid.as_str()) { None } else { Some(pid.clone()) };
                    }
                    cx.notify();
                })));
            }
            let (mid, xid, cid) = (id.clone(), id.clone(), id.clone());
            let minimized = win.minimized;
            actions = actions
                .child(act("min", icons::MINIMIZE, 15., false, false).on_click(cx.listener(move |this, _, _, cx| {
                    this.st.store.minimize_window(&mid, !minimized);
                    cx.notify();
                })))
                .child(act("max", if maximized { icons::RESTORE } else { icons::MAXIMIZE }, 14., false, false).on_click(cx.listener(move |this, _, _, cx| {
                    this.st.store.maximize_window(&xid);
                    cx.notify();
                })))
                .child(act("close", icons::CLOSE, 15., false, true).on_click(cx.listener(move |this, _, _, cx| {
                    this.close_window(&cid);
                    cx.notify();
                })));
            let did = id.clone();
            head = head
                .child(actions)
                .when(!maximized, |d| d.cursor(CursorStyle::OpenHand))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    if ev.click_count >= 2 {
                        this.st.store.maximize_window(&did);
                    } else {
                        this.start_drag(&did, ev, None);
                    }
                    cx.notify();
                }));
        }
        frame = frame.child(head);
        if let Some(body) = body {
            frame = frame.child(if win.minimized {
                div().flex_none().child(body).into_any_element()
            } else {
                // Kaydırma çubuğu (ui.css: 10 px, 2 px saydam kenarlıklı başparmak rgba(120,170,255,.18)):
                // içerik taşınca 10 px yer kaplar (Chromium gibi). Taşma bir önceki karenin ölçüsünden.
                let handle = self.scrolls.borrow_mut().entry(id.clone()).or_default().clone();
                let (view, max) = (f32::from(handle.bounds().size.height), f32::from(handle.max_offset().y));
                let overflow = !ghost && max > 0.5 && view > 0.;
                let thumb = overflow.then(|| {
                    let content = view + max;
                    let len = (view / content * view).max(20.);
                    let top = (-f32::from(handle.offset().y) / max).clamp(0., 1.) * (view - len);
                    div().absolute().right(px(2.)).top(px(top)).w(px(6.)).h(px(len)).rounded(radius::PILL).bg(rgba(120, 170, 255, 0.18))
                });
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .rounded_b(px(13.))
                    .child(
                        div()
                            .id(SharedString::from(format!("body-{id}")))
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .when(overflow, |d| d.pr(px(10.)))
                            .when(!ghost, |d| d.overflow_y_scroll().track_scroll(&handle))
                            .when(ghost, |d| d.overflow_hidden())
                            .child(body),
                    )
                    .children(thumb)
                    .into_any_element()
            });
        }

        let mut ring = p.shadow_lg();
        if focused {
            ring.push(BoxShadow { color: p.accent_soft_2, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false });
        }
        let fid = id.clone();
        let mut outer = div()
            .id(SharedString::from(format!("ow-{id}")))
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .when(!win.minimized && !compact, |d| d.h(px(rect.h)))
            .when(compact && !win.minimized, |d| d.max_h(px(rect.h)))
            .flex()
            .flex_col()
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border_strong })
            .rounded(radius::LG)
            .bg(p.glass.background())
            .shadow(ring)
            .text_color(p.text)
            .child(frame)
            // Kenarlık içeriğin ÜSTÜNE ayrıca çizilir: başlık/gövde zeminleri köşe kavisinde kenarlığı
            // örtüyordu (düz kenarda çizgi var, köşede yok → köşe tırtıklı ve garip görünüyordu).
            .child(
                div()
                    .absolute()
                    .top(px(-1.))
                    .left(px(-1.))
                    .right(px(-1.))
                    .bottom(px(-1.))
                    .rounded(radius::LG)
                    .border_1()
                    .border_color(if focused { p.border_accent } else { p.border_strong }),
            )
            // Sabitleme kartı çerçevenin dışında (çerçeve kırpar): başlığın 6 px altında, sağdan 6 px.
            .when(!ghost && self.pin_menu.as_deref() == Some(id.as_str()), |d| d.child(self.pin_pop(win, p, cx)));
        if ghost {
            outer = outer.opacity(win.pin_opacity);
        } else {
            outer = outer.occlude().capture_any_mouse_down(cx.listener(move |this, _, _, cx| {
                this.focus_window(&fid);
                cx.notify();
            }));
            if !win.minimized && !maximized {
                for edge in EDGES {
                    let gid = id.clone();
                    outer = outer.child(grip(edge).on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_drag(&gid, ev, Some(edge));
                        cx.notify();
                    })));
                }
            }
        }
        let el = outer.into_any_element();
        match self.entering.get(&id) {
            Some(seq) if !ghost => div().absolute().top_0().left_0().size_full().child(div().size_full().child(el).p2_enter(SharedString::from(format!("ow-enter-{id}-{seq}")), Entrance::PopElastic, Duration::ZERO)).into_any_element(),
            _ => el,
        }
    }

    /// `.ow-pin-pop`: sabitleme kartı (saydamlık kaydırıcısı, sabitlemeyi kaldır, paneli kapat ve gör).
    fn pin_pop(&self, win: &WinState, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let id = win.id.clone();
        let pct = (win.pin_opacity * 100.).round();
        let value = (pct - wm::PIN_OPACITY_MIN * 100.) / ((wm::PIN_OPACITY_MAX - wm::PIN_OPACITY_MIN) * 100.);
        let slider_key = format!("win:{id}:opacity");
        let (sid, uid) = (id.clone(), id.clone());
        let measured = self.measured.clone();
        let slider = div()
            .id(SharedString::from(slider_key.clone()))
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(14.))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(self.mark(slider_key.clone()))
            .child(slider_track(value, p))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if let Some(b) = measured.borrow().get(&slider_key).copied() {
                    let t = (f32::from(ev.position.x - b.origin.x) / f32::from(b.size.width)).clamp(0., 1.);
                    let v = wm::PIN_OPACITY_MIN * 100. + t * (wm::PIN_OPACITY_MAX - wm::PIN_OPACITY_MIN) * 100.;
                    let stepped = (v / 5.).round() * 5.;
                    this.st.store.set_pin_opacity(&sid, stepped / 100.);
                }
                cx.notify();
            }));
        let card = div()
            .id(SharedString::from(format!("pin-pop-{id}")))
            .w(px(290.))
            .occlude()
            .flex()
            .flex_col()
            .gap(px(8.))
            .py(px(12.))
            .px(px(14.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::LG)
            .bg(p.glass.background())
            .shadow(p.shadow_lg())
            .cursor_default()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(kicker(p2_i18n::t("ovr.pin.kicker"), p))
            .child(div().text_size(font_size::SM).text_color(p.text_2).line_height(gpui_kit::relative(1.35)).child(p2_i18n::t("ovr.pin.hint")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(font_size::SM)
                    .text_color(p.text_2)
                    .child(p2_i18n::t("ovr.pin.opacity"))
                    .child(slider)
                    .child(key_cap(&format!("%{}", pct as i32), p)),
            )
            .child(
                div()
                    .mt(px(2.))
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .child(self.btn("pin-unpin", p2_i18n::t("ovr.pin.unpin"), Btn::Ghost, p).child(self.mark(format!("win:{id}:unpin"))).on_click(cx.listener(move |this, _, _, cx| {
                        this.st.store.set_pinned(&uid, false);
                        this.pin_menu = None;
                        cx.notify();
                    })))
                    .child(self.btn("pin-hide", p2_i18n::t("ovr.pin.hidePanel"), Btn::Primary, p).child(self.mark(format!("win:{id}:hide"))).on_click(cx.listener(|this, _, _, cx| {
                        this.pin_menu = None;
                        this.pending.push(Cmd::SetVisible(false));
                        cx.notify();
                    }))),
            )
            .p2_enter("pin-pop", if self.st.look.reduce_motion { Entrance::None } else { Entrance::ScaleIn }, Duration::ZERO);
        // p2_enter öğeyi relative yapar: konum dış sarmalayıcıda.
        div().absolute().top(px(36. + 6.)).right(px(6.)).child(card)
    }

    /// `.p2-btn.sm` (h 28, p 0 10, r 6, 650 fs-md).
    pub fn btn(&self, id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, kind: Btn, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
        btn(id, label, kind, p)
    }

    // ======================================================================== bildirimler
    fn toasts_view(&self, interactive: bool, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let corner = self.st.look.toast_corner.as_str();
        let left = corner.ends_with("left");
        let bottom = corner.starts_with("bottom");
        let under_hud = self.st.hud() && self.st.look.hud_position == corner;
        let mut stack = div().absolute().w(px(352.)).flex().flex_col().gap(px(10.));
        stack = if bottom { stack.bottom(px(20.)) } else { stack.top(px(if under_hud { 52. } else { 20. })) };
        stack = if left { stack.left(px(if interactive { 76. } else { 20. })) } else { stack.right(px(20.)) };
        for item in &self.st.toasts.visible {
            stack = stack.child(self.toast_card(item, interactive, left, p, cx));
        }
        stack
    }

    fn toast_card(&self, item: &crate::toasts::Item, interactive: bool, left: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let t = &item.toast;
        let kind = if t.kind.is_empty() { "info" } else { t.kind.as_str() };
        let kicker_text = match kind {
            "message" | "mention" => p2_i18n::t("ovr.toastKind.message"),
            "call" => p2_i18n::t("ovr.toastKind.call"),
            "screenshot" => p2_i18n::t("ovr.tool.capture"),
            "play" | "play-stop" => p2_i18n::t("ovr.toastKind.play"),
            "task" => p2_i18n::t("ovr.toastKind.task"),
            "release" => p2_i18n::t("ovr.summary.releaseBadge"),
            "download" => p2_i18n::t("ovr.toastKind.download"),
            _ => "P2".to_string(),
        };
        let (bar_a, bar_b, kick) = match kind {
            "success" => (p.success, p.success_text, p.success_text),
            "error" => (p.danger, p.danger_text, p.danger_text),
            _ => (p.accent_bar.stops[0].0, p.accent_bar.stops.last().map_or(p.accent, |s| s.0), p.text_accent),
        };
        let has_actions = !t.actions.is_empty();
        let media = match &t.avatar {
            Some(a) => avatar(&a.user_id, &a.name, 44., p).into_any_element(),
            None => logo_image(44., p).into_any_element(),
        };
        let mut body = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(kicker(kicker_text, p).text_color(kick))
            .child(div().text_size(font_size::LG).font_weight(FontWeight::BOLD).line_height(gpui_kit::relative(1.2)).text_color(p.text_strong).truncate().child(t.title.clone()));
        if !t.text.is_empty() || !t.keys.is_empty() {
            body = body.child(
                div()
                    .mt(px(2.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .text_color(p.text_2)
                    .text_size(font_size::SM)
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(gpui_kit::relative(1.3))
                    .child(t.text.clone())
                    .when(!t.keys.is_empty(), |d| d.child(div().ml(px(8.)).child(keys_row(&t.keys, p)))),
            );
        }
        if has_actions {
            let mut row = div().flex().flex_wrap().gap(px(6.)).mt(px(8.));
            for a in &t.actions {
                let (tid, tag, aid, payload) = (t.id.clone(), t.tag.clone(), a.id.clone(), t.payload.clone());
                let b = btn(SharedString::from(format!("ta-{}-{}", t.id, a.id)), a.label.clone(), if a.primary { Btn::Primary } else { Btn::Secondary }, p)
                    .child(self.mark(format!("toast:{}:{}", t.id, a.id)))
                    .when(!interactive, |d| d.opacity(0.7));
                row = row.child(if interactive {
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.pending.push(Cmd::ToastAction { toast_id: tid.clone(), tag: tag.clone(), action_id: aid.clone(), payload: payload.clone() });
                        this.st.toasts.dismiss(&tid);
                        cx.notify();
                    }))
                } else {
                    b
                });
            }
            body = body.child(row);
        }
        let tid = t.id.clone();
        let close: AnyElement = if interactive {
            btn(SharedString::from(format!("tc-{}", t.id)), "×", Btn::IconGhost, p)
                .child(self.mark(format!("toast:{}:close", t.id)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.st.toasts.dismiss(&tid);
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let bar = (!t.sticky).then(|| {
            let life = crate::toasts::life(t);
            let total = life.saturating_sub(crate::toasts::OUT).as_secs_f32().max(0.001);
            let gone = (item.shown_at.elapsed().as_secs_f32() / total).clamp(0., 1.);
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .h(px(2.))
                .w(gpui_kit::relative(1. - gone))
                .opacity(0.85)
                .bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(bar_a, 0.), gpui_kit::linear_color_stop(bar_b, 1.)))
        });
        let card = div()
            .id(SharedString::from(format!("toast-{}", t.id)))
            .relative()
            .overflow_hidden()
            .w(px(352.))
            .flex()
            .when(has_actions, |d| d.items_start())
            .when(!has_actions, |d| d.items_center())
            .gap(px(13.))
            .pt(px(12.))
            .pr(px(12.))
            .pb(px(14.))
            .pl(px(14.))
            .border_1()
            .border_color(match kind {
                "error" => p.danger_border,
                "call" => p.border_accent,
                _ => p.border_strong,
            })
            .rounded(radius::LG)
            .bg(p.glass.background())
            .shadow(p.shadow_lg())
            .when(interactive, |d| d.occlude())
            .child(div().size(px(44.)).flex_shrink_0().child(media))
            .child(body)
            .child(div().self_start().mt(px(-4.)).mr(px(-4.)).child(close))
            .children(bar);
        let reduce = self.st.look.reduce_motion;
        let key = SharedString::from(format!("toast-{}-{}", t.id, item.seq));
        if let Some(at) = item.leaving {
            // p2-slide-right-out / ow-slide-left-out (.36s ease-in)
            let k = if reduce { 1. } else { p2_ui::motion::EASE_IN.ease((at.elapsed().as_secs_f32() / 0.36).clamp(0., 1.)) };
            let dx = (352. + 48.) * k * if left { -1. } else { 1. };
            return div().relative().left(px(dx)).opacity(1. - k).child(card).into_any_element();
        }
        let entrance = if reduce { Entrance::None } else if left { Entrance::SlideLeftElastic { width: 352. } } else { Entrance::SlideRightElastic { width: 352. } };
        card.p2_enter(key, entrance, Duration::ZERO).into_any_element()
    }

    // ======================================================================== HUD
    /// `.ow-hud`: 28 px hap, cam zemin, mono 700 fs-sm; birimler 800 fs-2xs + kicker aralığı.
    fn hud(&self, shifted: bool, p: &Palette) -> impl IntoElement {
        let s = &self.st.perf;
        let look = &self.st.look;
        let value = |v: String, unit: &str, low: bool| {
            div()
                .flex()
                .items_baseline()
                .text_color(if low { p.warning_text } else { p.text_strong })
                .child(v)
                .child(div().ml(px(4.)).font_family(font()).text_size(font_size::XXS).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_3).child(spaced(unit)))
        };
        let divider = || div().w(px(1.)).h(px(14.)).bg(p.border_strong);
        let fps = (s.fps.is_finite() && self.st.history.last().is_some()).then(|| s.fps.round() as i64);
        let ping = self.st.ping;
        let cpu = s.game_cpu.map(|c| c.round() as i64);
        let mut pill = div()
            .h(px(28.))
            .px(px(11.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(radius::PILL)
            .border_1()
            .border_color(p.border)
            .bg(p.glass.background())
            .shadow(p.shadow_sm())
            .font_family(FONT_MONO)
            .text_size(font_size::SM)
            .font_weight(FontWeight::BOLD)
            .whitespace_nowrap()
            .child(value(fps.map(|f| f.to_string()).unwrap_or_else(|| "—".into()), "FPS", fps.is_some_and(|f| f < 30)))
            .child(divider())
            .child(value(ping.map(|v| v.to_string()).unwrap_or_else(|| "—".into()), "ms", ping.is_some_and(|v| v > 150)));
        if look.hud_details {
            let loss = self.st.loss;
            pill = pill
                .child(divider())
                .child(value(cpu.map(|c| c.to_string()).unwrap_or_else(|| "—".into()), "CPU", cpu.is_some_and(|c| c > 90)))
                .when_some(s.game_ram_mb, |d, mb| d.child(value(if mb >= 1024 { format!("{}G", (mb as f32 / 1024. * 10.).round() / 10.) } else { format!("{mb}M") }, "RAM", false)))
                .child(value(loss.map(|l| format!("{}", (l * 10.).round() / 10.)).unwrap_or_else(|| "—".into()), &p2_i18n::t("ovr.hud.loss"), loss.is_some_and(|l| l > 5.)));
        }
        let row = div().absolute().top(px(12.)).left(px(if shifted && look.hud_position == "top-left" { 68. } else { 12. })).right(px(12.)).flex();
        match look.hud_position.as_str() {
            "top-center" => row.left(px(12.)).justify_center(),
            "top-right" => row.justify_end(),
            _ => row.justify_start(),
        }
        .child(pill)
    }

    // ======================================================================== açılır liste
    fn select_view(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pop = self.select.as_ref()?;
        let a = pop.anchor;
        let width = f32::from(a.size.width).max(160.);
        let item_h = 32.;
        let height = (pop.options.len() as f32 * item_h + 10.).min(260.);
        let mut top = f32::from(a.origin.y + a.size.height) + 4.;
        if top + height > self.st.h - 8. {
            top = (f32::from(a.origin.y) - 4. - height).max(8.);
        }
        let mut left = f32::from(a.origin.x);
        if left + width > self.st.w - 8. {
            left = (self.st.w - 8. - width).max(8.);
        }
        let mut list = div()
            .id("ow-select-pop")
            .absolute()
            .left(px(left.round()))
            .top(px(top.round()))
            .min_w(px(width.round()))
            .max_h(px(260.))
            .overflow_y_scroll()
            .occlude()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(5.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::MD)
            .bg(p.glass.background())
            .shadow(p.shadow_lg());
        for (value, label) in &pop.options {
            let selected = *value == pop.selected;
            let (key, v) = (pop.key.clone(), value.clone());
            let (soft, strong) = (p.accent_soft, p.text_strong);
            list = list.child(
                div()
                    .id(SharedString::from(format!("opt-{label}")))
                    .relative()
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(radius::XS)
                    .text_size(font_size::MD)
                    .text_color(if selected { p.text_accent } else { p.text })
                    .when(selected, |d| d.font_weight(FontWeight::BOLD))
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .child(label.clone())
                    .child(self.mark(format!("opt:{label}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select = None;
                        this.on_select(&key, &v, window, cx);
                        cx.notify();
                    })),
            );
        }
        // p2_enter öğeyi relative yapar: canlandırma tam boy iç katmanda, liste onun içinde mutlak.
        Some(div().absolute().top_0().left_0().size_full().child(div().size_full().child(list).p2_enter("ow-select-pop", if self.st.look.reduce_motion { Entrance::None } else { Entrance::ScaleIn }, Duration::ZERO)).into_any_element())
    }

    /// Açık pencerelerin çizim sırası (z).
    fn windows(&self, ghost: bool, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let wins: Vec<WinState> = if ghost { self.st.store.pinned_windows().into_iter().cloned().collect() } else { self.st.store.open_windows().into_iter().cloned().collect() };
        wins.iter().map(|w| self.window_el(w, ghost, p, window, cx)).collect()
    }
}

/// İpucu kutusu (`.ow-tip`): sağda 10 px, cam, gölge.
fn tip_box(title: &str, small: Option<AnyElement>, p: &Palette) -> impl IntoElement {
    div()
        .absolute()
        .left(px(40. + 10.))
        .top_0()
        .bottom_0()
        .flex()
        .items_center()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .py(px(7.))
                .px(px(10.))
                .border_1()
                .border_color(p.border_strong)
                .rounded(radius::SM)
                .bg(p.glass_strong.background())
                .shadow(p.shadow_md())
                .text_color(p.text)
                .whitespace_nowrap()
                .child(div().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title.to_string()))
                .when_some(small, |d, s| d.child(div().flex().items_center().gap(px(6.)).text_size(font_size::XS).text_color(p.text_3).child(s))),
        )
}

/// `.p2-slider`: 4 px iz (vurgu dolgu + navy-600), 14 px beyaz topuz.
pub fn slider_track(value: f32, p: &Palette) -> impl IntoElement {
    let v = value.clamp(0., 1.);
    div()
        .relative()
        .w_full()
        .h(px(4.))
        .rounded(radius::PILL)
        .bg(p.navy_600)
        .child(div().absolute().left_0().top_0().bottom_0().w(gpui_kit::relative(v)).rounded(radius::PILL).bg(p.accent))
        .child(
            div()
                .absolute()
                .top(px(-5.))
                .left(gpui_kit::relative(v))
                .ml(px(-7. + (0.5 - v) * 14.))
                .size(px(14.))
                .rounded_full()
                .bg(gpui_kit::white())
                .shadow(vec![BoxShadow { color: rgba(0, 0, 0, 0.45), offset: point(px(0.), px(1.)), blur_radius: px(4.), spread_radius: px(0.), inset: false }]),
        )
}

/// Kenar/köşe tutamacı (`.ow-grip-*`: kenar 6 px, köşe 14 px, çerçevenin 3 px dışına taşar).
fn grip(edge: Edge) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (g, gc) = (6., 14.);
    let d = div().id(SharedString::from(format!("grip-{edge:?}"))).absolute();
    match edge {
        Edge::N => d.top(px(-g / 2.)).left(px(gc)).right(px(gc)).h(px(g)).cursor(CursorStyle::ResizeUpDown),
        Edge::S => d.bottom(px(-g / 2.)).left(px(gc)).right(px(gc)).h(px(g)).cursor(CursorStyle::ResizeUpDown),
        Edge::E => d.right(px(-g / 2.)).top(px(gc)).bottom(px(gc)).w(px(g)).cursor(CursorStyle::ResizeLeftRight),
        Edge::W => d.left(px(-g / 2.)).top(px(gc)).bottom(px(gc)).w(px(g)).cursor(CursorStyle::ResizeLeftRight),
        Edge::NE => d.top(px(-g / 2.)).right(px(-g / 2.)).size(px(gc)).cursor(CursorStyle::ResizeUpRightDownLeft),
        Edge::SW => d.bottom(px(-g / 2.)).left(px(-g / 2.)).size(px(gc)).cursor(CursorStyle::ResizeUpRightDownLeft),
        Edge::NW => d.top(px(-g / 2.)).left(px(-g / 2.)).size(px(gc)).cursor(CursorStyle::ResizeUpLeftDownRight),
        Edge::SE => d.bottom(px(-g / 2.)).right(px(-g / 2.)).size(px(gc)).cursor(CursorStyle::ResizeUpLeftDownRight),
    }
}

/// Pencere başlığı eylem ikonu: rengi kapsayıcıdan (hover dahil) kalıtır.
struct Icon2(&'static str, f32);
impl IntoElement for Icon2 {
    type Element = gpui_kit::Svg;
    fn into_element(self) -> Self::Element {
        p2_ui::icons::Icon::raw(self.0).stroke(1.8).el().size(px(self.1))
    }
}

/// Kapsayıcının metin rengini (hover dahil) kalıtan ikon: gövde, kontur, boyut.
pub struct IconC(pub &'static str, pub f32, pub f32);
impl IntoElement for IconC {
    type Element = gpui_kit::Svg;
    fn into_element(self) -> Self::Element {
        p2_ui::icons::Icon::raw(self.0).stroke(self.1).el().size(px(self.2))
    }
}

/// Kicker harf aralığı (.18em) cosmic-text'te yok: ince boşlukla yaklaşık.
pub fn spaced(text: &str) -> String {
    text.to_string()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Primary,
    Secondary,
    Ghost,
    Danger,
    IconGhost,
}

/// `.p2-btn.sm` + tür (`p2_ui::controls::button`): h 28, p 0 10, r 6, 650 fs-sm; ikon: 28×28.
pub fn btn(id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, kind: Btn, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    use p2_ui::controls::{BtnSize, BtnVariant, button};
    let (variant, icon) = match kind {
        Btn::Primary => (BtnVariant::Primary, false),
        Btn::Secondary => (BtnVariant::Secondary, false),
        Btn::Ghost => (BtnVariant::Ghost, false),
        Btn::Danger => (BtnVariant::Danger, false),
        Btn::IconGhost => (BtnVariant::Ghost, true),
    };
    let label: SharedString = label.into();
    button(id, variant, BtnSize::Sm, icon, false, p).relative().when(!label.is_empty(), |d| d.child(label))
}

impl Render for OverlayRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        self.measured.borrow_mut().clear();
        let mut root = div().id("overlay-root").size_full().relative().font_family(font()).text_size(font_size::MD).line_height(gpui_kit::relative(p2_ui::tokens::LINE_HEIGHT_NORMAL)).text_color(p.text)
            // cosmic-text normal ağırlığı DirectWrite'tan ince çizer: overlay varsayılanı orta (500).
            .font_weight(FontWeight::MEDIUM);
        if !self.st.visible {
            // Panel kapalı: sabitli pencereler (girdisiz), bildirimler, HUD.
            let ghosts = self.windows(true, &p, window, cx);
            root = root.children(ghosts).child(self.toasts_view(false, &p, cx));
            if self.st.hud() {
                root = root.child(self.hud(false, &p));
            }
            return root;
        }
        let scrim = scrim(self.st.w, self.st.h, &p);
        let wins = self.windows(false, &p, window, cx);
        let snap = self.drag.as_ref().and_then(|d| d.zone).map(|z| {
            div()
                .absolute()
                .left(px(z.x))
                .top(px(z.y))
                .w(px(z.w))
                .h(px(z.h))
                .border_2()
                .border_color(p.accent_hover)
                .rounded(radius::LG)
                .bg(p.accent_soft)
                .shadow(vec![BoxShadow { color: p.accent_glow.color, offset: point(px(p.accent_glow.x), px(p.accent_glow.y)), blur_radius: px(p.accent_glow.blur), spread_radius: px(p.accent_glow.spread), inset: false }])
        });
        root = root
            .child(scrim)
            .children(wins)
            .children(snap)
            .child(self.toolbar(&p, cx))
            .child(self.toasts_view(true, &p, cx));
        if self.st.hud() {
            root = root.child(self.hud(true, &p));
        }
        if let Some(pop) = self.select_view(&p, cx) {
            let shield = div().id("select-shield").absolute().size_full().occlude().on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.pending.push(Cmd::Log("liste dışına tıklandı".into()));
                this.select = None;
                cx.notify();
            }));
            root = root.child(shield).child(pop);
        }
        if self.pin_menu.is_some() {
            // Kartın dışına tıklamak kapatır (OverlayWindow.tsx: pointerdown yakalama).
            root = root.on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.pin_menu = None;
                cx.notify();
            }));
        }
        root
    }
}

/// `.overlay-scrim`: radial-gradient(80% 60% at 50% 0%, accent-soft 0%, transparent 60%), --p2-overlay.
fn scrim(w: f32, h: f32, p: &Palette) -> impl IntoElement {
    // Chromium gibi piksel piksel: elips (rx = %80 w, ry = %60 h, merkez üst orta), accent-soft %0 → saydam %60,
    // altında --p2-overlay; ikisi düz alfa ile birleştirilip tek görsel (yüzey boyutu başına önbellekli).
    static CACHE: std::sync::Mutex<Option<((u32, u32, u32), std::sync::Arc<gpui_kit::Image>)>> = std::sync::Mutex::new(None);
    let to = |c: Hsla| {
        let r: gpui_kit::Rgba = c.into();
        (r.r, r.g, r.b, r.a)
    };
    let (gr, gg, gb, ga) = to(p.accent_soft);
    let (or, og, ob, oa) = to(p.overlay);
    let (iw, ih) = (w.round().max(1.) as u32, h.round().max(1.) as u32);
    let key = (iw, ih, (ga * 1000.) as u32 ^ ((oa * 1000.) as u32) << 10);
    let mut cache = CACHE.lock().unwrap();
    let image = match cache.as_ref().filter(|(k, _)| *k == key) {
        Some((_, img)) => img.clone(),
        None => {
            let (rx, ry, cx) = (0.8 * w, 0.6 * h, w / 2.);
            let mut buf = image::RgbaImage::new(iw, ih);
            for (x, y, px_) in buf.enumerate_pixels_mut() {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5) / ry);
                let t = (dx * dx + dy * dy).sqrt() / 0.6;
                // CSS gradyanı önceden çarpılmış uzayda ara değer bulur: renk sabit, alfa doğrusal.
                let a1 = if t < 1. { ga * (1. - t) } else { 0. };
                let a = a1 + oa * (1. - a1);
                let mix = |c1: f32, c0: f32| if a > 0. { (c1 * a1 + c0 * oa * (1. - a1)) / a } else { 0. };
                *px_ = image::Rgba([(mix(gr, or) * 255.).round() as u8, (mix(gg, og) * 255.).round() as u8, (mix(gb, ob) * 255.).round() as u8, (a * 255.).round() as u8]);
            }
            let mut png = Vec::new();
            let _ = buf.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
            let img = std::sync::Arc::new(gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, png));
            *cache = Some((key, img.clone()));
            img
        }
    };
    gpui_kit::img(image).absolute().top_0().left_0().w(px(w)).h(px(h))
}

#[allow(dead_code)]
fn _unused(_: Hsla) {
    let _ = hsla(0., 0., 0., 0.);
    let _ = deferred(div());
}

/// CSS `text-transform: uppercase`: dile duyarlı (tr: i → İ, ı → I).
pub fn upper_tr(s: &str) -> String {
    p2_i18n::upper(s)
}

/// `.p2-kicker`: büyük harf (Türkçe), 800 fs-2xs, vurgu rengi.
pub fn kick(text: impl AsRef<str>, p: &Palette) -> gpui_kit::Div {
    kicker(upper_tr(text.as_ref()), p)
}

pub trait UpperTr {
    fn pipe_upper(&self) -> String;
}
impl UpperTr for str {
    fn pipe_upper(&self) -> String {
        upper_tr(self)
    }
}
impl UpperTr for char {
    fn pipe_upper(&self) -> String {
        upper_tr(&self.to_string())
    }
}

/// `.p2-avatar` (Avatar.tsx, yuvarlak değil): köşe %25, 145° ton gradyanı, baş harfler 800 boy×.36.
pub fn avatar(key: &str, name: &str, size: f32, p: &Palette) -> gpui_kit::Div {
    p2_ui::controls::avatar_box(size, p2_ui::components::avatar_hue(key), false, p).child(p2_ui::components::user_initials(name))
}
