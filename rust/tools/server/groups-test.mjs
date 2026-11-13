// Proje sohbet grupları (groups.ts): aynı senaryo NestJS ve Rust'ta ayrı taze gruplarla çalışır,
// maskeli cevaplar ve gerçek zamanlı olaylar (etkinlik kayıtları dahil) karşılaştırılır.
import { createRequire } from 'node:module'
import { NEST, startRust, call, login, same, check, done, sleep } from './lib.mjs'

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')

const rust = await startRust(4123)
const owner = await login(NEST, 'p2test'), bot = await login(NEST, 'rustbot')
const org = (await call(NEST, 'GET', '/organizations', undefined, owner.accessToken)).body[0]
const project = (await call(NEST, 'GET', `/organizations/${org.id}/projects`, undefined, bot.accessToken)).body[0]

function listen(base, token) {
  return new Promise(resolve => {
    const events = []
    const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
    // Başka modüllerin (görevler vb.) eşzamanlı testlerinden gelen etkinlikler süzülür.
    socket.onAny((event, payload) => { if (event === 'activity:new' && !payload?.activity?.groupId) return; if (/^(group|activity):/.test(event)) events.push([event, payload]) })
    socket.on('connect', () => resolve({ socket, take: () => events.splice(0) }))
  })
}

async function scenario(base, tag, cross) {
  const A = owner.accessToken, B = bot.accessToken
  const la = await listen(base, A), lb = await listen(base, B)
  const out = []
  const ev = async name => { await sleep(300); out.push([name + ' olay A', la.take()], [name + ' olay B', lb.take()]) }
  const r = async (name, method, path, body, token = A) => { const res = await call(base, method, path, body, token); out.push([name, res]); return res }
  const only = async (name, id, token) => { const res = await call(base, 'GET', `${root}?projectId=${project.id}`, undefined, token); out.push([name, { status: res.status, body: res.body.filter(g => g.id === id) }]) }
  const root = `/organizations/${org.id}/groups`

  await r('jetonsuz', 'GET', root, undefined, null)
  await r('yabancı kuruluş', 'GET', `/organizations/yok/groups`)
  await r('liste projesiz', 'GET', root)
  await r('liste olmayan proje', 'GET', `${root}?projectId=yok`)
  await r('oluştur doğrulama', 'POST', root, {})
  await r('oluştur boş dizi', 'POST', root, { projectId: project.id, userIds: [] })
  await r('oluştur ad boş', 'POST', root, { projectId: project.id, name: '', userIds: ['x'] })
  await r('oluştur 51', 'POST', root, { projectId: project.id, userIds: Array.from({ length: 51 }, (_, i) => 'u' + i) })
  await r('oluştur sayı', 'POST', root, { projectId: project.id, userIds: [1] })
  await r('oluştur olmayan proje', 'POST', root, { projectId: 'yok', userIds: ['x'] })
  const g = (await r('oluştur tek', 'POST', root, { projectId: project.id, userIds: ['yok', owner.user.id] })).body
  await ev('oluştur')
  await r('üye ekle doğrulama', 'POST', `${root}/${g.id}/members`, { userIds: 'x' })
  await r('üye ekle olmayan grup', 'POST', `${root}/yok/members`, { userIds: ['x'] })
  await r('üye ekle (kimse)', 'POST', `${root}/${g.id}/members`, { userIds: ['yok'] })
  await ev('üye ekle boş')
  await r('üye ekle', 'POST', `${root}/${g.id}/members`, { userIds: [bot.user.id, bot.user.id] })
  await ev('üye ekle')
  await r('mesajlar', 'GET', `${root}/${g.id}/messages`, undefined, B)
  await r('mesaj doğrulama', 'POST', `${root}/${g.id}/messages`, { content: '' })
  const m1 = (await r('mesaj', 'POST', `${root}/${g.id}/messages`, { content: `selam @rustbot ${tag}`, quotedMessageId: 'yok' })).body
  await ev('mesaj')
  await sleep(5)
  const m2 = (await r('alıntılı mesaj', 'POST', `${root}/${g.id}/messages`, { content: ' alıntı ', quotedMessageId: m1.id }, B)).body
  await ev('alıntılı')
  await r('yanıt doğrulama', 'POST', `${root}/${g.id}/messages/${m1.id}/replies`, {})
  await r('yanıt olmayan', 'POST', `${root}/${g.id}/messages/yok/replies`, { content: 'x' })
  const rep = (await r('yanıt', 'POST', `${root}/${g.id}/messages/${m1.id}/replies`, { content: 'yanıt @p2test' }, B)).body
  await ev('yanıt')
  await r('yanıta yanıt', 'POST', `${root}/${g.id}/messages/${rep.id}/replies`, { content: 'x' })
  await r('yanıtlar', 'GET', `${root}/${g.id}/messages/${m1.id}/replies`, undefined, B)
  await r('tepki doğrulama', 'POST', `${root}/${g.id}/messages/${m1.id}/reactions`, {})
  await r('tepki olmayan', 'POST', `${root}/${g.id}/messages/yok/reactions`, { emoji: '👍' })
  await r('tepki B', 'POST', `${root}/${g.id}/messages/${m1.id}/reactions`, { emoji: '👍' }, B)
  await sleep(5)
  await r('tepki A', 'POST', `${root}/${g.id}/messages/${m1.id}/reactions`, { emoji: '👍' })
  await r('tepki B geri', 'POST', `${root}/${g.id}/messages/${m1.id}/reactions`, { emoji: ' 👍' }, B)
  await r('tepki yanıta', 'POST', `${root}/${g.id}/messages/${rep.id}/reactions`, { emoji: '🎉' })
  await ev('tepki')
  await r('düzenle başkası', 'PATCH', `${root}/${g.id}/messages/${m1.id}`, { content: 'x' }, B)
  await r('düzenle olmayan', 'PATCH', `${root}/${g.id}/messages/yok`, { content: 'x' })
  await r('düzenle', 'PATCH', `${root}/${g.id}/messages/${m1.id}`, { content: ' düzeltildi ' })
  await ev('düzenle')
  await r('mesajlar 2', 'GET', `${root}/${g.id}/messages`)
  await r('mesajlar limit=1', 'GET', `${root}/${g.id}/messages?limit=1`)
  await r('mesajlar before', 'GET', `${root}/${g.id}/messages?before=${encodeURIComponent(m2.createdAt)}`)
  await only('liste A', g.id, A)
  await only('liste B', g.id, B)
  await r('okundu', 'POST', `${root}/${g.id}/read`, undefined, B)
  await r('okundu olmayan', 'POST', `${root}/yok/read`, undefined, B)
  await only('liste B okunduktan sonra', g.id, B)
  await r('sil başkası', 'DELETE', `${root}/${g.id}/messages/${m1.id}`, undefined, B)
  await r('sil olmayan', 'DELETE', `${root}/${g.id}/messages/yok`)
  await r('sil (sahip, başkasının)', 'DELETE', `${root}/${g.id}/messages/${m2.id}`)
  await ev('sil')
  if (cross) await cross(g.id)
  await r('çıkar (kurucu değil)', 'DELETE', `${root}/${g.id}/members/${owner.user.id}`, undefined, B)
  await r('çıkar kendini', 'DELETE', `${root}/${g.id}/members/${owner.user.id}`)
  await r('çıkar olmayan', 'DELETE', `${root}/${g.id}/members/yok`)
  await r('çıkar', 'DELETE', `${root}/${g.id}/members/${bot.user.id}`)
  await ev('çıkar')
  await r('çıkarılan okur', 'GET', `${root}/${g.id}/messages`, undefined, B)
  await only('liste A çıkardıktan sonra', g.id, A)
  // Elle verilmiş adlı grup: ad üye değişince korunur; son kişi ayrılınca grup silinir.
  const g2 = (await r('oluştur adlı', 'POST', root, { projectId: project.id, name: `  Ekip ${tag}  `, userIds: [bot.user.id] })).body
  await ev('oluştur adlı')
  await r('ayrıl B', 'POST', `${root}/${g2.id}/leave`, undefined, B)
  await ev('ayrıl B')
  await only('liste A adlı', g2.id, A)
  await r('ayrıl A (son)', 'POST', `${root}/${g2.id}/leave`)
  await ev('ayrıl son')
  await r('ayrıl A (ilk grup)', 'POST', `${root}/${g.id}/leave`)
  await r('silinen grup', 'GET', `${root}/${g.id}/messages`)
  await ev('ayrıl ilk')
  la.socket.close(); lb.socket.close()
  return out
}

