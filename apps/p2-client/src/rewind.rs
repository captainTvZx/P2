//! Yayın geçmişi (geri sarma) — izleyici tarafında kayıt ve oynatma kaynağı.
//!
//! Yayıncı yayını başlatırken "Geçmiş (geri sarma)" süresini seçer (Kapalı / 30 sn … 5 dk); süre
//! `call:media {rewind}` ile (eski sunucuda `call:signal {rewind}` ile) izleyicilere duyurulur.
//! İzleyicinin istemcisi gelen yayın karelerini burada, kendi iş parçacığında saklar; arayüz geri
//! sarınca istenen ana en yakın kare arka planda çözülür.
//!
//! Bellek tasarımı (ödünleşim):
//! - Ham kare saklanmaz (1080p BGRA = 8 MB/kare; 5 dk × 15 fps ≈ 36 GB olurdu). Her kare en çok
//!   1280 px genişliğe küçültülüp JPEG (kalite 70) olarak saklanır: ekran içeriğinde ~60–250 KB.
//! - Kayıt hızı en çok 10 kare/sn; seçilen süre bütçeye sığmıyorsa hız kendiliğinden düşer
//!   (en az 1 kare/sn): `hız = bütçe / (süre × ortalama kare boyu)`. Geri sarılmış görüntü bu yüzden
//!   canlı yayından daha düşük kare hızında ve 1280 px'te oynar — metin okunur, hareket kesik olabilir.
//! - Sert tavan: bütün kayıtlar için toplam `BUDGET_BYTES` (400 MB), eşzamanlı yayınlar arasında eşit
//!   bölünür. Tavan aşılırsa en eski kareler atılır; yani gerçek geçmiş seçilen süreden kısa olabilir
//!   (arayüz her zaman gerçekten var olan aralığı gösterir).
//! - JPEG kodlama (~5–15 ms/kare) ve çözme arayüz iş parçacığında yapılmaz; arayüz yalnız kayda
//!   alınacak karenin kopyasını gönderir (kayıt hızında, canlı kare hızında değil).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use image::ImageEncoder as _;

/// Bütün yayın kayıtlarının toplam bellek tavanı.
pub const BUDGET_BYTES: usize = 400 * 1024 * 1024;
/// Saklanan karenin en büyük genişliği.
const MAX_W: u32 = 1280;
/// Kayıt hızı sınırları (kare/sn).
const MAX_FPS: f64 = 10.0;
const MIN_FPS: f64 = 1.0;
const JPEG_QUALITY: u8 = 70;

struct Stored {
    at: Instant,
    jpeg: Vec<u8>,
}

/// Çözülmüş geçmiş karesi (BGRA, GPUI `RenderImage` biçimi).
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Default)]
struct Store {
    frames: VecDeque<Stored>,
    bytes: usize,
    decoded: Option<Decoded>,
    /// Son çözülen karenin zamanı (aynı kare yeniden çözülmesin).
    decoded_at: Option<Instant>,
    /// Bekleyen arama: arayüzün en son istediği an (eskileri atlanır).
    seek: Option<Instant>,
}

enum Msg {
    Frame { at: Instant, width: u32, height: u32, bgra: Vec<u8> },
    Wake,
}

/// Bir yayının kaydı. Düşürülünce iş parçacığı biter, bellek bırakılır.
pub struct Recorder {
    // Sınırlı kuyruk: kodlayıcı yetişemezse kare atlanır (bellek birikmez).
    tx: mpsc::SyncSender<Msg>,
    store: Arc<Mutex<Store>>,
    interval_us: Arc<AtomicU64>,
    budget: Arc<AtomicUsize>,
    next_due: Option<Instant>,
}

impl Recorder {
    pub fn new(window_secs: u32, budget: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel(2);
        let store: Arc<Mutex<Store>> = Arc::default();
        let window = Duration::from_secs(window_secs.max(1) as u64);
        let interval_us = Arc::new(AtomicU64::new((1e6 / MAX_FPS) as u64));
        let budget = Arc::new(AtomicUsize::new(budget));
        let (s, i, b) = (store.clone(), interval_us.clone(), budget.clone());
        let _ = std::thread::Builder::new().name("p2-geri-sarma".into()).spawn(move || worker(rx, s, window, i, b));
        Self { tx, store, interval_us, budget, next_due: None }
    }

    pub fn set_budget(&self, bytes: usize) {
        self.budget.store(bytes, Ordering::Relaxed);
    }

