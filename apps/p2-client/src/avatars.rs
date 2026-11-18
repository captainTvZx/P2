//! Profil fotoğrafları: kullanıcı kimliği → `/uploads/...` yolu (üye listelerinden öğrenilir),
//! yol → çözülmüş görsel. GPUI'de uzak resim yükleyici olmadığından kimlikli indirilip
//! 128 px kare kırpılır; inince pencereler yeniden çizilir (`DIRTY`). `p2_ui::avatar` bu
//! önbelleğe kaynak olarak bağlanır, böylece bütün avatarlar kendiliğinden fotoğrafa döner.
//!
//! Hareketli GIF: kareler bir kez çözülür ([`Frames`]; her kare ayrı görsel + süre), çizen her yer
//! ortak saate göre o anki kareyi alır ve sıradaki karenin zamanı [`animation_due`] ile pencereleri
//! yeniler. Ekranda hareketli görsel yoksa (ya da pencere çizilmiyorsa) hiçbir zamanlayıcı işi kalmaz.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::RenderImage;
use p2_client_core::Api;

enum Slot {
    Loading,
    Ready(Frames),
    Failed,
}

/// Çözülmüş görsel: tek kare ya da hareketli GIF kareleri (görsel, süre ms).
pub(crate) struct Frames {
    frames: Vec<(Arc<RenderImage>, u32)>,
    total_ms: u64,
}

/// En kısa kare süresi (en çok ~20 kare/sn): daha hızlı GIF'lerde kareler birleştirilir, toplam süre korunur.
const MIN_FRAME_MS: u32 = 50;

fn clock_ms() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

/// Sıradaki karenin zamanı (`clock_ms`; 0 = ekranda hareketli görsel yok).
static NEXT_FRAME_AT: AtomicU64 = AtomicU64::new(0);

/// Hareketli bir görselin sıradaki karesinin zamanı geldiyse `true` (pencereler yenilenmeli). Yeni
/// zaman, yenilenen çizimde görseli isteyen yer tarafından kurulur; kimse istemezse animasyon durur.
pub fn animation_due() -> bool {
    let at = NEXT_FRAME_AT.load(Ordering::Relaxed);
    at != 0 && clock_ms() >= at && NEXT_FRAME_AT.swap(0, Ordering::Relaxed) != 0
}

/// Sıradaki kareye kalan süre (ms; `None` = kurulu kare zamanı yok).
pub fn next_frame_in() -> Option<u64> {
    let at = NEXT_FRAME_AT.load(Ordering::Relaxed);
    (at != 0).then(|| at.saturating_sub(clock_ms()))
}

