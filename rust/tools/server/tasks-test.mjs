// Görev panosu uçları (tasks.ts ↔ tasks.rs): aynı senaryo iki sunucuda ayrı taze projelerle
// çalışır, maskeli cevaplar ve gerçek zamanlı olaylar karşılaştırılır; Rust'ın yazdığı NestJS ile okunur.
import { createRequire } from 'node:module'
import { NEST, startRust, call, login, same, check, mask, done, sleep } from './lib.mjs'

const require = createRequire(new URL('../../../package.json', import.meta.url))
const { io } = require('socket.io-client')

const rust = await startRust(4124)
const owner = await login(NEST, 'p2test')
const bot = await login(NEST, 'rustbot')
const T = owner.accessToken, B = bot.accessToken
const BOT = bot.user.id, OWNER = owner.user.id
const org = (await call(NEST, 'GET', '/organizations', undefined, T)).body[0].id
const tag = Math.random().toString(36).slice(2, 8)

async function project(label, withBot = true) {
  const res = await call(NEST, 'POST', `/organizations/${org}/projects`, { name: `RT görev ${tag}`, slug: `rt-gorev-${label}-${tag}` }, T)
  if (res.status !== 201) throw new Error('proje açılamadı: ' + JSON.stringify(res.body))
  if (withBot) await call(NEST, 'PATCH', `/organizations/${org}/admin/access/projects/${res.body.id}/members/${BOT}`, { visible: true }, T)
  return res.body.id
}
const PN = await project('n'), PR = await project('r'), HIDDEN = await project('h', false)

// Gerçek zamanlı dinleyici: kişinin bu sunucuya gelen olayları.
async function listen(base, token) {
  const events = []
  const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
  socket.onAny((event, payload) => events.push([event, payload]))
  await new Promise((resolve, reject) => { socket.on('connect', resolve); socket.on('connect_error', reject) })
  await new Promise(resolve => socket.emit('subscribe', { organizationId: org }, resolve))
  return { events, close: () => socket.close() }
}

