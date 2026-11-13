// Faz 0 / Spike 4 sözleşme testi: NestJS (4110) ile Rust (4120) aynı isteklere aynı cevabı veriyor mu?
// Kullanım: node contract.mjs <P2 repo yolu>
import { createRequire } from 'node:module'
import { createHmac, randomBytes } from 'node:crypto'
import { isDeepStrictEqual } from 'node:util'

const repo = process.argv[2]
const require = createRequire(repo + '/package.json')
const { io } = require('socket.io-client')

const NEST = 'http://127.0.0.1:4110', RUST = 'http://127.0.0.1:4120'
const tag = randomBytes(3).toString('hex')
let pass = 0, fail = 0
const ok = (name) => { pass++; console.log('  ✔ ' + name) }
const bad = (name, detail) => { fail++; console.log('  ✘ ' + name + '\n      ' + detail) }

async function call(base, method, path, body, token, extra = {}) {
  const headers = { 'content-type': 'application/json', 'x-p2-client-version': '0.9.9', ...extra }
  if (token) headers.authorization = 'Bearer ' + token
  const res = await fetch(base + path, { method, headers, body: body === undefined ? undefined : (typeof body === 'string' ? body : JSON.stringify(body)) })
  const text = await res.text()
  let json; try { json = JSON.parse(text) } catch { json = text }
  return { status: res.status, body: json }
}

// Değişken alanları maskele: kimlikler, jetonlar, gizli anahtarlar, kullanıcıya özgü değerler.
function mask(value, who) {
  if (Array.isArray(value)) return value.map(v => mask(v, who))
  if (value && typeof value === 'object') {
    const out = {}
    for (const [k, v] of Object.entries(value)) {
      if (['id', 'accessToken', 'refreshToken', 'secret'].includes(k) && typeof v === 'string') out[k] = '<' + k + '>'
      else if (k === 'otpauth') out[k] = String(v).replace(/secret=[A-Z2-7]+/, 'secret=<s>').replace(who, '<u>')
      // Kabul edilen sapma: bozuk JSON'da ayrıştırıcının ham metni (V8 vs serde). Biçim/durum aynı olmalı.
      else if (k === 'message' && typeof v === 'string' && value.error === 'Bad Request' && value.statusCode === 400) out[k] = '<ayrıştırıcı mesajı>'
      else out[k] = mask(v, who)
    }
    return out
  }
  return typeof value === 'string' ? value.split(who).join('<u>') : value
}

// totp.ts codeFor
const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
function totp(secret, offset = 0) {
  let bits = ''; for (const c of secret) bits += ALPHABET.indexOf(c).toString(2).padStart(5, '0')
  const bytes = []; for (let i = 0; i + 8 <= bits.length; i += 8) bytes.push(parseInt(bits.slice(i, i + 8), 2))
  const counter = Math.floor(Date.now() / 30000) + offset, buf = Buffer.alloc(8)
  buf.writeUInt32BE(Math.floor(counter / 2 ** 32), 0); buf.writeUInt32BE(counter >>> 0, 4)
  const d = createHmac('sha1', Buffer.from(bytes)).update(buf).digest(), o = d[d.length - 1] & 15
  return String((((d[o] & 127) << 24) | (d[o + 1] << 16) | (d[o + 2] << 8) | d[o + 3]) % 1e6).padStart(6, '0')
}

