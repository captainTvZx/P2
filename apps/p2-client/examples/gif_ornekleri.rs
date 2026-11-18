//! Keşfet / profil demoları için yordamsal hareketli GIF'ler (dış görsel yok, her şey burada çizilir).
//!
//!   cargo run --release -p p2-client --example gif_ornekleri -- <çıkış klasörü>
//!
//! Afişler 1280×400 (sentwave ızgara, kutup ışıkları, katmanlı dağlar, piksel şehir, piksel plazma,
//! gün batımı dalgaları); ikon / profil fotoğrafları 256×256 (atan halkalar, dönen altıgen, yörünge,
//! lav lambası, piksel baş harfler). Her kare en çok 256 renkli sabit bir paletten çizilir: GIF
//! kodlayıcısı kareyi nicemlemeden birebir yazar, düz alanlar LZW ile iyi sıkışır. Döngüler dikişsizdir
//! (her hareket döngü süresinde tam tur atar). `tools/gif-seed.mjs` bu dosyaları yükler.

use std::f32::consts::TAU;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage};

type Rgb = [u8; 3];

/// En çok 256 renklik palet: art arda eklenen renk rampaları.
struct Palette {
    colors: Vec<Rgb>,
}

/// Paletteki bir rampa: `at(t)` 0..1 arası tonu palet sırasına çevirir.
#[derive(Clone, Copy)]
struct Ramp {
    start: u8,
    len: u8,
}

impl Ramp {
    fn at(self, t: f32) -> u8 {
        let k = (t.clamp(0.0, 1.0) * (self.len - 1) as f32).round() as u8;
        self.start + k
    }
}

impl Palette {
    fn new() -> Self {
        Self { colors: Vec::new() }
    }

    /// Duraklar arasında doğrusal geçişli `n` tonluk rampa.
    fn ramp(&mut self, stops: &[(f32, u32)], n: usize) -> Ramp {
        let start = self.colors.len();
        assert!(start + n <= 256, "palet 256 rengi aşıyor");
        for i in 0..n {
            let t = if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
            let j = stops.iter().rposition(|s| s.0 <= t).unwrap_or(0).min(stops.len() - 2);
            let (a, b) = (stops[j], stops[j + 1]);
            let k = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
            let (ca, cb) = (hex(a.1), hex(b.1));
            self.colors.push([0, 1, 2].map(|c| (ca[c] as f32 + (cb[c] as f32 - ca[c] as f32) * k).round() as u8));
        }
        Ramp { start: start as u8, len: n as u8 }
    }

    fn solid(&mut self, c: u32) -> u8 {
        self.ramp(&[(0.0, c), (1.0, c)], 1).start
    }
}

fn hex(c: u32) -> Rgb {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// Palet sıralı tuval.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![0; w * h] }
    }
    fn set(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }
    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: u8) {
        for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                if (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= r {
                    self.set(x, y, c);
                }
            }
        }
    }
    /// Düşük çözünürlüklü tuvali `k` kat büyütür (piksel sanatı).
    fn upscale(&self, k: usize) -> Canvas {
        let mut out = Canvas::new(self.w * k, self.h * k);
        for y in 0..out.h {
            for x in 0..out.w {
                out.px[y * out.w + x] = self.px[(y / k) * self.w + x / k];
            }
        }
        out
    }
    fn rgba(&self, pal: &Palette) -> RgbaImage {
        let mut img = RgbaImage::new(self.w as u32, self.h as u32);
        for (i, p) in img.pixels_mut().enumerate() {
            let c = pal.colors[self.px[i] as usize];
            p.0 = [c[0], c[1], c[2], 255];
        }
        img
    }
}

/// Tekrarlanabilir sözde rastgele (xorshift).
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn write_gif(path: &Path, frames: Vec<Canvas>, pal: &Palette, delay_ms: u32) {
    let file = BufWriter::new(File::create(path).expect("dosya"));
    let mut enc = GifEncoder::new_with_speed(file, 10);
    enc.set_repeat(Repeat::Infinite).expect("döngü");
    for f in &frames {
        enc.encode_frame(Frame::from_parts(f.rgba(pal), 0, 0, Delay::from_numer_denom_ms(delay_ms, 1))).expect("kare");
    }
    drop(enc);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("{} ({} kare, {:.2} MB)", path.display(), frames.len(), size as f64 / 1048576.0);
}

// ================================================================= afişler (1280×400)

const BW: usize = 1280;
const BH: usize = 400;

