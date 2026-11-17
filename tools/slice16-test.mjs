// Dilim 16 uçtan uca testi: sunucu (kurulum) yönetimi, sunucu yetkileri, sunucu yasağı, "Beni hatırla",
// İngilizce varsayılan dil ve projesi kalmayan kişinin başlangıç ekranına dönmesi.
//   [1] Sunucu uçları (HTTP): `GET /auth/me` → `serverPermissions`; proje açmak `project.create` ister
//       (403 PROJECT_CREATE_FORBIDDEN, iki adres); kurulum sahibi her yetkiye sahip; yetki ver/al; proje
//       açma kuralı (everyone/permission); kayıt kuralı (open/invite); sunucu yasağı (giriş 403 SERVER_BANNED +
//       gerekçe, eski erişim belirteci 401, yenileme reddi, yasak kalkınca giriş); bütün projeler listesi,
//       projeye gir, sahiplik devri, sil → geri yükle → sil → kalıcı sil; oturum kapatma; 2FA sıfırlama.
//   [2] İstemci: "Beni hatırla" açıkken yeniden başlatınca oturum sürer, kapalıyken giriş ekranı gelir.
//   [3] İstemci: son projeden ayrılınca başlangıç ekranı; açılışta projesi olmayan kişi başlangıç ekranında.
//   [4] Görüntüler (koyu + açık): İngilizce giriş ekranı (dil ayarı yok), soluk "Yeni proje oluştur",
//       sunucu yönetimi sekmeleri (Genel, Kişiler + kişi ayrıntısı, Projeler + proje ayrıntısı, Ayarlar).
// Pencereler ekran dışında açılır (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE).
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış) + cargo build --release.   node tools/slice16-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, existsSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET } from './lib.mjs'

const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice16'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName: `${name.toUpperCase()} ${tag}`, password: 'Parola123', passwordConfirm: 'Parola123', ...extra }))
const login = (identifier, rememberMe = true) => call('POST', '/auth/login', { identifier, password: 'Parola123', rememberMe })
const ALL = ['project.create', 'server.view_all', 'server.manage_users', 'server.ban', 'server.manage_projects', 'server.settings', 'server.admin']

