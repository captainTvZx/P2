//! İş parçacığı paneli — ChannelContent.tsx `<aside className="thread-panel">` (sağda sabit çekmece,
//! 360px, cam zemin): başlık ("Thread" + #kanal), kök mesaj (`.thread-root`), "N yanıt" ayracı,
//! yanıtlar (`ThreadReply`, gerçek zamanlı) ve yanıt yazma kutusu (`ReplyComposer`; emoji düğmesi
//! panelde gizli). Satır görünümü kanal akışıyla ortak (`chat_view.rs`). Forum konuları da bu
//! panelde açılır (Electron'da forum görünümü akışın yerine geçer — bilinen fark).

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, Focusable as _, SharedString, Subscription, Window, div, px};
use p2_client_core::{Api, ApiError, ChatTarget, Message, User};
use p2_ui::P2Text as _;
use p2_ui::icons::icon;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, Shadow, css_rgba, fs, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::chat::extras::{ImageCache, split_content};
use super::chat::view::{self, RowAct, RowKind, RowOpts};
use crate::task;

pub enum ThreadEvent {
    Close,
    /// Yanıt sayısı değişti: sohbet satırındaki "N yanıt" güncellensin.
    Count { parent: String, replies: u32 },
    // L ajanı: bağlam menüsünden sohbetin iletişim kutularına (ChannelContent ortak durumları).
    Delete(Message),
    Forward(String),
    History(String),
}

/// Panel genişliği (`useResizable('p2:thread-width',360,280,660,-1)`), oturum boyunca saklanır.
static THREAD_WIDTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(360);

