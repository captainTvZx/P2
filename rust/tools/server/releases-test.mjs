// Kütüphane sürümleri (releases.ts ↔ releases.rs): parça yükleme, manifest, yayın, canlı, fark,
// indirme (Range), kurulum kayıtları. Parça deposu diske yazdığı için paylaşılan NestJS yerine
// geçici P2_RELEASE_DIR ile ayrı bir NestJS (:4194) açılır; Rust (:4124) başka bir geçici dizinde.
import { spawn, execSync } from 'node:child_process'
import { createHash, randomBytes } from 'node:crypto'
import { mkdtempSync, readFileSync, rmSync, existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { startRust, call, login, same, check, mask, done, sleep } from './lib.mjs'

const REPO = fileURLToPath(new URL('../../../', import.meta.url))
const require = createRequire(join(REPO, 'package.json'))
const { io } = require('socket.io-client')

async function startNest(port, extra) {
  const env = JSON.parse(readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^\uFEFF/, ''))
  const child = spawn(process.execPath, [join(REPO, 'node_modules/tsx/dist/cli.mjs'), '--tsconfig', 'server/tsconfig.json', 'server/src/main.ts'], {
    cwd: REPO, env: { ...process.env, ...env, ...extra, PORT: String(port), HOST: '127.0.0.1' }, stdio: 'ignore',
  })
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 600; i++) { try { if ((await fetch(base + '/health')).ok) break } catch {} await sleep(100) }
  return { base, stop: () => { try { execSync(`taskkill /pid ${child.pid} /T /F`, { stdio: 'ignore' }) } catch {} } }
}

const nestDir = mkdtempSync(join(tmpdir(), 'p2-rel-nest-'))
const rustDir = mkdtempSync(join(tmpdir(), 'p2-rel-rust-'))
const nest = await startNest(4194, { P2_RELEASE_DIR: nestDir })
process.env.P2_RELEASE_DIR = rustDir
const rust = await startRust(4124)
delete process.env.P2_RELEASE_DIR
const NEST = nest.base

const owner = await login(NEST, 'p2test')
const bot = await login(NEST, 'rustbot')
const T = owner.accessToken, B = bot.accessToken, BOT = bot.user.id
const org = (await call(NEST, 'GET', '/organizations', undefined, T)).body[0].id
const tag = Math.random().toString(36).slice(2, 8)

async function project(label, withBot = true) {
  const res = await call(NEST, 'POST', `/organizations/${org}/projects`, { name: `RT kütüphane ${tag}`, slug: `rt-kutup-${label}-${tag}` }, T)
  if (res.status !== 201) throw new Error('proje açılamadı: ' + JSON.stringify(res.body))
  if (withBot) await call(NEST, 'PATCH', `/organizations/${org}/admin/access/projects/${res.body.id}/members/${BOT}`, { visible: true }, T)
  return res.body.id
}
const PN = await project('n'), PR = await project('r'), HIDDEN = await project('h', false)
const cleanup = async () => {
  for (const P of [PN, PR, HIDDEN]) await call(NEST, 'DELETE', `/organizations/${org}/projects/${P}`, undefined, T)
  nest.stop(); rust.stop()
  await sleep(300)
  for (const dir of [nestDir, rustDir]) try { rmSync(dir, { recursive: true, force: true }) } catch {}
}
process.on('uncaughtException', async e => { console.error(e); await cleanup(); process.exit(1) })
process.on('unhandledRejection', async e => { console.error(e); await cleanup(); process.exit(1) })

// Bu çalıştırmaya özgü parçalar (iki sunucuda aynı içerik → aynı özetler).
const sha = buf => createHash('sha256').update(buf).digest('hex')
const C = { exe1: randomBytes(10), a1: randomBytes(8), a2: randomBytes(12), exe2: randomBytes(10), c: randomBytes(5) }
const H = Object.fromEntries(Object.entries(C).map(([k, v]) => [k, sha(v)]))
const files1 = [
  { path: 'Game.exe', size: 10, chunks: [H.exe1] },
  { path: 'data\\a.pak', size: 20, chunks: [H.a1, H.a2], lengths: [8, 12] },
  { path: 'data/b.pak', size: 10, chunks: [H.exe1] },
]
const files2 = [
  { path: 'Game.exe', size: 10, chunks: [H.exe2] },
  { path: 'data/a.pak', size: 20, chunks: [H.a1, H.a2], lengths: [8, 12] },
  { path: 'data/c.pak', size: 5, chunks: [H.c] },
]

