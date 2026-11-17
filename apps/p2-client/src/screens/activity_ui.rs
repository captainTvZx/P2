//! Etkinlik paneli — ActivityPanel.tsx: raydaki zil (okunmamış rozeti), etiketlenme / yanıt /
//! tepki / görev atama / sürüm listesi, "tümünü okundu say", tıklayınca ilgili yere git.
//! Çalışma alanının alt modülü.

use gpui_kit::prelude::*;
use gpui_kit::{AnimationExt as _, AnyElement, Context, SharedString, Window, div, px};
use p2_client_core::{ActivityItem, ChatGroup};
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::icon;
use p2_ui::tokens::{Palette, css_rgba, fs};
use p2_ui::{Entrance, P2Motion as _};

use super::WorkspaceScreen;
use crate::task;

/// "3 dk önce", "dün 14:20" (ActivityPanel.tsx `when`).
fn when(ts: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(ts) else { return String::new() };
    let local = t.with_timezone(&chrono::Local);
    let secs = (chrono::Local::now() - local).num_seconds();
    match secs {
        s if s < 60 => "az önce".into(),
        s if s < 3600 => format!("{} dk önce", s / 60),
        s if s < 86_400 => format!("{} sa önce", s / 3600),
        s if s < 172_800 => format!("dün {}", local.format("%H:%M")),
        _ => {
            const AY: [&str; 12] = ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];
            use chrono::Datelike as _;
            format!("{} {}", local.day(), AY[local.month0() as usize])
        }
    }
}

/// `line-height: normal` satırı; satırda emoji varsa Chromium yedek yazı tipinin (Segoe UI Emoji) daha büyük
/// çıkış/inişiyle satır 1px uzar (ölçüm: önizlemesinde 👀 olan satır 57px, diğerleri 56px).
fn line(size: gpui_kit::Pixels, text: &str) -> gpui_kit::Pixels {
    let emoji = text.chars().any(|c| matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF));
    p2_ui::text::normal_line_height(size) + px(if emoji { 1. } else { 0. })
}

thread_local! {
    /// Etkinlik listesinin kaydırma tutamağı (taşma ölçümü için; panel tek örnek).
    static ACTIVITY_SCROLL: gpui_kit::ScrollHandle = gpui_kit::ScrollHandle::new();
}