// ---------------------------------------------------------------- A) aynı senaryo, iki sunucu
async function scenario(base, who) {
  const u = who, email = `${u}@test.dev`, pw = 'Parola123'
  const r = []
  const step = async (name, ...a) => { const res = await call(base, ...a); r.push([name, res]); return res }
  await step('health', 'GET', '/health')
  await step('setup durumu', 'GET', '/setup/status')
  await step('kayıt: boş gövde', 'POST', '/auth/register', {})
  await step('kayıt: bozuk JSON', 'POST', '/auth/register', '{bozuk')
  await step('kayıt: hepsi hatalı', 'POST', '/auth/register', { email: 'x', username: 'a b', displayName: 'Z', password: 'kisa', passwordConfirm: 'kisa' })
  await step('kayıt: kısa kullanıcı adı', 'POST', '/auth/register', { email, username: 'ab', displayName: 'Test', password: pw, passwordConfirm: pw })
  await step('kayıt: zayıf parola', 'POST', '/auth/register', { email, username: u, displayName: 'Test', password: 'kucukharf1', passwordConfirm: 'kucukharf1' })
  await step('kayıt: eşleşmeyen parola', 'POST', '/auth/register', { email, username: u, displayName: 'Test', password: pw, passwordConfirm: pw + 'x' })
  await step('kayıt: geçersiz e-posta', 'POST', '/auth/register', { email: 'a@b', username: u, displayName: 'Test', password: pw, passwordConfirm: pw })
  const reg = await step('kayıt: geçerli', 'POST', '/auth/register', { email, username: u, displayName: '  Test Kişi  ', password: pw, passwordConfirm: pw, fazladan: 1 })
  await step('kayıt: aynı e-posta', 'POST', '/auth/register', { email: email.toUpperCase(), username: u + 'x', displayName: 'Test', password: pw, passwordConfirm: pw })
  await step('kayıt: aynı kullanıcı adı (büyük harf)', 'POST', '/auth/register', { email: 'b' + email, username: u.toUpperCase(), displayName: 'Test', password: pw, passwordConfirm: pw })
  await step('giriş: yanlış parola', 'POST', '/auth/login', { identifier: u, password: 'Yanlis123' })
  await step('giriş: olmayan kullanıcı', 'POST', '/auth/login', { identifier: 'yok' + u, password: pw })
  await step('giriş: rememberMe tipi yanlış', 'POST', '/auth/login', { identifier: u, password: pw, rememberMe: 'evet' })
  await step('giriş: alan yok', 'POST', '/auth/login', { password: pw })
  await step('giriş: e-posta büyük harf', 'POST', '/auth/login', { identifier: '  ' + email.toUpperCase() + ' ', password: pw })
  const login = await step('giriş: kullanıcı adı, hatırlama yok', 'POST', '/auth/login', { identifier: u, password: pw, rememberMe: false })
  const access = login.body.accessToken, refresh = login.body.refreshToken
  await step('me: jetonsuz', 'GET', '/auth/me')
  await step('me: bozuk jeton', 'GET', '/auth/me', undefined, 'abc.def.ghi')
  await step('me: refresh jetonuyla', 'GET', '/auth/me', undefined, refresh)
  await step('me', 'GET', '/auth/me', undefined, access)
  await step('yenile: eksik', 'POST', '/auth/refresh', {})
  await step('yenile: çöp', 'POST', '/auth/refresh', { refreshToken: 'çöp' })
  await step('yenile: erişim jetonuyla', 'POST', '/auth/refresh', { refreshToken: access })
  const ref = await step('yenile: geçerli', 'POST', '/auth/refresh', { refreshToken: refresh })
  r.push(['yenile: aynı refresh jetonu döner', { same: ref.body.refreshToken === refresh }])
  await step('2fa kurulum: jetonsuz', 'POST', '/auth/2fa/setup')
  await step('2fa kurulum', 'POST', '/auth/2fa/setup', undefined, access)
  const setup = await step('2fa kurulum (tekrar)', 'POST', '/auth/2fa/setup', undefined, access)
  await step('2fa aç: 5 hane', 'POST', '/auth/2fa/enable', { code: '12345' }, access)
  await step('2fa aç: yanlış kod', 'POST', '/auth/2fa/enable', { code: totp(setup.body.secret, 5) }, access)
  await step('2fa aç: doğru kod', 'POST', '/auth/2fa/enable', { code: totp(setup.body.secret) }, access)
  await step('2fa kurulum: zaten açık', 'POST', '/auth/2fa/setup', undefined, access)
  await step('giriş: 2fa kodu yok', 'POST', '/auth/login', { identifier: u, password: pw })
  await step('giriş: 2fa kodu yanlış', 'POST', '/auth/login', { identifier: u, password: pw, code: totp(setup.body.secret, 5) })
  await step('giriş: 2fa kodu doğru', 'POST', '/auth/login', { identifier: u, password: pw, code: totp(setup.body.secret) })
  await step('2fa kapat: yanlış kod', 'POST', '/auth/2fa/disable', { code: totp(setup.body.secret, 5) }, access)
  await step('2fa kapat: doğru kod', 'POST', '/auth/2fa/disable', { code: totp(setup.body.secret) }, access)
  await step('2fa kapat: zaten kapalı', 'POST', '/auth/2fa/disable', { code: '000000' }, access)
  await step('profil: kısa ad', 'PATCH', '/auth/profile', { displayName: 'x' }, access)
  await step('profil: unvan sayı', 'PATCH', '/auth/profile', { title: 5 }, access)
  await step('profil: güncelle', 'PATCH', '/auth/profile', { displayName: ' Yeni Ad ', title: '   ', avatarUrl: '' }, access)
  await step('profil: unvan ver', 'PATCH', '/auth/profile', { title: 'Oyun geliştiricisi' }, access)
  await step('çıkış', 'POST', '/auth/logout', { refreshToken: refresh })
  await step('çıkış: gövdesiz', 'POST', '/auth/logout')
  await step('yenile: çıkıştan sonra', 'POST', '/auth/refresh', { refreshToken: refresh })
  await step('bilinmeyen yol', 'GET', '/yok/boyle-bir-yol')
  return { steps: r, user: u, email, pw, access: (await call(base, 'POST', '/auth/login', { identifier: u, password: pw })).body }
}

