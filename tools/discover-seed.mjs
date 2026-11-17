// Keşfet ekranı için örnek veri: afişli/ikonlu herkese açık projeler, üyeler ve haber gönderileri.
// Yalnız API üzerinden çalışır (test sunucusu); tekrar çalıştırılabilir (var olan proje güncellenir).
// Görseller burada üretilir (yordamsal gradyan afişler); dış görsel ya da marka kullanılmaz.
//
//   node tools/discover-seed.mjs [--refresh] → P2_API ya da kendi başlattığı test sunucusu (:4130);
//                                             --refresh afişleri ve haber kapaklarını yeniden üretir
//   import { seedDiscover } from './discover-seed.mjs'   → testlerden
//
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış), `p2test` kurulum sahibi (Parola123). Projelerin sahibi
// `kesif00` hesabıdır (p2test ona `project.create` verir; p2test'in proje rayı değişmez); üyeler `kesif01…24`.
import zlib from 'node:zlib'
import { testServer } from './lib.mjs'

// ---------------------------------------------------------------- yordamsal görseller

function encodePng(w, h, rgba) {
  const raw = Buffer.alloc((w * 4 + 1) * h)
  for (let y = 0; y < h; y++) rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4)
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const crc = Buffer.alloc(4); crc.writeUInt32BE(zlib.crc32(td) >>> 0); return Buffer.concat([len, td, crc]) }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw, { level: 6 })), chunk('IEND', Buffer.alloc(0))])
}

function rng(seed) { let s = seed >>> 0; return () => { s = (s + 0x6d2b79f5) >>> 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296 } }
const hex = c => [(c >> 16) & 255, (c >> 8) & 255, c & 255]

