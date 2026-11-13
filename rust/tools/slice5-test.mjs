// Dilim 5 uçtan uca testi: görev panosu. İstemci (p2test, otomatik pilot) panoyu açar, görev
// oluşturur, kartı taşır (sürükle-bırakla aynı kod yolu), görev ayrıntısını ve yorumları açar;
// karşı taraf (rustbot) REST ile görev açar/taşır, yorum yazar, sprint açar — istemci gerçek
// zamanlı görmeli. Masaüstüne giriş göndermez; görüntü yalnız istemci penceresinden.
// Önkoşul: test PostgreSQL (:55432) + P2 sunucusu (P2_API, varsayılan NestJS :4110).
//   node rust/tools/slice5-test.mjs <P2 repo yolu>
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'

const API = process.env.P2_API || 'http://127.0.0.1:4110'
const OUT = 'C:\\p2-pg\\slice5'
const PROFILE = 'C:\\p2-pg\\rust-client-profile-s5'
mkdirSync(OUT, { recursive: true })
rmSync(PROFILE, { recursive: true, force: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, bot.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, bot.accessToken)).body[0]
const boards = (await call('GET', `/organizations/${org.id}/projects/${project.id}/boards`, null, bot.accessToken)).body
const board = boards[0]
const base = `/organizations/${org.id}/projects/${project.id}/boards/${board.id}`
// Başkasının görevini silmek ve sprint açmak "Panoyu yönet" ister: botta yok, sahibi (p2test) yapar.
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
// Önceki koşulardan kalan test görevleri ve sprint'leri temizlenir.
const before = (await call('GET', base, null, bot.accessToken)).body
for (const t of before.tasks.filter(t => /^(Rust görevi|Bot görevi)/.test(t.title))) await call('DELETE', `${base}/tasks/${t.id}`, null, owner.accessToken)
for (const s of before.sprints.filter(s => s.name === 'Sprint R')) await call('DELETE', `${base}/sprints/${s.id}`, null, owner.accessToken)
console.log(`hazırlık: proje "${project.name}", pano "${board.name}" (${boards.length} pano)`)

let lines = [], client
function start(env) {
  lines = []
  client = spawn('C:\\p2-rust-target\\release\\p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_PAGE: 'board', ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (!/olay: durum|olay: activity/.test(l)) console.log('    [istemci] ' + l) } })
}
const stop = async () => { client.kill(); await sleep(600) }
const waitLine = async (re, ms = 10000) => { const end = Date.now() + ms; while (Date.now() < end) { if (lines.some(l => re.test(l))) return true; await sleep(100) } return false }
const shot = name => { try { execFileSync('C:\\p2-rust-target\\release\\p2-wincap.exe', [String(client.pid), `${OUT}\\${name}.png`]); console.log(`    görüntü: ${OUT}\\${name}.png`) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim()) } }

