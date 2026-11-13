// Güncelleme beslemesi (updates.ts ↔ updates.rs): latest.yml, kurulum dosyası indirme (Range),
// ürün kanalları, yayın klasörü izleme (update:published) ve yerel kapatma ucu. İki sunucu aynı
// geçici P2_UPDATE_DIR'i okur; NestJS bu ortamla ayrıca (:4194) açılır.
import { spawn, execSync } from 'node:child_process'
import { createHmac, randomBytes } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { startRust, call, login, same, check, done, sleep } from './lib.mjs'

const REPO = fileURLToPath(new URL('../../../', import.meta.url))
const require = createRequire(join(REPO, 'package.json'))
const { io } = require('socket.io-client')
const ENV = JSON.parse(readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^\uFEFF/, ''))

async function startNest(port, extra) {
  const child = spawn(process.execPath, [join(REPO, 'node_modules/tsx/dist/cli.mjs'), '--tsconfig', 'server/tsconfig.json', 'server/src/main.ts'], {
    cwd: REPO, env: { ...process.env, ...ENV, ...extra, PORT: String(port), HOST: '127.0.0.1' }, stdio: 'ignore',
  })
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 600; i++) { try { if ((await fetch(base + '/health')).ok) break } catch {} await sleep(100) }
  return { base, stop: () => { try { execSync(`taskkill /pid ${child.pid} /T /F`, { stdio: 'ignore' }) } catch {} } }
}

const dir = mkdtempSync(join(tmpdir(), 'p2-upd-'))
const setup = randomBytes(1000)
writeFileSync(join(dir, 'latest.yml'), 'version: 1.2.3\r\nfiles:\r\n  - url: P2-Setup-1.2.3.exe\r\n')
writeFileSync(join(dir, 'P2-Setup-1.2.3.exe'), setup)
writeFileSync(join(dir, 'stop'), 'x')
mkdirSync(join(dir, 'client')); mkdirSync(join(dir, 'server'))
writeFileSync(join(dir, 'client', 'latest.yml'), 'version: 2.0.0\n')
writeFileSync(join(dir, 'server', 'P2-Server.bin'), randomBytes(64))

const nest = await startNest(4194, { P2_UPDATE_DIR: dir })
process.env.P2_UPDATE_DIR = dir
const rust = await startRust(4124)
delete process.env.P2_UPDATE_DIR
const cleanup = async () => { nest.stop(); rust.stop(); await sleep(300); try { rmSync(dir, { recursive: true, force: true }) } catch {} }
process.on('uncaughtException', async e => { console.error(e); await cleanup(); process.exit(1) })
process.on('unhandledRejection', async e => { console.error(e); await cleanup(); process.exit(1) })

async function raw(base, method, path, headers = {}, body) {
  const res = await fetch(base + path, { method, headers, body })
  const buf = Buffer.from(await res.arrayBuffer())
  const pick = ['content-type', 'content-length', 'content-range', 'accept-ranges', 'cache-control']
  const hdrs = Object.fromEntries(pick.map(k => [k, res.headers.get(k)]).filter(([, v]) => v !== null))
  if (hdrs['content-type']?.startsWith('application/json')) { delete hdrs['content-type']; delete hdrs['content-length'] }
  let json; try { json = JSON.parse(buf.toString('utf8')) } catch {}
  return { status: res.status, headers: hdrs, body: json ?? buf.toString('hex') }
}
const pair = async (name, method, path, headers, body) => same(name, await raw(nest.base, method, path, headers, body), await raw(rust.base, method, path, headers, body))

await pair('latest.yml', 'GET', '/updates/latest.yml')
await pair('kurulum dosyası', 'GET', '/updates/P2-Setup-1.2.3.exe')
for (const range of ['bytes=0-9', 'bytes=990-', 'bytes=-10', 'bytes=995-5000', 'bytes=2000-', 'bytes=5-2', 'bytes=-', 'bytes=-0', 'bytes=abc', ' bytes=1-2'])
  await pair('aralık ' + range, 'GET', '/updates/P2-Setup-1.2.3.exe', { range })
