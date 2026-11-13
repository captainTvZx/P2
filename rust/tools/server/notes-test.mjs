// Proje notları: PRIVATE/TEAM görünürlük, iyimser kilit (version), yönetici düzenlemesi, olay kitlesi.
// Ortak projede (spike-2) sahip ve üye ile, her sunucuda taze notlarla aynı senaryo çalışır.
import { createRequire } from 'node:module'
import { NEST, startRust, call, login, both, same, check, sleep, done, mask } from './lib.mjs'

const require = createRequire('C:/Users/capta/OneDrive/Desktop/P2/package.json')
const { io } = require('socket.io-client')
async function listen(base, token, organizationId) {
  const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
  const events = []
  socket.onAny((event, payload) => { if (!event.startsWith('presence:')) events.push([event, payload]) })
  await new Promise((resolve, reject) => { socket.on('connect', resolve); socket.on('connect_error', reject) })
  await socket.timeout(5000).emitWithAck('subscribe', { organizationId })
  return { take: () => events.splice(0), close: () => socket.close() }
}

const rust = await startRust(4122)
const O = (await login(NEST, 'p2test')).accessToken
const M = (await login(NEST, 'rustbot')).accessToken
const org = (await call(NEST, 'GET', '/organizations', undefined, O)).body[0].id
const spike = (await call(NEST, 'GET', `/organizations/${org}/projects`, undefined, M)).body[0].id
const rnd = Math.random().toString(36).slice(2, 8)
const tags = { nest: 'n' + rnd, rust: 'r' + rnd }
const N = `/organizations/${org}/projects/${spike}/notes`

async function scenario(base, tag, cross) {
  const out = []
  const so = await listen(base, O, org), sm = await listen(base, M, org)
  const ids = new Set([tag])
  const own = list => list.filter(([, p]) => [...ids].some(id => JSON.stringify(p).includes(id))).map(e => [JSON.stringify(mask(e)), e]).sort((x, y) => x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0).map(x => x[1])
  async function step(name, method, path, body, token = O) {
    const res = await call(base, method, path, body, token)
    if (method !== 'GET') await sleep(150)
    if (method === 'POST' && res.body?.id) ids.add(res.body.id)
    out.push([name, res, { p2test: own(so.take()), rustbot: own(sm.take()) }])
    return res
  }
  // Liste: yalnız bu senaryonun notları (ortak projede başka notlar da olabilir).
  async function listStep(name, query = '', token = O) {
    const res = await call(base, 'GET', N + query, undefined, token)
    out.push([name, { status: res.status, body: Array.isArray(res.body) ? res.body.filter(n => ids.has(n.id)) : res.body }, {}])
    return res
  }
  await step('doğrulama görünürlük', 'POST', N, { visibility: 'HERKES' })
  await step('doğrulama sabit', 'POST', N, { pinned: 'evet' })
  await step('doğrulama başlık', 'POST', N, { title: 5 })
  const priv = (await step('özel not', 'POST', N, { title: 'Özel ' + tag, body: 'gizli' })).body
  const team = (await step('ekip notu', 'POST', N, { title: 'Ekip ' + tag, body: 'herkes', visibility: 'TEAM', pinned: true })).body
  await step('boş not', 'POST', N, {})
  await step('üye notu', 'POST', N, { title: 'Üye ' + tag, visibility: 'TEAM' }, M)
  await listStep('liste (sahip)')
  await listStep('liste (üye)', '', M)
  await listStep('liste limit=1', '?limit=1')
  await listStep('liste before', `?before=${encodeURIComponent(team.updatedAt)}`)
  if (cross) same('çapraz okuma: liste', await call(NEST, 'GET', N + '?limit=500', undefined, O), await call(base, 'GET', N + '?limit=500', undefined, O))
  await step('üye özel notu düzenleyemez', 'PATCH', `${N}/${priv.id}`, { title: 'x' }, M)
  await step('üye ekip notunu düzenler', 'PATCH', `${N}/${team.id}`, { body: 'üye düzenledi', version: 1 }, M)
  await step('eski sürüm çakışması', 'PATCH', `${N}/${team.id}`, { body: 'x', version: 1 })
  await step('sürüm doğrulama 0', 'PATCH', `${N}/${team.id}`, { version: 0 })
  await step('sürüm doğrulama 1.5', 'PATCH', `${N}/${team.id}`, { version: 1.5 })
  await step('başlık null', 'PATCH', `${N}/${team.id}`, { title: null })
  await step('gövde null', 'PATCH', `${N}/${team.id}`, { body: null })
  await step('görünürlük null (değişmez)', 'PATCH', `${N}/${team.id}`, { visibility: null, pinned: null })
  await step('ekip → özel', 'PATCH', `${N}/${team.id}`, { visibility: 'PRIVATE', title: 'x'.repeat(10) + tag })
  await step('üye artık göremez', 'PATCH', `${N}/${team.id}`, { title: 'y' }, M)
  await step('özel → ekip', 'PATCH', `${N}/${team.id}`, { visibility: 'TEAM', version: 4 })
  await step('not yok', 'PATCH', `${N}/yok`, { title: 'a' })
  await step('üye özel notu silemez', 'DELETE', `${N}/${priv.id}`, undefined, M)
  await step('özel not sil', 'DELETE', `${N}/${priv.id}`)
  await step('ekip notunu üye siler', 'DELETE', `${N}/${team.id}`, undefined, M)
  await step('not sil yok', 'DELETE', `${N}/${team.id}`)
  // Kalan notları temizle.
  for (const n of ((await call(base, 'GET', N + '?limit=500', undefined, O)).body || []).filter(n => ids.has(n.id))) await call(base, 'DELETE', `${N}/${n.id}`, undefined, n.authorId === (await call(base, 'GET', '/auth/me', undefined, M)).body.id ? M : O)
  so.close(); sm.close()
  return out
}

await both('proje yok', rust.base, 'GET', `/organizations/${org}/projects/yok/notes`, undefined, { nest: O })
await both('jetonsuz', rust.base, 'GET', N)
await both('yabancı kuruluş', rust.base, 'GET', `/organizations/yok/projects/${spike}/notes`, undefined, { nest: O })
const nest = await scenario(NEST, tags.nest, false)
const mine = await scenario(rust.base, tags.rust, true)
const replace = [[tags.nest, '<tag>'], [tags.rust, '<tag>']]
for (let i = 0; i < Math.max(nest.length, mine.length); i++) {
  const [name, a, ea] = nest[i] || ['?', null, null]
  const [, b, eb] = mine[i] || ['?', null, null]
  same(name, a, b, replace)
  same(name + ' (olaylar)', ea, eb, replace)
}
check('adım sayısı', nest.length === mine.length, `${nest.length} / ${mine.length}`)
rust.stop()
done()