/// Sentwave: mor-pembe gökyüzü, şeritli güneş, dağ silüeti, akan neon ızgara, yanıp sönen yıldızlar.
fn synthwave(n: usize) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let sky = pal.ramp(&[(0.0, 0x0d0221), (0.55, 0x3a0a5e), (0.85, 0x9b1d74), (1.0, 0xff4f7b)], 64);
    let sun = pal.ramp(&[(0.0, 0xffe259), (0.5, 0xffa34d), (1.0, 0xff3d81)], 48);
    let floor = pal.ramp(&[(0.0, 0x1a0633), (1.0, 0x07010f)], 24);
    let grid = pal.ramp(&[(0.0, 0x3b1d6b), (1.0, 0x2de2e6)], 24);
    let mountain = pal.ramp(&[(0.0, 0x2a0b4a), (1.0, 0x12052a)], 16);
    let ridge = pal.solid(0xff4fd8);
    let star = pal.ramp(&[(0.0, 0x6a4b8f), (1.0, 0xffffff)], 8);
    let horizon = 250.0;
    let mut rng = Rng(7);
    let stars: Vec<(i32, i32, f32)> = (0..90).map(|_| ((rng.next() * BW as f32) as i32, (rng.next() * 190.) as i32, rng.next())).collect();
    let peaks: Vec<(f32, f32, f32)> = (0..5).map(|k| (1.0 + k as f32 * 1.7, rng.next() * 6.0, 26.0 / (1.0 + k as f32))).collect();
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(BW, BH);
            for y in 0..horizon as usize {
                let idx = sky.at(y as f32 / horizon);
                c.px[y * BW..(y + 1) * BW].fill(idx);
            }
            for (x, y, ph) in &stars {
                let tw = 0.5 + 0.5 * (TAU * (t * 2.0 + ph)).sin();
                c.set(*x, *y, star.at(tw));
            }
            // Güneş: alt yarısında aşağı kayan boşluk şeritleri.
            let (sx, sy, sr) = (640.0f32, 200.0f32, 120.0f32);
            for y in (sy - sr) as i32..horizon as i32 {
                let fy = y as f32;
                let rel = (fy - (sy - sr)) / (2.0 * sr);
                if fy > sy - 10.0 {
                    let band = ((fy - sy) / 14.0 + t * 1.0).rem_euclid(1.0);
                    let gap = 0.12 + 0.5 * ((fy - sy) / (horizon - sy)).max(0.0);
                    if band < gap {
                        continue;
                    }
                }
                let half = (sr * sr - (fy - sy) * (fy - sy)).max(0.0).sqrt();
                for x in (sx - half) as i32..(sx + half) as i32 {
                    c.set(x, y, sun.at(rel));
                }
            }
            // Dağlar (sabit) ve ışıklı sırt çizgisi.
            for x in 0..BW {
                let fx = x as f32;
                let mut top = horizon - 8.0;
                for (fr, ph, a) in &peaks {
                    top -= ((fx / BW as f32 * TAU * fr + ph).sin()).abs() * a;
                }
                let dist = ((fx - 640.0).abs() / 640.0).powf(0.6);
                let top = horizon - (horizon - top) * (0.35 + 0.65 * dist);
                for y in top as usize..horizon as usize {
                    c.px[y * BW + x] = mountain.at((y as f32 - top) / (horizon - top + 1.0));
                }
                c.set(x as i32, top as i32, ridge);
            }
            // Zemin ve ızgara: yatay çizgiler izleyiciye doğru akar, dikeyler ufukta birleşir.
            for y in horizon as usize..BH {
                let d = (y as f32 - horizon) / (BH as f32 - horizon);
                c.px[y * BW..(y + 1) * BW].fill(floor.at(d));
            }
            let rows = 12;
            for k in 0..rows {
                let z = ((k as f32 + t) / rows as f32).powf(2.2);
                let y = horizon + z * (BH as f32 - horizon);
                let idx = grid.at(0.25 + z * 0.75);
                c.rect(0, y as i32, BW as i32, if z > 0.5 { 2 } else { 1 }, idx);
            }
            for k in -24..=24 {
                for y in horizon as usize..BH {
                    let d = (y as f32 - horizon) / (BH as f32 - horizon);
                    // Ufka yakın çizgiler birbirine çok yaklaşınca (moire) çizilmez.
                    if 22.0 * (0.08 + d * 4.0) < 6.0 {
                        continue;
                    }
                    let x = 640.0 + k as f32 * 22.0 * (0.08 + d * 4.0);
                    c.set(x as i32, y as i32, grid.at(0.2 + d * 0.8));
                }
            }
            c.rect(0, horizon as i32, BW as i32, 2, grid.at(1.0));
            c
        })
        .collect();
    (frames, pal)
}

