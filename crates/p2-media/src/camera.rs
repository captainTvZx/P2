//! Kamera yakalama (Electron'da `getUserMedia({video})`): Windows Media Foundation kaynak
//! okuyucusu, video işlemcisiyle RGB32'ye çevrilmiş kareler → I420 → `NativeVideoSource`.
//! Testler gerçek kamera açmaz: `CameraSource::TestPattern` hareketli bir desen üretir.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use libwebrtc::native::yuv_helper;
use libwebrtc::prelude::*;
use libwebrtc::video_source::native::NativeVideoSource;
#[cfg(windows)]
use windows::Win32::Media::MediaFoundation::*;
#[cfg(windows)]
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
#[cfg(windows)]
use windows::core::PWSTR;

use crate::video::Preview;

#[derive(Clone, Debug, PartialEq)]
pub enum CameraSource {
    /// Aygıt kimliği (sembolik bağlantı); `None` = ilk kamera.
    Device(Option<String>),
    TestPattern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraInfo {
    pub id: String,
    pub name: String,
}

#[cfg(windows)]
const READER_FIRST_VIDEO: u32 = 0xFFFF_FFFC; // MF_SOURCE_READER_FIRST_VIDEO_STREAM
#[cfg(windows)]
const STREAM_END: u32 = 0x2; // MF_SOURCE_READERF_ENDOFSTREAM

#[cfg(windows)]
fn mf_string(activate: &IMFActivate, key: &windows::core::GUID) -> String {
    unsafe {
        let mut value = PWSTR::null();
        let mut len = 0u32;
        if activate.GetAllocatedString(key, &mut value, &mut len).is_err() || value.is_null() {
            return String::new();
        }
        let s = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0 as *const _));
        s
    }
}

#[cfg(windows)]
fn devices() -> windows::core::Result<Vec<IMFActivate>> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
        attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID)?;
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut list, &mut count)?;
        let mut out = Vec::new();
        for i in 0..count as usize {
            if let Some(a) = (*list.add(i)).take() {
                out.push(a);
            }
        }
        if !list.is_null() {
            CoTaskMemFree(Some(list as *const _));
        }
        Ok(out)
    }
}

#[cfg(windows)]
fn init_mf() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    }
}

/// Bağlı kameralar.
#[cfg(not(windows))]
pub fn cameras() -> Vec<CameraInfo> {
    avf::cameras()
}

/// Bağlı kameralar.
#[cfg(windows)]
pub fn cameras() -> Vec<CameraInfo> {
    init_mf();
    devices()
        .unwrap_or_default()
        .iter()
        .map(|a| CameraInfo { id: mf_string(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK), name: mf_string(a, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME) })
        .collect()
}

