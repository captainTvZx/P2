// Projeler / kanallar / kanal mesajları + eski /projects, /friends: NestJS ve Rust aynı senaryoyu
// (taze, etiketli varlıklarla) çalıştırır; cevaplar ve p2test/rustbot soketlerine gelen olaylar karşılaştırılır.
import { createRequire } from 'node:module'
import { NEST, startRust, call, login, both, same, check, sleep, done, mask } from './lib.mjs'

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')
async function listen(base, token, organizationId) {
  const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
  const events = []
  socket.onAny((event, payload) => { if (!event.startsWith('presence:')) events.push([event, payload]) })
  await new Promise((resolve, reject) => { socket.on('connect', resolve); socket.on('connect_error', reject) })
  await socket.timeout(5000).emitWithAck('subscribe', { organizationId })
  return { take: () => events.splice(0), close: () => socket.close() }
}

const rust = await startRust(4122)
const O = (await login(NEST, 'p2test')).accessToken
const M = (await login(NEST, 'rustbot')).accessToken
const org = (await call(NEST, 'GET', '/organizations', undefined, O)).body[0].id
const spike = (await call(NEST, 'GET', `/organizations/${org}/projects`, undefined, M)).body[0].id
const rnd = Math.random().toString(36).slice(2, 8)
const tags = { nest: 'n' + rnd, rust: 'r' + rnd }