// ============================================================ 1) Sunucu uçları
console.log(`[1] Sunucu yetkileri ve yönetim uçları (etiket ${tag})`)
const O = (await login('p2test')).body
check('kurulum sahibi (p2test) girdi', !!O?.accessToken)
const OT = O.accessToken
// Kural varsayılanına dön (önceki koşudan kalmış olabilir).
await call('PATCH', '/server/settings', { projectCreation: 'permission', registrationPolicy: 'open' }, OT)
check('giriş yanıtında serverPermissions (sahip: hepsi)', JSON.stringify(O.user.serverPermissions) === JSON.stringify(ALL), O.user.serverPermissions?.join(','))
const meO = (await call('GET', '/auth/me', null, OT)).body
check('GET /auth/me serverPermissions (sahip)', JSON.stringify(meO.serverPermissions) === JSON.stringify(ALL))
const U = (await register('su')).body
const V = (await register('sv')).body
check('yeni kişilerin sunucu yetkisi yok', Array.isArray(U.user.serverPermissions) && U.user.serverPermissions.length === 0)
check('GET /auth/me serverPermissions (boş)', JSON.stringify((await call('GET', '/auth/me', null, U.accessToken)).body.serverPermissions) === '[]')
const org = (await call('GET', '/organizations', null, OT)).body[0]
// Yeni hesap kuruluş kabına MEMBER olarak girer (Ana sayfa / DM uçları), ama hiçbir projeyi görmez.
// (Kurulum kabı = ilk açılan kuruluş, `/server/overview` → organizationId.)
const instanceOrg = (await call('GET', '/server/overview', null, OT)).body.organizationId
const uOrgs = (await call('GET', '/organizations', null, U.accessToken)).body
check('yeni hesap kuruluş kabında (MEMBER), proje yok', uOrgs.length === 1 && uOrgs[0].id === instanceOrg && uOrgs[0].role === 'MEMBER' && (await call('GET', '/projects', null, U.accessToken)).body.length === 0, JSON.stringify(uOrgs.map(o => o.role)))
check('projesiz hesap DM listesine erişir', (await call('GET', `/organizations/${instanceOrg}/direct`, null, U.accessToken)).status === 200)
let r = await call('POST', '/projects', { name: `Yasak ${tag}`, slug: `yasak-${tag}` }, U.accessToken)
check('POST /projects yetkisiz → 403 PROJECT_CREATE_FORBIDDEN', r.status === 403 && r.body?.code === 'PROJECT_CREATE_FORBIDDEN', `${r.status} ${r.body?.code}`)
r = await call('POST', `/organizations/${org.id}/projects`, { name: `Yasak ${tag}`, slug: `yasak-${tag}` }, U.accessToken)
check('POST /organizations/:org/projects yetkisiz → 403', r.status === 403 && r.body?.code === 'PROJECT_CREATE_FORBIDDEN', `${r.status} ${r.body?.code}`)
r = await call('GET', '/server/overview', null, U.accessToken)
check('yetkisiz kişi sunucu yönetimine giremez', r.status === 403 && r.body?.code === 'SERVER_FORBIDDEN')
r = await call('PUT', `/server/users/${U.user.id}/permissions`, { permissions: ['project.create'] }, OT)
check('sahip U\'ya project.create verdi', r.status === 200 && r.body.permissions.includes('project.create'))
check('U /auth/me: project.create', (await call('GET', '/auth/me', null, U.accessToken)).body.serverPermissions.join() === 'project.create')
const P = (await call('POST', '/projects', { name: `Sunucu ${tag}`, slug: `sunucu-${tag}`, description: 'Sunucu yönetimi testi' }, U.accessToken)).body
check('U yetkiyle proje açtı', !!P?.id)
r = await call('PUT', `/server/users/${O.user.id}/permissions`, { permissions: [] }, OT)
check('sahibin yetkisine dokunulamaz', r.status === 409 || r.status === 403)
r = await call('PUT', `/server/users/${O.user.id}/permissions`, { permissions: [] }, U.accessToken)
check('yetki vermek server.admin ister', r.status === 403)
// Proje açma kuralı: herkes.
await call('PATCH', '/server/settings', { projectCreation: 'everyone' }, OT)
check('kural "everyone": V\'de project.create', (await call('GET', '/auth/me', null, V.accessToken)).body.serverPermissions.includes('project.create'))
const VP = (await call('POST', '/projects', { name: `Herkes ${tag}`, slug: `herkes-${tag}` }, V.accessToken))
check('kural "everyone": V proje açtı', VP.status === 201)
await call('PATCH', '/server/settings', { projectCreation: 'permission' }, OT)
check('kural "permission": V\'de yetki yok', !(await call('GET', '/auth/me', null, V.accessToken)).body.serverPermissions.includes('project.create'))
// Kayıt kuralı: yalnız davetle.
const st = (await call('PATCH', '/server/settings', { registrationPolicy: 'invite' }, OT)).body
check('kayıt kuralı invite', st.registrationPolicy === 'invite')
r = await register('sx')
check('davetsiz kayıt → 403 REGISTRATION_INVITE_ONLY', r.status === 403 && r.body?.code === 'REGISTRATION_INVITE_ONLY', `${r.status} ${r.body?.code}`)
const invite = (await call('POST', `/projects/${P.id}/admin/invites`, { maxUses: 20 }, U.accessToken)).body
r = await register('sy', { inviteCode: invite.code })
check('davetle kayıt olur ve projeye katılır', r.status === 201 && r.body.joined?.projectId === P.id)
const Y = r.body
await call('PATCH', '/server/settings', { registrationPolicy: 'open' }, OT)
// Sunucu yasağı.
await call('POST', '/projects/join', { code: invite.code }, V.accessToken)
const vOld = (await login(V.user.username)).body
r = await call('PUT', `/server/users/${V.user.id}/ban`, { reason: 'kurallara aykırı' }, OT)
check('V sunucudan yasaklandı (oturumlar kapandı)', r.status === 200 && r.body.banned === true && r.body.closed >= 1, `closed=${r.body?.closed}`)
r = await login(V.user.username)
check('yasaklı giriş → 403 SERVER_BANNED + gerekçe', r.status === 403 && r.body?.code === 'SERVER_BANNED' && r.body?.reason === 'kurallara aykırı', `${r.status} ${r.body?.message}`)
r = await call('GET', '/projects', null, vOld.accessToken)
check('eski erişim belirteci → 401 SERVER_BANNED', r.status === 401 && r.body?.code === 'SERVER_BANNED')
check('yasaklı DM uçlarına erişemez', (await call('GET', `/organizations/${instanceOrg}/direct`, null, vOld.accessToken)).status === 401)
r = await call('POST', '/auth/refresh', { refreshToken: vOld.refreshToken })
check('yenileme reddedildi', r.status === 401)
r = await call('PUT', `/server/users/${O.user.id}/ban`, {}, U.accessToken)
check('yetkisiz yasak → 403', r.status === 403)
r = await call('PUT', `/server/users/${O.user.id}/ban`, {}, OT)
check('kendini yasaklayamaz', r.status === 409)
let users = (await call('GET', '/server/users', null, OT)).body
const vRow = users.find(u => u.id === V.user.id)
check('kişi listesinde yasak + gerekçe', vRow?.ban?.reason === 'kurallara aykırı')
const uRow = users.find(u => u.id === U.user.id)
check('kişi listesinde projeler ve yetkiler', uRow?.projects.some(p => p.id === P.id && p.primaryOwner) && uRow.permissions.includes('project.create'))
r = await call('DELETE', `/server/users/${V.user.id}/ban`, null, OT)
check('yasak kaldırıldı', r.status === 200 && r.body.banned === false)
check('yasak kalkınca giriş', (await login(V.user.username)).status === 201)
// Oturum kapatma ve 2FA sıfırlama.
r = await call('POST', `/server/users/${Y.user.id}/sessions/revoke`, null, OT)
check('oturumlar kapatıldı', r.status === 201 && r.body.closed >= 1)
r = await call('POST', `/server/users/${Y.user.id}/2fa/reset`, null, OT)
check('2FA sıfırlandı', r.status === 201 && r.body.twoFactorEnabled === false)
// Bütün projeler (sahip P'nin üyesi değil).
let projects = (await call('GET', '/server/projects', null, OT)).body
const pRow = projects.find(p => p.id === P.id)
check('bütün projeler: P (sahip üye değil)', pRow && pRow.member === false && pRow.owner?.id === U.user.id && pRow.members >= 2, `members=${pRow?.members}`)
check('server.view_all olmadan liste yok', (await call('GET', '/server/projects', null, U.accessToken)).status === 403)
r = await call('POST', `/server/projects/${P.id}/transfer`, { userId: Y.user.id }, OT)
check('sahiplik devri (U → Y)', r.status === 201 && r.body.owner === Y.user.id)
check('Y artık birincil sahip', (await call('GET', '/projects', null, Y.accessToken)).body.find(p => p.id === P.id)?.primaryOwner === true)
r = await call('POST', `/server/projects/${P.id}/enter`, null, OT)
check('sahip projeye girdi (OWNER üye)', r.status === 201 && r.body.project?.id === P.id && r.body.project?.role === 'OWNER')
const Q = (await call('POST', '/projects', { name: `Çöp ${tag}`, slug: `cop-${tag}` }, U.accessToken)).body
r = await call('DELETE', `/server/projects/${Q.id}`, null, OT)
check('proje silindi (geri dönüşüm)', r.status === 200 && !!r.body.purgeAt)
check('silinen proje listede deletedAt ile', !!(await call('GET', '/server/projects', null, OT)).body.find(p => p.id === Q.id)?.deletedAt)
r = await call('POST', `/server/projects/${Q.id}/restore`, null, OT)
check('proje geri yüklendi', r.status === 200 && (await call('GET', '/projects', null, U.accessToken)).body.some(p => p.id === Q.id))
r = await call('DELETE', `/server/projects/${Q.id}/purge`, null, OT)
check('silinmemiş proje kalıcı silinmez', r.status === 409)
await call('DELETE', `/server/projects/${Q.id}`, null, OT)
r = await call('DELETE', `/server/projects/${Q.id}/purge`, null, OT)
check('kalıcı silme', r.status === 200 && !(await call('GET', '/server/projects', null, OT)).body.some(p => p.id === Q.id))
const ov = (await call('GET', '/server/overview', null, OT)).body
check('genel bakış (sayılar + sürüm)', ov.users >= 4 && ov.projects >= 1 && typeof ov.version === 'string', `users=${ov.users} projects=${ov.projects} v=${ov.version}`)
// Yetki ile panel erişimi (server.ban).
await call('PUT', `/server/users/${U.user.id}/permissions`, { permissions: ['project.create', 'server.ban'] }, OT)
const meU = (await call('GET', '/server/me', null, U.accessToken)).body
check('server.ban ile panel açılır', meU.panel === true && meU.serverPermissions.includes('server.ban'))
check('server.ban ile ayar değiştirilemez', (await call('PATCH', '/server/settings', { registrationPolicy: 'invite' }, U.accessToken)).status === 403)
await call('PUT', `/server/users/${U.user.id}/permissions`, { permissions: ['project.create'] }, OT)

