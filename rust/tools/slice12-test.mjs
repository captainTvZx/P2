// Dilim 12 uçtan uca testi: ekran üstü çizim (izleyici kalemi + yayıncının masaüstü kaplaması),
// bilgisayar sesi paylaşımının sinyal/karışım yolu ve arama balonu. Rust istemcisi (p2test) ve
// arama botu (rustbot2) aynı ses odasında.
//
// SES AYGITI KULLANILMAZ: istemci P2_NO_AUDIO_DEVICE ile hiçbir ses aygıtını açmaz (mikrofon,
// hoparlör, WASAPI loopback — listeleme dahil); bilgisayar sesi motorun sentetik halkasından
// gelir (gerçek loopback yalnız birim testli: `cargo test -p p2-media`). Bot da aygıtsızdır.
// Girdi YALNIZ testin başlattığı istemcinin pencerelerine PostMessage ile gider (p2-wincap).
// Önkoşul: test PostgreSQL (:55432) + P2 sunucusu (P2_API, varsayılan NestJS :4110).
//   node rust/tools/slice12-test.mjs <P2 repo yolu>
import { spawn, execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { mkdirSync, rmSync, readFileSync, writeFileSync, existsSync } from 'node:fs'
import { inflateSync } from 'node:zlib'

const repo = process.argv[2]
const require = createRequire(repo + '/package.json')
const { Client } = require('pg')
const API = process.env.P2_API || 'http://127.0.0.1:4110'
const BIN = 'C:/p2-rust-target/release/'
const BASE = 'C:/p2-pg/slice12'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// --- hazırlık: bot kullanıcısı kuruluşta ve projede
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
let bot2 = (await call('POST', '/auth/login', { identifier: 'rustbot2', password: 'Parola123' })).body
if (!bot2?.accessToken) bot2 = (await call('POST', '/auth/register', { email: 'rustbot2@p2.test', username: 'rustbot2', displayName: 'Rust Bot 2', password: 'Parola123', passwordConfirm: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
if (!(await call('GET', '/organizations', null, bot2.accessToken)).body.some(o => o.id === org.id)) await call('POST', '/organizations/join', { code: org.inviteCode }, bot2.accessToken)
const db = new Client({ connectionString: 'postgresql://p2@127.0.0.1:55432/p2_spike' })
await db.connect()
const project = (await db.query(`select id from "Project" where "organizationId" = $1 and "deletedAt" is null order by "createdAt" limit 1`, [org.id])).rows[0]
for (const u of [owner.user.id, bot2.user.id]) await db.query(`insert into "ProjectMember" ("projectId", "userId") values ($1, $2) on conflict do nothing`, [project.id, u])
await db.end()
const short = id => id.slice(0, 8)
const A = short(owner.user.id), B = short(bot2.user.id)
console.log(`hazırlık: A=${A} bot=${B}`)

function run(exe, env, tag, quiet = /olay: (durum|activity)|ses: mikrofon|kanallar:|etkinlik:|görüntü: |kaplama: \d+ çizgi|çizim: [0-9a-f]{8} → .* çizgi bot-|imleç alındı/) {
  const lines = []
  const proc = spawn(exe, [], { env: { ...process.env, ...env } })
  const take = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (!quiet.test(l) || process.env.VERBOSE) console.log(`    [${tag}] ${l.slice(0, 200)}`) } }
  proc.stdout.on('data', take)
  proc.stderr.on('data', take)
  const wait = async (re, ms = 15000, from = 0) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.slice(from).find(l => re.test(l)); if (hit) return hit; await sleep(100) } return null }
  return { proc, lines, wait, kill: async () => { proc.kill(); await sleep(500) } }
}
const wincap = (...args) => { try { return execFileSync(BIN + 'p2-wincap.exe', args.map(String), { timeout: 15000 }).toString() } catch (e) { return 'HATA ' + String(e.stderr || e.message).trim().slice(0, 200) } }
const windows = pid => wincap('--list', pid).split(/\r?\n/).filter(Boolean).map(l => { const [hwnd, title, rect, ex] = l.split('\t'); const [x, y, w, h] = (rect || '').split(',').map(Number); return { hwnd, title, x, y, w, h, ex: parseInt((ex || 'ex=0').slice(3), 16) } })
const mainWin = pid => windows(pid).filter(w => !/kaplama|balon/.test(w.title)).sort((a, b) => b.w * b.h - a.w * a.h)[0]

