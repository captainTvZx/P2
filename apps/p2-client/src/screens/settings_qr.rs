//! İki adımlı doğrulama karekodu — çevrimdışı QR üreticisi (bayt kipi, hata düzeltme M, sürüm 1–10,
//! en düşük cezalı maske). Electron `QRCode.toDataURL(otpauth, {margin:1, color:{dark:'#101317'}})`
//! karşılığı; anahtar hiçbir dış servise gönderilmez. Çıktı SVG (kenar boşluğu 1 modül).

/// (toplam kod sözcüğü, blok başına EC, [(blok sayısı, blok veri sözcüğü)]) — ISO 18004, düzey M.
const BLOCKS: [(usize, usize, [(usize, usize); 2]); 10] = [
    (26, 10, [(1, 16), (0, 0)]),
    (44, 16, [(1, 28), (0, 0)]),
    (70, 26, [(1, 44), (0, 0)]),
    (100, 18, [(2, 32), (0, 0)]),
    (134, 24, [(2, 43), (0, 0)]),
    (172, 16, [(4, 27), (0, 0)]),
    (196, 18, [(4, 31), (0, 0)]),
    (242, 22, [(2, 38), (2, 39)]),
    (292, 22, [(3, 36), (2, 37)]),
    (346, 26, [(4, 43), (1, 44)]),
];

const ALIGN: [&[usize]; 10] = [&[], &[6, 18], &[6, 22], &[6, 26], &[6, 30], &[6, 34], &[6, 22, 38], &[6, 24, 42], &[6, 26, 46], &[6, 28, 50]];

fn gf_tables() -> ([u8; 256], [u8; 256]) {
    let (mut exp, mut log) = ([0u8; 256], [0u8; 256]);
    let mut x: u16 = 1;
    for i in 0..255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11d;
        }
    }
    exp[255] = exp[0];
    (exp, log)
}

fn gf_mul(a: u8, b: u8, exp: &[u8; 256], log: &[u8; 256]) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    exp[(log[a as usize] as usize + log[b as usize] as usize) % 255]
}

/// Reed–Solomon artık sözcükleri.
fn rs_remainder(data: &[u8], degree: usize) -> Vec<u8> {
    let (exp, log) = gf_tables();
    // Üreteç: (x − α^0)(x − α^1)…(x − α^(n−1)); katsayılar yüksek dereceden.
    let mut generator = vec![1u8];
    for i in 0..degree {
        let mut next = vec![0u8; generator.len() + 1];
        for (j, &g) in generator.iter().enumerate() {
            next[j] ^= g;
            next[j + 1] ^= gf_mul(g, exp[i], &exp, &log);
        }
        generator = next;
    }
    let mut rem = vec![0u8; degree];
    for &d in data {
        let factor = d ^ rem[0];
        rem.remove(0);
        rem.push(0);
        for (k, r) in rem.iter_mut().enumerate() {
            *r ^= gf_mul(generator[k + 1], factor, &exp, &log);
        }
    }
    rem
}

fn bch(value: u32, poly: u32, poly_bits: u32) -> u32 {
    let mut v = value << (poly_bits - 1);
    while v >= (1 << (poly_bits - 1)) {
        let shift = 32 - v.leading_zeros() - poly_bits;
        v ^= poly << shift;
    }
    v
}

struct Grid {
    n: usize,
    dark: Vec<bool>,
    reserved: Vec<bool>,
}

impl Grid {
    fn set(&mut self, x: usize, y: usize, dark: bool) {
        let i = y * self.n + x;
        self.dark[i] = dark;
        self.reserved[i] = true;
    }
    fn get(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.n + x]
    }
}

fn mask_bit(mask: u8, x: usize, y: usize) -> bool {
    let (i, j) = (y, x);
    match mask {
        0 => (i + j) % 2 == 0,
        1 => i % 2 == 0,
        2 => j % 3 == 0,
        3 => (i + j) % 3 == 0,
        4 => (i / 2 + j / 3) % 2 == 0,
        5 => (i * j) % 2 + (i * j) % 3 == 0,
        6 => ((i * j) % 2 + (i * j) % 3) % 2 == 0,
        _ => ((i + j) % 2 + (i * j) % 3) % 2 == 0,
    }
}

