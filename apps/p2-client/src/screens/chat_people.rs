//! Bireysel ve grup sohbeti kabuğu (D ajanı) — DirectMessages.tsx / GroupChat.tsx başlığı
//! (avatar, ad, alt satır, `.channel-tools`: arama, üyeler, ••• ConversationMenu), boş sohbet
//! karşılaması (`.channel-welcome`) ve ••• menüsü. Mesaj satırları C ajanının (chat.rs).
//! Stiller: channel-content.css 6–11, 16–20; direct.css 31–39, 180–186, 244–251.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, ClipboardItem, Context, Hsla, WeakEntity, Window, div, linear_color_stop, linear_gradient, px};
use p2_client_core::{ChatGroup, Member};
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::icon;
use p2_ui::tokens::{Palette, fs, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::chat_header::{tip, tip_owned};
use super::view::{ROOT_TOP, Tone, menu_hr, menu_item, menu_panel, place_menu};
use super::{ChatEvent, ChatPane, ChatSubject, ToolMenu};

type Weak = WeakEntity<ChatPane>;

fn with<F: Fn(&mut ChatPane, &mut Window, &mut Context<ChatPane>) + 'static>(me: &Weak, f: F) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    let me = me.clone();
    move |_, window, cx| {
        let _ = me.update(cx, |this, cx| f(this, window, cx));
    }
}

fn status_text(status: &str) -> &'static str {
    match status {
        "ONLINE" => "Çevrimiçi",
        "AWAY" => "Uzakta",
        "BUSY" => "Meşgul",
        "INVISIBLE" => "Görünmez",
        _ => "Çevrimdışı",
    }
}

/// direct.css `.presence` (11px, 2px `ring` halka) — halka dış daire + iç nokta olarak (GPUI kenarlık halesi yok).
fn presence_ring(status: &str, ring: Hsla, p: &Palette) -> gpui_kit::Div {
    // Ortak çizim: workspace_nav::presence_mark ile aynı (p2_ui::controls::presence_ring).
    p2_ui::controls::presence_ring(p2_ui::controls::Presence::from_status(status), ring, p)
}

/// direct.css `.channel-tools .tool` (32px, ikon 18px, hover 1px yukarı).
fn tool(id: &'static str, glyph: AnyElement, active: bool, disabled: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let soft = p.accent_soft;
    div()
        .id(id)
        .group(id)
        .relative()
        .flex_none()
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .when(active, |s| s.bg(p.accent_soft_2))
        .when(disabled, |s| s.opacity(0.45))
        .when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(soft).top(px(-1.))))
        .child(glyph)
}

fn tool_icon(id: &'static str, name: &'static str, stroke: f32, active: bool, p: &Palette) -> AnyElement {
    let strong = p.text_strong;
    icon(name).stroke(stroke).el().size(px(18.)).text_color(if active { strong } else { p.text_3 }).group_hover(id, move |s| s.text_color(strong)).into_any_element()
}

/// `.channel-content > header` kabı (58px, `bg-1`).
fn header_shell(p: &Palette) -> gpui_kit::Div {
    div()
        .h(px(58.))
        .flex_none()
        // chat_header.rs ile aynı: Chromium yarım pikseli aşağı yuvarlar.
        .pt(px(1.))
        .px(px(18.))
        .flex()
        .items_center()
        .gap(px(11.))
        .bg(p.bg_1)
        .border_b_1()
        .border_color(p.border_subtle)
}

