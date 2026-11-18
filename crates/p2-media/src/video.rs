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
