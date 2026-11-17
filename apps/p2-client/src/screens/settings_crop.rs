//! Profil fotoğrafı kırpma penceresi — SettingsModal.tsx `AvatarCropper` (+ settings.css `.avatar-crop-*`):
//! 280px yuvarlak önizleme üzerinde doğrudan düzenleme (kaydırıcı yok): sürükleyerek konumlandır,
//! tekerlekle imlecin çevresinde yakınlaştır (1–4), çift tıkla sıfırla; küçük −/+ düğmeleri ortadan
//! yakınlaştırır. Görsel kırpma alanını her zaman tamamen kaplar (kaydırma ve yakınlaştırma sınırlı).
//! "Bu alanı kullan" ile 256px kare görsel üretilir (Electron webp veri adresi; burada geçici PNG →
//! "Profili kaydet" yükler). Durum Electron'daki gibi `zoom` + yüzde konum (`x`, `y`, 0–100):
//! çıktı hesabı (`draw`) değişmedi.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use p2_ui::P2Fx as _; // backdrop_blur

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Bounds, Context, CursorStyle, DispatchPhase, FontWeight, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, ScrollWheelEvent, SharedString, canvas, div, px};
use p2_ui::tokens::{Palette, Shadow, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::SettingsModal;
use super::settings_ui::{Btn, button, text};

/// Önizleme çapı (px). Konum yüzdeleri boyuttan bağımsız: 256px çıktı aynı kadrajı üretir.
const PREVIEW: f32 = 280.;
const ZOOM_MIN: f32 = 1.;
const ZOOM_MAX: f32 = 4.;

pub struct Crop {
    pub image: Option<image::RgbaImage>,
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
    preview: Option<(f32, f32, f32, Arc<RenderImage>)>,
    /// Önizleme alanının pencere içindeki sınırları (imlece göre yakınlaştırma için; ön çizimde ölçülür).
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Sürükleme sürüyorsa son imleç konumu.
    drag: Option<Point<Pixels>>,
}

impl Crop {
    pub fn open(path: &std::path::Path) -> Option<Self> {
        let img = image::open(path).ok()?.to_rgba8();
        Some(Self { image: Some(img), zoom: 1., x: 50., y: 50., preview: None, area: Rc::default(), drag: None })
    }

    /// `s` boyutlu çerçevede: ölçek, görsel genişlik/yükseklik ve sol-üst kayma (AvatarCropper `draw`).
    fn geometry(&self, s: f32) -> Option<(f32, f32, f32, f32, f32)> {
        let img = self.image.as_ref()?;
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let scale = (s / iw).max(s / ih) * self.zoom;
        let (w, h) = (iw * scale, ih * scale);
        Some((scale, w, h, (s - w) * (self.x / 100.), (s - h) * (self.y / 100.)))
    }

    /// Kaymadan yüzde konuma: boşluk (`s − w`, ≤ 0) içinde sınırlanır → görsel çerçeveyi hep kaplar.
    /// Boşluk yoksa (o eksende tam sığıyor) konum değişmez.
    fn percent(offset: f32, slack: f32, current: f32) -> f32 {
        if slack < -0.001 { (offset / slack).clamp(0., 1.) * 100. } else { current }
    }

    /// Sürükleme: önizleme pikseli kadar kaydır.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let Some((_, w, h, ox, oy)) = self.geometry(PREVIEW) else { return };
        self.x = Self::percent(ox + dx, PREVIEW - w, self.x);
        self.y = Self::percent(oy + dy, PREVIEW - h, self.y);
    }

    /// `at` (önizleme koordinatı) altındaki görsel noktası yerinde kalacak şekilde yakınlaştır.
    pub fn zoom_at(&mut self, zoom: f32, at: (f32, f32)) {
        let Some((scale, _, _, ox, oy)) = self.geometry(PREVIEW) else { return };
        let (u, v) = ((at.0 - ox) / scale, (at.1 - oy) / scale);
        self.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let Some((scale, w, h, _, _)) = self.geometry(PREVIEW) else { return };
        self.x = Self::percent(at.0 - u * scale, PREVIEW - w, 50.);
        self.y = Self::percent(at.1 - v * scale, PREVIEW - h, 50.);
    }

    pub fn reset(&mut self) {
        (self.zoom, self.x, self.y) = (1., 50., 50.);
    }

    /// AvatarCropper `draw`: kapla (cover) × yakınlaştırma, konum yüzdesine göre kaydır.
    fn draw(&self, size: u32) -> Option<image::RgbaImage> {
        let img = self.image.as_ref()?;
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let (scale, _, _, dx, dy) = self.geometry(size as f32)?;
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
        let mut bgra = self.draw(PREVIEW as u32)?;
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
        crate::trace(&format!("ayarlar: kırpma yakınlaştırma {:.4} konum {:.4} {:.4}", self.zoom, self.x, self.y));
        Some(path)
    }
}

