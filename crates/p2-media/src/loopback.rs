//! Bilgisayar sesi (WASAPI loopback) — `native/loopback/src/loopback.cc` + `systemAudio.ts`
//! karşılığı. Ekran paylaşımında "Bilgisayar sesini paylaş" seçiliyse hoparlörden çıkan ses
//! yakalanır ve mikrofonla karıştırılıp aynı ses izinden gider (CallProvider.tsx
//! `applyShareAudio`: mikrofon + sistem sesi tek MediaStreamDestination'da toplanır).
//!
//! 1) Tercih: kendi süreç ağacımızı HARİÇ tutan işlem loopback'i (Windows 10 20348+ /
//!    Windows 11). Aramadaki karşı tarafın sesi (bizim hoparlörümüz) yayına geri karışmaz.
//! 2) Yedek: uç nokta loopback'i (cpal; varsayılan çıkış aygıtı, kendi sesimiz dahil).
//!
//! Örnekler 48 kHz mono halkaya iter; motor 10 ms'lik saatinde `mix_into` ile mikrofon
//! karesine ekler. Halka 400 ms'yi aşarsa en eskisi atılır (systemAudio.ts worklet'i gibi).
//!
//! Testler: `P2_TEST_LOOPBACK_PID=<pid>` verilirse işlem loopback'i yalnız o sürecin
//! sesini (DAHİL kipinde) yakalar — test kendi sessize alınmış ton sürecini dinler,
//! hoparlöre hiçbir şey çalınmaz.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::{FRAME, RATE, Ring};

/// Halka üst sınırı: 400 ms (systemAudio.ts: "Gecikme birikmesin: 400 ms'ten fazlası atılır").
pub const LOOPBACK_MAX: usize = RATE as usize * 2 / 5;

/// Cihazın kanal düzenini stereoya indirger: ön sol/sağ tam, merkez ve arkalar kısık
/// (loopback.cc `Downmix` ile aynı).
pub fn downmix_stereo(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    out.clear();
    if channels == 0 {
        return;
    }
    for frame in input.chunks_exact(channels) {
        let (mut left, mut right);
        if channels == 1 {
            left = frame[0];
            right = frame[0];
        } else {
            left = frame[0];
            right = frame[1];
            if channels >= 4 {
                // 5.1/7.1: merkez ve arka kanalları -3 dB ile karıştır.
                let center = if channels >= 3 { frame[2] * 0.707 } else { 0.0 };
                let rest: f32 = frame[4.min(channels)..].iter().sum::<f32>() * 0.5;
                left += center + rest * 0.707;
                right += center + rest * 0.707;
            }
        }
        out.push(left.clamp(-1.0, 1.0));
        out.push(right.clamp(-1.0, 1.0));
    }
}

/// Stereo (iç içe) → mono halkaya; üst sınırı aşan en eski örnekler atılır.
pub fn push_stereo(ring: &Ring, stereo: &[f32]) {
    let mut q = ring.lock().unwrap();
    q.extend(stereo.chunks_exact(2).map(|lr| (lr[0] + lr[1]) * 0.5));
    let excess = q.len().saturating_sub(LOOPBACK_MAX);
    q.drain(..excess);
}

/// 10 ms'lik mikrofon karesine bilgisayar sesini ekler (doyurarak). Halkada yeterli örnek
/// yoksa olan kadarı eklenir; ses kesilmez, yalnız o an eksik kalır. Eklenen örnek sayısı.
pub fn mix_into(pcm: &mut [i16], ring: &Ring) -> usize {
    let mut q = ring.lock().unwrap();
    let n = pcm.len().min(q.len());
    for slot in pcm.iter_mut().take(n) {
        let v = q.pop_front().unwrap_or(0.0);
        let sum = *slot as f32 + v.clamp(-1.0, 1.0) * i16::MAX as f32;
        *slot = sum.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    }
    n
}