/// Kutup ışıkları: gece gökyüzünde dalgalanan yeşil-mor perdeler, yıldızlar, çam silüetleri.
fn aurora(n: usize) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let sky = pal.ramp(&[(0.0, 0x020617), (0.7, 0x0b1e3a), (1.0, 0x123456)], 40);
    let light = pal.ramp(&[(0.0, 0x0b2a3a), (0.35, 0x0f766e), (0.65, 0x34d399), (0.85, 0xa7f3d0), (1.0, 0xe9d5ff)], 64);
    let violet = pal.ramp(&[(0.0, 0x1e1b4b), (1.0, 0xa78bfa)], 24);
    let star = pal.ramp(&[(0.0, 0x334155), (1.0, 0xffffff)], 8);
    let hill = pal.solid(0x050b14);
    let tree = pal.solid(0x02060c);
    let snow = pal.ramp(&[(0.0, 0x1e293b), (1.0, 0x64748b)], 8);
    let mut rng = Rng(42);
    let stars: Vec<(i32, i32, f32)> = (0..140).map(|_| ((rng.next() * BW as f32) as i32, (rng.next() * 300.) as i32, rng.next())).collect();
    let trees: Vec<(f32, f32)> = (0..70).map(|_| (rng.next() * BW as f32, 30.0 + rng.next() * 60.0)).collect();
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let a = TAU * t;
            let mut c = Canvas::new(BW, BH);
            for y in 0..BH {
                c.px[y * BW..(y + 1) * BW].fill(sky.at(y as f32 / BH as f32));
            }
            for (x, y, ph) in &stars {
                c.set(*x, *y, star.at(0.5 + 0.5 * (TAU * ph + a * 2.0).sin()));
            }
            for x in 0..BW {
                let fx = x as f32 / BW as f32;
                let base = 150.0 + 45.0 * (fx * TAU * 1.0 + a).sin() + 22.0 * (fx * TAU * 3.0 - a * 2.0).sin();
                let rays = 0.55 + 0.25 * (fx * TAU * 23.0 + a * 3.0).sin() + 0.2 * (fx * TAU * 41.0 - a * 2.0).sin();
                let glow = (0.6 + 0.4 * (fx * TAU * 2.0 - a).sin()) * rays;
                let tall = 110.0 + 40.0 * (fx * TAU * 2.0 + a).cos();
                for y in 0..BH {
                    let dy = base - y as f32;
                    let v = if dy >= 0.0 { (1.0 - dy / tall).max(0.0).powf(1.6) } else { (1.0 + dy / 10.0).max(0.0) };
                    let v = v * glow;
                    if v > 0.08 {
                        let idx = if dy > tall * 0.65 { violet.at(v * 1.4) } else { light.at(v) };
                        c.px[y * BW + x] = idx;
                    }
                }
            }
            // Karlı tepe + çam silüetleri (sabit).
            for x in 0..BW {
                let fx = x as f32 / BW as f32;
                let top = 330.0 - 22.0 * (fx * TAU * 1.5 + 0.7).sin() - 10.0 * (fx * TAU * 4.0).sin();
                for y in top as usize..BH {
                    c.px[y * BW + x] = if (y as f32) < top + 3.0 { snow.at(0.8) } else { hill };
                }
            }
            for (tx, th) in &trees {
                let fx = tx / BW as f32;
                let ground = 330.0 - 22.0 * (fx * TAU * 1.5 + 0.7).sin() - 10.0 * (fx * TAU * 4.0).sin() + 6.0;
                for k in 0..*th as i32 {
                    let half = ((th - k as f32) / th * th * 0.22) as i32;
                    c.rect(*tx as i32 - half, ground as i32 - k, half * 2 + 1, 1, tree);
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

/// Katmanlı dağlar: dört sıradağ farklı hızlarda kayar (paralaks), alacakaranlık gökyüzü, güneş.
fn parallax(n: usize) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let sky = pal.ramp(&[(0.0, 0x1e3a8a), (0.5, 0x6d5bd0), (0.8, 0xf59e8b), (1.0, 0xfde68a)], 64);
    let sun = pal.ramp(&[(0.0, 0xfff7d6), (1.0, 0xffd08a)], 8);
    let layers = [0x7c6fb8u32, 0x56489a, 0x352c6e, 0x1b1640].map(|c| pal.solid(c));
    let mist = pal.ramp(&[(0.0, 0x8f84c9), (1.0, 0x7c6fb8)], 8);
    let bird = pal.solid(0x1b1640);
    // Katman k: frekanslar `step[k]`in katı → döngüde W/step kayınca dikişsiz.
    let step = [6.0f32, 4.0, 3.0, 2.0];
    let mut rng = Rng(99);
    let waves: Vec<Vec<(f32, f32, f32)>> = (0..4)
        .map(|k| (1..=3).map(|m| (step[k] * m as f32, rng.next() * TAU, 1.0 / (m * m) as f32)).collect())
        .collect();
    let base = [215.0f32, 260.0, 310.0, 365.0];
    let amp = [80.0f32, 75.0, 70.0, 60.0];
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(BW, BH);
            for y in 0..BH {
                c.px[y * BW..(y + 1) * BW].fill(sky.at(y as f32 / 300.0));
            }
            c.disc(900.0, 150.0, 70.0, sun.at(1.0));
            c.disc(900.0, 150.0, 58.0, sun.at(0.0));
            for k in 0..4 {
                let shift = t * BW as f32 / step[k];
                for x in 0..BW {
                    let fx = (x as f32 + shift) / BW as f32;
                    let (mut h, mut sum) = (0.0, 0.0);
                    for (fr, ph, w) in &waves[k] {
                        h += (fx * TAU * fr + ph).sin() * w;
                        sum += w;
                    }
                    let top = base[k] - (h / sum * 0.5 + 0.5) * amp[k];
                    for y in top.max(0.0) as usize..BH {
                        c.px[y * BW + x] = if k == 0 && (y as f32) > top + 30.0 { mist.at(((y as f32 - top - 30.0) / 60.0).min(1.0)) } else { layers[k] };
                    }
                }
            }
            // Süzülen kuşlar (ekranda tam tur).
            for b in 0..4 {
                let bx = ((t + b as f32 * 0.27) % 1.0) * (BW as f32 + 80.0) - 40.0;
                let by = 90.0 + b as f32 * 22.0 + 6.0 * (TAU * (t * 2.0 + b as f32 * 0.3)).sin();
                let flap = if ((t * 8.0 + b as f32 * 0.5) as i32) % 2 == 0 { -3 } else { 2 };
                for d in 0..7 {
                    c.set(bx as i32 - d, by as i32 + if d > 2 { flap } else { 0 } * (d - 2).max(0) / 2, bird);
                    c.set(bx as i32 + d, by as i32 + if d > 2 { flap } else { 0 } * (d - 2).max(0) / 2, bird);
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

/// Piksel şehir (320×100, 4× büyütülmüş): yanıp sönen pencereler, kayan bulutlar, ay, yıldızlar.
fn pixel_city(n: usize) -> (Vec<Canvas>, Palette) {
    let (w, h) = (BW / 4, BH / 4);
    let mut pal = Palette::new();
    let sky = pal.ramp(&[(0.0, 0x140c2e), (0.6, 0x3b1f5c), (1.0, 0xc2477a)], 24);
    let moon = pal.solid(0xfff1c1);
    let moon_shade = pal.solid(0xe8d29a);
    let star = pal.ramp(&[(0.0, 0x4a3570), (1.0, 0xffffff)], 4);
    let cloud = pal.ramp(&[(0.0, 0x5b3a7a), (1.0, 0x8a5c9e)], 3);
    let build = [0x23163d, 0x1a1030, 0x120a22].map(|c| pal.solid(c));
    let win_on = [0xffd166, 0xff9f68, 0x7ee8fa].map(|c| pal.solid(c));
    let win_off = pal.solid(0x2c1d4a);
    let sign = pal.ramp(&[(0.0, 0x5a1446), (1.0, 0xff4fd8)], 6);
    let mut rng = Rng(2024);
    let stars: Vec<(i32, i32, f32)> = (0..50).map(|_| ((rng.next() * w as f32) as i32, (rng.next() * 50.) as i32, rng.next())).collect();
    // Binalar: (x, genişlik, yükseklik, katman).
    let mut blds = Vec::new();
    for layer in 0..3 {
        let mut x = -(rng.next() * 10.0) as i32;
        while x < w as i32 {
            let bw = 10 + (rng.next() * 18.0) as i32;
            let bh = [40, 30, 20][layer] + (rng.next() * [30.0, 26.0, 18.0][layer]) as i32;
            blds.push((x, bw, bh, layer));
            x += bw + (rng.next() * 4.0) as i32;
        }
    }
    let windows: Vec<(usize, i32, i32, u8, f32, f32)> = blds
        .iter()
        .enumerate()
        .flat_map(|(i, &(x, bw, bh, _))| {
            let mut out = Vec::new();
            let mut r = Rng(1000 + i as u32 * 7);
            for wy in (h as i32 - bh + 3..h as i32 - 2).step_by(4) {
                for wx in (x + 2..x + bw - 2).step_by(3) {
                    out.push((i, wx, wy, (r.next() * 3.0) as u8, r.next(), 0.25 + r.next() * 0.6));
                }
            }
            out
        })
        .collect();
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(w, h);
            for y in 0..h {
                c.px[y * w..(y + 1) * w].fill(sky.at(y as f32 / h as f32));
            }
            for (x, y, ph) in &stars {
                c.set(*x, *y, star.at(0.5 + 0.5 * (TAU * (ph + t * 2.0)).sin()));
            }
            c.disc(250.0, 20.0, 9.0, moon);
            c.disc(253.0, 18.0, 3.0, moon_shade);
            c.disc(246.0, 24.0, 2.0, moon_shade);
            for k in 0..3 {
                let cx = ((t + k as f32 / 3.0) % 1.0) * (w as f32 + 60.0) - 30.0;
                let cy = 14.0 + k as f32 * 9.0;
                for (dx, dy, r) in [(0.0, 0.0, 5.0), (6.0, -2.0, 6.0), (13.0, 0.0, 5.0), (6.0, 2.0, 5.0)] {
                    c.disc(cx + dx, cy + dy, r, cloud.at(k as f32 / 2.0));
                }
            }
            for (b, &(x, bw, bh, layer)) in blds.iter().enumerate() {
                c.rect(x, h as i32 - bh, bw, bh, build[layer]);
                if layer == 0 {
                    continue;
                }
                for &(_, wx, wy, hue, ph, duty) in windows.iter().filter(|w| w.0 == b) {
                    let on = ((ph + t * 2.0) % 1.0) < duty;
                    c.rect(wx, wy, 2, 2, if on { win_on[hue as usize] } else { win_off });
                }
            }
            // Neon tabela: kayan parlaklık.
            for k in 0..18 {
                let v = 0.5 + 0.5 * (TAU * (t * 2.0 - k as f32 / 18.0)).sin();
                c.rect(140 + k, 46, 1, 3, sign.at(v));
            }
            c.upscale(4)
        })
        .collect();
    (frames, pal)
}

/// Piksel plazma (320×100, 4×): dönen renk çarkı paletiyle klasik demo sahnesi efekti.
fn plasma(n: usize) -> (Vec<Canvas>, Palette) {
    let (w, h) = (BW / 4, BH / 4);
    let mut pal = Palette::new();
    let wheel = pal.ramp(&[(0.0, 0x0a1220), (0.2, 0x19d3c5), (0.4, 0x2c7bff), (0.6, 0x7b5cff), (0.8, 0xf472b6), (1.0, 0x0a1220)], 96);
    let frames = (0..n)
        .map(|f| {
            let a = TAU * f as f32 / n as f32;
            let mut c = Canvas::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let (fx, fy) = (x as f32 / 40.0, y as f32 / 40.0);
                    let v = (fx * 1.3 + a).sin() + (fy * 2.1 - a).sin() + ((fx + fy) * 0.9 + a).sin() + ((fx - 4.0).hypot(fy - 1.2) * 1.7 - a).sin();
                    let k = (v / 8.0 + 0.5 + f as f32 / n as f32).rem_euclid(1.0);
                    c.px[y * w + x] = wheel.at(k);
                }
            }
            c.upscale(4)
        })
        .collect();
    (frames, pal)
}

