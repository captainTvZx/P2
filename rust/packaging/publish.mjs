// Rust kurulum paketlerini sunucunun güncelleme klasörüne yayınlar (scripts/publish-update.mjs'in
// Rust karşılığı; aynı klasör düzeni — rust/apps/p2-server/src/updates.rs bunu sunar):
//
//   <P2_UPDATE_DIR>/client/{P2-Client-Setup-<v>.exe, latest.yml}   → /updates/client/…
//   <P2_UPDATE_DIR>/{P2-Client-Setup-<v>.exe, latest.yml}          → /updates/… (eski istemciler)
//   <P2_UPDATE_DIR>/server/{P2-Server-Setup-<v>.exe, latest.yml}   → /updates/server/…
//   <P2_UPDATE_DIR>/P2-Server-Setup-<v>.exe                        (0.7.10 ve eski sunucular; latest.yml istemcinin)
//
//   node rust/packaging/publish.mjs                       → paketle (build-installers.mjs) + yayınla
//   node rust/packaging/publish.mjs --skip-build          → <out>'taki hazır paketi yayınla
//   node rust/packaging/publish.mjs --product client --version 0.11.0 --skip-build
//
// Hedef: P2_UPDATE_DIR, yoksa %ProgramData%\P2\updates (sunucu sürecinin okuduğu yer). Önce
// ikili dosyalar, en son latest.yml kopyalanır: yarım yayın hiçbir zaman ilan edilmez. Aynı ürünün
// eski kurulumları ve (Rust paketinde üretilmeyen) eski .blockmap dosyaları silinir — yanlış bir
// blockmap, Electron'un electron-updater'ını boşuna fark indirmesine sokar.

import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, unlinkSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'

const here = dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
const repo = resolve(here, '..', '..')
function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  return i >= 0 && process.argv[i + 1] && !process.argv[i + 1].startsWith('--') ? process.argv[i + 1] : fallback
}
const pkg = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8'))
const version = arg('version', pkg.version)
const which = arg('product', 'all')
const products = which === 'all' ? ['client', 'server'] : [which]
const outRoot = resolve(arg('out', join(process.env.CARGO_TARGET_DIR || 'C:/p2-rust-target', 'dist')))
const target = resolve(arg('target', process.env.P2_UPDATE_DIR || join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'updates')))

if (!process.argv.includes('--skip-build')) {
  const passthrough = ['--product', which, '--version', version, '--out', outRoot]
  execFileSync(process.execPath, [join(here, 'build-installers.mjs'), ...passthrough], { stdio: 'inherit' })
}

const META = {
  client: { installer: `P2-Client-Setup-${version}.exe`, mirrorRoot: true, exposeAtRoot: false, pattern: /^P2-Client-Setup-.+\.exe(\.blockmap)?$/i },
  server: { installer: `P2-Server-Setup-${version}.exe`, mirrorRoot: false, exposeAtRoot: true, pattern: /^P2-Server-Setup-.+\.exe(\.blockmap)?$/i },
}

for (const id of products) {
  const product = META[id]
  if (!product) throw new Error(`Bilinmeyen ürün: ${id}`)
  const source = join(outRoot, id)
  const installer = join(source, product.installer)
  const yml = join(source, 'latest.yml')
  const missing = [installer, yml].filter(file => !existsSync(file))
  if (missing.length) {
    console.error(`${id} paketi eksik: ${missing.join(', ')}\nÖnce: node rust/packaging/build-installers.mjs --product ${id} --version ${version}`)
    process.exit(1)
  }
  const declared = readFileSync(yml, 'utf8').match(/^version: (.+)$/m)?.[1]
  if (declared !== version) throw new Error(`${yml} sürümü ${declared}, beklenen ${version}`)

  const publish = destination => {
    mkdirSync(destination, { recursive: true })
    copyFileSync(installer, join(destination, product.installer))
  }
  const destinations = [join(target, id), ...(product.mirrorRoot ? [target] : [])]
  for (const destination of destinations) {
    publish(destination)
    copyFileSync(yml, join(destination, 'latest.yml'))
    for (const name of readdirSync(destination)) {
      if (name !== product.installer && product.pattern.test(name)) unlinkSync(join(destination, name))
    }
  }
  if (product.exposeAtRoot) {
    publish(target)
    for (const name of readdirSync(target)) {
      if (name !== product.installer && product.pattern.test(name)) unlinkSync(join(target, name))
    }
  }
  console.log(`  ${product.installer} · ${(statSync(installer).size / 1048576).toFixed(1)} MB → ${destinations.join(', ')}${product.exposeAtRoot ? ` (+ ${target})` : ''}`)
}

console.log(`\nYayınlandı: ${version} → ${target}`)
console.log('Açık uygulamalar sürümü anlık duyuruyla; duyuru kaçırılırsa en geç 5 dakika içinde görür.')
