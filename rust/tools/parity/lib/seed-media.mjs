// L ajanı: sohbet medyası/bağlantı önizlemesi/forum ekranları için belirlenimci tohum. #parite'ye DOKUNMAZ;
// ilk projede "PARİTE EK" kategorisi altında üç kanal açar (yoksa) ve bir kez doldurur:
//   #parite-baglanti  — uzak resim bağlantısı (test sunucusunun kendi /uploads adresi) + YouTube bağlantısı
//   #parite-medya     — PDF, ses (WAV) ve video (MP4) ekleri
//   #parite-forum     — forum kanalı: iki konu, ilk konuya iki yanıt
// Not: YouTube kartı küçük resmi i.ytimg.com'dan ister (Electron da aynı isteği yapar).
import { API, log } from './common.mjs'

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  const json = await r.json().catch(() => null)
  if (!r.ok) throw new Error(`${method} ${path} → ${r.status} ${JSON.stringify(json)}`)
  return json
}

const marker = f => `[[p2-file|${encodeURIComponent(f.name)}|${encodeURIComponent(f.type)}|${f.size}|${f.url}]]`
const dataUrl = (type, buf) => `data:${type};base64,` + Buffer.from(buf).toString('base64')

/** Tek sayfalık, başlık + iki satırlı PDF (xref uzaklıkları hesaplı). */
export function samplePdf() {
  const text = 'BT /F1 28 Tf 72 720 Td (P2 parite PDF) Tj ET\nBT /F1 14 Tf 72 690 Td (Ilk sayfa onizlemesi - Windows.Data.Pdf / pdf.js) Tj ET\n0.17 0.48 1 rg 72 600 300 40 re f'
  const objs = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${Buffer.byteLength(text)} >>\nstream\n${text}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ]
  let out = '%PDF-1.4\n'
  const offsets = []
  objs.forEach((o, i) => { offsets.push(Buffer.byteLength(out)); out += `${i + 1} 0 obj\n${o}\nendobj\n` })
  const xref = Buffer.byteLength(out)
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n` + offsets.map(o => String(o).padStart(10, '0') + ' 00000 n \n').join('')
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`
  return Buffer.from(out, 'latin1')
}

/** 0,5 sn sessiz WAV (8 kHz, mono, 16 bit). */
function sampleWav() {
  const n = 4000, b = Buffer.alloc(44 + n * 2)
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVE', 8); b.write('fmt ', 12)
  b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(8000, 24)
  b.writeUInt32LE(16000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write('data', 36); b.writeUInt32LE(n * 2, 40)
  return b
}

export async function seedMedia() {
  const me = await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })
  const bot = await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })
  const org = (await call('GET', '/organizations', null, me.accessToken))[0]
  const project = (await call('GET', `/organizations/${org.id}/projects`, null, me.accessToken))[0]
  const base = `/organizations/${org.id}/projects/${project.id}`
  let categories = await call('GET', `${base}/channels`, null, me.accessToken)
  let category = categories.find(c => c.name === 'PARİTE EK')
  if (!category) {
    category = await call('POST', `${base}/channel-categories`, { name: 'PARİTE EK' }, me.accessToken)
    categories = await call('GET', `${base}/channels`, null, me.accessToken)
  }
  const ensure = async (name, type) => {
    const found = categories.flatMap(c => c.channels).find(c => c.name === name)
    return found || call('POST', `${base}/channels`, { categoryId: category.id, name, type }, me.accessToken)
  }
  const links = await ensure('parite-baglanti', 'TEXT')
  const media = await ensure('parite-medya', 'TEXT')
  const forum = await ensure('parite-forum', 'FORUM')
  const count = async ch => { const r = await call('GET', `${base}/channels/${ch.id}/messages`, null, me.accessToken); return (Array.isArray(r) ? r : r.messages || r.items || []).length }
  const say = (who, ch, content) => call('POST', `${base}/channels/${ch.id}/messages`, { content }, who.accessToken)
  const upload = (who, name, type, buf) => call('POST', '/uploads', { name, type, data: dataUrl(type, buf) }, who.accessToken)

  if (await count(links) < 2) {
    // Uzak resim: test sunucusunun kendi yüklemesi (dış istek yok).
    const { PNG } = await import('pngjs').catch(() => ({ PNG: null }))
    let png
    if (PNG) {
      const img = new PNG({ width: 480, height: 240 })
      for (let y = 0; y < 240; y++) for (let x = 0; x < 480; x++) { const o = (y * 480 + x) * 4; img.data[o] = 200 - x / 4; img.data[o + 1] = 80 + y / 3; img.data[o + 2] = 120 + x / 6; img.data[o + 3] = 255 }
      png = PNG.sync.write(img)
    }
    const up = await upload(bot, 'uzak-resim.png', 'image/png', png || samplePdf())
    await say(bot, links, `Uzak resim bağlantısı: ${API}${up.url}`)
    await say(me, links, 'Tanıtım videosu: https://www.youtube.com/watch?v=dQw4w9WgXcQ')
    log('tohum: #parite-baglanti yazıldı')
  }
  if (await count(media) < 3) {
    const pdf = await upload(me, 'parite-belge.pdf', 'application/pdf', samplePdf())
    await say(me, media, 'Belge ekte.\n' + marker(pdf))
    const wav = await upload(bot, 'sesli-not.wav', 'audio/wav', sampleWav())
    await say(bot, media, marker(wav))
    const mp4 = await upload(me, 'tanitim.mp4', 'video/mp4', Buffer.from('p2-parite-video'))
    await say(me, media, marker(mp4))
    log('tohum: #parite-medya yazıldı')
  }
  if (await count(forum) < 2) {
    const t1 = await say(me, forum, 'Sürüm 1.0 yol haritası')
    await say(bot, forum, 'Arayüz geri bildirimleri')
    const reply = (who, content) => call('POST', `${base}/channels/${forum.id}/topics/${t1.id}/replies`, { content }, who.accessToken)
    await reply(bot, 'İlk taslak hazır, **göz atabilir misin**?')
    await reply(me, 'Baktım, birkaç notum var.')
    log('tohum: #parite-forum yazıldı')
  }
  return { org, project, links, media, forum }
}

if (process.argv[1] && process.argv[1].endsWith('seed-media.mjs')) seedMedia().then(r => log(`hazır: #${r.links.name} #${r.media.name} #${r.forum.name}`)).catch(e => { console.error(e); process.exit(1) })