/// Gün batımında okyanus: ışıltılı güneş yolu ve farklı hızlarda akan dalga katmanları.
fn waves(n: usize) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let sky = pal.ramp(&[(0.0, 0x0f172a), (0.45, 0x4c1d95), (0.8, 0xf97316), (1.0, 0xfde047)], 64);
    let sun = pal.solid(0xfff3c4);
    let sea = [0x1e3a8a, 0x1e2f6e, 0x172554, 0x0f1a3d].map(|c| pal.solid(c));
    let foam = pal.ramp(&[(0.0, 0x3b5bb5), (1.0, 0xc7d2fe)], 6);
    let glint = pal.ramp(&[(0.0, 0xf59e0b), (1.0, 0xfff7d6)], 6);
    let horizon = 230.0;
    let step = [8.0f32, 4.0, 2.0, 1.0];
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(BW, BH);
            for y in 0..horizon as usize {
                c.px[y * BW..(y + 1) * BW].fill(sky.at(y as f32 / horizon));
            }
            c.disc(640.0, horizon - 10.0, 64.0, sun);
            for y in horizon as usize..BH {
                c.px[y * BW..(y + 1) * BW].fill(sea[0]);
            }
            for k in 0..4 {
                let base = horizon + 20.0 + k as f32 * 42.0;
                let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
                let shift = dir * t * BW as f32 / step[k];
                for x in 0..BW {
                    let fx = (x as f32 + shift) / BW as f32 * TAU * step[k];
                    let top = base - 8.0 * (fx * 3.0).sin() - 4.0 * (fx * 7.0 + 1.3).sin();
                    for y in top.max(horizon) as usize..BH {
                        c.px[y * BW + x] = sea[k];
                    }
                    let crest = (fx * 3.0).sin();
                    if crest > 0.55 {
                        c.set(x as i32, top as i32, foam.at((crest - 0.55) / 0.45));
                    }
                }
            }
            // Güneş yolu: yatay ışıltı çizgileri yanıp söner.
            let mut rng = Rng(5);
            for i in 0..140 {
                let d = rng.next();
                let y = horizon + 4.0 + d * (BH as f32 - horizon - 8.0);
                let spread = 30.0 + d * 260.0;
                let x = 640.0 + (rng.next() - 0.5) * 2.0 * spread;
                let len = 6.0 + rng.next() * 26.0 * (0.4 + d);
                let ph = rng.next();
                let v = (TAU * (t * 2.0 + ph)).sin();
                if v > -0.2 || i % 5 == 0 {
                    c.rect((x - len / 2.0) as i32, y as i32, len as i32, 2, glint.at(0.5 + 0.5 * v));
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

// ================================================================= ikon / profil (256×256)

const IS: usize = 256;

/// Arka plan: köşeden köşeye koyu gradyan.
fn icon_bg(c: &mut Canvas, bg: Ramp) {
    for y in 0..IS {
        for x in 0..IS {
            c.px[y * IS + x] = bg.at((x + y) as f32 / (2 * IS) as f32);
        }
    }
}

/// Atan halkalar: merkezden dışarı yayılan, sönen halkalar ve parlayan çekirdek.
fn pulse(n: usize, colors: [u32; 3]) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let bg = pal.ramp(&[(0.0, 0x0b1020), (1.0, 0x05070f)], 32);
    let ring = pal.ramp(&[(0.0, 0x0b1020), (0.5, colors[0]), (1.0, colors[1])], 48);
    let core = pal.ramp(&[(0.0, colors[1]), (1.0, colors[2])], 24);
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(IS, IS);
            icon_bg(&mut c, bg);
            for y in 0..IS {
                for x in 0..IS {
                    let d = (x as f32 - 127.5).hypot(y as f32 - 127.5) / 128.0;
                    for k in 0..3 {
                        let r = (k as f32 / 3.0 + t) % 1.0;
                        let w = (1.0 - (d - r).abs() / 0.045).max(0.0);
                        if w > 0.0 {
                            let v = w * (1.0 - r).powf(0.7);
                            if v > 0.05 {
                                c.px[y * IS + x] = ring.at(v);
                            }
                        }
                    }
                    let beat = 0.5 + 0.5 * (TAU * t * 2.0).cos();
                    let cr = 0.2 + 0.03 * beat;
                    if d < cr {
                        c.px[y * IS + x] = core.at(d / cr);
                    }
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

/// Dönen altıgen çerçeve + ters yönde dönen üçgen, açıya göre renk geçişi.
fn hexagon(n: usize, colors: [u32; 3]) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let bg = pal.ramp(&[(0.0, 0x111827), (1.0, 0x030712)], 32);
    let hue = pal.ramp(&[(0.0, colors[0]), (0.33, colors[1]), (0.66, colors[2]), (1.0, colors[0])], 96);
    let inner = pal.ramp(&[(0.0, 0x1f2937), (1.0, 0x374151)], 16);
    // Düzgün çokgen içinde mi: kenar mesafesi (iç çember yarıçapına göre).
    let poly = |x: f32, y: f32, sides: f32, rot: f32| -> f32 {
        let a = y.atan2(x) - rot;
        let seg = TAU / sides;
        let local = (a.rem_euclid(seg)) - seg / 2.0;
        x.hypot(y) * local.cos() / (seg / 2.0).cos()
    };
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(IS, IS);
            icon_bg(&mut c, bg);
            let rot = TAU * t / 6.0;
            for y in 0..IS {
                for x in 0..IS {
                    let (fx, fy) = (x as f32 - 127.5, y as f32 - 127.5);
                    let hx = poly(fx, fy, 6.0, rot);
                    let ang = (fy.atan2(fx) / TAU + 0.5 + t).rem_euclid(1.0);
                    if (88.0..104.0).contains(&hx) {
                        c.px[y * IS + x] = hue.at(ang);
                    } else if hx < 88.0 {
                        let tr = poly(fx, fy, 3.0, -TAU * t / 3.0 + TAU / 4.0);
                        c.px[y * IS + x] = if tr < 44.0 { hue.at((ang + 0.5).rem_euclid(1.0)) } else { inner.at(hx / 88.0) };
                    }
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

/// Yörünge: parlayan gezegen, eğik yörüngelerde dönen üç uydu ve izleri.
fn orbit(n: usize, colors: [u32; 3]) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let bg = pal.ramp(&[(0.0, 0x0a0f24), (1.0, 0x02040c)], 32);
    let planet = pal.ramp(&[(0.0, colors[0]), (1.0, 0x0a0f24)], 32);
    let path = pal.solid(0x1c2547);
    let moons = colors.map(|c| pal.ramp(&[(0.0, 0x0a0f24), (1.0, c)], 16));
    let star = pal.solid(0x94a3b8);
    let mut rng = Rng(3);
    let stars: Vec<(i32, i32)> = (0..24).map(|_| ((rng.next() * 256.0) as i32, (rng.next() * 256.0) as i32)).collect();
    let orbits = [(100.0f32, 0.35f32, 0.3f32, 1.0f32), (76.0, 0.55, -0.5, 2.0), (118.0, 0.25, 0.9, -1.0)];
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(IS, IS);
            icon_bg(&mut c, bg);
            for (x, y) in &stars {
                c.set(*x, *y, star);
            }
            let pos = |o: &(f32, f32, f32, f32), a: f32| {
                let (x, y) = (o.0 * a.cos(), o.0 * o.1 * a.sin());
                (127.5 + x * o.2.cos() - y * o.2.sin(), 127.5 + x * o.2.sin() + y * o.2.cos())
            };
            for o in &orbits {
                for s in 0..360 {
                    let (x, y) = pos(o, s as f32 / 360.0 * TAU);
                    c.set(x as i32, y as i32, path);
                }
            }
            for y in 0..IS {
                for x in 0..IS {
                    let (dx, dy) = (x as f32 - 127.5, y as f32 - 127.5);
                    let d = dx.hypot(dy);
                    if d < 46.0 {
                        let light = ((dx + 20.0).hypot(dy + 20.0) / 80.0).min(1.0);
                        c.px[y * IS + x] = planet.at(light);
                    }
                }
            }
            for (k, o) in orbits.iter().enumerate() {
                let a = TAU * t * o.3 + k as f32 * 2.1;
                for tail in (0..14).rev() {
                    let (x, y) = pos(o, a - tail as f32 * 0.06 * o.3.signum());
                    // Gezegenin arkasındaki uydu gizlenir.
                    let behind = (y - 127.5).abs() < 46.0 && (x - 127.5).hypot(y - 127.5) < 46.0 && (a - tail as f32 * 0.06).sin() * o.3.signum() < 0.0;
                    if !behind {
                        c.disc(x, y, if tail == 0 { 9.0 } else { 6.0 - tail as f32 * 0.35 }, moons[k].at(1.0 - tail as f32 / 14.0));
                    }
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

/// Lav lambası: düşük çözünürlükte (64×64, 4×) birleşip ayrılan metatoplar.
fn lava(n: usize, colors: [u32; 3]) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let bg = pal.ramp(&[(0.0, 0x120a1f), (1.0, 0x1d1033)], 16);
    let blob = pal.ramp(&[(0.0, colors[0]), (0.6, colors[1]), (1.0, colors[2])], 40);
    let edge = pal.solid(0x2a1745);
    let balls = [(0.3f32, 1.0f32, 0.0f32, 9.0f32), (0.6, -1.0, 1.5, 11.0), (0.5, 2.0, 3.0, 7.0), (0.25, -2.0, 4.2, 8.0), (0.75, 1.0, 5.1, 6.0)];
    let s = 64;
    let frames = (0..n)
        .map(|f| {
            let a = TAU * f as f32 / n as f32;
            let mut c = Canvas::new(s, s);
            let centers: Vec<(f32, f32, f32)> = balls.iter().map(|b| (s as f32 * b.0 + 8.0 * (a * b.1 + b.2).cos(), s as f32 * 0.5 + 22.0 * (a * b.1.abs() + b.2).sin(), b.3)).collect();
            for y in 0..s {
                for x in 0..s {
                    let mut v = 0.0;
                    for (cx, cy, r) in &centers {
                        let d2 = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2);
                        v += r * r / (d2 + 1.0);
                    }
                    c.px[y * s + x] = if v > 1.0 {
                        blob.at(((v - 1.0) / 3.0).min(1.0))
                    } else if v > 0.8 {
                        edge
                    } else {
                        bg.at(y as f32 / s as f32)
                    };
                }
            }
            c.upscale(4)
        })
        .collect();
    (frames, pal)
}

/// 5×7 piksel yazı tipi (yalnız gereken harfler).
fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        _ => [0; 7],
    }
}

/// Piksel baş harfler: harfler hafifçe zıplar, renk dalgası çapraz akar, arkada kayan çizgiler.
fn initials(n: usize, text: &str, colors: [u32; 3]) -> (Vec<Canvas>, Palette) {
    let mut pal = Palette::new();
    let bg = pal.ramp(&[(0.0, 0x0f172a), (1.0, 0x1e1b4b)], 24);
    let stripe = pal.solid(0x272a5c);
    let wave = pal.ramp(&[(0.0, colors[0]), (0.33, colors[1]), (0.66, colors[2]), (1.0, colors[0])], 64);
    let shadow = pal.solid(0x070a18);
    let chars: Vec<char> = text.chars().collect();
    let cell = if chars.len() > 1 { 14 } else { 22 };
    let total_w = chars.len() as i32 * 6 * cell - cell;
    let frames = (0..n)
        .map(|f| {
            let t = f as f32 / n as f32;
            let mut c = Canvas::new(IS, IS);
            for y in 0..IS {
                for x in 0..IS {
                    let diag = ((x + y) as f32 + t * 64.0) % 32.0;
                    c.px[y * IS + x] = if diag < 3.0 { stripe } else { bg.at(y as f32 / IS as f32) };
                }
            }
            let x0 = (IS as i32 - total_w) / 2;
            let y0 = (IS as i32 - 7 * cell) / 2;
            for (i, ch) in chars.iter().enumerate() {
                let bob = (6.0 * (TAU * (t + i as f32 * 0.25)).sin()) as i32;
                let g = glyph(*ch);
                for (row, bits) in g.iter().enumerate() {
                    for col in 0..5 {
                        if bits & (1 << (4 - col)) != 0 {
                            let (px, py) = (x0 + (i as i32 * 6 + col) * cell, y0 + row as i32 * cell + bob);
                            c.rect(px + 4, py + 4, cell, cell, shadow);
                            let k = ((px + py) as f32 / 400.0 - t).rem_euclid(1.0);
                            c.rect(px, py, cell - 1, cell - 1, wave.at(k));
                        }
                    }
                }
            }
            c
        })
        .collect();
    (frames, pal)
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "gif-ornekleri".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out).expect("çıkış klasörü");
    let banners: [(&str, fn(usize) -> (Vec<Canvas>, Palette), usize, u32); 6] = [
        ("afis-synthwave", synthwave, 36, 70),
        ("afis-aurora", aurora, 40, 80),
        ("afis-daglar", parallax, 72, 70),
        ("afis-piksel-sehir", pixel_city, 40, 100),
        ("afis-plazma", plasma, 40, 70),
        ("afis-dalgalar", waves, 40, 80),
    ];
    for (name, scene, n, delay) in banners {
        let (frames, pal) = scene(n);
        write_gif(&out.join(format!("{name}.gif")), frames, &pal, delay);
    }
    type Icon = Box<dyn Fn() -> (Vec<Canvas>, Palette)>;
    let icons: Vec<(&str, Icon, u32)> = vec![
        ("ikon-halkalar", Box::new(|| pulse(36, [0x2c7bff, 0x7b5cff, 0xffffff])), 60),
        ("ikon-altigen", Box::new(|| hexagon(36, [0xff7a2f, 0xffb347, 0xc2410c])), 60),
        ("ikon-yorunge", Box::new(|| orbit(48, [0x4f9dff, 0xffd166, 0x7b5cff])), 50),
        ("ikon-lav", Box::new(|| lava(40, [0xff5fa2, 0xffb86b, 0xfff1c1])), 70),
        ("ikon-halkalar-yesil", Box::new(|| pulse(36, [0x1fd6a0, 0x2c9bff, 0xe0fff4])), 60),
        ("ikon-altigen-neon", Box::new(|| hexagon(36, [0xb14dff, 0xff4d9d, 0x2de2e6])), 60),
        ("avatar-p2", Box::new(|| initials(36, "P2", [0x2c7bff, 0x7b5cff, 0x18c8d8])), 70),
        ("avatar-rb", Box::new(|| initials(36, "RB", [0xff7a2f, 0xffb347, 0xff4f7b])), 70),
        ("avatar-r2", Box::new(|| initials(36, "R2", [0x1fd6a0, 0xb6f36a, 0x2c9bff])), 70),
        ("avatar-k", Box::new(|| initials(36, "K", [0xb14dff, 0xff4d9d, 0x2de2e6])), 70),
        ("avatar-yorunge", Box::new(|| orbit(48, [0xf472b6, 0x19d3c5, 0xfbbf24])), 50),
        ("avatar-lav", Box::new(|| lava(40, [0x2c7bff, 0x7b5cff, 0xe0e7ff])), 70),
        ("avatar-halkalar", Box::new(|| pulse(36, [0xff4d9d, 0xffb347, 0xfff7d6])), 60),
        ("avatar-altigen", Box::new(|| hexagon(36, [0x1fd6a0, 0x2c7bff, 0xb6f36a])), 60),
    ];
    for (name, make, delay) in icons {
        let (frames, pal) = make();
        write_gif(&out.join(format!("{name}.gif")), frames, &pal, delay);
    }
}
