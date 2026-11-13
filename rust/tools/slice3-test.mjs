// Dilim 3 uçtan uca testi: DM'ler, gruplar, profil, ayarlar, durum, tema.
// Rust istemcisi p2test olarak otomatik pilotla çalışır; karşı taraf (rustbot) REST ve
// socket.io ile yazar, düzenler, tepki verir, siler, "yazıyor" gönderir, grup kurar.
// Masaüstüne giriş göndermez; görüntü yalnız istemci penceresinden (p2-wincap).
// Önkoşul: test PostgreSQL (:55432) + P2 sunucusu (P2_API, varsayılan NestJS :4110).
//   node rust/tools/slice3-test.mjs <P2 repo yolu>
import { spawn, execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { mkdirSync, rmSync, readFileSync } from 'node:fs'

const repo = process.argv[2]
const require = createRequire(repo + '/package.json')
const { Client } = require('pg')
const { io } = require('socket.io-client')
const API = process.env.P2_API || 'http://127.0.0.1:4110'
const OUT = 'C:\\p2-pg\\slice3'
const PROFILE = 'C:\\p2-pg\\rust-client-profile-s3'
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

// --- hazırlık
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const me = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, bot.accessToken)).body[0]
const db = new Client({ connectionString: 'postgresql://p2@127.0.0.1:55432/p2_spike' })
await db.connect()
const project = (await db.query(`select id, name from "Project" where "organizationId" = $1 and "deletedAt" is null order by "createdAt" limit 1`, [org.id])).rows[0]
for (const u of [bot.user.id, me.user.id]) await db.query(`insert into "ProjectMember" ("projectId", "userId") values ($1, $2) on conflict do nothing`, [project.id, u])
// Önceki koşulardan kalan test grupları (aynı ad tekrar oluşmasın diye) temizlenir.
await db.query(`delete from "ChatGroup" where "organizationId" = $1 and name in ('Rust Grubu', 'Bot Grubu')`, [org.id])
await db.end()
// Ad ve durum başlangıç değerlerine
await call('PATCH', '/auth/profile', { displayName: 'P2 Test', title: '' }, me.accessToken)
console.log(`hazırlık: kuruluş "${org.name}", proje "${project.name}"`)

// Botun soketi: "yazıyor" göndermek için.
const botSocket = io(API + '/stream', { auth: { token: bot.accessToken }, transports: ['websocket'] })
await new Promise(r => botSocket.on('connect', r))
botSocket.emit('subscribe', { organizationId: org.id })

// --- istemci çalıştırıcı
let client, lines
function start(env) {
  lines = []
  client = spawn('C:\\p2-rust-target\\release\\p2-client.exe', [], {
    env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', ...env },
  })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); console.log('    [istemci] ' + l) } })
}
const stop = async () => { client.kill(); await sleep(600) }
const waitLine = async (re, ms = 8000) => { const end = Date.now() + ms; while (Date.now() < end) { if (lines.some(l => re.test(l))) return true; await sleep(100) } return false }
const shot = name => { try { execFileSync('C:\\p2-rust-target\\release\\p2-wincap.exe', [String(client.pid), `${OUT}\\${name}.png`]); console.log(`    görüntü: ${OUT}\\${name}.png`) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim()) } }

// ============================================================ 1) DM
console.log('\n[1] Birebir sohbet')
const dmSay = 'Rust DM: selam bot 👋'
start({ P2_AUTOPILOT_DM: 'rustbot', P2_AUTOPILOT_SAY: dmSay })
check('DM açıldı', await waitLine(/dm: @rustbot açıldı/))
check('DM mesajları yüklendi', await waitLine(/sohbet: @rustbot \d+ mesaj yüklendi/))
check('gerçek zamanlı bağlandı', await waitLine(/olay: gerçek zamanlı bağlandı/))
await sleep(1200)
const conv = (await call('POST', `/organizations/${org.id}/direct`, { userId: me.user.id }, bot.accessToken)).body
const dmBase = `/organizations/${org.id}/direct/${conv.id}`
const dmRows = (await call('GET', dmBase + '/messages', null, bot.accessToken)).body
check('istemcinin DM\'i sunucuda', dmRows.some(m => m.content === dmSay && m.author.username === 'p2test'))
botSocket.emit('typing', { conversationId: conv.id, typing: true })
check('"yazıyor" gerçek zamanlı geldi', await waitLine(/olay: yazıyor Rust Bot true \[dm\]/))
await sleep(400)
shot('1-dm-yaziyor')
const dmText = `Bot DM (${new Date().toLocaleTimeString('tr-TR')})`
const dm = (await call('POST', dmBase + '/messages', { content: dmText }, bot.accessToken)).body
check('karşı tarafın DM\'i geldi', await waitLine(new RegExp('olay: mesaj geldi «' + esc(dmText) + '» \\(rustbot\\) \\[dm\\]')))
await call('PATCH', `${dmBase}/messages/${dm.id}`, { content: dmText + ' ✎' }, bot.accessToken)
check('DM düzenlemesi geldi', await waitLine(/olay: mesaj düzenlendi «Bot DM .* ✎» \[dm\]/))
await call('POST', `${dmBase}/messages/${dm.id}/reactions`, { emoji: '🎉' }, bot.accessToken)
check('DM tepkisi geldi', await waitLine(/olay: tepkiler .*🎉.* \[dm\]/))
await sleep(600)
shot('2-dm')
await call('DELETE', `${dmBase}/messages/${dm.id}`, null, bot.accessToken)
check('DM silmesi geldi', await waitLine(new RegExp('olay: mesaj silindi ' + dm.id + ' \\[dm\\]')))
// Bot bir grup kurar ve yazar: istemci başka sohbette, okunmamış rozeti artmalı.
const botGroup = (await call('POST', `/organizations/${org.id}/groups`, { projectId: project.id, name: 'Bot Grubu', userIds: [me.user.id] }, bot.accessToken)).body
check('botun kurduğu grup gerçek zamanlı geldi', await waitLine(/olay: grup «Bot Grubu» \(2 üye\)/))
await call('POST', `/organizations/${org.id}/groups/${botGroup.id}/messages`, { content: 'Gruba hoş geldin' }, bot.accessToken)
check('grup okunmamış rozeti', await waitLine(/okunmamış: grup «Bot Grubu» 1/))
await sleep(600)
shot('3-grup-rozeti')
await stop()

