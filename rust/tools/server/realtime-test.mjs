// Gerçek zamanlı ağ geçidi (realtime.ts → realtime.rs + calls.rs) sözleşme testi: aynı senaryoyu
// NestJS (:4110) ve Rust (:4125) üzerinde socket.io-client ile çalıştırır; her adımda ack'leri ve
// her istemcinin aldığı olayları (olay adına göre gruplanmış, kimlikler etiketlenmiş) karşılaştırır.
//   node rust/tools/server/realtime-test.mjs           (45 sn'lik cevapsız arama için: FULL=1)
// Kurulum (kanal, grup, doküman) NestJS REST ile yapılır; test sonunda oluşturulanlar silinir.
import { createRequire } from 'node:module'
import { randomBytes } from 'node:crypto'
import { NEST, startRust, call, login, same, check, done, mask, sleep } from './lib.mjs'

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')
const pg = require('pg')
const env = JSON.parse((await import('node:fs')).readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^\uFEFF/, ''))
const db = new pg.Client({ connectionString: env.DATABASE_URL })
await db.connect()

const tag = randomBytes(3).toString('hex')
const SETTLE = 500
const RELEVANT = /^(presence:|typing$|library:playing$|document:cursor$|voice:presence$|call:|direct:(created|updated)$|group:(created|updated|message)$)/

const login3 = async () => ({ A: await login(NEST, 'p2test'), B: await login(NEST, 'rustbot'), C: await login(NEST, 'rustbot2') })
const users = await login3()
const ids = { A: users.A.user.id, B: users.B.user.id, C: users.C.user.id }
const org = (await call(NEST, 'GET', '/organizations', undefined, users.A.accessToken)).body[0].id
const project = (await call(NEST, 'GET', `/organizations/${org}/projects`, undefined, users.A.accessToken)).body[0].id
const categories = (await call(NEST, 'GET', `/organizations/${org}/projects/${project}/channels`, undefined, users.A.accessToken)).body
const textChannel = categories.flatMap(c => c.channels).find(c => c.type === 'TEXT').id
const category = categories[categories.length - 1].id

function connect(base, token) {
  const s = io(`${base}/stream`, { auth: { token }, transports: ['websocket'], reconnection: false, forceNew: true })
  const client = { s, events: [] }
  s.onAny((event, payload) => client.events.push([event, payload]))
  return new Promise((resolve, reject) => { s.once('connect', () => resolve(client)); s.once('connect_error', reject) })
}
const ack = (client, event, body) => client.s.timeout(5000).emitWithAck(event, body).catch(() => 'ACK-TIMEOUT')