// Yalnız sunucu uçları (`P2_SLICE16_SERVER_ONLY=1`): istemci bölümleri atlanır.
const serverOnly = !!process.env.P2_SLICE16_SERVER_ONLY

// ============================================================ İstemci yardımcıları
let lines = [], client
let cursor = 0
function start(user, env, theme = 'dark', profile = 'p', opts = {}) {
  lines = []
  cursor = 0
  const dir = `${BASE}/${profile}-${user || 'anon'}-${theme}`
  if (!opts.keep) {
    rmSync(dir, { recursive: true, force: true })
    mkdirSync(dir, { recursive: true })
    // Dil ayarı yazılmaz: varsayılan İngilizce olmalı (opts.language verilirse yazılır).
    writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, theme, ...(opts.language ? { language: opts.language } : {}) }))
  }
  const auto = user ? { P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123' } : {}
  client = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, ...auto,
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/sunucu yönetimi:|ray:|ekran:|yönet:|başlangıç:|sunucu/.test(l)) console.log('    [istemci] ' + l.slice(0, 180)) } })
  return dir
}
const next = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = cursor; i < lines.length; i++) if (re.test(lines[i])) { cursor = i + 1; return lines[i] } await sleep(100) } return null }
const wait = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(client.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
const finish = async () => { try { client.kill() } catch {} await sleep(800) }

if (!serverOnly) {
// ============================================================ 2) Beni hatırla
console.log('\n[2] "Beni hatırla" (yeniden başlatma)')
let dir = start(U.user.username, {}, 'dark', 'remember')
check('hatırla açık: çalışma alanı', !!(await wait(/ekran: çalışma alanı/)))
await sleep(800)
check('oturum dosyası yazıldı', existsSync(dir + '/session.bin'))
await finish()
// Aynı profille, otomatik giriş olmadan yeniden başlat.
client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: dir, P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1' } })
lines = []; cursor = 0
client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) lines.push(l) })
const restored = await wait(/ekran: (çalışma alanı|giriş)/)
check('yeniden başlatınca oturum sürüyor', /çalışma alanı/.test(restored || ''), restored || 'yok')
await finish()
dir = start(U.user.username, { P2_AUTOPILOT_REMEMBER: '0' }, 'dark', 'forget')
check('hatırla kapalı: çalışma alanı', !!(await wait(/ekran: çalışma alanı/)))
await sleep(800)
check('oturum dosyası yok', !existsSync(dir + '/session.bin'))
await finish()
client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: dir, P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1' } })
lines = []; cursor = 0
client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) lines.push(l) })
const forgot = await wait(/ekran: (çalışma alanı|giriş)/)
check('hatırla kapalıyken yeniden başlatınca giriş ekranı', /giriş/.test(forgot || ''), forgot || 'yok')
await finish()

