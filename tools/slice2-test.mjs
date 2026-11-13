// Dilim 2 uçtan uca testi: Rust istemcisi (p2test, otomatik pilot) #genel kanalında;
// karşı taraf (rustbot) REST ile yazar/düzenler/tepki verir/siler — istemci gerçek zamanlı almalı.
// Masaüstüne giriş göndermez; görüntü yalnız istemci penceresinden (p2-wincap) alınır.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice2-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import { testServer, Client } from './lib.mjs'

const { api: API } = await testServer()
const OUT = 'C:\\p2-pg\\slice2'
const PROFILE = 'C:\\p2-pg\\rust-client-profile-s2'
mkdirSync(OUT, { recursive: true })
rmSync(PROFILE, { recursive: true, force: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// --- hazırlık: rustbot proje üyesi olsun
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, bot.accessToken)).body[0]
const db = new Client({ connectionString: 'postgresql://p2@127.0.0.1:55432/p2_spike' })
await db.connect()
const project = (await db.query(`select id, name from "Project" where "organizationId" = $1 and "deletedAt" is null order by "createdAt" limit 1`, [org.id])).rows[0]
await db.query(`insert into "ProjectMember" ("projectId", "userId") values ($1, $2) on conflict do nothing`, [project.id, bot.user.id])
await db.end()
const categories = (await call('GET', `/organizations/${org.id}/projects/${project.id}/channels`, null, bot.accessToken)).body
const genel = categories.flatMap(c => c.channels).find(c => c.name === 'genel')
const base = `/organizations/${org.id}/projects/${project.id}/channels/${genel.id}`
console.log(`hazırlık: kuruluş "${org.name}", proje "${project.name}", kanal #genel`)

// --- Rust istemcisi
const say = 'Rust istemcisinden selam 👋 **kalın** ve `kod`'
const lines = []
const client = spawn('C:\\p2-rust-target\\release\\p2-client.exe', [], {
  env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_CHANNEL: 'genel', P2_AUTOPILOT_SAY: say },
})
client.stderr.on('data', d => { if (process.env.P2_TRACE_SOCKET) process.stdout.write('    [stderr] ' + d) })
client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); console.log('    [istemci] ' + l) } })
const waitLine = async (re, ms = 8000) => { const end = Date.now() + ms; while (Date.now() < end) { if (lines.some(l => re.test(l))) return true; await sleep(100) } return false }
const shot = name => { try { execFileSync('C:\\p2-rust-target\\release\\p2-wincap.exe', [String(client.pid), `${OUT}\\${name}.png`]); console.log(`    görüntü: ${OUT}\\${name}.png`) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim()) } }

check('giriş ve çalışma alanı', await waitLine(/ekran: çalışma alanı/))
check('#genel mesajları yüklendi', await waitLine(/sohbet: #genel \d+ mesaj yüklendi/))
check('gerçek zamanlı bağlandı', await waitLine(/olay: gerçek zamanlı bağlandı/))
await sleep(1500)

// İstemcinin gönderdiği mesaj sunucuda mı?
const recent = (await call('GET', base + '/messages', null, bot.accessToken)).body
check('istemcinin mesajı sunucuda (p2test)', recent.some(m => m.content === say && m.author.username === 'p2test'))

// Karşı taraf yazar
const text = `Bot: merhaba Rust (${new Date().toLocaleTimeString('tr-TR')})`
const sent = (await call('POST', base + '/messages', { content: text }, bot.accessToken)).body
check('karşı tarafın mesajı gerçek zamanlı geldi', await waitLine(new RegExp('olay: mesaj geldi «' + text.replace(/[()]/g, '\\$&') + '»')))
await call('PATCH', `${base}/messages/${sent.id}`, { content: text + ' (düzenlendi)' }, bot.accessToken)
check('düzenleme gerçek zamanlı geldi', await waitLine(/olay: mesaj düzenlendi «Bot: merhaba Rust .*\(düzenlendi\)»/))
await call('POST', `${base}/messages/${sent.id}/reactions`, { emoji: '👍' }, bot.accessToken)
await call('POST', `${base}/messages/${sent.id}/reactions`, { emoji: '🎉' }, bot.accessToken)
check('tepkiler gerçek zamanlı geldi', await waitLine(/olay: tepkiler .*🎉/))
await sleep(800)
shot('1-sohbet')

await call('DELETE', `${base}/messages/${sent.id}`, null, bot.accessToken)
check('silme gerçek zamanlı geldi', await waitLine(new RegExp('olay: mesaj silindi ' + sent.id)))
await sleep(800)
shot('2-silindikten-sonra')

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
client.kill()
process.exit(fail ? 1 : 0)
