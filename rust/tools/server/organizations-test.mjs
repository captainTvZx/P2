// Kuruluş / tercih / rol / kullanıcı grubu / kanal izni / yönetim paneli sözleşme testi
// (organizations.ts, preferences.ts, roles.ts, userGroups.ts, channelPermissions.ts, admin.ts).
//   node rust/tools/server/organizations-test.mjs
// Salt okunur uçlar ana kuruluşta iki sunucuya aynı istekle sorulur. Yazan uçlar için her sunucuda
// taze bir kuruluş + iki taze kullanıcı ile aynı senaryo çalışır; cevaplar ve socket.io olayları
// maskelenip karşılaştırılır. Rust'ın yazdığı NestJS ile de okunur. Sonunda oluşturulanlar silinir.
import { createRequire } from 'node:module'
import { randomBytes } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { NEST, startRust, call, login, both as both0, same as same0, check, done, sleep, mask } from './lib.mjs'
import { writeFileSync, mkdirSync } from 'node:fs'
const DUMP = process.env.DUMP
let dumpN = 0
function same(name, a, b, replace = []) {
  const ok = same0(name, a, b, replace)
  if (!ok && DUMP) { mkdirSync(DUMP, { recursive: true }); const n = String(++dumpN).padStart(2, '0'); writeFileSync(DUMP + '/' + n + '-nest.json', JSON.stringify(mask(a, replace), null, 1)); writeFileSync(DUMP + '/' + n + '-rust.json', JSON.stringify(mask(b, replace), null, 1)); console.log('      dump ' + n) }
  return ok
}
async function both(name, base, method, path, body, tokens, replace) {
  const a = await call(NEST, method, path, body, tokens?.nest)
  const b = await call(base, method, path, body, tokens?.rust ?? tokens?.nest)
  return same(name, a, b, replace)
}

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')
const pg = require('pg')
const env = JSON.parse(readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^\uFEFF/, ''))
const db = new pg.Client({ connectionString: env.DATABASE_URL })
await db.connect()

const rust = await startRust(4121)
const owner = await login(NEST, 'p2test')
const bot = await login(NEST, 'rustbot')
const T = owner.accessToken, B = bot.accessToken
const org = (await call(NEST, 'GET', '/organizations', undefined, T)).body[0]
const O = `/organizations/${org.id}`
const project = (await call(NEST, 'GET', `${O}/projects`, undefined, T)).body[0]
const channels = (await call(NEST, 'GET', `${O}/admin/channels`, undefined, T)).body
const channel = channels.find(c => c.projectId === project.id && !c.deletedAt)
const CP = `${O}/projects/${project.id}/channels/${channel.id}/permissions`

// Değişken kodlar/sayılar: davet kodu, depolama.
function norm(value) {
  if (Array.isArray(value)) return value.map(norm)
  if (value && typeof value === 'object') {
    const out = {}
    for (const [k, v] of Object.entries(value)) out[k] = k === 'storage' ? '<storage>' : k === 'sessions' && typeof v === 'number' ? '<n>' : norm(v)
    return out
  }
  if (typeof value === 'string' && /^P2-[0-9A-F]{8}$/.test(value)) return '<code>'
  return value
}

