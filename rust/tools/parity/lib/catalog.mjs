// Ekran kataloğu (catalog.json) okuyucu: makrolar ("use") açılır.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO } from './common.mjs'

export function loadCatalog(file = join(REPO, 'rust/tools/parity/catalog.json')) {
  return JSON.parse(readFileSync(file, 'utf8'))
}

/** Bir ekranın Electron adımları; {"use": "makro"} öğeleri catalog.macros'tan açılır. */
export function electronSteps(catalog, screen) {
  const expand = steps => (steps || []).flatMap(step => step.use ? expand(catalog.macros?.[step.use] || []) : [step])
  return expand(screen.electron?.steps || [])
}

/** Rust ortam değişkenleri: ortak + makro + ekrana özgü. */
export function rustEnv(catalog, screen) {
  const env = {}
  for (const use of screen.rust?.use || []) Object.assign(env, catalog.rustMacros?.[use] || {})
  Object.assign(env, screen.rust?.env || {})
  return env
}
