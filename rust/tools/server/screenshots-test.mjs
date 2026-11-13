// Ekran görüntüsü galerisi: yükleme meta denetimi (sahip/tür), boyut/yol doğrulaması, başlık ve
// çekim zamanı kuralları, sahip/yönetici düzenleme-silme, olaylar. Ortak projede (spike-2) çalışır.
// Yüklemeler NestJS'in /uploads ucuyla yapılır; Rust sunucusu aynı yükleme dizinini okur.
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

process.env.P2_UPLOAD_DIR = 'C:/Users/capta/OneDrive/Desktop/P2/.p2-data/uploads'
const rust = await startRust(4122)
const O = (await login(NEST, 'p2test')).accessToken
const M = (await login(NEST, 'rustbot')).accessToken
const me = (await call(NEST, 'GET', '/auth/me', undefined, O)).body.id
const org = (await call(NEST, 'GET', '/organizations', undefined, O)).body[0].id
const spike = (await call(NEST, 'GET', `/organizations/${org}/projects`, undefined, M)).body[0].id
const rnd = Math.random().toString(36).slice(2, 8)
const tags = { nest: 'n' + rnd, rust: 'r' + rnd }
const G = `/organizations/${org}/projects/${spike}/screenshots`
const upload = async (token, name, type, data) => (await call(NEST, 'POST', '/uploads', { name, type, data }, token)).body.url
const JPEG = 'data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP////8='
const PDF = 'data:application/pdf;base64,' + Buffer.from('%PDF-1.4 test').toString('base64')

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
  async function listStep(name, query = '', token = O) {
    const res = await call(base, 'GET', G + query, undefined, token)
    out.push([name, { status: res.status, body: Array.isArray(res.body) ? res.body.filter(n => ids.has(n.id)) : res.body }, {}])
  }
  const jpg = await upload(O, 'a.jpg', 'image/jpeg', JPEG)
  const jpg2 = await upload(O, 'b.jpeg', 'image/jpeg', JPEG)
  const pdf = await upload(O, 'c.png', 'image/png', PDF)
  const other = await upload(M, 'd.jpg', 'image/jpeg', JPEG)
  // Yükleme yolları her çalıştırmada farklı: cevaplarda maskelenir.
  const paths = [[jpg, '<jpg>'], [jpg2, '<jpg2>'], [other, '<other>']]
  const now = Date.now()
  await step('doğrulama {}', 'POST', G, {})
  await step('doğrulama genişlik dizgi', 'POST', G, { uploadPath: jpg, width: '1920', height: 1080 })
  await step('doğrulama boyut negatif', 'POST', G, { uploadPath: jpg, width: 1920, height: 1080, size: -1 })
  await step('doğrulama başlık', 'POST', G, { uploadPath: jpg, width: 1920, height: 1080, caption: 5 })
  await step('yol geçersiz', 'POST', G, { uploadPath: '/uploads/x.gif', width: 1, height: 1 })
  await step('boyut büyük', 'POST', G, { uploadPath: jpg, width: 8000, height: 1080 })
  await step('meta yok', 'POST', G, { uploadPath: '/uploads/3f2a1c9e-1b2c-4d5e-8f90-123456789abc.jpg', width: 1, height: 1 })
  await step('görsel değil', 'POST', G, { uploadPath: pdf, width: 1, height: 1 })
  await step('başkasının yüklemesi', 'POST', G, { uploadPath: other, width: 1, height: 1 })
  const a = (await step('oluştur', 'POST', G, { uploadPath: jpg, width: 1920.0, height: 1080, size: 5, caption: '  Boss \n kapısı ' + tag, releaseId: 'yok', takenAt: new Date(now - 60_000).toISOString() })).body
  await step('oluştur (gelecek tarih, boş başlık)', 'POST', G, { uploadPath: jpg2, width: 7680, height: 4320, caption: '   ', takenAt: new Date(now + 3_600_000).toISOString() })
  const b = (await step('üye oluşturur', 'POST', G, { uploadPath: other, width: 640, height: 480, caption: 'üye ' + tag }, M)).body
  await listStep('liste')
  await listStep('liste kişi', `?userId=${me}`)
  await listStep('liste limit=1', '?limit=1')
  await listStep('liste before', `?before=${encodeURIComponent(new Date(now).toISOString())}`)
  if (cross) same('çapraz okuma: liste', await call(NEST, 'GET', G + '?limit=200', undefined, O), await call(base, 'GET', G + '?limit=200', undefined, O), paths)
  await step('başlık güncelle', 'PATCH', `${G}/${a.id}`, { caption: ' yeni\tbaşlık ' })
  await step('başlık null', 'PATCH', `${G}/${a.id}`, { caption: null })
  await step('başlık yok (değişmez)', 'PATCH', `${G}/${b.id}`, {})
  await step('yönetici üyeninkini düzenler', 'PATCH', `${G}/${b.id}`, { caption: 'sahip ' + tag })
  await step('güncelle yok', 'PATCH', `${G}/yok`, { caption: 'a' })
  await step('güncelle doğrulama', 'PATCH', `${G}/${a.id}`, { caption: 'x'.repeat(501) })
  await step('üye sahibinkini siler (yönetici)', 'DELETE', `${G}/${a.id}`, undefined, M)
  await step('sil yok', 'DELETE', `${G}/${a.id}`)
  for (const id of ids) if (id !== tag && id !== a.id) await call(base, 'DELETE', `${G}/${id}`, undefined, O)
  so.close(); sm.close()
  return { out, paths }
}

await both('proje yok', rust.base, 'GET', `/organizations/${org}/projects/yok/screenshots`, undefined, { nest: O })
await both('jetonsuz', rust.base, 'GET', G)
await both('bilinmeyen yöntem', rust.base, 'PUT', G, {}, { nest: O })
const nest = await scenario(NEST, tags.nest, false)
const mine = await scenario(rust.base, tags.rust, true)
for (let i = 0; i < Math.max(nest.out.length, mine.out.length); i++) {
  const [name, a, ea] = nest.out[i] || ['?', null, null]
  const [, b, eb] = mine.out[i] || ['?', null, null]
  const ra = [[tags.nest, '<tag>'], ...nest.paths], rb = [[tags.rust, '<tag>'], ...mine.paths]
  same(name, maskWith(a, ra), maskWith(b, rb))
  same(name + ' (olaylar)', maskWith(ea, ra), maskWith(eb, rb))
}
for (const n of ['oluştur', 'üye oluşturur']) check(n + ' 201 (rust)', mine.out.find(o => o[0] === n)?.[1].status === 201)
check('olay geldi (screenshot:added)', mine.out.find(o => o[0] === 'oluştur')?.[2].p2test.length === 1)
check('adım sayısı', nest.out.length === mine.out.length, `${nest.out.length} / ${mine.out.length}`)
rust.stop()
done()

function maskWith(value, replace) {
  return JSON.parse(replace.reduce((text, [from, to]) => text.split(from).join(to), JSON.stringify(value ?? null)))
}