impl ChatPane {
    /// DirectMessages.tsx başlığı: tıklanır avatar (durum noktalı) + ad / "@kullanıcı · Durum", Sesli ara, •••.
    pub(super) fn direct_header(&self, person: &Member, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let hue = p.hues[avatar_hue(&person.id)];
        let (a, b) = (person.id.clone(), person.id.clone());
        let face: AnyElement = match crate::avatars::image(&person.id) {
            Some(image) => gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().rounded(px(12.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
            None => user_initials(&person.display_name).into_any_element(),
        };
        let avatar = div()
            .id("dm-avatar")
            .relative()
            .flex_none()
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
            .text_color(p.text_accent)
            .text_size(fs(11.))
            // Satır kutusu = kutu yüksekliği: baş harfler tam ortada.
            .line_height(px(36.))
            .css_weight(800.)
            .tracking(-0.02)
            .cursor_pointer()
            .hover(|s| s.opacity(0.92))
            .child(face)
            .child(presence_ring(&person.status, p.bg_1, p).absolute().right(px(-3.)).bottom(px(-3.)))
            .tooltip(tip_owned(format!("{} profilini aç", person.display_name)))
            .on_click(cx.listener(move |this, _, _, cx| this.open_profile(a.clone(), cx)));
        let title = div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .id("dm-title")
                    .truncate()
                    .text_size(fs(15.))
                    .css_weight(700.)
                    .text_color(p.text_strong)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(person.display_name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.open_profile(b.clone(), cx))),
            )
            .child(div().truncate().text_size(fs(11.)).text_color(p.text_3).child(format!("@{} · {}", person.username, status_text(&person.status))));
        let more_open = self.tool_menu == Some(ToolMenu::More);
        let tools = div()
            .ml_auto()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                tool("dm-call", tool_icon("dm-call", "call", 1.8, false, p), false, false, p)
                    .tooltip(tip_owned(format!("{} kişisini ara", person.display_name)))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::StartCall))),
            )
            .when(self.split_pane, |s| s.child(super::chat_header::split_close("dm-split-close", p, cx)))
            .child(
                tool("dm-more", tool_icon("dm-more", "more", 3.1, more_open, p), more_open, false, p)
                    .tooltip(tip("Diğer işlemler"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_tool_menu(ToolMenu::More, window, cx))),
            );
        header_shell(p).child(avatar).child(title).child(tools).into_any_element()
    }

    /// GroupChat.tsx başlığı: grup avatarı + ad / "N kişi · adlar", Üyeler (açılır), Grubu ara, •••.
    pub(super) fn group_header(&self, group: &ChatGroup, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let hue = p.hues[8];
        let names = group.members.iter().map(|m| m.display_name.as_str()).collect::<Vec<_>>().join(", ");
        let (members_open, more_open) = (self.tool_menu == Some(ToolMenu::Members), self.tool_menu == Some(ToolMenu::More));
        let can_call = group.members.len() >= 2;
        let tools = div()
            .ml_auto()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                tool("grp-members", tool_icon("grp-members", "call:personAdd", 1.8, members_open, p), members_open, false, p)
                    .tooltip(tip("Üyeler"))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_group_members(cx))),
            )
            .child(
                tool("grp-call", tool_icon("grp-call", "call", 1.8, false, p), false, !can_call, p)
                    .tooltip(tip(if can_call { "Grubu ara" } else { "Aramak için gruba en az bir kişi daha ekle" }))
                    .when(can_call, |s| s.on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::StartCall)))),
            )
            .when(self.split_pane, |s| s.child(super::chat_header::split_close("grp-split-close", p, cx)))
            .child(
                tool("grp-more", tool_icon("grp-more", "more", 3.1, more_open, p), more_open, false, p)
                    .tooltip(tip("Diğer işlemler"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_tool_menu(ToolMenu::More, window, cx))),
            );
        header_shell(p)
            .gap(px(9.))
            .child(
                div()
                    .flex_none()
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
                    .text_color(p.text_accent)
                    .text_size(fs(12.))
                    .line_height(px(34.))
                    .css_weight(800.)
                    .child(user_initials(&group.name)),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(div().truncate().text_size(fs(15.)).css_weight(700.).text_color(p.text_strong).child(group.name.clone()))
                    .child(div().mt(px(2.)).truncate().text_size(fs(10.5)).text_color(p.text_3).child(format!("{} kişi · {names}", group.members.len()))),
            )
            .child(tools)
            .into_any_element()
    }

    /// Boş sohbet `.channel-welcome`: avatar kutusu (74px, `accent-soft`), başlık, açıklama.
    pub(super) fn people_welcome(&self, p: &Palette) -> Option<AnyElement> {
        let (initials, title, body, radius, size) = match &self.subject {
            ChatSubject::Direct(m) => (user_initials(&m.display_name), m.display_name.clone(), format!("Bu, {} ile aranızdaki bireysel sohbetin başlangıcı.", m.display_name), 25., fs(22.)),
            ChatSubject::Group(g) => (user_initials(&g.name), g.name.clone(), format!("Bu grubun başlangıcı. {} kişi burada yazışabilir.", g.members.len()), 25., fs(24.)),
            ChatSubject::Channel(_) => return None,
        };
        Some(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .justify_end()
                .px(px(24.))
                .pb(px(36.))
                .child(
                    div()
                        .size(px(74.))
                        .mb(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.accent_soft)
                        .text_color(p.text_accent)
                        .text_size(size)
                        .line_height(px(72.))
                        .css_weight(800.)
                        .shadow(shadows(&[p.accent_glow]))
                        .child(initials),
                )
                .child(div().mt(px(14.)).mb(px(5.)).text_size(fs(24.)).css_weight(700.).text_color(p.text_strong).child(title))
                .child(div().text_size(fs(12.)).text_color(p.text_3).child(body))
                .p2_enter("people-welcome", Entrance::Rise, Duration::ZERO)
                .into_any_element(),
        )
    }

    /// "Üyeler" düğmesi: açılır liste çalışma alanında (ekleme/çıkarma orada); düğme etkin görünür.
    fn toggle_group_members(&mut self, cx: &mut Context<Self>) {
        let ChatSubject::Group(g) = &self.subject else { return };
        let g = g.clone();
        self.menu = None;
        self.tool_menu = Some(ToolMenu::Members);
        cx.emit(ChatEvent::ManageGroup(g));
        cx.notify();
    }

    /// Çalışma alanı üye listesini açınca "Üyeler" düğmesi etkin görünür.
    pub(crate) fn mark_people_menu(&mut self, cx: &mut Context<Self>) {
        if matches!(self.subject, ChatSubject::Group(_)) {
            self.tool_menu = Some(ToolMenu::Members);
            cx.notify();
        }
    }

    /// Çalışma alanı üye listesini kapatınca düğmenin etkin görünümü söner.
    pub(crate) fn close_people_menu(&mut self, cx: &mut Context<Self>) {
        if self.tool_menu == Some(ToolMenu::Members) {
            self.tool_menu = None;
            cx.notify();
        }
    }

    /// GroupChat.tsx `.group-system`: "X gruptan ayrıldı." gibi sistem satırı (ortalı hap).
    pub(super) fn group_system_row(&self, message: &p2_client_core::Message, p: &Palette) -> AnyElement {
        div()
            .mx(px(12.))
            .my(px(6.))
            .py(px(6.))
            .px(px(10.))
            .rounded(px(8.))
            .bg(p.bg_3)
            .text_color(p.text_3)
            .text_size(fs(10.5))
            .text_center()
            .child(message.content.clone())
            .into_any_element()
    }

    /// GroupChat.tsx "Grup detayları" gövdesi (`.channel-details`); başlık ve katman chat_float'ta.
    pub(super) fn group_details_rows(&self, g: &ChatGroup, p: &Palette) -> AnyElement {
        let row = |k: &'static str, v: String, last: bool| {
            div()
                .py(px(9.))
                .flex()
                .items_center()
                .justify_between()
                .when(!last, |s| s.border_b_1().border_color(p.border_subtle))
                .child(div().text_size(fs(10.)).text_color(p.text_3).child(k))
                .child(div().max_w(gpui_kit::relative(0.62)).text_size(fs(11.)).line_height(gpui_kit::relative(1.45)).css_weight(650.).text_color(p.text).child(v))
        };
        let creator = g.created_by_id.as_deref().and_then(|id| g.members.iter().find(|m| m.id == id)).map_or("Bilinmiyor".to_string(), |m| m.display_name.clone());
        div()
            .pt(px(14.))
            .px(px(17.))
            .pb(px(18.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(row("Tür", "Grup sohbeti".into(), false))
            .child(row("Grup kimliği", g.id.chars().take(8).collect(), false))
            .child(row("Üye", format!("{} kişi", g.members.len()), false))
            .child(row("Kuran", creator, false))
            .child(row("Katılımcılar", g.members.iter().map(|m| m.display_name.as_str()).collect::<Vec<_>>().join(", "), true))
            .into_any_element()
    }

    /// ConversationMenu (••• `tool-menu`, 290px) ve grubun üye açılır listesi (`.group-add-menu`, 250px).
    pub(super) fn people_menu_view(&self, p: &Palette, me: &Weak, window: &Window) -> Option<AnyElement> {
        let kind = self.tool_menu?;
        let vw = f32::from(window.viewport_size().width);
        let y = 89.;
        let item = |id: &'static str, glyph: &'static str, label: String, tone: Tone| menu_item(id, glyph, label, tone, None, 38., p);
        let starred = self.head.starred;
        let star_label = if starred { "Yıldızı kaldır".to_string() } else { "Yıldıza ekle".to_string() };
        fn close(this: &mut ChatPane, cx: &mut Context<ChatPane>) {
            this.tool_menu = None;
            cx.notify();
        }
        let el = match (&self.subject, kind) {
            (ChatSubject::Direct(person), ToolMenu::More) => {
                let (pid, link) = (person.id.clone(), format!("p2://users/{}", person.id));
                menu_panel("tool-menu", 290., 6., p)
                    .child(item("dm-profile", "ⓘ", "Profili görüntüle".into(), Tone::Normal).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        this.open_profile(pid.clone(), cx);
                    })))
                    .child(item("dm-link", "🔗", "Sohbet bağlantısını kopyala".into(), Tone::Normal).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    })))
                    .child(item("dm-star", "★", star_label, Tone::Normal).on_click(with(me, |this, _, cx| {
                        close(this, cx);
                        cx.emit(ChatEvent::ToggleStar);
                    })))
                    .child(menu_hr(p))
                    // L ajanı: bölünmüş görünüm ve ayrı pencere.
                    .child(item("dm-split", "◫", "Bölünmüş görünümde aç".into(), if self.split_pane || self.popout { Tone::Disabled } else { Tone::Normal }).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        if !this.split_pane && !this.popout {
                            cx.emit(ChatEvent::OpenSplit);
                        }
                    })))
                    .child(item("dm-window", "⧉", "Yeni pencerede aç".into(), Tone::Normal).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        cx.emit(ChatEvent::OpenWindow);
                    })))
                    .into_any_element()
            }
            (ChatSubject::Group(group), ToolMenu::More) => {
                let link = format!("p2://groups/{}", group.id);
                let gid = group.id.clone();
                menu_panel("tool-menu", 290., 6., p)
                    .child(item("grp-details", "ⓘ", "Grup detayları".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        close(this, cx);
                        this.dialog = Some(super::Dialog::Details);
                    })))
                    .child(item("grp-link", "🔗", "Grup bağlantısını kopyala".into(), Tone::Normal).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    })))
                    .child(item("grp-add", "＋", "Üye ekle ve çıkar".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        this.tool_menu = None;
                        this.toggle_group_members(cx);
                    })))
                    .child(menu_hr(p))
                    .child(item("grp-split", "◫", "Bölünmüş görünümde aç".into(), if self.split_pane || self.popout { Tone::Disabled } else { Tone::Normal }).on_click(with(me, |this, _, cx| {
                        close(this, cx);
                        if !this.split_pane && !this.popout {
                            cx.emit(ChatEvent::OpenSplit);
                        }
                    })))
                    .child(item("grp-window", "⧉", "Yeni pencerede aç".into(), Tone::Normal).on_click(with(me, |this, _, cx| {
                        close(this, cx);
                        cx.emit(ChatEvent::OpenWindow);
                    })))
                    .child(menu_hr(p))
                    .child(item("grp-leave", "⤳", "Gruptan ayrıl".into(), Tone::Danger).on_click(with(me, move |this, _, cx| {
                        close(this, cx);
                        cx.emit(ChatEvent::LeaveGroup(gid.clone()));
                    })))
                    .into_any_element()
            }
            (ChatSubject::Group(group), ToolMenu::Members) => {
                // Üyeler açılır listesi workspace'te (ekleme/çıkarma orada); burada yalnız olay.
                let _ = group;
                return None;
            }
            _ => return None,
        };
        let width = 290.;
        let (x, _) = place_menu(vw - 18., y, y, true, width, 0., window);
        Some(div().absolute().left(px(x)).top(px(y - ROOT_TOP)).child(div().child(el).p2_enter(("people-tool-menu", kind as usize), Entrance::ScaleIn, Duration::ZERO)).into_any_element())
    }
}
