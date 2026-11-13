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
        // Zil desenleri: gelen arama 2,4 sn'de bir iki çift nota; giden arama 4 sn'de bir
        // 1,2 sn'lik yumuşak 440+480 Hz bekleme sesi.
        if let Some(ring) = self.ring {
            for (i, m) in mix.iter_mut().enumerate() {
                let t = (self.ring_t + i as u64) as f32 / RATE as f32;
                *m += match ring {
                    Ring::Incoming => {
                        let cycle = t % 2.4;
                        let mut v = 0.0;
                        for (freq, start) in [(880.0f32, 0.0f32), (1174.66, 0.16), (880.0, 0.5), (1174.66, 0.66)] {
                            let local = cycle - start;
                            if (0.0..0.34).contains(&local) {
                                let env = envelope((local * RATE as f32) as u64, (0.34 * RATE as f32) as u64);
                                v += (local * freq * std::f32::consts::TAU).sin() * env * 0.16;
                            }
                        }
                        v
                    }
                    Ring::Outgoing => {
                        let cycle = t % 4.0;
                        if cycle < 1.2 {
                            let fade = (cycle / 0.03).min(1.0).min((1.2 - cycle) / 0.03);
                            ((t * 440.0 * std::f32::consts::TAU).sin() + (t * 480.0 * std::f32::consts::TAU).sin()) * 0.04 * fade
                        } else {
                            0.0
                        }
                    }
                    Ring::None => 0.0,
                } * volume;
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