console.log('A) salt okunur uçlar (ana kuruluş)')
const tok = t => ({ nest: t })
await both('kuruluş listesi (sahip)', rust.base, 'GET', '/organizations', undefined, tok(T))
await both('kuruluş listesi (üye)', rust.base, 'GET', '/organizations', undefined, tok(B))
await both('jetonsuz liste', rust.base, 'GET', '/organizations')
await both('üyeler', rust.base, 'GET', `${O}/members`, undefined, tok(T))
await both('üyeler ?projectId', rust.base, 'GET', `${O}/members?projectId=${project.id}`, undefined, tok(T))
await both('üyeler ?projectId boş', rust.base, 'GET', `${O}/members?projectId=`, undefined, tok(T))
await both('üyeler ?projectId yok', rust.base, 'GET', `${O}/members?projectId=yok`, undefined, tok(T))
await both('üyeler yabancı kuruluş', rust.base, 'GET', `/organizations/yok/members`, undefined, tok(T))
await both('tercihler', rust.base, 'GET', `${O}/preferences`, undefined, tok(T))
await both('tercihler (üye)', rust.base, 'GET', `${O}/preferences`, undefined, tok(B))
await both('tercihler yabancı', rust.base, 'GET', `/organizations/yok/preferences`, undefined, tok(T))
await both('roller', rust.base, 'GET', `${O}/roles`, undefined, tok(T))
await both('roller (üye)', rust.base, 'GET', `${O}/roles`, undefined, tok(B))
await both('roller yabancı', rust.base, 'GET', `/organizations/yok/roles`, undefined, tok(T))
await both('gruplar', rust.base, 'GET', `${O}/user-groups`, undefined, tok(T))
await both('gruplar yabancı', rust.base, 'GET', `/organizations/yok/user-groups`, undefined, tok(T))
await both('kanal izinleri (sahip)', rust.base, 'GET', CP, undefined, tok(T))
await both('kanal izinleri (üye)', rust.base, 'GET', CP, undefined, tok(B))
await both('kanal izinleri: kanal yok', rust.base, 'GET', `${O}/projects/${project.id}/channels/yok/permissions`, undefined, tok(T))
await both('kanal izinleri: proje uyuşmuyor', rust.base, 'GET', `${O}/projects/yok/channels/${channel.id}/permissions`, undefined, tok(T))
await both('yetkilerim (sahip)', rust.base, 'GET', `${O}/permissions/me`, undefined, tok(T))
await both('yetkilerim (üye)', rust.base, 'GET', `${O}/permissions/me`, undefined, tok(B))
await both('yetkilerim yabancı', rust.base, 'GET', `/organizations/yok/permissions/me`, undefined, tok(T))
for (const p of ['access', 'members', 'permissions', 'invites', 'projects/trash', 'channels']) {
  await both(`admin/${p}`, rust.base, 'GET', `${O}/admin/${p}`, undefined, tok(T))
  await both(`admin/${p} (üye, yetkisiz)`, rust.base, 'GET', `${O}/admin/${p}`, undefined, tok(B))
}
{
  const a = await call(NEST, 'GET', `${O}/admin/overview`, undefined, T)
  const b = await call(rust.base, 'GET', `${O}/admin/overview`, undefined, T)
  same('admin/overview (depolama hariç)', norm(a), norm(b))
}
await both('admin/overview yabancı', rust.base, 'GET', `/organizations/yok/admin/overview`, undefined, tok(T))