pub struct ThreadPane {
    api: Arc<Api>,
    pub target: ChatTarget,
    pub parent: Message,
    pub forum: bool,
    /// Başlıktaki alt yazı (`#kanal`).
    pub title: String,
    me: User,
    replies: Vec<Message>,
    loading: bool,
    sending: bool,
    error: Option<(String, usize)>,
    composer: Entity<TextareaState>,
    hovered: Option<String>,
    quoted: Option<Message>,
    copied: Option<String>,
    images: ImageCache,
    pub(crate) players: super::chat::media::Players,
    /// L ajanı: bağlam menüsü (mesaj, x, y, üst, sağa hizalı), tepki seçici, düzenlenen yanıt, sürükleme.
    menu: Option<(String, f32, f32, f32, bool)>,
    picker: Option<(super::chat::emoji::EmojiPanel, String, f32, f32, f32)>,
    editing: Option<String>,
    dragging: bool,
    origin: Rc<std::cell::Cell<(f32, f32)>>,
    pub can_delete_any: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ThreadEvent> for ThreadPane {}

impl ThreadPane {
    pub fn new(api: Arc<Api>, target: ChatTarget, parent: Message, forum: bool, me: User, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 11).submit_on_enter(true).placeholder(if forum { "Bu konuya yanıt yaz…" } else { "Thread'e yanıt yaz…" }));
        let subs = vec![cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
            InputEvent::Focus | InputEvent::Blur | InputEvent::Change => cx.notify(),
            _ => {}
        })];
        let mut pane = Self {
            api,
            target,
            parent,
            forum,
            title: String::new(),
            me,
            replies: Vec::new(),
            loading: true,
            sending: false,
            error: None,
            composer,
            hovered: None,
            quoted: None,
            copied: None,
            images: ImageCache::new(),
            players: Default::default(),
            menu: None,
            picker: None,
            editing: None,
            dragging: false,
            origin: Rc::default(),
            can_delete_any: false,
            _subs: subs,
        };
        pane.load(window, cx);
        pane
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, target, parent, forum, me) = (self.api.clone(), self.target.clone(), self.parent.id.clone(), self.forum, self.me.id.clone());
        task::run(window, cx, move || api.replies(&target, &parent, forum, &me), |this, r, window, cx| {
            this.loading = false;
            match r {
                Ok(rows) => {
                    crate::trace(&format!("konu: {} yanıt yüklendi", rows.len()));
                    this.replies = rows;
                    // L ajanı parite kancası: `P2_AUTOPILOT_THREAD_MENU=x,y` son yanıtın bağlam menüsü (veri yazmaz).
                    if let Some((x, y)) = std::env::var("P2_AUTOPILOT_THREAD_MENU").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?)))) {
                        if let Some(last) = this.replies.last().map(|m| m.id.clone()) {
                            this.menu = Some((last, x, y, y, false));
                            crate::trace("parite: konu menüsü açıldı");
                        }
                    }
                    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                    if let Ok(text) = std::env::var("P2_AUTOPILOT_THREAD") {
                        if !SAID.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            this.composer.update(cx, |s, cx| s.set_value(text, window, cx));
                            this.submit(window, cx);
                        }
                    }
                }
                Err(e) => this.set_error(e, cx),
            }
            cx.notify();
        });
    }

    fn set_error(&mut self, e: ApiError, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map(|x| x.1 + 1).unwrap_or(0);
        self.error = Some((if e.message.is_empty() { "Yanıt gönderilemedi.".into() } else { e.message }, epoch));
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.composer.read(cx).value().trim().to_string();
        if content.is_empty() || self.sending {
            return;
        }
        self.sending = true;
        // L ajanı: yanıt düzenleme (`editingReply`).
        if let Some(id) = self.editing.clone() {
            let (api, target, me) = (self.api.clone(), self.target.clone(), self.me.id.clone());
            task::run(window, cx, move || api.edit_message(&target, &id, &content, &me), |this, r, window, cx| {
                this.sending = false;
                match r {
                    Ok(m) => {
                        this.editing = None;
                        this.composer.update(cx, |s, cx| s.clean(window, cx));
                        if this.parent.id == m.id {
                            this.parent.content = m.content.clone();
                            this.parent.edited_at = m.edited_at.clone();
                        }
                        this.on_updated(m, cx);
                    }
                    Err(e) => this.set_error(e, cx),
                }
            });
            return;
        }
        let quoted = self.quoted.as_ref().map(|m| m.id.clone());
        let (api, target, parent, forum, me) = (self.api.clone(), self.target.clone(), self.parent.id.clone(), self.forum, self.me.id.clone());
        task::run(window, cx, move || api.send_reply(&target, &parent, forum, &content, quoted.as_deref(), &me), |this, r, window, cx| {
            this.sending = false;
            match r {
                Ok(m) => {
                    crate::trace(&format!("konu: yanıt gönderildi «{}»", m.content));
                    this.quoted = None;
                    this.composer.update(cx, |s, cx| s.clean(window, cx));
                    this.on_created(m, cx);
                }
                Err(e) => this.set_error(e, cx),
            }
        });
    }

    /// Gerçek zamanlı ya da kendi gönderdiğimiz yanıt (tekrarı yok sayılır).
    pub fn on_created(&mut self, mut message: Message, cx: &mut Context<Self>) {
        if self.replies.iter().any(|m| m.id == message.id) {
            return;
        }
        message.normalize(&self.me.id);
        self.replies.push(message);
        cx.emit(ThreadEvent::Count { parent: self.parent.id.clone(), replies: self.replies.len() as u32 });
        cx.notify();
    }

    pub fn on_updated(&mut self, mut message: Message, cx: &mut Context<Self>) {
        message.normalize(&self.me.id);
        if let Some(m) = self.replies.iter_mut().find(|m| m.id == message.id) {
            m.content = message.content;
            m.edited_at = message.edited_at;
            cx.notify();
        }
    }

    pub fn on_deleted(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.replies.len();
        self.replies.retain(|m| m.id != id);
        if before != self.replies.len() {
            cx.emit(ThreadEvent::Count { parent: self.parent.id.clone(), replies: self.replies.len() as u32 });
            cx.notify();
        }
    }

    fn row_act(&mut self, id: &str, act: RowAct, window: &mut Window, cx: &mut Context<Self>) {
        let message = if self.parent.id == id { Some(self.parent.clone()) } else { self.replies.iter().find(|m| m.id == id).cloned() };
        let Some(message) = message else { return };
        match act {
            RowAct::Hover(over) => {
                if over {
                    self.hovered = Some(id.to_string());
                } else if self.hovered.as_deref() == Some(id) {
                    self.hovered = None;
                }
                cx.notify();
            }
            RowAct::React(e) => {
                let (api, target, mid) = (self.api.clone(), self.target.clone(), id.to_string());
                task::run(window, cx, move || api.toggle_reaction(&target, &mid, &e), move |this, r, _, cx| {
                    if let Ok(reactions) = r {
                        if let Some(m) = std::iter::once(&mut this.parent).chain(this.replies.iter_mut()).find(|m| m.id == message.id) {
                            m.reactions = reactions;
                        }
                        cx.notify();
                    }
                });
            }
            RowAct::Quote => {
                self.quoted = Some(message);
                self.composer.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            }
            RowAct::Url(url) => cx.open_url(&url),
            RowAct::CopyCode(key, code) => {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(code));
                self.copied = Some(key.clone());
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(1500)).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.copied.as_deref() == Some(key.as_str()) {
                            this.copied = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            RowAct::Menu(at) => {
                self.picker = None;
                self.menu = Some((id.to_string(), at.x.into(), at.y.into(), at.y.into(), false));
                cx.notify();
            }
            RowAct::MenuFrom { right, bottom, top } => {
                self.picker = None;
                self.menu = Some((id.to_string(), right.into(), f32::from(bottom) + 6., f32::from(top) - 6., true));
                cx.notify();
            }
            RowAct::PickerFrom { right, bottom, top } => self.open_picker(id.to_string(), right.into(), f32::from(bottom) + 6., f32::from(top) - 6., window, cx),
            RowAct::Play(_) | RowAct::Seek(..) | RowAct::Volume(..) | RowAct::Mute(_) | RowAct::Fullscreen(_) | RowAct::Youtube(_) => {
                let api = self.api.clone();
                let mut players = std::mem::take(&mut self.players);
                super::chat::media_act(&mut players, &api, act, thread_players, window, cx);
                let ticking = players.ticking || self.players.ticking;
                self.players.map.extend(players.map);
                self.players.ticking = ticking;
            }
            _ => {}
        }
    }

    fn open_picker(&mut self, id: String, x: f32, y: f32, up: f32, window: &mut Window, cx: &mut Context<Self>) {
        let panel = super::chat::emoji::EmojiPanel::new(window, cx);
        let sub = cx.subscribe(&panel.search, |this, input, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let q = input.read(cx).value().to_string();
                if let Some((p, ..)) = this.picker.as_mut() {
                    p.set_query(q);
                }
                cx.notify();
            }
        });
        self._subs.push(sub);
        self.menu = None;
        self.picker = Some((panel, id, x, y, up));
        cx.notify();
    }

    fn react_to(&mut self, id: String, e: String, window: &mut Window, cx: &mut Context<Self>) {
        let (api, target) = (self.api.clone(), self.target.clone());
        let mid = id.clone();
        task::run(window, cx, move || api.toggle_reaction(&target, &mid, &e), move |this, r, _, cx| {
            if let Ok(reactions) = r {
                if let Some(m) = std::iter::once(&mut this.parent).chain(this.replies.iter_mut()).find(|m| m.id == id) {
                    m.reactions = reactions;
                }
                cx.notify();
            }
        });
    }

    /// Bağlam menüsü eylemi (Electron `MessageTools`; düzenleme yanıt kutusunda).
    fn menu_act(&mut self, act: view::MenuAct, m: Message, window: &mut Window, cx: &mut Context<Self>) {
        use view::MenuAct;
        let at = self.menu.take();
        match act {
            MenuAct::React(e) => self.react_to(m.id.clone(), e, window, cx),
            MenuAct::Picker => {
                if let Some((_, x, y, up, _)) = at {
                    self.open_picker(m.id.clone(), x, y, up, window, cx);
                }
            }
            MenuAct::Quote => {
                self.editing = None;
                self.quoted = Some(m);
                self.composer.update(cx, |s, cx| s.focus(window, cx));
            }
            MenuAct::Edit => {
                self.quoted = None;
                self.editing = Some(m.id.clone());
                let content = m.content.clone();
                self.composer.update(cx, |s, cx| {
                    s.set_value(content, window, cx);
                    s.focus(window, cx);
                });
            }
            MenuAct::Thread => {}
            MenuAct::Forward => cx.emit(ThreadEvent::Forward(m.id.clone())),
            MenuAct::Copy => cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(copy_plain(&m.content))),
            MenuAct::Save(_) => {}
            MenuAct::History => cx.emit(ThreadEvent::History(m.id.clone())),
            MenuAct::Link => {
                if let ChatTarget::Channel { channel, .. } = &self.target {
                    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(format!("p2://channels/{channel}/messages/{}", m.id)));
                }
            }
            MenuAct::Delete => cx.emit(ThreadEvent::Delete(m)),
        }
        cx.notify();
    }

    fn row(&self, m: &Message, kind: RowKind, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        let mid = m.id.clone();
        let cb: view::RowCb = Rc::new(move |act, window, cx| {
            let _ = weak.update(cx, |this, cx| this.row_act(&mid, act, window, cx));
        });
        view::message_row(
            RowOpts { message: m, kind, hovered: self.hovered.as_deref() == Some(m.id.as_str()), latest: false, flash: false, quote: true, thread: false, images: &self.images, players: Some(&self.players), copied: self.copied.as_deref(), cb },
            p,
        )
    }
}