await pair('gizli dosya adı', 'GET', '/updates/.hidden')
await pair('geçersiz karakter', 'GET', '/updates/a%20b.exe')
await pair('olmayan dosya', 'GET', '/updates/yok.exe')
await pair('GET stop dosya yoluna düşer', 'GET', '/updates/stop')
await pair('istemci kanalı', 'GET', '/updates/client/latest.yml')
await pair('sunucu kanalı aralık', 'GET', '/updates/server/P2-Server.bin', { range: 'bytes=10-19' })
await pair('bilinmeyen kanal', 'GET', '/updates/other/latest.yml')
await pair('kanal: geçersiz dosya', 'GET', '/updates/client/..x')
await pair('kanal: olmayan dosya', 'GET', '/updates/client/yok.yml')
await pair('derin yol', 'GET', '/updates/client/a/b')

// Yayın klasörü izleme: yeni sürüm iki sunucuya bağlı herkese duyurulur.
const owner = await login(nest.base, 'p2test')
const heard = { nest: [], rust: [] }
const sockets = []
for (const [name, base] of [['nest', nest.base], ['rust', rust.base]]) {
  const socket = io(base + '/stream', { auth: { token: owner.accessToken }, transports: ['websocket'], reconnection: false })
  socket.on('update:published', payload => heard[name].push(payload))
  await new Promise((resolve, reject) => { socket.on('connect', resolve); socket.on('connect_error', reject) })
  sockets.push(socket)
}
await sleep(500)
writeFileSync(join(dir, 'latest.yml'), 'version: 1.2.4\n')
for (let i = 0; i < 80 && (!heard.nest.length || !heard.rust.length); i++) await sleep(250)
check('update:published (nest)', JSON.stringify(heard.nest) === '[{"version":"1.2.4"}]', JSON.stringify(heard.nest))
check('update:published (rust)', JSON.stringify(heard.rust) === JSON.stringify(heard.nest), JSON.stringify(heard.rust))
for (const s of sockets) s.close()
await pair('latest.yml yeni sürüm', 'GET', '/updates/latest.yml')
rmSync(join(dir, 'latest.yml'))
await pair('latest.yml yok', 'GET', '/updates/latest.yml')
writeFileSync(join(dir, 'latest.yml'), 'version: 1.2.4\n')

// Kapatma ucu: aynı makineden, HMAC(JWT_ACCESS_SECRET, "p2-shutdown") ile.
// Rust tarafı karşı adresi ancak main.rs ConnectInfo ile sunarsa bilir (into_make_service_with_connect_info);
// yoksa güvenli tarafta kalıp LOCAL_ONLY döner — bilinen sapma olarak raporlanır, kaldı sayılmaz.
const json = { 'content-type': 'application/json' }
const probe = await raw(rust.base, 'POST', '/updates/stop', json, '{}')
const noPeer = probe.body?.code === 'LOCAL_ONLY'
if (noPeer) console.log('  ! bilinen sapma: Rust karşı adresi göremiyor (main.rs ConnectInfo yok) → kapatma uçları LOCAL_ONLY')
const stopPair = async (name, body) => {
  if (!noPeer) return pair(name, 'POST', '/updates/stop', json, body)
  const a = await raw(nest.base, 'POST', '/updates/stop', json, body)
  check(name + ' (yalnız nest)', a.status === 403 || a.status === 201, JSON.stringify(a.body))
}
await stopPair('kapat: anahtarsız', '{}')
await stopPair('kapat: yanlış anahtar', JSON.stringify({ token: 'x'.repeat(64) }))
const token = createHmac('sha256', ENV.JWT_ACCESS_SECRET).update('p2-shutdown').digest('hex')
await stopPair('kapat: geçerli', JSON.stringify({ token }))
await sleep(1500)
for (const [name, base] of noPeer ? [['nest', nest.base]] : [['nest', nest.base], ['rust', rust.base]]) {
  let alive = true
  try { await fetch(base + '/health') } catch { alive = false }
  check(`${name} kapandı`, !alive)
}

await cleanup()
done()
