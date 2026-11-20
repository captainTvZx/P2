//! GPUI çıktısı → DLL karesi: 2× küçültme + önceden çarpılmış BGRA + kirli dikdörtgen.

/// GPUI çıktısını (2× ölçekli, `2w × 2h` RGBA) DLL'in beklediği `w × h` önceden çarpılmış
/// BGRA'ya çevirir: 2×2 kutu filtresiyle küçültür (süper örnekleme). Düz alfa algılanırsa
/// (bir pikselde renk > alfa) ortalamadan önce çarpar. Dönüş: düz alfa mıydı.
///
/// Neden 2×: GPUI'nin ekran dışı test penceresi sabit 2.0 ölçekle çalışıyor ve ölçeği
/// değiştiren işlev crate içine kapalı (Faz 0 bulgusu).
pub fn downsample_premultiplied_bgra(rgba: &[u8], w: u32, h: u32, out: &mut [u8]) -> bool {
    let straight = rgba.chunks_exact(4).any(|p| p[3] < 255 && (p[0] > p[3] || p[1] > p[3] || p[2] > p[3]));
    let src_stride = (w * 2 * 4) as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut acc = [0u32; 4];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let s = &rgba[(y * 2 + dy) * src_stride + (x * 2 + dx) * 4..][..4];
                let a = s[3] as u32;
                let mul = |c: u8| if straight { (c as u32 * a + 127) / 255 } else { c as u32 };
                acc[0] += mul(s[2]);
                acc[1] += mul(s[1]);
                acc[2] += mul(s[0]);
                acc[3] += a;
            }
            let d = &mut out[(y * w as usize + x) * 4..][..4];
            for i in 0..4 {
                d[i] = ((acc[i] + 2) / 4) as u8;
            }
        }
    }
    straight
}

/// Arayüz ölçeği 1 değilken: `sw × sh` RGBA çıktıyı `w × h` önceden çarpılmış BGRA'ya alan
/// ortalamasıyla (kutu süzgeci) örnekler. Ölçek 0.5–2 aralığında kaynak/hedef oranı 1–4'tür;
/// her hedef pikseli kaynağın kendisine düşen dikdörtgeninin ortalamasıdır.
pub fn resample_premultiplied_bgra(rgba: &[u8], sw: u32, sh: u32, w: u32, h: u32, out: &mut [u8]) -> bool {
    let straight = rgba.chunks_exact(4).any(|p| p[3] < 255 && (p[0] > p[3] || p[1] > p[3] || p[2] > p[3]));
    let (fx, fy) = (sw as f32 / w as f32, sh as f32 / h as f32);
    let span = |i: u32, f: f32, max: u32| {
        let a = ((i as f32 * f).floor() as u32).min(max - 1);
        let b = (((i + 1) as f32 * f).ceil() as u32).clamp(a + 1, max);
        (a, b)
    };
    let xs: Vec<(u32, u32)> = (0..w).map(|x| span(x, fx, sw)).collect();
    let stride = (sw * 4) as usize;
    for y in 0..h {
        let (y0, y1) = span(y, fy, sh);
        for (x, &(x0, x1)) in xs.iter().enumerate() {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                let row = &rgba[sy as usize * stride..];
                for sx in x0..x1 {
                    let s = &row[sx as usize * 4..][..4];
                    let a = s[3] as u32;
                    let mul = |c: u8| if straight { (c as u32 * a + 127) / 255 } else { c as u32 };
                    acc[0] += mul(s[2]);
                    acc[1] += mul(s[1]);
                    acc[2] += mul(s[0]);
                    acc[3] += a;
                    n += 1;
                }
            }
            let d = &mut out[(y as usize * w as usize + x) * 4..][..4];
            for i in 0..4 {
                d[i] = ((acc[i] + n / 2) / n.max(1)) as u8;
            }
        }
    }
    straight
}

/// 1× çizim: `w × h` RGBA → önceden çarpılmış BGRA (aynı boyut; düz alfa ise çarpılır).
pub fn copy_premultiplied_bgra(rgba: &[u8], out: &mut [u8]) -> bool {
    let straight = rgba.chunks_exact(4).any(|p| p[3] < 255 && (p[0] > p[3] || p[1] > p[3] || p[2] > p[3]));
    for (s, d) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        let a = s[3] as u32;
        let mul = |c: u8| if straight { ((c as u32 * a + 127) / 255) as u8 } else { c };
        d[0] = mul(s[2]);
        d[1] = mul(s[1]);
        d[2] = mul(s[0]);
        d[3] = s[3];
    }
    straight
}

/// Değişen piksellerin sınır kutusu (x, y, w, h); değişiklik yoksa None.
pub fn dirty_rect(prev: &[u8], next: &[u8], w: u32, h: u32) -> Option<(i32, i32, i32, i32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let (pa, na): (&[u32], &[u32]) = (bytemuck_u32(prev), bytemuck_u32(next));
    for y in 0..h {
        let row = (y * w) as usize;
        let (pr, nr) = (&pa[row..row + w as usize], &na[row..row + w as usize]);
        if pr == nr {
            continue;
        }
        let first = pr.iter().zip(nr).position(|(a, b)| a != b).unwrap() as u32;
        let last = w - 1 - pr.iter().rev().zip(nr.iter().rev()).position(|(a, b)| a != b).unwrap() as u32;
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    (x0 != u32::MAX).then(|| (x0 as i32, y0 as i32, (x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32))
}

pub fn bytemuck_u32(bytes: &[u8]) -> &[u32] {
    assert!(bytes.len() % 4 == 0 && (bytes.as_ptr() as usize) % 4 == 0);
    unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u32, bytes.len() / 4) }
}