async function raw(base, method, path, { token = T, body, type = 'application/octet-stream', headers = {} } = {}) {
  const h = { ...headers }
  if (token) h.authorization = 'Bearer ' + token
  if (type) h['content-type'] = type
  const res = await fetch(base + path, { method, headers: h, body })
  const buf = Buffer.from(await res.arrayBuffer())
  const pick = ['content-type', 'content-length', 'content-range', 'accept-ranges', 'cache-control', 'x-content-type-options']
  const hdrs = Object.fromEntries(pick.map(k => [k, res.headers.get(k)]).filter(([, v]) => v !== null))
  // Express JSON'a charset ekler; içerik türü yalnız JSON dışı cevaplarda karşılaştırılır.
  if (hdrs['content-type']?.startsWith('application/json')) { delete hdrs['content-type']; delete hdrs['content-length'] }
  let json; try { json = JSON.parse(buf.toString('utf8')) } catch {}
  return { status: res.status, headers: hdrs, body: json ?? buf.toString('hex') }
}

async function listen(base, token) {
  const events = []
  const socket = io(base + '/stream', { auth: { token }, transports: ['websocket'], reconnection: false })
  socket.onAny((event, payload) => events.push([event, payload]))
  await new Promise((resolve, reject) => { socket.on('connect', resolve); socket.on('connect_error', reject) })
  await new Promise(resolve => socket.emit('subscribe', { organizationId: org }, resolve))
  return { events, close: () => socket.close() }
}

