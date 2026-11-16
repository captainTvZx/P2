// Dilim 18 uçtan uca testi (yalnız sunucu): arkadaşlar ve engeller (docs/arkadaslar.md).
// İstek gönder → kabul et → ortak proje olmadan DM → projeden ayrıl, arkadaş kalır → engelle
// (DM yok, istek yok, arama yok, mesajlar gizli) → engeli kaldır → arkadaşlıktan çıkar; gerçek
// zamanlı `friend:*` olayları ve arkadaşlara çevrimiçi durum yayını.
//
// Önkoşul: test PostgreSQL (:55432) + `cargo build --release -p p2-server` + `p2-server migrate`.
//   node tools/slice18-test.mjs
import { randomBytes } from 'node:crypto'
import { testServer, io, sleep, sql } from './lib.mjs'

const { api: API, stop } = await testServer()
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName: `${name} ${tag}`, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body
const friendsOf = async u => (await call('GET', '/friends', null, u.accessToken)).body
// Proje açmak sunucu yetkisi (server_admin): test kullanıcılarına doğrudan verilir.
const allowProjects = u => { try { sql(`insert into "ServerPermission" ("userId", "permission") values ($1, 'project.create') on conflict do nothing`, [u.user.id]) } catch {} }

console.log(`hazırlık: etiket ${tag}`)
// A ve B ayrı projelerde (ortak proje yok); C, A'nın projesinde; D hiçbir projede değil.
const A = await register('fa')
allowProjects(A)
const pA = (await call('POST', '/projects', { name: `Arkadaş A ${tag}`, slug: `arkadas-a-${tag}` }, A.accessToken)).body
const org = (await call('GET', '/organizations', null, A.accessToken)).body[0]
const B = await register('fb')
allowProjects(B)
const pB = (await call('POST', '/projects', { name: `Arkadaş B ${tag}`, slug: `arkadas-b-${tag}` }, B.accessToken)).body
const inviteA = (await call('POST', `/projects/${pA.id}/admin/invites`, { maxUses: 10 }, A.accessToken)).body
const C = await register('fc', { inviteCode: inviteA.code })
const D = await register('fd')
check('hazırlık: A ve B ayrı projelerde, C A ile aynı projede', !!pA?.id && !!pB?.id && C?.joined?.projectId === pA.id)

// Gerçek zamanlı olaylar.
const events = { A: [], B: [] }
const WATCHED = ['friend:request', 'friend:accepted', 'friend:removed', 'friend:blocked', 'friend:unblocked', 'presence:update', 'direct:created', 'call:incoming']
const connect = (user, bucket) => new Promise(done => {
  const s = io(API + '/stream', { auth: { token: user.accessToken }, transports: ['websocket'] })
  for (const e of WATCHED) s.on(e, p => bucket.push({ e, p }))
  s.on('connect', () => done(s))
})
const sockA = await connect(A, events.A)
const saw = (bucket, e, test = () => true) => bucket.some(x => x.e === e && test(x.p))

// --- ortak proje yokken DM yok
const noDm = await call('POST', `/organizations/${org.id}/direct`, { userId: B.user.id }, A.accessToken)
check('arkadaş değil + ortak proje yok: DM açılmaz', noDm.status === 403 && noDm.body.code === 'NO_SHARED_PROJECT', `${noDm.status} ${noDm.body?.code}`)

// --- istek
check('boş istek 400', (await call('POST', '/friends/requests', {}, A.accessToken)).status === 400)
check('bilinmeyen kullanıcı 404', (await call('POST', '/friends/requests', { username: 'yok_' + tag }, A.accessToken)).body.code === 'USER_NOT_FOUND')
check('kendine istek 409', (await call('POST', '/friends/requests', { username: A.user.username }, A.accessToken)).body.code === 'SELF_FRIEND')
const req = await call('POST', '/friends/requests', { username: '@' + B.user.username.toUpperCase() }, A.accessToken)
check('A → B istek (kullanıcı adıyla, @ ve büyük harf)', req.status === 201 && req.body.status === 'PENDING' && req.body.user.id === B.user.id, JSON.stringify(req.body))
check('aynı istek tekrar 409', (await call('POST', '/friends/requests', { userId: B.user.id }, A.accessToken)).body.code === 'FRIEND_REQUEST_EXISTS')
let fa = await friendsOf(A), fb = await friendsOf(B)
check('A: giden istek, B: gelen istek', fa.outgoing.length === 1 && fa.outgoing[0].user.id === B.user.id && fb.incoming.length === 1 && fb.incoming[0].user.id === A.user.id && fb.incoming[0].id === req.body.requestId)
check('istek bekleyen kişinin durumu sızmaz', fa.outgoing[0].user.status === undefined)
check('istek varken hâlâ DM yok', (await call('POST', `/organizations/${org.id}/direct`, { userId: B.user.id }, A.accessToken)).status === 403)
check('isteği gönderen kabul edemez (404)', (await call('POST', `/friends/requests/${req.body.requestId}/accept`, null, A.accessToken)).status === 404)
check('üçüncü kişi kabul edemez (404)', (await call('POST', `/friends/requests/${req.body.requestId}/accept`, null, C.accessToken)).status === 404)

