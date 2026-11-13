//! Sayfa zemini: `radial-gradient(circle at 20% 0%, var(--p2-accent-soft) 0, transparent 45%), var(--p2-bg-0)`.
//! GPUI'de dairesel degrade yok; belgenin boyutunda bir görüntü üretilip zemine serilir.
//! `circle` + varsayılan `farthest-corner`: yarıçap merkezden (0.2·G, 0) en uzak köşeye uzaklık.

use std::sync::Arc;

use gpui_kit::RenderImage;

/// `w`×`h` mantıksal piksel; `scale` ekran ölçeği (yarım çözünürlük yeter: degrade yumuşak).
pub fn radial(w: f32, h: f32) -> Option<Arc<RenderImage>> {
    if w < 1. || h < 1. {
        return None;
    }
    // Yarım çözünürlük: GPUI görüntüyü ölçekler, degradede fark görünmez.
    let (iw, ih) = (((w / 2.).ceil() as u32).max(1), ((h / 2.).ceil() as u32).max(1));
    let (cx, cy) = (0.2 * w, 0.);
    let radius = ((w - cx).max(cx).powi(2) + h.powi(2)).sqrt();
    let stop = 0.45 * radius;
    let (br, bg, bb) = (7., 11., 20.); // #070b14
    let (ar, ag, ab, aa) = (44., 123., 255., 0.16); // rgba(44,123,255,.16)
    let mut img = image::RgbaImage::new(iw, ih);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let (fx, fy) = ((x as f32 + 0.5) * 2., (y as f32 + 0.5) * 2.);
        let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
        // Önceden çarpılmış ara değerleme: renk sabit, örtücülük doğrusal azalır.
        let a = (aa * (1. - d / stop)).max(0.);
        let mix = |b: f32, f: f32| (b * (1. - a) + f * a).round().clamp(0., 255.) as u8;
        // GPUI BGRA bekler.
        *px = image::Rgba([mix(bb, ab), mix(bg, ag), mix(br, ar), 255]);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(img)])))
}

/// Gömülü sunucu simgesi (`assets/icon-server.png`), 96 px'e küçültülmüş (48 px kutu, 2x ekran).
pub fn server_icon() -> Option<Arc<RenderImage>> {
    let bytes = include_bytes!("../../../assets/icon-server.png");
    let image = image::load_from_memory(bytes).ok()?.resize_exact(96, 96, image::imageops::FilterType::Lanczos3);
    let mut bgra = image.to_rgba8();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}