const RELEVANT = /^(board:|sprint:|task:|activity:new)/
async function scenario(base, P) {
  const out = []
  const dates = []
  const r = async (name, method, path, body, token = T) => {
    const res = await call(base, method, `/organizations/${org}/projects/${P}/boards${path}`, body, token)
    out.push([name, res])
    return res
  }
  const b1 = (await r('panolar (Genel açılır)', 'GET', '')).body[0].id
  await r('panolar tekrar', 'GET', '')
  await r('pano: ad yok', 'POST', '', {})
  await r('pano: kısa ad', 'POST', '', { name: 'x' })
  await r('pano: yetkisiz üye', 'POST', '', { name: 'Bob panosu' }, B)
  const b2 = (await r('pano oluştur', 'POST', '', { name: '  Sprint Panosu  ' })).body.id
  await r('pano adı değiştir', 'PATCH', `/${b2}`, { name: 'Yeni ad' })
  await r('pano adı: olmayan pano', 'PATCH', `/yok`, { name: 'Yeni ad' })
  await r('pano adı: geçersiz', 'PATCH', `/${b2}`, { name: 5 })
  await r('pano içeriği (boş)', 'GET', `/${b1}`)

  await r('sprint: kısa ad', 'POST', `/${b1}/sprints`, { name: 'S' })
  await r('sprint: tarih geçersiz', 'POST', `/${b1}/sprints`, { name: 'Sprint 1', startsAt: 'yanlış' })
  await r('sprint: üye yetkisiz', 'POST', `/${b1}/sprints`, { name: 'Sprint 1' }, B)
  const s1res = await r('sprint oluştur', 'POST', `/${b1}/sprints`, { name: ' Sprint 1 ', goal: ' hedef ', startsAt: '2026-11-20', endsAt: '2026-12-01T10:00:00+03:00' })
  dates.push(['sprint1', s1res.body.startsAt, s1res.body.endsAt])
  const s1 = s1res.body.id
  const s2 = (await r('sprint 2 oluştur', 'POST', `/${b1}/sprints`, { name: 'Sprint 2', goal: '' })).body.id
  await r('sprint 1 aktif', 'PATCH', `/${b1}/sprints/${s1}`, { status: 'ACTIVE' })
  const s2a = await r('sprint 2 aktif (başlangıç atanır)', 'PATCH', `/${b1}/sprints/${s2}`, { status: 'ACTIVE', goal: '' })
  check('sprint 2 başlangıcı atandı', !!s2a.body.startsAt)
  await r('sprint: geçersiz durum', 'PATCH', `/${b1}/sprints/${s2}`, { status: 'X' })
  await r('sprint: olmayan', 'PATCH', `/${b1}/sprints/yok`, { name: 'Sprint 9' })
  const s2b = await r('sprint: bitiş null → 1970', 'PATCH', `/${b1}/sprints/${s2}`, { endsAt: null, startsAt: '2026-11-21T09:30' })
  dates.push(['sprint2', s2b.body.startsAt, s2b.body.endsAt])
  await r('sprint: ad null → 500', 'PATCH', `/${b1}/sprints/${s2}`, { name: null })

  await r('görev: başlık yok', 'POST', `/${b1}/tasks`, {})
  await r('görev: durum geçersiz', 'POST', `/${b1}/tasks`, { title: 'ab', status: 'NOPE' })
  await r('görev: etiket dizi değil', 'POST', `/${b1}/tasks`, { title: 'ab', labels: 'x' })
  await r('görev: puan 101', 'POST', `/${b1}/tasks`, { title: 'ab', points: 101 })
  await r('görev: puan 1.5', 'POST', `/${b1}/tasks`, { title: 'ab', points: 1.5 })
  await r('görev: tarih geçersiz', 'POST', `/${b1}/tasks`, { title: 'ab', dueAt: '2026-13-01' })
  await r('görev: sprint başka panonun', 'POST', `/${b1}/tasks`, { title: 'abc', sprintId: 'yok' })
  await r('görev: atanan üye değil', 'POST', `/${b1}/tasks`, { title: 'abc', assigneeId: 'yok' })
  const t1res = await r('görev oluştur (etiket, atama, anma)', 'POST', `/${b1}/tasks`, {
    title: '  Görev bir  ', description: ' açıklama @rustbot ', priority: 'HIGH', kind: 'BUG',
    labels: ['UI', ' ui ', 'Ağ,Ağı\nx', 'I', ' ı', 'çok uzun bir etiket adı burada var'], points: 3, dueAt: '2026-11-30', sprintId: s1, assigneeId: BOT, junk: 1,
  })
  dates.push(['task1', t1res.body.dueAt])
  const t1 = t1res.body.id
  const t2res = await r('üye görev açar (yerel saat)', 'POST', `/${b1}/tasks`, { title: 'Bob görevi', dueAt: '2026-11-30T10:00' }, B)
  dates.push(['task2', t2res.body.dueAt])
  const t2 = t2res.body.id
  await r('görev 3 (REVIEW)', 'POST', `/${b1}/tasks`, { title: 'Üçüncü', status: 'REVIEW', points: 0 })
  await r('güncelle: başlık kısa önce', 'PATCH', `/${b1}/tasks/${t1}`, { position: 'a', title: 'b' })
  await r('güncelle: konum sonra açıklama', 'PATCH', `/${b1}/tasks/${t1}`, { position: 'a', description: 5 })
  await r('güncelle: sprint başka panonun', 'PATCH', `/${b1}/tasks/${t1}`, { sprintId: 'yok' })
  await r('güncelle: olmayan görev', 'PATCH', `/${b1}/tasks/yok`, { title: 'abc' })
  await r('güncelle: atanan değil üye', 'PATCH', `/${b1}/tasks/${t1}`, { assigneeId: 'yok' })
  await r('güncelle: atanan üye düzenler', 'PATCH', `/${b1}/tasks/${t1}`, { title: 'Bot düzenledi' }, B)
  await r('güncelle: hepsi', 'PATCH', `/${b1}/tasks/${t1}`, { title: ' Yeni ', status: 'DONE', labels: [], points: null, dueAt: '', assigneeId: '', sprintId: '', position: 7 })
  await r('güncelle: artık yetkisiz', 'PATCH', `/${b1}/tasks/${t1}`, { title: 'Bot yine' }, B)
  await r('güncelle: kendi görevine anma', 'PATCH', `/${b1}/tasks/${t2}`, { description: '@p2test bak' }, B)
  await r('güncelle: yeniden ata', 'PATCH', `/${b1}/tasks/${t2}`, { assigneeId: BOT, sprintId: s2 })
  await r('güncelle: açıklama null → 500', 'PATCH', `/${b1}/tasks/${t1}`, { description: null })
  await r('taşı: geçersiz sütun', 'PATCH', `/${b1}/tasks/${t1}/move`, { status: 'NOPE' })
  await r('taşı: dizi sütun', 'PATCH', `/${b1}/tasks/${t1}/move`, { status: ['TODO'] })
  await r('taşı', 'PATCH', `/${b1}/tasks/${t1}/move`, { status: 'REVIEW', order: [t2, t1] })
  await r('taşı: sayı kimlik → 500', 'PATCH', `/${b1}/tasks/${t1}/move`, { order: [5] })
  await r('taşı: üye yetkisiz', 'PATCH', `/${b1}/tasks/${t1}/move`, { order: [] }, B)
  await r('taşı: olmayan görev', 'PATCH', `/${b1}/tasks/yok/move`, {})

  await r('yorumlar (boş)', 'GET', `/${b1}/tasks/${t1}/comments`)
  await r('yorumlar: olmayan görev', 'GET', `/${b1}/tasks/yok/comments`)
  await r('yorum: boş', 'POST', `/${b1}/tasks/${t1}/comments`, { content: '' })
  const c1 = (await r('yorum ekle (anma)', 'POST', `/${b1}/tasks/${t1}/comments`, { content: ' merhaba @rustbot ' })).body.id
  await r('yorum ekle (üye)', 'POST', `/${b1}/tasks/${t1}/comments`, { content: 'bot yorumu' }, B)
  await r('tepki', 'POST', `/${b1}/tasks/${t1}/comments/${c1}/reactions`, { emoji: '👍' })
  await r('tepki (üye)', 'POST', `/${b1}/tasks/${t1}/comments/${c1}/reactions`, { emoji: ' 👍 ' }, B)
  await r('tepki: aile emojisi', 'POST', `/${b1}/tasks/${t1}/comments/${c1}/reactions`, { emoji: '👨‍👩‍👧‍👦❤️' })
  await r('tepki geri al', 'POST', `/${b1}/tasks/${t1}/comments/${c1}/reactions`, { emoji: '👍' })
  await r('tepki: 25 karakter', 'POST', `/${b1}/tasks/${t1}/comments/${c1}/reactions`, { emoji: 'x'.repeat(25) })
  await r('tepki: olmayan yorum', 'POST', `/${b1}/tasks/${t1}/comments/yok/reactions`, { emoji: '👍' })
  await r('yorumlar (tepkili)', 'GET', `/${b1}/tasks/${t1}/comments`)
  await r('yorumlar (üye gözüyle)', 'GET', `/${b1}/tasks/${t1}/comments`, undefined, B)
  await r('yorum düzenle: başkası', 'PATCH', `/${b1}/tasks/${t1}/comments/${c1}`, { content: 'x' }, B)
  await r('yorum düzenle', 'PATCH', `/${b1}/tasks/${t1}/comments/${c1}`, { content: 'düzenlendi @rustbot @p2test' })
  await r('yorum düzenle: olmayan', 'PATCH', `/${b1}/tasks/${t1}/comments/yok`, { content: 'x' })
  await r('yorum sil: başkası', 'DELETE', `/${b1}/tasks/${t1}/comments/${c1}`, undefined, B)
  await r('yorum sil', 'DELETE', `/${b1}/tasks/${t1}/comments/${c1}`)
  await r('yorum sil: tekrar', 'DELETE', `/${b1}/tasks/${t1}/comments/${c1}`)

  await r('pano içeriği (dolu)', 'GET', `/${b1}`)
  await r('görev sil: üye yetkisiz', 'DELETE', `/${b1}/tasks/${t1}`, undefined, B)
  await r('görev sil', 'DELETE', `/${b1}/tasks/${t1}`)
  await r('görev sil: tekrar', 'DELETE', `/${b1}/tasks/${t1}`)
  await r('sprint sil', 'DELETE', `/${b1}/sprints/${s2}`)
  await r('sprint sil: olmayan', 'DELETE', `/${b1}/sprints/${s2}`)
  await r('pano sil', 'DELETE', `/${b2}`)
  await r('pano sil: son pano', 'DELETE', `/${b1}`)
  await r('pano sil: üye yetkisiz', 'DELETE', `/${b1}`, undefined, B)
  await r('son durum', 'GET', `/${b1}`)
  return { out, dates, b1 }
}

