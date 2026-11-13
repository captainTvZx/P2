//! Ses aygıtları (cpal / WASAPI): mikrofon ve hoparlör. İçeride her şey 48 kHz mono;
//! aygıtın kendi hızı ve kanal sayısı doğrusal yeniden örneklemeyle çevrilir.
//!
//! Mikrofon akışı örnekleri `mic` halkasına iter, hoparlör akışı `speaker` halkasından çeker.
//! Halkalar motorun 10 ms'lik saatiyle (engine.rs) boşaltılır/doldurulur; aygıt saati ile
//! motor saati arasındaki kaymayı halkaların üst sınırı emer (taşan en eski örnek düşer).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

pub const RATE: u32 = 48_000;
/// 10 ms'lik kare (48 kHz mono).
pub const FRAME: usize = 480;
/// Halka üst sınırı: 200 ms. Fazlası gecikme demek; en eskisi atılır.
const RING_MAX: usize = RATE as usize / 5;

pub type Ring = Arc<Mutex<VecDeque<f32>>>;

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub name: String,
    pub default: bool,
}

fn list(input: bool) -> Vec<DeviceInfo> {
    // Testler: ses aygıt katmanına hiç dokunulmaz (listeleme dahil).
    if std::env::var_os("P2_NO_AUDIO_DEVICE").is_some() {
        return Vec::new();
    }
    let host = cpal::default_host();
    let default = if input { host.default_input_device() } else { host.default_output_device() }.and_then(|d| d.name().ok());
    let devices = if input { host.input_devices() } else { host.output_devices() };
    devices
        .map(|it| it.filter_map(|d| d.name().ok()).map(|name| DeviceInfo { default: Some(&name) == default.as_ref(), name }).collect())
        .unwrap_or_default()
}

pub fn input_devices() -> Vec<DeviceInfo> {
    list(true)
}

pub fn output_devices() -> Vec<DeviceInfo> {
    list(false)
}

fn find(input: bool, name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name.filter(|n| !n.is_empty() && *n != "default") {
        let devices = if input { host.input_devices() } else { host.output_devices() };
        if let Some(d) = devices.ok().and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(name))) {
            return Some(d);
        }
    }
    if input { host.default_input_device() } else { host.default_output_device() }
}

/// Doğrusal yeniden örnekleyici (akış hâlinde; kare sınırlarında süreklidir).
pub struct Linear {
    /// giriş hızı / çıkış hızı
    step: f64,
    pos: f64,
    prev: f32,
    cur: f32,
}

impl Linear {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, pos: 0.0, prev: 0.0, cur: 0.0 }
    }

    /// İtme yönü: bir giriş örneği ver, üretilen çıkış örnekleri `out`'a gider.
    pub fn push(&mut self, x: f32, mut out: impl FnMut(f32)) {
        self.prev = self.cur;
        self.cur = x;
        while self.pos < 1.0 {
            out(self.prev + (self.cur - self.prev) * self.pos as f32);
            self.pos += self.step;
        }
        self.pos -= 1.0;
    }

    /// Çekme yönü: bir çıkış örneği üret, gerektikçe `next` ile giriş al.
    pub fn pull(&mut self, mut next: impl FnMut() -> f32) -> f32 {
        while self.pos >= 1.0 {
            self.prev = self.cur;
            self.cur = next();
            self.pos -= 1.0;
        }
        let v = self.prev + (self.cur - self.prev) * self.pos as f32;
        self.pos += self.step;
        v
    }
}

fn push_capped(ring: &Ring, samples: impl IntoIterator<Item = f32>) {
    let mut q = ring.lock().unwrap();
    q.extend(samples);
    let excess = q.len().saturating_sub(RING_MAX);
    q.drain(..excess);
}

