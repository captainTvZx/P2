// Kütüphane vitrini (hub): gönderiler, başarımlar, kazanımlar, incelemeler, oynama süresi.
// Her sunucu kendi taze projesinde aynı senaryoyu çalıştırır; cevaplar ve olaylar karşılaştırılır.
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

async function scenario(base, tag, cross) {
  const out = []
  const so = await listen(base, O, org)
  const ids = new Set([tag])
  const own = list => list.filter(([, p]) => [...ids].some(id => JSON.stringify(p).includes(id))).map(e => [JSON.stringify(mask(e)), e]).sort((x, y) => x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0).map(x => x[1])
  async function step(name, method, path, body, token = O) {
    const res = await call(base, method, path, body, token)
    if (method !== 'GET') await sleep(150)
    out.push([name, res, own(so.take())])
    return res
  }
  async function crossRead(name, path, token = O) {
    if (!cross) return
    same('çapraz okuma: ' + name, await call(NEST, 'GET', path, undefined, token), await call(base, 'GET', path, undefined, token))
  }
  const proj = (await call(base, 'POST', `/organizations/${org}/projects`, { name: 'Hub ' + tag, slug: 'hub-' + tag }, O)).body
  ids.add(proj.id)
  const H = `/organizations/${org}/projects/${proj.id}/hub`
  await step('vitrin boş', 'GET', H)
  await step('vitrin üye (erişim yok)', 'GET', H, undefined, M)
  await step('vitrin proje yok', 'GET', `/organizations/${org}/projects/yok/hub`)
  await step('gönderi doğrulama', 'POST', `${H}/posts`, {})
  const post = (await step('gönderi', 'POST', `${H}/posts`, { content: ' merhaba ' })).body
  await step('gönderi düzenle', 'PATCH', `${H}/posts/${post.id}`, { content: ' yeni ' })
  await step('gönderi düzenle yok', 'PATCH', `${H}/posts/yok`, { content: 'a' })
  await step('gönderi düzenle doğrulama', 'PATCH', `${H}/posts/${post.id}`, { content: 5 })
  await step('başarım adsız', 'POST', `${H}/achievements`, {})
  await step('başarım boş simge', 'POST', `${H}/achievements`, { title: 'x', icon: '' })
  const ach = (await step('başarım', 'POST', `${H}/achievements`, { title: ' İlk ', description: ' d ', icon: ' ⭐ ' })).body
  await step('başarım varsayılan simge', 'POST', `${H}/achievements`, { title: 'İkinci', description: null, icon: null })
  await step('başarım güncelle', 'PATCH', `${H}/achievements/${ach.id}`, { title: '  ', description: '' })
  await step('başarım güncelle ad', 'PATCH', `${H}/achievements/${ach.id}`, { title: 'Yeni ad', icon: '🎯' })
  await step('başarım güncelle null', 'PATCH', `${H}/achievements/${ach.id}`, { title: null })
  await step('başarım güncelle yok', 'PATCH', `${H}/achievements/yok`, { title: 'a' })
  await step('kazanım doğrulama', 'POST', `${H}/achievements/${ach.id}/unlock`, {})
  await step('kazanım', 'POST', `${H}/achievements/${ach.id}/unlock`, { unlocked: true })
  await step('kazanım tekrar', 'POST', `${H}/achievements/${ach.id}/unlock`, { unlocked: true })
  await step('kazanım yok', 'POST', `${H}/achievements/yok/unlock`, { unlocked: true })
  await step('vitrin (kazanımlı)', 'GET', H)
  await crossRead('vitrin', H)
  await step('başarım güncelle (kazanımlı)', 'PATCH', `${H}/achievements/${ach.id}`, { description: 'açıklama' })
  await step('kazanım kaldır', 'POST', `${H}/achievements/${ach.id}/unlock`, { unlocked: false })
  await step('inceleme doğrulama', 'PUT', `${H}/reviews`, { recommended: 'evet' })
  await step('inceleme', 'PUT', `${H}/reviews`, { recommended: true, body: ' iyi ' })
  await step('inceleme güncelle', 'PUT', `${H}/reviews`, { recommended: false })
  await step('vitrin (incelemeli)', 'GET', H)
  await crossRead('vitrin (incelemeli)', H)
  await step('inceleme sil', 'DELETE', `${H}/reviews`)
  await step('başarım sil', 'DELETE', `${H}/achievements/${ach.id}`)
  await step('başarım sil yok', 'DELETE', `${H}/achievements/${ach.id}`)
  await step('gönderi sil', 'DELETE', `${H}/posts/${post.id}`)
  await step('gönderi sil yok', 'DELETE', `${H}/posts/${post.id}`)
  await step('vitrin son', 'GET', H)
  await step('bilinmeyen yöntem', 'PATCH', `/organizations/${org}/projects/${spike}/hub`, {})
  await call(base, 'DELETE', `/organizations/${org}/projects/${proj.id}`, undefined, O)

  // Ortak projede üye: gönderi yazıp siler (rustbot channel.manage taşıyor).
  const S = `/organizations/${org}/projects/${spike}/hub`
  const mp = (await step('S üye gönderi', 'POST', `${S}/posts`, { content: 'üye ' + tag }, M)).body
  ids.add(mp.id)
  await step('S sahip üyenin gönderisini düzenler', 'PATCH', `${S}/posts/${mp.id}`, { content: 'sahip ' + tag })
  await step('S üye siler', 'DELETE', `${S}/posts/${mp.id}`, undefined, M)
  so.close()
  return out
}

await both('S vitrin (sahip)', rust.base, 'GET', `/organizations/${org}/projects/${spike}/hub`, undefined, { nest: O })
await both('S vitrin (üye)', rust.base, 'GET', `/organizations/${org}/projects/${spike}/hub`, undefined, { nest: M })
await both('jetonsuz', rust.base, 'GET', `/organizations/${org}/projects/${spike}/hub`)
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