impl Render for ThreadPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        // L ajanı: forum konusu akışın yerinde (`.forum-topic-view`).
        if self.forum {
            return self.forum_view(&p, window, cx);
        }
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let root = self.row(&self.parent.clone(), RowKind::ThreadRoot, &p, cx);
        let mut replies = div().id("thread-replies").flex_1().min_h_0().overflow_y_scroll().py(px(10.)).flex().flex_col();
        if self.loading {
            replies = replies.child(div().text_size(fs(12.)).text_color(p.text_3).child("Yanıtlar yükleniyor…"));
        } else if self.replies.is_empty() {
            replies = replies.child(div().text_size(fs(12.)).text_color(p.text_3).child("Henüz yanıt yok. İlk yanıtı sen yaz."));
        }
        for m in self.replies.clone() {
            replies = replies.child(self.row(&m, RowKind::ThreadReply, &p, cx));
        }
        let count = self.replies.len();
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let empty = self.composer.read(cx).value().trim().is_empty();
        let composer = self.composer(&p, focused, empty, cx);
        let _ = split_content;
        let overlay = self.overlay(&p, window, cx);
        let width = THREAD_WIDTH.load(std::sync::atomic::Ordering::Relaxed) as f32;
        // `.resize-handle.left`: 8px tutamaç, üzerine gelince 2px `accent-soft-2`, sürüklerken `accent-hover`.
        let (hover_line, drag_line) = (p.accent_soft_2, p.accent_hover);
        let dragging = self.dragging;
        let handle = div()
            .id("thread-resize")
            .group("thread-resize")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-4.))
            .w(px(8.))
            .cursor_col_resize()
            .child(div().absolute().top_0().bottom_0().left(px(3.)).w(px(2.)).rounded(px(2.)).when(dragging, |s| s.bg(drag_line)).group_hover("thread-resize", move |s| s.bg(if dragging { drag_line } else { hover_line })))
            .tooltip(super::chat::view::tip_text("Thread genişliği"))
            .on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.dragging = true;
                cx.stop_propagation();
                cx.notify();
            }));
        div()
            .id("thread-panel")
            .relative()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .px(px(12.))
            .pb(px(12.))
            .border_l_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(shadows(&[Shadow::new(-8., 0., 24., 0., css_rgba(0, 0, 0, 0.4)), Shadow { x: 1., y: 0., blur: 0., spread: 0., color: css_rgba(255, 255, 255, 0.05), inset: true }]))
            .occlude()
            .child(
                div()
                    .h(px(52.))
                    .flex_none()
                    .mx(px(-12.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child(if self.forum { "Konu" } else { "Thread" }))
                            .child(div().text_size(fs(9.)).text_color(p.text_3).child(self.title.clone())),
                    )
                    .child(
                        div()
                            .id("thread-close")
                            .size(px(30.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(fs(19.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ThreadEvent::Close))),
                    ),
            )
            .child(root)
            .child(
                div()
                    .mt(px(2.))
                    .mx(px(5.))
                    .mb(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(fs(10.5))
                    .text_color(p.text_3)
                    .child(if count > 0 { format!("{count} yanıt") } else { "Henüz yanıt yok".to_string() })
                    .child(div().flex_1().h(px(1.)).bg(p.border_subtle)),
            )
            .child(replies)
            .child(composer)
            .child(handle)
            .child(self.origin_probe())
            .children(overlay)
            .p2_enter("thread-panel", Entrance::SlideRight, Duration::ZERO)
            .into_any_element()
    }
}

