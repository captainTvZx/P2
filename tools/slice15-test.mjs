// Dilim 15 uçtan uca testi: proje toplulukları — İSTEMCİ arayüzü (docs/proje-bazli-yapi.md, Faz 2).
// Rust istemcisi otomatik pilot kancalarıyla (masaüstüne girdi göndermeden) sürülür:
//   [1] Projesi olmayan E başlangıç ekranında davet koduyla katılır: geçersiz kod → hata kartı,
//       geçerli kod → önizleme kartı → katıl → çalışma alanında o proje (sunucudan doğrulanır).
//   [2] Proje sahibi A'nın proje yönetim paneli: Genel, Üyeler (E), yasak onayı (görünüm), Davetler
//       (kod oluştur → iptal), Roller, Gruplar (grup oluştur → E'yi ekle), İzinler, Yasakla → Yasaklılar
//       → yasağı kaldır, Ayarlar, grubu sil. Her adım sunucudan doğrulanır.
//   [3] Görünüm kancaları: rayın "＋" menüsü, katılma penceresi (önizleme + hata), projeden ayrıl
//       onayı (üye ve sahip), sunucu yönetimi (kurulum sahibi p2test: Genel, Kişiler, Projeler).
// Görüntüler koyu ve açık temada alınır (C:/p2-pg/slice15/out). Pencereler ekran dışında açılır
// (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE).
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış) + cargo build --release. Sunucu: P2_API verilmezse
// test kendi Rust sunucusunu (:4130) başlatır.   node tools/slice15-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET } from './lib.mjs'