impl WorkspaceScreen {
    pub(super) fn load_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.activity(&org), |this, result, _, cx| {
            if let Ok(page) = result {
                crate::trace(&format!("etkinlik: {} öğe, {} okunmamış", page.items.len(), page.unread));
                this.activity = page.items;
                this.activity_unread = page.unread;
                cx.notify();
            }
        });
    }

    pub(super) fn on_activity(&mut self, item: ActivityItem, cx: &mut Context<Self>) {
        crate::trace(&format!("etkinlik: yeni {} ({})", item.kind, item.actor.username));
        if !self.activity.iter().any(|a| a.id == item.id) {
            self.activity.insert(0, item);
            self.activity_unread += 1;
        }
        cx.notify();
    }

    fn read_all_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = chrono::Utc::now().to_rfc3339();
        for a in &mut self.activity {
            a.read_at.get_or_insert_with(|| now.clone());
        }
        self.activity_unread = 0;
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.read_all_activity(&org), |_, _, _, _| {});
        cx.notify();
    }

    /// Öğeye tıklayınca: okundu say ve ilgili yeri aç (kanal, grup, görev panosu, dokümanlar).
    fn open_activity(&mut self, item: ActivityItem, window: &mut Window, cx: &mut Context<Self>) {
        if item.read_at.is_none() {
            if let Some(a) = self.activity.iter_mut().find(|a| a.id == item.id) {
                a.read_at = Some(chrono::Utc::now().to_rfc3339());
            }
            self.activity_unread = self.activity_unread.saturating_sub(1);
            let (api, org, id) = (self.api.clone(), self.org.id.clone(), item.id.clone());
            task::run(window, cx, move || api.read_activity(&org, &id), |_, _, _, _| {});
        }
        self.activity_open = false;
        if item.task_id.is_some() {
            self.documents = None;
            self.open_board(window, cx);
        } else if item.document_id.is_some() {
            self.open_documents(window, cx);
        } else if let Some(group) = item.group_id.as_ref().and_then(|g| self.group_by_id(g).cloned()) {
            self.close_pages();
            self.select_group(group as ChatGroup, window, cx);
        } else if let Some(channel) = item.channel_id.as_ref().and_then(|c| self.categories.iter().flat_map(|x| x.channels.iter()).find(|x| &x.id == c).cloned()) {
            self.close_pages();
            self.select_channel(channel, window, cx);
        }
        cx.notify();
    }

    fn close_pages(&mut self) {
        self.board = None;
        self.documents = None;
        self.stage = None;
    }

    /// ActivityPanel.tsx `.activity-panel` (direct.css 299–327): sağda sabit çekmece (380px,
    /// başlık 52px), satır ızgarası 34px + metin; dışarı basınca, × ya da Esc ile kapanır.
    pub(super) fn activity_panel(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.activity_open {
            return None;
        }
        let (hover, soft, strong) = (p.bg_hover, p.accent_soft, p.text_strong);
        // Chromium kaydırma çubuğu 10px yer ayırır (GPUI ayırmaz): liste taşıyorsa sağ dolgu +10.
        let scroll = ACTIVITY_SCROLL.with(|h| h.clone());
        let overflows = scroll.max_offset().y > px(0.);
        let mut list = div().id("activity-list").track_scroll(&scroll).flex_1().min_h_0().overflow_y_scroll().p(px(6.)).when(overflows, |s| s.pr(px(16.))).flex().flex_col().gap(px(2.));
        for (ix, item) in self.activity.iter().enumerate() {
            let unread = item.read_at.is_none();
            let open = item.clone();
            let hue = p.hues[avatar_hue(&item.actor.id)];
            let (badge_bg, badge_fs) = match item.kind.as_str() {
                "MENTION" => (p.accent, fs(8.)),
                "REPLY" => (p.hues[8].bg2, fs(8.)),
                "REACTION" => (p.hues[4].bg2, fs(9.)),
                _ => (p.navy_500, fs(8.)),
            };
            let badge = div()
                .absolute()
                .right(px(-4.))
                .bottom(px(-4.))
                .min_w(px(16.))
                .h(px(16.))
                .px(px(2.))
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(p.bg_2)
                .rounded(px(8.))
                .bg(badge_bg)
                .text_color(p.text_strong)
                .text_size(badge_fs)
                .line_height(p2_ui::text::normal_line_height(badge_fs))
                .css_weight(700.)
                .child(item.badge());
            let row = div()
                .id(SharedString::from(format!("act-{}", item.id)))
                .relative()
                .w_full()
                .flex_none()
                .py(px(9.))
                .px(px(10.))
                .flex()
                .gap(px(10.))
                .rounded(px(11.))
                .cursor_pointer()
                .when(unread, |s| s.bg(soft))
                .hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |this, _, window, cx| this.open_activity(open.clone(), window, cx)))
                // `.activity-row.unread::before`: solda 4px parlayan nokta.
                .when(unread, |s| {
                    s.child(div().absolute().left(px(3.)).top_0().bottom_0().flex().items_center().child(div().size(px(4.)).rounded_full().bg(p.accent_hover).shadow(vec![gpui_kit::BoxShadow {
                        color: p.accent_hover,
                        offset: gpui_kit::point(px(0.), px(0.)),
                        blur_radius: px(6.),
                        spread_radius: px(0.),
                        inset: false,
                    }])))
                })
                .child(
                    div().flex_none().w(px(34.)).child(
                        div()
                            .relative()
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(11.))
                            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                            .text_color(hue.fg)
                            .text_size(fs(11.))
                            .line_height(px(32.))
                            .css_weight(800.)
                            .child(p2_ui::components::initials_label(user_initials(&item.actor.display_name)))
                            .child(badge),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .text_size(fs(11.5))
                                .line_height(line(fs(11.5), &item.title()))
                                .text_color(p.text_2)
                                .child(div().css_weight(650.).text_color(p.text_strong).child(item.actor.display_name.clone()))
                                .child(format!("\u{a0}{}", item.title())),
                        )
                        .child(div().truncate().text_size(fs(11.)).line_height(line(fs(11.), &item.preview)).text_color(p.text_3).child(if item.preview.is_empty() { "(mesaj)".to_string() } else { item.preview.clone() }))
                        .child(div().text_size(fs(9.5)).line_height(p2_ui::text::normal_line_height(fs(9.5))).text_color(p.text_4).child(format!("{} · {}", item.place(), when(&item.created_at)))),
                );
            // `p2-rise var(--m-base)` (her satır).
            list = list.child(div().w_full().child(row).p2_enter(("act-row", ix), Entrance::Rise, std::time::Duration::ZERO));
        }
        if self.activity.is_empty() {
            list = list.child(
                div()
                    .py(px(44.))
                    .px(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(44.)).flex().items_center().justify_center().rounded(px(14.)).bg(p.accent_soft).child(icon("call:personAdd").filled().el().size(px(22.)).text_color(p.text_accent)))
                    .child(div().text_size(fs(12.5)).css_weight(700.).text_color(p.text).child("Henüz etkinlik yok"))
                    .child(div().text_size(fs(11.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).text_center().child("Biri seni etiketlediğinde, mesajına yanıt verdiğinde ya da tepki bıraktığında burada görürsün.")),
            );
        }
        // Workspace.tsx: okunmamış = yüklü öğelerden okunmamışlar.
        let unread = self.activity.iter().filter(|a| a.read_at.is_none()).count();
        let read_all = div()
            .id("act-read")
            .h(px(26.))
            .px(px(9.))
            .flex()
            .items_center()
            .border_1()
            .border_color(p.border)
            .rounded(px(8.))
            .text_size(fs(10.))
            .text_color(p.text_2)
            .cursor_pointer()
            .hover(move |s| s.bg(soft).text_color(strong))
            .child("Tümünü okundu say")
            .on_click(cx.listener(|this, _, window, cx| this.read_all_activity(window, cx)));
        let close = div()
            .id("act-close")
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
            .on_click(cx.listener(|this, _, _, cx| {
                this.activity_open = false;
                cx.notify();
            }));
        let panel = div()
            .id("activity-panel")
            .relative()
            .h_full()
            .w(px(380.))
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(vec![
                gpui_kit::BoxShadow { color: css_rgba(0, 0, 0, 0.4), offset: gpui_kit::point(px(-8.), px(0.)), blur_radius: px(24.), spread_radius: px(0.), inset: false },
                gpui_kit::BoxShadow { color: css_rgba(255, 255, 255, 0.05), offset: gpui_kit::point(px(1.), px(0.)), blur_radius: px(0.), spread_radius: px(0.), inset: true },
            ])
            .occlude()
            .child(
                div()
                    .h(px(52.))
                    .flex_none()
                    .pl(px(16.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(fs(13.5)).line_height(p2_ui::text::normal_line_height(fs(13.5))).css_weight(700.).text_color(p.text_strong).child("Etkinlik"))
                            .child(div().mt(px(1.)).text_size(fs(10.)).line_height(p2_ui::text::normal_line_height(fs(10.))).text_color(p.text_3).child(if unread > 0 { format!("{unread} yeni") } else { "Hepsi okundu".into() })),
                    )
                    .child(div().flex().items_center().gap(px(4.)).when(unread > 0, |s| s.child(read_all)).child(close)),
            )
            .child(list);
        // `profile-drawer-in var(--m-slow) var(--ease-elastic)`: 28px sağdan + opaklık.
        let panel = panel.with_animation("activity-in", gpui_kit::Animation::new(p2_ui::motion::M_SLOW), |el, t| {
            let k = p2_ui::motion::EASE_ELASTIC.ease(t);
            el.opacity(t.min(1.)).left(px(28. * (1. - k)))
        });
        Some(
            // Gölge (-8px 0 24px) başlık çubuğunun üstüne taşmasın: Electron'da `.titlebar` (z 1000) üstte kalır.
            // Kap solda 40px gölge payıyla kırpılır; pay alanı tıklamaları engellemez.
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .pl(px(40.))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            if this.activity_open {
                                this.activity_open = false;
                                cx.notify();
                            }
                        }))
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}