impl ThreadPane {
    /// ChannelContent `.forum-topic-view`: "← Konulara dön", konu başı kartı (`.forum-topic-head`),
    /// "N yanıt" ayracı ve alta yaslı yanıtlar (`.forum-replies`), yanıt kutusu (9px üst boşluk).
    fn forum_view(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (hover, strong) = (p.bg_hover, p.text_strong);
        let back = div()
            .id("forum-back")
            .flex_none()
            .h(px(32.))
            .px(px(9.))
            .flex()
            .items_center()
            .rounded(px(5.))
            .text_size(fs(11.))
            .text_color(p.text_3)
            .cursor_pointer()
            .hover(move |s| s.bg(hover).text_color(strong))
            .child("← Konulara dön")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ThreadEvent::Close)));
        let m = self.parent.clone();
        let weak = cx.entity().downgrade();
        let mid = m.id.clone();
        let cb: view::RowCb = Rc::new(move |act, window, cx| {
            let _ = weak.update(cx, |this, cx| this.row_act(&mid, act, window, cx));
        });
        let (visible, _) = split_content(&m.content);
        let menu_cb = cb.clone();
        let head = div()
            .id("forum-topic-head")
            .flex_none()
            .mt(px(12.))
            .p(px(18.))
            .flex()
            .gap(px(11.))
            .border_1()
            .border_color(p.border)
            .rounded(px(14.))
            .bg(p.glass.background())
            .shadow(shadows(&[p.inset_top]))
            .on_mouse_down(gpui_kit::MouseButton::Right, move |e: &gpui_kit::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                menu_cb(RowAct::Menu(e.position), window, cx)
            })
            .child(div().flex_none().w(px(42.)).child(view::avatar_tile(&m.author.id, &m.author.display_name, 40., 13., fs(12.), p)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(14.))
                            .flex()
                            .items_end()
                            .gap(px(8.))
                            .child(div().text_size(fs(13.)).line_height(px(14.)).css_weight(700.).text_color(p.text).child(m.author.display_name.clone()))
                            .child(div().text_size(fs(9.)).line_height(px(9.)).mb(px(1.)).text_color(p.text_4).child(view::tr_datetime(&m.created_at))),
                    )
                    .child(div().mt(px(7.)).text_size(fs(16.)).line_height(px(19.72)).css_weight(650.).text_color(p.text_strong).child(visible))
                    .children(view::reactions(&m.id, &m, p, &cb)),
            );
        let count = self.replies.len();
        let mut replies = div().id("forum-replies").flex_1().min_h_0().overflow_y_scroll().py(px(12.)).flex().flex_col().child(div().flex_1());
        if count > 0 || !self.loading {
            replies = replies.child(
                div()
                    .mt(px(2.))
                    .mx(px(5.))
                    .mb(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(fs(10.5))
                    .text_color(p.text_3)
                    .child(format!("{count} yanıt"))
                    .child(div().flex_1().h(px(1.)).bg(p.border_subtle)),
            );
        }
        if self.loading {
            replies = replies.child(div().text_size(fs(12.)).text_color(p.text_3).child("Yanıtlar yükleniyor…"));
        } else if count == 0 {
            replies = replies.child(div().text_size(fs(12.)).text_color(p.text_3).child("Henüz yanıt yok. İlk yanıtı sen yaz."));
        }
        for r in self.replies.clone() {
            replies = replies.child(self.row(&r, RowKind::Feed, p, cx));
        }
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let empty = self.composer.read(cx).value().trim().is_empty();
        let composer = self.composer(p, focused, empty, cx);
        let overlay = self.overlay(p, window, cx);
        div()
            .id("forum-topic-view")
            .relative()
            .size_full()
            .pt(px(18.))
            .px(px(20.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .child(div().flex().child(back))
            .child(head)
            .child(replies)
            .child(composer)
            .child(self.origin_probe())
            .children(overlay)
            .into_any_element()
    }

    /// Panelin pencere içindeki sol üst köşesini ölçer (menü katmanı pencere koordinatıyla çizilir).
    fn origin_probe(&self) -> impl IntoElement {
        let cell = self.origin.clone();
        gpui_kit::canvas(move |b, _, _| cell.set((f32::from(b.origin.x), f32::from(b.origin.y))), |_, _, _, _| {}).absolute().inset_0()
    }

    /// Menü/tepki seçici katmanı ve sürükleme yakalayıcı (pencere koordinatı → panel).
    fn overlay(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu.is_none() && self.picker.is_none() && !self.dragging {
            return None;
        }
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        // Panelin pencere içindeki sol kenarı (forumda akışın solu: pencere − içerik genişliği yerine sınırdan).
        let (ox, oy) = self.origin.get();
        let mut layer = div()
            .id("thread-layer")
            .absolute()
            .left(px(-ox))
            .top(px(-oy))
            .w(px(vw))
            .h(px(vh))
            .on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.menu = None;
                this.picker = None;
                cx.notify();
            }))
            .on_mouse_down(gpui_kit::MouseButton::Right, cx.listener(|this, _, _, cx| {
                this.menu = None;
                this.picker = None;
                cx.notify();
            }));
        if self.dragging {
            layer = layer
                .occlude()
                .cursor_col_resize()
                .on_mouse_move(cx.listener(|_, e: &gpui_kit::MouseMoveEvent, window, cx| {
                    let w = f32::from(window.viewport_size().width - e.position.x).clamp(280., 660.);
                    THREAD_WIDTH.store(w.round() as u32, std::sync::atomic::Ordering::Relaxed);
                    cx.notify();
                }))
                .on_mouse_up(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.dragging = false;
                    cx.notify();
                }));
        }
        if let Some((id, x, y, up, right)) = self.menu.clone() {
            let message = if self.parent.id == id { Some(self.parent.clone()) } else { self.replies.iter().find(|m| m.id == id).cloned() };
            if let Some(message) = message {
                let own = message.author.id == self.me.id;
                let channel = matches!(self.target, ChatTarget::Channel { .. });
                let spec = view::MenuSpec { message: &message, editable: own, deletable: own || self.can_delete_any, thread: false, forward: channel, history: channel, link: channel };
                let (who, m) = (cx.entity().downgrade(), message.clone());
                let cb: view::MenuCb = Rc::new(move |act, window, cx| {
                    let m = m.clone();
                    let _ = who.update(cx, |this, cx| this.menu_act(act, m, window, cx));
                });
                layer = layer.child(view::message_menu_view(spec, x, y, up, right, p, window, cb));
            }
        }
        if let Some((panel, id, x, y, up)) = &self.picker {
            let (who, id) = (cx.entity().downgrade(), id.clone());
            let pick: super::chat::emoji::Pick = Rc::new(move |e, window, cx| {
                let id = id.clone();
                let _ = who.update(cx, |this, cx| {
                    this.picker = None;
                    this.react_to(id, e, window, cx);
                });
            });
            let (left, top) = view::place_menu(*x, *y, *up, true, 330., 354., window);
            layer = layer.child(
                div().absolute().left(px(left)).top(px(top)).child(
                    div()
                        .id("thread-reaction-picker")
                        .w(px(330.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(14.))
                        .bg(p.bg_2)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(panel.render(p, pick))
                        .p2_enter("thread-reaction-picker", Entrance::ScaleIn, Duration::ZERO),
                ),
            );
        }
        Some(layer.into_any_element())
    }

    /// `ReplyComposer` (`.reply-composer`): alıntı şeridi, giriş satırı, araç çubuğu (emoji gizli).
    fn composer(&self, p: &Palette, focused: bool, empty: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut form = div()
            .mt(px(9.))
            .flex_none()
            .flex()
            .flex_col()
            .border_1()
            .border_color(if focused { p.border_accent } else { p.border })
            .rounded(px(14.))
            .bg(p.glass.background())
            .shadow(if focused {
                let mut s = p.shadow_md.clone();
                s.push(p.ring);
                s.push(p.inset_top);
                shadows(&s)
            } else {
                let mut s = p.shadow_sm.clone();
                s.push(p.inset_top);
                shadows(&s)
            });
        if let Some(q) = &self.quoted {
            let (hover, strong) = (p.bg_hover, p.text_strong);
            form = form.child(
                div()
                    .min_h(px(48.))
                    .px(px(10.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .bg(p.accent_soft)
                    .rounded_t(px(14.))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .text_size(fs(10.))
                            .text_color(p.text_2)
                            .child(div().flex().child(div().css_weight(700.).text_color(p.text_strong).child(q.author.display_name.clone())).child(" adlı kullanıcıya yanıt veriliyor"))
                            .child(div().truncate().text_size(fs(9.)).text_color(p.text_3).child(q.content.split_whitespace().collect::<Vec<_>>().join(" "))),
                    )
                    .child(
                        div()
                            .id("thread-quote-x")
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .text_size(fs(17.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.quoted = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let (soft2, strong) = (p.accent_soft_2, p.text_strong);
        form = form.child(
            div()
                .min_h(px(43.)).py(px(4.5))
                .px(px(7.))
                .flex()
                .items_center()
                .child(div().flex_none().w(px(35.)).child(
                    div().id("thread-attach").group("thread-attach").size(px(29.)).flex().items_center().justify_center().rounded_full().bg(p.bg_3).cursor_pointer().hover(move |s| s.bg(soft2)).child(icon("composer:plus").stroke(2.).el().size(px(15.)).text_color(p.text_2).group_hover("thread-attach", move |s| s.text_color(strong))),
                ))
                .child(div().flex_1().min_w_0().py(px(9.)).text_color(p.text).child(Textarea::new(&self.composer).appearance(false).with_size(gpui_kit::component::Size::XSmall).text_size(fs(13.)).line_height(px(16.0225)))),
        );
        let fmt = |id: &'static str, name: &'static str, stroke: f32| {
            let soft = p.accent_soft;
            div().id(id).group(id).w(px(27.)).h(px(26.)).flex().items_center().justify_center().rounded(px(6.)).cursor_pointer().hover(move |s| s.bg(soft)).child(icon(name).stroke(stroke).el().size(px(15.)).text_color(p.text_3).group_hover(id, move |s| s.text_color(strong)))
        };
        let can_send = !self.sending;
        let _ = empty;
        form = form.child(
            div()
                .h(px(34.))
                .px(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(p.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .child(fmt("t-bold", "composer:bold", 2.2))
                        .child(fmt("t-italic", "composer:italic", 1.8))
                        .child(fmt("t-link", "composer:link", 1.8))
                        .child(div().w(px(1.)).h(px(17.)).mx(px(4.)).bg(p.border_subtle))
                        .child(fmt("t-mention", "composer:mention", 1.8).on_click(cx.listener(|this, _, window, cx| {
                            this.composer.update(cx, |s, cx| {
                                s.insert("@", window, cx);
                                s.focus(window, cx);
                            });
                        }))),
                )
                .child(
                    div()
                        .id("thread-send")
                        .w(px(30.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .map(|s| if can_send { s.bg(p.accent_grad.background()).shadow(shadows(&[p.accent_glow_strong])).cursor_pointer() } else { s.bg(p.bg_3) })
                        .child(icon("composer:send").stroke(1.8).el().size(px(15.)).text_color(if can_send { gpui_kit::white() } else { p.text_4 }))
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        );
        let err = self.error.clone().map(|(m, e)| div().mt(px(6.)).px(px(11.)).py(px(8.)).border_1().border_color(p.danger_border).rounded(px(8.)).bg(p.danger_soft).text_color(p.danger_text).text_size(fs(11.)).child(m).p2_enter(("thread-err", e), Entrance::Fade, Duration::ZERO));
        div().flex_none().flex().flex_col().child(form).children(err).child(div().id(SharedString::from("thread-end"))).into_any_element()
    }
}

/// `MessageTools.copy`: ek satırları ve kod çitleri olmadan metin.
fn copy_plain(value: &str) -> String {
    value.lines().filter(|l| !l.trim_start().starts_with("[[p2-file|")).collect::<Vec<_>>().join("
").trim().to_string()
}

fn thread_players(t: &mut ThreadPane) -> &mut super::chat::media::Players {
    &mut t.players
}