impl SettingsModal {
    pub(super) fn crop_dialog(&mut self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let crop = self.crop.as_mut()?;
        let preview = crop.preview_image();
        let (zoom, dragging, area) = (crop.zoom, crop.drag.is_some(), crop.area.clone());
        let (hb, ht) = (p.accent_soft, p.text_strong);
        // Küçük −/+ (`crop-close` biçiminde 26px simge düğmesi): ortadan yakınlaştırır.
        let zoom_btn = |id: &'static str, glyph: &'static str, factor: f32, enabled: bool| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::XS)
                .text_size(fs(16.))
                .text_color(p.text_3)
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hb).text_color(ht)))
                .when(!enabled, |d| d.opacity(0.4))
                .child(glyph)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(c) = this.crop.as_mut() {
                        let z = c.zoom * factor;
                        c.zoom_at(z, (PREVIEW / 2., PREVIEW / 2.));
                        cx.notify();
                    }
                }))
        };
        let weak = cx.entity().downgrade();
        let stage = div()
            .id("crop-area")
            .relative()
            .size(px(PREVIEW))
            .rounded_full()
            .overflow_hidden()
            .bg(p.bg_0)
            .shadow({
                let mut s = shadows(&[Shadow::new(0., 0., 0., 5., p.accent_soft_2)]);
                s.extend(p.shadow_md());
                s
            })
            .cursor(if dragging { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
            .children(preview.map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size(px(PREVIEW)).rounded_full()))
            .child(
                // Ölçüm + sürükleme izleme: imleç alanın (hatta pencerenin) dışına çıksa da hareket/bırakma
                // olayları pencere düzeyinde dinlenir (üstteki katmanların `occlude`'u etkilemez).
                canvas(
                    move |b, _, _| area.set(Some(b)),
                    move |_, _, window, _| {
                        if !dragging {
                            return;
                        }
                        let w = weak.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let _ = w.update(cx, |this, cx| {
                                let Some(c) = this.crop.as_mut() else { return };
                                let Some(last) = c.drag else { return };
                                if e.pressed_button != Some(MouseButton::Left) {
                                    c.drag = None;
                                } else {
                                    c.pan(f32::from(e.position.x - last.x), f32::from(e.position.y - last.y));
                                    c.drag = Some(e.position);
                                }
                                cx.notify();
                            });
                        });
                        let w = weak.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture || e.button != MouseButton::Left {
                                return;
                            }
                            let _ = w.update(cx, |this, cx| {
                                if let Some(c) = this.crop.as_mut() {
                                    c.drag = None;
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &gpui_kit::MouseDownEvent, _, cx| {
                    if let Some(c) = this.crop.as_mut() {
                        if e.click_count >= 2 {
                            c.reset();
                            c.drag = None;
                            crate::trace("ayarlar: kırpma sıfırlandı");
                        } else {
                            c.drag = Some(e.position);
                        }
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                let Some(c) = this.crop.as_mut() else { return };
                let dy = f32::from(e.delta.pixel_delta(px(20.)).y);
                let at = c.area.get().map_or((PREVIEW / 2., PREVIEW / 2.), |b| (f32::from(e.position.x - b.origin.x), f32::from(e.position.y - b.origin.y)));
                // Yukarı (dy > 0) yakınlaştırır; bir tekerlek adımı (3 satır = 60px) ≈ ×1.27.
                let z = c.zoom * (dy * 0.004).exp();
                c.zoom_at(z, at);
                cx.stop_propagation();
                cx.notify();
            }));
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
                    .child(div().flex().flex_col().gap(px(2.)).child(text(fs(13.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(p2_i18n::t("set2.crop.title"))).child(text(fs(9.)).text_color(p.text_3).child(p2_i18n::t("set2.crop.subtitle"))))
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
            .child(div().flex().justify_center().child(stage))
            .child(
                // İpucu (alt başlıkla aynı açıklama biçimi) + küçük yakınlaştırma düğmeleri.
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(text(fs(9.)).text_color(p.text_3).child(p2_i18n::t("set2.crop.hint")))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .child(zoom_btn("crop-zoom-out", "−", 1. / 1.25, zoom > ZOOM_MIN + 0.001))
                            .child(text(fs(9.)).w(px(34.)).flex().justify_center().text_color(p.text_3).child(format!("%{}", (zoom * 100.).round() as i32)))
                            .child(zoom_btn("crop-zoom-in", "+", 1.25, zoom < ZOOM_MAX - 0.001)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(button("crop-cancel", p2_i18n::t("act.cancel"), Btn::Default, false, p).on_click(cx.listener(|this, _, _, cx| {
                        this.crop = None;
                        cx.notify();
                    })))
                    .child(button("crop-use", p2_i18n::t("set2.crop.use"), Btn::Primary, false, p).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(path) = this.crop.as_ref().and_then(Crop::export) {
                            this.avatar_draft = Some(path.to_string_lossy().to_string());
                            crate::toasts::info(p2_i18n::t("set2.crop.ready"));
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
                        // Sürükleme dışarıda bırakılırsa pencereyi kapatma.
                        if this.crop.as_ref().is_some_and(|c| c.drag.is_some()) {
                            return;
                        }
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