async function scenario(base, label) {
  const tok = users
  const created = { messages: [], groups: [] }
  // Kurulum (NestJS REST): yeni sesli kanal, grup, doküman.
  const channel = (await call(NEST, 'POST', `/organizations/${org}/projects/${project}/channels`, { categoryId: category, name: `rt-${tag}-${label}`, type: 'VOICE' }, tok.A.accessToken)).body.id
  const group = (await call(NEST, 'POST', `/organizations/${org}/groups`, { projectId: project, userIds: [ids.B], name: `rt-${tag}-${label}` }, tok.A.accessToken)).body.id
  const doc = (await call(NEST, 'POST', `/organizations/${org}/projects/${project}/documents`, { title: `rt-${tag}-${label}`, kind: 'CANVAS' }, tok.A.accessToken)).body.id
  await call(NEST, 'PUT', `/organizations/${org}/projects/${project}/documents/${doc}/permissions/${ids.B}`, { access: 'READ' }, tok.A.accessToken)
  created.groups.push(group)

  const labels = new Map([[ids.A, 'A'], [ids.B, 'B'], [ids.C, 'C'], [org, 'ORG'], [project, 'PRJ'], [channel, 'CH'], [textChannel, 'TXT'], [group, 'GRP'], [doc, 'DOC']])
  let callNo = 0, convNo = 0
  const learn = value => {
    // Ack'lerdeki arama kimliklerini ve olaylardaki sohbet/kanal grubu kimliklerini etiketle.
    if (value?.call?.id && !labels.has(value.call.id)) labels.set(value.call.id, 'CALL' + ++callNo)
  }
  const norm = value => {
    if (Array.isArray(value)) return value.map(norm)
    if (value && typeof value === 'object') {
      const out = {}
      for (const [k, v] of Object.entries(value)) out[k] = norm(v)
      if (out.kind === 'CALL_ENDED' && typeof out.content === 'string') out.content = Number(out.content) >= 1 ? '<sn>' : out.content
      return out
    }
    if (typeof value === 'string' && labels.has(value)) return labels.get(value)
    if (typeof value === 'number' && value > 1e12) return '<ms>'
    return value
  }
  const clients = {}
  const results = []
  const collect = () => {
    const out = {}
    for (const [name, client] of Object.entries(clients)) {
      const grouped = {}
      for (const [event, payload] of client.events) {
        if (!RELEVANT.test(event)) continue
        if (/^presence:(update|activity)$/.test(event) && !Object.values(ids).includes(payload?.userId)) continue
        if (event === 'direct:created' || event === 'direct:updated') {
          if (!labels.has(payload.conversationId)) labels.set(payload.conversationId, 'CONV' + ++convNo)
          if (!created.messages.includes(payload.message.id)) created.messages.push(payload.message.id)
        }
        if (event === 'group:created' && !labels.has(payload.group.id)) { labels.set(payload.group.id, 'CHGRP'); created.groups.push(payload.group.id) }
        ;(grouped[event] ||= []).push(payload)
      }
      // voice:presence arka planda yayınlanır (void emitChannel): sırası belirsiz, çoklu küme olarak karşılaştır.
      if (grouped['voice:presence']) grouped['voice:presence'] = grouped['voice:presence'].map(norm).sort((x, y) => JSON.stringify(x).localeCompare(JSON.stringify(y)))
      out[name] = Object.fromEntries(Object.keys(grouped).sort().map(k => [k, norm(grouped[k])]))
      client.events.length = 0
    }
    return out
  }
  const step = async (name, fn, settle = SETTLE) => {
    for (const client of Object.values(clients)) client.events.length = 0
    const acks = []
    const a = async (who, event, body) => { const r = await ack(clients[who], event, body); learn(r); acks.push([who, event, norm(r)]); return r }
    await fn(a)
    await sleep(settle)
    results.push([name, { acks: acks.map(x => [x[0], x[1], norm(x[2])]), events: collect() }])
  }

  // ---------------------------------------------------------------- bağlantı ve durum
  await step('bağlantı (A, B, C)', async () => {
    clients.A = await connect(base, tok.A.accessToken); await sleep(200)
    clients.B = await connect(base, tok.B.accessToken); await sleep(200)
    clients.C = await connect(base, tok.C.accessToken)
  })
  await step('subscribe / snapshot', async a => {
    await a('C', 'presence:snapshot', { organizationId: org })
    await a('A', 'subscribe', { organizationId: org })
    await a('B', 'subscribe', { organizationId: org })
    await a('C', 'subscribe', { organizationId: org })
    await a('C', 'subscribe', { organizationId: 'yok' })
    await a('C', 'subscribe', {})
    await a('C', 'presence:snapshot', { organizationId: org })
  })
  await step('presence:prefer', async a => {
    await a('B', 'presence:prefer', { status: 'BUSY' })
    await a('B', 'presence:prefer', { status: 'UYKU' })
    await a('B', 'presence:prefer', { status: 'ONLINE' })
  })
  await step('presence:idle', async a => {
    await a('B', 'presence:idle', { away: true })
    await a('B', 'presence:idle', { away: false })
  })
  await step('typing (kanal)', async a => {
    await a('A', 'typing', { organizationId: org, channelId: textChannel, typing: true })
    await a('A', 'typing', { organizationId: org, channelId: textChannel, typing: false })
    await a('A', 'typing', { channelId: textChannel })
    await a('A', 'typing', { conversationId: 'yok' })
    await a('C', 'typing', { organizationId: 'yok', channelId: textChannel })
  })
  await step('activity:set', async a => {
    await a('A', 'activity:set', { kind: 'playing', projectId: project, projectName: 'Spike 2', detail: 'Bölüm 1', percent: 42.5, since: 1000 })
    await a('A', 'activity:set', { kind: 'playing', projectId: project, projectName: 'Spike 2', since: 5000 })
    await a('B', 'activity:set', { kind: 'downloading', projectId: 'x'.repeat(70), projectName: 'İndirme', percent: 140, since: '77' })
    await a('B', 'activity:set', { kind: 'uçuyor', projectId: project, projectName: 'x' })
    await a('B', 'activity:set', { kind: 'working', projectId: project })
    await a('C', 'activity:set', { kind: 'working', projectId: project, projectName: 'Spike 2', percent: null })
  })
  await step('activity replay (snapshot) + presence:update etkinlikli', async a => {
    await a('C', 'presence:snapshot', { organizationId: org })
    await a('A', 'presence:idle', { away: true })
    await a('A', 'presence:idle', { away: false })
  })
  await step('activity temizleme', async a => {
    await a('A', 'activity:set', { kind: null })
    await a('B', 'activity:set', {})
    await a('C', 'activity:set', { kind: '' })
  })
  await step('library:announce', async a => {
    await a('A', 'library:announce', { organizationId: org, projectId: project, projectName: 'Spike 2' })
    await a('A', 'library:announce', { organizationId: org })
    await a('B', 'library:announce', { organizationId: org, projectId: 'yok', projectName: 'x' })
  })
  await step('document:cursor', async a => {
    await a('A', 'document:cursor', { organizationId: org, projectId: project, documentId: doc, area: 'CANVAS', x: 12.5, y: 'abc' })
    await a('B', 'document:cursor', { organizationId: org, projectId: project, documentId: doc, area: 'PAGE', x: 1e9, y: -7 })
    await a('A', 'document:cursor', { organizationId: org, projectId: project, documentId: doc, area: 'MAP', x: 1, y: 1 })
    await a('C', 'document:cursor', { organizationId: org, projectId: project, documentId: doc, area: 'CANVAS', x: 1, y: 1 })
  })

  // ---------------------------------------------------------------- bireysel arama
  let cid
  await step('direct: arama başlat (A → B)', async a => { cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B }))?.call?.id })
  await step('direct: kabul', async a => { await a('B', 'call:accept', { callId: cid }) })
  await step('direct: sinyal', async a => {
    await a('A', 'call:signal', { callId: cid, to: ids.B, data: { type: 'offer', sdp: 'v=0' } })
    await a('B', 'call:signal', { callId: cid, to: ids.A })
    await a('A', 'call:signal', { callId: cid, to: ids.C, data: 1 })
    await a('C', 'call:signal', { callId: cid, to: ids.A, data: 1 })
  })
  await step('direct: medya', async a => {
    await a('A', 'call:media', { callId: cid, muted: true })
    await a('B', 'call:media', { callId: cid, video: true })
    await a('A', 'call:media', { callId: cid, screen: true, deafened: true })
    await a('A', 'call:media', { callId: cid, muted: false })
    await a('C', 'call:media', { callId: cid, muted: true })
  })
  await step('direct: moderasyon', async a => {
    await a('A', 'call:moderate', { callId: cid, userId: ids.B, muted: true })
    await a('B', 'call:moderate', { callId: cid, userId: ids.A, muted: true })
    await a('A', 'call:moderate', { callId: cid, userId: ids.A, muted: true })
    await a('A', 'call:moderate', { callId: cid, userId: ids.B, muted: false })
  })
  await step('direct: sync', async a => {
    await a('B', 'call:sync', { callId: cid })
    await a('C', 'call:sync', { callId: cid })
    await a('C', 'call:sync', {})
  })
  const conversation = [...labels.entries()].find(([, v]) => v === 'CONV1')?.[0]
  await step('typing (bireysel sohbet)', async a => {
    await a('A', 'typing', { conversationId: conversation })
    await a('C', 'typing', { conversationId: conversation })
  })
  await step('direct: davet (C) + ayrılma', async a => {
    await a('A', 'call:invite', { callId: cid, userId: ids.C })
    await a('C', 'call:decline', { callId: cid })
    await sleep(1100)
    await a('A', 'call:leave', { callId: cid })
  })
  await step('direct: reddetme', async a => {
    cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B }))?.call?.id
    await a('B', 'call:decline', { callId: cid })
    await a('B', 'call:accept', { callId: cid })
  })
  await step('direct: geri arama aynı aramada buluşur', async a => {
    cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B }))?.call?.id
    await a('B', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.A })
    await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B })
    await a('B', 'call:leave', { callId: cid })
  })
  await step('direct: görünmez kişiye arama (reject)', async a => {
    await a('C', 'presence:prefer', { status: 'INVISIBLE' })
    cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.C }))?.call?.id
    await a('B', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.C })
    await a('A', 'call:leave', { callId: cid })
    await a('B', 'call:leave', { callId: [...labels.entries()].find(([, v]) => v === 'CALL' + callNo)?.[0] })
    await a('C', 'presence:prefer', { status: 'BUSY' })
  })
  await step('direct: meşgul kişiye sessiz arama', async a => {
    cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.C }))?.call?.id
    await a('C', 'call:accept', { callId: cid })
    await a('C', 'presence:prefer', { status: 'ONLINE' })
    await a('C', 'call:leave', { callId: cid })
  })

  // ---------------------------------------------------------------- kanal ve grup aramaları
  let channelCall, groupCall
  await step('kanal: zil çalarak başlat', async a => { channelCall = (await a('A', 'call:start', { organizationId: org, kind: 'channel', channelId: channel, ring: true }))?.call?.id }, 900)
  await step('kanal: kabul', async a => { await a('B', 'call:accept', { callId: channelCall }) })
  await step('kanal: replay (subscribe + snapshot)', async a => {
    await a('C', 'subscribe', { organizationId: org })
    await a('A', 'presence:snapshot', { organizationId: org })
  })
  await step('kanal: davet + medya izinleri', async a => {
    await a('A', 'call:invite', { callId: channelCall, userId: ids.C })
    await a('B', 'call:invite', { callId: channelCall, userId: ids.A })
    await a('B', 'call:media', { callId: channelCall, screen: true })
    await a('B', 'call:media', { callId: channelCall, muted: true })
    await a('B', 'call:media', { callId: channelCall, muted: false })
  })
  await step('grup: B grup araması başlatır (kanaldan geçiş)', async a => { groupCall = (await a('B', 'call:start', { organizationId: org, kind: 'group', groupId: group }))?.call?.id }, 900)
  await step('grup: A kabul (kanal araması biter)', async a => { await a('A', 'call:accept', { callId: groupCall }) }, 900)
  await step('grup: ayrılma', async a => {
    await a('B', 'call:leave', { callId: groupCall })
    await a('A', 'call:leave', { callId: groupCall })
  })
  await step('grup: ring=false ile yeniden başlat', async a => {
    groupCall = (await a('A', 'call:start', { organizationId: org, kind: 'group', groupId: group, ring: false }))?.call?.id
    await a('B', 'call:start', { organizationId: org, kind: 'group', groupId: group })
    await a('C', 'call:start', { organizationId: org, kind: 'group', groupId: group })
  }, 900)
  await step('grup: herkes ayrılır', async a => {
    await a('A', 'call:leave', { callId: groupCall })
    await a('B', 'call:leave', { callId: groupCall })
  })
  await step('kanal: var olan grup (group:updated) + sessiz katılım', async a => {
    channelCall = (await a('A', 'call:start', { organizationId: org, kind: 'channel', channelId: channel, ring: true }))?.call?.id
    await a('B', 'call:start', { organizationId: org, kind: 'channel', channelId: channel })
  }, 900)
  await step('kanal: herkes ayrılır', async a => {
    await a('B', 'call:leave', { callId: channelCall })
    await a('A', 'call:leave', { callId: channelCall })
  })

  // ---------------------------------------------------------------- hatalı istekler
  await step('hatalı istekler', async a => {
    await a('A', 'call:start', {})
    await a('A', 'call:start', { organizationId: 'yok', kind: 'direct', targetUserId: ids.B })
    await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.A })
    await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: 'yok' })
    await a('A', 'call:start', { organizationId: org, kind: 'channel', channelId: 'yok' })
    await a('A', 'call:start', { organizationId: org, kind: 'channel', channelId: textChannel.replace(/.$/, '0') })
    await a('A', 'call:start', { organizationId: org, kind: 'group', groupId: 'yok' })
    await a('C', 'call:start', { organizationId: org, kind: 'group', groupId: group })
    for (const event of ['call:invite', 'call:accept', 'call:decline', 'call:leave', 'call:media', 'call:moderate', 'call:signal']) await a('A', event, { callId: 'yok', userId: ids.B })
  })

  // ---------------------------------------------------------------- kopma bekleme süresi
  await step('grace: arama + kısa kopma (3 sn içinde geri döner)', async a => {
    cid = (await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B }))?.call?.id
    await a('B', 'call:accept', { callId: cid })
    clients.B.s.disconnect()
    await sleep(1000)
    clients.B = await connect(base, tok.B.accessToken)
    await a('B', 'subscribe', { organizationId: org })
    await sleep(2600)
    await a('B', 'call:sync', { callId: cid })
  })
  await step('grace: uzun kopma aramadan düşürür', async a => {
    clients.B.s.disconnect()
    await sleep(3600)
    clients.B = await connect(base, tok.B.accessToken)
    await a('B', 'call:sync', { callId: cid })
  })
  if (process.env.FULL) {
    await step('direct: 45 sn cevapsız', async a => {
      await a('A', 'call:start', { organizationId: org, kind: 'direct', targetUserId: ids.B })
      await sleep(45500)
    })
  }
  await step('kapanış', async () => {
    for (const client of Object.values(clients)) client.s.disconnect()
  })

  // Temizlik: oluşturulan kanal (yumuşak silme), gruplar, doküman ve arama kayıtları.
  await call(NEST, 'DELETE', `/organizations/${org}/projects/${project}/channels/${channel}`, undefined, tok.A.accessToken)
  await db.query('delete from "ChatGroup" where id = any($1) or "channelId" = $2', [created.groups, channel])
  await db.query('delete from "ProjectDocument" where id = $1', [doc])
  await db.query('delete from "DirectMessage" where id = any($1)', [created.messages])
  await db.query(`update "User" set "statusPreference" = 'ONLINE' where id = any($1)`, [Object.values(ids)])
  return { results, created }
}

