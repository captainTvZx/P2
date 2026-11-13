//! Profil fotoğrafı kırpma penceresi — SettingsModal.tsx `AvatarCropper` (+ settings.css `.avatar-crop-*`):
//! 280px yuvarlak önizleme, yakınlaştırma (1–4) ve yatay/dikey konum (0–100), "Bu alanı kullan" ile
//! 256px kare görsel üretilir (Electron webp veri adresi; burada geçici PNG → "Profili kaydet" yükler).

use std::sync::Arc;
use p2_ui::P2Fx as _; // backdrop_blur

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, RenderImage, SharedString, div, px};
use p2_ui::tokens::{Palette, Shadow, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::settings_ui::{Btn, button, range, text};
use super::{SettingsModal, sliders};

pub struct Crop {
    pub image: Option<image::RgbaImage>,
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
    preview: Option<(f32, f32, f32, Arc<RenderImage>)>,
}

impl Crop {
    pub fn open(path: &std::path::Path) -> Option<Self> {
        let img = image::open(path).ok()?.to_rgba8();
        Some(Self { image: Some(img), zoom: 1., x: 50., y: 50., preview: None })
    }

    /// AvatarCropper `draw`: kapla (cover) × yakınlaştırma, konum yüzdesine göre kaydır.
    fn draw(&self, size: u32) -> Option<image::RgbaImage> {
        let img = self.image.as_ref()?;
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let s = size as f32;
        let scale = (s / iw).max(s / ih) * self.zoom;
        let (w, h) = (iw * scale, ih * scale);
        let (dx, dy) = ((s - w) * (self.x / 100.), (s - h) * (self.y / 100.));
        let mut out = image::RgbaImage::new(size, size);
        for oy in 0..size {
            for ox in 0..size {
                let sx = ((ox as f32 + 0.5 - dx) / scale).floor();
                let sy = ((oy as f32 + 0.5 - dy) / scale).floor();
                if sx >= 0. && sy >= 0. && sx < iw && sy < ih {
                    out.put_pixel(ox, oy, *img.get_pixel(sx as u32, sy as u32));
                }
            }
        }
        Some(out)
    }

    fn preview_image(&mut self) -> Option<Arc<RenderImage>> {
        if let Some((z, x, y, img)) = &self.preview {
            if *z == self.zoom && *x == self.x && *y == self.y {
                return Some(img.clone());
            }
        }
        let mut bgra = self.draw(280)?;
        for px in bgra.pixels_mut() {
            px.0.swap(0, 2);
        }
        let img = Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]));
        self.preview = Some((self.zoom, self.x, self.y, img.clone()));
        Some(img)
    }

    /// "Bu alanı kullan": 256px PNG geçici dosyaya.
    pub fn export(&self) -> Option<std::path::PathBuf> {
        let out = self.draw(256)?;
        let path = std::env::temp_dir().join(format!("p2-avatar-{}.png", std::process::id()));
        out.save(&path).ok()?;
        Some(path)
    }
}

impl SettingsModal {
    pub(super) fn crop_dialog(&mut self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let crop = self.crop.as_mut()?;
        let preview = crop.preview_image();
        let (zoom, x, y) = (crop.zoom, crop.x, crop.y);
        let label = |t: &'static str| div().w(px(85.)).flex_shrink_0().text_size(fs(10.)).text_color(p.text_2).child(t);
        let slider_row = |t: &'static str, el: AnyElement| div().flex().items_center().gap(px(9.)).child(label(t)).child(div().flex_1().child(el));
        let zoom_el = range("crop-zoom", (zoom * 100.).round() as i32, 100, 400, 5, true, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| {
            if let Some(c) = this.crop.as_mut() {
                c.zoom = v as f32 / 100.;
            }
            cx.notify();
        }), cx);
        let x_el = range("crop-x", x.round() as i32, 0, 100, 1, true, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| {
            if let Some(c) = this.crop.as_mut() {
                c.x = v as f32;
            }
            cx.notify();
        }), cx);
        let y_el = range("crop-y", y.round() as i32, 0, 100, 1, true, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| {
            if let Some(c) = this.crop.as_mut() {
                c.y = v as f32;
            }
            cx.notify();
        }), cx);
        let (hb, ht) = (p.accent_soft, p.text_strong);
        let dialog = div()
            .id("avatar-crop")
            .occlude()
            .w(px(420.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(15.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::XL)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().flex().flex_col().gap(px(2.)).child(text(fs(13.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("Profil fotoğrafını ayarla")).child(text(fs(9.)).text_color(p.text_3).child("Kullanılacak alanı seç")))
                    .child(
                        div()
                            .id("crop-close")
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XS)
                            .text_size(fs(19.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hb).text_color(ht))
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.crop = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().flex().justify_center().child(
                    div()
                        .size(px(280.))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(p.bg_0)
                        .shadow({
                            let mut s = shadows(&[Shadow::new(0., 0., 0., 5., p.accent_soft_2)]);
                            s.extend(p.shadow_md());
                            s
                        })
                        .children(preview.map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size(px(280.)).rounded_full())),
                ),
            )
            .child(slider_row("Yakınlaştır", zoom_el))
            .child(slider_row("Yatay konum", x_el))
            .child(slider_row("Dikey konum", y_el))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(button("crop-cancel", "Vazgeç", Btn::Default, false, p).on_click(cx.listener(|this, _, _, cx| {
                        this.crop = None;
                        cx.notify();
                    })))
                    .child(button("crop-use", "Bu alanı kullan", Btn::Primary, false, p).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(path) = this.crop.as_ref().and_then(Crop::export) {
                            this.avatar_draft = Some(path.to_string_lossy().to_string());
                            this.profile_notice = "Fotoğraf hazır. Kaydetmeyi unutma.".into();
                            crate::trace("ayarlar: fotoğraf kırpıldı");
                        }
                        this.crop = None;
                        cx.notify();
                    }))),
            )
            .p2_enter("avatar-crop", Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            gpui_kit::deferred(div().absolute().inset_0().child(
                div()
                    .id("avatar-crop-layer")
                    .size_full()
                    .p(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                    .occlude()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.crop = None;
                        cx.notify();
                    }))
                    .child(dialog)
                    .p2_enter("avatar-crop-layer", Entrance::Fade, std::time::Duration::ZERO),
            ))
            .with_priority(3)
            .into_any_element(),
        )
    }
}

#[allow(dead_code)]
fn _id(_: SharedString) {}
