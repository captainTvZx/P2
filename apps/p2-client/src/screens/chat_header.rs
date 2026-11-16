//! Kanal başlığı — ChannelContent.tsx `<section className="channel-content"><header>…`
//! (B ajanı: çalışma alanı iskeleti). Stiller: channel-content.css satır 6–11, direct.css
//! `.channel-tools`, `.facepile`, `.star-inline`. Boyutlar CSS px; yazı `fs()` ölçekli.
//!
//! Açılır menüler (üyeler, bildirim tercihleri, diğer işlemler) C ajanı: içerik ve konum
//! chat_float.rs `tool_menu_view`; düğmeler burada açar/kapatır, açıkken `.active` görünür.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, div, linear_color_stop, linear_gradient, px};
use p2_client_core::{Channel, ChannelType};
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::icon;
use p2_ui::tokens::{Palette, fs};

use super::{ChatEvent, ChatPane};

/// Başlığın çalışma alanından gelen durumu (yıldız, bildirim tercihi, kuruluş üye sayısı).
#[derive(Clone, Default)]
pub struct HeadState {
    pub starred: bool,
    /// ALL | MENTIONS | MUTED
    pub notify: String,
    /// I ajanı: bu kanaldaki aramadayım (`.tool.call-active`, yeşil arama düğmesi).
    pub in_call: bool,
    /// I ajanı: sesli odada "Oda sohbeti" açık (`.voice-chat-toggle.active`).
    pub voice_chat: bool,
}

/// Electron `headerText`.
fn header_text(channel: &Channel) -> String {
    if let Some(t) = channel.topic.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    match channel.kind {
        ChannelType::Forum => "Konular ve tartışmalar",
        ChannelType::Voice => "Sesli oda ve yazılı sohbet",
        ChannelType::Announcement => "Proje duyuruları",
        ChannelType::Text => "Ekip konuşmaları",
    }
    .into()
}

