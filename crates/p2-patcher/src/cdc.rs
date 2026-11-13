//! İçerik tanımlı parçalama — electron/cdc.cjs'nin birebir karşılığı (FastCDC benzeri, Gear
//! kayan özeti). Parametreler sunucu ve bütün istemcilerde SONSUZA DEK aynı kalmalı; yoksa aynı
//! dosya iki makinede farklı parçalanır ve tekilleştirme bozulur. `cross_check` testi JS
//! uygulamasıyla aynı kesimleri verdiğini doğrular.

pub const MIN: usize = 1024 * 1024;
pub const AVG: usize = 2 * 1024 * 1024;
pub const MAX: usize = 4 * 1024 * 1024;
const MASK_S: u32 = 0x001F_FFFF;
const MASK_L: u32 = 0x0007_FFFF;

/// Deterministik 256'lık Gear tablosu (xorshift32, sabit tohum).
pub const GEAR: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut x: u32 = 0x9E37_79B9;
    let mut i = 0;
    while i < 256 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        table[i] = x;
        i += 1;
    }
    table
};

/// `buffer[start..end]` içinde bir parçanın kesim noktası (hariç). `final` değilse ve veri
/// yetmezse `None` (daha fazla veri gerekir).
pub fn next_cut(buffer: &[u8], start: usize, mut end: usize, fin: bool) -> Option<usize> {
    let available = end - start;
    if available <= MIN {
        return fin.then_some(end);
    }
    if available >= MAX {
        end = start + MAX;
    }
    let mut hash: u32 = 0;
    let avg_at = start + AVG;
    let mut i = start + MIN;
    while i < avg_at && i < end {
        hash = (hash << 1).wrapping_add(GEAR[buffer[i] as usize]);
        if hash & MASK_S == 0 {
            return Some(i + 1);
        }
        i += 1;
    }
    while i < end {
        hash = (hash << 1).wrapping_add(GEAR[buffer[i] as usize]);
        if hash & MASK_L == 0 {
            return Some(i + 1);
        }
        i += 1;
    }
    if end - start >= MAX {
        return Some(end);
    }
    fin.then_some(end)
}

/// Akış parçalayıcı: `feed` biten parçaları, `finish` sonuncusunu verir.
#[derive(Default)]
pub struct Chunker {
    carry: Vec<u8>,
}

impl Chunker {
    pub fn feed(&mut self, block: &[u8], fin: bool) -> Vec<Vec<u8>> {
        let mut buffer = std::mem::take(&mut self.carry);
        buffer.extend_from_slice(block);
        let mut out = Vec::new();
        let mut start = 0;
        while let Some(cut) = next_cut(&buffer, start, buffer.len(), fin) {
            if cut == start {
                break;
            }
            out.push(buffer[start..cut].to_vec());
            start = cut;
        }
        self.carry = buffer[start..].to_vec();
        out
    }

    pub fn finish(&mut self) -> Vec<Vec<u8>> {
        if self.carry.is_empty() {
            return Vec::new();
        }
        let out = self.feed(&[], true);
        self.carry.clear();
        out
    }
}

pub fn split(buffer: &[u8]) -> Vec<Vec<u8>> {
    let mut c = Chunker::default();
    let mut out = c.feed(buffer, false);
    out.extend(c.finish());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministik sözde rastgele veri (JS testinde aynı üretici kullanılır).
    pub fn sample(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    /// electron/cdc.cjs ile çapraz kontrol: aynı tablo ve aynı veride aynı kesimler/özetler.
    /// Değerler JS uygulamasından alındı (sample(11 MB + 12345, 7) → splitBuffer).
    #[test]
    fn cross_check_with_js() {
        assert_eq!([GEAR[0], GEAR[1], GEAR[2], GEAR[255]], [0x510c4619, 0xe02e553e, 0x7bb98f3a, 0xef588093]);
        let parts = split(&sample(11 * 1024 * 1024 + 12345, 7));
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![2285570, 2437849, 2429338, 2215617, 1480226, 698081]);
        use sha2::{Digest, Sha256};
        assert_eq!(hex::encode(Sha256::digest(&parts[0])), "d20fee479533e65f595084e709e724908b027f9bc4b29c88301adc3f50dd9750");
    }

    #[test]
    fn chunks_cover_input_within_limits() {
        let data = sample(11 * 1024 * 1024 + 12345, 7);
        let parts = split(&data);
        assert_eq!(parts.iter().map(|p| p.len()).sum::<usize>(), data.len());
        for p in &parts[..parts.len() - 1] {
            assert!(p.len() > MIN && p.len() <= MAX, "{}", p.len());
        }
        // Akış hâlinde (küçük bloklarla) beslemek aynı kesimleri verir.
        let mut c = Chunker::default();
        let mut streamed = Vec::new();
        for block in data.chunks(700_000) {
            streamed.extend(c.feed(block, false));
        }
        streamed.extend(c.finish());
        assert_eq!(streamed, parts);
    }

    #[test]
    fn insertion_only_disturbs_nearby_chunks() {
        let data = sample(16 * 1024 * 1024, 99);
        let mut shifted = data[..5_000_000].to_vec();
        shifted.extend_from_slice(b"P2 yama testi: araya giren birkac bayt");
        shifted.extend_from_slice(&data[5_000_000..]);
        let a = split(&data);
        let b = split(&shifted);
        let same = b.iter().filter(|p| a.contains(p)).count();
        assert!(same + 2 >= a.len(), "{same}/{}", a.len());
    }
}
