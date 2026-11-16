// Dilim 10 uçtan uca testi: profil fotoğrafı (yükle → avatar her yerde görsel olur), yıldızlı
// kanal/kişi, kanal bildirim tercihi (Tümü → bildirim, Sessiz → bildirim yok), kanal içi arama,
// mesaj düzenleme geçmişi, resim görüntüleyici, grup üyesi ekle/çıkar.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice10-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import zlib from 'node:zlib'
import { testServer, TARGET } from './lib.mjs'

const { api: API } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice10'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
function png(w, h, color) {
  const raw = Buffer.alloc((w * 4 + 1) * h)
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const o = y * (w * 4 + 1) + 1 + x * 4; raw[o] = color[0]; raw[o + 1] = (x * 3) & 255; raw[o + 2] = color[2]; raw[o + 3] = 255 }
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const crc = Buffer.alloc(4); crc.writeUInt32BE(zlib.crc32(td) >>> 0); return Buffer.concat([len, td, crc]) }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))])
}

const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const proot = `/organizations/${org.id}/projects/${project.id}`
const genel = (await call('GET', `${proot}/channels`, null, owner.accessToken)).body.flatMap(c => c.channels).find(c => c.name === 'genel')
// rustbot'un fotoğrafı (sunucuya doğrudan): istemci üye listesinden öğrenip göstermeli
const up = (await call('POST', '/uploads', { name: 'bot.png', type: 'image/png', data: 'data:image/png;base64,' + png(64, 64, [220, 0, 40]).toString('base64') }, bot.accessToken)).body
await call('PATCH', '/auth/profile', { avatarUrl: up.url }, bot.accessToken)
const mine = BASE + '/ben.png'
writeFileSync(mine, png(80, 80, [20, 0, 230]))