console.log('A) Aynı senaryo, iki sunucu (değişken alanlar maskelendi)')
const nest = await scenario(NEST, 'n' + tag)
const rust = await scenario(RUST, 'r' + tag)
for (let i = 0; i < nest.steps.length; i++) {
  const [name, a] = nest.steps[i], [, b] = rust.steps[i]
  const ma = mask(a, nest.user), mb = mask(b, rust.user)
  if (isDeepStrictEqual(ma, mb)) ok(name + (a.status ? ` (${a.status})` : ''))
  else bad(name, 'NestJS: ' + JSON.stringify(ma) + '\n      Rust:   ' + JSON.stringify(mb))
}

// ---------------------------------------------------------------- B) çapraz uyumluluk
console.log('\nB) Çapraz uyumluluk')
const x = async (name, res, status) => res.status === status ? ok(`${name} (${res.status})`) : bad(name, JSON.stringify(res))
await x('Rust\'ın argon2 özetiyle NestJS\'te giriş', await call(NEST, 'POST', '/auth/login', { identifier: rust.user, password: rust.pw }), 201)
await x('NestJS\'in argon2 özetiyle Rust\'ta giriş', await call(RUST, 'POST', '/auth/login', { identifier: nest.user, password: nest.pw }), 201)
await x('NestJS erişim jetonu Rust\'ta /auth/me', await call(RUST, 'GET', '/auth/me', undefined, nest.access.accessToken), 200)
await x('Rust erişim jetonu NestJS\'te /auth/me', await call(NEST, 'GET', '/auth/me', undefined, rust.access.accessToken), 200)
await x('Rust refresh jetonu NestJS\'te yenileniyor', await call(NEST, 'POST', '/auth/refresh', { refreshToken: rust.access.refreshToken }), 201)
await x('NestJS refresh jetonu Rust\'ta yenileniyor', await call(RUST, 'POST', '/auth/refresh', { refreshToken: nest.access.refreshToken }), 201)
const s2 = await call(RUST, 'POST', '/auth/2fa/setup', undefined, rust.access.accessToken)
await x('2FA: Rust\'ta kurulup NestJS\'te açılıyor', await call(NEST, 'POST', '/auth/2fa/enable', { code: totp(s2.body.secret) }, rust.access.accessToken), 201)
await x('2FA: NestJS\'te açılmış hesaba Rust\'ta kodla giriş', await call(RUST, 'POST', '/auth/login', { identifier: rust.user, password: rust.pw, code: totp(s2.body.secret) }), 201)

