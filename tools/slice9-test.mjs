// Dilim 9 uçtan uca testi: uygulama kabuğu. Tek örnek (aynı profille ikinci açılış hemen çıkar,
// argümanları ilkine iletir; `p2://play/<proje>`), kapat düğmesi (WM_CLOSE yalnız testin kendi
// açtığı istemci penceresine): "sor" → tepsiye (hatırlanır) → pencere gizlenir; ikinci açılış
// pencereyi geri getirir; hatırlanan seçim ayarlarda; "çık" seçiliyken süreç kapanır; arka planda
// gelen DM masaüstü bildirimi üretir (otomatik pilotta masaüstüne balon çıkmaz, iz düşer).
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice9-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, readFileSync, writeFileSync } from 'node:fs'
import { testServer } from './lib.mjs'

const { api: API } = await testServer()
const BIN = 'C:/p2-rust-target/release/'
const BASE = 'C:/p2-pg/slice9'
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
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]

const env = { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_CLOSE_CHOICE: 'tray', P2_FORCE_NOTIFY: '1' }
let lines = [], client, exited = null
function start(extra = {}) {
  lines = []; exited = null
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...env, ...extra } })
  client.on('exit', code => { exited = code })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/kabuk|pencere|bildirim|kütüphane: bağlantı/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
}
const wait = async (re, ms = 15000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const second = args => { try { return execFileSync(BIN + 'p2-client.exe', args, { env: { ...env, P2_AUTOPILOT_SERVER: '' }, timeout: 15000 }).toString() } catch (e) { return 'HATA ' + (e.stdout || e.message) } }
const wincap = (...args) => { try { return execFileSync(BIN + 'p2-wincap.exe', args).toString().trim() } catch (e) { return 'HATA ' + String(e.stderr || e.message).trim() } }

console.log('\n[1] Tek örnek ve p2://play')
start()
check('çalışma alanı açıldı', !!(await wait(/ekran: çalışma alanı/, 20000)))
await wait(/olay: gerçek zamanlı bağlandı/, 15000)
const t0 = Date.now()
const out = second([`p2://play/${project.id}`])
check('ikinci açılış hemen çıktı', out.includes('[tek örnek]') && Date.now() - t0 < 10000, out.trim().slice(0, 80))
check('argümanlar ilk örneğe ulaştı', !!(await wait(/kabuk: ikinci örnek \[.*p2:\/\/play/)))
check('p2://play işlendi', !!(await wait(new RegExp(`kabuk: p2://play ${project.id}`))))
check('kurulu olmayan proje kütüphanede açıldı', !!(await wait(/kütüphane: bağlantı — .* kurulu değil/)))

console.log('\n[2] Kapat düğmesi: sor → tepsiye')
check('pencere görünür', wincap('--visible', String(client.pid)) === '1')
wincap('--close', String(client.pid))
check('kapat isteği soruldu', !!(await wait(/pencere: kapat isteği \(ask\)/)))
check('seçim: tepsiye (hatırlandı)', !!(await wait(/pencere: kapatma seçimi tray \(hatırlandı\)/)))
check('pencere tepsiye gizlendi', !!(await wait(/pencere: tepsiye gizlendi \(görünür=false\)/)))
await sleep(500)
check('süreç açık, pencere görünmüyor', exited === null && wincap('--visible', String(client.pid)) === '0')
const saved = JSON.parse(readFileSync(PROFILE + '/settings.json', 'utf8'))
check('seçim ayarlara yazıldı', saved.app?.closeAction === 'tray', JSON.stringify(saved.app))

console.log('\n[3] Arka planda bildirim')
const conv = (await call('POST', `/organizations/${org.id}/direct`, { userId: owner.user.id }, bot.accessToken)).body
const dm = (await call('POST', `/organizations/${org.id}/direct/${conv.id}/messages`, { content: 'Kabuk testi: gizliyken mesaj' }, bot.accessToken)).body
check('DM masaüstü bildirimi', !!(await wait(/bildirim: Rust Bot — Kabuk testi: gizliyken mesaj \(gösterildi\)/)))
await call('DELETE', `/organizations/${org.id}/direct/${conv.id}/messages/${dm.id}`, null, bot.accessToken)

console.log('\n[4] İkinci açılış pencereyi geri getirir')
second([])
check('pencere gösterildi', !!(await wait(/pencere: gösterildi \(görünür=true\)/)))
check('pencere yeniden görünür', wincap('--visible', String(client.pid)) === '1')
await sleep(400)
try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/1-geri-geldi.png`]) } catch {}
wincap('--close', String(client.pid))
check('hatırlanan seçim: soru yok, doğrudan tepsiye', !!(await wait(/pencere: kapat isteği \(tray\)/)))
client.kill(); await sleep(800)

console.log('\n[5] "Uygulamadan çık"')
const s = JSON.parse(readFileSync(PROFILE + '/settings.json', 'utf8'))
s.app.closeAction = 'quit'
writeFileSync(PROFILE + '/settings.json', JSON.stringify(s, null, 2))
start()
check('yeniden açıldı', !!(await wait(/ekran: çalışma alanı/, 20000)))
await sleep(800)
wincap('--close', String(client.pid))
check('kapat isteği (quit)', !!(await wait(/pencere: kapat isteği \(quit\)/)))
const end = Date.now() + 8000
while (exited === null && Date.now() < end) await sleep(100)
check('süreç kapandı', exited !== null, `çıkış kodu ${exited}`)
if (exited === null) client.kill()

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