async function scenario(base, tag, cross) {
  const out = []
  const so = await listen(base, O, org), sm = await listen(base, M, org)
  const P = `/organizations/${org}/projects`
  // Başka ajanların testleri aynı kuruluşta olay üretiyor: yalnız bu senaryonun varlıklarına ait olaylar
  // tutulur; aynı adımdaki olayların sırası NestJS'te yarışa bağlı olduğundan sıralanarak karşılaştırılır.
  const ids = new Set([tag])
  const own = list => list.filter(([, p]) => [...ids].some(id => JSON.stringify(p).includes(id))).map(e => [JSON.stringify(mask(e)), e]).sort((x, y) => x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0).map(x => x[1])
  async function step(name, method, path, body, token = O) {
    const res = await call(base, method, path, body, token)
    if (method !== 'GET') await sleep(150)
    if (method === 'POST' && res.status === 201 && res.body?.id && !path.includes('/messages')) ids.add(res.body.id)
    out.push([name, res, { p2test: own(so.take()), rustbot: own(sm.take()) }])
    return res
  }
  // Rust tarafında okunan veriyi NestJS ile de oku (Rust'ın yazdığı NestJS'te aynı görünmeli).
  async function crossRead(name, path, token = O) {
    if (!cross) return
    same('çapraz okuma: ' + name, await call(NEST, 'GET', path, undefined, token), await call(base, 'GET', path, undefined, token))
  }

  // ---- proje
  await step('proje doğrulama {}', 'POST', P, {})
  await step('proje kötü slug', 'POST', P, { name: 'Proje ' + tag, slug: 'Bad Slug' })
  await step('proje kötü motor', 'POST', P, { name: 'Proje ' + tag, slug: 'rt-' + tag, engine: 'X' })
  await step('proje üye yetkisiz', 'POST', P, { name: 'Proje ' + tag, slug: 'rt-' + tag }, M)
  const proj = (await step('proje oluştur', 'POST', P, { name: '  Proje ' + tag + '  ', slug: 'rt-' + tag, description: '  açıklama ', engine: 'GODOT' })).body
  await step('proje slug çakışması', 'POST', P, { name: 'Proje ' + tag, slug: 'rt-' + tag })
  await step('proje listesi', 'GET', P)
  await step('proje listesi (üye)', 'GET', P, undefined, M)
  const cats = (await step('kanallar (varsayılan)', 'GET', `${P}/${proj.id}/channels`)).body
  await crossRead('kanallar', `${P}/${proj.id}/channels`)
  const genel = cats[0].channels[1]
  await step('proje ayarı', 'PATCH', `${P}/${proj.id}`, { description: ' yeni ', defaultChannelId: genel.id, coverUrl: null, iconUrl: '' })
  await step('proje kapak geçersiz', 'PATCH', `${P}/${proj.id}`, { coverUrl: 'http://x/y.png' })
  await step('proje kapak geçerli', 'PATCH', `${P}/${proj.id}`, { coverUrl: '/uploads/3f2a1c9e-1b2c-4d5e-8f90-123456789abc.png' })
  await step('proje varsayılan kanal yok', 'PATCH', `${P}/${proj.id}`, { defaultChannelId: 'yok' })
  await step('proje kütüphane', 'PATCH', `${P}/${proj.id}`, { libraryEnabled: true, defaultChannelId: null })
  const before = out[out.length - 1][1].body.updatedAt
  const after = (await step('proje boş güncelleme', 'PATCH', `${P}/${proj.id}`, {})).body.updatedAt
  out.push(['boş güncelleme updatedAt değişti mi', { changed: before !== after }, {}])
  await step('proje üye güncelleyemez', 'PATCH', `${P}/${proj.id}`, { description: 'x' }, M)

  // ---- kategori ve kanal
  await step('kategori doğrulama', 'POST', `${P}/${proj.id}/channel-categories`, {})
  await step('kategori boş ad', 'POST', `${P}/${proj.id}/channel-categories`, { name: '   ' })
  const cat = (await step('kategori oluştur', 'POST', `${P}/${proj.id}/channel-categories`, { name: ' Kat ' + tag })).body
  await step('kategori çakışma', 'POST', `${P}/${proj.id}/channel-categories`, { name: 'kat ' + tag })
  await step('kanal kötü tür', 'POST', `${P}/${proj.id}/channels`, { categoryId: cat.id, name: 'k', type: 'BAD' })
  await step('kanal kategori yok', 'POST', `${P}/${proj.id}/channels`, { categoryId: 'yok', name: 'k', type: 'TEXT' })
  await step('kanal boş ad', 'POST', `${P}/${proj.id}/channels`, { categoryId: cat.id, name: ' ', type: 'TEXT' })
  const X = (await step('kanal oluştur', 'POST', `${P}/${proj.id}/channels`, { categoryId: cat.id, name: 'sohbet', type: 'TEXT' })).body
  await step('kanal çakışma', 'POST', `${P}/${proj.id}/channels`, { categoryId: cat.id, name: 'SOHBET', type: 'TEXT' })
  const F = (await step('forum oluştur', 'POST', `${P}/${proj.id}/channels`, { categoryId: cat.id, name: 'forum', type: 'FORUM' })).body
  const C = `${P}/${proj.id}/channels/${X.id}`, CF = `${P}/${proj.id}/channels/${F.id}`
  await step('kanal güncelle', 'PATCH', C, { name: 'Sohbet2', topic: '  konu  ' })
  await step('kanal konu boş', 'PATCH', C, { topic: '' })
  await step('kanal aynı ad küçük harf', 'PATCH', C, { name: 'sohbet2' })
  await step('kanal ad null', 'PATCH', C, { name: null })
  await step('kanal ad çakışma', 'PATCH', C, { name: 'FORUM' })
  await step('kanal doğrulama', 'PATCH', C, { topic: 5 })
  await step('kanal yok', 'PATCH', `${P}/${proj.id}/channels/yok`, { name: 'a' })

  // ---- mesajlar
  await step('mesaj doğrulama', 'POST', `${C}/messages`, { content: '' })
  const m1 = (await step('mesaj oluştur', 'POST', `${C}/messages`, { content: '  merhaba @rustbot  ' })).body
  await step('mesaj alıntı yok', 'POST', `${C}/messages`, { content: 'x', quotedMessageId: 'yok' })
  const m2 = (await step('mesaj alıntılı', 'POST', `${C}/messages`, { content: 'alıntı', quotedMessageId: m1.id })).body
  await step('mesaj @everyone + dosya', 'POST', `${C}/messages`, { content: '@everyone [[p2-file|a.png]]' })
  await step('mesajlar', 'GET', `${C}/messages`)
  await step('mesajlar limit=1', 'GET', `${C}/messages?limit=1`)
  await step('mesajlar before', 'GET', `${C}/messages?before=${encodeURIComponent(m2.createdAt)}`)
  await step('arama', 'GET', `${C}/search?q=MERH`)
  await step('arama kısa', 'GET', `${C}/search?q=a`)
  await step('arama boş', 'GET', `${C}/search`)
  await step('düzenle aynı', 'PATCH', `${C}/messages/${m1.id}`, { content: '  merhaba @rustbot  ' })
  await step('düzenle', 'PATCH', `${C}/messages/${m1.id}`, { content: 'düzenlendi' })
  await step('düzenle doğrulama', 'PATCH', `${C}/messages/${m1.id}`, {})
  await step('düzenle yok', 'PATCH', `${C}/messages/yok`, { content: 'a' })
  await step('geçmiş', 'GET', `${C}/messages/${m1.id}/history`)
  await step('geçmiş yok', 'GET', `${C}/messages/yok/history`)
  await step('tepki ekle', 'POST', `${C}/messages/${m1.id}/reactions`, { emoji: '👍' })
  await step('tepki doğrulama', 'POST', `${C}/messages/${m1.id}/reactions`, { emoji: 'a b' })
  await step('tepki kaldır', 'POST', `${C}/messages/${m1.id}/reactions`, { emoji: '👍' })
  await step('tepki 2', 'POST', `${C}/messages/${m1.id}/reactions`, { emoji: '🎉' })
  await step('tepki yok', 'POST', `${C}/messages/yok/reactions`, { emoji: '🎉' })
  const r1 = (await step('yanıt', 'POST', `${C}/messages/${m1.id}/replies`, { content: ' yanıt ' })).body
  await step('yanıta yanıt', 'POST', `${C}/messages/${r1.id}/replies`, { content: 'x' })
  await step('yanıtlar', 'GET', `${C}/messages/${m1.id}/replies`)
  await step('yanıtlar yok', 'GET', `${C}/messages/yok/replies`)
  await step('mesajlar (yanıtlı)', 'GET', `${C}/messages`)
  await crossRead('mesajlar', `${C}/messages`)
  await crossRead('yanıtlar', `${C}/messages/${m1.id}/replies`)
  const t1 = (await step('konu', 'POST', `${CF}/messages`, { content: 'konu' })).body
  await step('konu yanıtı', 'POST', `${CF}/topics/${t1.id}/replies`, { content: 'cevap' })
  await step('konu yanıtları', 'GET', `${CF}/topics/${t1.id}/replies`)
  await crossRead('konu yanıtları', `${CF}/topics/${t1.id}/replies`)
  await step('forum değil', 'GET', `${C}/topics/${m1.id}/replies`)
  await step('forum değil (yaz)', 'POST', `${C}/topics/${m1.id}/replies`, { content: 'x' })
  await step('konu yok', 'GET', `${CF}/topics/yok/replies`)
  await step('ilet', 'POST', `${C}/messages/${m1.id}/forward`, { targetChannelId: F.id })
  await step('ilet hedef yok', 'POST', `${C}/messages/${m1.id}/forward`, { targetChannelId: 'yok' })
  await step('ilet doğrulama', 'POST', `${C}/messages/${m1.id}/forward`, {})
  await step('yanıt sil', 'DELETE', `${C}/messages/${r1.id}`)
  await step('sil yok', 'DELETE', `${C}/messages/yok`)
  await step('okundu', 'POST', `${C}/read`)
  await step('okunmamış', 'GET', `${P}/unread`)
  await step('okunmamış (üye)', 'GET', `${P}/unread`, undefined, M)
  await step('kanallar (son)', 'GET', `${P}/${proj.id}/channels`)

  // ---- üye yetkileri
  await step('üye kanallar', 'GET', `${P}/${proj.id}/channels`, undefined, M)
  await step('üye proje sil', 'DELETE', `${P}/${proj.id}`, undefined, M)
  await step('üye mesajlar', 'GET', `${C}/messages`, undefined, M)

  // ---- silme
  await step('dolu kategori sil', 'DELETE', `${P}/${proj.id}/channel-categories/${cat.id}`)
  await step('kanal sil', 'DELETE', C)
  await step('forum sil', 'DELETE', CF)
  await step('kategori sil', 'DELETE', `${P}/${proj.id}/channel-categories/${cat.id}`)
  await step('kategori yok', 'DELETE', `${P}/${proj.id}/channel-categories/${cat.id}`)
  await step('proje sil', 'DELETE', `${P}/${proj.id}`)
  await step('silinmiş proje kanalları', 'GET', `${P}/${proj.id}/channels`)
  await step('proje tekrar sil', 'DELETE', `${P}/${proj.id}`)

  // ---- ortak proje (spike-2): üyeyle etkileşim, etkinlik kayıtları
  const S = `${P}/${spike}`
  const K = (await step('S kategori', 'POST', `${S}/channel-categories`, { name: 'RT ' + tag })).body
  const Y = (await step('S kanal', 'POST', `${S}/channels`, { categoryId: K.id, name: 'rt-' + tag, type: 'TEXT' })).body
  const SY = `${S}/channels/${Y.id}`
  const b1 = (await step('S üye mesajı (@p2test)', 'POST', `${SY}/messages`, { content: 'selam @p2test' }, M)).body
  await step('S sahibin yanıtı', 'POST', `${SY}/messages/${b1.id}/replies`, { content: 'yanıt @rustbot' })
  const o1 = (await step('S sahip mesajı', 'POST', `${SY}/messages`, { content: 'sahip' })).body
  await step('S üye tepkisi', 'POST', `${SY}/messages/${o1.id}/reactions`, { emoji: '🔥' }, M)
  await step('S üye başkasını silemez', 'DELETE', `${SY}/messages/${o1.id}`, undefined, M)
  await step('S üye başkasını düzenleyemez', 'PATCH', `${SY}/messages/${o1.id}`, { content: 'x' }, M)
  await step('S üye @everyone yetkisiz', 'POST', `${SY}/messages`, { content: '@here bakın' }, M)
  await step('S üye kütüphane yetkisiz', 'PATCH', S, { libraryEnabled: true }, M)
  await step('S üye mesajlar', 'GET', `${SY}/messages`, undefined, M)
  await crossRead('S üye mesajlar', `${SY}/messages`, M)
  await step('S üye kendi mesajını siler', 'DELETE', `${SY}/messages/${b1.id}`, undefined, M)
  await step('S sahip mesajı siler', 'DELETE', `${SY}/messages/${o1.id}`)
  await step('S kanal sil', 'DELETE', SY)
  await step('S kategori sil', 'DELETE', `${S}/channel-categories/${K.id}`)

  // ---- genel
  await step('jetonsuz', 'GET', P, undefined, null)
  await step('yabancı kuruluş', 'GET', '/organizations/yok/projects')
  await step('bilinmeyen yöntem', 'GET', `${P}/${spike}`)
  so.close(); sm.close()
  return out
}

const nest = await scenario(NEST, tags.nest, false)
const mine = await scenario(rust.base, tags.rust, true)
const replace = [[tags.nest, '<tag>'], [tags.rust, '<tag>']]
for (let i = 0; i < Math.max(nest.length, mine.length); i++) {
  const [name, a, ea] = nest[i] || ['?', null, null]
  const [, b, eb] = mine[i] || ['?', null, null]
  same(name, a, b, replace)
  same(name + ' (olaylar)', ea, eb, replace)
}
await both('eski /projects', rust.base, 'GET', '/projects', undefined, { nest: O })
await both('eski /projects (üye)', rust.base, 'GET', '/projects', undefined, { nest: M })
await both('eski /friends', rust.base, 'GET', '/friends', undefined, { nest: O })
await both('eski /friends (üye)', rust.base, 'GET', '/friends', undefined, { nest: M })
await both('eski /friends jetonsuz', rust.base, 'GET', '/friends')
check('adım sayısı', nest.length === mine.length, `${nest.length} / ${mine.length}`)
rust.stop()
done()
