// Dilim 4c uçtan uca testi: arama kalanları, 3 kişilik örgü. İki Rust istemcisi (p2test ve
// rustbot, ayrı profiller) ve arama botu (rustbot2) aynı ses odasında:
//   kamera (test deseni) → karşı istemcide görüntü; kamera açıkken ekran paylaşımı kamerayı
//   değiştirir; izleyici kalite ister (480p) → gönderen kodlayıcıyı sınırlar ve gelen çözünürlük
//   düşer; kişi başına dinleme sesi; yönetici susturması (call:moderate → karşı taraf susar).
// Gerçek mikrofon/hoparlör/kamera kullanılmaz (P2_TEST_TONE, P2_SILENT_AUDIO, P2_TEST_CAMERA).
// UYARI: ses motorunu (WebRTC/ses yolu) gerçekten çalıştırır; geliştirme makinesinde Windows Audio'yu
// kilitleyebilir. Yalnız ayrı bir test makinesinde çalıştır; ses mantığı için: cargo test -p p2-media.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice4c-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import { testServer, Client, TARGET, testEnv } from './lib.mjs'

const { api: API } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice4c'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// --- hazırlık: üçüncü kullanıcı (arama botu) kuruluşta ve projede
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const second = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
let third = (await call('POST', '/auth/login', { identifier: 'rustbot2', password: 'Parola123' })).body
if (!third?.accessToken) third = (await call('POST', '/auth/register', { email: 'rustbot2@p2.test', username: 'rustbot2', displayName: 'Rust Bot 2', password: 'Parola123', passwordConfirm: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
// Proje bazlı yapı: kuruluş kodu yok; kişi projeye proje davet koduyla katılır (bkz. aşağıda proje bulununca).
const db = new Client({ connectionString: testEnv().DATABASE_URL })
await db.connect()
const project = (await db.query(`select id from "Project" where "organizationId" = $1 and "deletedAt" is null order by "createdAt" limit 1`, [org.id])).rows[0]
if (!(await call('GET', '/projects', null, third.accessToken)).body.some(p => p.id === project.id)) {
  const inv = (await call('POST', `/projects/${project.id}/admin/invites`, { maxUses: 1 }, owner.accessToken)).body
  await call('POST', '/projects/join', { code: inv.code }, third.accessToken)
}
for (const u of [owner.user.id, second.user.id, third.user.id]) await db.query(`insert into "ProjectMember" ("projectId", "userId") values ($1, $2) on conflict do nothing`, [project.id, u])
await db.end()
const short = id => id.slice(0, 8)
console.log(`hazırlık: kuruluş rolü (p2test) ${org.role}; A=${short(owner.user.id)} B=${short(second.user.id)} bot=${short(third.user.id)}`)

function run(exe, env, tag, quiet = /olay: (durum|activity)|ses: mikrofon|kanallar:|etkinlik:/) {
  const lines = []
  const proc = spawn(exe, [], { env: { ...process.env, ...env } })
  const take = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (!quiet.test(l) || process.env.VERBOSE) console.log(`    [${tag}] ${l.slice(0, 200)}`) } }
  proc.stdout.on('data', take)
  proc.stderr.on('data', take)
  const wait = async (re, ms = 15000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(100) } return null }
  return { proc, lines, wait, kill: async () => { proc.kill(); await sleep(500) } }
}
const common = { P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_PASS: 'Parola123', P2_TEST_TONE: '1', P2_SILENT_AUDIO: '1', P2_TRACE_AUDIO: '1', P2_TEST_CAMERA: '1', P2_TEST_PATTERN: '1', P2_AUTOPILOT_VOICE: 'ekip-odası', P2_AUTOPILOT_CHANNEL: 'genel', P2_TRACE_RTC: '1' }
const shot = (pid, name) => { try { execFileSync(BIN + 'p2-wincap.exe', [String(pid), `${BASE}/out/${name}.png`]) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)) } }
const pollFrames = async (lines, user, pred, ms) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = frames(lines, user).find(pred); if (hit) return hit; await sleep(250) } return frames(lines, user).slice(-1)[0] || null }
const frames = (lines, user) => lines.map(l => new RegExp(`görüntü: ${short(user)} (\\d+)x(\\d+) (\\d+) kare/sn`).exec(l)).filter(Boolean).map(m => ({ w: +m[1], h: +m[2], fps: +m[3] }))