    /// Sıradaki kareyi kayda alma zamanı geldi mi (kare kopyası yalnız o zaman yapılır).
    pub fn due(&self, now: Instant) -> bool {
        self.next_due.is_none_or(|d| now >= d)
    }

    pub fn push(&mut self, at: Instant, width: u32, height: u32, bgra: Vec<u8>) {
        self.next_due = Some(at + Duration::from_micros(self.interval_us.load(Ordering::Relaxed)));
        let _ = self.tx.try_send(Msg::Frame { at, width, height, bgra });
    }

    /// Testler: kuyruk boşalana dek bekleyerek ekle (kare atlanmasın).
    #[cfg(test)]
    fn push_wait(&mut self, at: Instant, width: u32, height: u32, bgra: Vec<u8>) {
        let _ = self.tx.send(Msg::Frame { at, width, height, bgra });
    }

    /// Bu ana (ya da öncesindeki en yakın) kareyi arka planda çöz.
    pub fn seek(&self, at: Instant) {
        self.store.lock().unwrap().seek = Some(at);
        let _ = self.tx.try_send(Msg::Wake);
    }

    /// Arama sonucu hazırsa al.
    pub fn take_decoded(&self) -> Option<Decoded> {
        self.store.lock().unwrap().decoded.take()
    }

    /// Kayıttaki en eski ve en yeni kare.
    pub fn range(&self) -> Option<(Instant, Instant)> {
        let s = self.store.lock().unwrap();
        Some((s.frames.front()?.at, s.frames.back()?.at))
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.store.lock().unwrap().bytes
    }

    /// Şu anki kayıt hızı (kare/sn).
    #[cfg(test)]
    pub fn rate(&self) -> f64 {
        1e6 / self.interval_us.load(Ordering::Relaxed).max(1) as f64
    }
}

fn worker(rx: mpsc::Receiver<Msg>, store: Arc<Mutex<Store>>, window: Duration, interval_us: Arc<AtomicU64>, budget: Arc<AtomicUsize>) {
    while let Ok(msg) = rx.recv() {
        if let Msg::Frame { at, width, height, bgra } = msg {
            let Some(jpeg) = encode(&bgra, width, height) else { continue };
            drop(bgra);
            let mut s = store.lock().unwrap();
            s.bytes += jpeg.len();
            s.frames.push_back(Stored { at, jpeg });
            // Süre penceresi ve bütçe: en eskiler atılır.
            let cap = budget.load(Ordering::Relaxed);
            while let Some(front) = s.frames.front() {
                if at.duration_since(front.at) > window || (s.bytes > cap && s.frames.len() > 1) {
                    let gone = s.frames.pop_front().map(|f| f.jpeg.len()).unwrap_or(0);
                    s.bytes -= gone;
                } else {
                    break;
                }
            }
            // Kayıt hızı: seçilen süre bütçeye sığacak kadar (ortalama kare boyuna göre).
            if s.frames.len() >= 5 {
                let avg = s.bytes as f64 / s.frames.len() as f64;
                let rate = (cap as f64 / (window.as_secs_f64() * avg)).clamp(MIN_FPS, MAX_FPS);
                interval_us.store((1e6 / rate) as u64, Ordering::Relaxed);
            }
        }
        // Bekleyen arama (yalnız en sonuncusu).
        let job = {
            let mut s = store.lock().unwrap();
            let Some(target) = s.seek.take() else { continue };
            let ix = s.frames.partition_point(|f| f.at <= target).saturating_sub(1);
            let found = s.frames.get(ix).filter(|f| s.decoded_at != Some(f.at)).map(|f| (f.at, f.jpeg.clone()));
            if let Some((at, _)) = &found {
                s.decoded_at = Some(*at);
            }
            found
        };
        if let Some((_, jpeg)) = job {
            if let Some((width, height, bgra)) = decode(&jpeg) {
                store.lock().unwrap().decoded = Some(Decoded { width, height, bgra });
            }
        }
    }
}

