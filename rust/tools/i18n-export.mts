// src/locales/*.ts sözlüklerini rust/crates/p2-i18n/locales/<dil>.json olarak dışa aktarır.
// Geçiş boyunca kaynak TypeScript sözlükleridir; Electron emekliye ayrılınca JSON kaynak olur.
//   npx tsx rust/tools/i18n-export.mts
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { DICTIONARIES } from '../../src/locales/index.ts'

const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'crates', 'p2-i18n', 'locales')
mkdirSync(out, { recursive: true })
for (const [lang, dict] of Object.entries(DICTIONARIES)) {
  const sorted = Object.fromEntries(Object.entries(dict).sort(([a], [b]) => a.localeCompare(b)))
  writeFileSync(join(out, `${lang}.json`), JSON.stringify(sorted, null, 1) + '\n')
  console.log(`${lang}: ${Object.keys(dict).length} anahtar`)
}