pub struct Camera {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Camera {
    /// Yakalamayı kendi iş parçacığında başlatır; kamera açılamazsa hata döner.
    /// `preview`: kendi kameramın önizlemesi (arayüzdeki kendi kutucuğum).
    pub fn start(source: CameraSource, video: NativeVideoSource, preview: Option<Preview>) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("p2-kamera".into())
            .spawn(move || match source {
                CameraSource::TestPattern => {
                    let _ = ready_tx.send(Ok(()));
                    pattern_loop(&video, &flag, preview.as_ref());
                }
                #[cfg(windows)]
                CameraSource::Device(id) => match open_reader(id.as_deref()) {
                    Ok((reader, w, h, stride)) => {
                        let _ = ready_tx.send(Ok(()));
                        read_loop(&reader, w, h, stride, &video, &flag, preview.as_ref());
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                },
                #[cfg(not(windows))]
                CameraSource::Device(id) => avf::run(id.as_deref(), &video, &flag, preview.as_ref(), ready_tx),
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(Self { stop, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err(p2_i18n::t("media2.camTimeout").into())
            }
        }
    }
}

#[cfg(windows)]
fn open_reader(id: Option<&str>) -> Result<(IMFSourceReader, u32, u32, i32), String> {
    init_mf();
    let list = devices().map_err(|e| format!("Kameralar listelenemedi: {e}"))?;
    let chosen = match id {
        Some(id) => list.into_iter().find(|a| mf_string(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK) == id),
        None => list.into_iter().next(),
    }
    .ok_or_else(|| p2_i18n::t("media2.camMissing"))?;
    unsafe {
        let source: IMFMediaSource = chosen.ActivateObject().map_err(|e| p2_i18n::tv("media2.camOpen", &[("error", &e.to_string())]))?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).map_err(|e| e.to_string())?;
        let attrs = attrs.ok_or("öznitelik")?;
        // Video işlemcisi: kameranın biçimi (NV12/YUY2/MJPG) ne olursa olsun RGB32 verir.
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).map_err(|e| e.to_string())?;
        let reader = MFCreateSourceReaderFromMediaSource(&source, &attrs).map_err(|e| format!("Kamera okuyucusu: {e}"))?;
        let mt = MFCreateMediaType().map_err(|e| e.to_string())?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| e.to_string())?;
        reader.SetCurrentMediaType(READER_FIRST_VIDEO, None, &mt).map_err(|e| format!("Kamera biçimi ayarlanamadı: {e}"))?;
        let current = reader.GetCurrentMediaType(READER_FIRST_VIDEO).map_err(|e| e.to_string())?;
        let size = current.GetUINT64(&MF_MT_FRAME_SIZE).map_err(|e| e.to_string())?;
        let (w, h) = ((size >> 32) as u32, (size & 0xFFFF_FFFF) as u32);
        let stride = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or((w * 4) as i32);
        Ok((reader, w, h, stride))
    }
}

#[cfg(windows)]
fn read_loop(reader: &IMFSourceReader, w: u32, h: u32, stride: i32, video: &NativeVideoSource, stop: &AtomicBool, preview: Option<&Preview>) {
    let start = Instant::now();
    let (ew, eh) = (w & !1, h & !1);
    while !stop.load(Ordering::Relaxed) {
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        let ok = unsafe { reader.ReadSample(READER_FIRST_VIDEO, 0, None, Some(&mut flags), None, Some(&mut sample)) };
        if ok.is_err() || flags & STREAM_END != 0 {
            break;
        }
        let Some(sample) = sample else { continue };
        let Ok(buffer) = (unsafe { sample.ConvertToContiguousBuffer() }) else { continue };
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        if unsafe { buffer.Lock(&mut ptr, None, Some(&mut len)) }.is_err() || ptr.is_null() {
            continue;
        }
        let data = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        let mut i420 = I420Buffer::new(ew, eh);
        let (sy, su, sv) = i420.strides();
        let (y, u, v) = i420.data_mut();
        // Negatif adım: görüntü alttan yukarı; libyuv negatif yükseklikle ters çevirir.
        let (rows, step) = if stride < 0 { (-(eh as i32), -stride) } else { (eh as i32, stride) };
        if (step as usize) * (eh as usize) <= data.len() {
            yuv_helper::argb_to_i420(data, step as u32, y, sy, u, su, v, sv, ew as i32, rows);
            let mut frame = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: start.elapsed().as_micros() as i64, frame_metadata: None, buffer: i420 };
            video.capture_frame(&frame);
            if let Some(p) = preview {
                p.push(&mut frame.buffer);
            }
        }
        unsafe {
            let _ = buffer.Unlock();
        }
    }
}

