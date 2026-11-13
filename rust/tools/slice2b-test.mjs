// Dilim 2b uçtan uca testi: sohbetin kalanları. Rust istemcisi (p2test) otomatik pilotla
// yanıt/alıntı, iş parçacığı, dosya eki (resim önizleme), @bahsetme önerisi, emoji paneli,
// mesaj iletme, forum konusu ve kanal tarayıcısı (ayrıl/katıl) akışlarını sürer; sonuçlar
// sunucudan doğrulanır. Karşı taraf (rustbot) REST ile yazar. Masaüstüne giriş göndermez.
// Önkoşul: test PostgreSQL (:55432) + P2 sunucusu (P2_API, varsayılan NestJS :4110).
//   node rust/tools/slice2b-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import zlib from 'node:zlib'

const API = process.env.P2_API || 'http://127.0.0.1:4110'
const BIN = 'C:/p2-rust-target/release/'
const BASE = 'C:/p2-pg/slice2b'
const PROFILE = BASE + '/profile'
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

// --- hazırlık
const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const proot = `/organizations/${org.id}/projects/${project.id}`
let cats = (await call('GET', `${proot}/channels`, null, owner.accessToken)).body
const all = () => cats.flatMap(c => c.channels)
async function ensure(name, type) {
  let ch = all().find(c => c.name === name)
  if (!ch) {
    const r = await call('POST', `${proot}/channels`, { categoryId: cats[0].id, name, type }, owner.accessToken)
    if (r.status >= 300) throw new Error(`${name} kurulamadı: ${JSON.stringify(r.body)}`)
    cats = (await call('GET', `${proot}/channels`, null, owner.accessToken)).body
    ch = all().find(c => c.name === name)
  }
  return ch
}
const A = all().find(c => c.name === 'genel') || all().find(c => c.type === 'TEXT')
const B = await ensure('rust-ilet', 'TEXT')
const F = await ensure('rust-forum', 'FORUM')
// Önceki koşuda ayrılınmışsa geri katıl
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'CHANNEL', targetId: B.id, hidden: false }, owner.accessToken)
const chBase = id => `${proot}/channels/${id}`
const lastMessages = async (id, n = 5) => (await call("GET", `${chBase(id)}/messages`, null, owner.accessToken)).body.sort((a, b) => b.createdAt.localeCompare(a.createdAt)).slice(0, n)
const stamp = Date.now().toString().slice(-6)
console.log(`hazırlık: "${project.name}" — #${A.name}, #${B.name}, forum #${F.name}`)