// ============================================================ 3) Projesi kalmayınca başlangıç ekranı
console.log('\n[3] Son projeden ayrılınca başlangıç ekranı')
const W = (await register('sw')).body
await call('POST', '/projects/join', { code: invite.code }, W.accessToken)
start(W.user.username, { P2_AUTOPILOT_MANAGE: 'leave-project' }, 'dark', 'leave')
check('W çalışma alanında (1 proje)', !!(await wait(/ray: 1 proje/)))
check('son projeden ayrıldı → başlangıç ekranı', !!(await wait(/ray: proje kalmadı, başlangıç ekranına dönülüyor/)) && !!(await wait(/ekran: kuruluş/)))
check('W sunucuda projesiz', (await call('GET', '/projects', null, W.accessToken)).body.length === 0)
await sleep(1200); shot('dark-ayrilinca-baslangic')
await finish()
start(W.user.username, {}, 'dark', 'empty')
check('açılışta projesi yok → başlangıç ekranı', !!(await wait(/ekran: kuruluş/)) && !lines.some(l => /ekran: çalışma alanı/.test(l)))
await finish()

// ============================================================ 4) Görüntüler (koyu + açık)
console.log('\n[4] Görüntüler')
await call('POST', '/projects/join', { code: invite.code }, W.accessToken)
for (const theme of ['dark', 'light']) {
  const t = theme
  // Giriş ekranı: dil ayarı yok → İngilizce.
  start(null, {}, t, 'login')
  check(`${t}: giriş ekranı`, !!(await wait(/ekran: giriş/)))
  await sleep(1800); shot(`${t}-1-giris-en`)
  await finish()
  // Türkçe giriş ekranı (dil seçili).
  if (t === 'dark') {
    start(null, {}, t, 'login-tr', { language: 'tr' })
    await wait(/ekran: giriş/); await sleep(1800); shot(`${t}-1b-giris-tr`)
    await finish()
  }
  // Soluk "Yeni proje oluştur" (W'nin project.create yetkisi yok).
  start(W.user.username, { P2_AUTOPILOT_RAIL_ADD: '1' }, t, 'rail')
  check(`${t}: ray "＋" menüsü (yetkisiz)`, !!(await wait(/ray: 1 proje/)))
  await sleep(2200); shot(`${t}-2-ray-olustur-soluk`)
  await finish()
  // Sunucu yönetimi sekmeleri (kurulum sahibi).
  const steps = ['overview', 'wait:2500', 'users', 'wait:2500', `user:${U.user.username}`, 'wait:2500', 'projects', 'wait:2500', `project:Sunucu ${tag}`, 'wait:2500', 'settings'].join(';')
  start('p2test', { P2_AUTOPILOT_SERVER_ADMIN: steps }, t, 'srv')
  check(`${t}: sunucu yönetimi açıldı`, !!(await next(/sunucu yönetimi: açıldı/)))
  await next(/sunucu yönetimi: bölüm overview/); await sleep(1800); shot(`${t}-3-sunucu-genel`)
  await next(/sunucu yönetimi: \d+ kişi/); await sleep(1200); shot(`${t}-4-sunucu-kisiler`)
  await next(/sunucu yönetimi: adım user:/); await sleep(1200); shot(`${t}-5-sunucu-kisi-ayrinti`)
  await next(/sunucu yönetimi: \d+ proje/); await sleep(1200); shot(`${t}-6-sunucu-projeler`)
  await next(/sunucu yönetimi: adım project:/); await sleep(1200); shot(`${t}-7-sunucu-proje-ayrinti`)
  check(`${t}: ayarlar sekmesi`, !!(await next(/sunucu yönetimi: bölüm settings/)))
  await sleep(1800); shot(`${t}-8-sunucu-ayarlar`)
  await finish()
}

}

// ============================================================ temizlik
for (const p of [P, VP.body]) {
  if (!p?.id) continue
  await call('DELETE', `/server/projects/${p.id}`, null, OT)
  await call('DELETE', `/server/projects/${p.id}/purge`, null, OT)
}
check('test projeleri kalıcı silindi', !(await call('GET', '/server/projects', null, OT)).body.some(p => p.id === P.id))
stop()
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