/** Koyu zemin üstünde yumuşak renk bulutları, ışık şeridi, ince halkalar ve nokta ızgarası. */
export function artwork(w, h, colors, seed, { rings = true, motif = null } = {}) {
  const rand = rng(seed)
  const px = new Float32Array(w * h * 3)
  const [base, ...glows] = colors.map(hex)
  for (let i = 0; i < w * h; i++) { const y = Math.floor(i / w) / h; const k = 1 - y * 0.45; px[i * 3] = base[0] * k; px[i * 3 + 1] = base[1] * k; px[i * 3 + 2] = base[2] * k }
  const blobs = glows.flatMap(c => [0, 1].map(() => ({ c, x: rand() * w, y: rand() * h, r: (0.35 + rand() * 0.55) * Math.max(w, h) * 0.6, a: 0.55 + rand() * 0.35 })))
  for (const b of blobs) {
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const d = Math.hypot(x - b.x, y - b.y) / b.r
      if (d >= 1) continue
      const k = (1 - d) ** 2 * b.a, i = (y * w + x) * 3
      px[i] += (b.c[0] - px[i]) * k; px[i + 1] += (b.c[1] - px[i + 1]) * k; px[i + 2] += (b.c[2] - px[i + 2]) * k
    }
  }
  // Çapraz ışık şeridi.
  const sx = (0.25 + rand() * 0.5) * w, sw = w * (0.10 + rand() * 0.08)
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const d = Math.abs(x - sx + (y - h / 2) * 0.55) / sw
    if (d < 1) { const k = (1 - d) ** 3 * 0.16, i = (y * w + x) * 3; px[i] += (255 - px[i]) * k; px[i + 1] += (255 - px[i + 1]) * k; px[i + 2] += (255 - px[i + 2]) * k }
  }
  const tint = (i, c, k) => { px[i] += (c[0] - px[i]) * k; px[i + 1] += (c[1] - px[i + 1]) * k; px[i + 2] += (c[2] - px[i + 2]) * k }
  const white = [255, 255, 255]
  if (motif === 'ridges') {
    // Üst üste üç sıradağ silüeti (uzak → yakın, giderek koyulaşır) + ufukta güneş diski.
    const sunX = (0.58 + rand() * 0.25) * w, sunY = h * 0.42, sunR = h * 0.2
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const d = Math.hypot(x - sunX, y - sunY); if (d < sunR * 2.2) tint((y * w + x) * 3, white, d < sunR ? 0.42 : 0.2 * (1 - (d - sunR) / (sunR * 1.2)) ** 2) }
    for (let layer = 0; layer < 3; layer++) {
      const baseY = h * (0.5 + layer * 0.15), amp = h * (0.16 - layer * 0.03)
      const waves = [0, 1, 2, 3].map(n => ({ f: (1.2 + n * 1.7 + rand()) * Math.PI * 2 / w, p: rand() * 7, a: amp / (1 + n * 1.3) }))
      const shade = glows[layer % glows.length].map((v, k) => base[k] * (0.9 - layer * 0.22) + v * (0.26 - layer * 0.08))
      for (let x = 0; x < w; x++) {
        let top = baseY
        for (const wv of waves) top -= Math.abs(Math.sin(x * wv.f + wv.p)) * wv.a
        for (let y = Math.max(0, Math.floor(top)); y < h; y++) tint((y * w + x) * 3, shade, 0.86)
      }
    }
  } else if (motif === 'grid') {
    // Ufka doğru daralan ızgara (alt yarı) + üstte ince tarama çizgileri.
    const horizon = h * 0.52, vx = w * (0.4 + rand() * 0.2)
    for (let n = 1; n < 16; n++) { const y = Math.round(horizon + (h - horizon) * (n / 15) ** 2.1); if (y < h) for (let x = 0; x < w; x++) tint((y * w + x) * 3, glows[0], 0.34 * (n / 15)) }
    for (let n = -14; n <= 14; n++) for (let y = Math.ceil(horizon); y < h; y++) { const t = (y - horizon) / (h - horizon); const x = Math.round(vx + n * w * 0.09 * (0.12 + t * 1.6)); if (x >= 0 && x < w) tint((y * w + x) * 3, glows[0], 0.3 * t) }
    for (let y = 0; y < horizon; y += 6) for (let x = 0; x < w; x++) tint((y * w + x) * 3, white, 0.03)
    const gx = vx, gy = horizon
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const d = Math.hypot((x - gx) / (w * 0.5), (y - gy) / (h * 0.16)); if (d < 1) tint((y * w + x) * 3, glows[1] || white, 0.5 * (1 - d) ** 2) }
  } else if (motif === 'bars') {
    // Ekolayzır çubukları (alttan yükselen) + yansıması.
    const n = 56, bw = w / n
    let level = 0.5
    for (let b = 0; b < n; b++) {
      level = Math.max(0.12, Math.min(0.95, level + (rand() - 0.5) * 0.42))
      const bh = h * 0.62 * level * (0.55 + 0.45 * Math.sin((b / n) * Math.PI))
      const c = glows[Math.floor((b / n) * glows.length) % glows.length]
      for (let x = Math.floor(b * bw + bw * 0.22); x < Math.floor((b + 1) * bw - bw * 0.22); x++) {
        for (let y = Math.floor(h * 0.8 - bh); y < h * 0.8; y++) tint((y * w + x) * 3, c.map(v => v * 0.55 + 255 * 0.45), 0.62)
        for (let y = Math.floor(h * 0.8) + 3; y < Math.min(h, h * 0.8 + bh * 0.4); y++) tint((y * w + x) * 3, c, 0.2 * (1 - (y - h * 0.8) / (bh * 0.4)))
      }
    }
  } else if (motif === 'bubbles') {
    // Yumuşak kenarlı, üst üste binen daireler (topluluk).
    for (let b = 0; b < 16; b++) {
      const cx = rand() * w, cy = rand() * h, r = (0.06 + rand() * 0.16) * h * 1.4, c = glows[b % glows.length], fill = rand() < 0.5
      for (let y = Math.max(0, Math.floor(cy - r)); y < Math.min(h, cy + r); y++) for (let x = Math.max(0, Math.floor(cx - r)); x < Math.min(w, cx + r); x++) {
        const d = Math.hypot(x - cx, y - cy)
        if (d < r) tint((y * w + x) * 3, fill ? c.map(v => v * 0.7 + 255 * 0.3) : white, fill ? 0.2 : (r - d < 2 ? 0.34 : 0.03))
      }
    }
  } else if (rings) {
    const cx = (0.6 + rand() * 0.3) * w, cy = (0.2 + rand() * 0.6) * h
    for (const r of [0.16, 0.27, 0.40].map(k => k * Math.max(w, h))) {
      for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
        const d = Math.abs(Math.hypot(x - cx, y - cy) - r)
        if (d < 1.4) { const k = (1 - d / 1.4) * 0.22, i = (y * w + x) * 3; px[i] += (255 - px[i]) * k; px[i + 1] += (255 - px[i + 1]) * k; px[i + 2] += (255 - px[i + 2]) * k }
      }
    }
    const step = Math.round(w / 34)
    for (let y = step; y < h; y += step) for (let x = step; x < w * 0.42; x += step) {
      const k = 0.16 * (1 - x / (w * 0.42)), i = (y * w + x) * 3
      for (const o of [0, 3, w * 3, w * 3 + 3]) { px[i + o] += (255 - px[i + o]) * k; px[i + o + 1] += (255 - px[i + o + 1]) * k; px[i + o + 2] += (255 - px[i + o + 2]) * k }
    }
  }
  const out = Buffer.alloc(w * h * 4)
  for (let i = 0; i < w * h; i++) {
    const n = (rand() - 0.5) * 5 // bantlanmayı kıran ince gren
    out[i * 4] = Math.max(0, Math.min(255, px[i * 3] + n)); out[i * 4 + 1] = Math.max(0, Math.min(255, px[i * 3 + 1] + n)); out[i * 4 + 2] = Math.max(0, Math.min(255, px[i * 3 + 2] + n)); out[i * 4 + 3] = 255
  }
  return encodePng(w, h, out)
}

