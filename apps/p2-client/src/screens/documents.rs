//! Dokümanlar — DocumentsPage.tsx: klasörler, sayfa ve tuval dokümanları, paylaşım.
//!
//! Sayfa: A4 kâğıt görünümünde zengin metin; düzenlerken satır başına bir blok ve sade biçim
//! (`# başlık`, `**kalın**`…, bkz. `p2_client_core::documents`). Electron'un ürettiği HTML'in
//! alt kümesi kayıpsız gidip gelir; alt kümenin dışında biçim içeren sayfa salt okunur açılır.
//! Tuval: kaydır/yakınlaştır, düğümü sürükle, ekle/sil, çift tıkla metni düzenle, bağlantılar.
//! Değişiklikler 600 ms sonra kaydedilir; başkasının düzenlemesi gelince yeniden yüklenir.

use std::collections::HashSet;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Bounds, Context, Entity, FontStyle, FontWeight, HighlightStyle, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, Point, ScrollEvent, SharedString, StyledText, Subscription, UnderlineStyle, Window, canvas, div, point, px, rgb};
use p2_client_core::{Align, Api, ApiError, Block, BlockKind, CanvasContent, CanvasNode, DocPermissions, DocumentFolder, Member, ProjectDocument, blocks_to_html, blocks_to_markup, html_to_blocks, markup_to_blocks, page_bodies, set_page_bodies};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, fs, radius};
use p2_ui::P2Text as _;
use p2_ui::{Entrance, P2Motion as _};
use serde_json::{Map, Value, json};

use super::{submit_on_enter, text_input, value};

#[path = "documents_canvas.rs"]
mod canvas_view;
#[path = "documents_paged.rs"]
mod paged;

use canvas_view::{CanvasView, tool_grid_icon as tool_grid_mini};
use crate::task;

pub struct DocumentsPage {
    api: Arc<Api>,
    org: String,
    project: String,
    project_name: String,
    me: String,
    _members: Vec<Member>,
    folders: Vec<DocumentFolder>,
    documents: Vec<ProjectDocument>,
    collapsed: HashSet<String>,
    doc: Option<ProjectDocument>,
    title: Entity<InputState>,
    /// Sayfa: düzenleme kipinde sayfa başına bir alan; `None` iken okuma görünümü.
    pages: Option<Vec<Entity<TextareaState>>>,
    lossless: bool,
    canvas: Option<CanvasView>,
    save_epoch: u64,
    saving: bool,
    dirty: bool,
    /// Paylaşım penceresi açık mı + yetki verisi (yüklenince).
    sharing: bool,
    share: Option<DocPermissions>,
    share_error: Option<String>,
    pending_share: bool,
    confirm_doc: Option<ProjectDocument>,
    /// Bir kez kaydedildi mi ("Otomatik kaydedilir" → "Kaydedildi").
    saved_once: bool,
    save_notify: bool,
    active_page: usize,
    page_scroll: gpui_kit::ScrollHandle,
    tools_scroll: gpui_kit::ScrollHandle,
    focus: gpui_kit::FocusHandle,
    /// P2_AUTOPILOT_DOCS_DEMO: sahte veri; sunucuya hiçbir şey yazılmaz.
    demo: bool,
    error: Option<(String, usize)>,
    search: Entity<InputState>,
    loading: bool,
    creating_menu: bool,
    sidebar_hidden: bool,
    /// Sağ tık / ⋯ menüsü: doküman kimliği + pencere konumu.
    /// Konum `None`: ⋯ düğmesine bağlı (çizimde pencere genişliğinden hesaplanır).
    doc_menu: Option<(String, Option<Point<Pixels>>)>,
    /// Klasör adı penceresi: (düzenlenen klasör, ad alanı).
    folder_dialog: Option<(Option<DocumentFolder>, Entity<InputState>)>,
    confirm_folder: Option<DocumentFolder>,
    _subs: Vec<Subscription>,
}

fn parse_color(s: Option<&str>) -> Option<Hsla> {
    let hex = s?.trim().strip_prefix('#')?;
    let v = match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok()?,
        3 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
            (r * 17) << 16 | (g * 17) << 8 | (b * 17)
        }
        _ => return None,
    };
    Some(rgb(v).into())
}


/// `toLocaleDateString('tr-TR')`: GG.AA.YYYY (yerel saat).
pub(super) fn tr_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso).map(|d| d.with_timezone(&chrono::Local).format("%d.%m.%Y").to_string()).unwrap_or_default()
}

/// Türkçe büyük harf (CSS `text-transform: uppercase` + `lang=tr`: i → İ, ı → I).
pub(super) fn tr_upper(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'i' => "İ".to_string(),
            'ı' => "I".to_string(),
            c => c.to_uppercase().to_string(),
        })
        .collect()
}

/// SVG işaretlemesini (renk `black`, alfa maskesi) bir kez sızdırıp önbellekler: `svg().data(..)` 'static ister.
pub(super) fn svg_mark(view_box: &str, body: &str, stroke: f32, round: bool, fill: bool) -> &'static [u8] {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let key = format!("{view_box}|{body}|{stroke}|{round}|{fill}");
    let mut map = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    if let Some(d) = map.get(&key) {
        return d;
    }
    let caps = if round { r#" stroke-linecap="round" stroke-linejoin="round""# } else { "" };
    let paint = if fill { r#"fill="black""#.to_string() } else { format!(r#"fill="none" stroke="black" stroke-width="{stroke}"{caps}"#) };
    let markup = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}" {paint}>{body}</svg>"#);
    let data: &'static [u8] = Box::leak(markup.into_bytes().into_boxed_slice());
    map.insert(key, data);
    data
}

/// Çizgi ikonu öğesi (renk `.text_color`).
pub(super) fn line_icon(view_box: &str, body: &str, size: f32, stroke: f32, round: bool, color: Hsla) -> gpui_kit::Svg {
    gpui_kit::svg().data(svg_mark(view_box, body, stroke, round, false)).size(px(size)).flex_shrink_0().text_color(color)
}