// ============================================================ 1) Pano + görev oluşturma
console.log('\n[1] Pano ve görev oluşturma')
start({ P2_AUTOPILOT_TASK: 'Rust görevi' })
check('pano açıldı', await waitLine(/sayfa: pano/))
check('pano yüklendi', await waitLine(new RegExp(`pano: ${esc(board.name)} \\d+ görev`)))
check('görev oluşturuldu', await waitLine(/pano: görev kaydedildi #\d+ «Rust görevi»/))
let content = (await call('GET', base, null, bot.accessToken)).body
const mine = content.tasks.find(t => t.title === 'Rust görevi')
check('görev sunucuda', !!mine)
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
await sleep(800)
// Karşı taraf görev açar ve taşır
const theirs = (await call('POST', `${base}/tasks`, { title: 'Bot görevi', priority: 'URGENT', kind: 'BUG', labels: ['ses', 'çökme'], points: 5, status: 'TODO' }, bot.accessToken)).body
check('karşı tarafın görevi gerçek zamanlı geldi', await waitLine(new RegExp(`pano: olay task:updated #${theirs.number} TODO`)))
await call('PATCH', `${base}/tasks/${theirs.id}/move`, { status: 'IN_PROGRESS', order: [theirs.id] }, bot.accessToken)
check('karşı tarafın taşıması gerçek zamanlı geldi', await waitLine(/pano: olay task:moved/))
await call('POST', `${base}/tasks/${mine.id}/comments`, { content: 'Bot yorumu: **güzel** iş' }, bot.accessToken)
await sleep(1200)
shot('1-pano')
await stop()

// ============================================================ 2) Taşıma (sürükle-bırak yolu) + ayrıntı + yorumlar
console.log('\n[2] Kartı taşı, ayrıntı ve yorumlar')
start({ P2_AUTOPILOT_MOVE: 'Rust görevi|DONE', P2_AUTOPILOT_OPEN_TASK: 'Rust görevi' })
check('kart taşındı', await waitLine(new RegExp(`pano: taşındı #${mine.number} → DONE`)))
content = (await call('GET', base, null, bot.accessToken)).body
check('sunucuda Bitti sütununda', content.tasks.find(t => t.id === mine.id)?.status === 'DONE')
check('görev ayrıntısı ve yorumu açıldı', await waitLine(/pano: görev açıldı, 1 yorum/))
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
await sleep(600)
await call('POST', `${base}/tasks/${mine.id}/comments`, { content: 'İkinci yorum (canlı)' }, bot.accessToken)
await sleep(1500)
// Sprint: bot açar ve başlatır → istemci panoyu yeniden yükler
const sprint = (await call('POST', `${base}/sprints`, { name: 'Sprint R', goal: 'Rust geçişi' }, owner.accessToken)).body
check('sprint olayı panoyu yeniledi', await waitLine(/pano: .* \d+ görev, [1-9]\d* sprint/, 8000))
await sleep(800)
shot('2-gorev-ayrintisi')
await stop()

// Temizlik
for (const t of [mine, theirs]) await call('DELETE', `${base}/tasks/${t.id}`, null, owner.accessToken)
if (sprint?.id) await call('DELETE', `${base}/sprints/${sprint.id}`, null, owner.accessToken)

// ============================================================ 3) Dokümanlar: sayfa
console.log('\n[3] Doküman: sayfa')
const droot = `/organizations/${org.id}/projects/${project.id}/documents`
for (const d of ((await call('GET', droot, null, owner.accessToken)).body || []).filter(d => /^Rust (sayfası|tuvali)$/.test(d.title))) await call('DELETE', `${droot}/${d.id}`, null, owner.accessToken)
const page = (await call('POST', droot, { title: 'Rust sayfası', kind: 'PAGE' }, owner.accessToken)).body
await call('PATCH', `${droot}/${page.id}`, { content: { version: 1, pageSize: 'A4', pages: [{ id: 'a', body: '<h1>Başlık</h1><p>Merhaba <b>dünya</b> ve <i>Rust</i>.</p><p style="text-align: center;">Ortada</p>' }] } }, owner.accessToken)
await call('PUT', `${droot}/${page.id}/permissions/${bot.user.id}`, { access: 'WRITE' }, owner.accessToken)
start({ P2_AUTOPILOT_PAGE: 'documents', P2_AUTOPILOT_DOC: 'Rust sayfası', P2_AUTOPILOT_DOC_APPEND: "Rust'tan eklenen satır" })
check('dokümanlar açıldı', await waitLine(/sayfa: dokümanlar/))
check('sayfa açıldı (sahibi)', await waitLine(/doküman: açıldı «Rust sayfası» \(PAGE, OWNER\)/))
check('sayfa kaydedildi', await waitLine(/doküman: kaydedildi «Rust sayfası»/, 8000))
const savedPage = (await call('GET', `${droot}/${page.id}`, null, owner.accessToken)).body
const html = savedPage.content.pages?.[0]?.body || ''
check('eklenen satır sunucuda', html.includes("<p>Rust'tan eklenen satır</p>"), html)
check('Electron biçimi korundu (başlık, kalın, italik, hizalama)', html.includes('<h1>Başlık</h1>') && html.includes('<b>dünya</b>') && html.includes('<i>Rust</i>') && html.includes('text-align: center'), html)
check('sayfa düzeni alanları korundu', savedPage.content.pageSize === 'A4' && savedPage.content.pages[0].id === 'a')
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
await sleep(500)
await call('PATCH', `${droot}/${page.id}`, { title: 'Rust sayfası', content: { ...savedPage.content, pages: [...savedPage.content.pages, { id: 'b', body: '<p>Botun sayfası</p>' }] } }, bot.accessToken)
check('başkasının düzenlemesi gerçek zamanlı geldi', await waitLine(/doküman: olay document:updated/))
await sleep(1000)
shot('3-dokuman-sayfa')
await stop()

// ============================================================ 4) Dokümanlar: tuval
console.log('\n[4] Doküman: tuval')
const board2 = (await call('POST', droot, { title: 'Rust tuvali', kind: 'CANVAS' }, owner.accessToken)).body
await call('PATCH', `${droot}/${board2.id}`, { content: { version: 1, grid: 'DOTS', nodes: [
  { id: 'a', type: 'BOX', x: 60, y: 60, w: 200, h: 110, text: 'Oyun döngüsü', emoji: '🎮', color: '#7ad3ff' },
  { id: 'b', type: 'BOX', x: 380, y: 200, w: 200, h: 110, text: 'Seviye tasarımı' },
  { id: 'c', type: 'SPHERE', x: 140, y: 300, w: 120, h: 120, text: 'Ses', color: '#ff9f6b' },
], connections: [{ id: 'w', from: 'a', fromSide: 'right', to: 'b', toSide: 'left' }] } }, owner.accessToken)
start({ P2_AUTOPILOT_PAGE: 'documents', P2_AUTOPILOT_DOC: 'Rust tuvali', P2_AUTOPILOT_NODE: 'Rust notu' })
check('tuval açıldı', await waitLine(/doküman: açıldı «Rust tuvali» \(CANVAS, OWNER\)/))
check('tuval kaydedildi', await waitLine(/doküman: kaydedildi «Rust tuvali»/, 8000))
const savedCanvas = (await call('GET', `${droot}/${board2.id}`, null, owner.accessToken)).body.content
check('yeni not sunucuda', savedCanvas.nodes.some(n => n.text === 'Rust notu' && n.color === '#ffd166'), JSON.stringify(savedCanvas.nodes.map(n => n.text)))
check('bilinmeyen alanlar korundu (emoji, bağlantı yönleri, ızgara)', savedCanvas.nodes.find(n => n.id === 'a')?.emoji === '🎮' && savedCanvas.connections?.[0]?.fromSide === 'right' && savedCanvas.grid === 'DOTS')
await sleep(1000)
shot('4-dokuman-tuval')
await stop()
for (const d of [page, board2]) await call('DELETE', `${droot}/${d.id}`, null, owner.accessToken)

// ============================================================ 5) Etkinlik
console.log('\n[5] Etkinlik')
start({ P2_AUTOPILOT_PAGE: '', P2_AUTOPILOT_ACTIVITY: '1', P2_AUTOPILOT_CHANNEL: 'genel' })
check('etkinlik yüklendi', await waitLine(/etkinlik: \d+ öğe, \d+ okunmamış/))
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
await sleep(500)
const cats = (await call('GET', `/organizations/${org.id}/projects/${project.id}/channels`, null, bot.accessToken)).body
const genel = cats.flatMap(c => c.channels).find(c => c.name === 'genel')
await call('POST', `/organizations/${org.id}/projects/${project.id}/channels/${genel.id}/messages`, { content: '@p2test pano hazır mı? 👀' }, bot.accessToken)
check('etiketlenme gerçek zamanlı geldi', await waitLine(/etkinlik: yeni MENTION \(rustbot\)/))
await sleep(900)
shot('6-etkinlik')
await stop()

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