/** Proje ikonu: gradyan zemin + sade geometrik işaret (daire / kare / üçgen / halka / çubuklar / artı). */
export function iconArt(size, colors, seed, mark) {
  const [a, b] = colors.map(hex)
  const out = Buffer.alloc(size * size * 4)
  const c = size / 2
  const inMark = (x, y) => {
    const dx = x - c, dy = y - c, r = size * 0.26
    switch (mark % 6) {
      case 0: return Math.hypot(dx, dy) < r
      case 1: return Math.abs(dx) < r * 0.85 && Math.abs(dy) < r * 0.85 && !(Math.abs(dx) < r * 0.4 && Math.abs(dy) < r * 0.4)
      case 2: return dy > -r && dy < r && Math.abs(dx) < (dy + r) * 0.58
      case 3: { const d = Math.hypot(dx, dy); return d < r * 1.1 && d > r * 0.62 }
      case 4: return Math.abs(dy) < r && [-0.8, -0.27, 0.27, 0.8].some((k, i) => Math.abs(dx - k * r) < r * 0.14 && Math.abs(dy) < r * [0.5, 1, 0.75, 0.4][i])
      default: return (Math.abs(dx) < r * 0.24 && Math.abs(dy) < r) || (Math.abs(dy) < r * 0.24 && Math.abs(dx) < r)
    }
  }
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const t = (x + y) / (2 * size), i = (y * size + x) * 4
    const glow = Math.max(0, 1 - Math.hypot(x - size * 0.3, y - size * 0.22) / (size * 0.75)) * 0.25
    let col = [0, 1, 2].map(k => a[k] + (b[k] - a[k]) * t + (255 - a[k]) * glow)
    if (inMark(x, y)) col = col.map(v => v + (255 - v) * 0.92)
    out[i] = col[0]; out[i + 1] = col[1]; out[i + 2] = col[2]; out[i + 3] = 255
  }
  void seed
  return encodePng(size, size, out)
}

// ---------------------------------------------------------------- örnek içerik

