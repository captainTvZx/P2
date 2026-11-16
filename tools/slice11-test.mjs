// Dilim 11 uçtan uca testi: Ayarlar > Kütüphane (hız sınırı, overlay aç/kapat, kısayollar —
// biri gerçekten tuş yakalamayla, görünüm, HUD) ve Ayarlar > Uygunluk (günler, saatler, saat
// dışında durum). Tercihlerin ayarlara yazıldığı, çalışma alanına uygulandığı (overlay kısayol
// tablosu, patcher hız sınırı) ve saat dışında durumun otomatik "Uzakta"/"Meşgul" olduğu doğrulanır.
// Tuş yalnız testin açtığı istemci penceresine gider (p2-wincap --key).
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice11-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, readFileSync } from 'node:fs'
import { testServer, TARGET } from './lib.mjs'

const { api: API } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice11'
const PROFILE = BASE + '/profile'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]

let lines = [], client
function start(env) {
  lines = []
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/ayarlar:|tercihler|uygunluk|durum/.test(l)) console.log('    [istemci] ' + l.slice(0, 220)) } })
}
const stop = async () => { client.kill(); await sleep(600) }
const wait = async (re, ms = 15000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const settings = () => JSON.parse(readFileSync(PROFILE + '/settings.json', 'utf8'))

console.log('\n[1] Kütüphane ve overlay tercihleri')
start({ P2_AUTOPILOT_SETTINGS: 'library', P2_AUTOPILOT_PREFS: 'bandwidth:25;overlay:off;hud:on;corner:bottom-left;hotkey:panel:Ctrl+Alt+O;capture:screenshot' })
check('ayarlar açıldı (kütüphane)', !!(await wait(/ayarlar: açıldı \(Library\)/)))
check('tercihler uygulandı (overlay kapalı, 25 MB/s, Ctrl+Alt+O)', !!(await wait(/tercihler uygulandı: overlay=false hız=25 kısayol paneli=Ctrl\+Alt\+O/)))
check('kısayol bekleniyor', !!(await wait(/ayarlar: kısayol bekleniyor \(screenshot\)/)))
await sleep(600)
execFileSync(BIN + 'p2-wincap.exe', ['--key', '', String(client.pid), '120']) // F9 (0x78; testinput.rs: --key <başlık> <pid> <vk ondalık>)
check('tuş yakalandı', !!(await wait(/ayarlar: tuş yakalandı f9 \(screenshot\)/)))
await sleep(500)
try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/1-kutuphane.png`]) } catch {}
let s = settings()
check('ayarlar dosyası (kütüphane)', s.library?.bandwidthMbps === 25 && s.library?.overlay === false, JSON.stringify(s.library))
check('ayarlar dosyası (overlay)', s.overlay?.hud === true && s.overlay?.toastCorner === 'bottom-left' && s.overlay?.hotkeys?.panel === 'Ctrl+Alt+O' && s.overlay?.hotkeys?.screenshot === 'F9', JSON.stringify(s.overlay?.hotkeys))
await stop()

console.log('\n[2] Uygunluk')
// Pazar 12:00 — çalışma günleri Pt–Cu: saat dışı → Meşgul
start({ P2_AUTOPILOT_SETTINGS: 'availability', P2_AUTOPILOT_PREFS: 'days:1,2,3,4,5;start:10:00;end:16:30;outside:BUSY', P2_FAKE_CLOCK: '0 12:00' })
check('uygunluk ayarları', !!(await wait(/uygunluk=\[1, 2, 3, 4, 5\] 10:00-16:30 BUSY/)))
check('saat dışı → Meşgul', !!(await wait(/uygunluk: çalışma saati dışı → BUSY/, 20000)))
// Sunucu bağlı olmayan kullanıcıyı OFFLINE gösterir: önce gerçek zamanlı bağlantıyı, sonra durumu bekle.
await wait(/olay: gerçek zamanlı bağlandı/, 15000)
let me
for (let i = 0; i < 40; i++) {
  const members = (await call('GET', `/organizations/${org.id}/members`, null, bot.accessToken)).body
  me = members.find(m => m.username === 'p2test')
  if (me?.status === 'BUSY') break
  await sleep(250)
}
check('ekip p2test\'i Meşgul görüyor', me?.status === 'BUSY', me?.status)
try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/2-uygunluk.png`]) } catch {}
await stop()
s = settings()
check('ayarlar dosyası (uygunluk)', JSON.stringify(s.availability) === JSON.stringify({ workDays: [1, 2, 3, 4, 5], workStart: '10:00', workEnd: '16:30', outsideHoursStatus: 'BUSY' }), JSON.stringify(s.availability))
// Pazartesi 12:00: çalışma saati — otomatik değiştirilmediği için dokunulmaz; durumu geri al
start({ P2_FAKE_CLOCK: '1 12:00', P2_AUTOPILOT_PRESENCE: 'ONLINE' })
await wait(/olay: gerçek zamanlı bağlandı/, 15000)
await sleep(3000)
check('çalışma saatinde otomatik değişiklik yok', !lines.some(l => /uygunluk: çalışma saati dışı/.test(l)))
await stop()

console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