fn to_render(mut rgba: image::RgbaImage) -> Arc<RenderImage> {
    for px in rgba.pixels_mut() {
        px.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
}

impl Frames {
    /// Görseli çöz: GIF ise bütün kareler (`fit` her kareye uygulanır, ikinci değişken "hareketli mi";
    /// en çok `max_frames` kare ve `max_bytes` bellek — aşılırsa kareler seyreltilir, süreler
    /// toplanır), değilse tek kare.
    pub(crate) fn decode(bytes: &[u8], max_frames: usize, max_bytes: usize, fit: impl Fn(image::DynamicImage, bool) -> image::RgbaImage) -> Option<Self> {
        if bytes.starts_with(b"GIF8") {
            if let Some(frames) = Self::decode_gif(bytes, max_frames, max_bytes, &fit) {
                return Some(frames);
            }
        }
        let img = image::load_from_memory(bytes).ok()?;
        Some(Self { frames: vec![(to_render(fit(img, false)), 0)], total_ms: 0 })
    }

    fn decode_gif(bytes: &[u8], max_frames: usize, max_bytes: usize, fit: &impl Fn(image::DynamicImage, bool) -> image::RgbaImage) -> Option<Self> {
        use image::AnimationDecoder as _;
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
        let mut kept: Vec<(image::RgbaImage, u32)> = Vec::new();
        let (mut min_ms, mut cap) = (MIN_FRAME_MS, max_frames.max(2));
        // Kareler tek tek çözülüp hemen küçültülür (tam boy kareler bellekte birikmez).
        for frame in decoder.into_frames().take(4000) {
            let Ok(frame) = frame else { break };
            let (num, den) = frame.delay().numer_denom_ms();
            // Süresi 0 / çok kısa kareler tarayıcılardaki gibi 100 ms sayılır.
            let ms = if den == 0 || num / den < 20 { 100 } else { num / den };
            if let Some(last) = kept.last_mut() {
                if last.1 < min_ms {
                    // Önceki kare en kısa süreye ulaşmadı: bu kare atlanır, süresi ona eklenir.
                    last.1 += ms;
                    continue;
                }
            }
            let small = fit(image::DynamicImage::ImageRgba8(frame.into_buffer()), true);
            if kept.is_empty() {
                // Bellek sınırı ilk kareden belli olur.
                let per = (small.width() as usize * small.height() as usize * 4).max(1);
                cap = cap.min((max_bytes / per).max(2));
            }
            kept.push((small, ms));
            if kept.len() > cap {
                // Sınır aşıldı: her ikinci kare atılır (süresi öncekine), sonrası yarı sıklıkta örneklenir.
                let mut thin: Vec<(image::RgbaImage, u32)> = Vec::with_capacity(kept.len() / 2 + 1);
                for (i, (img, ms)) in kept.drain(..).enumerate() {
                    match (i % 2, thin.last_mut()) {
                        (1, Some(last)) => last.1 += ms,
                        _ => thin.push((img, ms)),
                    }
                }
                kept = thin;
                min_ms = min_ms.saturating_mul(2);
            }
        }
        if kept.is_empty() {
            return None;
        }
        let single = kept.len() == 1;
        let frames: Vec<(Arc<RenderImage>, u32)> = kept.into_iter().map(|(img, ms)| (to_render(img), if single { 0 } else { ms })).collect();
        let total_ms = frames.iter().map(|f| f.1 as u64).sum();
        Some(Self { frames, total_ms })
    }

    pub(crate) fn animated(&self) -> bool {
        self.frames.len() > 1 && self.total_ms > 0
    }

    /// O anki kare (ortak saat: aynı GIF her yerde eşzamanlı oynar). Hareketliyse sıradaki karenin
    /// zamanını kurar.
    pub(crate) fn current(&self) -> Arc<RenderImage> {
        if !self.animated() {
            return self.frames[0].0.clone();
        }
        let now = clock_ms();
        let mut t = now % self.total_ms;
        for (img, ms) in &self.frames {
            if t < *ms as u64 {
                let due = now + (*ms as u64 - t);
                // En yakın zaman kalır.
                let _ = NEXT_FRAME_AT.try_update(Ordering::Relaxed, Ordering::Relaxed, |at| (at == 0 || due < at).then_some(due));
                return img.clone();
            }
            t -= *ms as u64;
        }
        self.frames[0].0.clone()
    }
}

struct Cache {
    urls: HashMap<String, String>,
    images: HashMap<String, Slot>,
    api: Option<Arc<Api>>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { urls: HashMap::new(), images: HashMap::new(), api: None }))
}

/// Yeni görsel indi: pencereler yeniden çizilmeli.
pub static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn install(api: Arc<Api>) {
    cache().lock().unwrap().api = Some(api);
    p2_ui::components::set_avatar_source(source);
}

/// Oturumun API'si (afiş önbelleği kendi API'si kurulmadan önce bunu kullanır).
pub(crate) fn api() -> Option<Arc<Api>> {
    cache().lock().unwrap().api.clone()
}