export const DEMO = [
  { slug: 'kesif-northwind', name: 'Northwind', category: 'GAME', members: 23, online: 9, palette: [0x0b1730, 0x2c7bff, 0x7b5cff, 0x18c8d8],
    tagline: 'Açık dünya keşif oyunu — haftalık oynanabilir sürümler',
    description: 'Northwind, rüzgârla yön bulduğun bir açık dünya keşif oyunu. Burada her hafta yeni bir oynanabilir sürüm paylaşıyor, geri bildirim topluyor ve yol haritasını birlikte şekillendiriyoruz.',
    posts: [
      { title: 'Sürüm 0.8 “Kuzey Işıkları” yayında', content: 'Yeni buzul bölgesi, yeniden yazılan rüzgâr sistemi ve 40’tan fazla düzeltme.\n\nKaydınız korunur; kütüphaneden güncellemeniz yeterli. Geri bildirimlerinizi #geri-bildirim kanalına bekliyoruz.', cover: [0x071226, 0x19d3c5, 0x2c7bff, 0x8a5cff] },
      { title: 'Topluluk oyun gecesi: cumartesi 21.00', content: 'Geliştirici ekibiyle birlikte yeni haritayı baştan sona oynuyoruz. Sesli kanalda buluşalım; en iyi rotayı bulan üç kişiye oyun içi pelerin.' },
    ] },
  { slug: 'kesif-rust-atolyesi', name: 'Rust Atölyesi', category: 'DEV', members: 17, online: 6, palette: [0x1a0f0a, 0xff7a2f, 0xffb347, 0xc2410c],
    tagline: 'Sistem programlama, async ve oyun motorları üzerine',
    description: 'Rust yazanların buluşma noktası. Kod incelemeleri, haftalık küçük projeler ve async, bellek güvenliği, GPU programlama üzerine uzun sohbetler.',
    posts: [
      { title: 'Haftalık görev: kendi ayırıcını yaz', content: 'Bu haftanın görevi bir “bump allocator”. Başlangıç deposu ve test paketi #görevler kanalında. Pazar akşamı çözümleri birlikte inceliyoruz.', cover: [0x160b06, 0xff8a3d, 0xffc56b, 0x7c2d12] },
    ] },
  { slug: 'kesif-piksel-kahvesi', name: 'Piksel Kahvesi', category: 'COMMUNITY', members: 14, online: 5, palette: [0x1b0f24, 0xff5fa2, 0xffb86b, 0x8a5cff],
    tagline: 'Piksel sanat, oyun tasarımı ve bol sohbet',
    description: 'Çizenlerin, tasarlayanların ve sadece bakmayı sevenlerin kahvesi. Her gün bir çizim konusu, her ay ortak bir tuval.',
    posts: [
      { title: 'Kasım ortak tuvali açıldı', content: '64×64’lük karelerden oluşan dev bir şehir çiziyoruz. Kareni #tuval kanalından ayırt; palet sabit, hayal gücü serbest.', cover: [0x1a0d22, 0xff6fae, 0xffc27a, 0x6d4aff] },
    ] },
  { slug: 'kesif-synthwave', name: 'Synthwave Kolektifi', category: 'MUSIC', members: 11, online: 4, palette: [0x12082a, 0xb14dff, 0xff4d9d, 0x2de2e6],
    tagline: 'Analog synth, retro ritimler, ortak albümler',
    description: 'Synth tutkunları için: yama paylaşımı, miks geri bildirimi ve her çeyrekte çıkardığımız ortak albüm.',
    posts: [
      { title: 'Ortak albüm “Neon Sahil” için parça çağrısı', content: 'Tema: gece yarısı sahil yolu. 100–118 BPM arası, en fazla 5 dakika. Son gönderim 15 Aralık.' },
    ] },
  { slug: 'kesif-acik-akademi', name: 'Açık Akademi', category: 'EDUCATION', members: 19, online: 3, palette: [0x06201c, 0x1fd6a0, 0x2c9bff, 0xb6f36a],
    tagline: 'Birlikte öğren: matematik, algoritmalar, oyun fiziği',
    description: 'Ücretsiz çalışma grupları. Her dönem üç kulvar açıyoruz; herkes hem öğrenci hem anlatıcı.',
    posts: [
      { title: 'Kış dönemi çalışma grupları', content: 'Doğrusal cebir, grafik programlamaya giriş ve oyun fiziği kulvarları 2 Aralık’ta başlıyor. Kayıt #duyurular kanalında.', cover: [0x051a17, 0x22e0aa, 0x3aa0ff, 0xc8f77a] },
    ] },
  { slug: 'kesif-orbit-jam', name: 'Orbit Jam 2026', category: 'GAME', members: 9, online: 2, palette: [0x070b1c, 0x4f9dff, 0xffd166, 0x7b5cff],
    tagline: '72 saatlik oyun yapım maratonu — tema: yörünge',
    description: 'Üç gün, bir tema, sınırsız fikir. Ekip bul, oyununu yap, kütüphaneden herkesle paylaş.',
    posts: [] },
  { slug: 'kesif-shader-gunlukleri', name: 'Shader Günlükleri', category: 'DEV', members: 7, online: 1, palette: [0x0a1220, 0x19d3c5, 0x2c7bff, 0xf472b6],
    tagline: 'Her gün bir shader: gürültü, ışık, yansıma',
    description: 'Kısa, öğretici shader denemeleri ve arkasındaki matematik.',
    posts: [
      { title: 'Gün 112: hacimsel sis', content: 'Işın adımlama ile ucuz hacimsel sis. Kaynak ve adım adım açıklama sabitlenmiş mesajda.' },
    ] },
  { slug: 'kesif-klavye-kulubu', name: 'Mekanik Klavye Kulübü', category: 'OTHER', members: 5, online: 1, palette: [0x18181b, 0xa3a3a3, 0x4f9dff, 0xfbbf24], noCover: true,
    tagline: 'Switch’ler, tuş takımları ve tık sesleri',
    description: 'Kendi klavyesini toplayanlar için rehberler, grup alımları ve ses testleri.',
    posts: [] },
]