// --- iptal / ret
check('A isteği iptal etti', (await call('DELETE', `/friends/requests/${req.body.requestId}`, null, A.accessToken)).status === 200)
check('iptalden sonra liste boş', (await friendsOf(B)).incoming.length === 0)
const req2 = (await call('POST', '/friends/requests', { userId: B.user.id }, A.accessToken)).body
check('alıcı iptal edemez (404)', (await call('DELETE', `/friends/requests/${req2.requestId}`, null, B.accessToken)).status === 404)
check('alıcı reddeder', (await call('POST', `/friends/requests/${req2.requestId}/decline`, null, B.accessToken)).status === 200)
check('retten sonra istek yok', (await friendsOf(A)).outgoing.length === 0)

// --- kabul
const req3 = (await call('POST', '/friends/requests', { userId: B.user.id }, A.accessToken)).body
const sockB = await connect(B, events.B)
await sleep(300)
const acc = await call('POST', `/friends/requests/${req3.requestId}/accept`, null, B.accessToken)
check('B kabul etti', acc.status === 201 && acc.body.status === 'ACCEPTED' && acc.body.user.id === A.user.id, JSON.stringify(acc.body))
fa = await friendsOf(A); fb = await friendsOf(B)
check('iki tarafta arkadaş listesi', fa.friends.length === 1 && fa.friends[0].id === B.user.id && fb.friends.length === 1 && fb.friends[0].id === A.user.id && fa.outgoing.length === 0 && fb.incoming.length === 0)
check('arkadaş durumu (çevrimiçi) listede', fa.friends[0].status === 'ONLINE' && fb.friends[0].status === 'ONLINE', `${fa.friends[0].status} / ${fb.friends[0].status}`)
check('zaten arkadaş 409', (await call('POST', '/friends/requests', { userId: A.user.id }, B.accessToken)).body.code === 'ALREADY_FRIENDS')
await sleep(300)
check('olay: friend:request ve friend:accepted (iki tarafa)', saw(events.A, 'friend:request') && saw(events.A, 'friend:accepted', p => p.requestId === req3.requestId) && saw(events.B, 'friend:accepted'))
check('kabulde arkadaşın durumu yayınlandı', saw(events.A, 'presence:update', p => p.userId === B.user.id && p.status === 'ONLINE'))

// --- ortak proje olmadan DM
const dm = await call('POST', `/organizations/${org.id}/direct`, { userId: B.user.id }, A.accessToken)
check('arkadaşlar ortak proje olmadan DM açar', dm.status === 201 && dm.body.user.id === B.user.id && dm.body.friend === true, `${dm.status} ${JSON.stringify(dm.body).slice(0, 120)}`)
const m1 = await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'selam arkadaş' }, A.accessToken)
const m2 = await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'selam!' }, B.accessToken)
check('iki yönde mesaj', m1.status === 201 && m2.status === 201, `${m1.status} ${m2.status} ${JSON.stringify(m2.body).slice(0, 100)}`)
const listB = (await call('GET', `/organizations/${org.id}/direct`, null, B.accessToken)).body
check('B\'nin DM listesinde sohbet (arkadaş, son mesaj)', listB.length === 1 && listB[0].friend && listB[0].lastMessage.content === 'selam!')
check('A\'da okunmamış 1', (await call('GET', `/organizations/${org.id}/direct`, null, A.accessToken)).body[0].unread === 1)