/// Testler için: 30 fps, ortada dönen bir kare.
fn pattern_loop(video: &NativeVideoSource, stop: &AtomicBool, preview: Option<&Preview>) {
    const W: u32 = 640;
    const H: u32 = 480;
    let start = Instant::now();
    let mut n = 0u32;
    while !stop.load(Ordering::Relaxed) {
        let mut i420 = I420Buffer::new(W, H);
        let (sy, su, sv) = i420.strides();
        let (y, u, v) = i420.data_mut();
        let cx = (W / 2) as i32 + ((n as f32 / 15.).sin() * 160.) as i32;
        for row in 0..H {
            for col in 0..W {
                let inside = (col as i32 - cx).abs() < 60 && (row as i32 - (H / 2) as i32).abs() < 60;
                y[(row * sy + col) as usize] = if inside { 210 } else { 60 + (row / 4) as u8 };
            }
        }
        for row in 0..H / 2 {
            for col in 0..W / 2 {
                u[(row * su + col) as usize] = 150;
                v[(row * sv + col) as usize] = 100;
            }
        }
        let mut frame = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: start.elapsed().as_micros() as i64, frame_metadata: None, buffer: i420 };
        video.capture_frame(&frame);
        if let Some(p) = preview {
            p.push(&mut frame.buffer);
        }
        n += 1;
        std::thread::sleep(Duration::from_millis(33));
    }
}

/// Windows ve macOS dışında kamera desteği yok.
#[cfg(not(any(windows, target_os = "macos")))]
mod avf {
    use super::*;
    pub fn cameras() -> Vec<CameraInfo> {
        Vec::new()
    }
    pub fn run(_id: Option<&str>, _video: &NativeVideoSource, _stop: &AtomicBool, _preview: Option<&Preview>, ready: std::sync::mpsc::Sender<Result<(), String>>) {
        let _ = ready.send(Err(p2_i18n::t("media2.camMissing").into()));
    }
}