console.log('B) doğrulama / yetki hataları (yazmayan)')
const bad = [
  ['kuruluş: boş gövde', 'POST', '/organizations', {}],
  ['kuruluş: kısa ad', 'POST', '/organizations', { name: 'a', slug: 'abc' }],
  ['kuruluş: slug büyük harf', 'POST', '/organizations', { name: 'Ad', slug: 'ABC' }],
  ['kuruluş: slug sayı', 'POST', '/organizations', { name: 'Ad', slug: 5 }],
  ['kuruluş: slug alınmış', 'POST', '/organizations', { name: 'Ad', slug: org.slug }],
  ['katıl: kısa kod', 'POST', '/organizations/join', { code: 'abc' }],
  ['katıl: kod yok', 'POST', '/organizations/join', {}],
  ['katıl: bulunamadı', 'POST', '/organizations/join', { code: 'P2-ZZZZZZZZ' }],
  ['katıl: zaten üye', 'POST', '/organizations/join', { code: ' ' + (org.inviteCode || 'x').toLowerCase().split('').join(' ') + ' ' }],
  ['tercih: boş', 'PUT', `${O}/preferences`, {}],
  ['tercih: tür yanlış', 'PUT', `${O}/preferences`, { kind: 'X', targetId: 'a' }],
  ['tercih: notify yanlış', 'PUT', `${O}/preferences`, { kind: 'CHANNEL', targetId: 'a', notify: 'X', starred: 'evet' }],
  ['tercih: yabancı', 'PUT', `/organizations/yok/preferences`, { kind: 'CHANNEL', targetId: 'a' }],
  ['rol: boş', 'POST', `${O}/roles`, {}],
  ['rol: handle desen', 'POST', `${O}/roles`, { handle: 'Q A', name: 'QA' }],
  ['rol: handle kısa', 'POST', `${O}/roles`, { handle: 'q', name: 'QA' }],
  ['rol: ad sayı', 'POST', `${O}/roles`, { handle: 'qaa', name: 5, color: 'x'.repeat(21) }],
  ['rol: yetkisiz', 'POST', `${O}/roles`, { handle: 'qaa', name: 'QA' }, B],
  ['rol: everyone handle zaten', 'POST', `${O}/roles`, { handle: 'everyone', name: 'QA' }],
  ['rol: yama yok', 'PATCH', `${O}/roles/yok`, { name: 'QA' }],
  ['rol: yama desen', 'PATCH', `${O}/roles/yok`, { handle: '-a' }],
  ['rol: izin yok', 'PATCH', `${O}/roles/yok/permissions`, { permissions: [] }],
  ['rol: sil yok', 'DELETE', `${O}/roles/yok`],
  ['rol: üye ekle rol yok', 'POST', `${O}/roles/yok/members/${bot.user.id}`],
  ['rol: grup ekle yetkisiz', 'POST', `${O}/roles/yok/groups/yok`, undefined, B],
  ['grup: boş', 'POST', `${O}/user-groups`, {}],
  ['grup: desen', 'POST', `${O}/user-groups`, { handle: 'A!', name: 'Ad' }],
  ['grup: yetkisiz', 'POST', `${O}/user-groups`, { handle: 'abc', name: 'Ad' }, B],
  ['grup: kullanıcı adıyla çakışır', 'POST', `${O}/user-groups`, { handle: 'rustbot', name: 'Ad' }],
  ['grup: ILIKE joker', 'POST', `${O}/user-groups`, { handle: 'p2tes_', name: 'Ad' }],
  ['grup: yama yok', 'PATCH', `${O}/user-groups/yok`, { name: 'Ad' }],
  ['grup: sil yok', 'DELETE', `${O}/user-groups/yok`],
  ['grup: üye ekle doğrulama', 'POST', `${O}/user-groups/yok/members`, { userIds: 'x' }],
  ['grup: üye ekle dizi değil-dizgi', 'POST', `${O}/user-groups/yok/members`, { userIds: [1] }],
  ['grup: üye ekle grup yok', 'POST', `${O}/user-groups/yok/members`, { userIds: [] }],
  ['grup: üye çıkar grup yok', 'DELETE', `${O}/user-groups/yok/members/yok`],
  ['kanal izni: hedef geçersiz', 'PUT', `${CP}/bad/x`, { allow: [] }],
  ['kanal izni: rol yok', 'PUT', `${CP}/role/yok`, { allow: [] }],
  ['kanal izni: grup yok', 'PUT', `${CP}/GROUP/yok`, { allow: [] }],
  ['kanal izni: üye yok', 'PUT', `${CP}/user/yok`, { allow: [] }],
  ['kanal izni: üye yetkisiz', 'PUT', `${CP}/user/yok`, { allow: [] }, B],
  ['kanal izni: sil yetkisiz', 'DELETE', `${CP}/user/yok`, undefined, B],
  ['kanal izni: kanal yok', 'PUT', `${O}/projects/${project.id}/channels/yok/permissions/ROLE/x`, {}],
  ['admin: görünürlük doğrulama', 'PATCH', `${O}/admin/access/projects/${project.id}/members/${bot.user.id}`, { visible: 'evet' }],
  ['admin: proje erişimi üye yok', 'PATCH', `${O}/admin/access/projects/${project.id}/members/yok`, { visible: true }],
  ['admin: proje erişimi proje yok', 'PATCH', `${O}/admin/access/projects/yok/members/${bot.user.id}`, { visible: true }],
  ['admin: proje erişimi sahip', 'PATCH', `${O}/admin/access/projects/${project.id}/members/${owner.user.id}`, { visible: false }],
  ['admin: kanal erişimi kanal yok', 'PATCH', `${O}/admin/access/channels/yok/members/${bot.user.id}`, { visible: true }],
  ['admin: kanal erişimi sahip', 'PATCH', `${O}/admin/access/channels/${channel.id}/members/${owner.user.id}`, { visible: true }],
  ['admin: kanal erişimi yetkisiz', 'PATCH', `${O}/admin/access/channels/${channel.id}/members/${owner.user.id}`, { visible: true }, B],
  ['admin: devir yalnız sahip', 'POST', `${O}/admin/members/${owner.user.id}/transfer-owner`, undefined, B],
  ['admin: devir kendine', 'POST', `${O}/admin/members/${owner.user.id}/transfer-owner`],
  ['admin: devir üye yok', 'POST', `${O}/admin/members/yok/transfer-owner`],
  ['admin: çıkar kendini', 'DELETE', `${O}/admin/members/${owner.user.id}`],
  ['admin: çıkar yok', 'DELETE', `${O}/admin/members/yok`],
  ['admin: çıkar yetkisiz', 'DELETE', `${O}/admin/members/${owner.user.id}`, undefined, B],
  ['admin: oturum kapat yok', 'POST', `${O}/admin/members/yok/sessions/revoke`],
  ['admin: kişi yetki sahip', 'PATCH', `${O}/admin/members/${owner.user.id}/permissions`, { allow: [] }],
  ['admin: kişi yetki yok', 'PATCH', `${O}/admin/members/yok/permissions`, { allow: [] }],
  ['admin: davet doğrulama', 'POST', `${O}/admin/invites`, { maxUses: 0, expiresInDays: 1.5 }],
  ['admin: davet doğrulama 2', 'POST', `${O}/admin/invites`, { maxUses: '5' }],
  ['admin: davet doğrulama 3', 'POST', `${O}/admin/invites`, { expiresInDays: 366 }],
  ['admin: davet yetkisiz', 'POST', `${O}/admin/invites`, {}, B],
  ['admin: davet sil yok', 'DELETE', `${O}/admin/invites/yok`],
  ['admin: döndür yetkisiz', 'POST', `${O}/admin/invites/rotate`, undefined, B],
  ['admin: purge yok', 'DELETE', `${O}/admin/projects/yok/purge`],
  ['admin: purge silinmemiş', 'DELETE', `${O}/admin/projects/${project.id}/purge`],
  ['admin: ad doğrulama', 'PATCH', `${O}/admin`, { name: 'a' }],
  ['admin: ad boşluk', 'PATCH', `${O}/admin`, { name: '    ' }],
  ['admin: ad yok', 'PATCH', `${O}/admin`, {}],
  ['admin: ad yetkisiz', 'PATCH', `${O}/admin`, { name: 'Yeni' }, B],
  ['admin: ad yabancı', 'PATCH', `/organizations/yok/admin`, { name: 'Yeni' }],
]
for (const [name, method, path, body, t] of bad) await both(name, rust.base, method, path, body, tok(t || T))

