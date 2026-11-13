//! Sohbetin açılır öğeleri — çalışma alanı kökünde (pencere üstünden 38px aşağıda) mutlak
//! konumlu çizilir (Electron'da `createPortal(document.body)` / `position: fixed`):
//! mesaj bağlam menüsü (messageMenu.tsx `MessageTools`), tepki seçici, başlık araç menüleri
//! (üyeler, bildirim, diğer işlemler), kanal içi arama çekmecesi (`SearchPanel`), iletişim
//! kutuları (silme, düzenleme geçmişi, kanal detayları, kanaldan ayrıl, kanal ayarları),
//! ek görüntüleyici (AttachmentViewer.tsx) ve hata kutusu (`.channel-content-error`).

use std::rc::Rc;
use p2_ui::P2Fx as _; // backdrop_blur
use std::time::Duration;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ClipboardItem, Context, ImageSource, MouseButton, ObjectFit, ScrollEvent, SharedString, WeakEntity, Window, div, img, px};
use p2_client_core::{ChannelType, ChatTarget};
use p2_ui::P2Text as _;
use p2_ui::icons::icon;
use p2_ui::tokens::{FONT_MONO, KICKER_TRACKING_EM, Palette, fs};
use p2_ui::{Entrance, P2Motion as _};

use super::extras::{Kind, Slot, format_size, preview_kind};
use super::view::{ROOT_TOP, Tone, menu_hr, menu_item, menu_panel, place_menu, tr_datetime, tr_upper};
use super::{ChatEvent, ChatPane, ChatSubject, Dialog, EmojiTarget, ToolMenu, emoji};

type Weak = WeakEntity<ChatPane>;

fn with<F: Fn(&mut ChatPane, &mut Window, &mut Context<ChatPane>) + 'static>(me: &Weak, f: F) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    let me = me.clone();
    move |_, window, cx| {
        let _ = me.update(cx, |this, cx| f(this, window, cx));
    }
}

/// Pencere noktası → çalışma alanı kökü.
fn at(left: f32, top: f32) -> gpui_kit::Div {
    div().absolute().left(px(left)).top(px(top - ROOT_TOP))
}

