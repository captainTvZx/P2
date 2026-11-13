// Belirlenimci (deterministic) parite verisi: test kuruluşunun ilk projesinde "GENEL › parite" kanalı.
// Yalnız kanal yoksa ya da mesajları eksikse yazar; var olan veriye dokunmaz. Ekranlar bu kanalı açar
// ki iki istemci de aynı mesajları (biçim, yanıt, tepki, ek, bahsetme, kod) çizsin.
// DİKKAT (parite ajanları): bu kanalı değiştiren adım (mesaj gönderme/silme/tepki) yazmayın — başka ekranların
// görüntüsü bozulur. Yazan senaryolar için kendi kanalınızı açın.
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { REPO, API, log } from './common.mjs'

const { PNG } = createRequire(join(REPO, 'package.json'))('pngjs')
export const SEED_CHANNEL = 'parite'

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  const json = await r.json().catch(() => null)
  if (!r.ok) throw new Error(`${method} ${path} → ${r.status} ${JSON.stringify(json)}`)
  return json
}

function samplePng() {
  const png = new PNG({ width: 320, height: 180 })
  for (let y = 0; y < 180; y++) for (let x = 0; x < 320; x++) {
    const o = (y * 320 + x) * 4
    png.data[o] = 30 + x / 4; png.data[o + 1] = 60 + y / 2; png.data[o + 2] = 160; png.data[o + 3] = 255
  }
  return 'data:image/png;base64,' + PNG.sync.write(png).toString('base64')
}

export async function seed() {
  const me = await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })
  const bot = await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })
  const org = (await call('GET', '/organizations', null, me.accessToken))[0]
  const project = (await call('GET', `/organizations/${org.id}/projects`, null, me.accessToken))[0]
  const base = `/organizations/${org.id}/projects/${project.id}`
  let categories = await call('GET', `${base}/channels`, null, me.accessToken)
  let channel = categories.flatMap(c => c.channels).find(c => c.name === SEED_CHANNEL)
  if (!channel) {
    const genel = categories.find(c => c.name === 'GENEL') || categories[0]
    channel = await call('POST', `${base}/channels`, { categoryId: genel.id, name: SEED_CHANNEL, type: 'TEXT' }, me.accessToken)
    log(`tohum: #${SEED_CHANNEL} kanalı oluşturuldu`)
  }
  const chan = `${base}/channels/${channel.id}`
  const existing = await call('GET', `${chan}/messages`, null, me.accessToken)
  const rows = Array.isArray(existing) ? existing : existing.messages || existing.items || []
  if (rows.length >= 7) return { org, project, channel, me, bot }
  const say = (who, content, extra = {}) => call('POST', `${chan}/messages`, { content, ...extra }, who.accessToken)
  const m1 = await say(bot, 'Merhaba **ekip**! #parite kanalına hoş geldiniz. _İtalik_, ~~üstü çizili~~ ve bir bağlantı: https://example.com/p2')
  const m2 = await say(me, '@rustbot şu parçayı inceler misin? Satır içi `kod` da var:\n```js\nconst hedef = 42\nconsole.log(`değer: ${hedef}`)\n```')
  await say(bot, 'Baktım, sorun yok 👍 Bir de liste:\n- birinci madde\n- ikinci madde\n- üçüncü madde', { quotedMessageId: m2.id })
  await call('POST', `${chan}/messages/${m1.id}/reactions`, { emoji: '👍' }, me.accessToken)
  await call('POST', `${chan}/messages/${m1.id}/reactions`, { emoji: '👍' }, bot.accessToken)
  await call('POST', `${chan}/messages/${m1.id}/reactions`, { emoji: '🎉' }, me.accessToken)
  await call('POST', `${chan}/messages/${m2.id}/replies`, { content: 'İş parçacığında ilk yanıt.' }, bot.accessToken)
  const up = await call('POST', '/uploads', { name: 'parite-ornek.png', type: 'image/png', data: samplePng() }, me.accessToken)
  const marker = `[[p2-file|${encodeURIComponent(up.name)}|${encodeURIComponent(up.type)}|${up.size}|${up.url}]]`
  await say(me, 'Ekran görüntüsü ekte.\n' + marker)
  const doc = await call('POST', '/uploads', { name: 'surum-notlari.txt', type: 'text/plain', data: 'data:text/plain;base64,' + Buffer.from('P2 parite sürüm notları\n').toString('base64') }, bot.accessToken)
  await say(bot, `[[p2-file|${encodeURIComponent(doc.name)}|${encodeURIComponent(doc.type)}|${doc.size}|${doc.url}]]`)
  await say(me, 'Uzun bir mesaj: Lorem ipsum yerine Türkçe — çalışma alanındaki satır kaydırmasını, satır yüksekliğini ve harf aralığını karşılaştırmak için yeterince uzun bir paragraf. Ğ ü ş ı ö ç İ harfleri de burada.')
  log(`tohum: #${SEED_CHANNEL} mesajları yazıldı`)
  return { org, project, channel, me, bot }
}

if (process.argv[1] && process.argv[1].endsWith('seed.mjs')) seed().then(r => log(`hazır: ${r.org.name} / ${r.project.name} / #${r.channel.name}`)).catch(e => { console.error(e); process.exit(1) })
