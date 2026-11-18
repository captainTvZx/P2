//! Zil ve işaret sesleri — notifications.ts / CallOverlay.tsx'in WebAudio sesleri. Dosya
//! taşınmaz; sesler örnek örnek üretilir ve motorun karışımına eklenir (yankı giderici
//! referansına da girer, yani karşı tarafa sızmaz).

use crate::audio::{FRAME, RATE};

/// notifications.ts `CUES`: kısa, yumuşak iki-üç nota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Join,
    Leave,
    StreamStart,
    StreamStop,
    Message,
}

/// Zil: gelen arama (çalma sesi) ya da giden arama (karşı taraf çalıyor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring {
    None,
    Incoming,
    Outgoing,
}

impl Cue {
    /// (frekans Hz, başlangıç sn) ve kazanç.
    fn notes(self) -> (&'static [(f32, f32)], f32) {
        match self {
            Cue::Join => (&[(523.25, 0.0), (783.99, 0.085)], 1.0),
            Cue::Leave => (&[(783.99, 0.0), (523.25, 0.085)], 0.95),
            Cue::StreamStart => (&[(659.25, 0.0), (830.61, 0.075), (1046.5, 0.15)], 1.05),
            Cue::StreamStop => (&[(1046.5, 0.0), (830.61, 0.075), (659.25, 0.15)], 0.95),
            Cue::Message => (&[(880.0, 0.0), (1318.5, 0.09)], 1.0),
        }
    }
}

struct Voice {
    freq: f32,
    /// Başlamasına kalan örnek (negatifse çalıyor).
    delay: i64,
    age: u64,
    length: u64,
    level: f32,
}

#[derive(Default)]
pub struct Synth {
    voices: Vec<Voice>,
    ring: Option<Ring>,
    ring_t: u64,
}

/// Saldırı 14 ms, sonra üstel sönüm (CUES'taki exponentialRamp eğrisi).
fn envelope(age: u64, length: u64) -> f32 {
    let t = age as f32 / RATE as f32;
    let total = length as f32 / RATE as f32;
    if t < 0.014 {
        t / 0.014
    } else {
        let k = (1.0f32 / 0.0001).ln() / (total - 0.014).max(0.05);
        (-(t - 0.014) * k).exp()
    }
}

/// Marimba/çan benzeri tek tokmak vuruşu (t: vuruştan beri sn): 6 ms yumuşak saldırı, temel
/// sinüs üstel söner; marimbanın ~4× kısmisi ve hafif oktav kısa sürer (tını, sertlik değil).
fn mallet(t: f32, freq: f32, decay: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    let tau = std::f32::consts::TAU;
    let attack = (t / 0.006).min(1.0);
    let fundamental = (tau * freq * t).sin() * (-t / decay).exp();
    let octave = (tau * freq * 2.0 * t).sin() * (-t / (decay * 0.35)).exp() * 0.08;
    let partial = (tau * freq * 3.98 * t).sin() * (-t / (decay * 0.16)).exp() * 0.18;
    attack * (fundamental + octave + partial)
}

/// Zil deseni: (döngü sn, sönüm sn, kazanç, [(frekans, başlangıç sn, göreli seviye)]).
fn ring_pattern(ring: Ring) -> (f32, f32, f32, &'static [(f32, f32, f32)]) {
    match ring {
        // Giden arama ("karşı taraf çalıyor"): 3,2 sn'de bir sakin iki nota (do → mi), arkasından
        // aynı çiftin daha kısık yankısı. Düşük seviye: konuşmanın arkasında rahatsız etmez.
        Ring::Outgoing => (3.2, 0.42, 0.06, &[(523.25, 0.0, 1.0), (659.25, 0.2, 0.85), (523.25, 0.75, 0.5), (659.25, 0.95, 0.42)]),
        // Gelen arama: Join işaretini büyüten yükselen arpej (do-mi-sol-do) ve sol→do cevabı;
        // 2,6 sn'de bir. Duyulacak kadar belirgin ama tiz/kare dalga yok.
        Ring::Incoming => (2.6, 0.5, 0.065, &[(523.25, 0.0, 0.9), (659.25, 0.13, 0.9), (783.99, 0.26, 0.95), (1046.5, 0.39, 1.0), (783.99, 1.0, 0.6), (1046.5, 1.13, 0.65)]),
        Ring::None => (1.0, 1.0, 0.0, &[]),
    }
}