// --- arkadaşlara durum yayını (ortak proje yok)
events.A.length = 0
sockB.close()
await sleep(1500)
check('arkadaş çevrimdışı olunca durum A\'ya gelir', saw(events.A, 'presence:update', p => p.userId === B.user.id && p.status === 'OFFLINE'), JSON.stringify(events.A.map(x => x.e)))
const sockB2 = await connect(B, events.B)
await sleep(800)
check('arkadaş çevrimiçi olunca durum A\'ya gelir', saw(events.A, 'presence:update', p => p.userId === B.user.id && p.status === 'ONLINE'))

// --- projeden ayrıl: arkadaş kalır
const reqC = (await call('POST', '/friends/requests', { userId: C.user.id }, A.accessToken)).body
check('C ters yönden istek gönderince otomatik kabul', !!reqC.requestId && (await call('POST', '/friends/requests', { username: A.user.username }, C.accessToken)).body.status === 'ACCEPTED')
const dmC = (await call('POST', `/organizations/${org.id}/direct`, { userId: A.user.id }, C.accessToken)).body
const left = await call('POST', `/projects/${pA.id}/leave`, null, C.accessToken)
check('C projeden ayrıldı', left.status < 300, String(left.status))
check('ayrılınca proje yok ama arkadaş duruyor', (await call('GET', '/projects', null, C.accessToken)).body.length === 0 && (await friendsOf(C)).friends.some(f => f.id === A.user.id))
check('ayrıldıktan sonra arkadaşa DM gider', (await call('POST', `/organizations/${org.id}/direct/${dmC.id}/messages`, { content: 'hâlâ buradayım' }, C.accessToken)).status === 201)

// --- hiç projeye girmemiş D ile arkadaşlık ve DM
const reqD = (await call('POST', '/friends/requests', { username: D.user.username }, A.accessToken)).body
check('projesiz D isteği kabul eder', (await call('POST', `/friends/requests/${reqD.requestId}/accept`, null, D.accessToken)).status === 201)
const dOrgs = (await call('GET', '/organizations', null, D.accessToken)).body
check('D artık kurulumun hesap kabında (proje görmez)', dOrgs.some(o => o.id === org.id) && (await call('GET', '/projects', null, D.accessToken)).body.length === 0)
const dmD = await call('POST', `/organizations/${org.id}/direct`, { userId: A.user.id }, D.accessToken)
check('projesiz D arkadaşına DM açar ve yazar', dmD.status === 201 && (await call('POST', `/organizations/${org.id}/direct/${dmD.body.id}/messages`, { content: 'merhaba' }, D.accessToken)).status === 201)

// --- engelle
check('kendini engelleyemez', (await call('POST', `/blocks/${A.user.id}`, null, A.accessToken)).body.code === 'SELF_BLOCK')
events.A.length = 0; events.B.length = 0
const blk = await call('POST', `/blocks/${B.user.id}`, null, A.accessToken)
check('A, B\'yi engelledi', blk.status === 201 && blk.body.blocked === true)
fa = await friendsOf(A); fb = await friendsOf(B)
check('engel: arkadaşlık silindi, A\'nın engellenenler listesinde B', fa.friends.every(f => f.id !== B.user.id) && fa.blocked.some(b => b.user.id === B.user.id) && fb.friends.length === 0 && fb.blocked.length === 0)
const s1 = await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'engelliyken' }, B.accessToken)
const s2 = await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'engelleyen yazamaz' }, A.accessToken)
check('engel: iki yönde de mesaj reddedilir (USER_BLOCKED)', s1.status === 403 && s1.body.code === 'USER_BLOCKED' && s2.status === 403 && s2.body.code === 'USER_BLOCKED', `${s1.status} ${s1.body?.code} / ${s2.status} ${s2.body?.code}`)
const r1 = await call('POST', '/friends/requests', { userId: A.user.id }, B.accessToken)
const r2 = await call('POST', '/friends/requests', { userId: B.user.id }, A.accessToken)
check('engel: istek gönderilemez (engellenen USER_BLOCKED, engelleyen BLOCKED_BY_ME)', r1.status === 403 && r1.body.code === 'USER_BLOCKED' && r2.status === 409 && r2.body.code === 'BLOCKED_BY_ME', `${r1.body?.code} / ${r2.body?.code}`)
const hidden = (await call('GET', `/organizations/${org.id}/direct/${dm.body.id}/messages`, null, A.accessToken)).body
const visibleB = (await call('GET', `/organizations/${org.id}/direct/${dm.body.id}/messages`, null, B.accessToken)).body
check('engel: engellenenin mesajları engelleyene gizli, sohbet okunabilir', hidden.length === 1 && hidden[0].content === 'selam arkadaş' && visibleB.length === 2)
const listA = (await call('GET', `/organizations/${org.id}/direct`, null, A.accessToken)).body
check('engel: DM listesinde blockedByMe ve okunmamış 0', listA.find(c => c.id === dm.body.id)?.blockedByMe === true && listA.find(c => c.id === dm.body.id)?.unread === 0)
sockB2.emit('call:start', { organizationId: org.id, targetUserId: A.user.id, kind: 'direct' })
await sleep(800)
check('engel: arama ulaşmaz', !saw(events.A, 'call:incoming'))
check('olay: friend:blocked iki tarafa', saw(events.A, 'friend:blocked', p => p.userId === B.user.id) && saw(events.B, 'friend:blocked', p => p.byId === A.user.id))