// ============================================================ Oda: bot + B + A
console.log('\n[1] Üç kişilik ses odası')
const bot = run(BIN + 'p2-callbot.exe', { P2_API: API, P2_BOT_USER: 'rustbot2', P2_BOT_PASS: 'Parola123', P2_CALL_CHANNEL: 'ekip-odası' }, 'bot', /^$/)
await sleep(2500)
const b = run(BIN + 'p2-client.exe', { ...common, P2_PROFILE_DIR: BASE + '/profile-b', P2_AUTOPILOT_USER: 'rustbot', P2_AUTOPILOT_QUALITY: '480p:40' }, 'B')
await sleep(2500)
// A: 3 sn sonra kamera, 14 sn sonra ekran paylaşımı (kamerayı geçer), 6 sn sonra B'yi susturur, B'nin sesini %50 yapar
const a = run(BIN + 'p2-client.exe', { ...common, P2_PROFILE_DIR: BASE + '/profile-a', P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_CAMERA: '3', P2_AUTOPILOT_SCREEN: '14', P2_AUTOPILOT_MODERATE: '8:Rust Bot', P2_AUTOPILOT_VOLUME: '0.5' }, 'A')
check('A odaya katıldı (3 kişi)', !!(await a.wait(/olay: arama durumu channel \(3 kişi\)/, 30000)))
const meshA = async () => { const end = Date.now() + 25000; while (Date.now() < end) { const ok = [second.user.id, third.user.id].every(u => a.lines.some(l => l.includes(`arama: eş ${short(u)} connected`)) ); if (ok) return true; await sleep(300) } return false }
check('A iki eşe de bağlandı (örgü)', await meshA())

// ============================================================ Kamera
console.log('\n[2] Kamera')
check('A kamerayı açtı', !!(await a.wait(/arama: kamera açık/, 15000)))
const cam = await pollFrames(b.lines, owner.user.id, f => f.fps >= 5 && Math.abs(f.w / f.h - 4 / 3) < 0.05, 12000)
check('B, A\'nın kamerasını alıyor (4:3; çözünürlük bant genişliğine uyarlanır)', !!cam && Math.abs(cam.w / cam.h - 4 / 3) < 0.05, JSON.stringify(cam || {}))
const botSaw = await bot.wait(new RegExp(`${short(owner.user.id)}: .*görüntü: [1-9][\\d.]* kare/sn \\d+x\\d+`), 12000)
check('arama botu da kamera görüntüsünü alıyor', !!botSaw, botSaw || '')
shot(b.proc.pid, '1-B-kamera')

// ============================================================ Kişi sesi + moderasyon
console.log('\n[3] Kişi başına ses, moderasyon')
check('A, B\'nin sesini %50 yaptı', !!(await a.wait(new RegExp(`arama: ${short(second.user.id)} sesi %50`), 10000)))
check('A (yönetici) B\'yi susturdu', !!(await a.wait(new RegExp(`arama: ${short(second.user.id)} susturuldu`), 15000)))
check('B susturuldu (call:force-mute)', !!(await b.wait(/arama: mikrofon kapalı/, 10000)))

// ============================================================ Ekran paylaşımı + izleyici kalitesi
console.log('\n[4] Ekran paylaşımı ve izleyici kalitesi')
check('A ekran paylaşımına geçti', !!(await a.wait(/arama: ekran paylaşımı açık/, 25000)))
const before = await pollFrames(b.lines, owner.user.id, f => f.w === 1280 && f.h === 720, 15000)
check('B paylaşımı 720p alıyor (kamera yerine)', before?.w === 1280 && before?.h === 720, JSON.stringify(before || {}))
check('B 480p istedi', !!(await b.wait(/yayını 480p istendi/, 45000)))
check('A isteği aldı', !!(await a.wait(new RegExp(`arama: ${short(second.user.id)} yayınımı 480p istiyor`), 10000)))
check('A kodlayıcı sınırını uyguladı', !!(await a.wait(new RegExp(`\\[rtc\\] ${second.user.id} kodlayıcı sınırları uygulandı`), 5000)))
const asked = b.lines.findIndex(l => /yayını 480p istendi/.test(l))
const after = await (async () => { const end = Date.now() + 15000; while (Date.now() < end) { const hit = frames(b.lines.slice(asked), owner.user.id).find(f => f.h < 720 && f.h >= 400 && Math.abs(f.w / f.h - 16 / 9) < 0.05); if (hit) return hit; await sleep(250) } return null })()
check('B\'nin aldığı çözünürlük düştü', !!after && after.h < 720 && after.h >= 400, JSON.stringify(after || {}))
shot(b.proc.pid, '2-B-480p')
shot(a.proc.pid, '3-A-sahne')

await a.kill(); await b.kill(); await bot.kill()
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