async function scenario(base, P) {
  const out = []
  const root = `/organizations/${org}/projects/${P}/releases`
  const r = async (name, method, path, body, token = T) => {
    const res = await call(base, method, root + path, body, token)
    // Sırasız Prisma findMany: NestJS bazen dizin, bazen yığın sırası döndürüyor; küme olarak karşılaştır.
    if (name === 'eksikler: hepsi' && Array.isArray(res.body?.missing)) res.body.missing.sort()
    out.push([name, res])
    return res
  }
  const x = async (name, method, path, opts) => {
    const res = await raw(base, method, root + path, opts)
    out.push([name, res])
    return res
  }
  await r('liste (boş)', 'GET', '')
  await r('liste (üye)', 'GET', '', undefined, B)
  await r('en son (yok)', 'GET', '/latest')
  await r('oluştur: boş gövde', 'POST', '', {})
  await r('oluştur: kategori boşlukla başlar', 'POST', '', { channel: ' beta', version: '1.0', executable: 'Game.exe', files: [] })
  await r('oluştur: dosyalar dizi değil', 'POST', '', { channel: 'beta', version: '1.0', executable: 'Game.exe', files: 'x' })
  const bad = [
    ['boş liste', { files: [] }],
    ['exe değil', { executable: 'game.txt' }],
    ['üst dizin', { files: [{ path: '../x', size: 1, chunks: [H.exe1] }] }],
    ['kayıt sayı', { files: [5] }],
    ['kayıt dizi', { files: [[1]] }],
    ['yol sayı', { files: [{ path: 12.5, size: 1, chunks: [H.exe1] }] }],
    ['aynı yol', { files: [files1[0], { ...files1[0], path: 'game.EXE' }] }],
    ['boyut eksi', { files: [{ path: 'Game.exe', size: -1, chunks: [] }] }],
    ['parça sayısı', { files: [{ path: 'Game.exe', size: 10, chunks: [] }] }],
    ['parça özeti', { files: [{ path: 'Game.exe', size: 10, chunks: ['ABC'] }] }],
    ['uzunluk null', { files: [{ path: 'Game.exe', size: 10, chunks: [H.exe1], lengths: null }] }],
    ['uzunluk toplamı', { files: [{ path: 'Game.exe', size: 10, chunks: [H.exe1], lengths: [9] }] }],
    ['exe listede yok', { files: [files1[1]] }],
    ['ayrılmış ad', { files: [{ path: 'x/CON.txt', size: 1, chunks: [H.exe1] }] }],
  ]
  for (const [name, patch] of bad) await r('manifest: ' + name, 'POST', '', { channel: 'beta', version: '9.9', executable: 'Game.exe', files: files1, ...patch })
  const create1 = { channel: 'beta  test', version: '1.0.0', notes: ' ilk sürüm ', executable: 'Game.exe', launchArgs: ' -windowed ', overlay: false, files: files1, junk: true }
  const v1 = (await r('oluştur 1.0.0', 'POST', '', create1)).body.release.id
  await r('oluştur tekrar → devam', 'POST', '', create1)
  await r('oluştur başka içerik → çakışma', 'POST', '', { ...create1, files: files2 })
  await r('oluştur: üye yetkisiz', 'POST', '', create1, B)
  await x('manifest (sahip)', 'GET', `/${v1}/manifest`)
  await x('manifest taslak (üye)', 'GET', `/${v1}/manifest`, { token: B })
  await r('ayrıntı', 'GET', `/${v1}`)
  await r('ayrıntı taslak (üye)', 'GET', `/${v1}`, undefined, B)
  await r('ayrıntı: olmayan', 'GET', `/yok`)
  await r('eksikler: süzgeçli', 'POST', `/${v1}/missing`, { hashes: [H.a1, 'zz'] })
  await r('eksikler: hepsi', 'POST', `/${v1}/missing`, { hashes: [] })
  await r('eksikler: geçersiz', 'POST', `/${v1}/missing`, { hashes: 'x' })
  await r('eksikler: olmayan sürüm', 'POST', `/yok/missing`, {})
  await x('parça: özet geçersiz', 'PUT', `/${v1}/chunks/abc`, { body: C.exe1 })
  await x('parça: içerik türü', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.exe1, type: 'text/plain' })
  await x('parça: 4 MB üstü', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: Buffer.alloc(4 * 1024 * 1024 + 1) })
  await x('parça: özet uyuşmuyor', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.a1 })
  await x('parça: boş', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: Buffer.alloc(0) })
  await x('parça: manifestte yok', 'PUT', `/${v1}/chunks/${H.c}`, { body: C.c })
  await x('parça: üye yetkisiz', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.exe1, token: B })
  await x('parça: olmayan sürüm', 'PUT', `/yok/chunks/${H.exe1}`, { body: C.exe1 })
  await x('parça yükle', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.exe1 })
  await x('parça zaten var', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.exe1 })
  await r('yayınla: eksik parça', 'POST', `/${v1}/publish`)
  await x('parça a1', 'PUT', `/${v1}/chunks/${H.a1}`, { body: C.a1 })
  await x('parça a2', 'PUT', `/${v1}/chunks/${H.a2}`, { body: C.a2 })
  await r('eksikler: tamam', 'POST', `/${v1}/missing`)
  await r('canlı: taslak', 'POST', `/${v1}/live`)
  await r('yayınla (ilk → canlı)', 'POST', `/${v1}/publish`)
  await r('yayınla tekrar', 'POST', `/${v1}/publish`)
  await r('yayınla: olmayan', 'POST', `/yok/publish`)
  await x('parça: yayınlanmışa ekleme', 'PUT', `/${v1}/chunks/${H.exe1}`, { body: C.exe1 })
  await x('indir: tam', 'GET', `/chunks/${H.exe1}`, { type: null })
  await x('indir: aralık', 'GET', `/chunks/${H.a2}`, { type: null, headers: { range: 'bytes=2-5' } })
  await x('indir: sonek', 'GET', `/chunks/${H.a2}`, { type: null, headers: { range: 'bytes=-3' } })
  await x('indir: açık uç', 'GET', `/chunks/${H.a2}`, { type: null, headers: { range: ' bytes=4- ' } })
  await x('indir: taşan', 'GET', `/chunks/${H.a2}`, { type: null, headers: { range: 'bytes=50-' } })
  await x('indir: bozuk aralık', 'GET', `/chunks/${H.a2}`, { type: null, headers: { range: 'items=1-2' } })
  await x('indir: üye', 'GET', `/chunks/${H.a1}`, { type: null, token: B })
  await x('indir: özet geçersiz', 'GET', `/chunks/xyz`, { type: null })
  await x('indir: bilinmeyen parça', 'GET', `/chunks/${'0'.repeat(64)}`, { type: null })
  await x('manifest yayınlı (üye)', 'GET', `/${v1}/manifest`, { token: B })
  await r('fark: ilk sürüm', 'GET', `/${v1}/diff`)

  const create2 = { channel: 'beta test', version: '1.1.0', notes: 'ikinci', executable: 'Game.exe', files: files2 }
  const v2 = (await r('oluştur 1.1.0', 'POST', '', create2)).body.release.id
  await r('fark: taslak', 'GET', `/${v2}/diff`)
  for (const k of ['exe2', 'c']) await x('parça ' + k, 'PUT', `/${v2}/chunks/${H[k]}`, { body: C[k] })
  await r('yayınla 1.1.0 (canlı var)', 'POST', `/${v2}/publish`)
  await r('fark: 1.1.0', 'GET', `/${v2}/diff`)
  await r('fark: against açık', 'GET', `/${v2}/diff?against=${v1}`)
  await r('fark: against kendisi', 'GET', `/${v2}/diff?against=${v2}`)
  await r('fark: against yok', 'GET', `/${v2}/diff?against=yok`)
  await r('fark: üye', 'GET', `/${v1}/diff`, undefined, B)
  await r('liste (iki yayın)', 'GET', '')
  await r('en son (canlı 1.0.0)', 'GET', '/latest?channel=beta%20%20test')
  await r('canlı 1.1.0', 'POST', `/${v2}/live`)
  await r('canlı tekrar', 'POST', `/${v2}/live`)
  await r('canlı: olmayan', 'POST', `/yok/live`)
  const v3 = (await r('taslak 3.0', 'POST', '', { channel: '7', version: '3.0', executable: 'Game.exe', files: files2 })).body.release.id
  await r('taslak 2 (sayısal kategori)', 'POST', '', { channel: '10', version: '3.0', executable: 'Game.exe', files: files2, launchArgs: '-x' })
  await r('liste (taslaklar dahil)', 'GET', '')
  await r('liste (üye, taslaksız)', 'GET', '', undefined, B)
  await r('liste: kategori süzgeci', 'GET', '?channel=7')
  await r('en son (üye)', 'GET', '/latest', undefined, B)
  await r('düzenle: geçersiz kategori', 'PATCH', `/${v1}`, { channel: '-x' })
  await r('düzenle: yayınlı not/argüman', 'PATCH', `/${v1}`, { notes: '  yeni not ', launchArgs: '', overlay: true })
  await x('manifest argümansız', 'GET', `/${v1}/manifest`)
  await r('düzenle: argüman ekle', 'PATCH', `/${v2}`, { launchArgs: ' -fast ' })
  await x('manifest argümanlı', 'GET', `/${v2}/manifest`)
  await r('düzenle: yayınlı sürüm no', 'PATCH', `/${v1}`, { version: '2.0' })
  await r('düzenle: taslak yeniden adlandır', 'PATCH', `/${v3}`, { channel: ' alpha  kanal', version: '3.0.1' })
  await r('düzenle: taslak var olan ad', 'PATCH', `/${v3}`, { channel: 'beta test', version: '1.0.0' })
  await r('düzenle: notlar null → 500', 'PATCH', `/${v3}`, { notes: null })
  await r('düzenle: üye yetkisiz', 'PATCH', `/${v3}`, { notes: 'x' }, B)
  await r('düzenle: olmayan', 'PATCH', `/yok`, { notes: 'x' })

  await r('kurulum: cihaz geçersiz', 'POST', `/${v1}/installed`, { deviceId: 'bad id', status: 'INSTALLED' })
  await r('kurulum: durum geçersiz', 'POST', `/${v1}/installed`, { deviceId: 'dev1', status: 'X' })
  await r('kurulum: süre çok', 'POST', `/${v1}/installed`, { deviceId: 'dev1', status: 'INSTALLED', playSeconds: 7 * 24 * 3600 + 1 })
  await r('kurulum', 'POST', `/${v1}/installed`, { deviceId: 'dev1', status: 'INSTALLED', clientVersion: '0.9.9', launched: true, playSeconds: 60, pinned: true })
  await r('kurulum güncelle', 'POST', `/${v2}/installed`, { deviceId: 'dev1', status: 'UPDATING', playSeconds: 30 })
  await r('kurulum: üye', 'POST', `/${v2}/installed`, { deviceId: 'bot-1', status: 'INSTALLED' }, B)
  await r('kurulum: kaldırıldı', 'POST', `/${v2}/installed`, { deviceId: 'bot-2', status: 'REMOVED' }, B)
  await r('kurulum: pinned null → 500', 'POST', `/${v2}/installed`, { deviceId: 'dev1', status: 'INSTALLED', pinned: null })
  await r('kurulum: olmayan sürüm', 'POST', `/yok/installed`, { deviceId: 'dev1', status: 'INSTALLED' })
  await r('kurulumlar', 'GET', '/installs')
  await r('kurulumlar: üye yetkisiz', 'GET', '/installs', undefined, B)
  await r('liste (kurulum sayısı)', 'GET', '')

  await r('sil: üye yetkisiz', 'DELETE', `/${v2}`, undefined, B)
  await r('sil canlı 1.1.0 (1.0.0 canlıya)', 'DELETE', `/${v2}`)
  await r('sil: tekrar', 'DELETE', `/${v2}`)
  await r('sil taslak', 'DELETE', `/${v3}`)
  await r('son liste', 'GET', '')
  return { out, v1 }
}

