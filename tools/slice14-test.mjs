// Dilim 14 uçtan uca testi (yalnız sunucu, arayüz yok): proje bazlı topluluklar
// (docs/proje-bazli-yapi.md). Proje davet kodu oluştur → önizle → katıl (kayıtla ve kodla) → kişi
// yalnız o projeyi görür, diğer proje görünmez → çıkar (tekrar katılabilir) → yasakla (katılamaz)
// → yasağı kaldır → iptal edilen kod → eski "kuruluşa katıl" ucu → DM yalnız ortak projede.
//
// Önkoşul: test PostgreSQL (:55432) + `cargo build --release -p p2-server`. Sunucu: P2_API verilmezse
// test kendi Rust sunucusunu (:4130, P2_TEST_PORT; tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice14-test.mjs
import { randomBytes } from 'node:crypto'
import { testServer } from './lib.mjs'

const { api: API, stop } = await testServer()
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName: `${name} ${tag}`, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body

console.log(`hazırlık: etiket ${tag}`)
const A = await register('pa')
check('A kayıt oldu', !!A?.accessToken)
// Proje açmak sunucu yetkisi (project.create): kurulum sahibi p2test A'ya verir.
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
check('yetkisiz proje açılamaz (403 PROJECT_CREATE_FORBIDDEN)', (await call('POST', '/projects', { name: `Yok ${tag}`, slug: `yok-${tag}` }, A.accessToken)).body?.code === 'PROJECT_CREATE_FORBIDDEN')
await call('PUT', `/server/users/${A.user.id}/permissions`, { permissions: ['project.create'] }, owner.accessToken)
const p1 = (await call('POST', '/projects', { name: `Topluluk Bir ${tag}`, slug: `topluluk-bir-${tag}` }, A.accessToken)).body
const p2 = (await call('POST', '/projects', { name: `Topluluk İki ${tag}`, slug: `topluluk-iki-${tag}` }, A.accessToken)).body
check('project.create ile proje açar (A iki proje)', !!p1?.id && !!p2?.id, `${p1?.id?.slice(0, 8)} / ${p2?.id?.slice(0, 8)}`)
const org = (await call('GET', '/organizations', null, A.accessToken)).body[0]
const aRoles = (await call('GET', `/projects/${p1.id}/roles`, null, A.accessToken)).body
check('projeye kurucu + herkes rolleri tohumlandı', aRoles.some(r => r.everyone) && aRoles.some(r => r.founder))
const aMine = (await call('GET', `/projects/${p1.id}/permissions/me`, null, A.accessToken)).body
check('açan kişi projenin sahibi', aMine.owner === true && aMine.projectId === p1.id)

// --- davet: oluştur → önizle
const invite = (await call('POST', `/projects/${p1.id}/admin/invites`, { maxUses: 10, expiresInDays: 7 }, A.accessToken)).body
check('davet kodu oluştu', /^P2-[0-9A-F]{8}$/.test(invite?.code || ''), invite?.code)
const preview = await call('GET', `/invites/${invite.code.toLowerCase()}`)
check('önizleme oturumsuz çalışır', preview.status === 200 && preview.body.valid && preview.body.project.name === p1.name && preview.body.project.memberCount === 1, JSON.stringify(preview.body?.project))
check('bilinmeyen kod 404', (await call('GET', '/invites/P2-00000000')).status === 404)

