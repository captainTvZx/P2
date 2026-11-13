// Electron'un tema motorunu (src/theme.ts paletteFor) çalıştırıp 19 temanın bütün token'larını JSON'a döker.
// Rust testi (crates/p2-ui/src/palette.rs) bu dosyayla birebir karşılaştırır.
//   node rust/tools/parity/export-palettes.mjs
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { REPO } from './lib/common.mjs'

// Node 24 TypeScript tür ayıklamayla .ts dosyasını doğrudan yükler (theme.ts'in içe aktarımı yok).
const theme = await import(pathToFileURL(join(REPO, 'src/theme.ts')).href)
const out = {}
for (const t of theme.P2_THEMES) out[t.id] = theme.paletteFor(t)
const file = join(REPO, 'rust/crates/p2-ui/src/palette_electron.json')
writeFileSync(file, JSON.stringify(out, null, 1))
console.log(file, Object.keys(out).length, 'tema')
