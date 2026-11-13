// Ortak sabitler ve yardımcılar.
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

export const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..')
/** Çıktılar depo dışında (OneDrive senkronu kilitlemesin). */
export const PARITY_DIR = process.env.P2_PARITY_OUT || 'C:/p2-pg/parity'
export const RUST_BIN = process.env.P2_RUST_BIN || 'C:/p2-rust-target/release'
export const API = process.env.P2_API || 'http://127.0.0.1:4110'
export const sleep = ms => new Promise(r => setTimeout(r, ms))
export const log = (...a) => console.log('[parite]', ...a)

export function parseArgs(argv) {
  const out = { _: [] }
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (a.startsWith('--')) {
      const [k, v] = a.slice(2).split('=')
      if (v !== undefined) out[k] = v
      else if (argv[i + 1] && !argv[i + 1].startsWith('--')) out[k] = argv[++i]
      else out[k] = true
    } else out._.push(a)
  }
  return out
}