impl ChatPane {
    pub fn floating(&self, p: &Palette, me: Weak, window: &Window, cx: &App) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let menus_open = self.menu.is_some() || self.tool_menu.is_some() || matches!(self.emoji, Some((_, EmojiTarget::Reaction(_))));
        if menus_open {
            // `.message-menu-layer`: dışarı tıklayınca kapanır.
            let (a, b) = (me.clone(), me.clone());
            let close = move |this: &mut ChatPane, cx: &mut Context<ChatPane>| {
                this.menu = None;
                this.tool_menu = None;
                if matches!(this.emoji, Some((_, EmojiTarget::Reaction(_)))) {
                    this.emoji = None;
                }
                cx.notify();
            };
            out.push(
                div()
                    .id("chat-menu-layer")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let _ = a.update(cx, |this, cx| close(this, cx));
                    })
                    .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                        let _ = b.update(cx, |this, cx| close(this, cx));
                    })
                    .into_any_element(),
            );
        }
        if let Some(menu) = self.message_menu(p, &me, window) {
            out.push(menu);
        }
        if let Some((panel, EmojiTarget::Reaction(m))) = &self.emoji {
            let pick = me.clone();
            let pick: emoji::Pick = Rc::new(move |e, window, cx| {
                let _ = pick.update(cx, |this, cx| this.emoji_picked(e, window, cx));
            });
            let (x, y) = place_menu(m.x, m.y, m.up, m.right, 330., 354., window);
            out.push(
                at(x, y)
                    .child(
                        div()
                            .id("reaction-picker")
                            .w(px(330.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(p.border_strong)
                            .rounded(px(14.))
                            .bg(p.bg_2)
                            .shadow(p.shadow_lg())
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(panel.render(p, pick))
                            .p2_enter("reaction-picker", Entrance::ScaleIn, Duration::ZERO),
                    )
                    .into_any_element(),
            );
        }
        if let Some(menu) = self.tool_menu_view(p, &me, window, cx) {
            out.push(menu);
        }
        // D ajanı: DM/grup ••• menüsü (chat_people.rs).
        if let Some(menu) = self.people_menu_view(p, &me, window) {
            out.push(menu);
        }
        if let Some(search) = self.search_drawer(p, &me, window) {
            out.push(search);
        }
        if let Some(d) = self.dialog_view(p, &me) {
            out.push(d);
        }
        if let Some(v) = self.viewer_view(p, &me, window) {
            out.push(v);
        }
        // L ajanı: `.drop-zone` (dosya sürüklenirken).
        if let Some(z) = self.drop_zone(p, window, cx) {
            out.push(z);
        }
        if let Some((message, epoch)) = self.error().cloned() {
            out.push(
                div()
                    .absolute()
                    .right(px(18.))
                    .bottom(px(14.))
                    .px(px(11.))
                    .py(px(8.))
                    .border_1()
                    .border_color(p.danger_border)
                    .rounded(px(8.))
                    .bg(p.danger_soft)
                    .text_color(p.danger_text)
                    .text_size(fs(11.))
                    .p2_enter(("chat-err", epoch), Entrance::Fade, Duration::ZERO)
                    .child(message)
                    .into_any_element(),
            );
        }
        out
    }

    // ------------------------------------------------------------ mesaj menüsü

    fn message_menu(&self, p: &Palette, me: &Weak, window: &Window) -> Option<AnyElement> {
        let menu = self.menu.clone()?;
        let message = self.messages().iter().find(|m| m.id == menu.id)?.clone();
        let own = message.author.id == self.me_id();
        let channel = matches!(self.target, ChatTarget::Channel { .. });
        let group = matches!(self.target, ChatTarget::Group { .. });
        let spec = super::view::MenuSpec {
            message: &message,
            editable: own,
            // deletable: kendi mesajı ya da message.delete_any / delete_unlimited (L ajanı).
            deletable: own || self.can_delete_any,
            thread: channel || group,
            forward: channel,
            history: channel,
            link: self.message_link(&message.id).is_some(),
        };
        let (who, m) = (me.clone(), message.clone());
        let cb: super::view::MenuCb = Rc::new(move |act, window, cx| {
            let m = m.clone();
            let menu = menu.clone();
            let _ = who.update(cx, |this, cx| this.menu_act(act, m, menu, window, cx));
        });
        let menu = self.menu.clone()?;
        Some(div().absolute().left_0().top(px(-ROOT_TOP)).size_full().child(super::view::message_menu_view(spec, menu.x, menu.y, menu.up, menu.right, p, window, cb)).into_any_element())
    }

    /// Bağlam menüsü eylemi (menü kapanır).
    fn menu_act(&mut self, act: super::view::MenuAct, m: p2_client_core::Message, at: super::MsgMenu, window: &mut Window, cx: &mut Context<Self>) {
        use super::view::MenuAct;
        self.menu = None;
        let channel = matches!(self.target, ChatTarget::Channel { .. });
        match act {
            MenuAct::React(e) => self.react(m.id.clone(), &e, window, cx),
            MenuAct::Picker => self.open_emoji(EmojiTarget::Reaction(at), window, cx),
            MenuAct::Quote => self.start_reply(&m.id, window, cx),
            MenuAct::Edit => self.start_edit_pub(&m.id, window, cx),
            MenuAct::Thread => cx.emit(ChatEvent::OpenThread(m)),
            MenuAct::Forward => cx.emit(ChatEvent::Forward(m.id.clone())),
            MenuAct::Copy => cx.write_to_clipboard(ClipboardItem::new_string(copy_text(&m.content))),
            MenuAct::Save(f) => self.download(f, window, cx),
            MenuAct::History => self.show_history(m.id.clone(), window, cx),
            MenuAct::Link => {
                if let Some(link) = self.message_link(&m.id) {
                    cx.write_to_clipboard(ClipboardItem::new_string(link));
                }
            }
            // DM/grup: Electron onaysız siler (removeMessage); kanal: onay penceresi.
            MenuAct::Delete => {
                if channel {
                    self.dialog = Some(Dialog::Delete(m));
                } else {
                    self.delete(m.id.clone(), window, cx);
                }
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------ başlık araç menüleri

    fn tool_menu_view(&self, p: &Palette, me: &Weak, window: &Window, cx: &App) -> Option<AnyElement> {
        let kind = self.tool_menu?;
        let ChatSubject::Channel(channel) = &self.subject else { return None };
        let vw = f32::from(window.viewport_size().width);
        // Düğme sağ kenarları (başlık: sağ boşluk 18, araçlar 32px + 2px aralık; üye yığını 12px sağ boşluklu).
        let (right, width) = match kind {
            ToolMenu::More => (vw - 18., 290.),
            ToolMenu::Notify => (vw - 52., 290.),
            ToolMenu::Members => (vw - 166., 334.),
        };
        let y = 89.;
        let starred = self.head.starred;
        let notify = if self.head.notify == "MUTED" { "MUTED" } else { "MENTIONS" };
        let el = match kind {
            ToolMenu::Notify => {
                let row = |id: &'static str, check: bool, title: &'static str, sub: &'static str, tone: Tone| {
                    menu_item(id, if check { "✓" } else { "" }, "", tone, None, 38., p).py(px(7.)).children([div()
                        .absolute()
                        .left(px(33.))
                        .top(px(7.))
                        .right(px(9.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(fs(11.)).css_weight(650.).text_color(p.text).child(title))
                        .child(div().text_size(fs(9.5)).css_weight(500.).line_height(gpui_kit::relative(1.35)).text_color(p.text_3).child(sub))
                        .into_any_element()])
                };
                menu_panel("tool-menu", 290., 6., p)
                    .child(div().mt(px(2.)).mx(px(4.)).mb(px(6.)).px(px(5.)).text_size(fs(9.5)).css_weight(800.).text_color(p.text_accent).tracking(KICKER_TRACKING_EM).child(tr_upper("Sana neler bildirilsin?")))
                    .child(row("tn-mentions", notify == "MENTIONS", "Yalnızca etiketler", "@sen, @here, @everyone", Tone::Normal).relative().on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        cx.emit(ChatEvent::SetNotify("MENTIONS"));
                        cx.notify();
                    })))
                    .child(row("tn-muted", notify == "MUTED", "Sessize al", "Bildirim gelmez, kanal sessizleşir", Tone::Normal).relative().on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        cx.emit(ChatEvent::SetNotify("MUTED"));
                        cx.notify();
                    })))
                    .child(menu_hr(p))
                    .child(menu_item("tn-more", "⚙", "", Tone::Coming, None, 38., p).relative().children([div()
                        .absolute()
                        .left(px(33.))
                        .top(px(7.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(fs(11.)).css_weight(650.).text_color(p.text).child("Daha fazla seçenek"))
                        .child(div().text_size(fs(9.5)).css_weight(500.).text_color(p.text_3).child("yakında"))
                        .into_any_element()]))
                    .into_any_element()
            }
            ToolMenu::More => {
                let item = |id: &'static str, glyph: &'static str, label: String, tone: Tone| menu_item(id, glyph, label, tone, None, 38., p);
                let link = format!("p2://channels/{}", channel.id);
                menu_panel("tool-menu", 290., 6., p)
                    .child(item("tm-details", "ⓘ", "Kanal detayları".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        this.dialog = Some(Dialog::Details);
                        cx.notify();
                    })))
                    .child(item("tm-link", "🔗", "Kanal bağlantısını kopyala".into(), Tone::Normal).on_click(with(me, move |this, _, cx| {
                        this.tool_menu = None;
                        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                        cx.notify();
                    })))
                    .child(item("tm-star", "★", if starred { "Yıldızı kaldır".into() } else { "Yıldıza ekle".into() }, Tone::Normal).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        cx.emit(ChatEvent::ToggleStar);
                        cx.notify();
                    })))
                    .child(item("tm-search", "⌕", "Kanalda ara".into(), Tone::Normal).on_click(with(me, |this, window, cx| {
                        this.tool_menu = None;
                        if this.search.is_none() {
                            this.toggle_search(window, cx);
                        }
                    })))
                    .child(menu_hr(p))
                    .child(item("tm-settings", "✎", "Ayarları düzenle".into(), Tone::Normal).on_click(with(me, |this, window, cx| {
                        this.tool_menu = None;
                        this.open_channel_settings(window, cx);
                    })))
                    .child(item("tm-perms", "🔐", "Kanal izinleri".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        cx.emit(ChatEvent::ChannelPermissions);
                        cx.notify();
                    })))
                    // L ajanı: bölünmüş görünüm (bölünmüş bölmede devre dışı) ve ayrı pencere.
                    .child(item("tm-split", "◫", "Bölünmüş görünümde aç".into(), if self.split_pane || self.popout { Tone::Disabled } else { Tone::Normal }).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        if !this.split_pane && !this.popout {
                            cx.emit(ChatEvent::OpenSplit);
                        }
                        cx.notify();
                    })))
                    .child(item("tm-window", "⧉", "Yeni pencerede aç".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        cx.emit(ChatEvent::OpenWindow);
                        cx.notify();
                    })))
                    .child(menu_hr(p))
                    .child(item("tm-leave", "⤳", "Kanaldan ayrıl".into(), Tone::Danger).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        this.dialog = Some(Dialog::Leave);
                        cx.notify();
                    })))
                    .into_any_element()
            }
            ToolMenu::Members => {
                let term = self.members_search.as_ref().map(|s| s.read(cx).value().trim().to_lowercase()).unwrap_or_default();
                let visible: Vec<_> = self.members().iter().filter(|m| term.is_empty() || m.display_name.to_lowercase().contains(&term) || m.username.to_lowercase().contains(&term)).cloned().collect();
                let mut list = div().id("members-popover-list").max_h(px(308.)).overflow_y_scroll().pr(px(2.)).flex().flex_col().gap(px(1.));
                for m in &visible {
                    let hover = p.bg_hover;
                    let uid = m.id.clone();
                    let dot = presence_fill(&m.status, p);
                    let badge = match m.role.as_str() {
                        "OWNER" => Some("Kurucu"),
                        "ADMIN" => Some("Yönetici"),
                        _ => None,
                    };
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("mp-{}", m.id)))
                            .w_full()
                            .flex_none()
                            .px(px(7.))
                            .py(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .child(
                                div()
                                    .relative()
                                    .child(super::view::avatar_tile(&m.id, &m.display_name, 34., 11., fs(11.), p))
                                    .child(div().absolute().right(px(-2.)).bottom(px(-2.)).size(px(11.)).rounded_full().border_2().border_color(p.bg_2).bg(dot)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .child(div().truncate().text_size(fs(12.)).css_weight(600.).text_color(p.text).child(format!("{}{}", m.display_name, if m.id == self.me_id() { " (sen)" } else { "" })))
                                    .child(div().truncate().text_size(fs(10.)).text_color(p.text_3).child(format!("@{}", m.username))),
                            )
                            .children(badge.map(|b| div().px(px(6.)).py(px(2.)).border_1().border_color(p.border).rounded(px(9.)).text_size(fs(8.)).css_weight(750.).text_color(p.text_accent).child(b)))
                            .on_click(with(me, move |this, _, cx| {
                                this.tool_menu = None;
                                cx.emit(ChatEvent::OpenProfile(uid.clone()));
                                cx.notify();
                            })),
                    );
                }
                if visible.is_empty() {
                    list = list.child(div().text_size(fs(12.)).text_color(p.text_3).child("Eşleşen kişi yok."));
                }
                let soft = p.accent_soft;
                menu_panel("members-popover", 334., 9., p)
                    .max_h(px(440.))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .children(self.members_search.as_ref().map(|s| search_field(s, p, false).mb(px(4.))))
                            .child(
                                div()
                                    .id("member-add")
                                    .w_full()
                                    .px(px(7.))
                                    .py(px(6.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .rounded(px(6.))
                                    .text_size(fs(11.))
                                    .css_weight(600.)
                                    .text_color(p.text_2)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(soft))
                                    .child(div().flex_none().w(px(26.)).child(div().size(px(24.)).flex().items_center().justify_center().border_1().border_dashed().border_color(p.border_strong).rounded(px(8.)).child("＋")))
                                    .child(div().flex_1().child("Kanala kişi ekle"))
                                    .child(div().text_size(fs(8.)).css_weight(600.).text_color(p.text_4).child("yakında")),
                            )
                            .child(list),
                    )
                    .into_any_element()
            }
        };
        let (x, _) = place_menu(right, y, y, true, width, 0., window);
        Some(at(x, y).child(div().child(el).p2_enter(("tool-menu", kind as usize), Entrance::ScaleIn, Duration::ZERO)).into_any_element())
    }

    // ------------------------------------------------------------ arama çekmecesi

    fn search_drawer(&self, p: &Palette, me: &Weak, window: &Window) -> Option<AnyElement> {
        let input = self.search.as_ref()?;
        let ChatSubject::Channel(channel) = &self.subject else { return None };
        let vh = f32::from(window.viewport_size().height) - ROOT_TOP;
        let (soft, strong, hover) = (p.accent_soft, p.text_strong, p.bg_hover);
        let mut scroll = div().id("search-scroll").flex_1().min_h_0().overflow_y_scroll().pt(px(8.)).px(px(10.)).pb(px(16.)).flex().flex_col().child(search_field(input, p, true).mb(px(10.)));
        let state = |t: &'static str| div().text_size(fs(12.)).text_color(p.text_3).child(t);
        if self.searching {
            scroll = scroll.child(state("Aranıyor…"));
        } else {
            match &self.search_hits {
                None => scroll = scroll.child(state("En az 2 karakter yaz.")),
                Some(hits) if hits.is_empty() => scroll = scroll.child(state("Sonuç bulunamadı.")),
                Some(hits) => {
                    for (i, h) in hits.iter().enumerate() {
                        let hit = h.clone();
                        scroll = scroll.child(
                            div()
                                .id(SharedString::from(format!("hit-{i}")))
                                .w_full()
                                .flex_none()
                                .p(px(9.))
                                .rounded(px(8.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover))
                                .child(
                                    div()
                                        .flex()
                                        .items_start()
                                        .gap(px(7.))
                                        .child(div().text_size(fs(11.)).line_height(px(11.)).css_weight(700.).text_color(p.text_strong).child(h.author.display_name.clone()))
                                        .child(div().mt(px(2.)).text_size(fs(9.)).line_height(px(9.)).text_color(p.text_3).child(tr_datetime(&h.created_at))),
                                )
                                .child(div().mt(px(3.)).text_size(fs(11.)).line_height(gpui_kit::relative(1.45)).text_color(p.text_2).child(h.content.split_whitespace().collect::<Vec<_>>().join(" ")))
                                .when(h.parent_id.is_some(), |s| {
                                    s.child(div().flex().child(div().mt(px(5.)).px(px(6.)).py(px(1.)).border_1().border_color(p.border).rounded(px(8.)).text_size(fs(8.)).css_weight(700.).text_color(p.text_accent).child("thread yanıtı")))
                                })
                                .tooltip(super::chat_header::tip("Mesaja git"))
                                .on_click(with(me, move |this, window, cx| this.jump_to(hit.clone(), window, cx))),
                        );
                    }
                }
            }
        }
        Some(
            div()
                .id("search-drawer")
                .absolute()
                .top_0()
                .right_0()
                .w(px(330.))
                .h(px(vh))
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(p.border_strong)
                .bg(p.glass_strong.background())
                .shadow(p2_ui::tokens::shadows(&[p2_ui::tokens::Shadow::new(-8., 0., 24., 0., p2_ui::tokens::css_rgba(0, 0, 0, 0.4)), p2_ui::tokens::Shadow { x: 1., y: 0., blur: 0., spread: 0., color: p2_ui::tokens::css_rgba(255, 255, 255, 0.05), inset: true }]))
                .occlude()
                .child(
                    div()
                        .h(px(46.))
                        .flex_none()
                        .pl(px(16.))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(p.border_subtle)
                        .child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child(format!("#{} içinde ara", channel.name)))
                        .child(
                            div()
                                .id("search-close")
                                .size(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .text_size(fs(18.))
                                .text_color(p.text_3)
                                .cursor_pointer()
                                .hover(move |s| s.bg(soft).text_color(strong))
                                .child("×")
                                .tooltip(super::chat_header::tip("Kapat (Esc)"))
                                .on_click(with(me, |this, window, cx| this.toggle_search(window, cx))),
                        ),
                )
                .child(scroll)
                .p2_enter("search-drawer", Entrance::SlideRight, Duration::ZERO)
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ iletişim kutuları

    fn dialog_view(&self, p: &Palette, me: &Weak) -> Option<AnyElement> {
        let dialog = self.dialog.as_ref()?;
        let ChatSubject::Channel(channel) = &self.subject else {
            // DM/grup: yalnız silme ve geçmiş.
            if !matches!(dialog, Dialog::Delete(_) | Dialog::History(..) | Dialog::Details) {
                return None;
            }
            return self.dialog_card(dialog, None, p, me);
        };
        self.dialog_card(dialog, Some(channel), p, me)
    }

    fn dialog_card(&self, dialog: &Dialog, channel: Option<&p2_client_core::Channel>, p: &Palette, me: &Weak) -> Option<AnyElement> {
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let close_x = |id: &'static str| {
            div()
                .id(id)
                .size(px(29.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_size(fs(19.))
                .text_color(p.text_3)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child("×")
                .on_click(with(me, |this, _, cx| {
                    this.dialog = None;
                    cx.notify();
                }))
        };
        let header = |kicker: &str, title: String| {
            div()
                .h(px(66.))
                .flex_none()
                .px(px(17.))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(p.border_subtle)
                .child(div().flex().flex_col().child(div().text_size(fs(9.5)).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child(tr_upper(kicker))).child(div().mt(px(3.)).text_size(fs(17.)).css_weight(700.).text_color(p.text_strong).child(title)))
                .child(close_x("dlg-close"))
        };
        let footer_btn = |id: &'static str, label: &'static str, danger: bool| {
            let (hover_bg, hover_border, hover_fg) = if danger { (p.danger, p.danger, gpui_kit::white()) } else { (p.bg_hover, p.border_strong, p.text) };
            div()
                .id(id)
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .border_1()
                .border_color(if danger { p.danger_border } else { p.border })
                .rounded(px(8.))
                .bg(if danger { p.danger_soft } else { p.bg_3 })
                .text_color(if danger { p.danger_text } else { p.text })
                .text_size(p2_ui::tokens::font_size::ROOT)
                .css_weight(650.)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).border_color(hover_border).text_color(hover_fg))
                .child(label)
        };
        let compact = |title: String, body: String, confirm: AnyElement| {
            div()
                .p(px(22.))
                .child(div().mb(px(7.)).text_size(fs(17.)).css_weight(700.).text_color(p.text_strong).child(title))
                .child(div().text_size(fs(11.)).line_height(gpui_kit::relative(1.55)).text_color(p.text_2).child(body))
                .child(
                    div()
                        .mt(px(18.))
                        .flex()
                        .justify_end()
                        .gap(px(7.))
                        .child(footer_btn("dlg-cancel", "Vazgeç", false).on_click(with(me, |this, _, cx| {
                            this.dialog = None;
                            cx.notify();
                        })))
                        .child(confirm),
                )
        };
        let card: AnyElement = match dialog {
            Dialog::Delete(m) => {
                let id = m.id.clone();
                compact(
                    "Mesaj silinsin mi?".into(),
                    if self.can_delete_unlimited || m.author.id != self.me_id() { "Bu içerik kalıcı olarak silinecek.".into() } else { "Mesajlar yalnızca gönderildikten sonraki 2 saat içinde silinebilir.".into() },
                    footer_btn("dlg-delete", "Mesajı sil", true).on_click(with(me, move |this, window, cx| this.delete(id.clone(), window, cx))).into_any_element(),
                )
                .into_any_element()
            }
            Dialog::Leave => {
                let name = channel.map(|c| c.name.clone()).unwrap_or_default();
                compact(
                    format!("#{name} kanalından ayrılasın mı?"),
                    "Kanal kenar çubuğundan kaldırılır ve bildirim almazsın. Proje menüsündeki “Kanallara göz at” ile istediğin zaman geri dönebilirsin.".into(),
                    footer_btn("dlg-leave", "Kanaldan ayrıl", true)
                        .on_click(with(me, |this, _, cx| {
                            this.dialog = None;
                            cx.emit(ChatEvent::LeaveChannel);
                            cx.notify();
                        }))
                        .into_any_element(),
                )
                .into_any_element()
            }
            Dialog::History(rows, loading) => {
                let mut list = div().id("history-list").max_h(px(420.)).overflow_y_scroll().p(px(12.)).flex().flex_col();
                for r in rows {
                    list = list.child(
                        div()
                            .mb(px(8.))
                            .p(px(10.))
                            .border_1()
                            .border_color(p.border_subtle)
                            .rounded(px(8.))
                            .bg(p.bg_3)
                            .child(div().text_size(fs(9.)).text_color(p.text_4).child(tr_datetime(&r.created_at)))
                            .child(div().mt(px(5.)).text_size(fs(12.)).line_height(gpui_kit::relative(1.45)).text_color(p.text).child(r.content.clone())),
                    );
                }
                if rows.is_empty() && !loading {
                    list = list.child(div().mt(px(5.)).text_size(fs(12.)).text_color(p.text).child("Önceki sürüm bulunamadı."));
                }
                div().child(header("Düzenleme geçmişi", "Eski sürümler".into())).child(list).into_any_element()
            }
            // D ajanı: grup detayları (GroupChat.tsx).
            Dialog::Details if channel.is_none() => match &self.subject {
                ChatSubject::Group(g) => div().child(header("Grup detayları", g.name.clone())).child(self.group_details_rows(g, p)).into_any_element(),
                _ => return None,
            },
            Dialog::Details => {
                let c = channel?;
                let row = |k: &'static str, v: String, last: bool| {
                    div()
                        .py(px(9.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .when(!last, |s| s.border_b_1().border_color(p.border_subtle))
                        .child(div().text_size(fs(10.)).text_color(p.text_3).child(k))
                        .child(div().max_w(relative_62()).text_size(fs(11.)).css_weight(650.).text_color(p.text).child(v))
                };
                let kind = match c.kind {
                    ChannelType::Text => "Metin kanalı",
                    ChannelType::Announcement => "Duyuru kanalı",
                    ChannelType::Voice => "Sesli oda",
                    ChannelType::Forum => "Forum",
                };
                div()
                    .child(header("Kanal detayları", format!("#{}", c.name)))
                    .child(
                        div()
                            .pt(px(14.))
                            .px(px(17.))
                            .pb(px(18.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(row("Tür", kind.into(), false))
                            .child(row("Kanal kimliği", c.id.chars().take(8).collect(), false))
                            .child(row("Üye", format!("{} kişi", self.members().len()), false))
                            .child(row("Bildirim", if self.head.notify == "MUTED" { "Sessize alındı".into() } else { "Yalnızca etiketler".into() }, false))
                            .child(row("Yıldız", if self.head.starred { "Yıldızlı".into() } else { "Yıldızsız".into() }, true)),
                    )
                    .into_any_element()
            }
            Dialog::Settings { name, topic, busy, error } => {
                let c = channel?;
                let label = |t: &'static str| div().mb(px(6.)).text_size(fs(11.)).css_weight(650.).text_color(p.text_2).child(t);
                div()
                    .child(header("Kanal ayarları", format!("#{}", c.name)))
                    .child(
                        div()
                            .p(px(17.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(div().child(label("Kanal adı")).child(Input::new(name)))
                            .child(div().child(label("Konu")).child(Input::new(topic)))
                            .child(div().text_size(fs(10.)).text_color(p.text_3).child("Konu, kanal başlığında adın altında görünür ve herkes tarafından görülür."))
                            .children(error.clone().map(|e| div().text_size(fs(10.)).text_color(p.danger_text).child(e)))
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap(px(7.))
                                    .child(footer_btn("set-cancel", "Vazgeç", false).on_click(with(me, |this, _, cx| {
                                        this.dialog = None;
                                        cx.notify();
                                    })))
                                    .child(
                                        div()
                                            .id("set-save")
                                            .h(px(34.))
                                            .px(px(12.))
                                            .flex()
                                            .items_center()
                                            .rounded(px(8.))
                                            .bg(p.accent_grad.background())
                                            .text_color(p.text_on_accent)
                                            .css_weight(650.)
                                            .cursor_pointer()
                                            .child(if *busy { "Kaydediliyor…" } else { "Kaydet" })
                                            .on_click(with(me, |this, window, cx| this.save_channel_settings(window, cx))),
                                    ),
                            ),
                    )
                    .into_any_element()
            }
        };
        let compact_card = matches!(dialog, Dialog::Delete(_) | Dialog::Leave);
        Some(
            div()
                .id("chat-dialog-layer")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                .on_mouse_down(MouseButton::Left, {
                    let me = me.clone();
                    move |_, _, cx| {
                        let _ = me.update(cx, |this, cx| {
                            this.dialog = None;
                            cx.notify();
                        });
                    }
                })
                .child(
                    div()
                        .id("chat-dialog")
                        .w(px(400.))
                        .max_h(px(520.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(18.))
                        .bg(p.glass_strong.background())
                        .shadow(p.shadow_lg())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(compact_card, |s| s)
                        .child(card)
                        .p2_enter("chat-dialog", Entrance::PopElastic, Duration::ZERO),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ görüntüleyici

    fn viewer_view(&self, p: &Palette, me: &Weak, window: &Window) -> Option<AnyElement> {
        let (file, zoom) = self.viewer.clone()?;
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        let (w, h) = ((1180f32).min(vw * 0.96), (850f32).min(vh * 0.92));
        let kind = preview_kind(&file.name, &file.mime);
        let stage_w = w - 2.;
        let stage_h = h - 2. - 52.;
        let body: AnyElement = match (kind, self.images().get(&file.path())) {
            (Kind::Image | Kind::Svg, Some(Slot::Ready(image, iw, ih))) => {
                let base = (stage_w * 0.88 / *iw as f32).min(stage_h * 0.88 / *ih as f32).min(1.);
                let (dw, dh) = (*iw as f32 * base * zoom, *ih as f32 * base * zoom);
                let zoomer = me.clone();
                div()
                    .id("viewer-stage")
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_scroll(move |e: &ScrollEvent, window, cx| {
                        let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                        let _ = zoomer.update(cx, |this, cx| {
                            if let Some((_, z)) = this.viewer.as_mut() {
                                *z = (*z * if dy > 0. { 1.12 } else { 0.89 }).clamp(0.2, 8.);
                                cx.notify();
                            }
                        });
                    })
                    .on_click(with(me, |this, _, cx| {
                        // Çift tıklama: sıfırla (tek tıklamada da güvenli).
                        let _ = cx;
                        let _ = this;
                    }))
                    .child(img(ImageSource::Render(image.clone())).w(px(dw)).h(px(dh)).object_fit(ObjectFit::Contain))
                    .child(div().absolute().right(px(14.)).bottom(px(14.)).px(px(8.)).py(px(5.)).rounded(px(6.)).bg(p2_ui::tokens::css_rgba(0, 0, 0, 0.66)).text_color(gpui_kit::white()).text_size(fs(10.)).child(format!("{}%", (zoom * 100.).round())))
                    .into_any_element()
            }
            (Kind::Text, Some(Slot::Text(t))) => div()
                .id("viewer-text")
                .size_full()
                .overflow_y_scroll()
                .py(px(22.))
                .px(px(26.))
                .bg(p.bg_0)
                .font_family(FONT_MONO)
                .text_size(px(12.))
                .line_height(gpui_kit::relative(1.6))
                .text_color(p.text)
                .child(t.clone())
                .into_any_element(),
            (Kind::Image | Kind::Svg | Kind::Text, None | Some(Slot::Loading)) => div().size_full().flex().items_center().justify_center().text_color(p.text_3).child(if kind == Kind::Text { "Metin yükleniyor…" } else { "Görsel yükleniyor…" }).into_any_element(),
            _ => self.viewer_doc_body(&file, zoom, p).unwrap_or_else(|| div().size_full().flex().items_center().justify_center().text_color(p.text_3).child("Bu dosya türü için önizleme henüz yok; dosyayı indirebilirsin.").into_any_element()),
        };
        let (hover_bg, border_strong, soft) = (p.bg_hover, p.border_strong, p.accent_soft);
        let dl = file.clone();
        Some(
            div()
                .id("viewer-layer")
                .absolute()
                .left_0()
                .top(px(-ROOT_TOP))
                .w(px(vw))
                .h(px(vh))
                .flex()
                .items_center()
                .justify_center()
                .bg(p2_ui::tokens::css_rgba(2, 4, 10, 0.86)).backdrop_blur(p2_ui::fx::BLUR_SM)
                .on_mouse_down(MouseButton::Left, {
                    let me = me.clone();
                    move |_, _, cx| {
                        let _ = me.update(cx, |this, cx| {
                            this.viewer = None;
                            cx.notify();
                        });
                    }
                })
                .child(
                    div()
                        .id("viewer")
                        .w(px(w))
                        .h(px(h))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(18.))
                        .bg(p.bg_0)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .h(px(52.))
                                .flex_none()
                                .pl(px(17.))
                                .pr(px(14.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(15.))
                                .border_b_1()
                                .border_color(p.border_subtle)
                                .bg(p.bg_1)
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.))
                                        .child(div().truncate().text_size(fs(12.)).css_weight(700.).text_color(p.text_strong).child(file.name.clone()))
                                        .child(div().text_size(fs(9.)).text_color(p.text_3).child(format!("{} · {}", format_size(file.size), if file.mime.is_empty() { "Bilinmeyen tür".to_string() } else { file.mime.clone() }))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(7.))
                                        .child(
                                            div()
                                                .id("viewer-download")
                                                .h(px(30.))
                                                .px(px(11.))
                                                .flex()
                                                .items_center()
                                                .border_1()
                                                .border_color(p.border)
                                                .rounded(px(8.))
                                                .bg(p.bg_3)
                                                .text_size(fs(10.))
                                                .text_color(p.text)
                                                .cursor_pointer()
                                                .hover(move |s| s.bg(hover_bg).border_color(border_strong))
                                                .child("İndir")
                                                .on_click(with(me, move |this, window, cx| this.download(dl.clone(), window, cx))),
                                        )
                                        .child(
                                            div()
                                                .id("viewer-close")
                                                .size(px(30.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .cursor_pointer()
                                                .hover(move |s| s.bg(soft))
                                                .child(icon("composer:close").stroke(1.5).el().size(px(14.)).text_color(p.text))
                                                .tooltip(super::chat_header::tip("Kapat"))
                                                .on_click(with(me, |this, _, cx| {
                                                    this.viewer = None;
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                        )
                        .child(div().flex_1().min_h_0().bg(p.bg_0).child(body)),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ kanal ayarları

    pub(super) fn open_channel_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ChatSubject::Channel(c) = &self.subject else { return };
        let (n, t) = (c.name.clone(), c.topic.clone().unwrap_or_default());
        let name = cx.new(|cx| InputState::new(window, cx));
        name.update(cx, |s, cx| {
            s.set_value(n, window, cx);
            s.focus(window, cx);
        });
        let topic = cx.new(|cx| InputState::new(window, cx).placeholder("Bu kanal ne için kullanılıyor?"));
        topic.update(cx, |s, cx| s.set_value(t, window, cx));
        self.dialog = Some(Dialog::Settings { name, topic, busy: false, error: None });
        cx.notify();
    }

    fn save_channel_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Settings { name, topic, busy, .. }) = &mut self.dialog else { return };
        if *busy {
            return;
        }
        *busy = true;
        let (n, t) = (name.read(cx).value().trim().to_string(), topic.read(cx).value().trim().to_string());
        let (api, target) = self.api();
        let ChatTarget::Channel { org, project, channel } = target else { return };
        cx.notify();
        crate::task::run(window, cx, move || api.update_channel(&org, &project, &channel, serde_json::json!({ "name": n, "topic": t })), |this, r, _, cx| match r {
            Ok(v) => {
                this.dialog = None;
                if let (ChatSubject::Channel(c), Ok(updated)) = (&mut this.subject, serde_json::from_value::<p2_client_core::Channel>(v)) {
                    c.name = updated.name.clone();
                    c.topic = updated.topic.clone();
                    cx.emit(ChatEvent::ChannelUpdated(updated));
                }
                cx.notify();
            }
            Err(e) => {
                if let Some(Dialog::Settings { busy, error, .. }) = &mut this.dialog {
                    *busy = false;
                    *error = Some(if e.message.is_empty() { "Kanal güncellenemedi.".into() } else { e.message });
                }
                cx.notify();
            }
        });
    }
}

fn relative_62() -> gpui_kit::DefiniteLength {
    gpui_kit::relative(0.62)
}

/// `.search-field`: 34px, ikon 15 (kontur `text-4`), odakta `border-accent` + halka.
pub(super) fn search_field(input: &gpui_kit::Entity<InputState>, p: &Palette, focused: bool) -> gpui_kit::Div {
    div()
        .h(px(34.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .border_1()
        .border_color(if focused { p.border_accent } else { p.border })
        .rounded(px(8.))
        .bg(p.bg_3)
        .when(focused, |s| s.shadow(p2_ui::tokens::shadows(&[p.ring])))
        .child(div().w(px(22.)).child(icon("composer:search").stroke(1.8).el().size(px(15.)).text_color(p.text_4)))
        .child(div().flex_1().min_w_0().child(Input::new(input).appearance(false).with_size(gpui_kit::component::Size::XSmall).text_size(fs(12.))))
}

/// messageMenu.tsx `copy`: dosya işaretleri ve kod çitleri ayıklanır.
fn copy_text(value: &str) -> String {
    let clean: String = value.lines().filter(|l| !(l.starts_with("[[p2-file|") && l.ends_with("]]"))).collect::<Vec<_>>().join("\n");
    p2_ui::rich::without_code_fences(clean.trim())
}

/// `.presence` noktası: çevrimiçi yeşil, uzakta sarı, meşgul kırmızı, aksi `text-4`.
fn presence_fill(status: &str, p: &Palette) -> gpui_kit::Hsla {
    match status {
        "ONLINE" => p.success,
        "AWAY" => p.warning,
        "BUSY" => p.danger,
        _ => p.text_4,
    }
}