/// Kullanıcının fotoğraf yolunu kaydet (`None` ya da boş: fotoğraf yok).
pub fn remember(user: &str, url: Option<&str>) {
    let mut c = cache().lock().unwrap();
    match url.filter(|u| !u.trim().is_empty()) {
        Some(u) => {
            let path = u.find("/uploads/").map(|i| u[i..].to_string()).unwrap_or_else(|| u.to_string());
            if c.urls.get(user) != Some(&path) {
                c.urls.insert(user.to_string(), path);
                DIRTY.store(true, Ordering::Relaxed);
            }
        }
        None => {
            if c.urls.remove(user).is_some() {
                DIRTY.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Avatar / ikon: ortadan kare kırpılıp 128 px'e indirilir (GIF'te her kare; en çok 120 kare, 16 MB).
fn decode(bytes: &[u8]) -> Option<Frames> {
    Frames::decode(bytes, 120, 16 * 1024 * 1024, |img, _| {
        let side = img.width().min(img.height());
        img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).resize_exact(128, 128, image::imageops::FilterType::Triangle).to_rgba8()
    })
}

fn source(user: &str) -> Option<Arc<RenderImage>> {
    let mut c = cache().lock().unwrap();
    let url = c.urls.get(user)?.clone();
    match c.images.get(&url) {
        Some(Slot::Ready(frames)) => return Some(frames.current()),
        Some(_) => return None,
        None => {}
    }
    let api = c.api.clone()?;
    c.images.insert(url.clone(), Slot::Loading);
    drop(c);
    std::thread::spawn(move || {
        let slot = api.get_bytes(&url).ok().and_then(|b| decode(&b)).map(Slot::Ready).unwrap_or(Slot::Failed);
        let ready = matches!(slot, Slot::Ready(_));
        cache().lock().unwrap().images.insert(url.clone(), slot);
        crate::trace(&format!("avatar: {url} {}", if ready { "hazır" } else { "yüklenemedi" }));
        DIRTY.store(true, Ordering::Relaxed);
    });
    None
}

/// Hazır fotoğraf (yoksa indirmeyi başlatır) — kare/yuvarlatılmış avatar çizen ekranlar için (B: kenar çubuğu).
pub fn image(user: &str) -> Option<Arc<RenderImage>> {
    source(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frames` karelik 8×4 GIF (kare süresi `delay_ms`).
    fn sample_gif(frames: u16, delay_ms: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
            for i in 0..frames {
                let buf = image::RgbaImage::from_pixel(8, 4, image::Rgba([(i * 20) as u8, 10, 200, 255]));
                enc.encode_frame(image::Frame::from_parts(buf, 0, 0, image::Delay::from_numer_denom_ms(delay_ms, 1))).unwrap();
            }
        }
        out
    }

    #[test]
    fn gif_keeps_its_frames() {
        let frames = decode(&sample_gif(4, 100)).unwrap();
        assert!(frames.animated());
        assert_eq!((frames.frames.len(), frames.total_ms), (4, 400));
        let size = frames.current().size(0);
        assert_eq!((size.width.0, size.height.0), (128, 128));
    }

    #[test]
    fn fast_or_long_gifs_are_thinned_but_keep_duration() {
        // 20 ms kareler: 50 ms altı birleştirilir, toplam süre aynı kalır.
        let fast = decode(&sample_gif(10, 20)).unwrap();
        assert!(fast.animated() && fast.frames.len() < 10 && fast.total_ms == 200, "{} kare {} ms", fast.frames.len(), fast.total_ms);
        // Kare sınırı: 9 kare, sınır 4 → seyreltilir, süre korunur.
        let long = Frames::decode(&sample_gif(9, 100), 4, usize::MAX, |img, _| img.to_rgba8()).unwrap();
        assert!(long.animated() && long.frames.len() <= 4 && long.total_ms == 900, "{} kare {} ms", long.frames.len(), long.total_ms);
    }

    #[test]
    fn static_images_stay_single_frame() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(6, 3, image::Rgba([1, 2, 3, 255]))).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let frames = decode(&png).unwrap();
        assert!(!frames.animated() && frames.frames.len() == 1);
        // Tek karelik GIF de durağan sayılır.
        assert!(!decode(&sample_gif(1, 100)).unwrap().animated());
    }

    #[test]
    fn remembers_paths() {
        remember("u1", Some("http://10.0.0.1:4000/uploads/a.png"));
        assert_eq!(cache().lock().unwrap().urls.get("u1").map(String::as_str), Some("/uploads/a.png"));
        remember("u1", None);
        assert!(!cache().lock().unwrap().urls.contains_key("u1"));
    }
}
