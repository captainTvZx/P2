//! UpdateBanner.tsx: indirme sürerken üstte süzülen şerit (`.update-banner`), sürüm hazır olunca ortada
//! küçük onay penceresi (`Modal size="sm" .update-modal`, "Sonra" / "Şimdi yeniden başlat").
//! Durum `updates::state()`; `ClientApp` render'ında `updates::watch` ile tazelenir.
//! Parite kancası (yalnız görünüm): `P2_AUTOPILOT_UPDATE=downloading:42[:0.10.0]` ya da `ready[:0.10.0]`.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, Window, div, px, relative};
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, kicker, modal, modal_body, modal_foot, modal_layer, progress};
use p2_ui::tokens::{Palette, font_size};
use p2_ui::{Entrance, P2Motion as _};
use p2_updater::Status;

use crate::app::ClientApp;

/// Arayüzün gösterdiği güncelleme durumu.
#[derive(Clone, PartialEq)]
pub enum Shown {
    Downloading { version: String, percent: u32 },
    Ready { version: String },
}

fn autopilot() -> Option<Shown> {
    let raw = std::env::var("P2_AUTOPILOT_UPDATE").ok()?;
    let mut parts = raw.split(':');
    match parts.next()? {
        "downloading" => Some(Shown::Downloading { percent: parts.next().and_then(|p| p.parse().ok()).unwrap_or(42), version: parts.next().unwrap_or("").into() }),
        "ready" => Some(Shown::Ready { version: parts.next().unwrap_or("").into() }),
        _ => None,
    }
}

pub fn current() -> Option<Shown> {
    if let Some(s) = autopilot() {
        return Some(s);
    }
    let s = crate::updates::state();
    if s.startup {
        return None;
    }
    match s.status {
        Status::Downloading => Some(Shown::Downloading { version: s.version, percent: s.percent }),
        Status::Ready => Some(Shown::Ready { version: s.version }),
        _ => None,
    }
}

impl ClientApp {
    pub(crate) fn update_overlay(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        crate::updates::watch(cx);
        let shown = current()?;
        // UpdateBanner.tsx: "Sonra" gizler; yeni bir durum (yüzde/sürüm) gelince yeniden görünür.
        if self.update_hidden.as_ref() == Some(&shown) {
            return None;
        }
        match shown {
            Shown::Downloading { version, percent } => {
                let vw = f32::from(window.viewport_size().width);
                let w = 640f32.min(vw - 32.);
                let title = if version.is_empty() { "Yeni sürüm indiriliyor".to_string() } else { format!("Yeni sürüm indiriliyor · {version}") };
                let banner = div()
                    .w(px(w))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .pt(px(9.))
                    .pr(px(12.))
                    .pb(px(9.))
                    .pl(px(15.))
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(p2_ui::tokens::radius::LG)
                    .bg(p.glass.background())
                    .shadow(p.shadow_lg())
                    .text_color(p.text)
                    .text_size(font_size::SM)
                    .child(div().flex_shrink_0().text_color(p.text_strong).text_size(font_size::MD).font_weight(FontWeight::BOLD).child(title))
                    .child(div().flex_1().min_w(px(120.)).child(progress(Some(percent.max(4) as f32 / 100.), false, p)))
                    .child(div().flex_shrink_0().text_color(p.text_3).text_size(font_size::SM).child(format!("%{percent}")))
                    // `update-in`: 10px yukarıdan, 220ms ease-out
                    .p2_enter("update-banner", Entrance::Rise, Duration::ZERO);
                Some(div().absolute().top(px(44.)).left_0().right_0().flex().justify_center().child(banner).into_any_element())
            }
            Shown::Ready { version } => {
                let installing = self.update_installing;
                let title = if version.is_empty() { "Yeni sürüm hazır".to_string() } else { format!("Yeni sürüm hazır · {version}") };
                let close = button("update-close", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×").on_click(cx.listener(|this, _, _, cx| this.hide_update(cx)));
                let head = div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .pt(px(18.))
                    .pr(px(16.))
                    .pb(px(12.))
                    .pl(px(20.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(kicker("GÜNCELLEME", p))
                            .child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).line_height(relative(1.25)).child(title)),
                    )
                    .child(close);
                let body = modal_body()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().mt(px(2.)).mb(px(8.)).child(p2_ui::components::logo_image(64., p)))
                    .child(div().w_full().text_color(p.text_2).child("Kurulum sessizce yapılır ve uygulama kendiliğinden yeniden açılır. “Sonra” dersen güncelleme, uygulamayı kapattığında kurulur."));
                let foot = modal_foot(p)
                    .child(div().flex_1().flex().child(button("update-later", BtnVariant::Ghost, BtnSize::Md, false, false, p).flex_1().child("Sonra").on_click(cx.listener(|this, _, _, cx| this.hide_update(cx)))))
                    .child(
                        div().flex_1().flex().child(
                            button("update-now", BtnVariant::Primary, BtnSize::Md, false, false, p)
                                .flex_1()
                                .when(installing, |s| s.opacity(0.55))
                                .child(if installing { "Kuruluyor…" } else { "Şimdi yeniden başlat" })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if !this.update_installing {
                                        this.update_installing = true;
                                        cx.notify();
                                        crate::updates::install();
                                    }
                                })),
                        ),
                    );
                let card = modal(ModalSize::Sm, window, p).child(head).child(body).child(foot).p2_enter("update-modal", Entrance::PopElastic, Duration::ZERO);
                // Kökün son çocuğu olduğu için zaten en üstte çizilir (deferred mutlak boyutu bozar).
                // p2_enter öğeyi `relative` yapar: mutlak katman bir sarmalayıcıda tam boy çizilir.
                Some(div().absolute().inset_0().child(modal_layer("update-layer", false, p).size_full().child(card).p2_enter("update-layer", Entrance::Fade, Duration::ZERO)).into_any_element())
            }
        }
    }

    fn hide_update(&mut self, cx: &mut Context<Self>) {
        self.update_hidden = current();
        cx.notify();
    }
}
