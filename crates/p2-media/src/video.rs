//! Görüntü: gelen ekran paylaşımı / kamera karelerini BGRA'ya çevirip arayüze verir;
//! kendi ekranını (libwebrtc DesktopCapturer) ya da test desenini yerel görüntü izine iter.
//!
//! Electron'daki gibi her eşte baştan bir görüntü izi vardır (CallProvider.tsx: video
//! alıcı-vericisi `sendrecv`); yayın açılınca yalnız kaynağa kare itilir, yeniden pazarlık
//! gerekmez.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc as fmpsc;
use libwebrtc::desktop_capturer::{DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions};
use libwebrtc::native::yuv_helper;
use libwebrtc::prelude::*;
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_stream::native::NativeVideoStream;

use crate::engine::Event;

/// Son kare (BGRA, GPUI `RenderImage` biçimi).
pub struct VideoFrameData {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Kullanıcı başına son kare. Arayüz `Event::Video` gelince alır (kopyasız `take`).
pub type Frames = Arc<Mutex<HashMap<String, VideoFrameData>>>;

/// Gelen görüntü izini çözer; arayüz önceki kareyi almadıysa yeni olay göndermez
/// (kareler birikmez, en yenisi kalır).
pub fn spawn_receiver(user: String, track: RtcVideoTrack, frames: Frames, events: fmpsc::UnboundedSender<Event>) -> Arc<AtomicBool> {
    let alive = Arc::new(AtomicBool::new(true));
    let flag = alive.clone();
    tokio::spawn(async move {
        let mut stream = NativeVideoStream::new(track);
        while let Some(frame) = stream.next().await {
            if !flag.load(Ordering::Relaxed) {
                break;
            }
            let (w, h) = (frame.buffer.width(), frame.buffer.height());
            if w == 0 || h == 0 {
                continue;
            }
            // Windows'ta yazılım çözücü I420 verir; yerel (donanım) arabellekleri bu sürümde
            // dışarıdan çevrilemiyor — o kareler atlanır.
            let Some(i420) = frame.buffer.as_i420() else { continue };
            let (sy, su, sv) = i420.strides();
            let (y, u, v) = i420.data();
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            // libyuv "ARGB" = bellekte B,G,R,A (GPUI'nin beklediği sıra).
            yuv_helper::i420_to_argb(y, sy, u, su, v, sv, &mut bgra, w * 4, w as i32, h as i32);
            frames.lock().unwrap().insert(user.clone(), VideoFrameData { width: w, height: h, bgra });
            let _ = events.unbounded_send(Event::Video { user: user.clone() });
        }
        frames.lock().unwrap().remove(&user);
        let _ = events.unbounded_send(Event::VideoGone { user });
    });
    alive
}

/// Kendi görüntülerimin önizleme anahtarları (`Frames` içinde): yayınım ve kameram.
pub const LOCAL_SCREEN: &str = "@yerel-ekran";
pub const LOCAL_CAMERA: &str = "@yerel-kamera";

/// Önizleme karesinin en büyük genişliği (arayüze yüklenen doku küçük kalsın).
const PREVIEW_MAX_W: u32 = 1280;

/// Kendi görüntümün önizlemesi: giden I420 kareden (en çok `max_fps`, en çok 1280 geniş) BGRA
/// üretip `Frames`'e yazar ve `Event::Video` gönderir. Paylaşım/kamera kapanınca `clear`.
#[derive(Clone)]
pub struct Preview {
    key: &'static str,
    frames: Frames,
    events: fmpsc::UnboundedSender<Event>,
    last: Arc<Mutex<Option<Instant>>>,
    interval: Duration,
}

impl Preview {
    pub fn new(key: &'static str, frames: Frames, events: fmpsc::UnboundedSender<Event>, max_fps: u32) -> Self {
        Self { key, frames, events, last: Arc::default(), interval: Duration::from_millis(1000 / max_fps.max(1) as u64) }
    }