// ---------------------------------------------------------------- C) socket.io /stream
console.log('\nC) socket.io /stream (P2 istemcisinin kullandığı socket.io-client ile)')
const org = await call(NEST, 'POST', '/organizations', { name: 'Spike ' + tag, slug: 'spike-' + tag }, nest.access.accessToken)
const orgId = org.body?.id ?? org.body?.organization?.id
if (!orgId) bad('kuruluş oluşturma (NestJS)', JSON.stringify(org))
const wait = (socket, event, ms = 3000) => new Promise((resolve, reject) => { const t = setTimeout(() => reject(new Error('zaman aşımı: ' + event)), ms); socket.once(event, v => { clearTimeout(t); resolve(v) }) })
const ack = (socket, event, body) => socket.timeout(3000).emitWithAck(event, body)

async function socketScenario(base, token) {
  const out = []
  const s = io(`${base}/stream`, { auth: { token }, transports: ['websocket', 'polling'], reconnection: false })
  const self1 = wait(s, 'presence:self')
  await wait(s, 'connect')
  out.push(['bağlanınca presence:self', await self1])
  const self2 = wait(s, 'presence:self')
  out.push(['presence:prefer BUSY ack', await ack(s, 'presence:prefer', { status: 'BUSY' })])
  out.push(['presence:self BUSY', await self2])
  out.push(['presence:prefer geçersiz', await ack(s, 'presence:prefer', { status: 'UYKU' })])
  out.push(['presence:prefer ONLINE', await ack(s, 'presence:prefer', { status: 'ONLINE' })])
  const self3 = wait(s, 'presence:self')
  out.push(['presence:idle away', await ack(s, 'presence:idle', { away: true })])
  out.push(['presence:self AWAY (boşta)', await self3])
  out.push(['presence:idle geri', await ack(s, 'presence:idle', { away: false })])
  out.push(['subscribe: üye olunmayan', await ack(s, 'subscribe', { organizationId: 'yok' })])
  out.push(['subscribe: gövdesiz', await ack(s, 'subscribe', {})])
  out.push(['snapshot: abone olmadan', await ack(s, 'presence:snapshot', { organizationId: orgId })])
  out.push(['subscribe: üye', await ack(s, 'subscribe', { organizationId: orgId })])
  out.push(['snapshot: abone olduktan sonra', await ack(s, 'presence:snapshot', { organizationId: orgId })])
  s.disconnect()
  await new Promise(r => setTimeout(r, 300))
  const bad = io(`${base}/stream`, { auth: { token: 'bozuk' }, transports: ['websocket'], reconnection: false })
  const outcome = await Promise.race([wait(bad, 'disconnect').then(reason => 'disconnect: ' + reason), wait(bad, 'connect_error').then(e => 'connect_error'), new Promise(r => setTimeout(() => r('zaman aşımı'), 3000))])
  out.push(['bozuk jetonla bağlantı', outcome])
  bad.close()
  return out
}
const sn = await socketScenario(NEST, nest.access.accessToken)
const sr = await socketScenario(RUST, nest.access.accessToken)
for (let i = 0; i < sn.length; i++) {
  const [name, a] = sn[i], [, b] = sr[i]
  if (isDeepStrictEqual(a, b)) ok(`${name} → ${JSON.stringify(a)}`)
  else bad(name, 'NestJS: ' + JSON.stringify(a) + '\n      Rust:   ' + JSON.stringify(b))
}
// Çapraz yayın: Rust'a bağlı kullanıcının durum değişikliği veritabanına yansıyor mu?
const watch = io(`${RUST}/stream`, { auth: { token: rust.access.accessToken }, transports: ['websocket'], reconnection: false })
await wait(watch, 'connect')
const seen = wait(watch, 'presence:update')
const other = io(`${RUST}/stream`, { auth: { token: nest.access.accessToken }, transports: ['websocket'], reconnection: false })
const update = await seen
update.userId && update.status ? ok('başka kullanıcının presence:update yayını → ' + JSON.stringify({ ...update, userId: '<id>' })) : bad('presence:update', JSON.stringify(update))
other.close(); watch.close()
await new Promise(r => setTimeout(r, 400))
const after = await call(NEST, 'GET', '/auth/me', undefined, nest.access.accessToken)
after.body.status === 'OFFLINE' ? ok('bağlantılar kapanınca durum OFFLINE (NestJS\'ten okundu)') : bad('çıkışta durum', JSON.stringify(after.body))

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
