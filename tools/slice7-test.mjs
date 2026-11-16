// Dilim 7 uçtan uca testi: oyun içi overlay. Rust istemcisi kütüphaneden bir "oyun" (p2-d3dtest,
// gerçek D3D11 takas zinciri) kurar ve oynatır; oyun DLL enjeksiyonuyla başlar, overlay host'u
// (p2-overlay, ekran dışı GPUI) panel, görev/not araçları, performans, HUD, bildirim ve ekran
// görüntüsünü oyunun içinde çizer. Panel girdisi yalnız OYUN PENCERESİNE PostMessage ile gider
// (masaüstüne sentetik girdi yok).
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice7-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, readFileSync, existsSync, copyFileSync, readdirSync } from 'node:fs'
import { testServer, TARGET } from './lib.mjs'

const { api: API } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice7-e2e'
const PROFILE = BASE + '/profile'
const GAMES = BASE + '/games'
const SHOTS = BASE + '/shots'
const OUT = BASE + '/out'
rmSync(BASE, { recursive: true, force: true })
for (const d of [GAMES, SHOTS, OUT, BASE + '/build']) mkdirSync(d, { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// "Oyun": gerçek D3D11 test uygulaması (varsayılan d3d11)
copyFileSync(BIN + 'p2-d3dtest.exe', BASE + '/build/Game.exe')

const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const root = `/organizations/${org.id}/projects/${project.id}`
const board = (await call('GET', `${root}/boards`, null, owner.accessToken)).body.sort((a, b) => a.position - b.position)[0]
// Önceki denemelerden kalanlar
for (const t of (await call('GET', `${root}/boards/${board.id}`, null, owner.accessToken)).body.tasks.filter(t => /^Overlay/.test(t.title))) await call('DELETE', `${root}/boards/${board.id}/tasks/${t.id}`, null, owner.accessToken)
for (const n of (await call('GET', `${root}/notes`, null, owner.accessToken)).body.filter(n => /^Overlay/.test(n.title))) await call('DELETE', `${root}/notes/${n.id}`, null, owner.accessToken)
const openTasks = (await call('GET', `${root}/boards/${board.id}`, null, owner.accessToken)).body.tasks
console.log(`hazırlık: proje "${project.name}", pano "${board.name}" (${openTasks.length} görev)`)

const stamp = Date.now().toString().slice(-6)
const taskTitle = `Overlay görevi ${stamp}`
const noteTitle = `Overlay notu ${stamp}`
const channel = 'rusttest'
const version = `7.0.${stamp}`

let lines = [], client
function start(env) {
  lines = []
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_PAGE: 'library', P2_LIBRARY_ROOT: GAMES, P2_SHOT_DIR: SHOTS, ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (process.env.P2_SLICE_VERBOSE === '1' || /overlay|kütüphane: (iş|oyun|yükleme tamam)/.test(l)) console.log('    [istemci] ' + l.slice(0, 220)) } })
}
const stop = async () => { client.kill(); await sleep(700) }
const waitLine = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(150) } return null }
const capture = (pid, name) => { try { execFileSync(BIN + 'p2-wincap.exe', [String(pid), `${OUT}/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 200)); return false } }

// ============================================================ 1) Yükle, yayınla, kur
console.log('\n[1] Oyunu yükle ve kur')
start({ P2_AUTOPILOT_UPLOAD: `${BASE}/build|Game.exe|${channel}|${version}` })
check('overlay ikilileri bulundu', !!(await waitLine(/overlay: hazır \(enjektör/, 15000)))
check('yükleme tamamlandı', !!(await waitLine(/kütüphane: yükleme tamam/, 60000)))
await stop()
const rel = (await call('GET', `${root}/releases`, null, owner.accessToken)).body.releases.find(r => r.version === version && r.channel === channel)
await call('POST', `${root}/releases/${rel.id}/publish`, {}, owner.accessToken)
await call('POST', `${root}/releases/${rel.id}/live`, {}, owner.accessToken)
start({ P2_AUTOPILOT_LIBRARY: 'install' })
check('kurulum bitti', !!(await waitLine(/kütüphane: iş done/, 60000)))
await stop()

// ============================================================ 2) Overlay ile oyna
console.log('\n[2] Overlay ile oyna')
// Tıklama hedefleri çizimde ölçülen öğeler (ui.rs `hit`): araç çubuğu `tool:<araç>`, "Yeni görev"
// `hit:tasks:new`, satırın durum listesi `hit:task-status:<başlık>` + seçenek `pick:<etiket>`.
const steps = [
  'open', 'tool:tasks', 'wait:2500', 'hit:tasks:new', 'wait:800', `type:${taskTitle}`, 'key:enter', 'wait:3000', 'state', `hit:task-status:${taskTitle}`, 'wait:800', 'pick:Sürüyor', 'wait:3000',
  'tool:notes', 'wait:2500', 'hit:notes:new', 'wait:2500', `type:${noteTitle}`, 'wait:3000', 'tool:perf', 'wait:2500', 'hit:win:perf:max', 'wait:1200', 'hit:perf:hud', 'wait:800', 'close', 'wait:1500', 'shot', 'wait:2500', 'state',
].join(';')
start({ P2_AUTOPILOT_LIBRARY: 'play', P2_OVERLAY_AUTOPILOT: steps })
check('oyun başlatıldı', !!(await waitLine(/kütüphane: oyun başlatıldı/, 30000)))
const injected = await waitLine(/overlay\[(\d+)\]: .*kök süreç \d+ enjekte edildi/, 20000)
check('DLL enjekte edildi', !!injected, injected || '')
const gamePid = Number(/kök süreç (\d+)/.exec(injected || '')?.[1] || 0)
check('overlay hazır (oyun kare çiziyor)', !!(await waitLine(/overlay: oyun hazır/, 30000)))
check('başlangıç bildirimi', !!(await waitLine(/overlay\[\d+\]: hazır: \d+x\d+/, 5000)))
await sleep(800)
capture(gamePid, '1-bildirim')
// Oyundayken DM gelir → köşe bildirimi (bağlantıdan önce gönderilen mesaj gerçek zamanlı gelmez: önce bağlantıyı bekle)
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
const me =(await call('GET', '/auth/me', null, owner.accessToken)).body
const conv = (await call('POST', `/organizations/${org.id}/direct`, { userId: me.user?.id || me.id }, bot.accessToken)).body
const dm = (await call('POST', `/organizations/${org.id}/direct/${conv.id}/messages`, { content: 'Overlay testi: oyundayken mesaj 🎮' }, bot.accessToken)).body
check('DM oyunda bildirim oldu', !!(await waitLine(/overlay\[\d+\]: bildirim: Rust Bot/, 10000)))
check('panel açıldı', !!(await waitLine(/overlay: panel açık/, 10000)))
check('görevler yüklendi', !!(await waitLine(/overlay\[\d+\]: görevler: /, 10000)))
check('görev panelden yazılıp eklendi', !!(await waitLine(new RegExp('overlay\\[\\d+\\]: Görev eklendi: #\\d+ ' + taskTitle), 15000)))
await sleep(600)
capture(gamePid, '2-gorevler')
check('görev durumu panelden değişti', !!(await waitLine(/overlay\[\d+\]: görev durumu: TODO → IN_PROGRESS/, 15000)))
check('not panelden eklendi', !!(await waitLine(new RegExp('overlay\\[\\d+\\]: Not kaydedildi: ' + noteTitle), 20000)))
await sleep(500)
capture(gamePid, '3-notlar')
check('performans aracı', !!(await waitLine(/overlay\[\d+\]: araç: Perf/, 10000)))
await sleep(1500)
capture(gamePid, '4-performans')
check('HUD sabitlendi', !!(await waitLine(/overlay\[\d+\]: HUD sabitlendi/, 10000)))
check('panel kapandı', !!(await waitLine(/overlay: panel kapalı/, 10000)))
const shotLine = await waitLine(/overlay: ekran görüntüsü (.+\.png) \((\d+)x(\d+)\)/, 15000)
check('ekran görüntüsü (DLL arka tamponu)', !!shotLine, shotLine || '')
const shotPath = /ekran görüntüsü (.+\.png)/.exec(shotLine || '')?.[1]
check('ekran görüntüsü dosyası', !!shotPath && existsSync(shotPath) && readFileSync(shotPath).length > 1000)
const finalState = await waitLine(/overlay\[\d+\]: durum: görünür=false .*hud=true .*maske=[45]/, 15000)
check('kapalı panelde HUD + bildirim maskesi', !!finalState, finalState?.replace(/.*durum: /, '') || '')
capture(gamePid, '5-hud')

// Sunucu tarafı: panelden yapılanlar gerçekten kaydedildi mi
const tasks = (await call('GET', `${root}/boards/${board.id}`, null, owner.accessToken)).body.tasks
const created = tasks.find(t => t.title === taskTitle)
check('görev sunucuda (Sürüyor)', created?.status === 'IN_PROGRESS', created ? `#${created.number} ${created.status}` : 'yok')
const notes = (await call('GET', `${root}/notes`, null, owner.accessToken)).body
check('not sunucuda', notes.some(n => n.title === noteTitle))

// ============================================================ 3) Oyunu kapat
console.log('\n[3] Oyunu kapat')
try { execFileSync('taskkill', ['/PID', String(gamePid), '/F']) } catch {}
check('istemci oyunun kapandığını gördü (oynama süresi)', !!(await waitLine(/kütüphane: oyun kapandı/, 15000)))
check('overlay host durdu', !!(await waitLine(/overlay\[\d+\]: (oyun kapandı|overlay host kapandı)/, 10000)))
await stop()

// Temizlik
if (created) await call('DELETE', `${root}/boards/${board.id}/tasks/${created.id}`, null, owner.accessToken)
for (const n of notes.filter(n => n.title === noteTitle)) await call('DELETE', `${root}/notes/${n.id}`, null, owner.accessToken)
await call('DELETE', `/organizations/${org.id}/direct/${conv.id}/messages/${dm.id}`, null, bot.accessToken)
console.log(`\ngörüntüler: ${OUT} (${readdirSync(OUT).join(', ')})`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