// PNG (8 bit RGBA/RGB, taramasız) çözücü: belirli renge yakın piksel sayısı.
function countColor(file, [r0, g0, b0], tol = 40) {
  const buf = readFileSync(file)
  let pos = 8, w = 0, h = 0, ct = 6, idat = []
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos), type = buf.toString('ascii', pos + 4, pos + 8), data = buf.subarray(pos + 8, pos + 8 + len)
    if (type === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); ct = data[9] }
    if (type === 'IDAT') idat.push(data)
    pos += 12 + len
  }
  const bpp = ct === 6 ? 4 : 3, raw = inflateSync(Buffer.concat(idat)), stride = w * bpp
  const prev = Buffer.alloc(stride), cur = Buffer.alloc(stride)
  let hits = 0
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1))
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0, b = prev[i], c = i >= bpp ? prev[i - bpp] : 0
      let v = line[i]
      if (f === 1) v += a; else if (f === 2) v += b; else if (f === 3) v += (a + b) >> 1
      else if (f === 4) { const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c }
      cur[i] = v & 255
    }
    for (let x = 0; x < w; x++) { const i = x * bpp; if (Math.abs(cur[i] - r0) < tol && Math.abs(cur[i + 1] - g0) < tol && Math.abs(cur[i + 2] - b0) < tol && (bpp === 3 || cur[i + 3] > 128)) hits++ }
    cur.copy(prev)
  }
  return { w, h, hits }
}
const hex = s => [1, 3, 5].map(i => parseInt(s.slice(i, i + 2), 16))
const PENS = ['#ff6b6b', '#ffa94d', '#ffd43b', '#69db7c', '#38d9a9', '#4dabf7', '#748ffc', '#da77f2', '#f783ac', '#63e6be']
const penColor = key => { let h = 0; for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0; return PENS[h % PENS.length] }

const PROFILE = BASE + '/profile-a'
const env = {
  P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123',
  P2_NO_AUDIO_DEVICE: '1', P2_TEST_TONE: '1', P2_SILENT_AUDIO: '1', P2_SILENT_NOTIFY: '1', P2_NO_TRAY: '1',
  P2_TEST_PATTERN: '1', P2_TEST_OVERLAY: '1', P2_TRACE_AUDIO: '1', P2_TRACE_WIDGET: '1',
  P2_AUTOPILOT_VOICE: 'ekip-odası', P2_AUTOPILOT_CHANNEL: 'genel',
  P2_AUTOPILOT_MUTE_AFTER: '1',            // mikrofon kapalı: bot tarafında yalnız bilgisayar sesi duyulur
  P2_AUTOPILOT_DRAW: '1',                  // botun yayınında kalemi aç, çizim alanını yaz (bot görüntüsü gelince)
  P2_AUTOPILOT_SCREEN: '6', P2_AUTOPILOT_SCREEN_STOP: '16', P2_AUTOPILOT_SCREEN_AUDIO: '1',
}

// ============================================================ Oda
console.log('\n[1] Ses odası (aygıtsız)')
const bot = run(BIN + 'p2-callbot.exe', { P2_API: API, P2_BOT_USER: 'rustbot2', P2_BOT_PASS: 'Parola123', P2_CALL_CHANNEL: 'ekip-odası', P2_BOT_DRAW: '1' }, 'bot', /← |çizim alındı|imleç alındı|çiziyor/)
await sleep(2500)
const a = run(BIN + 'p2-client.exe', env, 'A')
check('A odaya katıldı', !!(await a.wait(/olay: arama durumu channel \(2 kişi\)|arama: katıldın \(channel, 2 kişi\)/, 30000)))
check('A bota bağlandı', !!(await a.wait(new RegExp(`arama: eş ${B} connected`), 25000)))
check('A ses aygıtı açmadı (aygıtlar giriş=- çıkış=-)', !!(await a.wait(/arama: aygıtlar giriş=- çıkış=-/, 5000)))
check('A mikrofonu kapattı', !!(await a.wait(/arama: mikrofon kapalı/, 10000)))