const cleanup = async () => { for (const P of [PN, PR, HIDDEN]) await call(NEST, 'DELETE', `/organizations/${org}/projects/${P}`, undefined, T) }
process.on('uncaughtException', async e => { console.error(e); await cleanup(); rust.stop(); process.exit(1) })
process.on('unhandledRejection', async e => { console.error(e); await cleanup(); rust.stop(); process.exit(1) })
const nestListen = [await listen(NEST, T), await listen(NEST, B)]
const rustListen = [await listen(rust.base, T), await listen(rust.base, B)]
const n = await scenario(NEST, PN)
const r = await scenario(rust.base, PR)
await sleep(800)
for (let i = 0; i < n.out.length; i++) same(n.out[i][0], n.out[i][1], r.out[i][1])
check('tarih çevirisi aynı', JSON.stringify(n.dates) === JSON.stringify(r.dates), JSON.stringify(n.dates) + ' / ' + JSON.stringify(r.dates))

// Gerçek zamanlı olaylar: olay adı + maskeli yük, sıradan bağımsız.
// Paylaşılan NestJS'e başka testlerin olayları da düşebilir: yalnız bu testin projeleri.
const mine = p => [PN, PR].some(id => JSON.stringify(p).includes(id))
const norm = list => list.filter(([e, p]) => RELEVANT.test(e) && mine(p)).map(([e, p]) => JSON.stringify([e, mask(p)])).sort()
for (const [i, who] of [[0, 'sahip'], [1, 'üye']]) {
  const a = norm(nestListen[i].events), b = norm(rustListen[i].events)
  check(`gerçek zamanlı olaylar (${who}): ${a.length} olay`, JSON.stringify(a) === JSON.stringify(b), a.length === b.length ? '' : `nest ${a.length} / rust ${b.length}`)
  if (JSON.stringify(a) !== JSON.stringify(b)) {
    const onlyA = a.filter(x => !b.includes(x)), onlyB = b.filter(x => !a.includes(x))
    console.log('    yalnız nest:', onlyA.slice(0, 3).join('\n      '), '\n    yalnız rust:', onlyB.slice(0, 3).join('\n      '))
  }
}
for (const l of [...nestListen, ...rustListen]) l.close()

