// İçerik tanımlı parçalama (FastCDC benzeri, Gear kayan özeti). Saf, Electron'suz.
//
// Neden: sabit 4 MB dilimlerde dosyanın başına birkaç bayt girince sonraki BÜTÜN
// dilimlerin özeti değişir ve 2 GB'lık bir .ucas "tamamen değişti" sayılır. Kesim
// noktaları içerikten türetilince kayma yalnız 1-2 parçayı etkiler; Steam'in
// küçük yamalarının sırrı budur.
//
// Parametreler sunucu ve bütün istemcilerde SONSUZA DEK aynı kalmalı; yoksa aynı
// dosya iki makinede farklı parçalanır ve tekilleştirme bozulur.
const MIN = 1 * 1024 * 1024        // bu kadar bayt kesinlikle atlanır (hızlandırır)
const AVG = 2 * 1024 * 1024        // hedef ortalama
const MAX = 4 * 1024 * 1024        // = sunucudaki parça üst sınırı (CHUNK_SIZE)
const MASK_S = 0x001FFFFF >>> 0    // ortalamadan önce: 21 bit (daha zor kesim)
const MASK_L = 0x0007FFFF >>> 0    // ortalamadan sonra: 19 bit (daha kolay kesim)

// Deterministik 256'lık Gear tablosu (xorshift32, sabit tohum). Rastgele kaynak YOK.
const GEAR = new Uint32Array(256)
{
  let x = 0x9E3779B9 >>> 0
  for (let i = 0; i < 256; i++) {
    x ^= x << 13; x >>>= 0
    x ^= x >>> 17
    x ^= x << 5; x >>>= 0
    GEAR[i] = x
  }
}

// `buffer` içinde `start`'tan başlayan bir parçanın kesim noktasını döndürür
// (parça sonu indeksi, hariç). `final` false ise ve bitişe ulaşılırsa -1 döner:
// daha fazla veri gerekir.
function nextCut(buffer, start, end, final) {
  const available = end - start
  if (available <= MIN) return final ? end : -1
  if (available >= MAX) {
    // En çok MAX; MAX'a kadar tararız.
    end = start + MAX
  } else if (!final) {
    // Tampon MAX'tan kısa ve devamı gelecek: ancak MAX'ı garanti edemeyiz, yine de
    // mevcut aralıkta kesim bulunursa onu kullanmak güvenlidir (deterministiktir,
    // çünkü kesim yalnız kendinden önceki baytlara bağlıdır).
  }
  let hash = 0
  const avgAt = start + AVG
  let i = start + MIN
  for (; i < avgAt && i < end; i++) {
    hash = ((hash << 1) + GEAR[buffer[i]]) >>> 0
    if ((hash & MASK_S) === 0) return i + 1
  }
  for (; i < end; i++) {
    hash = ((hash << 1) + GEAR[buffer[i]]) >>> 0
    if ((hash & MASK_L) === 0) return i + 1
  }
  if (end - start >= MAX) return end           // MAX'a dayandı: zorunlu kesim
  return final ? end : -1
}

// Akış parçalayıcı: feed(buf) → biten parçalar; finish() → son parça (varsa).
// Bellek: en çok MAX + bir okuma bloğu kadar.
class Chunker {
  constructor() { this.carry = null }
  feed(block, final = false) {
    const out = []
    let buffer = this.carry ? Buffer.concat([this.carry, block]) : block
    let start = 0
    while (true) {
      const cut = nextCut(buffer, start, buffer.length, final)
      if (cut < 0) break
      if (cut === start) break
      out.push(buffer.subarray(start, cut))
      start = cut
    }
    this.carry = start < buffer.length ? Buffer.from(buffer.subarray(start)) : null
    return out
  }
  finish() {
    const out = this.carry ? this.feed(Buffer.alloc(0), true) : []
    this.carry = null
    return out
  }
}

// Tek seferlik: bir Buffer'ı parçalara böler (testler ve küçük dosyalar için).
function splitBuffer(buffer) {
  const chunker = new Chunker()
  return [...chunker.feed(buffer, false), ...chunker.finish()]
}

module.exports = { MIN, AVG, MAX, GEAR, nextCut, Chunker, splitBuffer }