fn function_patterns(version: usize) -> Grid {
    let n = 17 + 4 * version;
    let mut g = Grid { n, dark: vec![false; n * n], reserved: vec![false; n * n] };
    // Konum desenleri + ayırıcılar.
    for (ox, oy) in [(0isize, 0isize), (n as isize - 7, 0), (0, n as isize - 7)] {
        for dy in -1..=7isize {
            for dx in -1..=7isize {
                let (x, y) = (ox + dx, oy + dy);
                if x < 0 || y < 0 || x >= n as isize || y >= n as isize {
                    continue;
                }
                let on = (0..=6).contains(&dx) && (0..=6).contains(&dy) && (dx == 0 || dx == 6 || dy == 0 || dy == 6 || ((2..=4).contains(&dx) && (2..=4).contains(&dy)));
                g.set(x as usize, y as usize, on);
            }
        }
    }
    // Zamanlama.
    for i in 8..n - 8 {
        g.set(i, 6, i % 2 == 0);
        g.set(6, i, i % 2 == 0);
    }
    // Hizalama.
    let pos = ALIGN[version - 1];
    for &ay in pos {
        for &ax in pos {
            if g.reserved[ay * n + ax] {
                continue;
            }
            for dy in -2..=2isize {
                for dx in -2..=2isize {
                    let on = dx.abs() == 2 || dy.abs() == 2 || (dx == 0 && dy == 0);
                    g.set((ax as isize + dx) as usize, (ay as isize + dy) as usize, on);
                }
            }
        }
    }
    // Biçim alanları (şimdilik ayrılmış) + karanlık modül.
    for i in 0..9 {
        g.reserved[8 * n + i] = true;
        g.reserved[i * n + 8] = true;
    }
    for i in 0..8 {
        g.reserved[8 * n + (n - 1 - i)] = true;
        g.reserved[(n - 1 - i) * n + 8] = true;
    }
    g.set(8, n - 8, true);
    // Sürüm bilgisi (7+).
    if version >= 7 {
        let bits = ((version as u32) << 12) | bch(version as u32, 0x1f25, 13);
        for i in 0..18 {
            let on = (bits >> i) & 1 == 1;
            let (a, b) = (i / 3, i % 3 + n - 11);
            g.set(a, b, on);
            g.set(b, a, on);
        }
    }
    g
}

fn draw_format(g: &mut Grid, mask: u8) {
    let n = g.n;
    // Düzey M = 00.
    let data = mask as u32;
    let bits = ((data << 10) | bch(data, 0x537, 11)) ^ 0x5412;
    let bit = |i: u32| (bits >> i) & 1 == 1;
    for i in 0..6 {
        g.set(8, i, bit(i as u32));
    }
    g.set(8, 7, bit(6));
    g.set(8, 8, bit(7));
    g.set(7, 8, bit(8));
    for i in 9..15 {
        g.set(14 - i, 8, bit(i as u32));
    }
    for i in 0..8 {
        g.set(n - 1 - i, 8, bit(i as u32));
    }
    for i in 8..15 {
        g.set(8, n - 15 + i, bit(i as u32));
    }
    g.set(8, n - 8, true);
}

fn penalty(g: &Grid) -> u32 {
    let n = g.n;
    let mut score = 0u32;
    for horizontal in [true, false] {
        for a in 0..n {
            let mut run = 1;
            let at = |b: usize| if horizontal { g.get(b, a) } else { g.get(a, b) };
            for b in 1..n {
                if at(b) == at(b - 1) {
                    run += 1;
                    if run == 5 {
                        score += 3;
                    } else if run > 5 {
                        score += 1;
                    }
                } else {
                    run = 1;
                }
            }
            // 1:1:3:1:1 desenleri.
            for b in 0..n.saturating_sub(10) {
                let pat: Vec<bool> = (0..11).map(|k| at(b + k)).collect();
                let p1 = [true, false, true, true, true, false, true, false, false, false, false];
                let p2 = [false, false, false, false, true, false, true, true, true, false, true];
                if pat == p1 || pat == p2 {
                    score += 40;
                }
            }
        }
    }
    for y in 0..n - 1 {
        for x in 0..n - 1 {
            let c = g.get(x, y);
            if c == g.get(x + 1, y) && c == g.get(x, y + 1) && c == g.get(x + 1, y + 1) {
                score += 3;
            }
        }
    }
    let dark = g.dark.iter().filter(|d| **d).count() as i64;
    let total = (n * n) as i64;
    let k = ((dark * 20 - total * 10).abs() + total - 1) / total - 1;
    score + (k.max(0) as u32) * 10
}

