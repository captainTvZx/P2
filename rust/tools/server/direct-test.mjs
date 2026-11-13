// Birebir sohbet uçları (direct.ts): aynı senaryo NestJS ve Rust'ta ayrı taze kullanıcılarla çalışır,
// maskeli cevaplar ve gerçek zamanlı olaylar karşılaştırılır.
import { createRequire } from 'node:module'
import { NEST, startRust, call, login, same, check, done, sleep } from './lib.mjs'

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')

const rust = await startRust(4123)
const owner = await login(NEST, 'p2test')
const org = (await call(NEST, 'GET', '/organizations', undefined, owner.accessToken)).body[0]

// Kuruluşa katılmış taze kullanıcı (her koşu için ayrı sohbet).
export async function freshUser(prefix) {
  const tag = prefix + Math.random().toString(36).slice(2, 8)
  const reg = await call(NEST, 'POST', '/auth/register', { email: `${tag}@example.com`, username: tag, displayName: 'Kişi ' + tag, password: 'Parola123', passwordConfirm: 'Parola123' })
  const session = reg.body.accessToken ? reg.body : await login(NEST, tag)
  await call(NEST, 'POST', '/organizations/join', { code: org.inviteCode }, session.accessToken)
  return { token: session.accessToken, id: session.user.id, tag, name: 'Kişi ' + tag }
}

function listen(base, token) {
  return new Promise(resolve => {
    const events = []
    const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
    socket.onAny((event, payload) => { if (/^direct:/.test(event)) events.push([event, payload]) })
    socket.on('connect', () => resolve({ socket, take: () => events.splice(0) }))
  })
}

