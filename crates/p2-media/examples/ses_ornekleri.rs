//! Zil ve işaret seslerini WAV dosyasına yazar (ses aygıtı AÇILMAZ; yalnız `Synth` örnek üretir).
//! Kullanıcı kendi oynatıcısında dinlesin diye: varsayılan klasör `C:\p2-pg\sesler`.
//!
//!   cargo run -p p2-media --example ses_ornekleri [--release] [-- <klasör>]

use std::io::Write as _;

use p2_media::audio::{FRAME, RATE};
use p2_media::synth::Synth;
use p2_media::{Cue, Ring};

/// 16 bit PCM, tek kanal WAV.
fn write_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&out)
}

/// Motorun karışımındaki gibi 10 ms'lik karelerle üretir (`volume` = ayarlardaki zil sesi, varsayılan %70).
fn render(seconds: f32, setup: impl FnOnce(&mut Synth)) -> Vec<f32> {
    let mut synth = Synth::default();
    setup(&mut synth);
    let frames = (seconds * RATE as f32) as usize / FRAME;
    let mut out = Vec::with_capacity(frames * FRAME);
    for _ in 0..frames {
        let mut mix = [0f32; FRAME];
        synth.render(&mut mix, 0.7);
        out.extend_from_slice(&mix);
    }
    out
}

fn main() -> std::io::Result<()> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| r"C:\p2-pg\sesler".into()));
    std::fs::create_dir_all(&dir)?;
    let mut files: Vec<(String, Vec<f32>)> = vec![
        ("zil-giden.wav".into(), render(10.0, |s| s.ring(Ring::Outgoing))),
        ("zil-gelen.wav".into(), render(8.0, |s| s.ring(Ring::Incoming))),
    ];
    for (name, cue) in [("katilma", Cue::Join), ("ayrilma", Cue::Leave), ("mesaj", Cue::Message), ("yayin-basladi", Cue::StreamStart), ("yayin-bitti", Cue::StreamStop)] {
        files.push((format!("isaret-{name}.wav"), render(0.8, |s| s.cue(cue))));
    }
    // Hepsi art arda (karşılaştırma): işaretler, ardından giden ve gelen zil.
    let mut all = Vec::new();
    for (_, samples) in files.iter().skip(2).chain(files.iter().take(2)) {
        all.extend_from_slice(samples);
        all.extend(std::iter::repeat_n(0.0, RATE as usize / 3));
    }
    files.push(("hepsi-art-arda.wav".into(), all));
    for (name, samples) in &files {
        let path = dir.join(name);
        write_wav(&path, samples)?;
        let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        println!("{} ({:.1} sn, tepe {:.2})", path.display(), samples.len() as f32 / RATE as f32, peak);
    }
    Ok(())
}