/// `text`i karekod modüllerine çevirir (satır satır, `true` = koyu).
pub fn encode(text: &str) -> Option<(usize, Vec<bool>)> {
    let bytes = text.as_bytes();
    let version = (1..=10).find(|&v| {
        let (total, ec, groups) = BLOCKS[v - 1];
        let blocks = groups[0].0 + groups[1].0;
        let data_cw = total - ec * blocks;
        4 + if v < 10 { 8 } else { 16 } + bytes.len() * 8 <= data_cw * 8
    })?;
    let (total, ec, groups) = BLOCKS[version - 1];
    let blocks = groups[0].0 + groups[1].0;
    let data_cw = total - ec * blocks;
    // Bit akışı: kip 0100, uzunluk, veri, sonlandırıcı, dolgu.
    let mut bits: Vec<bool> = Vec::new();
    let push = |v: u32, len: usize, bits: &mut Vec<bool>| {
        for i in (0..len).rev() {
            bits.push((v >> i) & 1 == 1);
        }
    };
    push(0b0100, 4, &mut bits);
    push(bytes.len() as u32, if version < 10 { 8 } else { 16 }, &mut bits);
    for &b in bytes {
        push(b as u32, 8, &mut bits);
    }
    let cap = data_cw * 8;
    let term = (cap - bits.len()).min(4);
    push(0, term, &mut bits);
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut data: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8)).collect();
    let mut pad = [0xecu8, 0x11].iter().cycle();
    while data.len() < data_cw {
        data.push(*pad.next().unwrap());
    }
    // Bloklara böl, EC ekle, iç içe geçir.
    let mut data_blocks: Vec<Vec<u8>> = Vec::new();
    let mut at = 0;
    for &(count, size) in groups.iter() {
        for _ in 0..count {
            data_blocks.push(data[at..at + size].to_vec());
            at += size;
        }
    }
    let ec_blocks: Vec<Vec<u8>> = data_blocks.iter().map(|b| rs_remainder(b, ec)).collect();
    let mut stream = Vec::with_capacity(total);
    let longest = data_blocks.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..longest {
        for b in &data_blocks {
            if i < b.len() {
                stream.push(b[i]);
            }
        }
    }
    for i in 0..ec {
        for b in &ec_blocks {
            stream.push(b[i]);
        }
    }
    let stream_bits: Vec<bool> = stream.iter().flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1 == 1)).collect();

    let base = function_patterns(version);
    let n = base.n;
    // Zikzak yerleşim (sağdan sola, sütun 6 atlanır).
    let mut placed = Grid { n, dark: base.dark.clone(), reserved: base.reserved.clone() };
    let mut data_cells: Vec<(usize, usize)> = Vec::new();
    let mut right = n as isize - 1;
    let mut upward = true;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for k in 0..n {
            let y = if upward { n - 1 - k } else { k };
            for dx in 0..2 {
                let x = (right - dx) as usize;
                if !base.reserved[y * n + x] {
                    data_cells.push((x, y));
                }
            }
        }
        upward = !upward;
        right -= 2;
    }
    for (i, &(x, y)) in data_cells.iter().enumerate() {
        placed.dark[y * n + x] = stream_bits.get(i).copied().unwrap_or(false);
    }
    // En düşük cezalı maske.
    let mut best: Option<(u32, Grid)> = None;
    for mask in 0..8u8 {
        let mut g = Grid { n, dark: placed.dark.clone(), reserved: base.reserved.clone() };
        for &(x, y) in &data_cells {
            if mask_bit(mask, x, y) {
                g.dark[y * n + x] = !g.dark[y * n + x];
            }
        }
        draw_format(&mut g, mask);
        let score = penalty(&g);
        if best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, g));
        }
    }
    best.map(|(_, g)| (n, g.dark))
}

/// SVG karekod (1 modül kenar boşluğu, koyu #101317, açık beyaz).
pub fn svg(text: &str) -> Option<String> {
    let (n, cells) = encode(text)?;
    let size = n + 2;
    let mut path = String::new();
    for y in 0..n {
        for x in 0..n {
            if cells[y * n + x] {
                path.push_str(&format!("M{} {}h1v1h-1z", x + 1, y + 1));
            }
        }
    }
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 {size} {size}" shape-rendering="crispEdges"><rect width="{size}" height="{size}" fill="#ffffff"/><path d="{path}" fill="#101317"/></svg>"##,
        px = size * 8
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surum_ve_boyut() {
        let (n, _) = encode("otpauth://totp/P2:test?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=P2").unwrap();
        // 77 bayt → sürüm 5 (37×37).
        assert_eq!(n, 37);
        // Bilinen RS örneği (ISO 18004 ek I, "01234567" sürüm 1-M veri sözcükleri).
        let rem = rs_remainder(&[0x10, 0x20, 0x0c, 0x56, 0x61, 0x80, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11], 10);
        assert_eq!(rem, vec![0xa5, 0x24, 0xd4, 0xc1, 0xed, 0x36, 0xc7, 0x87, 0x2c, 0x55]);
    }
}