/** Kategoriye göre afiş motifi (yoksa halkalar + nokta ızgarası). */
const MOTIFS = { GAME: 'ridges', DEV: 'grid', MUSIC: 'bars', COMMUNITY: 'bubbles' }
const NEWS_MOTIFS = ['ridges', null, 'bubbles', 'grid', 'bars']

/** Örnek veriyi kurar (`refresh`: afişleri ve haber kapaklarını yeniden üretir). Döner: `{ owner, projects: [{...demo, id}], users: [{ id, username, token }] }`. */
export async function seedDiscover(API, { log = console.log, refresh = false } = {}) {
  async function call(method, path, body, token) {
    const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
    return { status: r.status, body: await r.json().catch(() => null) }
  }
  const upload = async (name, buffer, token) => {
    const r = await call('POST', '/uploads', { name, type: 'image/png', data: 'data:image/png;base64,' + buffer.toString('base64') }, token)
    if (!r.body?.url) throw new Error('yükleme başarısız: ' + JSON.stringify(r.body))
    return r.body.url.slice(r.body.url.indexOf('/uploads/'))
  }
  const admin = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
  if (!admin?.accessToken) throw new Error('p2test girişi başarısız (test veritabanı kurulu mu?)')
  const signIn = async (username, displayName) => {
    let auth = (await call('POST', '/auth/login', { identifier: username, password: 'Parola123' })).body
    if (!auth?.accessToken) auth = (await call('POST', '/auth/register', { email: `${username}@p2.test`, username, displayName, password: 'Parola123', passwordConfirm: 'Parola123' })).body
    if (!auth?.accessToken) throw new Error(`${username} açılamadı: ${JSON.stringify(auth)}`)
    return auth
  }
  // Örnek projelerin sahibi ayrı bir hesaptır (kesif00): p2test'in proje rayı diğer testler için değişmesin.
  // Proje açmak sunucu yetkisi olduğundan p2test ona `project.create` verir.
  let owner = await signIn('kesif00', 'Keşif Ekibi')
  const grant = await call('PUT', `/server/users/${owner.user.id}/permissions`, { permissions: ['project.create'] }, admin.accessToken)
  if (grant.status !== 200) throw new Error('kesif00 için project.create verilemedi: ' + JSON.stringify(grant.body))
  // Eski sürüm projeleri p2test adına açmıştı: onları geri dönüşüme gönder.
  const adminOrg = (await call('GET', '/organizations', null, admin.accessToken)).body[0]
  for (const old of ((await call('GET', '/projects', null, admin.accessToken)).body || []).filter(p => p.ownerId === admin.user.id && p.slug.startsWith('kesif-') && DEMO.some(d => d.name === p.name))) {
    await call('DELETE', `/organizations/${adminOrg.id}/projects/${old.id}`, null, admin.accessToken)
    log(`  eski örnek proje kaldırıldı: ${old.name}`)
  }
  // Üyeler: kesif01..kesif24 (yoksa kayıt).
  const people = ['Ada Yıldız', 'Bora Tekin', 'Cemre Aksoy', 'Deniz Kara', 'Ece Şahin', 'Fırat Öz', 'Gökçe Arı', 'Hakan Demir', 'Irmak Su', 'Jale Tunç', 'Kaan Polat', 'Lale Güneş', 'Mert Aydın', 'Nil Erdem', 'Onur Çelik', 'Pelin Koç', 'Rüzgâr Ateş', 'Selin Uçar', 'Tuna Bulut', 'Umut Kaya', 'Vera Soylu', 'Yiğit Aslan', 'Zeynep Ak', 'Arda Bilgin']
  const users = []
  for (let i = 0; i < people.length; i++) {
    const username = `kesif${String(i + 1).padStart(2, '0')}`
    const auth = await signIn(username, people[i])
    users.push({ id: auth.user.id, username, token: auth.accessToken })
  }
  const mine = (await call('GET', '/projects', null, owner.accessToken)).body || []
  let org = null
  const projects = []
  for (const [index, demo] of DEMO.entries()) {
    let project = mine.find(p => p.name === demo.name && p.ownerId === owner.user.id)
    const fresh = !project
    if (fresh) project = (await call('POST', '/projects', { name: demo.name, slug: demo.slug, description: demo.description, visibility: 'PUBLIC', category: demo.category, tagline: demo.tagline }, owner.accessToken)).body
    if (!project?.id) throw new Error(`${demo.name} açılamadı: ${JSON.stringify(project)}`)
    const patch = { description: demo.description, visibility: 'PUBLIC', category: demo.category, tagline: demo.tagline }
    if (!project.iconUrl) patch.iconUrl = await upload(`${demo.slug}-ikon.png`, iconArt(256, [demo.palette[1], demo.palette[2]], index, index), owner.accessToken)
    if ((refresh || !project.coverUrl) && !demo.noCover) patch.coverUrl = await upload(`${demo.slug}-afis.png`, artwork(1200, 520, demo.palette, 1000 + index * 77, { motif: MOTIFS[demo.category] }), owner.accessToken)
    const updated = await call('PATCH', `/projects/${project.id}/admin`, patch, owner.accessToken)
    if (updated.status !== 200) throw new Error(`${demo.name} güncellenemedi: ${JSON.stringify(updated.body)}`)
    // Üyeler kodsuz katılır (Keşfet ucu).
    for (const u of users.slice(0, demo.members - 1)) await call('POST', `/discover/projects/${project.id}/join`, null, u.token)
    // Projenin kuruluşu: sahibin (ilk projeyle üye olduğu) kurulum kabı.
    org ??= (await call('GET', '/organizations', null, owner.accessToken)).body[0]
    const hub = `/organizations/${org.id}/projects/${project.id}/hub`
    const existing = (await call('GET', hub, null, owner.accessToken)).body?.posts || []
    for (const [n, post] of demo.posts.entries()) {
      const old = existing.find(p => p.title === post.title)
      if (old && !(refresh && post.cover)) continue
      const body = { content: post.content, title: post.title }
      if (post.cover) body.coverUrl = await upload(`${demo.slug}-haber-${n}.png`, artwork(1200, 600, post.cover, 5000 + index * 31 + n, { motif: NEWS_MOTIFS[(index + n) % NEWS_MOTIFS.length] }), owner.accessToken)
      const made = old ? await call('PATCH', `${hub}/posts/${old.id}`, body, owner.accessToken) : await call('POST', `${hub}/posts`, body, owner.accessToken)
      if (made.status !== 201 && made.status !== 200) throw new Error(`gönderi yazılamadı: ${JSON.stringify(made.body)}`)
    }
    projects.push({ ...demo, id: project.id })
    log(`  ${fresh ? 'açıldı' : 'güncellendi'}: ${demo.name} (${demo.category}, ${demo.members} üye)`)
  }
  return { owner, admin, org, projects, users, call }
}

if (process.argv[1]?.split('\\').join('/').endsWith('tools/discover-seed.mjs')) {
  const { api, stop } = await testServer()
  console.log('Keşfet örnek verisi → ' + api)
  const seeded = await seedDiscover(api, { refresh: process.argv.includes('--refresh') })
  const list = (await seeded.call('GET', '/discover/projects', null, seeded.users[0].token)).body
  console.log(`açık proje: ${list.projects.length}, kategoriler: ${JSON.stringify(list.categories)}`)
  stop()
  process.exit(0)
}