const tn = 'gn' + Math.random().toString(36).slice(2, 7), tr = 'gr' + Math.random().toString(36).slice(2, 7)
const a = await scenario(NEST, tn)
const b = await scenario(rust.base, tr, async id => {
  for (const path of [`/messages`, `/messages?limit=2`]) {
    const x = await call(NEST, 'GET', `/organizations/${org.id}/groups/${id}${path}`, undefined, owner.accessToken)
    const y = await call(rust.base, 'GET', `/organizations/${org.id}/groups/${id}${path}`, undefined, owner.accessToken)
    same('Rust yazdı, NestJS okudu ' + path, x, y)
  }
  const x = await call(NEST, 'GET', `/organizations/${org.id}/groups?projectId=${project.id}`, undefined, owner.accessToken)
  const y = await call(rust.base, 'GET', `/organizations/${org.id}/groups?projectId=${project.id}`, undefined, owner.accessToken)
  same('tüm liste iki sunucuda', x, y, [['OFFLINE', '<status>'], ['ONLINE', '<status>'], ['AWAY', '<status>'], ['BUSY', '<status>']])
})
const replace = [[tn, '<tag>'], [tr, '<tag>'], ['OFFLINE', '<status>'], ['ONLINE', '<status>'], ['AWAY', '<status>'], ['BUSY', '<status>']]
const count = list => list.filter(([n]) => n.includes(' olay ')).reduce((t, [, e]) => t + e.length, 0)
check('olaylar geldi', count(a) > 0 && count(a) === count(b), `${count(a)} / ${count(b)}`)
check('adım sayısı', a.length === b.length, `${a.length} / ${b.length}`)
for (let i = 0; i < a.length; i++) same(a[i][0], a[i][1], b[i][1], replace)
rust.stop()
done()