console.log('C) yazan senaryo (her sunucuda taze kuruluş)')
const RELEVANT = /^(organization:|roles:changed$|permissions:changed$|usergroup:|channel:permissions$|project:(permissions|members|purged)$|group:(left|updated)$)/
const created = { orgs: [], users: [] }

function connect(base, token) {
  const s = io(`${base}/stream`, { auth: { token }, transports: ['websocket'], reconnection: false, forceNew: true })
  const client = { s, events: [] }
  s.onAny((event, payload) => { if (RELEVANT.test(event)) client.events.push([event, payload]) })
  return new Promise((resolve, reject) => { s.once('connect', () => resolve(client)); s.once('connect_error', reject) })
}
const subscribe = (client, organizationId) => client.s.timeout(5000).emitWithAck('subscribe', { organizationId }).catch(() => 'ACK-TIMEOUT')

async function register(label) {
  const username = `rt${randomBytes(3).toString('hex')}${label}`
  const r = await call(NEST, 'POST', '/auth/register', { email: `${username}@example.com`, username, displayName: `RT ${label[0]}`, password: 'Parola123', passwordConfirm: 'Parola123' })
  created.users.push(r.body.user.id)
  return { id: r.body.user.id, username, token: r.body.accessToken }
}

// Kanal kimlikleri nesne anahtarı: iki kuruluşta farklı, yalnız değer sırası karşılaştırılır.
const vals = r => ({ ...r, body: { ...r.body, channels: Object.values(r.body.channels) } })

