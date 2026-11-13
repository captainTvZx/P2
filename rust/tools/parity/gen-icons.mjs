// Electron'un çizgi/dolgu SVG ikonlarını (ComposerIcons.tsx, ChannelIcons.tsx, CallIcons.tsx) Rust'a aktarır:
//   node rust/tools/parity/gen-icons.mjs  →  rust/crates/p2-ui/src/icons_data.rs
// TSX değişince yeniden çalıştırın. Biçim: (takım, ad, svg iç işaretlemesi). Kontur kalınlığı CSS'ten
// gelir (bileşen başına farklı) — `p2_ui::glyphs::{filled, named}`.
import { readFileSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '../../..')
const src = f => readFileSync(join(ROOT, 'src', f), 'utf8')

// JSX → SVG öznitelikleri
const jsxToSvg = s => s
  .replace(/strokeWidth=/g, 'stroke-width=').replace(/strokeLinecap=/g, 'stroke-linecap=')
  .replace(/strokeLinejoin=/g, 'stroke-linejoin=').replace(/fillRule=/g, 'fill-rule=').replace(/clipRule=/g, 'clip-rule=')
  .replace(/=\{(\d+(?:\.\d+)?)\}/g, '="$1"').replace(/currentColor/g, '#fff')

const out = []
// ComposerIcon: paths={ad:<>…</>,…}
{
  const t = src('ComposerIcons.tsx')
  for (const m of t.matchAll(/(\w+):<>(.*?)<\/>/g)) out.push(['composer', m[1], jsxToSvg(m[2])])
}
// ChannelIcon: if(type==='X')return <svg …>…</svg>
{
  const t = src('ChannelIcons.tsx')
  for (const m of t.matchAll(/type==='(\w+)'\)return <svg[^>]*>(.*?)<\/svg>/g)) out.push(['channel', m[1].toLowerCase(), jsxToSvg(m[2])])
  const last = t.match(/return <svg viewBox="0 0 24 24" aria-hidden="true">(.*?)<\/svg>\s*}\s*$/m)
  if (last) out.push(['channel', 'forum', jsxToSvg(last[1])])
}
// CallIcons: paths:Record<…>={ ad:<>…</>, … }
{
  const t = src('CallIcons.tsx')
  for (const m of t.matchAll(/^\s+(\w+):<>(.*?)<\/>,?$/gm)) out.push(['call', m[1], jsxToSvg(m[2])])
}

const esc = s => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
let rs = '//! OTOMATİK ÜRETİLDİ — `node rust/tools/parity/gen-icons.mjs` (elle düzenlemeyin).\n'
rs += '//! Kaynak: src/ComposerIcons.tsx, src/ChannelIcons.tsx, src/CallIcons.tsx (viewBox 0 0 24 24).\n\n'
rs += '/// (takım, ad, svg iç işaretlemesi)\npub static ICONS: &[(&str, &str, &str)] = &[\n'
for (const [set, name, body] of out) rs += `    ("${set}", "${name}", "${esc(body)}"),\n`
rs += '];\n'
writeFileSync(join(ROOT, 'rust/crates/p2-ui/src/icons_data.rs'), rs)
console.log(`${out.length} ikon →`, 'rust/crates/p2-ui/src/icons_data.rs', [...new Set(out.map(o => o[0]))].map(s => `${s}:${out.filter(o => o[0] === s).length}`).join(' '))
