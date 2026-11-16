// Dilim 18b uçtan uca testi: arkadaşlar ve Ana sayfa — İSTEMCİ arayüzü (docs/arkadaslar.md).
// Rust istemcisi `P2_AUTOPILOT_HOME` kancasıyla (masaüstüne girdi göndermeden) sürülür:
//   ray (Ana sayfa düğmesi + okunmamış DM baloncukları) → Arkadaşlar sekmeleri (Çevrimiçi, Tümü,
//   Bekleyen, Engellenen) → Arkadaş ekle (formdan istek; sunucudan doğrulanır) → DM (Ana sayfada
//   açılır) → kişi menüsü → profil kartı → gelen isteği kabul et → projeye dön.
// Görüntüler koyu ve açık temada alınır (C:/p2-pg/slice18/out). Pencereler ekran dışında açılır
// (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE).
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış) + cargo build --release. Sunucu: P2_API verilmezse
// test kendi Rust sunucusunu (:4130, P2_TEST_PORT) başlatır.   node tools/slice18b-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET, io, sql } from './lib.mjs'

const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice18'
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
const register = async (name, displayName, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body
const befriend = async (a, b) => { const r = (await call('POST', '/friends/requests', { userId: b.user.id }, a.accessToken)).body; return (await call('POST', `/friends/requests/${r.requestId}/accept`, null, b.accessToken)).status }
const say = async (from, to, org, ...texts) => {
  const c = (await call('POST', `/organizations/${org}/direct`, { userId: to.user.id }, from.accessToken)).body
  for (const content of texts) await call('POST', `/organizations/${org}/direct/${c.id}/messages`, { content }, from.accessToken)
  return c
}

console.log(`hazırlık: etiket ${tag}`)
const A = await register('ada', 'Ada Yıldız')
try { sql(`insert into "ServerPermission" ("userId", "permission") values ($1, 'project.create') on conflict do nothing`, [A.user.id]) } catch {}
const P = (await call('POST', '/projects', { name: `Gece Atölyesi ${tag}`, slug: `gece-atolyesi-${tag}`, description: 'Arkadaşlar testi' }, A.accessToken)).body
const org = (await call('GET', '/organizations', null, A.accessToken)).body[0]
const invite = (await call('POST', `/projects/${P.id}/admin/invites`, { maxUses: 20 }, A.accessToken)).body
check('A projesini açtı', !!P?.id && !!invite?.code)
const F1 = await register('deniz', 'Deniz Aksoy', { inviteCode: invite.code }) // arkadaş + aynı projede
const F2 = await register('mert', 'Mert Kaya') // arkadaş, ortak proje yok
const F3 = await register('elif', 'Elif Demir') // arkadaş, çevrimdışı
const R1 = await register('baran', 'Baran Çelik') // A'ya istek gönderdi
const O1 = await register('selin', 'Selin Arslan') // A istek gönderdi
const X1 = await register('kaan', 'Kaan Öztürk') // A engelledi
const Z1 = await register('zeynep', 'Zeynep Şahin', { inviteCode: invite.code }) // projede, arkadaş değil
const friendsOk = [await befriend(A, F1), await befriend(A, F2), await befriend(A, F3)]
check('A\'nın 3 arkadaşı var', friendsOk.every(s => s === 201), friendsOk.join())
const r1 = (await call('POST', '/friends/requests', { userId: A.user.id }, R1.accessToken)).body
await call('POST', '/friends/requests', { userId: O1.user.id }, A.accessToken)
await call('POST', `/blocks/${X1.user.id}`, null, A.accessToken)
await say(A, F3, org.id, 'Yarın bakarız')
await say(F3, A, org.id, 'Tamam, haber ver')
const dmF3 = (await call('GET', `/organizations/${org.id}/direct`, null, A.accessToken)).body.find(c => c.user.id === F3.user.id)
await call('POST', `/organizations/${org.id}/direct/${dmF3.id}/read`, null, A.accessToken)
const unreadSeed = async () => {
  await say(F1, A, org.id, 'Selam! Yeni sahneyi gördün mü?', 'Akşam beraber bakalım mı')
  await say(F2, A, org.id, 'Oyun gecesi cuma mı?')
}
await unreadSeed()
const before = (await call('GET', '/friends', null, A.accessToken)).body
check('hazırlık: 3 arkadaş, 1 gelen, 1 giden, 1 engelli', before.friends.length === 3 && before.incoming.length === 1 && before.outgoing.length === 1 && before.blocked.length === 1)
// F1 ve F2 çevrimiçi görünsün.
const online = [F1, F2].map(u => io(API + '/stream', { auth: { token: u.accessToken }, transports: ['websocket'] }))
await sleep(800)

let lines = [], client, cursor = 0
function start(user, env, theme) {
  lines = []
  cursor = 0
  const dir = `${BASE}/p-${user}-${theme}`
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: 'tr', theme }))
  client = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123',
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/arkadaşlar:|ana sayfa:|dm:|ekran:|ray:/.test(l)) console.log('    [istemci] ' + l.slice(0, 170)) } })
}
const next = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = cursor; i < lines.length; i++) if (re.test(lines[i])) { cursor = i + 1; return lines[i] } await sleep(100) } return null }
const wait = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(client.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
const finish = async () => { try { client.kill() } catch {} await sleep(800) }
const step = s => next(new RegExp(`ana sayfa: otomatik pilot «${s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}»`))

const u = x => x.user.username
async function tour(theme, mutate) {
  console.log(`\n[${theme}] Ana sayfa turu (A)`)
  const steps = ['wait:ray', 'online', 'all', 'pending', 'blocked', 'add', `request:${u(Z1)}`, `dm:${u(F1)}`, `menu:${u(F2)}`, 'wait:menu', `profile:${u(F3)}`, 'wait:profil', ...(mutate ? [`accept:${u(R1)}`, 'pending'] : []), 'project:0', `dm:${u(Z1)}`]
  start(u(A), { P2_AUTOPILOT_HOME: steps.join(';'), P2_AUTOPILOT_HOME_DELAY: '3200' }, theme)
  check('çalışma alanı açıldı', !!(await wait(/ekran: çalışma alanı/)))
  check('arkadaş listesi yüklendi', !!(await wait(/arkadaşlar: \d+ arkadaş, \d+ gelen, \d+ giden, 1 engelli/)))
  await step('wait:ray'); await sleep(900)
  check('ray: proje görünümü + Ana sayfa düğmesi', shot(`${theme}-1-ray-dm-baloncuklari`))
  check('Ana sayfa açıldı', !!(await step('online')) && !!(await wait(/ana sayfa: açıldı/)))
  await sleep(1500); shot(`${theme}-2-arkadaslar-cevrimici`)
  await step('all'); await sleep(1300); shot(`${theme}-3-arkadaslar-tumu`)
  await step('pending'); await sleep(1300); shot(`${theme}-4-arkadaslar-bekleyen`)
  await step('blocked'); await sleep(1300); shot(`${theme}-5-arkadaslar-engellenen`)
  await step('add'); await sleep(1300); shot(`${theme}-6-arkadas-ekle`)
  await step(`request:${u(Z1)}`)
  if (mutate) {
    check('formdan istek gönderildi', !!(await wait(/arkadaşlar: istek PENDING → Zeynep Şahin/)))
    const out = (await call('GET', '/friends', null, A.accessToken)).body.outgoing
    check('sunucuda giden istek (Zeynep)', out.some(r => r.user.id === Z1.user.id))
  } else {
    check('aynı istek tekrar: hata gösterilir', !!(await wait(/arkadaşlar: istek hatası FRIEND_REQUEST_EXISTS/)))
  }
  await sleep(1300); shot(`${theme}-7-arkadas-ekle-sonuc`)
  await step(`dm:${u(F1)}`)
  check('DM Ana sayfada açıldı', !!(await wait(new RegExp(`dm: @${u(F1)} açıldı`))))
  await sleep(2000); shot(`${theme}-8-dm`)
  await step(`menu:${u(F2)}`); await sleep(1200); shot(`${theme}-9-kisi-menusu`)
  await step(`profile:${u(F3)}`); await sleep(1500); shot(`${theme}-10-profil-karti`)
  if (mutate) {
    await step(`accept:${u(R1)}`)
    await sleep(1500)
    const now = (await call('GET', '/friends', null, A.accessToken)).body
    check('gelen istek arayüzden kabul edildi', now.friends.some(f => f.id === R1.user.id) && now.incoming.length === 0)
    await step('pending'); await sleep(1200); shot(`${theme}-11-bekleyen-kabul-sonrasi`)
  }
  check('projeye dönüldü', !!(await step('project:0')))
  await sleep(2000); shot(`${theme}-12-projeye-donus`)
  await step(`dm:${u(Z1)}`)
  check('proje kişisine mesaj: sohbet Ana sayfada açılır', !!(await wait(new RegExp(`dm: @${u(Z1)} açıldı`))) && lines.filter(l => /ana sayfa: açıldı/.test(l)).length >= 2)
  await sleep(1800); shot(`${theme}-13-proje-kisisi-dm-ana-sayfada`)
  await finish()
}

await tour('dark', true)
await unreadSeed()
await tour('light', false)

for (const s of online) s.close()
stop()
console.log(`\n${pass} geçti, ${fail} kaldı — görüntüler: ${BASE}/out`)
process.exit(fail ? 1 : 0)