let lines = [], client
function start(env) {
  lines = []
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: BASE + '/profile', P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_FORCE_NOTIFY: '1', ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/avatar|profil fotoğrafı|tercih|bildirim:|arama «|geçmiş|görüntüleyici|grup:/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
}
const stop = async () => { client.kill(); await sleep(600) }
const wait = async (re, ms = 15000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/${name}.png`]) } catch {} }

console.log('\n[1] Profil fotoğrafı ve avatarlar')
start({ P2_AUTOPILOT_SETTINGS: 'account', P2_AUTOPILOT_AVATAR: mine })
check('fotoğraf yüklendi', !!(await wait(/ayarlar: profil fotoğrafı \/uploads\//)))
const me = (await call('GET', '/auth/me', null, owner.accessToken)).body
const myUrl = me.user?.avatarUrl || me.avatarUrl
check('hesapta avatarUrl', /^\/uploads\//.test(myUrl || ''), myUrl)
check('kendi avatarım indi', !!(await wait(new RegExp(`avatar: ${esc(myUrl || 'yok')} hazır`))))
check('rustbot\'un avatarı (üye listesinden) indi', !!(await wait(new RegExp(`avatar: ${esc(up.url.slice(up.url.indexOf('/uploads/')))} hazır`))))
await sleep(600)
shot('1-avatarlar')
await stop()

console.log('\n[2] Yıldız ve kanal bildirimi')
start({ P2_AUTOPILOT_CHANNEL: 'başlangıç', P2_AUTOPILOT_MANAGE: 'star:genel;star-person:rustbot;notify:genel:ALL;wait:1500' })
check('kanal yıldızlandı', !!(await wait(/tercih: CHANNEL:.* yıldızlandı/)))
check('kişi yıldızlandı', !!(await wait(/tercih: PERSON:.* yıldızlandı/)))
check('bildirim: tüm mesajlar', !!(await wait(/tercih: kanal bildirimi ALL/)))
let prefs = (await call('GET', `/organizations/${org.id}/preferences`, null, owner.accessToken)).body
check('tercihler sunucuda', prefs.some(p => p.kind === 'CHANNEL' && p.targetId === genel.id && p.starred && p.notify === 'ALL') && prefs.some(p => p.kind === 'PERSON' && p.targetId === bot.user.id && p.starred))
await wait(/olay: gerçek zamanlı bağlandı/)
await sleep(500)
shot('2-yildizli')
const plain = `Sıradan kanal mesajı ${Date.now() % 100000}`
await call('POST', `${proot}/channels/${genel.id}/messages`, { content: plain }, bot.accessToken)
check('"Tüm mesajlar": bahsetmesiz mesaj bildirildi', !!(await wait(new RegExp(`bildirim: Rust Bot — ${esc(plain)}`))))
await stop()
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'CHANNEL', targetId: genel.id, notify: 'MUTED' }, owner.accessToken)
start({ P2_AUTOPILOT_CHANNEL: 'başlangıç' })
await wait(/olay: gerçek zamanlı bağlandı/)
await sleep(800)
const muted = `@p2test sessizdeyken ${Date.now() % 100000}`
await call('POST', `${proot}/channels/${genel.id}/messages`, { content: muted }, bot.accessToken)
await sleep(3000)
check('"Sessiz": bahsetme bile bildirilmedi', !lines.some(l => l.includes('bildirim: Rust Bot — ' + muted)) && lines.some(l => l.includes('olay: mesaj geldi')))
await stop()
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'CHANNEL', targetId: genel.id, notify: 'MENTIONS', starred: false }, owner.accessToken)
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'PERSON', targetId: bot.user.id, starred: false }, owner.accessToken)

console.log('\n[3] Arama, düzenleme geçmişi, görüntüleyici')
// Görüntüleyici için yeni bir resim mesajı: eski koşulardan kalan resim, kanal büyüdükçe ilk
// yüklenen sayfanın dışında kalıyordu (test verisine bağımlılık).
const pic = (await call('POST', '/uploads', { name: 'gorsel.png', type: 'image/png', data: 'data:image/png;base64,' + png(96, 64, [30, 160, 90]).toString('base64') }, owner.accessToken)).body
const picPath = pic.url.slice(pic.url.indexOf('/uploads/'))
await call('POST', `${proot}/channels/${genel.id}/messages`, { content: `görüntüleyici testi\n[[p2-file|gorsel.png|image%2Fpng|${pic.size || 100}|${picPath}]]` }, owner.accessToken)
const target = `Rust arama hedefi ${Date.now() % 100000}`
const msg = (await call('POST', `${proot}/channels/${genel.id}/messages`, { content: target }, owner.accessToken)).body
await call('PATCH', `${proot}/channels/${genel.id}/messages/${msg.id}`, { content: target + ' (düzeltildi)' }, owner.accessToken)
start({ P2_AUTOPILOT_CHANNEL: 'genel', P2_AUTOPILOT_SEARCH: target, P2_AUTOPILOT_HISTORY: '1', P2_AUTOPILOT_VIEW: '1' })
check('arama sonucu bulundu', !!(await wait(new RegExp(`sohbet: arama «${esc(target)}» 1 sonuç`))))
check('düzenleme geçmişi (1 eski sürüm)', !!(await wait(/sohbet: düzenleme geçmişi 1 sürüm/)))
await sleep(500)
shot('3-gecmis-arama')
check('resim görüntüleyici açıldı', !!(await wait(/sohbet: görüntüleyici/, 20000)))
await sleep(500)
shot('4-goruntuleyici')
await stop()

console.log('\n[4] Grup üyeleri')
const group = (await call('GET', `/organizations/${org.id}/groups?projectId=${project.id}`, null, owner.accessToken)).body.find(g => g.name === 'Rust Grubu')
start({ P2_AUTOPILOT_OPEN_GROUP: 'Rust Grubu', P2_AUTOPILOT_GROUP_MEMBERS: 'add:rustbot2;remove:rustbot2' })
check('üye penceresi açıldı', !!(await wait(/grup: üyeler açıldı/)))
check('üye eklendi', !!(await wait(/grup: üye eklendi/)))
check('üye çıkarıldı', !!(await wait(/grup: üye çıkarıldı/)))
const after = (await call('GET', `/organizations/${org.id}/groups?projectId=${project.id}`, null, owner.accessToken)).body.find(g => g.id === group?.id)
check('grup eski haline döndü', !!after && !after.members.some(m => m.username === 'rustbot2') && after.members.length === group.members.length, `${group?.members.length} → ${after?.members.length}`)
await stop()

// Temizlik: fotoğraflar
await call('PATCH', '/auth/profile', { avatarUrl: '' }, owner.accessToken)
await call('PATCH', '/auth/profile', { avatarUrl: '' }, bot.accessToken)
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