/// Açık bilgisayar sesi yakalaması. Düşürülünce durur.
pub struct Loopback {
    pub ring: Ring,
    /// "işlem" (kendi sesimiz hariç), "işlem-test <pid>" ya da "uç nokta".
    pub mode: String,
    running: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    _endpoint: Option<EndpointStream>,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

/// cpal akışı iş parçacığına bağlıdır; motor iş parçacığında oluşturulup orada düşürülür.
struct EndpointStream(#[allow(dead_code)] cpal::Stream);

impl Loopback {
    /// Yakalamayı başlatır. Hata mesajı Electron'unkiyle aynı biçimde (Türkçe, kısa).
    pub fn start() -> Result<Self, String> {
        let ring: Ring = Arc::new(Mutex::new(VecDeque::new()));
        let test_pid = std::env::var("P2_TEST_LOOPBACK_PID").ok().and_then(|p| p.trim().parse::<u32>().ok());
        let running = Arc::new(AtomicBool::new(true));
        match process::spawn(ring.clone(), running.clone(), test_pid) {
            Ok(worker) => {
                let mode = match test_pid {
                    Some(pid) => format!("işlem-test {pid}"),
                    None => "işlem".into(),
                };
                Ok(Self { ring, mode, running, worker: Some(worker), _endpoint: None })
            }
            Err(process_error) => {
                if test_pid.is_some() {
                    // Testte uç nokta yedeğine düşülmez: gerçek hoparlör sesi yakalanmasın.
                    return Err(process_error);
                }
                match endpoint(ring.clone()) {
                    Ok(stream) => Ok(Self { ring, mode: "uç nokta".into(), running, worker: None, _endpoint: Some(EndpointStream(stream)) }),
                    Err(e) => Err(format!("{process_error}; uç nokta loopback de açılamadı ({e})")),
                }
            }
        }
    }

    pub fn mix_into(&self, pcm: &mut [i16]) -> usize {
        mix_into(pcm, &self.ring)
    }
}

/// Yedek: varsayılan çıkış aygıtında giriş akışı = WASAPI uç nokta loopback'i (cpal).
fn endpoint(ring: Ring) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("Varsayılan ses cihazı açılamadı")?;
    let cfg = device.default_output_config().map_err(|e| e.to_string())?;
    let config = cfg.config();
    let channels = config.channels as usize;
    let mut resampler = crate::audio::Linear::new(config.sample_rate.0, RATE);
    let mut stereo = Vec::with_capacity(4096);
    let mut out = Vec::with_capacity(4096);
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                downmix_stereo(data, channels, &mut stereo);
                out.clear();
                for lr in stereo.chunks_exact(2) {
                    resampler.push((lr[0] + lr[1]) * 0.5, |m| out.push(m));
                }
                let mut q = ring.lock().unwrap();
                q.extend(out.iter().copied());
                let excess = q.len().saturating_sub(LOOPBACK_MAX);
                q.drain(..excess);
            },
            |e| eprintln!("[ses] bilgisayar sesi hatası: {e}"),
            None,
        ),
        other => return Err(format!("desteklenmeyen örnek biçimi {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

/// Windows dışında işlem loopback'i yok (macOS: ScreenCaptureKit ses yakalama ayrı iş).
#[cfg(not(windows))]
mod process {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    pub fn spawn(_ring: super::Ring, _running: Arc<AtomicBool>, _test_pid: Option<u32>) -> Result<std::thread::JoinHandle<()>, String> {
        Err("bilgisayar sesi bu sistemde henüz paylaşılamıyor".into())
    }
}

#[cfg(windows)]
mod process {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{BLOB, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows::Win32::System::Variant::VT_BLOB;
    use windows::core::{HRESULT, IUnknown, Interface, implement};

    use crate::audio::Ring;

    const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Handler {
        done: Mutex<Option<mpsc::Sender<Result<IAudioClient, String>>>>,
    }

    impl IActivateAudioInterfaceCompletionHandler_Impl for Handler_Impl {
        fn ActivateCompleted(&self, operation: windows::core::Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
            let result = (|| -> Result<IAudioClient, String> {
                let op = operation.ok().map_err(|e| e.message())?;
                let mut hr = HRESULT(0);
                let mut unknown: Option<IUnknown> = None;
                unsafe { op.GetActivateResult(&mut hr, &mut unknown) }.map_err(|e| e.message())?;
                hr.ok().map_err(|_| format!("işlem loopback etkinleştirilemedi (0x{:08X})", hr.0 as u32))?;
                unknown.ok_or("işlem loopback yanıt vermedi".to_string())?.cast::<IAudioClient>().map_err(|e| e.message())
            })();
            if let Some(tx) = self.done.lock().unwrap().take() {
                let _ = tx.send(result);
            }
            Ok(())
        }
    }

    fn format() -> WAVEFORMATEX {
        WAVEFORMATEX { wFormatTag: WAVE_FORMAT_IEEE_FLOAT, nChannels: 2, nSamplesPerSec: 48_000, nAvgBytesPerSec: 48_000 * 8, nBlockAlign: 8, wBitsPerSample: 32, cbSize: 0 }
    }

    /// İşlem loopback'ini (COM MTA) kendi iş parçacığında açar; açılış sonucu dönene kadar bekler.
    pub fn spawn(ring: Ring, running: Arc<AtomicBool>, test_pid: Option<u32>) -> Result<std::thread::JoinHandle<()>, String> {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let stop = running.clone();
        let worker = std::thread::Builder::new()
            .name("p2-loopback".into())
            .spawn(move || {
                let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                match open(test_pid) {
                    Ok((client, capture, event)) => {
                        let _ = tx.send(Ok(()));
                        capture_loop(&capture, event, &ring, &running);
                        unsafe {
                            let _ = client.Stop();
                            let _ = CloseHandle(event);
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
                if com {
                    unsafe { CoUninitialize() };
                }
            })
            .map_err(|e| e.to_string())?;
        match rx.recv_timeout(Duration::from_secs(4)) {
            Ok(Ok(())) => Ok(worker),
            Ok(Err(e)) => {
                let _ = worker.join();
                Err(e)
            }
            Err(_) => {
                stop.store(false, Ordering::Relaxed);
                Err("işlem loopback yanıt vermedi".into())
            }
        }
    }

    fn open(test_pid: Option<u32>) -> Result<(IAudioClient, IAudioCaptureClient, HANDLE), String> {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS { ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, ..Default::default() };
        params.Anonymous.ProcessLoopbackParams = AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
            TargetProcessId: test_pid.unwrap_or_else(std::process::id),
            ProcessLoopbackMode: if test_pid.is_some() { PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE } else { PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE },
        };
        let mut prop = PROPVARIANT::default();
        unsafe {
            let inner = &mut *prop.Anonymous.Anonymous;
            inner.vt = VT_BLOB;
            inner.Anonymous.blob = BLOB { cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, pBlobData: &mut params as *mut _ as *mut u8 };
        }
        let (tx, rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Handler { done: Mutex::new(Some(tx)) }.into();
        let _op = unsafe { ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&prop), &handler) }.map_err(|_| "işlem loopback etkinleştirilemedi".to_string())?;
        // PROPVARIANT'ın blob'u yerel değişkeni gösterir: Drop'ta PropVariantClear çağrılmasın.
        std::mem::forget(prop);
        let client = rx.recv_timeout(Duration::from_secs(3)).map_err(|_| "işlem loopback yanıt vermedi".to_string())??;
        let fmt = format();
        unsafe {
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, 2_000_000, 0, &fmt, None)
                .map_err(|e| format!("işlem loopback Initialize hatası (0x{:08X})", e.code().0 as u32))?;
            let event = CreateEventW(None, false, false, None).map_err(|e| e.message())?;
            client.SetEventHandle(event).map_err(|_| "olay tanıtılamadı".to_string())?;
            let capture: IAudioCaptureClient = client.GetService().map_err(|e| e.message())?;
            client.Start().map_err(|_| "Ses akışı başlatılamadı".to_string())?;
            Ok((client, capture, event))
        }
    }

    fn capture_loop(capture: &IAudioCaptureClient, event: HANDLE, ring: &Ring, running: &AtomicBool) {
        let mut stereo: Vec<f32> = Vec::with_capacity(4096);
        while running.load(Ordering::Relaxed) {
            let _ = unsafe { WaitForSingleObject(event, 200) } == WAIT_OBJECT_0;
            loop {
                let packet = match unsafe { capture.GetNextPacketSize() } {
                    Ok(n) => n,
                    Err(_) => return,
                };
                if packet == 0 || !running.load(Ordering::Relaxed) {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }.is_err() {
                    return;
                }
                stereo.clear();
                if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                    stereo.resize(frames as usize * 2, 0.0);
                } else {
                    let samples = unsafe { std::slice::from_raw_parts(data as *const f32, frames as usize * 2) };
                    stereo.extend(samples.iter().map(|s| s.clamp(-1.0, 1.0)));
                }
                let _ = unsafe { capture.ReleaseBuffer(frames) };
                super::push_stereo(ring, &stereo);
            }
        }
    }
}

/// Testler için: bir halkadan 10 ms'lik karede ne kadar örnek alınabilir.
pub fn available(ring: &Ring) -> usize {
    ring.lock().unwrap().len().min(FRAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_like_native_module() {
        let mut out = Vec::new();
        downmix_stereo(&[0.5], 1, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
        downmix_stereo(&[0.1, 0.2, 0.3, 0.4], 2, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
        // 5.1: L R C LFE SL SR → merkez -3 dB, arkalar yarı × -3 dB; doyurma ±1.
        downmix_stereo(&[0.2, 0.1, 0.4, 0.9, 0.2, 0.2], 6, &mut out);
        let c = 0.4 * 0.707;
        let rest = 0.2 * 0.707;
        assert!((out[0] - (0.2 + c + rest)).abs() < 1e-5 && (out[1] - (0.1 + c + rest)).abs() < 1e-5, "{out:?}");
        downmix_stereo(&[1.0, 1.0, 1.0, 0.0], 4, &mut out);
        assert_eq!(out, vec![1.0, 1.0]);
    }

    #[test]
    fn mixer_adds_saturates_and_caps() {
        let ring: Ring = Arc::default();
        // Mikrofon karesi + bilgisayar sesi toplanır.
        push_stereo(&ring, &[0.25, 0.25].repeat(FRAME));
        let mut pcm = [1000i16; FRAME];
        assert_eq!(mix_into(&mut pcm, &ring), FRAME);
        let expect = 1000 + (0.25 * i16::MAX as f32) as i16;
        assert!(pcm.iter().all(|s| (*s - expect).abs() <= 1), "{}", pcm[0]);
        // Doyurma: taşma yerine sınırda kalır.
        push_stereo(&ring, &[1.0, 1.0].repeat(FRAME));
        let mut pcm = [30000i16; FRAME];
        mix_into(&mut pcm, &ring);
        assert!(pcm.iter().all(|s| *s == i16::MAX));
        // Halka boşken kare değişmez.
        let mut pcm = [7i16; FRAME];
        assert_eq!(mix_into(&mut pcm, &ring), 0);
        assert!(pcm.iter().all(|s| *s == 7));
        // Kısmi: yalnız olan kadar eklenir.
        push_stereo(&ring, &[0.5, 0.5].repeat(100));
        let mut pcm = [0i16; FRAME];
        assert_eq!(mix_into(&mut pcm, &ring), 100);
        assert!(pcm[99] > 16000 && pcm[100] == 0);
        // 400 ms sınırı: fazlası (en eski) atılır.
        push_stereo(&ring, &vec![0.1; (LOOPBACK_MAX + 960) * 2]);
        assert_eq!(ring.lock().unwrap().len(), LOOPBACK_MAX);
        assert_eq!(available(&ring), FRAME);
    }
}