/// direct.css `.channel-tools .tool` (32px yükseklik, ikon 18px).
fn tool(id: &'static str, body: &'static str, stroke: f32, color: gpui_kit::Hsla, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    div()
        .id(id)
        .group(id)
        .relative()
        .flex_none()
        .h(px(32.))
        .min_w(px(32.))
        .px(px(7.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .cursor_pointer()
        .hover(move |s| s.bg(soft).top(px(-1.)))
        .child(icon(body).stroke(stroke).el().size(px(18.)).text_color(color).group_hover(id, move |s| s.text_color(strong)))
}

pub(super) fn tip(text: &'static str) -> impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView + 'static {
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text).build(window, cx)
}

pub(super) fn tip_owned(text: String) -> impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView + 'static {
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

impl ChatPane {
    pub(super) fn channel_header(&self, channel: &Channel, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (soft, strong, warning) = (p.accent_soft, p.text_strong, p.warning);
        let starred = self.head.starred;
        let star_color = if starred { warning } else { p.text_3 };
        let star = div()
            .id("head-star")
            .group("head-star")
            .flex_none()
            .size(px(32.))
            .mr(px(-3.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .hover(move |s| s.bg(soft))
            .child({
                let i = icon("star").stroke(1.8);
                let i = if starred { i.filled() } else { i };
                i.el().size(px(18.)).text_color(star_color).when(!starred, |s| s.group_hover("head-star", move |s| s.text_color(strong)))
            })
            .tooltip(tip(if starred { "Yıldızı kaldır" } else { "Yıldıza ekle" }))
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::ToggleStar)));
        let glyph: AnyElement = match channel.kind {
            ChannelType::Text => div().flex_none().text_size(fs(22.)).text_color(p.text_accent).child("#").into_any_element(),
            kind => icon(match kind {
                ChannelType::Announcement => "channel:announcement",
                ChannelType::Voice => "channel:voice",
                _ => "channel:forum",
            })
            .el()
            .size(px(18.))
            .text_color(p.text_accent)
            .into_any_element(),
        };
        // Üye yığını: ilk üç üye + toplam (direct.css `.facepile`).
        let mut pile = div().flex().items_center().gap(px(5.));
        for m in self.members.iter().take(3) {
            let hue = p.hues[avatar_hue(&m.id)];
            // 2px `bg-1` kenarlık: GPUI kenarlık halesi olmasın diye dış zemin + iç kare.
            pile = pile.child(
                div().flex_none().size(px(23.)).mr(px(-7.)).p(px(2.)).rounded(px(8.)).bg(p.bg_1).child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
                        .text_color(hue.fg)
                        .text_size(fs(9.))
                        .line_height(px(19.))
                        .css_weight(800.)
                        .child(p2_ui::components::initials_label(user_initials(&m.display_name))),
                ),
            );
        }
        let count = self.members.len();
        let facepile = div()
            .id("head-members")
            .relative()
            .flex_none()
            .h(px(32.))
            .pl(px(7.))
            .pr(px(20.))
            .mr(px(12.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .cursor_pointer()
            .hover(move |s| s.bg(soft).top(px(-1.)))
            .child(pile)
            .child(
                div().absolute().right(px(4.)).top(px(-2.)).p(px(2.)).rounded(px(9.5)).bg(p.bg_1).child(
                    div()
                        .min_w(px(15.))
                        .h(px(15.))
                        .px(px(2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.5))
                        .bg(p.accent)
                        .text_color(gpui_kit::white())
                        .text_size(fs(9.5))
                        .line_height(fs(9.5))
                        .css_weight(700.)
                        .child(count.to_string()),
                ),
            )
            .when(self.tool_menu == Some(super::ToolMenu::Members), |s| s.bg(p.accent_soft_2))
            .tooltip(tip("Üyeler"))
            .on_click(cx.listener(|this, _, window, cx| this.open_tool_menu(super::ToolMenu::Members, window, cx)));
        let muted = self.head.notify == "MUTED";
        let (notify_open, more_open) = (self.tool_menu == Some(super::ToolMenu::Notify), self.tool_menu == Some(super::ToolMenu::More));
        let tools = div()
            .ml_auto()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            // I ajanı: sesli odada "Sohbeti göster/gizle" (`.tool.voice-chat-toggle`: 17px ikon, 10px/700 yazı).
            .when(channel.kind == ChannelType::Voice, |s| {
                let open = self.head.voice_chat;
                let (fg, strong) = (if open { p.text_strong } else { p.text_3 }, p.text_strong);
                let label: &'static str = if open { "Sohbeti gizle" } else { "Sohbeti göster" };
                s.child(
                    div()
                        .id("head-voice-chat")
                        .group("head-voice-chat")
                        .flex_none()
                        .h(px(32.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .text_color(fg)
                        .when(open, |s| s.bg(p.accent_soft_2))
                        .hover(move |s| s.bg(soft).top(px(-1.)).text_color(strong))
                        .child(icon("composer:thread").stroke(1.8).el().size(px(17.)).text_color(fg).group_hover("head-voice-chat", move |s| s.text_color(strong)))
                        .child(div().text_size(fs(10.)).css_weight(700.).whitespace_nowrap().child(label))
                        .tooltip(tip(label))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::ToggleVoiceChat))),
                )
            })
            .child(facepile)
            .child({
                // `.tool.call-active` (call.css): bu kanaldaki aramadayken yeşil.
                let active = self.head.in_call;
                tool("head-call", "call", 1.8, if active { p.success_text } else { p.text_3 }, p)
                    .tooltip(tip(if active { "Aramadasın" } else { "Bu kanalda arama başlat" }))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::StartCall)))
            })
            .child(
                tool("head-search", "search", 1.8, p.text_3, p)
                    .when(self.search.is_some(), |s| s.bg(p.accent_soft_2))
                    .tooltip(tip("Kanalda ara"))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_search(window, cx))),
            )
            .child(
                tool("head-notify", "bell", 1.8, if notify_open { p.text_strong } else if muted { p.text_4 } else { p.text_3 }, p)
                    .when(notify_open, |s| s.bg(p.accent_soft_2))
                    .tooltip(tip("Bildirim tercihleri"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_tool_menu(super::ToolMenu::Notify, window, cx))),
            )
            // L ajanı: `{splitPane&&<button className="tool" title="Bölünmüş görünümü kapat">✕</button>}`.
            .when(self.split_pane, |s| {
                let (soft, strong) = (p.accent_soft, p.text_strong);
                s.child(
                    div()
                        .id("head-split-close")
                        .flex_none()
                        .h(px(32.))
                        .min_w(px(32.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .cursor_pointer()
                        .text_size(fs(16.))
                        .text_color(p.text_3)
                        .hover(move |s| s.bg(soft).top(px(-1.)).text_color(strong))
                        .child("✕")
                        .tooltip(tip("Bölünmüş görünümü kapat"))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::CloseSplit))),
                )
            })
            .child(
                tool("head-more", "more", 3.1, if more_open { p.text_strong } else { p.text_3 }, p)
                    .when(more_open, |s| s.bg(p.accent_soft_2))
                    .tooltip(tip("Diğer işlemler"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_tool_menu(super::ToolMenu::More, window, cx))),
            );
        div()
            .h(px(58.))
            .flex_none()
            // Chromium yarım pikselleri (ortalanmış 32px araçlar y=50.5) aşağı yuvarlıyor; GPUI yukarı.
            // 1px üst boşluk ortalamayı tam piksele taşır, görüntü Electron'la aynı satıra oturur.
            .pt(px(1.))
            .px(px(18.))
            .flex()
            .items_center()
            .gap(px(11.))
            .bg(p.bg_1)
            .border_b_1()
            .border_color(p.border_subtle)
            .child(star)
            .child(glyph)
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().text_size(fs(15.)).css_weight(700.).text_color(p.text_strong).child(channel.name.clone()))
                    .child(div().truncate().text_size(fs(11.)).text_color(p.text_3).child(header_text(channel))),
            )
            .child(tools)
            .into_any_element()
    }
}

/// L ajanı: `.tool` "Bölünmüş görünümü kapat" (✕) — DM/grup başlığı için.
pub(super) fn split_close(id: &'static str, p: &Palette, cx: &mut gpui_kit::Context<ChatPane>) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    div()
        .id(id)
        .flex_none()
        .h(px(32.))
        .min_w(px(32.))
        .px(px(7.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(fs(16.))
        .text_color(p.text_3)
        .hover(move |s| s.bg(soft).top(px(-1.)).text_color(strong))
        .child("✕")
        .tooltip(tip("Bölünmüş görünümü kapat"))
        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::CloseSplit)))
}
