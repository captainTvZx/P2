// Proje dokümanları (documents.ts): aynı senaryo NestJS ve Rust'ta ayrı taze doküman/klasörlerle
// çalışır, maskeli cevaplar ve gerçek zamanlı olaylar (etiket etkinlikleri dahil) karşılaştırılır.
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
    socket.onAny((event, payload) => { if (event === 'activity:new' && !payload?.activity?.documentId) return; if (/^(document:(created|updated|deleted|access|permissions|revoked)|activity:new)$/.test(event)) events.push([event, payload]) })
    socket.on('connect', () => resolve({ socket, take: () => events.splice(0) }))
  })
}

async function scenario(base, tag, cross) {
  const A = owner.accessToken, B = bot.accessToken
  const la = await listen(base, A), lb = await listen(base, B)
  const out = []
  const ev = async name => { await sleep(300); out.push([name + ' olay A', la.take()], [name + ' olay B', lb.take()]) }
  const r = async (name, method, path, body, token = A) => { const res = await call(base, method, path, body, token); out.push([name, res]); return res }
  const root = `/organizations/${org.id}/projects/${project.id}/documents`
  const mine = new Set()
  const only = async (name, path, token = A) => { const res = await call(base, 'GET', path, undefined, token); out.push([name, { status: res.status, body: Array.isArray(res.body) ? res.body.filter(d => mine.has(d.id)) : res.body }]) }

  await r('jetonsuz', 'GET', root, undefined, null)
  await r('olmayan proje', 'GET', `/organizations/${org.id}/projects/yok/documents`)
  await r('yabancı kuruluş', 'GET', `/organizations/yok/projects/${project.id}/documents`)
  await r('klasör doğrulama', 'POST', `${root}/folders`, { name: '' })
  await r('klasör boşluk', 'POST', `${root}/folders`, { name: '   ' })
  const f1 = (await r('klasör', 'POST', `${root}/folders`, { name: ` Klasör ${tag} ` })).body
  mine.add(f1.id)
  await r('klasör aynı (büyük harf)', 'POST', `${root}/folders`, { name: ` klasör ${tag.toUpperCase()}` })
  const f0 = await r('klasör aynı (Ö büyük harf)', 'POST', `${root}/folders`, { name: `KLASÖR ${tag}` })
  if (f0.body?.id) mine.add(f0.body.id)
  const f2 = (await r('klasör', 'POST', `${root}/folders`, { name: `aXb ${tag}` })).body
  mine.add(f2.id)
  const f3 = await r('klasör joker eşleşmesi (a_b)', 'POST', `${root}/folders`, { name: `a_b ${tag}` })
  if (f3.body?.id) mine.add(f3.body.id)
  await only('klasörler', `${root}/folders`)
  await r('klasör adlandır olmayan', 'PATCH', `${root}/folders/yok`, { name: 'x' })
  await r('klasör adlandır çakışma', 'PATCH', `${root}/folders/${f2.id}`, { name: `klasör ${tag}` })
  await r('klasör adlandır kendi adı', 'PATCH', `${root}/folders/${f1.id}`, { name: `klasör ${tag}` })
  await r('klasör adlandır', 'PATCH', `${root}/folders/${f2.id}`, { name: `İkinci ${tag}` })
  await r('doküman doğrulama', 'POST', root, { title: 'x', kind: 'DOC' })
  await r('doküman doğrulama 2', 'POST', root, { kind: 'PAGE', folderId: 5 })
  await r('doküman boşluk adı', 'POST', root, { title: '   ', kind: 'PAGE' })
  await r('doküman olmayan klasör', 'POST', root, { title: 'x', kind: 'PAGE', folderId: 'yok' })
  const d1 = (await r('sayfa', 'POST', root, { title: ` Sayfa ${tag} `, kind: 'PAGE', folderId: f1.id })).body
  mine.add(d1.id)
  await ev('sayfa')
  const d2 = (await r('tuval', 'POST', root, { title: `Tuval ${tag}`, kind: 'CANVAS', folderId: '' })).body
  mine.add(d2.id)
  await ev('tuval')
  await only('liste', root)
  await r('getir', 'GET', `${root}/${d1.id}`)
  await r('getir B (yetkisiz)', 'GET', `${root}/${d1.id}`, undefined, B)
  await r('getir olmayan', 'GET', `${root}/yok`)
  await r('paylaşım', 'GET', `${root}/${d1.id}/permissions`)
  await r('ver doğrulama', 'PUT', `${root}/${d1.id}/permissions/${bot.user.id}`, { access: 'OWNER' })
  await r('ver sahibe', 'PUT', `${root}/${d1.id}/permissions/${owner.user.id}`, { access: 'READ' })
  await r('ver olmayan kişi', 'PUT', `${root}/${d1.id}/permissions/yok`, { access: 'READ' })
  await r('ver okuma', 'PUT', `${root}/${d1.id}/permissions/${bot.user.id}`, { access: 'READ' })
  await ev('ver okuma')
  await r('getir B', 'GET', `${root}/${d1.id}`, undefined, B)
  await r('düzenle B (salt okunur)', 'PATCH', `${root}/${d1.id}`, { title: 'x' }, B)
  await r('sil B', 'DELETE', `${root}/${d1.id}`, undefined, B)
  await r('paylaşım B', 'GET', `${root}/${d1.id}/permissions`, undefined, B)
  await r('ver yazma', 'PUT', `${root}/${d1.id}/permissions/${bot.user.id}`, { access: 'WRITE' })
  await ev('ver yazma')
  await r('paylaşım 2', 'GET', `${root}/${d1.id}/permissions`)
  await r('düzenle doğrulama', 'PATCH', `${root}/${d1.id}`, { content: [1] })
  await r('düzenle doğrulama 2', 'PATCH', `${root}/${d1.id}`, { title: '' })
  await r('düzenle boşluk adı', 'PATCH', `${root}/${d1.id}`, { title: '  ' })
  await r('düzenle ad null', 'PATCH', `${root}/${d1.id}`, { title: null })
  await r('düzenle olmayan klasör', 'PATCH', `${root}/${d1.id}`, { folderId: 'yok' })
  await sleep(5)
  const before = (await call(base, 'GET', `${root}/${d1.id}`, undefined, A)).body.updatedAt
  const empty = await r('düzenle boş', 'PATCH', `${root}/${d1.id}`, {})
  out.push(['boş PATCH updatedAt korunur', empty.body.updatedAt === before])
  await ev('düzenle boş')
  await r('düzenle içerik etiket', 'PATCH', `${root}/${d1.id}`, { content: { version: 1, body: `<p>@rustbot ${tag}</p>`, 2: ['@p2test'], 1: { x: 'y' } } })
  await ev('etiket A')
  await r('düzenle içerik etiket tekrar', 'PATCH', `${root}/${d1.id}`, { content: { version: 1, body: `<p>@rustbot yine ${tag}</p>` } })
  await ev('etiket tekrar')
  await r('düzenle B etiket', 'PATCH', `${root}/${d1.id}`, { title: ` Yeni ${tag} `, content: { version: 1, body: '@p2test bak' }, folderId: f2.id }, B)
  await ev('etiket B')
  await r('düzenle klasör null', 'PATCH', `${root}/${d2.id}`, { folderId: null })
  await only('liste B', root, B)
  await r('klasör sil olmayan', 'DELETE', `${root}/folders/yok`)
  await r('klasör sil', 'DELETE', `${root}/folders/${f2.id}`)
  await r('getir klasör silindikten sonra', 'GET', `${root}/${d1.id}`)
  if (cross) await cross(d1.id)
  await r('geri al', 'DELETE', `${root}/${d1.id}/permissions/${bot.user.id}`)
  await r('geri al (olmayan)', 'DELETE', `${root}/${d1.id}/permissions/yok`)
  await ev('geri al')
  await r('getir B geri alındı', 'GET', `${root}/${d1.id}`, undefined, B)
  await r('sil', 'DELETE', `${root}/${d1.id}`)
  await ev('sil')
  await r('sil tekrar', 'DELETE', `${root}/${d1.id}`)
  await r('sil tuval', 'DELETE', `${root}/${d2.id}`)
  await r('klasör sil 1', 'DELETE', `${root}/folders/${f1.id}`)
  for (const f of (await call(base, 'GET', `${root}/folders`, undefined, A)).body) if (f.name.toLowerCase().includes(tag)) await call(base, 'DELETE', `${root}/folders/${f.id}`, undefined, A)
  await ev('temizlik')
  la.socket.close(); lb.socket.close()
  return out
}

const tn = 'dn' + Math.random().toString(36).slice(2, 7), tr = 'dr' + Math.random().toString(36).slice(2, 7)
const a = await scenario(NEST, tn)
const b = await scenario(rust.base, tr, async id => {
  const path = `/organizations/${org.id}/projects/${project.id}/documents/${id}`
  for (const p of [path, path + '/permissions']) {
    const x = await call(NEST, 'GET', p, undefined, owner.accessToken)
    const y = await call(rust.base, 'GET', p, undefined, owner.accessToken)
    same('Rust yazdı, NestJS okudu ' + p.slice(-12), x, y)
  }
})
const replace = [[tn, '<tag>'], [tr, '<tag>'], [tn.toUpperCase(), '<TAG>'], [tr.toUpperCase(), '<TAG>']]
const count = list => list.filter(([n]) => n.includes(' olay ')).reduce((t, [, e]) => t + e.length, 0)
check('olaylar geldi', count(a) > 0 && count(a) === count(b), `${count(a)} / ${count(b)}`)
check('adım sayısı', a.length === b.length, `${a.length} / ${b.length}`)
for (let i = 0; i < a.length; i++) same(a[i][0], a[i][1], b[i][1], replace)
rust.stop()
done()