/// macOS: kamera AVFoundation üzerinden (AVCaptureSession + BGRA video çıkışı). İlk açılışta
/// macOS kamera izni sorar (Info.plist `NSCameraUsageDescription`).
#[cfg(target_os = "macos")]
mod avf {
    use super::*;
    use std::sync::Mutex;

    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession, AVCaptureVideoDataOutput,
        AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
    };
    use objc2_core_media::CMSampleBuffer;
    use objc2_core_video::{
        CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
        CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSString};

    /// Kare işleyicisi: AVFoundation kendi kuyruğunda çağırır.
    struct Sink {
        video: NativeVideoSource,
        preview: Option<Preview>,
        start: Instant,
    }

    struct Ivars {
        sink: Mutex<Option<Sink>>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "P2CameraDelegate"]
        #[ivars = Ivars]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for Delegate {
            #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
            fn did_output(&self, _output: &AVCaptureOutput, sample: &CMSampleBuffer, _connection: &AVCaptureConnection) {
                let guard = self.ivars().sink.lock().unwrap();
                if let Some(sink) = guard.as_ref() {
                    frame(sink, sample);
                }
            }
        }
    );

    impl Delegate {
        fn new(sink: Sink) -> Retained<Self> {
            let this = Self::alloc().set_ivars(Ivars { sink: Mutex::new(Some(sink)) });
            unsafe { msg_send![super(this), init] }
        }
    }

    /// BGRA piksel arabelleğini I420'ye çevirip yayına verir.
    fn frame(sink: &Sink, sample: &CMSampleBuffer) {
        let Some(pixels) = (unsafe { sample.image_buffer() }) else { return };
        unsafe {
            if CVPixelBufferLockBaseAddress(&pixels, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return;
            }
            let (w, h, stride) = (CVPixelBufferGetWidth(&pixels) as u32, CVPixelBufferGetHeight(&pixels) as u32, CVPixelBufferGetBytesPerRow(&pixels) as u32);
            let base = CVPixelBufferGetBaseAddress(&pixels) as *const u8;
            let (ew, eh) = (w & !1, h & !1);
            if !base.is_null() && ew > 0 && eh > 0 {
                let data = std::slice::from_raw_parts(base, (stride * h) as usize);
                let mut i420 = I420Buffer::new(ew, eh);
                let (sy, su, sv) = i420.strides();
                let (y, u, v) = i420.data_mut();
                // libyuv "ARGB" = bellekte B,G,R,A sırası (kCVPixelFormatType_32BGRA).
                yuv_helper::argb_to_i420(data, stride, y, sy, u, su, v, sv, ew as i32, eh as i32);
                let mut f = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: sink.start.elapsed().as_micros() as i64, frame_metadata: None, buffer: i420 };
                sink.video.capture_frame(&f);
                if let Some(p) = &sink.preview {
                    p.push(&mut f.buffer);
                }
            }
            CVPixelBufferUnlockBaseAddress(&pixels, CVPixelBufferLockFlags::ReadOnly);
        }
    }

    #[allow(deprecated)]
    fn devices() -> Vec<Retained<AVCaptureDevice>> {
        let Some(media) = (unsafe { AVMediaTypeVideo }) else { return Vec::new() };
        unsafe { AVCaptureDevice::devicesWithMediaType(media) }.to_vec()
    }

    pub fn cameras() -> Vec<CameraInfo> {
        devices()
            .iter()
            .map(|d| unsafe { CameraInfo { id: d.uniqueID().to_string(), name: d.localizedName().to_string() } })
            .collect()
    }

    /// Kamera iznini ister; kullanıcı yanıtlayana kadar (en çok 60 sn) bekler.
    fn authorized() -> bool {
        let Some(media) = (unsafe { AVMediaTypeVideo }) else { return false };
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) } {
            AVAuthorizationStatus::Authorized => true,
            AVAuthorizationStatus::NotDetermined => {
                let (tx, rx) = std::sync::mpsc::channel();
                let tx = Mutex::new(tx);
                let block = RcBlock::new(move |granted: Bool| {
                    let _ = tx.lock().unwrap().send(granted.as_bool());
                });
                unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block) };
                rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
            }
            _ => false,
        }
    }

    pub fn run(id: Option<&str>, video: &NativeVideoSource, stop: &AtomicBool, preview: Option<&Preview>, ready: std::sync::mpsc::Sender<Result<(), String>>) {
        if !authorized() {
            let _ = ready.send(Err(p2_i18n::tv("media2.camOpen", &[("error", "macOS kamera izni verilmedi")])));
            return;
        }
        let list = devices();
        let device = match id {
            Some(id) => list.into_iter().find(|d| unsafe { d.uniqueID() }.to_string() == id),
            None => list.into_iter().next(),
        };
        let Some(device) = device else {
            let _ = ready.send(Err(p2_i18n::t("media2.camMissing").into()));
            return;
        };
        let input = match unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) } {
            Ok(i) => i,
            Err(e) => {
                let _ = ready.send(Err(p2_i18n::tv("media2.camOpen", &[("error", &e.localizedDescription().to_string())])));
                return;
            }
        };
        let session = unsafe { AVCaptureSession::new() };
        let output = unsafe { AVCaptureVideoDataOutput::new() };
        unsafe {
            if !session.canAddInput(&input) || !session.canAddOutput(&output) {
                let _ = ready.send(Err(p2_i18n::tv("media2.camOpen", &[("error", "oturum kurulamadı")])));
                return;
            }
            session.addInput(&input);
            // BGRA iste: dönüşümü AVFoundation yapar, kameranın yerel biçimi önemsiz.
            let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
            let fmt = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let fmt: &AnyObject = &fmt;
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[fmt]);
            output.setVideoSettings(Some(&settings));
            output.setAlwaysDiscardsLateVideoFrames(true);
            session.addOutput(&output);
        }
        let delegate = Delegate::new(Sink { video: video.clone(), preview: preview.cloned(), start: Instant::now() });
        let queue = DispatchQueue::new("p2.kamera", None);
        unsafe {
            output.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(&*delegate)), Some(&queue));
            session.startRunning();
        }
        let _ = ready.send(Ok(()));
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            session.stopRunning();
            output.setSampleBufferDelegate_queue(None, None);
        }
        delegate.ivars().sink.lock().unwrap().take();
    }
}