// ============================================================ Yayın + kaplama + bilgisayar sesi
console.log('\n[2] Yayıncı: masaüstü kaplaması ve bilgisayar sesi')
check('A ekran paylaşımına başladı', !!(await a.wait(/arama: ekran paylaşımı açık/, 45000)))
const shareFrom = bot.lines.length
check('bilgisayar sesi açıldı (aygıtsız sentetik halka)', !!(await a.wait(/arama: bilgisayar sesi açık \(sentetik\)/, 5000)))
const opened = await a.wait(/kaplama: açıldı/, 8000)
check('çizim kaplaması açıldı', !!opened, opened || '')
const overlay = windows(a.proc.pid).find(w => /çizim kaplaması/.test(w.title))
const EX = { TOPMOST: 0x8, TRANSPARENT: 0x20, TOOLWINDOW: 0x80, LAYERED: 0x80000, NOACTIVATE: 0x08000000 }
check('kaplama: en üstte, tıklama geçirir, odak almaz, görev çubuğunda yok', !!overlay && Object.values(EX).every(f => (overlay.ex & f) === f), overlay ? `ex=0x${overlay.ex.toString(16)}` : 'yok')
if (overlay) {
  const hit = wincap('--hit', overlay.x + (overlay.w >> 1), overlay.y + (overlay.h >> 1))
  check('kaplamanın ortasına "tıklama" alttaki pencereye gider', !/kaplama/.test(hit), hit.trim())
}
check('bot A\'nın yayınına çiziyor', !!(await bot.wait(new RegExp(`çiziyor: ${A}`), 15000)))
const remote = await a.wait(new RegExp(`çizim: ${B} → ${A} çizgi bot-\\d+ \\(1 nokta\\) · kaplamada`), 10000)
check('A botun çizgisini aldı ve kaplamaya verdi', !!remote, remote || '')
check('A botun imlecini aldı (Rust Bot 2)', !!(await a.wait(new RegExp(`çizim: ${B} imleci \\(Rust Bot 2\\)`), 8000)))
const painted = await a.wait(/kaplama: \d+ çizgi, \d+ imleç çiziliyor/, 8000)
check('kaplama çizgi ve imleci çiziyor', !!painted && !/: 0 çizgi/.test(painted), painted || '')
if (overlay) {
  await sleep(900)
  const shot = `${BASE}/out/2-kaplama.png`
  const out = wincap('--title', 'çizim kaplaması', a.proc.pid, shot)
  const green = existsSync(shot) ? countColor(shot, hex('#69db7c'), 45) : { hits: 0 }
  check('kaplama görüntüsünde botun yeşil çizgisi var', green.hits > 50, `${green.hits} piksel (${out.trim()})`)
}
// Bilgisayar sesi: A'nın mikrofonu kapalı; botun ölçtüğü A sesi yayında yükselir, sonra düşer.
const peaks = (from = 0) => bot.lines.slice(from).map(l => new RegExp(`← ${A}: .*tepe (-?[\\d.]+) dBFS`).exec(l)).filter(Boolean).map(m => +m[1])
const loud = await (async () => { const end = Date.now() + 10000; while (Date.now() < end) { const p = peaks(shareFrom).filter(v => v > -40); if (p.length) return p[0]; await sleep(300) } return null })()
check('bot yayın süresince A\'dan bilgisayar sesi alıyor (mikrofon kapalıyken)', loud !== null, `tepe ${loud} dBFS`)
check('yayın bitti', !!(await a.wait(/arama: ekran paylaşımı kapalı/, 25000)))
check('kaplama kapandı ve bilgisayar sesi durdu', !!(await a.wait(/kaplama: kapandı/, 5000)) && !!(await a.wait(/arama: bilgisayar sesi kapalı/, 5000)))
const quietFrom = bot.lines.length
const quiet = await (async () => { const end = Date.now() + 10000; while (Date.now() < end) { const p = bot.lines.slice(quietFrom).map(l => new RegExp(`← ${A}: .*tepe (-?[\\d.]+) dBFS`).exec(l)).filter(Boolean).map(m => +m[1]); if (p.some(v => v < -80)) return Math.min(...p); await sleep(300) } return null })()
check('yayın bitince A sessiz (yalnız kapalı mikrofon)', quiet !== null, `tepe ${quiet} dBFS`)

// ============================================================ İzleyici kalemi
console.log('\n[3] İzleyici: botun yayınına çizim')
const areaLine = await a.wait(/kalem: alan /, 30000)
check('kalem açıldı ve çizim alanı ölçüldü', !!areaLine, areaLine || '')
if (areaLine) {
  const [, x, y, w, h, s] = /alan (-?\d+),(-?\d+),(\d+),(\d+) ölçek ([\d.]+)/.exec(areaLine).map(Number)
  const win = mainWin(a.proc.pid)
  const P = (fx, fy) => [Math.round((x + w * fx) * s), Math.round((y + h * fy) * s)]
  const from = a.lines.length
  // Gerçek fare yolu: kendi penceremize WM_LBUTTONDOWN / WM_MOUSEMOVE / WM_LBUTTONUP.
  wincap('--drag', win.hwnd, a.proc.pid, ...P(0.25, 0.3), ...P(0.75, 0.7), 24)
  check('A çizgiyi gönderdi (call:signal draw)', !!(await a.wait(new RegExp(`çizim: gönderildi → ${B} çizgi ${A.slice(0, 6)}-`), 8000, from)))
  const got = await bot.wait(new RegExp(`çizim alındı: ${A} → ${B} ${A.slice(0, 6)}-\\S+ \\(\\d+ nokta`), 8000)
  check('bot çizgiyi aldı (hedef = botun yayını)', !!got, got || '')
  const cur = await bot.wait(new RegExp(`imleç alındı: ${A} `), 8000)
  check('bot A\'nın kalem imlecini aldı (ad + konum)', !!cur, cur || '')
  await sleep(250)
  const shot = `${BASE}/out/1-A-kalem.png`
  wincap('--shot', win.hwnd, a.proc.pid, shot)
  // Kendi çizgim kişiye özgü renkte (drawBus.ts penColor) kutucukta görünür.
  const own = a.lines.map(l => /çizim: gönderildi/.test(l)).some(Boolean)
  const seen = existsSync(shot) ? countColor(shot, hex(penColor(owner.user.id)), 40) : { hits: 0 }
  check('A\'nın kutucuğunda kendi çizgisi görünüyor (ekran görüntüsü)', own && seen.hits > 40, `${seen.hits} piksel ${penColor(owner.user.id)}`)
}

