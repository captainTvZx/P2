// Ürün sürümü: tek kaynak kök Cargo.toml `[workspace.package] version` (build.rs'ler de bunu
// CARGO_PKG_VERSION olarak görür). Sıra: --version > P2_PRODUCT_VERSION > Cargo.toml.
import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

export function workspaceVersion(root = repoRoot) {
  const text = readFileSync(join(root, 'Cargo.toml'), 'utf8')
  const section = text.split(/^\[workspace\.package\]\s*$/m)[1]?.split(/^\[/m)[0] || ''
  const version = section.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (!version) throw new Error('Cargo.toml [workspace.package] version bulunamadı')
  return version
}

export function productVersion(cliVersion) {
  const env = (process.env.P2_PRODUCT_VERSION || '').trim()
  return cliVersion || env || workspaceVersion()
}