// --- kayıtla katılma
const badCode = await call('POST', '/auth/register', { email: `pz_${tag}@p2.test`, username: `pz_${tag}`, displayName: 'Hayalet', password: 'Parola123', passwordConfirm: 'Parola123', inviteCode: 'P2-DEADBEEF' })
const ghost = await call('POST', '/auth/login', { identifier: `pz_${tag}`, password: 'Parola123' })
check('geçersiz kodla kayıt reddedilir (hesap açılmaz)', badCode.status === 404 && ghost.status === 401, `${badCode.status} ${JSON.stringify(badCode.body)} / giriş ${ghost.status}`)
const B = await register('pb', { inviteCode: invite.code })
check('B kayıtla P1\'e katıldı', B?.joined?.projectId === p1.id, JSON.stringify(B?.joined))
const bProjects = (await call('GET', '/projects', null, B.accessToken)).body
check('B yalnız P1\'i görür (/projects)', bProjects.length === 1 && bProjects[0].id === p1.id && bProjects[0].role === 'MEMBER')
const bOrgProjects = (await call('GET', `/organizations/${org.id}/projects`, null, B.accessToken)).body
check('B kuruluş listesinde de yalnız P1', bOrgProjects.map(p => p.id).join() === p1.id)
check('P2 kanalları B\'ye 404', (await call('GET', `/organizations/${org.id}/projects/${p2.id}/channels`, null, B.accessToken)).status === 404)
check('P2 kişi listesi B\'ye 404', (await call('GET', `/projects/${p2.id}/members`, null, B.accessToken)).status === 404)
check('P2 davet listesi B\'ye 404', (await call('GET', `/projects/${p2.id}/admin/invites`, null, B.accessToken)).status === 404)
const p1Members = (await call('GET', `/projects/${p1.id}/members`, null, B.accessToken)).body
check('P1 kişi listesi: A sahip, B üye', p1Members.length === 2 && p1Members.find(m => m.id === A.user.id)?.owner && p1Members.find(m => m.id === B.user.id)?.role === 'MEMBER')
const bPerms = (await call('GET', `/projects/${p1.id}/permissions/me`, null, B.accessToken)).body
check('B everyone yetkileriyle (kanal görür, üye çıkaramaz)', bPerms.permissions.includes('channel.view') && !bPerms.permissions.includes('member.kick'))
check('B üye çıkaramaz (403)', (await call('DELETE', `/projects/${p1.id}/admin/members/${A.user.id}`, null, B.accessToken)).status === 403)

// --- kodla katılma, kişi listeleri
const C = await register('pc')
const joinC = await call('POST', `/invites/${invite.code}/join`, null, C.accessToken)
check('C kodla katıldı', joinC.status === 201 && joinC.body.joined && joinC.body.projectId === p1.id)
check('ikinci katılma no-op', (await call('POST', '/projects/join', { code: invite.code }, C.accessToken)).body.joined === false)
const D = await register('pd')
// Her hesap kuruluş kabındadır (Ana sayfa/DM için) ama projesi olmayan D kimseyi görmez.
const dPeople = await call('GET', `/organizations/${org.id}/members`, null, D.accessToken)
check('projesi olmayan D kişi görmez (kuruluş kabı boş liste)', dPeople.status === 200 && dPeople.body.filter(p => p.id !== D.user.id).length === 0, `${dPeople.status} ${dPeople.body?.length}`)
check('projesi olmayan D proje görmez', (await call('GET', '/projects', null, D.accessToken)).body.length === 0)
const peopleB = (await call('GET', `/organizations/${org.id}/members`, null, B.accessToken)).body.map(p => p.id)
check('B\'nin kişi listesi: A, B, C (D yok)', [A, B, C].every(u => peopleB.includes(u.user.id)) && !peopleB.includes(D.user.id))

// --- DM yalnız ortak projede
check('B → C DM açılır (ortak P1)', (await call('POST', `/organizations/${org.id}/direct`, { userId: C.user.id }, B.accessToken)).status === 201)

// --- çıkar → tekrar katıl
const kick = await call('DELETE', `/projects/${p1.id}/admin/members/${B.user.id}`, null, A.accessToken)
check('A, B\'yi çıkardı', kick.status === 200)
check('çıkarılan B projeyi görmez', (await call('GET', '/projects', null, B.accessToken)).body.length === 0)
check('çıkarılan B kodla tekrar katılır', (await call('POST', '/projects/join', { code: invite.code }, B.accessToken)).body.joined === true)

