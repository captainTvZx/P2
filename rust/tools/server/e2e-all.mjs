// Rust istemcisinin bütün uçtan uca dilim testlerini Rust sunucusuna (P2_API) karşı çalıştırır.
//   node rust/tools/server/e2e-all.mjs [slice2 slice3 ...]
import { spawnSync } from 'node:child_process'
import { startRust } from './lib.mjs'

const all = ['slice2', 'slice2b', 'slice3', 'slice4', 'slice4c', 'slice5', 'slice6', 'slice7', 'slice8', 'slice8b', 'slice9', 'slice10', 'slice11']
const pick = process.argv.slice(2).length ? process.argv.slice(2) : all
const rust = await startRust(4130)
const results = []
for (const name of pick) {
  const r = spawnSync('node', [`C:/Users/capta/OneDrive/Desktop/P2/rust/tools/${name}-test.mjs`, 'C:/Users/capta/OneDrive/Desktop/P2'], { env: { ...process.env, P2_API: rust.base }, encoding: 'utf8', timeout: 900000, maxBuffer: 64 << 20 })
  const out = (r.stdout || '') + (r.stderr || '')
  const line = out.split(/\r?\n/).reverse().find(l => /SONUÇ/.test(l)) || `çıkış ${r.status}`
  const failed = out.split(/\r?\n/).filter(l => /✘/.test(l))
  results.push({ name, line, failed })
  console.log(`${name}: ${line}`)
  for (const f of failed) console.log('    ' + f.trim().slice(0, 300))
}
rust.stop()
process.exit(results.some(r => !/0 kaldı/.test(r.line)) ? 1 : 0)