async function scenario(base, B) {
  const A = { token: owner.accessToken, id: owner.user.id }
  const la = await listen(base, A.token), lb = await listen(base, B.token)
  const out = []
  const ev = async name => { await sleep(250); out.push([name + ' olay A', la.take()], [name + ' olay B', lb.take()]) }
  const r = async (name, method, path, body, token = A.token) => { const res = await call(base, method, path, body, token); out.push([name, res]); return res }
  const root = `/organizations/${org.id}/direct`

  await r('jetonsuz', 'GET', root, undefined, null)
  await r('yabancı kuruluş', 'GET', `/organizations/yok/direct`)
  await r('başlat doğrulama', 'POST', root, {})
  await r('başlat sayı', 'POST', root, { userId: 5 })
  await r('kendinle', 'POST', root, { userId: A.id })
  await r('üye değil', 'POST', root, { userId: 'yok' })
  const conv = (await r('başlat', 'POST', root, { userId: B.id })).body
  await ev('başlat')
  await r('yeniden başlat', 'POST', root, { userId: A.id }, B.token)
  await r('liste B', 'GET', root, undefined, B.token)
  await r('mesajlar boş', 'GET', `${root}/${conv.id}/messages`)
  await r('olmayan sohbet', 'GET', `${root}/yok/messages`)
  await r('mesaj boş', 'POST', `${root}/${conv.id}/messages`, { content: '' })
  await r('mesaj uzun', 'POST', `${root}/${conv.id}/messages`, { content: 'x'.repeat(4001) })
  await r('alıntı sayı', 'POST', `${root}/${conv.id}/messages`, { content: 'a', quotedMessageId: 3 })
  await r('alıntı yok', 'POST', `${root}/${conv.id}/messages`, { content: 'a', quotedMessageId: 'yok' })
  const m1 = (await r('mesaj', 'POST', `${root}/${conv.id}/messages`, { content: `  merhaba @${B.tag}  ` })).body
  await ev('mesaj')
  await sleep(5)
  const m2 = (await r('alıntılı mesaj', 'POST', `${root}/${conv.id}/messages`, { content: 'cevap', quotedMessageId: m1.id }, B.token)).body
  await ev('alıntılı')
  await r('alıntı boş dizgi', 'POST', `${root}/${conv.id}/messages`, { content: 'üç', quotedMessageId: '' })
  await ev('üçüncü')
  const listA = await call(base, 'GET', root, undefined, A.token)
  out.push(['liste A (bu sohbet)', { status: listA.status, body: listA.body.find(c => c.id === conv.id) }])
  await r('liste B 2', 'GET', root, undefined, B.token)
  await r('mesajlar', 'GET', `${root}/${conv.id}/messages`)
  await r('mesajlar limit=1', 'GET', `${root}/${conv.id}/messages?limit=1`)
  await r('mesajlar limit=abc', 'GET', `${root}/${conv.id}/messages?limit=abc`)
  await r('mesajlar before', 'GET', `${root}/${conv.id}/messages?before=${encodeURIComponent(m2.createdAt)}`)
  await r('mesajlar before geçersiz', 'GET', `${root}/${conv.id}/messages?before=xyz&limit=0`)
  await r('düzenle başkası', 'PATCH', `${root}/${conv.id}/messages/${m1.id}`, { content: 'x' }, B.token)
  await r('düzenle doğrulama', 'PATCH', `${root}/${conv.id}/messages/${m1.id}`, {})
  await r('düzenle', 'PATCH', `${root}/${conv.id}/messages/${m1.id}`, { content: ' düzeltildi ' })
  await ev('düzenle')
  await r('tepki doğrulama', 'POST', `${root}/${conv.id}/messages/${m1.id}/reactions`, { emoji: 'x'.repeat(25) })
  await r('tepki olmayan', 'POST', `${root}/${conv.id}/messages/yok/reactions`, { emoji: '👍' })
  await r('tepki A', 'POST', `${root}/${conv.id}/messages/${m1.id}/reactions`, { emoji: ' 👍 ' })
  await sleep(5)
  await r('tepki B', 'POST', `${root}/${conv.id}/messages/${m1.id}/reactions`, { emoji: '👍' }, B.token)
  await sleep(5)
  await r('tepki B kalp', 'POST', `${root}/${conv.id}/messages/${m1.id}/reactions`, { emoji: '❤️' }, B.token)
  await r('tepki A geri', 'POST', `${root}/${conv.id}/messages/${m1.id}/reactions`, { emoji: '👍' })
  await ev('tepki')
  await r('okundu', 'POST', `${root}/${conv.id}/read`, undefined, B.token)
  await r('okundu olmayan', 'POST', `${root}/yok/read`)
  await r('sil başkası', 'DELETE', `${root}/${conv.id}/messages/${m1.id}`, undefined, B.token)
  await r('sil olmayan', 'DELETE', `${root}/${conv.id}/messages/yok`)
  await r('sil', 'DELETE', `${root}/${conv.id}/messages/${m1.id}`)
  await r('sil (sahip, başkasının)', 'DELETE', `${root}/${conv.id}/messages/${m2.id}`)
  await ev('sil')
  await r('mesajlar son', 'GET', `${root}/${conv.id}/messages`, undefined, B.token)
  la.socket.close(); lb.socket.close()
  return out
}

const nb = await freshUser('dmn'), rb = await freshUser('dmr')
const a = await scenario(NEST, nb)
const b = await scenario(rust.base, rb)
// Çevrimiçi durumu (status) iki sunucunun soket bağlantılarına göre değişir; karşılaştırma dışı.
const replace = [[nb.tag, '<tag>'], [rb.tag, '<tag>'], ['OFFLINE', '<status>'], ['ONLINE', '<status>'], ['AWAY', '<status>'], ['BUSY', '<status>']]
const count = list => list.filter(([n]) => n.includes(' olay ')).reduce((t, [, e]) => t + e.length, 0)
check('olaylar geldi', count(a) > 0 && count(a) === count(b), `${count(a)} / ${count(b)}`)
check('adım sayısı', a.length === b.length, `${a.length} / ${b.length}`)
for (let i = 0; i < a.length; i++) same(a[i][0], a[i][1], b[i][1], replace)

// Rust'ın yazdığını NestJS okur.
const conv = (await call(rust.base, 'GET', `/organizations/${org.id}/direct`, undefined, rb.token)).body[0]
const x = await call(NEST, 'GET', `/organizations/${org.id}/direct/${conv.id}/messages`, undefined, rb.token)
const y = await call(rust.base, 'GET', `/organizations/${org.id}/direct/${conv.id}/messages`, undefined, rb.token)
same('Rust yazdı, NestJS okudu', x, y)
rust.stop()
done()