async function scenario(base, label) {
  const R = []
  const rec = (name, res) => { R.push([name, res]); return res }
  const tag = randomBytes(3).toString('hex') + label
  const a = await register('a' + label), b = await register('b' + label)
  const reqT = (method, path, body, t = T) => call(base, method, path, body, t)

  const org = rec('kuruluş oluştur', await reqT('POST', '/organizations', { name: `  RT ${tag} `, slug: `rt-${tag}` })).body
  created.orgs.push(org.id)
  const P = `/organizations/${org.id}`
  const sockOwner = await connect(base, T)
  await subscribe(sockOwner, org.id)
  rec('liste (yeni kuruluş)', { list: (await reqT('GET', '/organizations')).body.filter(o => o.id === org.id) })
  rec('katıl (normalize)', await reqT('POST', '/organizations/join', { code: ` ${org.inviteCode.toLowerCase()} ` }, a.token))
  rec('katıl (zaten üye)', await reqT('POST', '/organizations/join', { code: org.inviteCode }, a.token))
  const sockA = await connect(base, a.token)
  await subscribe(sockA, org.id)
  rec('liste (üye, davet kodu yok)', { list: (await reqT('GET', '/organizations', undefined, a.token)).body.filter(o => o.id === org.id) })

  // Davetler
  const one = rec('davet: tek kullanımlık', await reqT('POST', `${P}/admin/invites`, { maxUses: 1, expiresInDays: 2 })).body
  rec('katıl b (tek kullanımlık)', await reqT('POST', '/organizations/join', { code: one.code }, b.token))
  rec('katıl rustbot (dolu)', await reqT('POST', '/organizations/join', { code: one.code }, B))
  rec('davetler', await reqT('GET', `${P}/admin/invites`))
  rec('davet döndür', await reqT('POST', `${P}/admin/invites/rotate`))
  rec('davetler (döndürme sonrası)', await reqT('GET', `${P}/admin/invites`))
  rec('davet sil', await reqT('DELETE', `${P}/admin/invites/${one.id}`))
  const remaining = (await reqT('GET', `${P}/admin/invites`)).body
  rec('son davet silinemez', await reqT('DELETE', `${P}/admin/invites/${remaining[0].id}`))

  // Roller ve gruplar
  const role = rec('rol oluştur', await reqT('POST', `${P}/roles`, { handle: ' QA ', name: ' Kalite ', description: '  ', color: '#fff' })).body
  rec('rol oluştur (geçersiz kimlik)', await reqT('POST', `${P}/roles`, { handle: ' QA ', name: 'Kalite' }))
  rec('rol oluştur (çakışma)', await reqT('POST', `${P}/roles`, { handle: 'qa', name: 'Kalite' }))
  rec('rol yetkisiz', await reqT('POST', `${P}/roles`, { handle: 'qb', name: 'Kalite' }, a.token))
  const roles0 = (await reqT('GET', `${P}/roles`)).body
  const founder = roles0.find(r => r.founder), everyone = roles0.find(r => r.everyone)
  rec('rol yama', await reqT('PATCH', `${P}/roles/${role.id}`, { handle: 'qa2', name: 'Kalite 2', description: ' açıklama ', color: '' }))
  rec('rol yama (aynı kimlik)', await reqT('PATCH', `${P}/roles/${role.id}`, { handle: 'qa2' }))
  rec('rol yama (çakışma)', await reqT('PATCH', `${P}/roles/${role.id}`, { handle: 'everyone' }))
  rec('rol yama (null açıklama → 500)', await reqT('PATCH', `${P}/roles/${role.id}`, { description: null }))
  rec('herkes rolü yeniden adlandırılamaz', await reqT('PATCH', `${P}/roles/${everyone.id}`, { handle: 'herkes' }))
  rec('herkes rolü ad değişir', await reqT('PATCH', `${P}/roles/${everyone.id}`, { name: 'Herkes' }))
  rec('kurucu yetkileri değişmez', await reqT('PATCH', `${P}/roles/${founder.id}/permissions`, { permissions: [] }))
  rec('rol yetkileri', await reqT('PATCH', `${P}/roles/${role.id}/permissions`, { permissions: ['usergroup.manage', 'x', 'member.roles', 'usergroup.manage', 5] }))
  rec('rol üye ekle', await reqT('POST', `${P}/roles/${role.id}/members/${a.id}`))
  rec('rol üye ekle (tekrar)', await reqT('POST', `${P}/roles/${role.id}/members/${a.id}`))
  rec('rol üye ekle (üye değil)', await reqT('POST', `${P}/roles/${role.id}/members/${bot.user.id}`))
  rec('herkes rolüne üye eklenemez', await reqT('POST', `${P}/roles/${everyone.id}/members/${a.id}`))
  rec('kurucu rolü grup', await reqT('POST', `${P}/roles/${founder.id}/groups/yok`))
  // a artık usergroup.manage + member.roles sahibi.
  const group = rec('grup oluştur (a)', await reqT('POST', `${P}/user-groups`, { handle: 'Devs', name: ' Geliştiriciler ', description: 'x', color: ' ' }, a.token)).body
  rec('grup oluştur (kullanıcı adı)', await reqT('POST', `${P}/user-groups`, { handle: b.username, name: 'Ad' }))
  rec('grup oluştur (çakışma)', await reqT('POST', `${P}/user-groups`, { handle: 'devs', name: 'Ad' }))
  rec('grup üye ekle', await reqT('POST', `${P}/user-groups/${group.id}/members`, { userIds: [b.id, 'yok', bot.user.id, b.id] }))
  rec('grup üye ekle (kimse yok)', await reqT('POST', `${P}/user-groups/${group.id}/members`, { userIds: ['yok'] }))
  rec('role grup bağla', await reqT('POST', `${P}/roles/${role.id}/groups/${group.id}`))
  rec('role grup bağla (grup yok)', await reqT('POST', `${P}/roles/${role.id}/groups/yok`))
  rec('roller', await reqT('GET', `${P}/roles`))
  rec('gruplar', await reqT('GET', `${P}/user-groups`))
  rec('admin üyeler', await reqT('GET', `${P}/admin/members`))
  rec('admin yetkiler', await reqT('GET', `${P}/admin/permissions`))
  rec('yetkilerim (b, grup üzerinden)', await reqT('GET', `${P}/permissions/me`, undefined, b.token))
  rec('grup yama', await reqT('PATCH', `${P}/user-groups/${group.id}`, { handle: 'devs2', description: '', color: '#000' }))
  rec('grup yama (aynı)', await reqT('PATCH', `${P}/user-groups/${group.id}`, { handle: 'devs2' }))
  rec('grup yama (null ad → 500)', await reqT('PATCH', `${P}/user-groups/${group.id}`, { name: null }))
  rec('rolden grup çıkar', await reqT('DELETE', `${P}/roles/${role.id}/groups/${group.id}`))
  rec('rolden üye çıkar', await reqT('DELETE', `${P}/roles/${role.id}/members/${a.id}`))
  rec('gruptan üye çıkar', await reqT('DELETE', `${P}/user-groups/${group.id}/members/${b.id}`))
  rec('herkes rolü silinemez', await reqT('DELETE', `${P}/roles/${everyone.id}`))
  rec('herkes rolü yetkileri', await reqT('PATCH', `${P}/roles/${everyone.id}/permissions`, { permissions: ['channel.view', 'message.send'] }))

  // Tercihler
  rec('tercih yaz', await reqT('PUT', `${P}/preferences`, { kind: 'CHANNEL', targetId: `t-${label}`, starred: true, extra: 1 }))
  rec('tercih güncelle', await reqT('PUT', `${P}/preferences`, { kind: 'CHANNEL', targetId: `t-${label}`, notify: 'MUTED', hidden: true }))
  rec('tercih null → 500', await reqT('PUT', `${P}/preferences`, { kind: 'PERSON', targetId: `t-${label}`, hidden: null }))
  rec('tercihler', { list: (await reqT('GET', `${P}/preferences`)).body.map(p => ({ ...p, targetId: p.targetId.replace(label, '') })) })

  // Proje ve kanal erişimi (proje NestJS ile kurulur)
  const proj = (await call(NEST, 'POST', `${P}/projects`, { name: 'Oyun', slug: 'oyun' }, T)).body
  const proj2 = (await call(NEST, 'POST', `${P}/projects`, { name: 'Başka', slug: 'baska' }, T)).body
  const chans = (await call(NEST, 'GET', `${P}/admin/channels`, undefined, T)).body.filter(c => c.projectId === proj.id)
  const ch = chans.find(c => c.name === 'genel')
  const CPP = `${P}/projects/${proj.id}/channels/${ch.id}/permissions`
  rec('erişim', await reqT('GET', `${P}/admin/access`))
  rec('proje erişimi aç (a)', await reqT('PATCH', `${P}/admin/access/projects/${proj.id}/members/${a.id}`, { visible: true }))
  rec('proje erişimi aç (a, tekrar)', await reqT('PATCH', `${P}/admin/access/projects/${proj.id}/members/${a.id}`, { visible: true }))
  rec('proje erişimi aç (b)', await reqT('PATCH', `${P}/admin/access/projects/${proj.id}/members/${b.id}`, { visible: true }))
  rec('üyeler ?projectId', await reqT('GET', `${P}/members?projectId=${proj.id}`))
  rec('üyeler ?projectId (proje 2)', await reqT('GET', `${P}/members?projectId=${proj2.id}`))
  rec('üyeler ?projectId (a, erişimsiz)', await reqT('GET', `${P}/members?projectId=${proj2.id}`, undefined, a.token))
  rec('kanal erişimi kapat (a)', await reqT('PATCH', `${P}/admin/access/channels/${ch.id}/members/${a.id}`, { visible: false }))
  rec('kanal erişimi aç (b)', await reqT('PATCH', `${P}/admin/access/channels/${ch.id}/members/${b.id}`, { visible: true }))
  rec('kanal izinleri (sahip)', await reqT('GET', CPP))
  rec('kanal izinleri (a, göremez)', await reqT('GET', CPP, undefined, a.token))
  rec('kanal izinleri (b)', await reqT('GET', CPP, undefined, b.token))
  rec('kanal izni: herkes rolü', await reqT('PUT', `${CPP}/role/${everyone.id}`, { allow: ['message.send', 'admin.panel', 'voice.video'], deny: ['voice.video', 'screen.share', 'nope'] }))
  rec('kanal izni: kurucu rolü', await reqT('PUT', `${CPP}/ROLE/${founder.id}`, { allow: ['message.send'] }))
  rec('kanal izni: grup', await reqT('PUT', `${CPP}/group/${group.id}`, { deny: ['message.send'] }))
  rec('kanal izni: grup güncelle', await reqT('PUT', `${CPP}/GROUP/${group.id}`, { allow: ['message.react'] }))
  rec('kanal izni: kişi boş → sil', await reqT('PUT', `${CPP}/user/${b.id}`, { allow: [], deny: 'x' }))
  rec('kanal izinleri (sahip, kurallarla)', await reqT('GET', CPP))
  rec('yetkilerim (a)', vals(await reqT('GET', `${P}/permissions/me`, undefined, a.token)))
  rec('yetkilerim (sahip)', vals(await reqT('GET', `${P}/permissions/me`)))
  rec('erişim (kurallarla)', await reqT('GET', `${P}/admin/access`))
  rec('kanal izni sil', await reqT('DELETE', `${CPP}/group/${group.id}`))
  rec('kanal izni sil (yok)', await reqT('DELETE', `${CPP}/bad/x`))
  rec('proje erişimi kapat (b)', await reqT('PATCH', `${P}/admin/access/projects/${proj.id}/members/${b.id}`, { visible: false }))
  rec('proje erişimi kapat (proje sahibi)', await reqT('PATCH', `${P}/admin/access/projects/${proj.id}/members/${owner.user.id}`, { visible: false }))

  // Kişi yetkileri, genel bakış, ad, kanallar, çöp kutusu
  rec('kişi yetkileri (a)', await reqT('PATCH', `${P}/admin/members/${a.id}/permissions`, { allow: ['member.kick', 'admin.panel', 'x', 'member.kick'], deny: ['member.kick', 'message.send'] }))
  rec('genel bakış', norm(await reqT('GET', `${P}/admin/overview`)))
  rec('genel bakış (a)', norm(await reqT('GET', `${P}/admin/overview`, undefined, a.token)))
  rec('ad değiştir', await reqT('PATCH', `${P}/admin`, { name: '  Yeni Ad  ' }))
  rec('liste (ad sonrası)', { list: (await reqT('GET', '/organizations')).body.filter(o => o.id === org.id) })
  await call(NEST, 'DELETE', `${P}/projects/${proj2.id}`, undefined, T)
  rec('çöp kutusu', await reqT('GET', `${P}/admin/projects/trash`))
  rec('admin kanallar', await reqT('GET', `${P}/admin/channels`))
  rec('kalıcı sil', await reqT('DELETE', `${P}/admin/projects/${proj2.id}/purge`))
  rec('kalıcı sil (tekrar)', await reqT('DELETE', `${P}/admin/projects/${proj2.id}/purge`))

  // Oturumlar, sahiplik, çıkarma
  rec('oturum kapat: sahip, a tarafından', await reqT('POST', `${P}/admin/members/${owner.user.id}/sessions/revoke`, undefined, a.token))
  rec('oturum kapat: b', await reqT('POST', `${P}/admin/members/${b.id}/sessions/revoke`, undefined, a.token))
  rec('sahiplik devri (a)', await reqT('POST', `${P}/admin/members/${a.id}/transfer-owner`))
  rec('liste (devir sonrası)', { list: (await reqT('GET', '/organizations')).body.filter(o => o.id === org.id) })
  rec('roller (devir sonrası)', await reqT('GET', `${P}/roles`))
  rec('sahiplik geri', await reqT('POST', `${P}/admin/members/${owner.user.id}/transfer-owner`, undefined, a.token))
  rec('çıkar: sahip', await reqT('DELETE', `${P}/admin/members/${owner.user.id}`, undefined, a.token))
  rec('çıkar: b', await reqT('DELETE', `${P}/admin/members/${b.id}`, undefined, a.token))
  rec('çıkar: b (tekrar)', await reqT('DELETE', `${P}/admin/members/${b.id}`, undefined, a.token))
  rec('rol sil', await reqT('DELETE', `${P}/roles/${role.id}`))
  rec('grup sil', await reqT('DELETE', `${P}/user-groups/${group.id}`))
  rec('grup sil (tekrar)', await reqT('DELETE', `${P}/user-groups/${group.id}`))
  rec('üyeler (son)', await reqT('GET', `${P}/members`))
  await sleep(500)
  sockOwner.s.close(); sockA.s.close()
  const events = { owner: sockOwner.events, a: sockA.events }
  const replace = [[a.username, '<a>'], [b.username, '<b>'], [tag, '<tag>'], [`t-${label}`, 't-']]
  return { R, events, replace, org }
}