/// BGRA → (en çok 1280 geniş, alan ortalamalı) RGB → JPEG.
fn encode(bgra: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || bgra.len() < (w * h * 4) as usize {
        return None;
    }
    let (ow, oh) = if w > MAX_W { (MAX_W, ((h as u64 * MAX_W as u64 / w as u64) as u32).max(1)) } else { (w, h) };
    let mut rgb = vec![0u8; (ow * oh * 3) as usize];
    for oy in 0..oh {
        let y0 = (oy as u64 * h as u64 / oh as u64) as u32;
        let y1 = (((oy + 1) as u64 * h as u64 / oh as u64) as u32).max(y0 + 1).min(h);
        for ox in 0..ow {
            let x0 = (ox as u64 * w as u64 / ow as u64) as u32;
            let x1 = (((ox + 1) as u64 * w as u64 / ow as u64) as u32).max(x0 + 1).min(w);
            let (mut b, mut g, mut r, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in y0..y1 {
                let row = (y * w * 4) as usize;
                for x in x0..x1 {
                    let i = row + (x * 4) as usize;
                    b += bgra[i] as u32;
                    g += bgra[i + 1] as u32;
                    r += bgra[i + 2] as u32;
                    n += 1;
                }
            }
            let o = ((oy * ow + ox) * 3) as usize;
            rgb[o] = (r / n) as u8;
            rgb[o + 1] = (g / n) as u8;
            rgb[o + 2] = (b / n) as u8;
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).write_image(&rgb, ow, oh, image::ExtendedColorType::Rgb8).ok()?;
    Some(out)
}

fn decode(jpeg: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let rgb = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for px in rgb.pixels() {
        bgra.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    Some((w, h, bgra))
}

/// İzleyicinin geri sarma durumu (canlıdaysa yok). Gösterilen an = duraklatılmışsa `position`,
/// değilse `position + (şimdi − anchor)` (geride, gerçek zamanda oynar).
pub struct View {
    pub position: Instant,
    pub anchor: Instant,
    pub paused: bool,
    pub image: Option<(Arc<gpui_kit::RenderImage>, u32, u32)>,
}

impl View {
    pub fn shown(&self, now: Instant) -> Instant {
        if self.paused { self.position } else { self.position + now.duration_since(self.anchor) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, seed: u8) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i as u8).wrapping_mul(seed), seed, 200, 255]).collect()
    }

    fn settle(r: &Recorder, frames: usize) {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if r.store.lock().unwrap().frames.len() >= frames {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Kare küçültülür (≤1280) ve JPEG olarak döner; çözünce aynı boyut.
    #[test]
    fn encode_downscales_and_roundtrips() {
        let jpeg = encode(&frame(1920, 1080, 7), 1920, 1080).unwrap();
        let (w, h, bgra) = decode(&jpeg).unwrap();
        assert_eq!((w, h), (1280, 720));
        assert_eq!(bgra.len(), 1280 * 720 * 4);
        assert!(jpeg.len() < 1280 * 720 * 3 / 4, "{} bayt", jpeg.len());
    }

    /// Süre penceresi dışındaki kareler atılır; arama en yakın önceki kareyi çözer.
    #[test]
    fn window_and_seek() {
        let mut r = Recorder::new(2, BUDGET_BYTES);
        let base = Instant::now();
        for i in 0..40u32 {
            r.push_wait(base + Duration::from_millis(100 * i as u64), 64, 36, frame(64, 36, i as u8));
        }
        settle(&r, 20);
        std::thread::sleep(Duration::from_millis(100));
        let (old, new) = r.range().unwrap();
        assert!(new.duration_since(old) <= Duration::from_secs(2), "{:?}", new.duration_since(old));
        assert_eq!(new, base + Duration::from_millis(3900));
        r.seek(base + Duration::from_millis(3050));
        let end = Instant::now() + Duration::from_secs(5);
        let d = loop {
            if let Some(d) = r.take_decoded() {
                break d;
            }
            assert!(Instant::now() < end, "çözülmedi");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(r.store.lock().unwrap().decoded_at, Some(base + Duration::from_millis(3000)));
        assert_eq!((d.width, d.height), (64, 36));
    }

    /// Bütçe aşılınca en eskiler atılır ve kayıt hızı düşer.
    #[test]
    fn budget_caps_memory_and_lowers_rate() {
        let one = encode(&frame(640, 360, 3), 640, 360).unwrap().len();
        let mut r = Recorder::new(300, one * 10);
        let base = Instant::now();
        for i in 0..30u32 {
            r.push_wait(base + Duration::from_millis(100 * i as u64), 640, 360, frame(640, 360, 3));
        }
        std::thread::sleep(Duration::from_millis(300));
        settle(&r, 1);
        let end = Instant::now() + Duration::from_secs(10);
        while r.store.lock().unwrap().frames.back().map(|f| f.at) != Some(base + Duration::from_millis(2900)) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(r.bytes() <= one * 10 + one, "{} > {}", r.bytes(), one * 10);
        assert!(r.rate() <= MIN_FPS + 0.01, "hız {}", r.rate());
    }
}