/// DocumentsPage.tsx `DocumentIcon`: sayfa ya da tuval.
pub(super) fn doc_icon(kind: &str, size: f32, stroke: f32, round: bool, color: Hsla) -> gpui_kit::Svg {
    if kind == "PAGE" {
        line_icon("0 0 24 24", r#"<path d="M7 3.5h7l4 4V20H7z"/><path d="M14 3.5V8h4M10 12h5M10 15.5h5"/>"#, size, stroke, round, color)
    } else {
        line_icon("0 0 24 24", r#"<rect x="3.5" y="4" width="17" height="16" rx="2.5"/><path d="m6.5 16 3.5-4 3 3 2.2-2.5 2.8 3.5M8 8.5h.01"/>"#, size, stroke, round, color)
    }
}

/// `.documents-sidebar>header>button`: 32×32, r 8, `border`, `bg-3`; ikon 15px, çizgi 1,8 yuvarlak uç.
/// Üzerine gelince `accent-soft` zemin, `border-strong`, `text-strong`.
fn head_button(id: &'static str, body: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (hbg, hbd, htx) = (p.accent_soft, p.border_strong, p.text_strong);
    div()
        .id(id)
        .size(px(32.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(radius::SM)
        .border_1()
        .border_color(p.border)
        .bg(p.bg_3)
        .text_color(p.text)
        .cursor_pointer()
        .group(id)
        .hover(move |s| s.bg(hbg).border_color(hbd).text_color(htx))
        .child(line_icon("0 0 20 20", body, 15., 1.8, true, p.text).group_hover(id, move |s| s.text_color(htx)))
}

impl DocumentsPage {
    pub fn new(api: Arc<Api>, org: String, project: String, project_name: String, me: String, members: Vec<Member>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = text_input(window, cx, "Adsız doküman");
        let subs = vec![cx.subscribe_in(&title, window, |this, _, e: &InputEvent, window, cx| {
            if matches!(e, InputEvent::Change) && this.doc.as_ref().is_some_and(|d| d.writable() && d.title != value(&this.title, cx)) {
                this.schedule_save(window, cx);
            }
        })];
        let mut page = Self {
            api,
            org,
            project,
            project_name,
            me,
            _members: members,
            folders: Vec::new(),
            documents: Vec::new(),
            collapsed: HashSet::new(),
            doc: None,
            title,
            pages: None,
            lossless: true,
            canvas: None,
            save_epoch: 0,
            saving: false,
            dirty: false,
            sharing: false,
            share: None,
            share_error: None,
            pending_share: false,
            confirm_doc: None,
            saved_once: false,
            save_notify: false,
            active_page: 0,
            page_scroll: gpui_kit::ScrollHandle::new(),
            tools_scroll: gpui_kit::ScrollHandle::new(),
            focus: cx.focus_handle(),
            demo: false,
            error: None,
            search: text_input(window, cx, "Doküman ara"),
            loading: true,
            creating_menu: false,
            sidebar_hidden: false,
            doc_menu: None,
            folder_dialog: None,
            confirm_folder: None,
            _subs: subs,
        };
        let search = page.search.clone();
        page._subs.push(cx.subscribe_in(&search, window, |_, _, _: &InputEvent, _, cx| cx.notify()));
        page.load(window, cx);
        page
    }

    /// Yeni klasör / yeniden adlandır penceresi (Electron: `prompt()`).
    fn ask_folder(&mut self, folder: Option<DocumentFolder>, window: &mut Window, cx: &mut Context<Self>) {
        let input = text_input(window, cx, "Klasör adı");
        if let Some(f) = &folder {
            let name = f.name.clone();
            input.update(cx, |i, cx| i.set_value(name, window, cx));
        }
        input.update(cx, |i, cx| i.focus(window, cx));
        self._subs.push(submit_on_enter(&input, window, cx, |this, window, cx| this.save_folder(window, cx)));
        self.folder_dialog = Some((folder, input));
        cx.notify();
    }

    fn save_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((folder, input)) = self.folder_dialog.take() else { return };
        let name = value(&input, cx).trim().to_string();
        cx.notify();
        if name.is_empty() || folder.as_ref().is_some_and(|f| f.name == name) {
            return;
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        match folder {
            None => task::run(window, cx, move || api.create_document_folder(&org, &project, &name), |this, result, _, cx| match result {
                Ok(f) => {
                    this.collapsed.remove(&f.id);
                    this.folders.push(f);
                    cx.notify();
                }
                Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Klasör oluşturulamadı." } else { &e.message }),
            }),
            Some(f) => task::run(window, cx, move || api.rename_document_folder(&org, &project, &f.id, &name), |this, result, _, cx| match result {
                Ok(saved) => {
                    if let Some(x) = this.folders.iter_mut().find(|x| x.id == saved.id) {
                        *x = saved;
                    }
                    cx.notify();
                }
                Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Klasör yeniden adlandırılamadı." } else { &e.message }),
            }),
        }
    }

    fn remove_folder(&mut self, folder: DocumentFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm_folder = None;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.delete_document_folder(&org, &project, &folder.id).map(|_| folder.id), |this, result, _, cx| match result {
            Ok(id) => {
                this.folders.retain(|f| f.id != id);
                for d in &mut this.documents {
                    if d.folder_id.as_deref() == Some(id.as_str()) {
                        d.folder_id = None;
                    }
                }
                cx.notify();
            }
            Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Klasör silinemedi." } else { &e.message }),
        });
    }

    fn fail(&mut self, e: ApiError, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((if e.message.is_empty() { "Bir hata oluştu.".into() } else { e.message }, epoch));
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // P2_AUTOPILOT_DOCS_DEMO=<json|yol>: { documents, folders } — parite ekranları için sahte liste (sunucuya yazmaz).
        if let Some(demo) = std::env::var("P2_AUTOPILOT_DOCS_DEMO").ok().and_then(|v| if v.trim_start().starts_with('{') { Some(v) } else { std::fs::read_to_string(v).ok() }).and_then(|v| serde_json::from_str::<Value>(&v).ok()) {
            self.demo = true;
            self.documents = serde_json::from_value(demo["documents"].clone()).unwrap_or_default();
            self.folders = serde_json::from_value(demo["folders"].clone()).unwrap_or_default();
            self.loading = false;
            crate::trace(&format!("dokümanlar: {} doküman (demo)", self.documents.len()));
            if let Some(first) = self.documents.first().map(|d| d.id.clone()) {
                self.open(first, window, cx);
            }
            self.autopilot(window, cx);
            return cx.notify();
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || (api.documents(&org, &project), api.document_folders(&org, &project)), |this, (docs, folders), window, cx| {
            match docs {
                Ok(docs) => {
                    crate::trace(&format!("dokümanlar: {} doküman", docs.len()));
                    this.documents = docs;
                }
                Err(e) => this.fail(e, cx),
            }
            if let Ok(f) = folders {
                this.folders = f;
            }
            this.loading = false;
            // Electron gibi: seçili doküman yoksa (ya da silindiyse) ilk doküman açılır.
            let keep = this.doc.as_ref().is_some_and(|d| this.documents.iter().any(|x| x.id == d.id));
            if !keep {
                this.doc = None;
                this.canvas = None;
                this.pages = None;
                if std::env::var("P2_AUTOPILOT_DOC").is_err() {
                    if let Some(first) = this.documents.first().map(|d| d.id.clone()) {
                        this.open(first, window, cx);
                    }
                }
            }
            this.autopilot(window, cx);
            cx.notify();
        });
    }

    /// Otomatik pilot: P2_AUTOPILOT_DOC=<başlık> o dokümanı açar; P2_AUTOPILOT_DOC_APPEND=<satır>
    /// sayfanın sonuna satır ekleyip kaydeder; P2_AUTOPILOT_NODE=<metin> tuvale not ekler.
    fn autopilot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if let Ok(title) = std::env::var("P2_AUTOPILOT_DOC") {
            if let Some(id) = self.documents.iter().find(|d| d.title == title).map(|d| d.id.clone()) {
                self.open(id, window, cx);
            }
        }
    }

    /// ⋯ düğmesi: Electron menüyü düğmenin sağ kenarından, alt kenarın 5px altından açar
    /// (düğme üst çubukta: sağdan 13px, başlık çubuğu 38 + (56−30)/2).
    fn open_more_menu(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.doc_menu = Some((id, None));
        cx.notify();
    }

    fn autopilot_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // P2_AUTOPILOT_DOCS_MENU: yalnız görünüm açan kancalar (parite ekranları).
        if let Ok(which) = std::env::var("P2_AUTOPILOT_DOCS_MENU") {
            match which.as_str() {
                "more" => {
                    if let Some(id) = self.doc.as_ref().map(|d| d.id.clone()) {
                        self.open_more_menu(id, window, cx);
                    }
                }
                "create" => self.creating_menu = true,
                "share" => self.open_share(window, cx),
                // Electron adımı kaydırma alanının ortasına sağ tıklar: konum ilk tuval çiziminde (ölçüler belliyken).
                "canvas" => {
                    if let Some(c) = &mut self.canvas {
                        c.pending_menu = true;
                    }
                }
                _ => {}
            }
            cx.notify();
        }
        // P2_AUTOPILOT_CANVAS_SELECT=<id,id|all>[;x,y,w,h]: yalnız görünüm — düğümleri seçer, isteğe bağlı
        // dünya koordinatlı seçim kutusunu gösterir (seçim vurgusu ekran görüntüleri için; veri yazmaz).
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_CANVAS_SELECT") {
            let (ids, rect) = spec.split_once(';').unwrap_or((spec.as_str(), ""));
            if let Some(c) = &mut self.canvas {
                c.selection = if ids == "all" { c.content.nodes.iter().map(|n| n.id.clone()).collect() } else { ids.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect() };
                let r: Vec<f32> = rect.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                if let [x, y, w, h] = r[..] {
                    c.marquee = Some((x, y, w, h));
                }
                c.format_suppressed = true;
            }
            cx.notify();
        }
        // P2_AUTOPILOT_CANVAS_WIRE=<bağlantı id>[;yakınlık]: yalnız görünüm — bağlantıyı seçer (tutamaçlar +
        // araç çubuğu ekran görüntüleri için), isteğe bağlı yakınlaştırma; veri yazmaz.
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_CANVAS_WIRE") {
            let (id, zoom) = spec.split_once(';').unwrap_or((spec.as_str(), ""));
            if let Some(c) = &mut self.canvas {
                if !id.is_empty() {
                    c.selection.clear();
                    c.selected_wire = Some(id.to_string());
                }
                if let Ok(z) = zoom.trim().parse::<f32>() {
                    c.zoom = z.clamp(0.2, 3.);
                }
            }
            cx.notify();
        }
        if let Ok(line) = std::env::var("P2_AUTOPILOT_DOC_APPEND") {
            self.edit_pages(window, cx);
            if let Some(last) = self.pages.as_ref().and_then(|p| p.last().cloned()) {
                last.update(cx, |t, cx| {
                    let text = format!("{}\n{}", t.value(), line);
                    t.set_value(text, window, cx);
                });
                self.schedule_save(window, cx);
            }
        }
        if let Ok(text) = std::env::var("P2_AUTOPILOT_NODE") {
            self.canvas_add("BOX", None, window, cx);
            if let Some(c) = &mut self.canvas {
                if let Some(n) = c.content.nodes.last_mut() {
                    n.text = Some(text);
                    n.color = Some("#ffd166".into());
                }
            }
            self.schedule_save(window, cx);
        }
    }

    fn open(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.share = None;
        self.sharing = false;
        if self.demo {
            if let Some(doc) = self.documents.iter().find(|d| d.id == id).cloned() {
                crate::trace(&format!("doküman: açıldı «{}» ({}, {})", doc.title, doc.kind, doc.access));
                self.show(doc, window, cx);
                self.autopilot_doc(window, cx);
            }
            return;
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.document(&org, &project, &id), |this, result, window, cx| match result {
            Ok(doc) => {
                crate::trace(&format!("doküman: açıldı «{}» ({}, {})", doc.title, doc.kind, doc.access));
                this.show(doc, window, cx);
                this.autopilot_doc(window, cx);
                if std::mem::take(&mut this.pending_share) {
                    this.open_share(window, cx);
                }
            }
            Err(e) => this.fail(e, cx),
        });
    }

    fn show(&mut self, doc: ProjectDocument, window: &mut Window, cx: &mut Context<Self>) {
        let title = doc.title.clone();
        self.title.update(cx, |i, cx| i.set_value(title, window, cx));
        self.pages = None;
        self.dirty = false;
        self.lossless = page_bodies(&doc.content).iter().all(|b| html_to_blocks(b).1);
        self.active_page = 0;
        self.saved_once = false;
        self.canvas = (doc.kind == "CANVAS").then(|| CanvasView::new(CanvasContent::from_value(&doc.content)));
        self.doc = Some(doc);
        cx.notify();
    }

    /// Sayfa düzenleme kipine geç: her sayfa için biçimli metin alanı.
    fn edit_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        if !doc.writable() || !self.lossless || self.pages.is_some() {
            return;
        }
        let bodies = page_bodies(&doc.content);
        let mut editors = Vec::new();
        for body in bodies {
            let markup = blocks_to_markup(&html_to_blocks(&body).0);
            let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(12, 400).placeholder("Yazmaya başla… (# başlık, **kalın**, *italik*, __altı çizili__, > alıntı)"));
            editor.update(cx, |t, cx| t.set_value(markup, window, cx));
            self._subs.push(cx.subscribe_in(&editor, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    this.schedule_save(window, cx);
                }
            }));
            editors.push(editor);
        }
        self.pages = Some(editors);
        cx.notify();
    }

    fn add_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_pages(window, cx);
        if let Some(pages) = &mut self.pages {
            let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(12, 400).placeholder("Yeni sayfa"));
            self._subs.push(cx.subscribe_in(&editor, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    this.schedule_save(window, cx);
                }
            }));
            pages.push(editor);
        }
        self.schedule_save(window, cx);
    }

    /// Şu anki içerik (sayfa: düzenleyicilerden HTML; tuval: düğümler).
    fn current_content(&self, cx: &gpui_kit::App) -> Option<Value> {
        let doc = self.doc.as_ref()?;
        if let Some(c) = &self.canvas {
            let mut v = c.content.to_value();
            v["version"] = json!(2);
            return Some(v);
        }
        match &self.pages {
            Some(pages) => {
                let bodies: Vec<String> = pages.iter().map(|p| blocks_to_html(&markup_to_blocks(&p.read(cx).value()))).collect();
                Some(set_page_bodies(&doc.content, &bodies))
            }
            None => Some(doc.content.clone()),
        }
    }

    fn schedule_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dirty = true;
        self.save_epoch += 1;
        let epoch = self.save_epoch;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(600)).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.save_epoch == epoch {
                    this.save(window, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone().filter(|d| d.writable()) else { return };
        let Some(content) = self.current_content(cx) else { return };
        if self.demo {
            if let Some(d) = &mut self.doc {
                d.content = content;
            }
            self.dirty = false;
            self.saved_once = true;
            return cx.notify();
        }
        let title = value(&self.title, cx).trim().to_string();
        let title = if title.is_empty() { "Adsız doküman".to_string() } else { title };
        self.saving = true;
        cx.notify();
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        let epoch = self.save_epoch;
        task::run(window, cx, move || api.update_document(&org, &project, &doc.id, json!({ "title": title, "content": content })), move |this, result, _, cx| {
            this.saving = false;
            match result {
                Ok(saved) => {
                    crate::trace(&format!("doküman: kaydedildi «{}»", saved.title));
                    if let Some(d) = this.documents.iter_mut().find(|d| d.id == saved.id) {
                        d.title = saved.title.clone();
                    }
                    if this.save_epoch == epoch {
                        this.dirty = false;
                    }
                    this.saved_once = true;
                    if std::mem::take(&mut this.save_notify) {
                        crate::toasts::push(p2_ui::controls::ToastTone::Ok, None, "Doküman kaydedildi.");
                    }
                    if let Some(d) = &mut this.doc {
                        d.content = saved.content;
                        d.title = saved.title;
                    }
                }
                Err(e) => this.fail(e, cx),
            }
            cx.notify();
        });
    }

    fn create(&mut self, kind: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        let title = if kind == "PAGE" { "Yeni sayfa" } else { "Yeni tuval" };
        self.creating_menu = false;
        task::run(window, cx, move || api.create_document(&org, &project, title, kind, None), |this, result, window, cx| match result {
            Ok(doc) => {
                crate::trace(&format!("doküman: oluşturuldu ({})", doc.kind));
                let id = doc.id.clone();
                this.documents.insert(0, doc);
                this.open(id, window, cx);
            }
            Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Doküman oluşturulamadı." } else { &e.message }),
        });
    }

    /// Dokümanı siler (onaydan sonra); seçiliyse kalan ilk doküman açılır.
    fn delete_doc(&mut self, doc: ProjectDocument, window: &mut Window, cx: &mut Context<Self>) {
        if doc.access != "OWNER" {
            return;
        }
        let after = |this: &mut Self, id: &str, window: &mut Window, cx: &mut Context<Self>| {
            this.documents.retain(|d| d.id != id);
            if this.doc.as_ref().is_some_and(|d| d.id == id) {
                this.doc = None;
                this.canvas = None;
                this.pages = None;
                if let Some(first) = this.documents.first().map(|d| d.id.clone()) {
                    this.open(first, window, cx);
                }
            }
            cx.notify();
        };
        if self.demo {
            return after(self, &doc.id, window, cx);
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.delete_document(&org, &project, &doc.id).map(|_| doc.id), move |this, result, window, cx| match result {
            Ok(id) => after(this, &id, window, cx),
            Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Doküman silinemedi." } else { &e.message }),
        });
    }

    fn open_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        self.sharing = true;
        self.share = None;
        self.share_error = None;
        cx.notify();
        if self.demo {
            self.share = std::env::var("P2_AUTOPILOT_DOCS_PERMS").ok().and_then(|v| serde_json::from_str(&v).ok());
            return;
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.document_permissions(&org, &project, &doc.id), |this, result, _, cx| {
            match result {
                Ok(p) => this.share = Some(p),
                Err(e) => this.share_error = Some(if e.message.is_empty() { "Paylaşım bilgileri alınamadı.".into() } else { e.message }),
            }
            cx.notify();
        });
    }

    fn set_access(&mut self, user: String, access: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        self.share_error = None;
        if self.demo {
            if let Some(s) = &mut self.share {
                s.permissions.retain(|x| x.user_id != user);
                if access != "NONE" {
                    s.permissions.push(p2_client_core::DocPermission { user_id: user, access: access.into() });
                }
            }
            return cx.notify();
        }
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.set_document_access(&org, &project, &doc.id, &user, access).and_then(|_| api.document_permissions(&org, &project, &doc.id)), |this, result, _, cx| match result {
            Ok(p) => {
                crate::trace(&format!("doküman: paylaşım güncellendi ({} kişi)", p.permissions.len()));
                this.share = Some(p);
                cx.notify();
            }
            Err(e) => {
                this.share_error = Some(if e.message.is_empty() { "Yetki kaydedilemedi.".into() } else { e.message });
                cx.notify();
            }
        });
    }

    /// Gerçek zamanlı: başkası düzenledi/oluşturdu/sildi.
    pub fn on_event(&mut self, event: &str, payload: &Value, window: &mut Window, cx: &mut Context<Self>) {
        if payload["projectId"].as_str().is_some_and(|p| p != self.project) {
            return;
        }
        match event {
            "document:updated" => {
                if payload["editorId"].as_str() == Some(self.me.as_str()) {
                    return;
                }
                let id = payload["documentId"].as_str().unwrap_or_default().to_string();
                crate::trace("doküman: olay document:updated");
                // Açık doküman yerelde değişmemişse yeniden yüklenir (değişmişse kendi kaydımız kazanır).
                if self.doc.as_ref().is_some_and(|d| d.id == id) && !self.dirty && self.pages.is_none() {
                    self.open(id, window, cx);
                } else {
                    self.load(window, cx);
                }
            }
            _ => self.load(window, cx),
        }
    }

    // ------------------------------------------------------------ üst çubuk, menüler, pencereler

    /// `.documents-topbar`: tür simgesi, başlık alanı, erişim hapı, kayıt durumu, paylaş, ⋯ menü.
    fn topbar(&self, doc: &ProjectDocument, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let writable = doc.writable();
        let (kind_bg, kind_fg) = if doc.kind == "PAGE" { (p.hues[8].bg, p.hues[8].fg) } else { (p.accent_soft, p.text_accent) };
        let focused = gpui_kit::Focusable::focus_handle(self.title.read(cx), cx).is_focused(window);
        let (hbd, hbg) = (p.border, p.bg_3);
        let title: AnyElement = if writable {
            div()
                .id("doc-title")
                .flex_1()
                .min_w(px(90.))
                .max_w(px(520.))
                .h(px(29.))
                .rounded(px(6.))
                .border_1()
                .border_color(if focused { p.border } else { gpui_kit::transparent_black() })
                .when(focused, |s| s.bg(p.bg_3))
                .hover(move |s| s.border_color(hbd).bg(hbg))
                .child(Input::new(&self.title).appearance(false).bordered(false).focus_bordered(false).h(px(27.)).px(px(8.)).text_size(fs(14.)).css_weight(720.).text_color(p.text_strong))
                .into_any_element()
        } else {
            div().flex_1().min_w(px(90.)).max_w(px(520.)).h(px(29.)).px(px(9.)).flex().items_center().text_size(fs(14.)).css_weight(720.).text_color(p.text_strong).child(doc.title.clone()).into_any_element()
        };
        let (pill_border, pill_text, pill_label) = match doc.access.as_str() {
            "OWNER" => (p.border_accent, p.text_accent, "Sahibi sensin"),
            "WRITE" => (p.success_border, p.success_text, "Düzenleyebilir"),
            _ => (p.border, p.text_3, "Salt okunur"),
        };
        let status = if self.saving || self.dirty {
            "Kaydediliyor…"
        } else if self.saved_once {
            "Kaydedildi"
        } else {
            "Otomatik kaydedilir"
        };
        let (dsoft, dtext, dborder) = (p.danger_soft, p.danger_text, p.danger_border);
        let more_id = doc.id.clone();
        div()
            .h(px(56.))
            .flex_shrink_0()
            .px(px(13.))
            .when(self.sidebar_hidden, |s| s.pl(px(52.)))
            .flex()
            .items_center()
            .gap(px(9.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.bg_1)
            .child(div().size(px(29.)).flex_shrink_0().flex().items_center().justify_center().rounded(px(7.)).bg(kind_bg).child(doc_icon(&doc.kind, 18., 1.7, true, kind_fg)))
            .child(title)
            .child(div().ml_auto().h(px(19.)).px(px(7.)).flex().items_center().rounded(px(99.)).border_1().border_color(pill_border).text_size(fs(8.)).css_weight(750.).tracking(0.04).text_color(pill_text).child(tr_upper(pill_label)))
            .child(div().text_size(fs(9.)).text_color(p.text_3).child(status))
            .when(doc.access == "OWNER", |s| {
                s.child(
                    div()
                        .id("doc-share-button")
                        .h(px(30.))
                        .px(px(11.))
                        .flex()
                        .items_center()
                        .rounded(radius::SM)
                        .bg(p.accent_grad.background())
                        .shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong, p.inset_top]))
                        .text_size(fs(10.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.text_on_accent)
                        .cursor_pointer()
                        .child("Kişilerle paylaş")
                        .on_click(cx.listener(|this, _, window, cx| this.open_share(window, cx))),
                )
            })
            .child(
                div()
                    .id("doc-more")
                    .group("doc-more")
                    .size(px(30.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::SM)
                    .border_1()
                    .border_color(p.border)
                    .bg(p.bg_3)
                    .cursor_pointer()
                    .hover(move |s| s.bg(dsoft).border_color(dborder))
                    .tooltip(|window, cx| p2_ui::controls::tooltip("Doküman menüsü", window, cx))
                    .child(gpui_kit::svg().data(svg_mark("0 0 20 20", r#"<circle cx="5" cy="10" r="1.25"/><circle cx="10" cy="10" r="1.25"/><circle cx="15" cy="10" r="1.25"/>"#, 0., false, true)).size(px(16.)).text_color(p.text).group_hover("doc-more", move |s| s.text_color(dtext)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_more_menu(more_id.clone(), window, cx);
                    })),
            )
            .into_any_element()
    }

    /// `.documents-file-menu`: Adlandır / Kaydet / Paylaş, klasöre taşı, tuvalde ızgara ayarları, Sil.
    fn doc_menu_layer(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, at) = self.doc_menu.clone()?;
        let at = at.unwrap_or_else(|| point(window.viewport_size().width - px(13.), px(38. + 13. + 30. + 5.)));
        let d = self.documents.iter().find(|d| d.id == id)?.clone();
        let vp = window.viewport_size();
        let canvas = d.kind == "CANVAS";
        let x = f32::from(at.x).min(f32::from(vp.width) - 242.).max(8.);
        let y = f32::from(at.y).min(f32::from(vp.height) - if canvas { 470. } else { 230. }).max(8.);
        let writable = d.access != "READ";
        let owner = d.access == "OWNER";
        let (hbg, htx, dsoft) = (p.accent_soft, p.text_strong, p.danger_soft);
        let item = |id: &'static str, label: &'static str, kbd: Option<&'static str>, danger: bool| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(9.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(5.))
                .text_size(fs(10.))
                .text_color(if danger { p.danger_text } else { p.text })
                .cursor_pointer()
                .hover(move |s| if danger { s.bg(dsoft) } else { s.bg(hbg).text_color(htx) })
                .child(label)
                .when_some(kbd, |s, k| s.child(div().text_size(fs(8.)).font_family(p2_ui::tokens::FONT_MONO).text_color(rgb(0x727d9e)).child(k)))
        };
        let hr = || div().w_full().h(px(1.)).my(px(3.)).bg(p.border_subtle);
        let kicker = |t: &str| div().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(tr_upper(t));
        let mut menu = div()
            .id("doc-file-menu")
            .w(px(if canvas { 234. } else { 186. }))
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("doc-file-menu", Entrance::ScaleIn, Duration::ZERO);
        if writable {
            let (i1, i2) = (d.id.clone(), d.clone());
            menu = menu
                .child(item("dm-rename", "Adlandır", Some("F2"), false).on_click(cx.listener(move |this, _, window, cx| {
                    this.doc_menu = None;
                    if this.doc.as_ref().map(|x| x.id.as_str()) != Some(i1.as_str()) {
                        this.open(i1.clone(), window, cx);
                    }
                    this.title.update(cx, |t, cx| t.focus(window, cx));
                    cx.notify();
                })))
                .child(item("dm-save", "Kaydet", Some("Ctrl+S"), false).on_click(cx.listener(move |this, _, window, cx| {
                    this.doc_menu = None;
                    if this.doc.as_ref().is_some_and(|x| x.id == i2.id) {
                        this.save_notify = true;
                        this.save(window, cx);
                    }
                    cx.notify();
                })));
        }
        if owner {
            let i3 = d.id.clone();
            menu = menu.child(item("dm-share", "Paylaş", None, false).on_click(cx.listener(move |this, _, window, cx| {
                this.doc_menu = None;
                if this.doc.as_ref().map(|x| x.id.as_str()) != Some(i3.as_str()) {
                    this.pending_share = true;
                    this.open(i3.clone(), window, cx);
                } else {
                    this.open_share(window, cx);
                }
                cx.notify();
            })));
        }
        if writable {
            let (bd, hv) = (p.border, p.bg_hover);
            let mv = |id: String, label: String, active: bool| {
                div()
                    .id(SharedString::from(id))
                    .h(px(29.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(if active { bd } else { gpui_kit::transparent_black() })
                    .when(active, |s| s.bg(hv))
                    .text_size(fs(9.))
                    .truncate()
                    .cursor_pointer()
                    .hover(move |s| s.border_color(bd).bg(hv))
                    .child(label)
            };
            let did = d.id.clone();
            let mut sec = div().p(px(4.)).flex().flex_col().gap(px(3.)).child(div().px(px(5.)).py(px(3.)).child(kicker("Klasöre taşı"))).child(mv("mv-none".into(), "Klasörsüz".into(), d.folder_id.is_none()).on_click(cx.listener({
                let did = did.clone();
                move |this, _, window, cx| this.move_doc(did.clone(), None, window, cx)
            })));
            for f in &self.folders {
                let (did, fid) = (did.clone(), f.id.clone());
                sec = sec.child(mv(format!("mv-{}", f.id), format!("▰ {}", f.name), d.folder_id.as_deref() == Some(f.id.as_str())).on_click(cx.listener(move |this, _, window, cx| this.move_doc(did.clone(), Some(fid.clone()), window, cx))));
            }
            menu = menu.child(hr()).child(sec);
        }
        if canvas {
            let content = if self.doc.as_ref().is_some_and(|x| x.id == d.id) { self.canvas.as_ref().map(|c| c.content.to_value()).unwrap_or(d.content.clone()) } else { d.content.clone() };
            let grid = content["grid"].as_str().unwrap_or("DOTS").to_string();
            let spacing = content["gridSpacing"].as_f64().unwrap_or(40.);
            let opacity = content["gridOpacity"].as_f64().unwrap_or(0.1);
            let color = content["gridColor"].as_str().unwrap_or("#dfe5f0").to_string();
            let (ab, ah, ats) = (p.border_accent, p.accent_soft, p.text_strong);
            let mut types = div().flex().gap(px(5.));
            for (mode, label) in [("DOTS", "Nokta"), ("LINES", "Çizgi"), ("NONE", "Yok")] {
                let active = grid == mode;
                types = types.child(
                    div()
                        .id(SharedString::from(format!("gt-{mode}")))
                        .flex_1()
                        .h(px(49.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(5.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(if active { p.border_accent } else { p.border })
                        .bg(if active { p.accent_soft } else { p.bg_3 })
                        .text_size(fs(8.))
                        .text_color(if active { p.text_strong } else { p.text_3 })
                        .when(writable, move |s| s.cursor_pointer().hover(move |s| s.border_color(ab).bg(ah).text_color(ats)))
                        .child(tool_grid_mini(mode, p))
                        .child(label)
                        .when(writable, |s| s.on_click(cx.listener(move |this, _, window, cx| this.grid_setting("grid", json!(mode), window, cx)))),
                );
            }
            // Chromium `input[type=range]` + accent-color: 5px iz (dolu kısım vurgu, kalan açık gri), 16px yuvarlak tutamaç.
            // İzin sol kenarı: menü + 6 iç boşluk + 1 kenarlık + 8 + 54 etiket + 7 aralık; genişlik 102.
            let track_left = x + 76.;
            let (accent, rest) = (p.accent, rgb(0xefefef));
            let range_row = |id: &'static str, label: &'static str, value: f64, min: f64, max: f64, step: f64, out: String, apply: fn(f64) -> (&'static str, Value), cx: &mut Context<Self>| {
                let frac = ((value - min) / (max - min)).clamp(0., 1.) as f32;
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(fs(9.))
                    .text_color(p.text_3)
                    .child(div().w(px(54.)).child(label))
                    .child(
                        div()
                            .id(id)
                            .relative()
                            .flex_1()
                            .h(px(16.))
                            .when(writable, |s| s.cursor_pointer())
                            .child(div().absolute().left_0().right_0().top(px(5.5)).h(px(5.)).rounded(px(3.)).bg(rest).child(div().h_full().w(gpui_kit::relative(frac)).rounded(px(3.)).bg(accent)))
                            .child(div().absolute().top_0().left(gpui_kit::relative(frac)).ml(px(-8. * frac * 2. + 0.)).size(px(16.)).rounded_full().bg(accent))
                            .when(writable, |s| {
                                s.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                        let t = ((f32::from(e.position.x) - track_left) / 102.).clamp(0., 1.) as f64;
                                        let v = ((min + t * (max - min)) / step).round() * step;
                                        let (key, value) = apply(v);
                                        this.grid_setting(key, value, window, cx);
                                    }),
                                )
                            }),
                    )
                    .child(div().w(px(34.)).text_right().text_size(fs(8.)).text_color(p.text).child(out))
            };
            let spacing_row = range_row("gr-spacing", "Sıklık", spacing, 10., 100., 5., format!("{spacing:.0}px"), |v| ("gridSpacing", json!(v)), cx);
            let opacity_row = range_row("gr-opacity", "Saydamlık", opacity * 100., 2., 40., 1., format!("%{:.0}", opacity * 100.), |v| ("gridOpacity", json!(v / 100.)), cx);
            let sec = div()
                .pt(px(7.))
                .px(px(8.))
                .pb(px(5.))
                .flex()
                .flex_col()
                .gap(px(9.))
                .child(kicker("Grid ayarları"))
                .child(types)
                .child(div().flex().items_center().gap(px(7.)).text_size(fs(9.)).text_color(p.text_3).child(div().w(px(54.)).child("Renk")).child(div().w(px(32.)).h(px(25.)).p(px(2.)).rounded(px(5.)).border_1().border_color(p.border).bg(p.bg_3).child(div().size_full().rounded(px(2.)).bg(parse_color(Some(&color)).unwrap_or(rgb(0xdfe5f0).into())))))
                .child(spacing_row)
                .child(opacity_row);
            menu = menu.child(hr()).child(sec);
        }
        if owner {
            let i4 = d.clone();
            menu = menu.child(hr()).child(item("dm-delete", "Sil", Some("Delete"), true).on_click(cx.listener(move |this, _, _, cx| {
                this.doc_menu = None;
                this.confirm_doc = Some(i4.clone());
                cx.notify();
            })));
        }
        Some(
            div()
                .id("doc-menu-layer")
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.doc_menu = None;
                        cx.notify();
                    }),
                )
                // p2_enter öğeyi relative yapar: konum dıştaki mutlak sarmalayıcıda.
                .child(div().absolute().left(px(x)).top(px(y)).child(menu))
                .into_any_element(),
        )
    }

    fn grid_setting(&mut self, key: &str, v: Value, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.canvas {
            c.content.extra.insert(key.into(), v);
            self.schedule_save(window, cx);
        }
    }

    fn move_doc(&mut self, id: String, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.doc_menu = None;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        if self.demo {
            if let Some(d) = self.documents.iter_mut().find(|d| d.id == id) {
                d.folder_id = folder;
            }
            return cx.notify();
        }
        task::run(window, cx, move || api.update_document(&org, &project, &id, json!({ "folderId": folder })), |this, result, _, cx| match result {
            Ok(saved) => {
                let moved = saved.folder_id.is_some();
                if let Some(d) = this.documents.iter_mut().find(|d| d.id == saved.id) {
                    d.folder_id = saved.folder_id.clone();
                }
                if let Some(d) = &mut this.doc {
                    if d.id == saved.id {
                        d.folder_id = saved.folder_id;
                    }
                }
                crate::toasts::push(p2_ui::controls::ToastTone::Ok, None, if moved { "Doküman klasöre taşındı." } else { "Doküman klasörsüz alana taşındı." });
                cx.notify();
            }
            Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Doküman taşınamadı." } else { &e.message }),
        });
    }

    /// `.documents-modal-layer` + `.documents-share`: sahip satırı, üye listesi (erişim seçimi), alt not.
    fn share_modal(&self, doc: &ProjectDocument, perms: Option<&DocPermissions>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let avatar_box = |id: &str, name: &str| {
            let hue = p2_ui::components::avatar_hue(id);
            let h = p.hues[hue % p.hues.len()];
            div()
                .size(px(32.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(h.bg, 0.), gpui_kit::linear_color_stop(h.bg2, 1.)))
                .text_color(h.fg)
                .text_size(fs(9.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .child(p2_ui::components::user_initials(name))
        };
        let owner_name = doc.owner.as_ref().map(|o| o.display_name.clone()).unwrap_or_default();
        let mut list = div().id("share-list").flex_1().min_h_0().overflow_y_scroll().pt(px(6.)).px(px(14.)).pb(px(13.)).flex().flex_col();
        match perms {
            None => list = list.child(div().p(px(8.)).text_size(fs(10.)).text_color(p.text_3).child("Kişiler yükleniyor…")),
            Some(perms) => {
                for m in perms.members.iter().filter(|m| m.id != doc.owner_id) {
                    let current = perms.permissions.iter().find(|x| x.user_id == m.id).map(|x| x.access.as_str()).unwrap_or("NONE");
                    let label = match current {
                        "READ" => "Okuyabilir",
                        "WRITE" => "Düzenleyebilir",
                        _ => "Erişim yok",
                    };
                    let next = match current {
                        "NONE" => "READ",
                        "READ" => "WRITE",
                        _ => "NONE",
                    };
                    let user = m.id.clone();
                    list = list.child(
                        div()
                            .py(px(8.))
                            .px(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(div().w(px(34.)).child(avatar_box(&m.id, &m.display_name)))
                            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(fs(10.)).font_weight(FontWeight::BOLD).child(m.display_name.clone())).child(div().text_size(fs(8.)).text_color(p.text_3).child(format!("@{}", m.username))))
                            .child(
                                div()
                                    .id(SharedString::from(format!("acc-{}", m.id)))
                                    .h(px(29.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(p.border)
                                    .bg(p.bg_3)
                                    .text_size(fs(9.))
                                    .text_color(p.text)
                                    .cursor_pointer()
                                    .child(label)
                                    .child(p2_ui::glyphs::ui_stroke("chevron-down", 10., 2.4).text_color(p.text))
                                    .on_click(cx.listener(move |this, _, window, cx| this.set_access(user.clone(), next, window, cx))),
                            ),
                    );
                }
            }
        }
        let card = div()
            .id("doc-share")
            .w(px(510.))
            .max_h(px(650.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("doc-share", Entrance::PopElastic, Duration::ZERO)
            .child(
                div()
                    .h(px(62.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(div().flex_1().flex().flex_col().gap(px(3.)).child(div().text_size(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("Dokümanı paylaş")).child(div().text_size(fs(9.)).text_color(p.text_3).child(doc.title.clone())))
                    .child(div().id("share-x").text_size(fs(22.)).text_color(p.text_3).cursor_pointer().hover(|s| s.opacity(0.8)).child("×").on_click(cx.listener(|this, _, _, cx| {
                        this.sharing = false;
                        cx.notify();
                    }))),
            )
            .child(
                div()
                    .mt(px(12.))
                    .mx(px(14.))
                    .mb(px(5.))
                    .p(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(radius::SM)
                    .border_1()
                    .border_color(p.border)
                    .bg(p.bg_3)
                    .child(div().w(px(34.)).child(avatar_box(&doc.owner_id, &owner_name)))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(fs(10.)).font_weight(FontWeight::BOLD).child(owner_name.clone())).child(div().text_size(fs(8.)).text_color(p.text_3).child("Dosya sahibi")))
                    .child(div().text_size(fs(9.)).font_weight(FontWeight::BOLD).text_color(p.text_accent).child("Tam yetki")),
            )
            .when_some(self.share_error.clone(), |s, e| s.child(div().mx(px(14.)).text_size(fs(9.)).text_color(p.danger_text).child(e)))
            .child(list)
            .child(div().py(px(11.)).px(px(16.)).border_t_1().border_color(p.border_subtle).text_size(fs(9.)).text_color(p.text_3).child("Yalnızca sen paylaşım yetkilerini değiştirebilirsin."));
        div()
            .id("share-layer")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            // Karartma başlık çubuğunun (38px) altından başlar: Electron'da `.titlebar` (z 1000) katmanın
            // üstünde kalır. Kart yine bütün pencereye göre ortalanır (katman `position: fixed; inset: 0`).
            .child(div().absolute().top(px(38.)).left_0().right_0().bottom_0().bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.sharing = false;
                    cx.notify();
                }),
            )
            .child(card)
            .into_any_element()
    }

    /// Onay penceresi (Electron `confirm()` yerine P2 modal kalıbı).
    fn confirm_modal(&self, title: &str, body: String, action: &str, id: &'static str, p: &Palette, window: &Window, cx: &mut Context<Self>, on_yes: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> AnyElement {
        use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, modal, modal_body, modal_foot, modal_head, modal_layer};
        modal_layer(id, true, p)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.confirm_doc = None;
                    this.confirm_folder = None;
                    this.folder_dialog = None;
                    cx.notify();
                }),
            )
            .child(
                modal(ModalSize::Sm, window, p)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .p2_enter(id, Entrance::PopElastic, Duration::ZERO)
                    .child(modal_head(title.to_string(), None, None, p))
                    .child(modal_body().child(div().text_size(font_size::MD).line_height(px(18.)).text_color(p.text_2).child(body)))
                    .child(
                        modal_foot(p)
                            .child(button("cf-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child("Vazgeç").on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_doc = None;
                                this.confirm_folder = None;
                                this.folder_dialog = None;
                                cx.notify();
                            })))
                            .child(button("cf-yes", BtnVariant::Danger, BtnSize::Md, false, false, p).child(action.to_string()).on_click(cx.listener(move |this, _, window, cx| on_yes(this, window, cx)))),
                    ),
            )
            .into_any_element()
    }

    fn folder_modal(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, modal, modal_body, modal_foot, modal_head, modal_layer};
        let (folder, input) = self.folder_dialog.as_ref()?;
        let title = if folder.is_some() { "Klasörün yeni adı" } else { "Yeni klasörün adı" };
        Some(
            modal_layer("folder-dialog", true, p)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.folder_dialog = None;
                        cx.notify();
                    }),
                )
                .child(
                    modal(ModalSize::Sm, window, p)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .p2_enter("folder-dialog", Entrance::PopElastic, Duration::ZERO)
                        .child(modal_head(title, None, None, p))
                        .child(modal_body().child(p2_ui::form::input(input, p2_ui::form::InputKind::P2, p, window, cx)))
                        .child(
                            modal_foot(p)
                                .child(button("fd-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child("Vazgeç").on_click(cx.listener(|this, _, _, cx| {
                                    this.folder_dialog = None;
                                    cx.notify();
                                })))
                                .child(button("fd-yes", BtnVariant::Primary, BtnSize::Md, false, false, p).child("Kaydet").on_click(cx.listener(|this, _, window, cx| this.save_folder(window, cx)))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// `.documents-list>button` / `.documents-folder>button`: 34px ikon sütunu, başlık + sahip · tarih, ◆.
    fn doc_row(&self, d: &ProjectDocument, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let on = self.doc.as_ref().is_some_and(|x| x.id == d.id);
        let id = d.id.clone();
        let menu_id = d.id.clone();
        let hover = p.bg_hover;
        let (kind_bg, kind_fg) = if d.kind == "PAGE" { (p.hues[8].bg, p.hues[8].fg) } else { (p.accent_soft, p.text_accent) };
        let owner = d.owner.as_ref().map(|o| o.display_name.clone()).unwrap_or_default();
        div()
            .id(SharedString::from(format!("doc-{}", d.id)))
            .relative()
            .w_full()
            .p(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(radius::SM)
            .cursor_pointer()
            .text_color(if on { p.text_strong } else { p.text })
            .when(on, |s| s.bg(p.bg_selected).shadow(p2_ui::tokens::shadows(&[p2_ui::tokens::Shadow { inset: true, ..p2_ui::tokens::Shadow::new(2., 0., 0., 0., p.accent_hover) }])))
            .when(!on, move |s| s.hover(move |s| s.bg(hover)))
            .on_click(cx.listener(move |this, _, window, cx| this.open(id.clone(), window, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.doc_menu = Some((menu_id.clone(), Some(e.position)));
                    cx.notify();
                }),
            )
            .child(div().w(px(34.)).flex_shrink_0().child(div().size(px(30.)).flex().items_center().justify_center().rounded(px(7.)).bg(kind_bg).child(doc_icon(&d.kind, 18., 1.7, true, kind_fg))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(fs(11.)).font_weight(FontWeight::BOLD).child(d.title.clone()))
                    .child(div().truncate().text_size(fs(9.)).text_color(p.text_3).child(format!("{owner} · {}", tr_date(&d.updated_at)))),
            )
            .when(d.access == "OWNER", |s| s.child(div().text_size(fs(8.)).text_color(p.accent_hover).child("◆")))
            .into_any_element()
    }

    /// `.documents-folder>header`: katlama oku, ▰ ad, adet; adlandır / sil düğmeleri.
    fn folder_head(&self, key: &str, name: String, count: usize, folder: Option<DocumentFolder>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let collapsed = self.collapsed.contains(key);
        let k = key.to_string();
        let (hover, text, muted) = (p.bg_hover, p.text, p.text_3);
        let small = |id: String, glyph: &'static str| {
            div()
                .id(SharedString::from(id))
                .w(px(25.))
                .h(px(25.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .text_size(fs(9.))
                .text_color(muted)
                .cursor_pointer()
                .hover(move |s| s.bg(hover).text_color(text))
                .child(glyph)
        };
        let gid = SharedString::from(format!("fold-{key}"));
        let body = if collapsed { r#"<path d="m7.5 5.5 4.5 4.5-4.5 4.5"/>"# } else { r#"<path d="m5.5 7.5 4.5 4.5 4.5-4.5"/>"# };
        let mut head = div().h(px(28.)).px(px(3.)).flex().items_center().gap(px(1.)).child(
            div()
                .id(gid.clone())
                .group(gid.clone())
                .flex_1()
                .min_w_0()
                .h(px(25.))
                .px(px(5.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .text_size(fs(9.))
                .text_color(muted)
                .cursor_pointer()
                .hover(move |s| s.bg(hover).text_color(text))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.collapsed.remove(&k) {
                        this.collapsed.insert(k.clone());
                    }
                    cx.notify();
                }))
                .child(line_icon("0 0 20 20", body, 14., 1.8, true, muted).group_hover(gid.clone(), move |s| s.text_color(text)))
                .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::BOLD).child(name))
                .child(div().text_size(fs(8.)).child(count.to_string())),
        );
        if let Some(f) = folder {
            let (f1, f2) = (f.clone(), f.clone());
            head = head
                .child(small(format!("fold-ren-{}", f.id), "✎").on_click(cx.listener(move |this, _, window, cx| this.ask_folder(Some(f1.clone()), window, cx))))
                .child(small(format!("fold-del-{}", f.id), "×").on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_folder = Some(f2.clone());
                    cx.notify();
                })));
        }
        head.into_any_element()
    }

    fn sidebar(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let needle = value(&self.search, cx).to_lowercase();
        let filtered: Vec<ProjectDocument> = self.documents.iter().filter(|d| d.title.to_lowercase().contains(&needle)).cloned().collect();
        let mut list = div().id("doc-list").flex_1().min_h_0().overflow_y_scroll().pt(px(1.)).px(px(8.)).pb(px(12.)).flex().flex_col();
        if self.loading {
            list = list.child(div().p(px(14.)).text_size(fs(11.)).text_color(p.text_3).child("Yükleniyor…"));
        } else {
            for f in self.folders.clone() {
                let items: Vec<&ProjectDocument> = filtered.iter().filter(|d| d.folder_id.as_deref() == Some(f.id.as_str())).collect();
                if !needle.is_empty() && items.is_empty() {
                    continue;
                }
                let mut sec = div().flex().flex_col().gap(px(1.)).mb(px(7.)).child(self.folder_head(&f.id, format!("▰ {}", f.name), items.len(), Some(f.clone()), p, cx));
                if !self.collapsed.contains(&f.id) {
                    for d in items {
                        sec = sec.child(self.doc_row(d, p, cx));
                    }
                }
                list = list.child(sec);
            }
            let unfiled: Vec<&ProjectDocument> = filtered.iter().filter(|d| d.folder_id.as_ref().is_none_or(|f| !self.folders.iter().any(|x| &x.id == f))).collect();
            if !unfiled.is_empty() {
                let mut sec = div().flex().flex_col().gap(px(1.)).mb(px(7.)).child(self.folder_head("__unfiled", "Klasörsüz".into(), unfiled.len(), None, p, cx));
                if !self.collapsed.contains("__unfiled") {
                    for d in unfiled {
                        sec = sec.child(self.doc_row(d, p, cx));
                    }
                }
                list = list.child(sec);
            }
            if filtered.is_empty() {
                let searching = !needle.is_empty();
                // `.documents-empty-list`: renkler temadan bağımsız sabit (#707a9c / #adb6d2).
                list = list.child(
                    div()
                        .pt(px(50.))
                        .pb(px(50.))
                        .px(px(16.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(5.))
                        .text_color(rgb(0x707a9c))
                        .child(div().h(px(37.)).text_size(fs(34.)).line_height(px(37.)).child("◇"))
                        .child(div().h(px(11.)).text_size(fs(11.)).line_height(px(11.)).font_weight(FontWeight::BOLD).text_color(rgb(0xadb6d2)).child(if searching { "Sonuç yok" } else { "Henüz doküman yok" }))
                        .child(div().h(px(9.)).text_size(fs(9.)).line_height(px(9.)).child(if searching { "Farklı bir ad ara." } else { "İlk tuvali veya sayfayı oluştur." })),
                );
            }
        }
        let search = Input::new(&self.search).appearance(false).bordered(false).focus_bordered(false).text_size(font_size::MD).text_color(rgb(0xe5e9fb)).h(px(16.)).px(px(2.)).py(px(1.));
        let focused = gpui_kit::Focusable::focus_handle(self.search.read(cx), cx).is_focused(window);
        let header = div()
            .relative()
            .h(px(68.))
            .flex_shrink_0()
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(p.border_subtle)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().h(px(15.)).text_size(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text).child("Dokümanlar"))
                    .child(div().h(px(10.)).truncate().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(tr_upper(&self.project_name))),
            )
            .child(head_button("docs-hide", r#"<path d="m12.5 5-5 5 5 5"/>"#, p).on_click(cx.listener(|this, _, _, cx| {
                this.sidebar_hidden = true;
                cx.notify();
            })))
            .child(head_button("docs-folder", r#"<path d="M3 6.5h5l1.5 2H17v7.5H3z"/><path d="M3 8.5h14"/>"#, p).on_click(cx.listener(|this, _, window, cx| this.ask_folder(None, window, cx))))
            .child(head_button("docs-new", r#"<path d="M10 4v12M4 10h12"/>"#, p).on_click(cx.listener(|this, _, _, cx| {
                this.creating_menu = !this.creating_menu;
                cx.notify();
            })))
            .when(self.creating_menu, |s| s.child(gpui_kit::deferred(self.create_menu(p, cx)).with_priority(2)));
        // DocumentsPage `p2-documents-sidebar-width` (280; sürüklerken 220–480), `.documents-sidebar { position: relative }`.
        div()
            .relative()
            .w(px(crate::resize::value(&crate::resize::DOCUMENTS_SIDEBAR)))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(p.bg_1)
            .border_r_1()
            .border_color(p.border_subtle)
            .p2_enter("docs-sidebar", Entrance::SlideLeft, Duration::ZERO)
            .child(header)
            .child(
                div()
                    .mx(px(12.))
                    .my(px(11.))
                    .h(px(35.))
                    .flex_shrink_0()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(radius::SM)
                    .border_1()
                    .border_color(if focused { p.accent_hover } else { p.border })
                    .when(focused, |s| s.shadow(p2_ui::tokens::shadows(&[p.ring])))
                    .bg(p.bg_3)
                    .text_size(font_size::MD)
                    .text_color(p.text)
                    .child("⌕")
                    .child(div().flex_1().min_w_0().child(search)),
            )
            .child(list)
            // `.documents-sidebar-resizer`: 8px, sağ kenarın 4px dışına taşar, `col-resize`; hover `rgba(104,130,255,.24)`.
            .child(crate::resize::strip("documents-sidebar-resizer", crate::resize::DOCUMENTS_SIDEBAR).right(px(-4.)).w(px(8.)).hover(|s| s.bg(gpui_kit::rgba(0x6882ff3d))))
            .into_any_element()
    }

    /// `.documents-create-menu`: + düğmesinin altında iki seçenek.
    fn create_menu(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hbg, htx, text, accent) = (p.accent_soft, p.text_strong, p.text, p.text_accent);
        let item = |id: &'static str, kind: &'static str, title: &'static str, sub: &'static str| {
            div()
                .id(id)
                .w_full()
                .p(px(9.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(7.))
                .text_color(text)
                .cursor_pointer()
                .hover(move |s| s.bg(hbg).text_color(htx))
                .child(div().w(px(34.)).flex().justify_center().child(doc_icon(kind, 18., 1.7, true, text)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(font_size::MD).font_weight(FontWeight::BOLD).child(title)).child(div().truncate().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(accent).child(tr_upper(sub))))
        };
        div()
            .absolute()
            .left(px(12.))
            .right(px(12.))
            .top(px(58.))
            .child(
                div()
                    .id("docs-create-menu")
                    .p(px(6.))
                    .rounded(radius::MD)
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
                    .shadow(p.shadow_lg())
                    .occlude()
                    .p2_enter("docs-create-menu", Entrance::ScaleIn, Duration::ZERO)
                    .child(item("create-canvas", "CANVAS", "Serbest tuval", "Notlar, metinler ve görseller").on_click(cx.listener(|this, _, window, cx| this.create("CANVAS", window, cx))))
                    .child(item("create-page", "PAGE", "Metin sayfası", "Confluence tarzı doküman").on_click(cx.listener(|this, _, window, cx| this.create("PAGE", window, cx)))),
            )
            .into_any_element()
    }

    /// `.documents-welcome`: seçili doküman yokken (proje boşken) ortadaki karşılama.
    fn welcome(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (grad, glow, on_accent) = (p.accent_grad.background(), p2_ui::tokens::shadows(&[p.accent_glow_strong, p.inset_top]), p.text_on_accent);
        let button = |id: &'static str, kind: &'static str, label: &'static str| {
            div()
                .id(id)
                .h(px(35.))
                .px(px(15.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(radius::SM)
                .bg(grad.clone())
                .shadow(glow.clone())
                .text_color(on_accent)
                .text_size(font_size::MD)
                .font_weight(FontWeight::BOLD)
                .cursor_pointer()
                .child(doc_icon(kind, 17., 1.7, false, on_accent))
                .child(label)
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(480.))
                    .p(px(36.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .p2_enter("docs-welcome", Entrance::Rise, Duration::ZERO)
                    .child(
                        div()
                            .size(px(74.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XL)
                            .border_1()
                            .border_color(p.border_strong)
                            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
                            .shadow(p.shadow_lg())
                            .text_color(p.text_accent)
                            .text_size(fs(44.))
                            .child("◇"),
                    )
                    .child(div().mt(px(8.)).h(px(25.)).text_size(fs(24.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("İlk çalışmanı oluştur"))
                    .child(div().w(px(408.)).text_center().text_size(font_size::MD).line_height(px(17.68)).text_color(p.text_3).child("Fikirlerini serbest bir tuvalde düzenle veya biçimlendirilmiş bir dokümanla yazıya dök."))
                    .child(
                        div()
                            .flex()
                            .gap(px(9.))
                            .child(button("welcome-canvas", "CANVAS", "Serbest tuval").on_click(cx.listener(|this, _, window, cx| this.create("CANVAS", window, cx))))
                            .child(button("welcome-page", "PAGE", "Doküman").on_click(cx.listener(|this, _, window, cx| this.create("PAGE", window, cx)))),
                    ),
            )
            .into_any_element()
    }
}

/// Pencere koordinatlı katman (Electron'daki  / body portalı): menüler ve pencereler
/// sayfanın değil pencerenin sol üstüne göre konumlanır, tüm pencereyi örter.
fn window_layer(id: &'static str, child: AnyElement, window: &Window) -> AnyElement {
    let size = window.viewport_size();
    gpui_kit::deferred(gpui_kit::anchored().position_mode(gpui_kit::AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(div().id(id).relative().w(size.width).h(size.height).child(child))).with_priority(3).into_any_element()
}

impl Render for DocumentsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let sidebar = (!self.sidebar_hidden).then(|| self.sidebar(&p, window, cx));
        let stage: AnyElement = match self.doc.clone() {
            None => self.welcome(&p, cx),
            Some(doc) => {
                let writable = doc.writable();
                let topbar = self.topbar(&doc, &p, window, cx);
                let body = if doc.kind == "CANVAS" { self.canvas_view(writable, &p, window, cx) } else { self.paged_view(writable, &p, cx) };
                div().size_full().flex().flex_col().child(topbar).child(body).into_any_element()
            }
        };
        let (hbg, hbd, htx) = (p.accent_soft, p.border_strong, p.text_strong);
        let show_sidebar = self.sidebar_hidden.then(|| {
            div()
                .id("docs-show")
                .absolute()
                .left(px(12.))
                .top(px(13.))
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::SM)
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .text_color(p.text)
                .cursor_pointer()
                .hover(move |s| s.bg(hbg).border_color(hbd).text_color(htx))
                .tooltip(|window, cx| p2_ui::controls::tooltip("Doküman panelini göster", window, cx))
                .child("☰")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sidebar_hidden = false;
                    if let Some(c) = &mut this.canvas {
                        c.centered = false;
                    }
                    cx.notify();
                }))
        });
        let banner = self.error.clone().map(|(m, _)| {
            div()
                .absolute()
                .top(px(65.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .py(px(8.))
                        .px(px(12.))
                        .rounded(radius::MD)
                        .border_1()
                        .border_color(p.danger_border)
                        .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
                        .shadow(p.shadow_lg())
                        .text_size(fs(10.))
                        .text_color(p.danger_text)
                        .p2_enter("docs-banner", Entrance::Rise, Duration::ZERO)
                        .child(m)
                        .child(div().id("docs-reload").text_color(p.text_strong).underline().cursor_pointer().child("Yenile").on_click(cx.listener(|this, _, window, cx| {
                            this.error = None;
                            this.load(window, cx);
                        }))),
                )
        });
        let share = self.sharing.then(|| self.doc.clone()).flatten().map(|d| self.share_modal(&d, self.share.as_ref(), &p, cx));
        let doc_menu = self.doc_menu_layer(&p, window, cx);
        let canvas_menu = self.canvas_menu_layer(&p, window, cx);
        let folder_modal = self.folder_modal(&p, window, cx);
        let confirm_doc = self.confirm_doc.clone().map(|d| {
            self.confirm_modal("Doküman silinsin mi?", format!("“{}” dokümanı silinsin mi?", d.title), "Sil", "confirm-doc", &p, window, cx, move |this, window, cx| {
                this.confirm_doc = None;
                this.delete_doc(d.clone(), window, cx);
            })
        });
        let confirm_folder = self.confirm_folder.clone().map(|f| {
            self.confirm_modal("Klasör silinsin mi?", format!("“{}” klasörü silinsin mi? İçindeki dokümanlar klasörsüz alana taşınır.", f.name), "Sil", "confirm-folder", &p, window, cx, move |this, window, cx| this.remove_folder(f.clone(), window, cx))
        });
        div()
            .size_full()
            .relative()
            .flex()
            .bg(p.bg_0)
            .text_size(font_size::MD)
            .text_color(p.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.creating_menu {
                        this.creating_menu = false;
                        cx.notify();
                    }
                }),
            )
            .children(sidebar)
            .child(div().relative().flex_1().min_w_0().h_full().child(stage).children(show_sidebar).children(banner))
            .children(share.map(|e| window_layer("docs-share-layer", e, window)))
            .children(doc_menu.map(|e| window_layer("docs-menu-layer", e, window)))
            .children(canvas_menu.map(|e| window_layer("docs-canvas-menu-layer", e, window)))
            .children(folder_modal.map(|e| window_layer("docs-folder-layer", e, window)))
            .children(confirm_doc.map(|e| window_layer("docs-confirm-doc", e, window)))
            .children(confirm_folder.map(|e| window_layer("docs-confirm-folder", e, window)))
            .p2_enter("docs-app", Entrance::Fade, Duration::ZERO)
    }
}
