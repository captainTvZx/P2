// Dilim 8b uçtan uca testi: proje ve kanal yönetimi, kanal izinleri, erişim. Rust istemcisi
// (p2test, kuruluş sahibi) otomatik pilotla kategori ve kanal (metin + ses) oluşturur, varsayılan
// kanal yapar, kanal izinlerinde "Herkes" için mesaj göndermeyi engeller (rustbot gerçekten
// yazamaz), kanalları ve kategoriyi siler, proje oluşturup kütüphaneye açar ve siler; ayrı
// koşuda yönetim panelinin Erişim sekmesinden rustbot'un bir kanalı görmesini kapatıp açar.
// Önkoşul: test PostgreSQL (:55432) + P2 sunucusu (P2_API, varsayılan NestJS :4110).
//   node rust/tools/slice8b-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'

const API = process.env.P2_API || 'http://127.0.0.1:4110'
const BIN = 'C:/p2-rust-target/release/'
const BASE = 'C:/p2-pg/slice8b'
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
const proot = `/organizations/${org.id}/projects/${project.id}`
const channels = async () => (await call('GET', `${proot}/channels`, null, owner.accessToken)).body
// Önceki koşudan kalanlar
for (const c of (await channels()).filter(c => c.name === 'Rust Kategori')) await call('DELETE', `${proot}/channel-categories/${c.id}`, null, owner.accessToken)
for (const p of (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body.filter(p => p.name === 'Rust Test Projesi')) await call('DELETE', `/organizations/${org.id}/projects/${p.id}`, null, owner.accessToken)
for (const p of (await call('GET', `/organizations/${org.id}/admin/projects/trash`, null, owner.accessToken)).body.filter(p => p.name === 'Rust Test Projesi')) await call('DELETE', `/organizations/${org.id}/admin/projects/${p.id}/purge`, null, owner.accessToken)
const defaultBefore = project.defaultChannelId
console.log(`hazırlık: "${project.name}" (varsayılan kanal ${defaultBefore?.slice(0, 8)})`)

let lines = [], client
function start(env) {
  lines = []
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: BASE + '/profile', P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/yönet|kanal izinleri|yetkiler/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
}
const wait = async (re, ms = 20000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/${name}.png`]) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)) } }

// ============================================================ 1) Kategori, kanal, varsayılan, izin
console.log('\n[1] Kategori ve kanallar')
start({ P2_AUTOPILOT_MANAGE: [
  'category:Rust Kategori', 'channel:Rust Kategori:rust-yonet:TEXT', 'channel:Rust Kategori:rust-ses:VOICE', 'default:rust-yonet',
  'perms:rust-yonet', 'perm:ROLE:Herkes:message.send:deny', 'wait:3000', 'perms-close',
].join(';') })
check('etkin yetkiler yüklendi', !!(await wait(/yetkiler: \d+ yetki \(sahip\)/)))
check('kategori oluşturuldu', !!(await wait(/yönet: Rust Kategori kategorisi oluşturuldu/)))
check('metin kanalı oluşturuldu', !!(await wait(/yönet: #rust-yonet kanalı oluşturuldu/)))
check('ses kanalı oluşturuldu', !!(await wait(/yönet: #rust-ses kanalı oluşturuldu/)))
check('varsayılan kanal yapıldı', !!(await wait(/yönet: #rust-yonet varsayılan kanal yapıldı/)))
const cat = (await channels()).find(c => c.name === 'Rust Kategori')
const text = cat?.channels.find(c => c.name === 'rust-yonet'), voice = cat?.channels.find(c => c.name === 'rust-ses')
check('kanallar sunucuda (tür doğru)', text?.type === 'TEXT' && voice?.type === 'VOICE')
check('varsayılan kanal sunucuda', (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body.find(p => p.id === project.id)?.defaultChannelId === text?.id)
check('kanal izinleri açıldı', !!(await wait(/kanal izinleri: #rust-yonet açıldı/)))
check('Herkes için mesaj engellendi', !!(await wait(/kanal izinleri: ROLE message\.send → deny/)))
await sleep(500)
shot('1-kanal-izinleri')
const perms = (await call('GET', `${proot}/channels/${text.id}/permissions`, null, owner.accessToken)).body
const everyone = perms.roles.find(r => r.everyone)
check('kural sunucuda', perms.overrides.some(o => o.targetType === 'ROLE' && o.targetId === everyone.id && o.deny.includes('message.send')))
const botSend = await call('POST', `${proot}/channels/${text.id}/messages`, { content: 'yazabilir miyim?' }, bot.accessToken)
check('rustbot artık bu kanala yazamıyor', botSend.status === 403 || botSend.status === 404, `HTTP ${botSend.status}`)
check('izin penceresi kapandı', !!(await wait(/kanal izinleri: kapandı/)))
client.kill(); await sleep(600)

// ============================================================ 2) Erişim (yönetim paneli)
console.log('\n[2] Erişim')
start({ P2_AUTOPILOT_ADMIN: ['wait:600', 'open:access', 'access-user:rustbot', 'access-channel:rust-yonet:off', 'wait:2000', 'access-channel:rust-yonet:on', 'wait:600', 'close'].join(';') })
check('erişim listesi yüklendi', !!(await wait(/yönetim: erişim \d+ kişi, \d+ proje/)))
check('kanal rustbot için gizlendi', !!(await wait(/yönetim: Rust Bot için kanal gizli\./)))
const hidden = (await call('GET', `${proot}/channels`, null, bot.accessToken)).body.flatMap(c => c.channels)
check('rustbot kanalı görmüyor', !hidden.some(c => c.id === text.id))
await sleep(300)
shot('2-erisim')
check('kanal rustbot için yeniden görünür', !!(await wait(/yönetim: Rust Bot için kanal görünür\./)))
const shown = (await call('GET', `${proot}/channels`, null, bot.accessToken)).body.flatMap(c => c.channels)
check('rustbot kanalı yeniden görüyor', shown.some(c => c.id === text.id))
await wait(/yönetim: kapandı/, 8000)
client.kill(); await sleep(600)

// ============================================================ 3) Silme ve proje
console.log('\n[3] Silme, proje oluştur/sil')
await call('PATCH', `/organizations/${org.id}/projects/${project.id}`, { defaultChannelId: defaultBefore }, owner.accessToken)
start({ P2_AUTOPILOT_MANAGE: ['delete-channel:rust-ses', 'delete-channel:rust-yonet', 'delete-category:Rust Kategori', 'project:Rust Test Projesi', 'wait:1500', 'library:on', 'wait:1000', 'delete-project'].join(';') })
check('ses kanalı silindi', !!(await wait(/yönet: #rust-ses silindi/)))
check('metin kanalı silindi', !!(await wait(/yönet: #rust-yonet silindi/)))
check('kategori silindi', !!(await wait(/yönet: Rust Kategori kategorisi silindi/)))
check('kategori sunucudan kalktı', !(await channels()).some(c => c.name === 'Rust Kategori'))
check('proje oluşturuldu', !!(await wait(/yönet: proje oluşturuldu Rust Test Projesi/)))
const created = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body.find(p => p.name === 'Rust Test Projesi')
check('proje sunucuda (kod)', created?.slug === 'rust-test-projesi', created?.slug || 'yok')
check('proje ayarları kaydedildi', !!(await wait(/yönet: Proje ayarları kaydedildi/)))
check('kütüphanede uygulama açıldı', (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body.find(p => p.name === 'Rust Test Projesi')?.libraryEnabled === true)
shot('3-proje')
check('proje silindi', !!(await wait(/yönet: proje silindi/)))
const trash = (await call('GET', `/organizations/${org.id}/admin/projects/trash`, null, owner.accessToken)).body
check('proje geri dönüşümde', trash.some(p => p.name === 'Rust Test Projesi'))
for (const p of trash.filter(p => p.name === 'Rust Test Projesi')) await call('DELETE', `/organizations/${org.id}/admin/projects/${p.id}/purge`, null, owner.accessToken)
client.kill()

console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
