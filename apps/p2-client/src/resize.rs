//! Sürüklenebilir panel boyutları — ueResizable.tsx + `ResizeHandle` (direct.css `.resize-handle`),
//! DocumentsPage `.documents-sidebar-resizer`, LibraryPage `.library-resizer` ve benzerleri.
//!
//! Electron'da sürükleme `document`'e bağlı `mousemove`/`mouseup` dinleyicileriyle izlenir: imleç
//! tutamaçtan, panelden ya da kırpılan bölgeden çıksa da sürer. Burada da aynısı: sürükleme
//! sürerken tutamacın içindeki görünmez `canvas` her karede pencere düzeyinde fare dinleyicisi
//! kaydeder (`Window::on_mouse_event`, isabet/hover şartı yok) ve imleci pencerenin tamamında
//! sütun boyutlandırma imlecine çevirir (`body.resizing { cursor: col-resize }`).
//!
//! Genişlikler aynı `localStorage` anahtarlarıyla profil klasöründeki `layout.json`'a yazılır
//! (`p2_client_core::store::save_layout_value`); Electron'dan taşınan değerler okunur.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, CursorStyle, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window, canvas, div, px};
use p2_ui::tokens::Palette;

/// Kayıtlı değerin okunuşu (Electron'daki üç farklı yazım).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Load {
    /// useResizable: `stored>=min&&stored<=max ? stored : initial`.
    Range,
    /// LibraryPage: `min(max, max(min, Number(stored)||initial))`.
    Clamp,
    /// DocumentsPage: `Number(stored)||initial` (sınır yalnız sürüklerken).
    Any,
}

/// Bir panel boyutu: anahtar, başlangıç, sınırlar ve yön (`direction`: sağ kenar tutamacı +1,
/// sol kenar tutamacı −1; dikeyde aşağı +1).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub key: &'static str,
    pub initial: f32,
    pub min: f32,
    pub max: f32,
    pub direction: f32,
    pub load: Load,
}

/// Workspace.tsx `useResizable('p2:sidebar-width',252,190,430,1)` — kanal kenar çubuğu.
pub const SIDEBAR: Spec = Spec { key: "p2:sidebar-width", initial: 252., min: 190., max: 430., direction: 1., load: Load::Range };
/// ChannelContent.tsx `useResizable('p2:thread-width',360,280,660,-1)`.
pub const THREAD: Spec = Spec { key: "p2:thread-width", initial: 360., min: 280., max: 660., direction: -1., load: Load::Range };
/// GroupChat.tsx `useResizable('p2:group-thread-width',360,280,660,-1)`.
pub const GROUP_THREAD: Spec = Spec { key: "p2:group-thread-width", initial: 360., min: 280., max: 660., direction: -1., load: Load::Range };
/// ChannelContent.tsx `useResizable('p2:voice-chat-width',430,300,980,-1)` — sesli oda sohbeti.
pub const VOICE_CHAT: Spec = Spec { key: "p2:voice-chat-width", initial: 430., min: 300., max: 980., direction: -1., load: Load::Range };
/// ProfilePanel.tsx `useResizable('p2:profile-width',340,280,580,-1)`.
pub const PROFILE: Spec = Spec { key: "p2:profile-width", initial: 340., min: 280., max: 580., direction: -1., load: Load::Range };
/// DocumentsPage.tsx `p2-documents-sidebar-width` (280; sürüklerken 220–480).
pub const DOCUMENTS_SIDEBAR: Spec = Spec { key: "p2-documents-sidebar-width", initial: 280., min: 220., max: 480., direction: 1., load: Load::Any };
/// LibraryPage.tsx `p2.library.side` (250; 190–440).
pub const LIBRARY_SIDE: Spec = Spec { key: "p2.library.side", initial: 250., min: 190., max: 440., direction: 1., load: Load::Clamp };

struct Drag {
    key: &'static str,
    /// Sürükleme ekseni boyunca başlangıç imleç konumu (pencere px).
    start: f32,
    start_value: f32,
    spec: Spec,
}

#[derive(Default)]
struct State {
    loaded: bool,
    values: HashMap<String, f32>,
    drag: Option<Drag>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.get_or_insert_with(State::default);
    if !state.loaded {
        state.loaded = true;
        // Önce kayıtlı değerler; bu arada bellekte değişen (henüz sürüklenmiş) değerler üstte kalır.
        let mut stored = p2_client_core::store::load_layout();
        stored.extend(state.values.drain());
        state.values = stored;
    }
    f(state)
}

/// Panelin geçerli boyutu (kayıtlı değer, yoksa başlangıç).
pub fn value(spec: &Spec) -> f32 {
    with_state(|s| {
        let stored = s.values.get(spec.key).copied();
        match (spec.load, stored) {
            (_, None) => spec.initial,
            (Load::Range, Some(v)) if v >= spec.min && v <= spec.max => v,
            (Load::Range, Some(_)) => spec.initial,
            (Load::Clamp, Some(v)) => (if v == 0. { spec.initial } else { v }).clamp(spec.min, spec.max),
            (Load::Any, Some(v)) => if v == 0. { spec.initial } else { v },
        }
    })
}