const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice15'
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
const register = async (name, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName: `${name.toUpperCase()} ${tag}`, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body

console.log(`hazırlık: etiket ${tag}`)
const A = await register('sa')
const B = await register('sb')
const E = await register('se')
// Proje açmak sunucu yetkisi (project.create): kurulum sahibi p2test A ve B'ye verir.
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
for (const u of [A, B]) await call('PUT', `/server/users/${u.user.id}/permissions`, { permissions: ['project.create'] }, owner.accessToken)
const P = (await call('POST', '/projects', { name: `Topluluk ${tag}`, slug: `topluluk-${tag}`, description: 'Rust istemcisi topluluk testi' }, A.accessToken)).body
const Q = (await call('POST', '/projects', { name: `Komşu ${tag}`, slug: `komsu-${tag}`, description: 'Başka bir topluluk' }, B.accessToken)).body
check('A ve B kendi projelerini açtı', !!P?.id && !!Q?.id)
const invite = (await call('POST', `/projects/${P.id}/admin/invites`, { maxUses: 20 }, A.accessToken)).body
const qInvite = (await call('POST', `/projects/${Q.id}/admin/invites`, {}, B.accessToken)).body
check('davet kodları hazır', !!invite?.code && !!qInvite?.code, `${invite?.code} / ${qInvite?.code}`)
// B, P'ye katılır (üye listesi ve gruplar için ikinci kişi).
await call('POST', '/projects/join', { code: invite.code }, B.accessToken)

let lines = [], client
function start(user, env, theme = 'dark', profile = 'p') {
  lines = []
  cursor = 0
  const dir = `${BASE}/${profile}-${user}-${theme}`
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: 'tr', theme }))
  client = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123',
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/katıl:|başlangıç:|yönetim:|topluluk:|ray:|yönet:|ekran:/.test(l)) console.log('    [istemci] ' + l.slice(0, 180)) } })
}
// Sıralı bekleme: imleçten sonraki ilk eşleşme (görüntüler doğru bölümde alınsın).
let cursor = 0
const next = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = cursor; i < lines.length; i++) if (re.test(lines[i])) { cursor = i + 1; return lines[i] } await sleep(100) } return null }
const wait = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(client.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
const finish = async () => { try { client.kill() } catch {} await sleep(700) }

// ============================================================ 1) Başlangıç: davet koduyla katıl (E)
console.log('\n[1] Başlangıç ekranında davet koduyla katılma (E)')
start(E.user.username, { P2_AUTOPILOT_JOIN: `P2-DEADBEEF;${invite.code};${invite.code}|join`, P2_AUTOPILOT_JOIN_DELAY: '2500' })
check('başlangıç ekranı (projesi yok)', !!(await wait(/başlangıç: projeye katıl ya da oluştur/)))
check('geçersiz kod: INVITE_NOT_FOUND', !!(await wait(/katıl: hata INVITE_NOT_FOUND/)))
await sleep(700); shot('dark-1-baslangic-hata')
check('geçerli kod: önizleme kartı', !!(await wait(new RegExp(`katıl: önizleme Topluluk ${tag} \\(2 üye, geçerli\\)`))))
await sleep(700); shot('dark-2-baslangic-onizleme')
check('katıldı', !!(await wait(new RegExp(`katıl: Topluluk ${tag} projesine katıldın`))))
check('çalışma alanı açıldı', !!(await wait(/ekran: çalışma alanı/)))
check('rayda 1 proje', !!(await wait(/ray: 1 proje/)))
const eProjects = (await call('GET', '/projects', null, E.accessToken)).body
check('E sunucuda yalnız P\'nin üyesi', eProjects.length === 1 && eProjects[0].id === P.id)
await sleep(2500); shot('dark-3-katildi-calisma-alani')
await finish()

// ============================================================ 2) Proje yönetim paneli (A, proje sahibi)
console.log('\n[2] Proje yönetim paneli (A)')
const GROUP = `Rust Ekibi ${tag}`
const eName = E.user.username
const steps = [
  'wait:3000', 'open:members', `member:${eName}`, 'wait:3000', `confirm:ban:${eName}`, 'wait:2500', 'confirm-close',
  'open:invites', 'invite:5:7', 'wait:3000', 'revoke-last', 'wait:1200',
  'open:roles', 'wait:3000', 'open:groups', `group:${GROUP}`, `group-add:${GROUP}:${eName}`, 'wait:3000',
  'open:access', `access-user:${eName}`, 'wait:3000',
  `ban:${eName}:deneme`, 'open:bans', 'wait:3000', `unban:${eName}`, 'wait:800',
  'open:settings', 'wait:3000', 'open:channels', 'wait:2500', 'open:groups', `delete-group:${GROUP}`, 'open:overview', 'wait:600', 'close',
].join(';')
start(A.user.username, { P2_AUTOPILOT_ADMIN: steps })
check('proje paneli açıldı', !!(await wait(new RegExp(`yönetim: panel açıldı \\(Topluluk ${tag}\\)`))))
check('genel bakış yüklendi', !!(await next(/yönetim: adım wait:3000/)))
await sleep(1500); shot('dark-4-panel-genel')
check('üyeler yüklendi (3 üye)', !!(await next(/yönetim: 3 üye, \d+ rol, \d+ grup/)))
await next(new RegExp(`yönetim: adım member:${eName}`)); await sleep(1500); shot('dark-5a-panel-uyeler')
await next(new RegExp(`yönetim: adım confirm:ban:${eName}`)); await sleep(900)
shot('dark-5-panel-uyeler-yasak-onayi')
check('davet kodu oluşturuldu', !!(await next(/yönetim: Yeni davet kodu oluşturuldu/)))
await sleep(1200); shot('dark-6-panel-davetler')
check('davet kodu iptal edildi', !!(await wait(/yönetim: P2-[0-9A-F]{8} davet kodu iptal edildi/)))
const invites = (await call('GET', `/projects/${P.id}/admin/invites`, null, A.accessToken)).body
check('sunucuda etkin kodlar (varsayılan + test kodu)', invites.length === 2 && invites.some(i => i.id === invite.id), `${invites.length}`)
await next(/yönetim: bölüm Roles/); await sleep(1500); shot('dark-7-panel-roller')
check('grup oluşturuldu', !!(await wait(new RegExp(`yönetim: ${GROUP} grubu oluşturuldu`))))
check('E gruba eklendi', !!(await wait(/yönetim: Kişi gruba eklendi/)))
const groups = (await call('GET', `/projects/${P.id}/user-groups`, null, A.accessToken)).body
check('grup sunucuda, E üyesi', groups.some(g => g.name === GROUP && g.members.some(m => m.id === E.user.id)))
await sleep(900); shot('dark-8-panel-gruplar')
check('izinler yüklendi', !!(await next(/yönetim: erişim \d+ kişi, 1 proje/)))
await next(new RegExp(`yönetim: adım access-user:${eName}`)); await sleep(1500); shot('dark-9-panel-izinler')
check('E yasaklandı', !!(await wait(/yönetim: .* projeden yasaklandı\./)))
const bans = (await call('GET', `/projects/${P.id}/admin/bans`, null, A.accessToken)).body
check('yasak sunucuda (neden: deneme)', bans.some(b => b.userId === E.user.id && b.reason === 'deneme'))
check('yasaklı E projeyi görmez', (await call('GET', '/projects', null, E.accessToken)).body.length === 0)
check('yasaklılar listesi (1)', !!(await next(/yönetim: 1 yasaklı/)))
await sleep(1500); shot('dark-10-panel-yasaklilar')
check('yasak kaldırıldı', !!(await wait(/yönetim: .* yasağı kaldırıldı\./)))
check('yasak sunucudan kalktı', !(await call('GET', `/projects/${P.id}/admin/bans`, null, A.accessToken)).body.length)
await next(/yönetim: bölüm Settings/); await sleep(1500); shot('dark-11-panel-ayarlar')
check('grup silindi', !!(await wait(new RegExp(`yönetim: ${GROUP} grubu silindi`))))
check('panel kapandı', !!(await wait(/yönetim: kapandı/)))
await finish()
// E yasak kalkınca yeniden katılır (sonraki görünümler için üye).
check('E yeniden katıldı', (await call('POST', '/projects/join', { code: invite.code }, E.accessToken)).body?.joined === true)

// ============================================================ 3) Görünüm kancaları (iki tema)
console.log('\n[3] Görünümler (koyu + açık)')
for (const theme of ['dark', 'light']) {
  const t = theme
  // Rayın "＋" menüsü (E).
  start(E.user.username, { P2_AUTOPILOT_RAIL_ADD: '1' }, t, 'rail')
  check(`${t}: ray "＋" menüsü`, !!(await wait(/ray: 1 proje/)))
  await sleep(2200); shot(`${t}-12-ray-arti-menusu`)
  await finish()
  // Katılma penceresi: geçerli kod önizlemesi (Q, E üye değil).
  start(E.user.username, { P2_AUTOPILOT_JOIN: qInvite.code }, t, 'join')
  check(`${t}: katılma penceresi önizleme`, !!(await wait(new RegExp(`katıl: önizleme Komşu ${tag}`))))
  await sleep(1500); shot(`${t}-13-katil-onizleme`)
  await finish()
  // Katılma penceresi: hata (iptal edilmiş kod yerine bilinmeyen kod).
  start(E.user.username, { P2_AUTOPILOT_JOIN: 'P2-0BADC0DE' }, t, 'joinerr')
  check(`${t}: katılma penceresi hata`, !!(await wait(/katıl: hata INVITE_NOT_FOUND/)))
  await sleep(1500); shot(`${t}-14-katil-hata`)
  await finish()
  // Projeden ayrıl onayı: üye (E) ve birincil sahip (A).
  start(E.user.username, { P2_AUTOPILOT_MANAGE: 'view:leave-project' }, t, 'leave')
  check(`${t}: ayrılma onayı (üye)`, !!(await wait(/yönet: görünüm leave-project/)))
  await sleep(1500); shot(`${t}-15-ayril-onayi`)
  await finish()
  if (t === 'dark') {
    start(A.user.username, { P2_AUTOPILOT_MANAGE: 'view:leave-project' }, t, 'leaveowner')
    check(`${t}: ayrılma onayı (sahip → devret)`, !!(await wait(/yönet: görünüm leave-project/)))
    await sleep(1500); shot(`${t}-16-ayril-sahip`)
    await finish()
  }
  // Proje paneli sekmeleri (açık temada da).
  if (t === 'light') {
    start(A.user.username, { P2_AUTOPILOT_ADMIN: ['wait:3000', 'open:members', `member:${eName}`, 'wait:3000', 'open:invites', 'wait:3000', 'open:roles', 'wait:3000', 'open:groups', 'wait:3000', 'open:bans', 'wait:3000', 'open:access', 'wait:3000', 'open:settings', 'wait:3000', 'close'].join(';') }, t, 'panel')
    for (const [section, name] of [['Overview', 'genel'], ['Members', 'uyeler'], ['Invites', 'davetler'], ['Roles', 'roller'], ['Groups', 'gruplar'], ['Bans', 'yasaklilar'], ['Access', 'izinler'], ['Settings', 'ayarlar']]) {
      if (section === 'Overview') await next(/yönetim: panel açıldı/)
      else await next(new RegExp(`yönetim: bölüm ${section}`))
      await sleep(1600); shot(`${t}-panel-${name}`)
    }
    check(`${t}: panel sekmeleri`, !!(await wait(/yönetim: kapandı/)))
    await finish()
  }
  // Sunucu yönetimi (kurulum sahibi p2test): yeni panel (server_admin.rs; ayrıntılı test slice16).
  start('p2test', { P2_AUTOPILOT_SERVER_ADMIN: 'overview;users;projects' }, t, 'instance')
  check(`${t}: sunucu yönetimi açıldı`, !!(await next(/sunucu yönetimi: açıldı/)))
  await next(/sunucu yönetimi: bölüm overview/); await sleep(1600); shot(`${t}-17-sunucu-genel`)
  await next(/sunucu yönetimi: [0-9]+ kişi/); await sleep(1600); shot(`${t}-18-sunucu-kisiler`)
  check(`${t}: sunucu yönetimi projeler`, !!(await next(/sunucu yönetimi: [0-9]+ proje/)))
  await sleep(1600); shot(`${t}-19-sunucu-projeler`)
  await finish()
}

// ============================================================ temizlik
const org = (await call('GET', '/organizations', null, A.accessToken)).body[0]
for (const [p, tok] of [[P, A.accessToken], [Q, B.accessToken]]) {
  await call('DELETE', `/organizations/${org.id}/projects/${p.id}`, null, tok)
  await call('DELETE', `/organizations/${org.id}/admin/projects/${p.id}/purge`, null, tok)
  await call('DELETE', `/server/projects/${p.id}/purge`, null, owner.accessToken)
}
check('test projeleri kalıcı silindi', !(await call('GET', '/projects', null, A.accessToken)).body.some(p => p.id === P.id))
stop()
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