// Hata ayıklama: iki değerin ilk farklı olduğu yol.
function firstDiff(x, y, path = '') {
  if (JSON.stringify(x) === JSON.stringify(y)) return null
  if (x && y && typeof x === 'object' && typeof y === 'object') {
    const keys = [...new Set([...Object.keys(x), ...Object.keys(y)])]
    for (const k of keys) { const d = firstDiff(x[k], y[k], path + '.' + k); if (d) return d }
    return path + ' (anahtar sırası) ' + JSON.stringify(Object.keys(x)) + ' / ' + JSON.stringify(Object.keys(y))
  }
  return path + ': ' + JSON.stringify(x)?.slice(0, 300) + ' ≠ ' + JSON.stringify(y)?.slice(0, 300)
}

console.log('NestJS senaryosu…')
const nest = await scenario(NEST, 'n')
await sleep(4000)
const rust = await startRust(4125)
console.log('Rust senaryosu…')
const rs = await scenario(rust.base, 'r')
rust.stop()
for (let i = 0; i < nest.results.length; i++) {
  const [name, a] = nest.results[i]
  const b = rs.results[i]?.[1]
  if (!same(name, a, b)) console.log('      ilk fark: ' + firstDiff(mask(a), mask(b)))
}
check('arama kayıtları yazıldı (NestJS/Rust)', nest.created.messages.length === rs.created.messages.length && rs.created.messages.length > 0, `${nest.created.messages.length}/${rs.created.messages.length}`)
await db.end()
done()