/// Bu anahtar şu an sürükleniyor mu.
pub fn dragging(key: &str) -> bool {
    with_state(|s| s.drag.as_ref().is_some_and(|d| d.key == key))
}

/// Herhangi bir panel sürükleniyor mu (`body.resizing`: bütün tutamaç çizgileri `accent-hover`).
pub fn any_dragging() -> bool {
    with_state(|s| s.drag.is_some())
}

fn start(spec: Spec, at: f32) {
    let start_value = value(&spec);
    with_state(|s| s.drag = Some(Drag { key: spec.key, start: at, start_value, spec }));
    crate::trace(&format!("boyutlandırma: başladı {} = {start_value}", spec.key));
}

fn update(at: f32) -> Option<f32> {
    with_state(|s| {
        let d = s.drag.as_ref()?;
        let v = (d.start_value + d.spec.direction * (at - d.start)).clamp(d.spec.min, d.spec.max);
        let key = d.key;
        s.values.insert(key.to_string(), v);
        Some(v)
    })
}

fn finish() {
    let done = with_state(|s| s.drag.take().map(|d| (d.key, s.values.get(d.key).copied().unwrap_or(d.start_value))));
    if let Some((key, v)) = done {
        p2_client_core::store::save_layout_value(key, v);
        crate::trace(&format!("boyutlandırma: bitti {key} = {v}"));
    }
}

/// Pencere düzeyinde sürükleme izleyici: `active` iken her karede fare hareketi/bırakma
/// dinleyicisi kaydeder ve imleci pencerenin tamamında `cursor` yapar. Tutamacın (ya da onu
/// çizen öğenin) içine konur; boyut kaplamaz.
pub fn tracker(
    active: bool,
    cursor: CursorStyle,
    on_move: impl Fn(Point<Pixels>, &mut Window, &mut App) + 'static,
    on_end: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    if !active {
        return div().into_any_element();
    }
    let on_move = Rc::new(on_move);
    let on_end = Rc::new(on_end);
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            window.set_window_cursor_style(cursor);
            let (mv, end) = (on_move.clone(), on_end.clone());
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                // Tuş pencere dışında bırakıldıysa (mouseup kaçtı) sürükleme biter.
                if e.pressed_button != Some(MouseButton::Left) {
                    end(window, cx);
                } else {
                    mv(e.position, window, cx);
                }
                window.refresh();
            });
            let end = on_end.clone();
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                    end(window, cx);
                    window.refresh();
                }
            });
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}

/// Tutamacın kenarı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Left,
    Right,
}

/// `.resize-handle.left|right` (direct.css): 8px, kenarın 4px dışına taşar, z-index 30, `col-resize`;
/// ortada 2px yuvarlak çizgi — boşta saydam, üzerine gelince `accent-soft-2`, sürükleme sürerken
/// (`body.resizing`) bütün tutamaçlarda `accent-hover`; `title` = etiket. Panelin **son** çocuğu
/// olarak eklenmeli (CSS z-index: içeriğin üstünde çizilir).
pub fn handle(id: &'static str, spec: Spec, edge: Edge, label: &'static str, p: &Palette) -> AnyElement {
    let resizing = any_dragging();
    let (hover_line, drag_line) = (p.accent_soft_2, p.accent_hover);
    div()
        .id(id)
        .group(id)
        .absolute()
        .top_0()
        .bottom_0()
        .when(edge == Edge::Left, |s| s.left(px(-4.)))
        .when(edge == Edge::Right, |s| s.right(px(-4.)))
        .w(px(8.))
        .cursor(CursorStyle::ResizeColumn)
        .occlude()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(3.))
                .w(px(2.))
                .rounded(px(2.))
                .when(resizing, |s| s.bg(drag_line))
                .when(!resizing, |s| s.group_hover(id, move |s| s.bg(hover_line))),
        )
        // `title` ipucu (yerel ipucu görünümü).
        .tooltip(move |window, cx| p2_ui::controls::tooltip(label, window, cx))
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            start(spec, f32::from(e.position.x));
            window.refresh();
        })
        .child(tracker(dragging(spec.key), CursorStyle::ResizeColumn, |pos, _, _| {
            update(f32::from(pos.x));
        }, |_, _| finish()))
        .into_any_element()
}

/// Yalnız `col-resize` imleci ve pencere düzeyinde izleme; görünümü çağıran çizer
/// (`.documents-sidebar-resizer`, `.library-resizer`: üzerine gelince/sürüklerken tüm şerit renkli).
pub fn strip(id: &'static str, spec: Spec) -> gpui_kit::Stateful<gpui_kit::Div> {
    let active = dragging(spec.key);
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .cursor(CursorStyle::ResizeColumn)
        .occlude()
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            start(spec, f32::from(e.position.x));
            window.refresh();
        })
        .child(tracker(active, CursorStyle::ResizeColumn, |pos, _, _| {
            update(f32::from(pos.x));
        }, |_, _| finish()))
}

