// Faz 2 sözleşme testleri için ortak yardımcılar: NestJS (:4110) ile Rust sunucusunu
// (varsayılan :4120) aynı isteklere karşı çalıştırıp maskelenmiş cevapları karşılaştırır.
// Rust sunucusunu başlatmak: startRust(port) — derlenmiş exe'yi C:/p2-pg/srv-<port>.exe olarak
// kopyalayıp test veritabanı ortamıyla (C:/p2-pg/test-env.json) çalıştırır.
import { spawn } from 'node:child_process'
import { copyFileSync, readFileSync } from 'node:fs'
import { isDeepStrictEqual } from 'node:util'

export const NEST = process.env.P2_NEST || 'http://127.0.0.1:4110'
export const sleep = ms => new Promise(r => setTimeout(r, ms))

export async function startRust(port = 4120, opts = {}) {
  const exe = `C:/p2-pg/srv-${port}.exe`
  copyFileSync('C:/p2-rust-target/release/p2-server.exe', exe)
  const env = JSON.parse(readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^﻿/, ''))
  const child = spawn(exe, [], { env: { ...process.env, ...env, PORT: String(port), HOST: '127.0.0.1', RUST_LOG: 'warn' }, stdio: ['ignore', 'inherit', 'inherit'], cwd: opts.cwd || 'C:/Users/capta/OneDrive/Desktop/P2' })
  const base = `http://127.0.0.1:${port}`
  for (let i = 0; i < 100; i++) { try { if ((await fetch(base + '/health')).ok) break } catch {} await sleep(100) }
  return { base, stop: () => child.kill() }
}

export async function call(base, method, path, body, token, extra = {}) {
  const headers = { 'content-type': 'application/json', 'x-p2-client-version': '0.9.9', ...extra }
  if (token) headers.authorization = 'Bearer ' + token
  const res = await fetch(base + path, { method, headers, body: body === undefined ? undefined : (typeof body === 'string' ? body : JSON.stringify(body)) })
  const text = await res.text()
  let json; try { json = JSON.parse(text) } catch { json = text }
  return { status: res.status, body: json }
}

export async function login(base, identifier, password = 'Parola123') {
  return (await call(base, 'POST', '/auth/login', { identifier, password })).body
}

const ISO = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

// Değişken değerleri (uuid, zaman, jeton) şekliyle maskeler; anahtar sırası korunur.
export function mask(value, replace = []) {
  if (Array.isArray(value)) return value.map(v => mask(v, replace))
  if (value && typeof value === 'object') {
    const out = {}
    for (const [k, v] of Object.entries(value)) out[k] = ['accessToken', 'refreshToken', 'secret', 'token'].includes(k) && typeof v === 'string' ? `<${k}>` : mask(v, replace)
    return out
  }
  if (typeof value === 'string') {
    if (UUID.test(value)) return '<uuid>'
    if (ISO.test(value)) return '<date>'
    let s = value
    for (const [from, to] of replace) s = s.split(from).join(to)
    return s
  }
  return value
}

let pass = 0, fail = 0
// İki cevabı (durum + gövde, anahtar sırası dahil) karşılaştırır.
export function same(name, a, b, replace = []) {
  const ma = mask(a, replace), mb = mask(b, replace)
  const ok = isDeepStrictEqual(ma, mb) && JSON.stringify(ma) === JSON.stringify(mb)
  if (ok) { pass++; console.log('  ✔ ' + name) }
  else { fail++; console.log('  ✘ ' + name + '\n      nest: ' + JSON.stringify(ma).slice(0, 900) + '\n      rust: ' + JSON.stringify(mb).slice(0, 900)) }
  return ok
}
export function check(name, ok, detail = '') { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
export function done() { console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`); process.exit(fail ? 1 : 0) }

// Aynı isteği iki sunucuya sırayla gönderip karşılaştırır (salt okunur uçlar için).
export async function both(name, rust, method, path, body, tokens, replace) {
  const a = await call(NEST, method, path, body, tokens?.nest)
  const b = await call(rust, method, path, body, tokens?.rust ?? tokens?.nest)
  return same(name, a, b, replace)
}