const nestListen = [await listen(NEST, T), await listen(NEST, B)]
const rustListen = [await listen(rust.base, T), await listen(rust.base, B)]
const n = await scenario(NEST, PN)
const r = await scenario(rust.base, PR)
await sleep(800)
for (let i = 0; i < n.out.length; i++) same(n.out[i][0], n.out[i][1], r.out[i][1])

const mine = p => [PN, PR].some(id => JSON.stringify(p).includes(id))
const norm = list => list.filter(([e, p]) => /^(release:|hub:changed|activity:new)/.test(e) && mine(p)).map(([e, p]) => JSON.stringify([e, mask(p)])).sort()
for (const [i, who] of [[0, 'sahip'], [1, 'üye']]) {
  const a = norm(nestListen[i].events), b = norm(rustListen[i].events)
  check(`gerçek zamanlı olaylar (${who}): ${a.length} olay`, JSON.stringify(a) === JSON.stringify(b), a.length === b.length ? '' : `nest ${a.length} / rust ${b.length}`)
  if (JSON.stringify(a) !== JSON.stringify(b)) {
    console.log('    yalnız nest:', a.filter(v => !b.includes(v)).slice(0, 3).join('\n      '))
    console.log('    yalnız rust:', b.filter(v => !a.includes(v)).slice(0, 3).join('\n      '))
  }
}
for (const l of [...nestListen, ...rustListen]) l.close()