// --- yasakla → katılamaz → kaldır
check('sahip yasaklanamaz', (await call('PUT', `/projects/${p1.id}/admin/bans/${A.user.id}`, { reason: 'x' }, A.accessToken)).status === 409)
const ban = await call('PUT', `/projects/${p1.id}/admin/bans/${B.user.id}`, { reason: 'deneme' }, A.accessToken)
check('A, B\'yi yasakladı', ban.status === 200 && ban.body.banned)
check('yasaklı B üye değil', (await call('GET', `/projects/${p1.id}/members`, null, B.accessToken)).status === 404)
const rejoin = await call('POST', '/projects/join', { code: invite.code }, B.accessToken)
check('yasaklı B tekrar katılamaz', rejoin.status === 403 && rejoin.body.code === 'PROJECT_BANNED', JSON.stringify(rejoin.body))
const bans = (await call('GET', `/projects/${p1.id}/admin/bans`, null, A.accessToken)).body
check('yasak listesinde B', bans.some(b => b.userId === B.user.id && b.reason === 'deneme'))
check('yasak kalktı', (await call('DELETE', `/projects/${p1.id}/admin/bans/${B.user.id}`, null, A.accessToken)).status === 200)
check('yasak kalkınca katılır', (await call('POST', '/projects/join', { code: invite.code }, B.accessToken)).body.joined === true)

// --- iptal edilen kod, eski uç, sayaç
const used = (await call('GET', `/projects/${p1.id}/admin/invites`, null, A.accessToken)).body.find(i => i.id === invite.id)
check('davet sayacı (B kayıt, C, B, B)', used?.useCount === 4, `${used?.useCount}`)
check('davet iptal', (await call('DELETE', `/projects/${p1.id}/admin/invites/${invite.id}`, null, A.accessToken)).status === 200)
check('iptal edilen kod listede yok', !(await call('GET', `/projects/${p1.id}/admin/invites`, null, A.accessToken)).body.some(i => i.id === invite.id))
const revoked = await call('POST', '/projects/join', { code: invite.code }, D.accessToken)
check('iptal edilen kodla katılınmaz', revoked.status === 400 && revoked.body.code === 'INVITE_REVOKED')
check('önizleme iptali gösterir', (await call('GET', `/invites/${invite.code}`)).body.reason === 'INVITE_REVOKED')
const p2Invite = (await call('POST', `/projects/${p2.id}/admin/invites/rotate`, {}, A.accessToken)).body
const legacy = await call('POST', '/organizations/join', { code: p2Invite.code }, D.accessToken)
check('eski /organizations/join proje koduyla P2\'ye sokar', legacy.status === 201 && legacy.body.id === org.id && legacy.body.projectId === p2.id)
const dProjects = (await call('GET', '/projects', null, D.accessToken)).body.map(p => p.id)
check('D yalnız P2\'yi görür', dProjects.join() === p2.id)
const peopleD = (await call('GET', `/organizations/${org.id}/members`, null, D.accessToken)).body.map(p => p.id)
check('D\'nin kişi listesi yalnız P2\'dekiler (A, D)', peopleD.length === 2 && peopleD.includes(A.user.id) && peopleD.includes(D.user.id), `${peopleD.length}`)
const dmD = await call('POST', `/organizations/${org.id}/direct`, { userId: D.user.id }, B.accessToken)
check('B → D DM reddedilir (ortak proje yok)', dmD.status === 403 && dmD.body.code === 'NO_SHARED_PROJECT', JSON.stringify(dmD.body))

// --- temizlik
for (const p of [p1, p2]) {
  await call('DELETE', `/organizations/${org.id}/projects/${p.id}`, null, A.accessToken)
  await call('DELETE', `/organizations/${org.id}/admin/projects/${p.id}/purge`, null, A.accessToken)
}
check('test projeleri kalıcı silindi', !(await call('GET', '/projects', null, A.accessToken)).body.some(p => p.id === p1.id || p.id === p2.id))

stop()
console.log(`\n${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
