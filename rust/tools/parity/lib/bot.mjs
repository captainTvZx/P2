// Arama ekranları (I ajanı): aygıtsız Rust arama botu (apps/p2-callbot) — ses odasına girer ya da
// arama ekranlarının kullanıcısını (CALL_USER) arar. Bot hiçbir ses aygıtı açmaz (libwebrtc kaynakları
// sentetik). Hesaplar: bot rustbot2 (slice12 de kullanır; aynı anda koşturmayın), "ben" rustbot.
//
// Neden p2test değil: sunucu kullanıcıyı (oturumu değil) aramaya alır; p2test'in diğer parite
// ajanlarındaki açık oturumları da arama panelini gösterir ve son bağlantı kopana dek kullanıcı odada
// "hayalet" kalır. Arama ekranları bu yüzden yalnız onlara ait rustbot hesabıyla açılır ve her taraftan
// sonra `call:leave` ile odadan açıkça çıkarılır.
import { spawn } from 'node:child_process'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { API, REPO, RUST_BIN, sleep, log } from './common.mjs'

export const CALL_USER = 'rustbot'
const PASS = 'Parola123'

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

/** rustbot2 hesabı yoksa açılır ve test kuruluşuna katılır (veri yalnız bot hesabı için yazılır). */
async function ensureBotAccount() {
  let bot = (await call('POST', '/auth/login', { identifier: 'rustbot2', password: PASS })).body
  if (!bot?.accessToken) bot = (await call('POST', '/auth/register', { email: 'rustbot2@p2.test', username: 'rustbot2', displayName: 'Rust Bot 2', password: PASS, passwordConfirm: PASS })).body
  const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: PASS })).body
  const org = (await call('GET', '/organizations', null, owner.accessToken)).body?.[0]
  if (org && !(await call('GET', '/organizations', null, bot.accessToken)).body?.some(o => o.id === org.id)) await call('POST', '/organizations/join', { code: org.inviteCode }, bot.accessToken)
}

/** `user` adına (socket.io /stream) `call:leave` — o kullanıcı bütün oturumlarıyla aramadan çıkar. */
export async function leaveCall(callId, user = CALL_USER) {
  if (!callId) return
  const { io } = createRequire(join(REPO, 'package.json'))('socket.io-client')
  const login = (await call('POST', '/auth/login', { identifier: user, password: PASS })).body
  if (!login?.accessToken) return
  const socket = io(API + '/stream', { auth: { token: login.accessToken }, transports: ['websocket'], reconnection: false })
  await new Promise(resolve => { socket.on('connect', resolve); socket.on('connect_error', resolve); setTimeout(resolve, 4000) })
  await new Promise(resolve => socket.timeout(3000).emit('call:leave', { callId }, () => resolve()))
  socket.close()
}

/** Ses odalarında kalmış kendi hesaplarımızı (ekran kullanıcısı + bot) çıkarır: `subscribe` sonrası
 *  sunucunun gönderdiği `voice:presence` anlık görüntüsünden çağrı kimlikleri okunur. */
export async function clearRooms(users = [CALL_USER, 'rustbot2']) {
  const { io } = createRequire(join(REPO, 'package.json'))('socket.io-client')
  for (const user of users) {
    const login = (await call('POST', '/auth/login', { identifier: user, password: PASS })).body
    if (!login?.accessToken) continue
    const org = (await call('GET', '/organizations', null, login.accessToken)).body?.[0]
    if (!org) continue
    const socket = io(API + '/stream', { auth: { token: login.accessToken }, transports: ['websocket'], reconnection: false })
    const calls = new Set()
    socket.on('voice:presence', p => { if (p?.participants?.some(x => x.userId === login.user.id)) calls.add(p.callId) })
    await new Promise(resolve => { socket.on('connect', resolve); socket.on('connect_error', resolve); setTimeout(resolve, 4000) })
    await new Promise(resolve => socket.timeout(3000).emit('subscribe', { organizationId: org.id }, () => resolve()))
    await sleep(400)
    for (const id of calls) await new Promise(resolve => socket.timeout(3000).emit('call:leave', { callId: id }, () => resolve()))
    if (calls.size) log(`arama: ${user} ${calls.size} odadan çıkarıldı (önceki koşudan kalmış)`)
    socket.close()
  }
}

export class CallBot {
  /** env: P2_CALL_CHANNEL | P2_CALL_TARGET, P2_BOT_SILENT, P2_BOT_NO_SCREEN, P2_BOT_MUTED… */
  static async start(env = {}) {
    await ensureBotAccount()
    await clearRooms()
    const b = new CallBot()
    b.lines = []
    b.child = spawn(join(RUST_BIN, 'p2-callbot.exe'), [], { env: { ...process.env, P2_API: API, P2_BOT_USER: 'rustbot2', P2_BOT_PASS: PASS, ...env }, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
    const take = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) b.lines.push(l) }
    b.child.stdout.on('data', take)
    b.child.stderr.on('data', take)
    const end = Date.now() + 15000
    while (Date.now() < end && !b.lines.some(l => /aramaya katıldı|aranıyor|bekleniyor/.test(l))) {
      if (b.child.exitCode !== null) throw new Error('arama botu kapandı:\n' + b.lines.slice(-10).join('\n'))
      await sleep(100)
    }
    // Önceki koşudan odada kalmış "hayalet" kullanıcı (kendi hesabımız) çıkarılır.
    if (env.P2_CALL_CHANNEL) { await leaveCall(b.callId()); await sleep(500) }
    log(`arama botu: ${b.lines.slice(-1)[0] || '?'}`)
    return b
  }

  callId() {
    for (const l of [...this.lines].reverse()) { const m = /aramaya katıldı: (\S+)/.exec(l); if (m) return m[1] }
    return null
  }

  async stop() {
    // Ekran kullanıcısı odadan açıkça çıkar (başka oturumu kalsa da hayalet kalmasın).
    await leaveCall(this.callId()).catch(() => {})
    if (this.child && this.child.exitCode === null) {
      try { this.child.kill() } catch {}
      await sleep(800)
    }
  }
}