// Rust'ın yazdığını NestJS de aynı okuyor mu (ve tersi).
for (const [P, b] of [[PR, r.b1], [PN, n.b1]]) {
  const a = await call(NEST, 'GET', `/organizations/${org}/projects/${P}/boards/${b}`, undefined, T)
  const c = await call(rust.base, 'GET', `/organizations/${org}/projects/${P}/boards/${b}`, undefined, T)
  same(`çapraz okuma ${P === PR ? 'rust yazdı' : 'nest yazdı'}`, a, c)
}

// Yetki / bulunamadı yolları (salt okunur, aynı istek iki sunucuya).
const bothPaths = [
  ['gizli proje', 'GET', `/organizations/${org}/projects/${HIDDEN}/boards`, B],
  ['olmayan proje', 'GET', `/organizations/${org}/projects/yok/boards`, T],
  ['yabancı kuruluş', 'GET', `/organizations/yok/projects/${PN}/boards`, T],
  ['jetonsuz', 'GET', `/organizations/${org}/projects/${PN}/boards`, undefined],
  ['olmayan pano', 'GET', `/organizations/${org}/projects/${PN}/boards/yok`, T],
  ['gizli proje görev', 'POST', `/organizations/${org}/projects/${HIDDEN}/boards/x/tasks`, B],
]
for (const [name, method, path, token] of bothPaths) {
  const body = method === 'POST' ? { title: 'abc' } : undefined
  same(name, await call(NEST, method, path, body, token), await call(rust.base, method, path, body, token))
}

// Temizlik: test projeleri silinir (yumuşak silme).
await cleanup()
rust.stop()
done()