// --- engeli kaldır
check('engelli olmayan kişi için 404', (await call('DELETE', `/blocks/${C.user.id}`, null, A.accessToken)).status === 404)
check('engel kalktı', (await call('DELETE', `/blocks/${B.user.id}`, null, A.accessToken)).status === 200)
check('engel kalkınca arkadaş değiller, ortak proje yok: DM yine yok', (await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'x' }, B.accessToken)).body.code === 'NO_SHARED_PROJECT')
check('engel kalkınca gizli mesajlar geri gelir', (await call('GET', `/organizations/${org.id}/direct/${dm.body.id}/messages`, null, A.accessToken)).body.length === 2)
const req4 = (await call('POST', '/friends/requests', { userId: A.user.id }, B.accessToken)).body
check('engel kalkınca yeniden istek ve kabul', req4.status === 'PENDING' && (await call('POST', `/friends/requests/${req4.requestId}/accept`, null, A.accessToken)).status === 201)
check('yeniden arkadaş: DM çalışır', (await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'barıştık' }, B.accessToken)).status === 201)

// --- arkadaşlıktan çıkar
events.B.length = 0
check('A, B\'yi arkadaşlıktan çıkardı', (await call('DELETE', `/friends/${B.user.id}`, null, A.accessToken)).status === 200)
check('arkadaş olmayan için 404', (await call('DELETE', `/friends/${B.user.id}`, null, A.accessToken)).body.code === 'NOT_FRIENDS')
fa = await friendsOf(A); fb = await friendsOf(B)
check('iki tarafta da listeden çıktı', fa.friends.every(f => f.id !== B.user.id) && fb.friends.length === 0)
check('çıkarınca DM reddedilir', (await call('POST', `/organizations/${org.id}/direct/${dm.body.id}/messages`, { content: 'x' }, A.accessToken)).body.code === 'NO_SHARED_PROJECT')
check('eski mesajlar okunur', (await call('GET', `/organizations/${org.id}/direct/${dm.body.id}/messages`, null, B.accessToken)).body.length === 3)
await sleep(300)
check('olay: friend:removed karşı tarafa', saw(events.B, 'friend:removed', p => p.reason === 'removed' && p.byId === A.user.id))

// --- ortak projedeki kişiyle (arkadaş olmadan) DM hâlâ çalışır; engel onu da keser
const E = await register('fe', { inviteCode: inviteA.code })
const dmE = await call('POST', `/organizations/${org.id}/direct`, { userId: E.user.id }, A.accessToken)
check('ortak projedeki kişiyle arkadaş olmadan DM', dmE.status === 201 && dmE.body.friend === false)
await call('POST', `/blocks/${A.user.id}`, null, E.accessToken)
check('ortak projede engel de DM\'i keser', (await call('POST', `/organizations/${org.id}/direct/${dmE.body.id}/messages`, { content: 'x' }, A.accessToken)).body.code === 'USER_BLOCKED')

// --- bütün gruplarım (Ana sayfa listesi)
const allGroups = await call('GET', `/organizations/${org.id}/groups?all=1`, null, A.accessToken)
check('?all=1 grup listesi dizi döner', allGroups.status === 200 && Array.isArray(allGroups.body))

sockA.close(); sockB2.close()
stop()
console.log(`\n${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