    pub fn push(&self, buffer: &mut I420Buffer) {
        {
            let mut last = self.last.lock().unwrap();
            if last.is_some_and(|l| l.elapsed() < self.interval) {
                return;
            }
            *last = Some(Instant::now());
        }
        let (w, h) = (buffer.width(), buffer.height());
        if w == 0 || h == 0 {
            return;
        }
        let scaled;
        let src: &I420Buffer = if w > PREVIEW_MAX_W {
            let nh = ((h as u64 * PREVIEW_MAX_W as u64 / w as u64) as i32 & !1).max(2);
            scaled = buffer.scale(PREVIEW_MAX_W as i32, nh);
            &scaled
        } else {
            buffer
        };
        let (w, h) = (src.width(), src.height());
        let (sy, su, sv) = src.strides();
        let (y, u, v) = src.data();
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        yuv_helper::i420_to_argb(y, sy, u, su, v, sv, &mut bgra, w * 4, w as i32, h as i32);
        self.frames.lock().unwrap().insert(self.key.to_string(), VideoFrameData { width: w, height: h, bgra });
        let _ = self.events.unbounded_send(Event::Video { user: self.key.to_string() });
    }

    /// Önizleme bitti: son kare silinir, arayüz kutucuğu avatara döner.
    pub fn clear(&self) {
        self.frames.lock().unwrap().remove(self.key);
        let _ = self.events.unbounded_send(Event::VideoGone { user: self.key.to_string() });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScreenSource {
    Screen(u64),
    Window(u64),
    /// Testler: hareketli renk deseni (gerçek ekran yakalanmaz).
    TestPattern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenInfo {
    pub source: ScreenSource,
    pub title: String,
}

/// Paylaşılabilir ekranlar ve pencereler (ScreenShareModal'daki liste).
pub fn screen_sources() -> Vec<ScreenInfo> {
    let mut out = Vec::new();
    for (kind, screen) in [(DesktopCaptureSourceType::Screen, true), (DesktopCaptureSourceType::Window, false)] {
        let Some(capturer) = DesktopCapturer::new(DesktopCapturerOptions::new(kind)) else { continue };
        for (ix, s) in capturer.get_source_list().into_iter().enumerate() {
            let title = s.title();
            out.push(ScreenInfo {
                source: if screen { ScreenSource::Screen(s.id()) } else { ScreenSource::Window(s.id()) },
                title: if screen { format!("Ekran {}", ix + 1) } else { title },
            });
        }
    }
    out
}

/// Seçim penceresi için kaynağın küçük resmi (BGRA, en çok `max_w` genişlikte). Engelleyicidir;
/// arka planda çağrılır.
///
/// - Pencere: `PrintWindow(PW_RENDERFULLCONTENT)` — DirectComposition / donanım hızlandırmalı
///   pencerelerde de içerik gelir (GDI `BitBlt` siyah verir). Sonuç boşsa libwebrtc yakalayıcısı.
/// - Ekran: libwebrtc DesktopCapturer'dan tek kare (Windows'ta DXGI masaüstü çoğaltma; yayınla
///   aynı yol, donanım yüzeyleri dahil).
pub fn source_thumbnail(source: &ScreenSource, max_w: u32) -> Option<VideoFrameData> {
    match source {
        ScreenSource::TestPattern => None,
        ScreenSource::Window(id) => thumb::print_window(*id, max_w).or_else(|| thumb::one_frame(false, *id, max_w)),
        ScreenSource::Screen(id) => thumb::one_frame(true, *id, max_w),
    }
}

mod thumb {
    use super::*;

    /// BGRA kareyi en-boy oranını koruyarak `max_w` genişliğe küçültür (kutu ortalaması).
    pub(super) fn downscale(src: &[u8], stride: usize, w: u32, h: u32, max_w: u32) -> Option<VideoFrameData> {
        if w == 0 || h == 0 || src.len() < stride * (h as usize - 1) + w as usize * 4 {
            return None;
        }
        let (nw, nh) = if w > max_w { (max_w, ((h as u64 * max_w as u64 / w as u64) as u32).max(1)) } else { (w, h) };
        let mut out = vec![0u8; (nw * nh * 4) as usize];
        let mut lit = 0u64;
        for y in 0..nh {
            let y0 = (y as u64 * h as u64 / nh as u64) as u32;
            let y1 = (((y + 1) as u64 * h as u64 / nh as u64) as u32).max(y0 + 1).min(h);
            for x in 0..nw {
                let x0 = (x as u64 * w as u64 / nw as u64) as u32;
                let x1 = (((x + 1) as u64 * w as u64 / nw as u64) as u32).max(x0 + 1).min(w);
                // En çok 3×3 örnek (hız): kutunun içinden eşit aralıklı.
                let (mut acc, mut n) = ([0u32; 3], 0u32);
                for sy in 0..3u32 {
                    let yy = y0 + (y1 - y0) * sy / 3;
                    for sx in 0..3u32 {
                        let xx = x0 + (x1 - x0) * sx / 3;
                        let i = yy as usize * stride + xx as usize * 4;
                        acc[0] += src[i] as u32;
                        acc[1] += src[i + 1] as u32;
                        acc[2] += src[i + 2] as u32;
                        n += 1;
                    }
                }
                let o = ((y * nw + x) * 4) as usize;
                out[o] = (acc[0] / n) as u8;
                out[o + 1] = (acc[1] / n) as u8;
                out[o + 2] = (acc[2] / n) as u8;
                out[o + 3] = 255;
                lit += (out[o] as u64 + out[o + 1] as u64 + out[o + 2] as u64).min(1);
            }
        }
        // Tamamen siyah kare (yakalanamadı) küçük resim sayılmaz.
        (lit > 0).then_some(VideoFrameData { width: nw, height: nh, bgra: out })
    }

    /// libwebrtc yakalayıcısından tek kare (ilk kareler "geçici hata" dönebilir: birkaç deneme).
    pub(super) fn one_frame(screen: bool, id: u64, max_w: u32) -> Option<VideoFrameData> {
        let kind = if screen { DesktopCaptureSourceType::Screen } else { DesktopCaptureSourceType::Window };
        let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(kind))?;
        let chosen = capturer.get_source_list().into_iter().find(|s| s.id() == id)?;
        let slot: Arc<Mutex<Option<VideoFrameData>>> = Arc::new(Mutex::new(None));
        let sink = slot.clone();
        capturer.start_capture(Some(chosen), move |result| {
            let Ok(frame) = result else { return };
            let (w, h) = (frame.width().max(0) as u32, frame.height().max(0) as u32);
            if let Some(t) = downscale(frame.data(), frame.stride() as usize, w, h, max_w) {
                *sink.lock().unwrap() = Some(t);
            }
        });
        for _ in 0..8 {
            capturer.capture_frame();
            if let Some(t) = slot.lock().unwrap().take() {
                return Some(t);
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        None
    }

    /// Pencere içeriği `PrintWindow(PW_RENDERFULLCONTENT)` ile (simge durumundaki pencere: yok).
    pub(super) fn print_window(id: u64, max_w: u32) -> Option<VideoFrameData> {
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindow};
        const PW_RENDERFULLCONTENT: u32 = 2;
        unsafe {
            let hwnd = HWND(id as usize as *mut _);
            if !IsWindow(Some(hwnd)).as_bool() || IsIconic(hwnd).as_bool() {
                return None;
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            let (w, h) = ((r.right - r.left).max(0), (r.bottom - r.top).max(0));
            if w < 8 || h < 8 || w > 16384 || h > 16384 {
                return None;
            }
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                ..Default::default()
            };
            let result = (|| {
                let bmp = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
                let old = SelectObject(dc, bmp.into());
                let ok = PrintWindow(hwnd, dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
                let _ = GdiFlush();
                let out = if ok && !bits.is_null() {
                    let data = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
                    downscale(data, w as usize * 4, w as u32, h as u32, max_w)
                } else {
                    None
                };
                SelectObject(dc, old);
                let _ = DeleteObject(bmp.into());
                out
            })();
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            result
        }
    }
}

/// Yayın kaynağı: gerçek yakalayıcı ya da test deseni.
pub struct Sharer {
    capturer: Option<DesktopCapturer>,
    source: NativeVideoSource,
    start: Instant,
    frame_no: u32,
    preview: Option<Preview>,
    /// Kaynağın son kare yüksekliği (kodlayıcı ölçeği: istenen kalite / gerçek yükseklik).
    height: Arc<AtomicU32>,
}

const PATTERN_W: u32 = 1280;
const PATTERN_H: u32 = 720;

impl Sharer {
    pub fn start(source: ScreenSource, video: NativeVideoSource, preview: Option<Preview>) -> Result<Self, String> {
        let height = Arc::new(AtomicU32::new(if matches!(source, ScreenSource::TestPattern) { PATTERN_H } else { 0 }));
        let capturer = match &source {
            ScreenSource::TestPattern => None,
            ScreenSource::Screen(id) | ScreenSource::Window(id) => {
                let kind = if matches!(source, ScreenSource::Screen(_)) { DesktopCaptureSourceType::Screen } else { DesktopCaptureSourceType::Window };
                let mut options = DesktopCapturerOptions::new(kind);
                options.set_include_cursor(true);
                let mut capturer = DesktopCapturer::new(options).ok_or_else(|| p2_i18n::t("media2.capOpen"))?;
                let chosen = capturer.get_source_list().into_iter().find(|s| s.id() == *id).ok_or_else(|| p2_i18n::t("media2.capGone"))?;
                let target = video.clone();
                let start = Instant::now();
                let (tap, seen) = (preview.clone(), height.clone());
                capturer.start_capture(Some(chosen), move |result| {
                    let Ok(frame) = result else { return };
                    let (w, h) = (frame.width().max(0) as u32 & !1, frame.height().max(0) as u32 & !1);
                    if w == 0 || h == 0 {
                        return;
                    }
                    let mut buffer = I420Buffer::new(w, h);
                    let (sy, su, sv) = buffer.strides();
                    let (y, u, v) = buffer.data_mut();
                    // Masaüstü karesi bellekte BGRA = libyuv "ARGB".
                    yuv_helper::argb_to_i420(frame.data(), frame.stride(), y, sy, u, su, v, sv, w as i32, h as i32);
                    let mut frame = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: start.elapsed().as_micros() as i64, frame_metadata: None, buffer };
                    target.capture_frame(&frame);
                    seen.store(h, Ordering::Relaxed);
                    if let Some(tap) = &tap {
                        tap.push(&mut frame.buffer);
                    }
                });
                Some(capturer)
            }
        };
        Ok(Self { capturer, source: video, start: Instant::now(), frame_no: 0, preview, height })
    }

    /// Saat vuruşu (yayın kare hızında): yakalayıcıdan kare iste ya da desen üret.
    pub fn tick(&mut self) {
        if let Some(capturer) = &mut self.capturer {
            capturer.capture_frame();
            return;
        }
        let mut buffer = I420Buffer::new(PATTERN_W, PATTERN_H);
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data_mut();
        let shift = self.frame_no * 8;
        for row in 0..PATTERN_H {
            for col in 0..PATTERN_W {
                y[(row * sy + col) as usize] = 50 + (((col + row + shift) / 128) % 6 * 30) as u8;
            }
        }
        for row in 0..PATTERN_H / 2 {
            for col in 0..PATTERN_W / 2 {
                let band = ((col * 2 + shift) / 200) % 6;
                u[(row * su + col) as usize] = (90 + band * 20) as u8;
                v[(row * sv + col) as usize] = (170 - band * 12) as u8;
            }
        }
        let mut frame = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: self.start.elapsed().as_micros() as i64, frame_metadata: None, buffer };
        self.source.capture_frame(&frame);
        if let Some(tap) = &self.preview {
            tap.push(&mut frame.buffer);
        }
        self.frame_no += 1;
    }

    /// Kaynağın gerçek yüksekliği (henüz kare yoksa 0).
    pub fn height(&self) -> u32 {
        self.height.load(Ordering::Relaxed)
    }

    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }
}
