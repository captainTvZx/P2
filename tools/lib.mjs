// Uçtan uca testlerin (tools/slice*-test.mjs, faz8-test.mjs) ortak yardımcıları. YALNIZ Node
// yerleşikleri kullanılır (Node 22+; npm paketi, package.json ya da node_modules gerekmez).
//
//   testServer()      P2_API verilmişse onu kullanır; yoksa derlenmiş Rust sunucusunu
//                     (<target>/release/p2-server.exe) test PostgreSQL'iyle :4130'da (P2_TEST_PORT) başlatır ve
//                     test süreci çıkarken kapatır.
//   startServer(port) Rust sunucusunu verilen portta başlatır ({ base, stop }).
//   Client            `pg` paketinin testlerde kullanılan küçük alt kümesi (connect/query/end), psql ile.
//   io(url, opts)     socket.io-client'ın testlerde kullanılan alt kümesi (on/emit/close), WebSocket ile.
//
// Ortam: test PostgreSQL (:55432) ve C:/p2-pg/test-env.json (DATABASE_URL, JWT_* — BOM'lu olabilir);
// değiştirmek için P2_TEST_ENV, P2_PG_BIN (psql klasörü), CARGO_TARGET_DIR. Test veritabanını açmak:
//   C:/p2-pg/pgsql/bin/pg_ctl.exe -D C:/p2-pg/data -l C:/p2-pg/pg.log -o "-p 55432" start
import { spawn, execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'node:net'

export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const TARGET = join(process.env.CARGO_TARGET_DIR || 'C:/p2-rust-target', 'release')
export const PG_BIN = process.env.P2_PG_BIN || 'C:/p2-pg/pgsql/bin'
export const sleep = ms => new Promise(r => setTimeout(r, ms))

export function testEnv() {
  const file = process.env.P2_TEST_ENV || 'C:/p2-pg/test-env.json'
  return JSON.parse(readFileSync(file, 'utf8').replace(/^\uFEFF/, ''))
}

/** Rust sunucusunu test veritabanıyla başlatır. İkili `C:/p2-pg/srv-<port>.exe` olarak kopyalanır
 *  (çalışırken `cargo build` asıl exe'nin üzerine yazabilsin). Test süreci çıkınca kapanır. */
/** Port boş mu (başka bir sunucu dinliyorsa testler yanlışlıkla ona bağlanmasın). */
function portFree(port) {
  return new Promise(done => { const s = createServer().once('error', () => done(false)).once('listening', () => s.close(() => done(true))).listen(port, '127.0.0.1') })
}

export async function startServer(port = 4120, opts = {}) {
  if (!(await portFree(port))) throw new Error('127.0.0.1:' + port + ' kullanımda; başka bir port ver (P2_TEST_PORT) ya da P2_API ile o sunucuyu kullan')
  const source = join(TARGET, 'p2-server.exe')
  if (!existsSync(source)) throw new Error(`${source} yok: önce cargo build --release -p p2-server`)
  const exe = `C:/p2-pg/srv-${port}.exe`
  copyFileSync(source, exe)
  const env = { ...process.env, ...testEnv(), PORT: String(port), HOST: '127.0.0.1', RUST_LOG: process.env.RUST_LOG || 'warn' }
  const child = spawn(exe, [], { env, stdio: ['ignore', 'inherit', 'inherit'], cwd: opts.cwd || REPO })
  const stop = () => { try { child.kill() } catch {} }
  process.on('exit', stop)
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 150; i++) {
    if (child.exitCode !== null) throw new Error(`p2-server erken çıktı (kod ${child.exitCode})`)
    try { if ((await fetch(base + '/health')).ok) return { base, stop } } catch {}
    await sleep(100)
  }
  stop()
  throw new Error(`p2-server ${base}/health yanıt vermedi`)
}
/** Eski adı (tools/server/lib.mjs). */
export const startRust = startServer

