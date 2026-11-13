// Faz 0 / Spike 2 — Rust WebRTC botu ile gerçek P2 Electron istemcisi aynı aramada.
//
// Masaüstüne sentetik giriş GÖNDERMEZ. Electron'u ayrı profil + uzaktan hata ayıklama
// portuyla kendisi açar; giriş ve "Kabul et" sayfanın içinde DOM üzerinden yapılır.
// Ölçüm Electron tarafındaki RTCPeerConnection istatistikleri ve botun kendi çıktısıdır.
//
// Önkoşul: test PostgreSQL + NestJS (P2_API, varsayılan :4110) + Vite (:5173) çalışıyor.
// Kullanım: node interop.mjs <P2 repo yolu>
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const repo = process.argv[2]
const API = process.env.P2_API || 'http://127.0.0.1:4110'
const PROFILE = 'C:\\p2-pg\\electron-profile'
const OUT = process.env.P2_SPIKE_OUT || 'C:\\p2-pg\\spike2'
const CDP_PORT = 9333
const HUMAN = { username: 'p2test', password: 'Parola123', email: 'p2test@test.dev', displayName: 'P2 Test' }
const BOT = { username: 'rustbot', password: 'Parola123', email: 'rustbot@test.dev', displayName: 'Rust Bot' }
mkdirSync(OUT, { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
const log = m => console.log(`[sürücü] ${m}`)

async function call(method, path, body, token) {
  const res = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: res.status, body: await res.json().catch(() => null) }
}
async function ensureUser(u, admin) {
  const login = await call('POST', '/auth/login', { identifier: u.username, password: u.password })
  if (login.status === 201) return login.body
  const r = await call('POST', admin ? '/setup/admin' : '/auth/register', { email: u.email, username: u.username, displayName: u.displayName, password: u.password, passwordConfirm: u.password })
  if (r.status !== 201) throw new Error(`${u.username} oluşturulamadı: ${JSON.stringify(r.body)}`)
  return r.body
}

// ---------------------------------------------------------------- 1) hesaplar + kuruluş
const installed = (await call('GET', '/setup/status')).body.installed
const human = await ensureUser(HUMAN, !installed)
const bot = await ensureUser(BOT, false)
let orgs = (await call('GET', '/organizations', null, human.accessToken)).body
if (!orgs.length) {
  await call('POST', '/organizations', { name: 'Spike 2', slug: 'spike-2' }, human.accessToken)
  orgs = (await call('GET', '/organizations', null, human.accessToken)).body
}
const botOrgs = (await call('GET', '/organizations', null, bot.accessToken)).body
if (!botOrgs.some(o => o.id === orgs[0].id)) {
  const j = await call('POST', '/organizations/join', { code: orgs[0].inviteCode }, bot.accessToken)
  if (j.status >= 300) throw new Error('bot kuruluşa katılamadı: ' + JSON.stringify(j.body))
}
// Gelen arama kaplaması proje çalışma alanının parçası; proje yoksa arayüz "ilk proje" ekranında kalır.
const projects = (await call('GET', `/organizations/${orgs[0].id}/projects`, null, human.accessToken)).body
if (!Array.isArray(projects) || !projects.length) {
  const p = await call('POST', `/organizations/${orgs[0].id}/projects`, { name: 'Spike 2', slug: 'spike-2', engine: 'CUSTOM' }, human.accessToken)
  if (p.status >= 300) throw new Error('proje oluşturulamadı: ' + JSON.stringify(p.body))
}
log(`hesaplar hazır: ${HUMAN.username} (Electron) ve ${BOT.username} (Rust), kuruluş "${orgs[0].name}"`)

// ---------------------------------------------------------------- 2) Electron (ayrı profil)
const electronExe = join(repo, 'node_modules', 'electron', 'dist', 'electron.exe')
const env = { ...process.env, P2_REPO: repo, P2_TEST_PROFILE: PROFILE, P2_API_URL: API }
delete env.ELECTRON_RUN_AS_NODE
const electron = spawn(electronExe, [
  join(repo, 'rust', 'tools', 'spike2', 'electron-isolated.cjs'),
  `--remote-debugging-port=${CDP_PORT}`,
  '--use-fake-device-for-media-stream', // sahte mikrofon (bip) — gerçek mikrofon kullanılmaz
  '--use-fake-ui-for-media-stream',
], { env, stdio: 'ignore' })
let electronExited = false
electron.on('exit', code => { electronExited = true; log(`Electron kapandı (kod ${code})`) })

async function target() {
  for (let i = 0; i < 60; i++) {
    if (electronExited) throw new Error('Electron hemen kapandı — profil izolasyonu tutmamış olabilir (tek örnek kilidi).')
    try {
      const list = await (await fetch(`http://127.0.0.1:${CDP_PORT}/json/list`)).json()
      const page = list.find(t => t.type === 'page' && t.url.startsWith('http://localhost:5173'))
      if (page) return page
    } catch {}
    await sleep(500)
  }
  throw new Error('Electron ana penceresi bulunamadı')
}
const page = await target()
if (!existsSync(PROFILE)) throw new Error('ayrı profil klasörü oluşmadı')
log(`Electron açıldı, ayrı profil: ${PROFILE}`)

// CDP
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise(r => ws.addEventListener('open', r, { once: true }))
let seq = 0; const waiting = new Map()
ws.addEventListener('message', e => { const m = JSON.parse(e.data); if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id) } })
const cdp = (method, params = {}) => new Promise(r => { const id = ++seq; waiting.set(id, r); ws.send(JSON.stringify({ id, method, params })) })
const evaluate = async (expression) => {
  const r = await cdp('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'değerlendirme hatası')
  return r.result?.result?.value
}
const shot = async name => { const r = await cdp('Page.captureScreenshot', { format: 'png' }); writeFileSync(join(OUT, name + '.png'), Buffer.from(r.result.data, 'base64')); log(`ekran görüntüsü: ${join(OUT, name + '.png')}`) }
const waitFor = async (expression, ms = 20000, what = expression) => {
  const end = Date.now() + ms
  while (Date.now() < end) { if (await evaluate(expression).catch(() => false)) return true; await sleep(300) }
  // Teşhis: sayfa metni + görüntü, sonra temiz kapanış.
  const text = await evaluate(`location.href + '\\n' + document.body.innerText.slice(0, 800)`).catch(e => String(e))
  console.log('--- sayfa ---\n' + text + '\n-------------')
  await shot('hata').catch(() => {})
  electron.kill()
  throw new Error('beklenen durum oluşmadı: ' + what)
}

// RTCPeerConnection örneklerini yakala (ölçüm için), sonra sayfayı yeniden yükle.
await cdp('Page.enable')
// Ayrıca "Kurulum Türü" seçimi: bu (ayrı) profilde istemci modu + test sunucusu.
await cdp('Page.addScriptToEvaluateOnNewDocument', { source: `(()=>{try{localStorage.setItem('p2:mode','client');localStorage.setItem('p2:apiUrl',${JSON.stringify(API)})}catch{}})()` })
await cdp('Page.addScriptToEvaluateOnNewDocument', { source: `(()=>{const N=window.RTCPeerConnection;window.__pcs=[];window.RTCPeerConnection=function(...a){const pc=new N(...a);window.__pcs.push(pc);return pc};window.RTCPeerConnection.prototype=N.prototype;Object.setPrototypeOf(window.RTCPeerConnection,N)})()` })
await cdp('Page.reload')
await sleep(1500)

// ---------------------------------------------------------------- 3) giriş (sayfa içinden)
await waitFor(`!!document.querySelector('input[name=identifier]') || /PROJE KANALLARI/.test(document.body.innerText)`, 30000, 'giriş formu ya da çalışma alanı')
const needsLogin = await evaluate(`!!document.querySelector('input[name=identifier]')`)
if (needsLogin) {
  await evaluate(`(()=>{const set=(el,v)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,v);el.dispatchEvent(new Event('input',{bubbles:true}))};
    set(document.querySelector('input[name=identifier]'),${JSON.stringify(HUMAN.username)});
    set(document.querySelector('input[name=password]'),${JSON.stringify(HUMAN.password)});
    document.querySelector('input[name=identifier]').form.requestSubmit();return true})()`)
  log('Electron\'da giriş yapıldı (sayfa içinden)')
}
await waitFor(`!document.querySelector('input[name=identifier]') && !/Kuruluşlar yükleniyor/.test(document.body.innerText)`, 30000, 'çalışma alanı')
await sleep(2500)
await shot('1-calisma-alani')

// ---------------------------------------------------------------- 4) Rust botu arar
const botExe = 'C:\\p2-rust-target\\release\\p2-callbot.exe'
const botLog = []
const botProc = spawn(botExe, [], { env: { ...process.env, P2_API: API, P2_BOT_USER: BOT.username, P2_BOT_PASS: BOT.password, P2_CALL_TARGET: HUMAN.username } })
botProc.stdout.on('data', d => { for (const line of String(d).split(/\r?\n/).filter(Boolean)) { botLog.push(line); console.log('  [bot] ' + line) } })
botProc.stderr.on('data', d => console.log('  [bot!] ' + String(d).trim()))

await waitFor(`!!document.querySelector('button.accept')`, 20000, 'gelen arama ekranı')
await shot('2-gelen-arama')
await evaluate(`document.querySelector('button.accept').click(),true`)
log('"Kabul et" tıklandı (sayfa içinden)')

// ---------------------------------------------------------------- 5) ölçüm
await sleep(12000)
const stats = await evaluate(`(async()=>{const out=[];for(const pc of window.__pcs){if(pc.connectionState==='closed')continue;const r=await pc.getStats();const s={state:pc.connectionState,codecs:[],in:{},out:{}};
  const codecs={};r.forEach(x=>{if(x.type==='codec')codecs[x.id]=x.mimeType});
  r.forEach(x=>{if(x.type==='inbound-rtp'){s.in[x.kind]={packets:x.packetsReceived,lost:x.packetsLost,codec:codecs[x.codecId],framesDecoded:x.framesDecoded,width:x.frameWidth,height:x.frameHeight,fps:x.framesPerSecond,energy:x.totalAudioEnergy,jitterMs:x.jitter!=null?Math.round(x.jitter*1000):undefined}}
    if(x.type==='outbound-rtp'){s.out[x.kind]={packets:x.packetsSent,codec:codecs[x.codecId]}}
    if(x.type==='candidate-pair'&&x.nominated)s.rttMs=Math.round((x.currentRoundTripTime||0)*1000)});out.push(s)}
  const videos=[...document.querySelectorAll('video')].map(v=>({w:v.videoWidth,h:v.videoHeight,playing:!v.paused}));return {peers:out,videos}})()`)
await shot('3-arama')

// Electron → Rust görüntü: P2'nin kamera düğmesi (sahte kamera; gerçek kamera/ekran kullanılmaz).
await evaluate(`document.querySelector('button[aria-label="Kamera"]').click(),true`)
log('Electron\'da kamera açıldı (sahte kamera)')
await sleep(7000)
// Arama sahnesini aç (başlık menüsü → aktif arama): Rust'ın test deseni P2 arayüzünde görünmeli.
await evaluate(`document.querySelector('.menu-trigger').click(),true`)
await waitFor(`!!document.querySelector('.menu-call')`, 5000, 'menüdeki arama satırı')
await evaluate(`document.querySelector('.menu-call').click(),true`)
await sleep(2500)
const tiles = await evaluate(`[...document.querySelectorAll('.call-stage video')].map(v=>({w:v.videoWidth,h:v.videoHeight,playing:!v.paused&&v.readyState>=2}))`)
await shot('4-sahne')
stats.tiles = tiles
console.log('\n=== Electron tarafı (RTCPeerConnection.getStats) ===')
console.log(JSON.stringify(stats, null, 2))
console.log('\n=== Bot tarafı (son ölçümler) ===')
for (const l of botLog.filter(l => l.includes('←')).slice(-3)) console.log('  ' + l)
console.log('Arama sahnesindeki video kutuları: ' + JSON.stringify(stats.tiles))

// ---------------------------------------------------------------- 6) değerlendirme + temizlik
const peer = stats?.peers?.find(p => p.state === 'connected')
const checks = [
  ['bağlantı kuruldu (connected)', !!peer],
  ['Rust → Electron ses (Opus) geliyor', !!peer?.in?.audio?.packets && /opus/i.test(peer?.in?.audio?.codec || '')],
  ['Rust → Electron görüntü çözülüyor', (peer?.in?.video?.framesDecoded || 0) > 30],
  ['Electron → Rust ses gidiyor', (peer?.out?.audio?.packets || 0) > 100],
  ['bot Electron sesini alıyor', botLog.some(l => /ses: [1-9]\d* kare\/sn/.test(l))],
  ['bot Electron kamerasının görüntüsünü alıyor', botLog.some(l => /görüntü: [1-9][\d.]* kare\/sn [1-9]\d*x[1-9]\d*/.test(l))],
  ['Rust test deseni P2 arama sahnesinde oynuyor', (stats.tiles || []).some(t => t.playing && t.w === 1280 && t.h === 720)],
]
console.log('\n=== SONUÇ ===')
for (const [name, ok] of checks) console.log(`  ${ok ? '✔' : '✘'} ${name}`)
botProc.kill()
await sleep(500)
ws.close()
electron.kill()
process.exit(checks.every(([, ok]) => ok) ? 0 : 1)