const nestRun = await scenario(NEST, 'n')
const rustRun = await scenario(rust.base, 'r')
for (let i = 0; i < Math.max(nestRun.R.length, rustRun.R.length); i++) {
  const [name, a] = nestRun.R[i] || ['?', null], [, b] = rustRun.R[i] || ['?', null]
  same(name, norm(JSON.parse(JSON.stringify(a))), norm(JSON.parse(JSON.stringify(b))), [...nestRun.replace, ...rustRun.replace])
}
for (const who of ['owner', 'a']) {
  // Olaylar: ad + maskeli yük; sıralı karşılaştırma (emitProject ile bekleme sırası farkı olabilir → sıralı küme).
  const norm2 = (list, replace) => list.map(([e, p]) => e + ' ' + JSON.stringify(norm(p))).map(s => replace.reduce((x, [f, t]) => x.split(f).join(t), s))
    .map(s => s.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, '<uuid>').replace(/\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z/g, '<date>'))
  const en = norm2(nestRun.events[who], nestRun.replace), er = norm2(rustRun.events[who], rustRun.replace)
  same(`olaylar (${who}) sırası`, en, er)
  check(`olaylar (${who}) sayısı ${en.length}`, en.length > 0)
}

console.log('D) Rust yazdı → NestJS okur')
{
  const P = `/organizations/${rustRun.org.id}`
  for (const p of ['/roles', '/user-groups', '/members', '/preferences', '/admin/invites', '/admin/members', '/admin/permissions', '/admin/access', '/permissions/me']) {
    await both(`çapraz ${p}`, rust.base, 'GET', P + p, undefined, tok(T))
  }
  const a = await call(NEST, 'GET', '/organizations', undefined, T), b = await call(rust.base, 'GET', '/organizations', undefined, T)
  same('çapraz kuruluş listesi', a, b)
}

// Temizlik: oluşturulan kuruluşlar ve kullanıcılar (kaskad).
await db.query('delete from "Organization" where id = any($1)', [created.orgs])
await db.query('delete from "User" where id = any($1)', [created.users])
await db.end()
rust.stop()
done()