let client, lines
function start(env) {
  lines = []
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_CHANNEL: A.name, ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/sohbet: (yan|ek|bahs|mesaj)|konu:|kanallar:|olay: yanıt/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
}
const stop = async () => { client.kill(); await sleep(600) }
const waitLine = async (re, ms = 15000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/${name}.png`]) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)) } }

// ============================================================ 1) Yanıt / alıntı
console.log('\n[1] Yanıtla (alıntı)')
const botMsg = (await call('POST', `${chBase(A.id)}/messages`, { content: `Bot: beni yanıtla ${stamp}` }, bot.accessToken)).body
const replyText = `Rust yanıtı ${stamp}`
start({ P2_AUTOPILOT_REPLY: replyText })
check('yanıt başlatıldı', !!(await waitLine(new RegExp(`sohbet: yanıtlanıyor «Bot: beni yanıtla ${stamp}»`))))
await sleep(1500)
const afterReply = await lastMessages(A.id)
const reply = afterReply.find(m => m.content === replyText)
check('yanıt alıntıyla gönderildi', reply?.quotedMessage?.id === botMsg.id, reply ? `alıntı: ${reply.quotedMessage?.content}` : 'yok')
shot('1-yanit')
await stop()

// ============================================================ 2) İş parçacığı
console.log('\n[2] İş parçacığı')
const threadText = `Rust konu yanıtı ${stamp}`
start({ P2_AUTOPILOT_THREAD: threadText })
check('konu paneli açıldı', !!(await waitLine(/konu: açıldı/)))
check('konuya yanıt gönderildi', !!(await waitLine(new RegExp(`konu: yanıt gönderildi «${esc(threadText)}»`))))
const parent = (await lastMessages(A.id)).find(m => m.content === replyText)
const replies = (await call('GET', `${chBase(A.id)}/messages/${parent.id}/replies`, null, owner.accessToken)).body
check('yanıt sunucuda (iş parçacığında)', replies.some(r => r.content === threadText))
await waitLine(/olay: gerçek zamanlı bağlandı/, 15000)
const botReply = `Bot konu yanıtı ${stamp}`
await call('POST', `${chBase(A.id)}/messages/${parent.id}/replies`, { content: botReply }, bot.accessToken)
check('karşı tarafın yanıtı gerçek zamanlı konuya geldi', !!(await waitLine(new RegExp(`olay: yanıt geldi «${esc(botReply)}»`))))
await sleep(700)
shot('2-konu')
await stop()

// ============================================================ 3) Dosya eki
console.log('\n[3] Dosya eki')
// Gerçek bir PNG (96x48 renk geçişi): zlib + crc32 ile
const png = (() => {
  const w = 96, h = 48, raw = Buffer.alloc((w * 4 + 1) * h)
  for (let y = 0; y < h; y++) { raw[y * (w * 4 + 1)] = 0; for (let x = 0; x < w; x++) { const o = y * (w * 4 + 1) + 1 + x * 4; raw[o] = x * 2; raw[o + 1] = 120; raw[o + 2] = 255 - y * 4; raw[o + 3] = 255 } }
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const crc = Buffer.alloc(4); crc.writeUInt32BE(zlib.crc32(td) >>> 0); return Buffer.concat([len, td, crc]) }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))])
})()
const pngPath = `${BASE}/rust-ek-${stamp}.png`
writeFileSync(pngPath, png)
const attachText = `Ekli resim ${stamp}`
start({ P2_AUTOPILOT_ATTACH: `${pngPath}|${attachText}` })
check('ek yüklendi', !!(await waitLine(/sohbet: ek yüklendi rust-ek-\d+\.png/)))
await sleep(1800)
const withFile = (await lastMessages(A.id)).find(m => m.content.startsWith(attachText))
check('mesaj ek işaretiyle gönderildi', !!withFile && /\[\[p2-file\|rust-ek-\d+\.png\|image%2Fpng\|\d+\|\/uploads\/[a-f0-9-]+\.png\]\]/.test(withFile.content), withFile?.content.split('\n')[1] || '')
const upPath = /\|(\/uploads\/[^\]]+)\]\]/.exec(withFile?.content || '')?.[1]
const fetched = upPath ? await fetch(API + upPath) : null
check('yüklenen dosya sunucudan iniyor', fetched?.status === 200 && Buffer.from(await fetched.arrayBuffer()).equals(png))
check('resim önizlemesi çözüldü', !!(await waitLine(new RegExp('sohbet: resim önizlemesi ' + esc(upPath || 'yok') + ' hazır'), 8000)))
shot('3-ek')
await stop()

// ============================================================ 4) Bahsetme + 5) Emoji
console.log('\n[4] @bahsetme ve emoji')
start({ P2_AUTOPILOT_MENTION: 'rust' })
check('öneri listesinde rustbot', !!(await waitLine(/sohbet: bahsetme önerileri \[.*"rustbot".*\]/)))
await sleep(1500)
const mention = (await lastMessages(A.id)).find(m => m.content === '@rustbot bahsetme testi')
check('bahsetme seçilip gönderildi', !!mention)
const act = (await call('GET', `/organizations/${org.id}/activity`, null, bot.accessToken)).body
const acts = Array.isArray(act) ? act : act?.items || []
check('rustbot\'a bahsetme etkinliği düştü', acts.some(a => a.kind === 'MENTION' && a.messageId === mention?.id))
await stop()
start({ P2_AUTOPILOT_EMOJI: `Emoji testi ${stamp}` })
await waitLine(/sohbet: #/, 10000)
await sleep(2000)
check('emoji panelden eklendi', (await lastMessages(A.id)).some(m => m.content === `Emoji testi ${stamp} 🎉`))
await stop()

// ============================================================ 6) İlet
console.log('\n[6] İlet')
start({ P2_AUTOPILOT_FORWARD: B.name })
check('mesaj iletildi', !!(await waitLine(new RegExp(`sohbet: mesaj #${esc(B.name)} kanalına iletildi`))))
const fwd = (await lastMessages(B.id, 1))[0]
check('iletilen mesaj hedef kanalda', fwd?.forwardedFromId && fwd.content === `Emoji testi ${stamp} 🎉`, fwd?.content || '')
await stop()

// ============================================================ 7) Forum
console.log('\n[7] Forum')
const topic = `Rust forum konusu ${stamp}`
start({ P2_AUTOPILOT_CHANNEL: F.name, P2_AUTOPILOT_SAY: topic })
await waitLine(new RegExp(`sohbet: #${esc(F.name)} \\d+ mesaj yüklendi`))
await sleep(1500)
await stop()
const topicMsg = (await lastMessages(F.id)).find(m => m.content === topic)
check('konu açıldı', !!topicMsg)
start({ P2_AUTOPILOT_CHANNEL: F.name, P2_AUTOPILOT_THREAD: `Konuya yanıt ${stamp}` })
check('forum konusu panelde', !!(await waitLine(/konu: açıldı «Rust forum konusu/)))
check('konuya yanıt gönderildi', !!(await waitLine(/konu: yanıt gönderildi/)))
const topicReplies = (await call('GET', `${chBase(F.id)}/topics/${topicMsg?.id}/replies`, null, owner.accessToken)).body
check('forum yanıtı sunucuda', Array.isArray(topicReplies) && topicReplies.some(r => r.content === `Konuya yanıt ${stamp}`))
await sleep(500)
shot('7-forum')
await stop()

// ============================================================ 8) Kanal tarayıcısı
console.log('\n[8] Kanal tarayıcısı')
start({ P2_AUTOPILOT_BROWSER: `leave:${B.name}` })
check('tarayıcı açıldı', !!(await waitLine(/kanallar: tarayıcı açıldı/)))
check('kanaldan ayrılındı', !!(await waitLine(new RegExp(`kanallar: ${B.id} ayrılındı`))))
await sleep(600)
shot('8-tarayici')
let prefs = (await call('GET', `/organizations/${org.id}/preferences`, null, owner.accessToken)).body
check('tercih sunucuda (gizli)', prefs.some(p => p.targetId === B.id && p.hidden))
await stop()
start({ P2_AUTOPILOT_BROWSER: `join:${B.name}` })
check('ayrılınan kanal yüklendi', !!(await waitLine(/kanallar: 1 kanaldan ayrılınmış/)))
check('kanala geri katılındı', !!(await waitLine(new RegExp(`kanallar: ${B.id} katılındı`))))
prefs = (await call('GET', `/organizations/${org.id}/preferences`, null, owner.accessToken)).body
check('tercih sunucuda (görünür)', prefs.some(p => p.targetId === B.id && !p.hidden))
await stop()

console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