fn input_stream<T: SizedSample>(device: &cpal::Device, config: &cpal::StreamConfig, ring: Ring) -> Result<cpal::Stream, String>
where
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut resampler = Linear::new(config.sample_rate.0, RATE);
    let mut scratch = Vec::with_capacity(4096);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                scratch.clear();
                for frame in data.chunks(channels) {
                    let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
                    resampler.push(mono, |v| scratch.push(v));
                }
                push_capped(&ring, scratch.drain(..));
            },
            |e| eprintln!("[ses] mikrofon hatası: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

fn output_stream<T: SizedSample + FromSample<f32>>(device: &cpal::Device, config: &cpal::StreamConfig, ring: Ring) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let mut resampler = Linear::new(RATE, config.sample_rate.0);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut q = ring.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    let v = resampler.pull(|| q.pop_front().unwrap_or(0.0)).clamp(-1.0, 1.0);
                    let s = T::from_sample(v);
                    frame.iter_mut().for_each(|o| *o = s);
                }
            },
            |e| eprintln!("[ses] hoparlör hatası: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Açık aygıt akışları. Düşürülünce akışlar kapanır. (cpal akışları iş parçacığına bağlıdır:
/// motorun kendi iş parçacığında oluşturulur ve orada yaşar.)
pub struct AudioIo {
    _input: Option<cpal::Stream>,
    _output: Option<cpal::Stream>,
    pub mic: Ring,
    pub speaker: Ring,
    pub input_name: Option<String>,
    pub output_name: Option<String>,
}

impl AudioIo {
    /// `input`/`output`: aygıt adı ya da `None` (varsayılan). `use_mic=false` mikrofonu hiç
    /// açmaz (test sesi), `use_speaker=false` hoparlörü açmaz (sessiz test).
    pub fn open(input: Option<&str>, output: Option<&str>, use_mic: bool, use_speaker: bool) -> (Self, Vec<String>) {
        let mic: Ring = Arc::default();
        let speaker: Ring = Arc::default();
        let mut errors = Vec::new();
        let mut input_name = None;
        let mut output_name = None;

        let input_stream = use_mic.then(|| find(true, input)).flatten().and_then(|device| {
            input_name = device.name().ok();
            let built = device.default_input_config().map_err(|e| e.to_string()).and_then(|cfg| {
                let config = cfg.config();
                match cfg.sample_format() {
                    cpal::SampleFormat::F32 => input_stream::<f32>(&device, &config, mic.clone()),
                    cpal::SampleFormat::I16 => input_stream::<i16>(&device, &config, mic.clone()),
                    cpal::SampleFormat::U16 => input_stream::<u16>(&device, &config, mic.clone()),
                    cpal::SampleFormat::I32 => input_stream::<i32>(&device, &config, mic.clone()),
                    other => Err(format!("desteklenmeyen örnek biçimi {other:?}")),
                }
            });
            match built.and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => Some(s),
                Err(e) => {
                    errors.push(format!("Mikrofon açılamadı: {e}"));
                    None
                }
            }
        });
        if use_mic && input_stream.is_none() && errors.is_empty() {
            errors.push("Mikrofon bulunamadı.".into());
        }

        let output_stream = use_speaker.then(|| find(false, output)).flatten().and_then(|device| {
            output_name = device.name().ok();
            let built = device.default_output_config().map_err(|e| e.to_string()).and_then(|cfg| {
                let config = cfg.config();
                match cfg.sample_format() {
                    cpal::SampleFormat::F32 => output_stream::<f32>(&device, &config, speaker.clone()),
                    cpal::SampleFormat::I16 => output_stream::<i16>(&device, &config, speaker.clone()),
                    cpal::SampleFormat::U16 => output_stream::<u16>(&device, &config, speaker.clone()),
                    cpal::SampleFormat::I32 => output_stream::<i32>(&device, &config, speaker.clone()),
                    other => Err(format!("desteklenmeyen örnek biçimi {other:?}")),
                }
            });
            match built.and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => Some(s),
                Err(e) => {
                    errors.push(format!("Hoparlör açılamadı: {e}"));
                    None
                }
            }
        });

        (Self { _input: input_stream, _output: output_stream, mic, speaker, input_name, output_name }, errors)
    }

    /// 10 ms mikrofon karesi; yeterli örnek yoksa eksik kısım sessizlik.
    pub fn take_mic(&self, out: &mut [f32; FRAME]) -> bool {
        let mut q = self.mic.lock().unwrap();
        let have = q.len() >= FRAME;
        for slot in out.iter_mut() {
            *slot = q.pop_front().unwrap_or(0.0);
        }
        have
    }

    pub fn play(&self, frame: &[f32]) {
        if self._output.is_some() {
            push_capped(&self.speaker, frame.iter().copied());
        }
    }
}

/// i16 ↔ f32 yardımcıları ve seviye ölçümü.
pub fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// RMS seviyesi dBFS (sessizlik −100).
pub fn level_db(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64 / i16::MAX as f64).powi(2)).sum();
    let rms = (sum / samples.len() as f64).sqrt();
    if rms <= 1e-5 { -100.0 } else { (20.0 * rms.log10()) as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_keeps_rate() {
        // 44,1 kHz → 48 kHz: 441 giriş ≈ 480 çıkış.
        let mut r = Linear::new(44_100, 48_000);
        let mut n = 0;
        for i in 0..44_100 {
            r.push((i as f32 * 0.01).sin(), |_| n += 1);
        }
        assert!((47_990..=48_010).contains(&n), "{n}");
        // Çekme yönü: 48 kHz → 44,1 kHz.
        let mut r = Linear::new(48_000, 44_100);
        let mut taken = 0;
        for _ in 0..44_100 {
            r.pull(|| {
                taken += 1;
                0.0
            });
        }
        assert!((47_990..=48_010).contains(&taken), "{taken}");
    }

    #[test]
    fn levels() {
        assert_eq!(level_db(&[0; 480]), -100.0);
        let full: Vec<i16> = (0..480).map(|i| if i % 2 == 0 { i16::MAX } else { -i16::MAX }).collect();
        assert!(level_db(&full).abs() < 0.1);
    }
}