// Disk düzeni: <dizin>/chunks/ab/cd/<hash>, tmp boş.
for (const [name, dir] of [['nest', nestDir], ['rust', rustDir]]) {
  const p = join(dir, 'chunks', H.a1.slice(0, 2), H.a1.slice(2, 4), H.a1)
  check(`${name} parça yolu`, existsSync(p) && sha(readFileSync(p)) === H.a1)
}
check('rust tmp dizini var', existsSync(join(rustDir, 'tmp')))

// Çapraz okuma: Rust'ın yazdığı sürümleri NestJS (aynı veritabanı) aynı görüyor mu.
for (const [P, label] of [[PR, 'rust yazdı'], [PN, 'nest yazdı']]) {
  const path = `/organizations/${org}/projects/${P}/releases`
  same(`çapraz liste (${label})`, await call(NEST, 'GET', path, undefined, T), await call(rust.base, 'GET', path, undefined, T))
  same(`çapraz kurulumlar (${label})`, await call(NEST, 'GET', path + '/installs', undefined, T), await call(rust.base, 'GET', path + '/installs', undefined, T))
}
const v1path = `/organizations/${org}/projects/${PR}/releases/${r.v1}`
same('çapraz manifest (rust yazdı)', await raw(NEST, 'GET', v1path + '/manifest', { type: null }), await raw(rust.base, 'GET', v1path + '/manifest', { type: null }))
same('çapraz fark (rust yazdı)', await call(NEST, 'GET', v1path + '/diff', undefined, T), await call(rust.base, 'GET', v1path + '/diff', undefined, T))

const paths = [
  ['gizli proje', 'GET', `/organizations/${org}/projects/${HIDDEN}/releases`, B],
  ['gizli proje parça', 'GET', `/organizations/${org}/projects/${HIDDEN}/releases/chunks/${H.a1}`, B],
  ['olmayan proje', 'GET', `/organizations/${org}/projects/yok/releases/latest`, T],
  ['yabancı kuruluş', 'GET', `/organizations/yok/projects/${PN}/releases`, T],
  ['jetonsuz', 'GET', `/organizations/${org}/projects/${PN}/releases`, undefined],
  ['jetonsuz parça', 'PUT', `/organizations/${org}/projects/${PN}/releases/x/chunks/${H.a1}`, undefined],
]
for (const [name, method, path, token] of paths) same(name, await raw(NEST, method, path, { token, type: null }), await raw(rust.base, method, path, { token, type: null }))

await cleanup()
done()