// ============================================================ Arama balonu
console.log('\n[4] Arama balonu')
const main = mainWin(a.proc.pid)
const btn = id => { const m = [...a.lines].reverse().map(l => new RegExp(`balon: düğme ${id} (-?\\d+),(-?\\d+),(\\d+),(\\d+) ölçek ([\\d.]+)`).exec(l)).find(Boolean); if (!m) return null; const [x, y, w, h, s] = m.slice(1).map(Number); return [Math.round((x + w / 2) * s), Math.round((y + h / 2) * s)] }
const clickBtn = async (id, expect, label) => {
  const at = btn(id)
  const from = a.lines.length
  if (!at) { check(label, false, `${id} konumu yok`); return }
  wincap('--click', 'arama balonu', a.proc.pid, ...at)
  check(label, !!(await a.wait(expect, 6000, from)))
}
let from = a.lines.length
wincap('--minimize', main.hwnd, a.proc.pid)
check('pencere küçültülünce balon belirdi', !!(await a.wait(/balon: göründü/, 6000, from)))
await sleep(1500)
await sleep(1200)
const widget = windows(a.proc.pid).find(w => /arama balonu/.test(w.title))
check('balon 404×126 (ölçekli), en üstte, görev çubuğunda yok', !!widget && (widget.ex & 0x88) === 0x88 && Math.abs(widget.w / widget.h - 404 / 126) < 0.05, widget ? `${widget.w}x${widget.h} ex=0x${widget.ex.toString(16)}` : 'yok')
wincap('--title', 'arama balonu', a.proc.pid, `${BASE}/out/3-balon.png`)
const state = () => { try { return JSON.parse(readFileSync(PROFILE + '/call-widget.json', 'utf8')) } catch { return {} } }
check('konum call-widget.json\'a yazıldı', Number.isFinite(state().x) && Number.isFinite(state().y), JSON.stringify(state()))
await clickBtn('w-mute', /arama: mikrofon açık/, 'balondan mikrofon açıldı')
await clickBtn('w-deafen', /arama: kulaklık kapalı/, 'balondan kulaklık kapatıldı')
await sleep(400)
wincap('--title', 'arama balonu', a.proc.pid, `${BASE}/out/4-balon-kapali.png`)
from = a.lines.length
await clickBtn('w-open', /balon: open/, '"P2\'yi aç" tıklandı')
check('ana pencere geri geldi, balon kapandı', !!(await a.wait(/balon: kapandı/, 5000, from)))
from = a.lines.length
wincap('--minimize', main.hwnd, a.proc.pid)
check('yeniden küçültünce balon yine belirdi', !!(await a.wait(/balon: göründü/, 6000, from)))
await sleep(1500)
await clickBtn('w-close', /balon: close/, '× kapatma sorusunu açtı')
await sleep(400)
wincap('--title', 'arama balonu', a.proc.pid, `${BASE}/out/5-balon-soru.png`)
await clickBtn('w-cancel', /balon: kapatma seçimi cancel/, '"Vazgeç" — balon kalır')
await sleep(300)
await clickBtn('w-close', /balon: close/, '× yeniden')
await sleep(300)
from = a.lines.length
await clickBtn('w-never', /balon: kapatma seçimi never/, '"Bir daha gösterme"')
check('balon kapandı ve tercih kalıcı (hidden:true)', !!(await a.wait(/balon: kapandı/, 4000, from)) && state().hidden === true, JSON.stringify(state()))
from = a.lines.length
await sleep(1500)
check('pencere hâlâ küçükken balon geri gelmiyor', !a.lines.slice(from).some(l => /balon: göründü/.test(l)))
// Ayarlar > Uygulama'daki anahtarın yaptığı gibi tercihi geri aç (kendi test profilimiz).
writeFileSync(PROFILE + '/call-widget.json', JSON.stringify({ ...state(), hidden: false }))
check('tercih geri açılınca balon döndü', !!(await a.wait(/balon: göründü/, 4000, from)))
await sleep(800)
from = a.lines.length
await clickBtn('w-leave', /arama: ayrıldın/, 'balondan aramadan ayrılındı')
check('arama bitince balon kapandı', !!(await a.wait(/balon: kapandı/, 4000, from)))

await a.kill(); await bot.kill()
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
