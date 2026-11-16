//! App.tsx `ServerProblem` (istemci modu): kayıtlı uzak sunucuya ulaşılamıyor.
//! Ölçüler setup.css `.server-problem-*` + dom-styles.mjs (server-problem ekranı).

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{ClipboardItem, Context, FontWeight, WeakEntity, Window, div, px, relative};
use p2_i18n::t;
use p2_ui::P2Text as _;
use p2_ui::text::normal_line_height;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{font_size, fs, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::setup_ui::{eyebrow, install_button, mode_back, setup_page, setup_shell, shell_enter};
use crate::app::ClientApp;

pub struct ProblemScreen {
    app: WeakEntity<ClientApp>,
    address: String,
    copied: bool,
}

impl ProblemScreen {
    pub fn new(app: WeakEntity<ClientApp>, address: String, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { app, address, copied: false }
    }
}

impl Render for ProblemScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let h2 = fs(27.);
        // `.server-problem-icon`: 46×46, kenarlık danger-border, r 14, danger-soft, "!" 20px×0,85 900; logo-pop .5 sn
        let icon = div()
            .size(px(46.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::LG)
            .bg(p.danger_soft)
            .border_1()
            .border_color(p.danger_border)
            .text_color(p.danger_text)
            .text_size(fs(20.))
            .css_weight(900.)
            .child("!")
            .p2_enter("problem-icon", Entrance::Fade, Duration::from_millis(100));
        let header = div()
            .flex()
            .items_start()
            .gap(px(18.))
            .pb(px(22.))
            .border_b_1()
            .border_color(p.border_subtle)
            .child(icon)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(eyebrow(t("problem.kicker"), &p).h(px(10.)).line_height(px(10.)))
                    .child(div().mt(px(3.)).h(p2_ui::text::scaled(normal_line_height(h2))).line_height(normal_line_height(h2)).text_size(h2).font_weight(FontWeight::BOLD).tracking(-0.02).text_color(p.text_strong).child(t("problem.title")))
                    .child(div().mt(px(8.)).max_w(px(500.)).text_size(fs(12.)).line_height(relative(1.55)).text_color(p.text_3).child(t("problem.body"))),
            );
        let (copy_hover_border, copy_hover_bg, copy_hover_fg) = (p.border_accent, p.accent_soft, p.text_strong);
        let endpoint = div()
            .mt(px(22.))
            .flex()
            .items_center()
            .gap(px(12.))
            .pt(px(12.))
            .pr(px(12.))
            .pb(px(12.))
            .pl(px(14.))
            .rounded(radius::LG)
            .border_1()
            .border_color(p.border)
            .bg(p.bg_3)
            .child(div().w(px(38.)).flex_shrink_0().child(div().size(px(34.)).flex().items_center().justify_center().rounded(radius::MD).bg(p.accent_soft).text_color(p.text_accent).text_size(fs(20.)).child("⌁")))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().h(p2_ui::text::scaled(px(10.))).line_height(px(10.)).text_size(font_size::XXS).css_weight(850.).tracking(0.13).text_color(p.text_accent).child(t("problem.address")))
                    // `font: 11.5px/1.35 mono` kısaltması ölçeklenmez
                    .child(div().font_family(p2_ui::tokens::FONT_MONO).text_size(px(11.5)).line_height(relative(1.35)).text_color(p.text).truncate().child(self.address.clone())),
            )
            .child(
                div()
                    .id("copy")
                    .h(px(32.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::SM)
                    .text_size(fs(10.5))
                    .text_color(p.text_2)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(copy_hover_border).bg(copy_hover_bg).text_color(copy_hover_fg))
                    .child(t(if self.copied { "common.copied" } else { "act.copy" }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(this.address.clone()));
                        this.copied = true;
                        cx.notify();
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(Duration::from_millis(1600)).await;
                            let _ = this.update(cx, |this, cx| {
                                this.copied = false;
                                cx.notify();
                            });
                        })
                        .detach();
                    })),
            );
        let actions = div()
            .mt(px(20.))
            .pt(px(18.))
            .border_t_1()
            .border_color(p.border_subtle)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(install_button("retry", t("problem.retry"), false, &p).on_click(cx.listener(|this, _, window, cx| {
                let app = this.app.clone();
                let _ = app.update(cx, |app, cx| app.restore(window, cx));
            })))
            .child(mode_back("change", t("problem.change"), &p).on_click(cx.listener(|this, _, window, cx| {
                let app = this.app.clone();
                let _ = app.update(cx, |app, cx| app.change_address(window, cx));
            })));
        let shell = setup_shell("problem-shine", 650., 32., 32., &p).child(header).child(endpoint).child(actions);
        setup_page(window, &p, (0.5, 0.28), p.danger_soft, 0.42, shell_enter(shell, ("problem", 0)))
    }
}