// ============================================================ 2) Grup oluşturma
console.log('\n[2] Grup')
const groupSay = 'Rust grubundan ilk mesaj'
start({ P2_AUTOPILOT_GROUP: 'Rust Grubu|rustbot', P2_AUTOPILOT_SAY: groupSay })
check('grup oluşturuldu', await waitLine(/grup: oluşturuldu «Rust Grubu» \(2 üye\)/))
check('grup sohbeti yüklendi', await waitLine(/sohbet: grup:Rust Grubu \d+ mesaj yüklendi/))
await waitLine(/olay: gerçek zamanlı bağlandı/)
await sleep(1200)
const groups = (await call('GET', `/organizations/${org.id}/groups?projectId=${project.id}`, null, bot.accessToken)).body
const rg = groups.find(g => g.name === 'Rust Grubu')
check('grup sunucuda, bot üye', !!rg && rg.members.some(m => m.username === 'rustbot'))
const gmsgs = rg ? (await call('GET', `/organizations/${org.id}/groups/${rg.id}/messages`, null, bot.accessToken)).body : []
check('istemcinin grup mesajı sunucuda', gmsgs.some(m => m.content === groupSay))
const gText = 'Bot gruba yazdı'
if (rg) await call('POST', `/organizations/${org.id}/groups/${rg.id}/messages`, { content: gText }, bot.accessToken)
check('grup mesajı gerçek zamanlı geldi', await waitLine(new RegExp('olay: mesaj geldi «' + gText + '» \\(rustbot\\) \\[grup\\]')))
await sleep(600)
shot('4-grup')
if (rg) await call('POST', `/organizations/${org.id}/groups/${rg.id}/leave`, null, bot.accessToken)
check('bot ayrılınca grup güncellendi', await waitLine(/olay: grup «Rust Grubu» \(1 üye\)/))
await stop()

// ============================================================ 3) Profil, durum, ayarlar
console.log('\n[3] Profil ve ayarlar')
start({ P2_AUTOPILOT_RENAME: 'P2 Test Rust', P2_AUTOPILOT_PRESENCE: 'BUSY' })
check('profil adı kaydedildi', await waitLine(/ayarlar: profil kaydedildi \(P2 Test Rust\)/))
check('durum tercihi gönderildi', await waitLine(/durum: tercih BUSY/))
check('sunucu durumu onayladı', await waitLine(/olay: kendi durumum BUSY\/BUSY/))
const meNow = (await call('GET', '/auth/me', null, me.accessToken)).body
check('sunucuda yeni ad', meNow.displayName === 'P2 Test Rust', meNow.displayName)
await sleep(800)
shot('5-ayarlar-hesap')
await stop()

console.log('\n[4] Profil paneli ve kullanıcı menüsü')
start({ P2_AUTOPILOT_PROFILE: 'rustbot', P2_AUTOPILOT_USER_MENU: '1', P2_AUTOPILOT_PRESENCE: 'ONLINE' })
check('profil paneli açıldı', await waitLine(/profil: @rustbot/))
check('durum geri alındı', await waitLine(/olay: kendi durumum ONLINE\/ONLINE/))
await sleep(1000)
shot('6-profil-ve-menu')
await stop()

console.log('\n[5] Açık tema')
start({ P2_AUTOPILOT_THEME: 'light', P2_AUTOPILOT_CHANNEL: 'genel' })
check('tema değişti', await waitLine(/ayarlar: tema light/))
await sleep(1000)
shot('7-acik-tema')
await stop()
const saved = JSON.parse(readFileSync(PROFILE + '\\settings.json', 'utf8').replace(/^\uFEFF/, ''))
check('tema ayarı kaydedildi', saved.theme === 'light')
start({ P2_AUTOPILOT_CHANNEL: 'genel' })
await waitLine(/sohbet: #genel/)
await sleep(1000)
shot('8-acik-tema-yeniden-acilis')
await stop()

// Temizlik: ad geri
await call('PATCH', '/auth/profile', { displayName: 'P2 Test', title: '' }, me.accessToken)
botSocket.close()
console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