/** Testin konuşacağı sunucu: P2_API ya da kendi başlattığı Rust sunucusu (:4130, P2_TEST_PORT). */
export async function testServer(port = Number(process.env.P2_TEST_PORT) || 4130) {
  if (process.env.P2_API) return { api: process.env.P2_API, stop: () => {} }
  const server = await startServer(port)
  console.log(`Rust sunucusu: ${server.base}`)
  return { api: server.base, stop: server.stop }
}

// ---------------------------------------------------------------- SQL (psql)

function literal(value) {
  if (value === null || value === undefined) return 'NULL'
  if (typeof value === 'number' || typeof value === 'bigint') return String(value)
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE'
  if (value instanceof Date) value = value.toISOString()
  else if (typeof value === 'object') value = JSON.stringify(value)
  return `'${String(value).replace(/'/g, "''")}'`
}

/** Sorguyu psql ile çalıştırır; satır döndürenlerde (SELECT/WITH/RETURNING) satırları JSON olarak
 *  verir. `$1…` parametreleri SQL değişmezine çevrilir (yalnız test verisi için). */
export function sql(text, params = [], url = testEnv().DATABASE_URL) {
  const query = text.trim().replace(/;$/, '').replace(/\$(\d+)/g, (_, n) => literal(params[Number(n) - 1]))
  const select = /^(select|with)\b/i.test(query)
  const rows = select || /\breturning\b/i.test(query)
  const wrapped = select ? `select coalesce(json_agg(t), '[]'::json) from (${query}) t` : rows ? `with t as (${query}) select coalesce(json_agg(t), '[]'::json) from t` : query
  const psql = join(PG_BIN, process.platform === 'win32' ? 'psql.exe' : 'psql')
  const out = execFileSync(existsSync(psql) ? psql : 'psql', ['-X', '-q', '-A', '-t', '-v', 'ON_ERROR_STOP=1', '-d', url, '-f', '-'], {
    input: wrapped + ';\n', encoding: 'utf8', windowsHide: true, env: { ...process.env, PGCLIENTENCODING: 'UTF8' },
  })
  return rows ? JSON.parse(out.trim() || '[]') : []
}

/** `new Client({ connectionString })` → connect / query(text, params) → { rows } / end. */
export class Client {
  constructor(options = {}) { this.url = options.connectionString || testEnv().DATABASE_URL }
  async connect() {}
  async query(text, params = []) { return { rows: sql(text, params, this.url) } }
  async end() {}
}

// ---------------------------------------------------------------- socket.io (Engine.IO v4, WebSocket)

/** `io('http://host:port/stream', { auth })` — yalnız websocket taşıması; on/emit/close. */
export function io(url, options = {}) {
  const target = new URL(url)
  const nsp = target.pathname && target.pathname !== '/' ? target.pathname : '/'
  const prefix = nsp === '/' ? '' : `${nsp},`
  const ws = new WebSocket(`${target.protocol === 'https:' ? 'wss' : 'ws'}://${target.host}/socket.io/?EIO=4&transport=websocket`)
  const handlers = new Map()
  const fire = (event, ...args) => { for (const fn of handlers.get(event) || []) fn(...args) }
  ws.addEventListener('message', ({ data }) => {
    const text = String(data)
    if (text[0] === '0') ws.send(`40${prefix}${JSON.stringify(options.auth || {})}`)
    else if (text === '2') ws.send('3')
    else if (text.startsWith(`40${prefix}`)) fire('connect')
    else if (text.startsWith(`44${prefix}`)) fire('connect_error', text.slice(2 + prefix.length))
    else if (text.startsWith(`42${prefix}`)) {
      const [event, ...args] = JSON.parse(text.slice(2 + prefix.length).replace(/^\d+/, ''))
      fire(event, ...args)
    }
  })
  ws.addEventListener('close', () => fire('disconnect'))
  return {
    on(event, fn) { handlers.set(event, [...(handlers.get(event) || []), fn]); return this },
    emit(event, ...args) { ws.send(`42${prefix}${JSON.stringify([event, ...args])}`) },
    close() { try { ws.send(`41${prefix}`) } catch {} ws.close() },
  }
}