/// Zil örneği (t: zil başlangıcından beri sn). Önceki döngünün kuyruğu da eklenir (kesik olmasın).
pub fn ring_sample(ring: Ring, t: f32) -> f32 {
    let (period, decay, gain, notes) = ring_pattern(ring);
    if notes.is_empty() {
        return 0.0;
    }
    let cycle = t % period;
    let mut v = 0.0;
    for (freq, start, level) in notes {
        v += mallet(cycle - start, *freq, decay) * level;
        if t >= period {
            v += mallet(cycle + period - start, *freq, decay) * level;
        }
    }
    v * gain
}

impl Synth {
    pub fn cue(&mut self, cue: Cue) {
        let (notes, gain) = cue.notes();
        for (freq, start) in notes {
            self.voices.push(Voice { freq: *freq, delay: (*start * RATE as f32) as i64, age: 0, length: (0.26 * RATE as f32) as u64, level: 0.15 * gain });
        }
    }

    pub fn ring(&mut self, ring: Ring) {
        self.ring = (ring != Ring::None).then_some(ring);
        self.ring_t = 0;
    }

    pub fn ringing(&self) -> bool {
        self.ring.is_some()
    }

    /// 10 ms'lik karışıma ekler.
    pub fn render(&mut self, mix: &mut [f32; FRAME], volume: f32) {
        // Zil desenleri: işaret sesleriyle aynı paletten (do majör beşli: Join/Leave/Message)
        // yumuşak tokmak notaları — bkz. `ring_sample`.
        if let Some(ring) = self.ring {
            for (i, m) in mix.iter_mut().enumerate() {
                let t = (self.ring_t + i as u64) as f32 / RATE as f32;
                *m += ring_sample(ring, t) * volume;
            }
            self.ring_t += FRAME as u64;
        }

        for voice in &mut self.voices {
            for (i, m) in mix.iter_mut().enumerate() {
                let n = i as i64 - voice.delay;
                if n < 0 {
                    continue;
                }
                let age = voice.age;
                if age >= voice.length {
                    break;
                }
                voice.age += 1;
                let t = age as f32 / RATE as f32;
                let env = envelope(age, voice.length);
                // İki katman: sinüs taban + hafif kalın üçgen (marimba benzeri tını).
                let sine = (t * voice.freq * std::f32::consts::TAU).sin() * 0.9;
                let tri_phase = (t * voice.freq * 1.004) % 1.0;
                let tri = (4.0 * (tri_phase - 0.5).abs() - 1.0) * 0.28;
                *m += (sine + tri) * env * voice.level * volume;
            }
            voice.delay -= FRAME as i64;
        }
        self.voices.retain(|v| v.age < v.length);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_plays_and_ends() {
        let mut s = Synth::default();
        s.cue(Cue::Join);
        let mut energy = 0.0;
        for _ in 0..50 {
            let mut mix = [0.0; FRAME];
            s.render(&mut mix, 1.0);
            energy += mix.iter().map(|v| v * v).sum::<f32>();
        }
        assert!(energy > 0.1, "{energy}");
        assert!(s.voices.is_empty(), "işaret 0,5 sn içinde bitmeli");
    }

    /// Yeni zil: kırpılmaz (tepe < 0,25), işaret sesleri düzeyinde, döngü başı sessizden başlar
    /// (6 ms saldırı — "tık" yok) ve giden zil gelen zilden kısıktır.
    #[test]
    fn ring_is_soft_and_bounded() {
        let peak = |ring: Ring| (0..RATE as usize * 6).map(|i| ring_sample(ring, i as f32 / RATE as f32).abs()).fold(0.0f32, f32::max);
        let (inc, out) = (peak(Ring::Incoming), peak(Ring::Outgoing));
        assert!(inc > 0.08 && inc < 0.2, "gelen tepe {inc}");
        assert!(out > 0.03 && out < inc, "giden tepe {out}");
        assert!(ring_sample(Ring::Incoming, 0.0).abs() < 1e-3);
        assert_eq!(ring_sample(Ring::None, 0.5), 0.0);
    }

    #[test]
    fn ring_repeats_until_stopped() {
        let mut s = Synth::default();
        s.ring(Ring::Incoming);
        let mut mix = [0.0; FRAME];
        s.render(&mut mix, 1.0);
        assert!(s.ringing());
        s.ring(Ring::None);
        let mut mix = [0.0; FRAME];
        s.render(&mut mix, 1.0);
        assert!(mix.iter().all(|v| *v == 0.0));
    }
}
