// P2 (Rust) Windows kurulum paketleri: "P2 Client" ve "P2 Server" NSIS kurulumları + latest.yml.
//
//   node rust/packaging/build-installers.mjs                 → ikisi (önce cargo build)
//   node rust/packaging/build-installers.mjs --product client --version 0.11.0
//   node rust/packaging/build-installers.mjs --skip-cargo    → var olan ikililerle paketle
//
// Seçenekler: --product client|server|all, --version X (varsayılan package.json; derlemeye
// P2_PRODUCT_VERSION olarak geçer, exe'lerin bildirdiği sürüm kurulumla aynı olur), --out DIR
// (varsayılan <target>/dist; OneDrive dışında — senkronizasyon büyük exe'yi yazarken kilitliyor),
// --skip-cargo, --target-dir DIR (varsayılan C:/p2-rust-target).
// Araçlar: makensis + eklentiler electron-builder önbelleğinden (%LOCALAPPDATA%\electron-builder\Cache),
// StdUtils/UAC/StrContains başlıkları node_modules/app-builder-lib'den. Değiştirmek için:
// P2_MAKENSIS, P2_NSIS_PLUGINS, P2_NSIS_INCLUDE, P2_WINSW.
//
// Çıktı: <out>/client/P2-Client-Setup-<v>.exe + latest.yml, <out>/server/P2-Server-Setup-<v>.exe +
// latest.yml (electron-builder ile aynı biçim; p2-updater ve Electron'un electron-updater'ı okur).
// Yayın: rust/packaging/publish.mjs.
//
// Bu betik kurulumu ÇALIŞTIRMAZ; doğrulama için verify-installer.mjs.

import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { copyFileSync, cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'

const here = dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
const repo = resolve(here, '..', '..')
const rustDir = join(repo, 'rust')

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  return i >= 0 && process.argv[i + 1] && !process.argv[i + 1].startsWith('--') ? process.argv[i + 1] : fallback
}
const flag = name => process.argv.includes(`--${name}`)

const pkg = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8'))
const version = arg('version', pkg.version)
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`Geçersiz sürüm: ${version}`)
const targetDir = resolve(arg('target-dir', process.env.CARGO_TARGET_DIR || 'C:/p2-rust-target'))
const release = join(targetDir, 'release')
const outRoot = resolve(arg('out', join(targetDir, 'dist')))
const which = arg('product', 'all')
const products = which === 'all' ? ['client', 'server'] : [which]
for (const p of products) if (!['client', 'server'].includes(p)) throw new Error(`Bilinmeyen ürün: ${p}`)

// electron-builder kimliği (package.json build + electron-builder.server.yml). GUID = UUID v5(appId,
// electron-builder ad alanı) — Electron kurulumunun kayıt anahtarlarıyla aynı olmalı.
const ELECTRON_BUILDER_NS = '50e065bc-3134-11e6-9bab-38c9862bdaf3'
export function uuidV5(name, namespace = ELECTRON_BUILDER_NS) {
  const ns = Buffer.from(namespace.replace(/-/g, ''), 'hex')
  const h = createHash('sha1').update(Buffer.concat([ns, Buffer.from(name, 'utf8')])).digest()
  h[6] = (h[6] & 0x0f) | 0x50
  h[8] = (h[8] & 0x3f) | 0x80
  const x = h.subarray(0, 16).toString('hex')
  return `${x.slice(0, 8)}-${x.slice(8, 12)}-${x.slice(12, 16)}-${x.slice(16, 20)}-${x.slice(20)}`
}

const company = typeof pkg.author === 'string' ? pkg.author.replace(/\s*<.*$/, '') : pkg.author?.name || 'P2 Team'
const PRODUCTS = {
  client: {
    kind: 'client', appId: 'com.p2.desktop', productName: 'P2 Client', shortcutName: 'P2', crates: ['p2-client', 'p2-overlay'],
    installer: `P2-Client-Setup-${version}.exe`, deleteAppData: true,
  },
  server: {
    kind: 'server', appId: 'com.p2.server', productName: 'P2 Server', shortcutName: 'P2 Server', crates: ['p2-server', 'p2-server-console'],
    installer: `P2-Server-Setup-${version}.exe`, deleteAppData: false,
  },
}

function run(file, args, options = {}) {
  execFileSync(file, args, { stdio: 'inherit', windowsHide: true, ...options })
}

function firstExisting(list) { return list.filter(Boolean).find(p => existsSync(p)) }
function globDirs(dir, re) { try { return readdirSync(dir).filter(n => re.test(n)).map(n => join(dir, n)) } catch { return [] } }

function locateTools() {
  const cache = join(process.env.LOCALAPPDATA || '', 'electron-builder', 'Cache')
  const nsisDirs = [...globDirs(join(cache, 'nsis'), /^nsis-3/), ...globDirs(cache, /^nsis-3/).flatMap(d => [d, ...globDirs(d, /^nsis-3/)])]
  const makensis = firstExisting([process.env.P2_MAKENSIS, ...nsisDirs.map(d => join(d, 'makensis.exe'))])
  if (!makensis) throw new Error('makensis bulunamadı. electron-builder önbelleğinde yoksa NSIS 3 kur ve P2_MAKENSIS ver.')
  const resDirs = globDirs(cache, /^nsis-resources/).flatMap(d => [d, ...globDirs(d, /^nsis-resources/)])
  const plugins = firstExisting([process.env.P2_NSIS_PLUGINS, ...resDirs.map(d => join(d, 'plugins', 'x86-unicode'))])
  if (!plugins || !existsSync(join(plugins, 'WinShell.dll')) || !existsSync(join(plugins, 'UAC.dll'))) throw new Error('NSIS eklentileri (WinShell, UAC, StdUtils) bulunamadı; P2_NSIS_PLUGINS ver.')
  const include = firstExisting([process.env.P2_NSIS_INCLUDE, join(repo, 'node_modules', 'app-builder-lib', 'templates', 'nsis', 'include')])
  if (!include || !existsSync(join(include, 'StdUtils.nsh'))) throw new Error('StdUtils.nsh/UAC.nsh bulunamadı (node_modules/app-builder-lib); P2_NSIS_INCLUDE ver.')
  return { makensis, plugins, include }
}

function dirSize(dir) {
  let total = 0
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    const s = statSync(p)
    total += s.isDirectory() ? dirSize(p) : s.size
  }
  return total
}

function exeVersion(file) {
  try {
    return execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-Item -LiteralPath '${file.replace(/'/g, "''")}').VersionInfo.ProductVersion`], { encoding: 'utf8', windowsHide: true }).trim()
  } catch { return '' }
}

function newestServerIcon() {
  const candidates = globDirs(join(release, 'build'), /^p2-server-console-/).map(d => join(d, 'out', 'icon-server.ico')).filter(existsSync)
  candidates.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
  return candidates[0]
}

function stage(product, dir) {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  const put = (from, to) => {
    if (!existsSync(from)) throw new Error(`Paket girdisi eksik: ${from}`)
    mkdirSync(dirname(join(dir, to)), { recursive: true })
    copyFileSync(from, join(dir, to))
  }
  if (product.kind === 'client') {
    // Kısayollar ve görev çubuğu sabitlemeleri Electron'daki exe adına işaret ediyor: "P2 Client.exe".
    put(join(release, 'p2-client.exe'), 'P2 Client.exe')
    put(join(release, 'p2-overlay.exe'), 'p2-overlay.exe')
    // Enjektör + C++ DLL (scripts/collect-overlay.mjs → native/overlay/dist); Rust istemcisi
    // <exe klasörü>\overlay altında arar (p2-overlay-client Paths::locate).
    const overlay = process.env.P2_OVERLAY_DIST || join(repo, 'native', 'overlay', 'dist')
    put(join(overlay, 'p2inject64.exe'), 'overlay/p2inject64.exe')
    put(join(overlay, 'p2overlay64.dll'), 'overlay/p2overlay64.dll')
  } else {
    // Konsol Electron'daki gibi "P2 Server.exe" (kısayollar); API/hizmet ikilisi p2-server.exe.
    put(join(release, 'p2-server-console.exe'), 'P2 Server.exe')
    put(join(release, 'p2-server.exe'), 'p2-server.exe')
    const winsw = firstExisting([process.env.P2_WINSW, join(repo, '.server-service', 'P2ServerService.exe'), join(repo, 'node_modules', 'node-windows', 'bin', 'winsw', 'winsw.exe')])
    if (!winsw) throw new Error('WinSW (P2ServerService.exe) bulunamadı; P2_WINSW ver.')
    put(winsw, 'P2ServerService.exe')
    for (const name of ['P2ServerService.xml', 'p2-server-install.bat', 'p2-server-remove.bat', 'prepare-service-config.ps1']) put(join(rustDir, 'deploy', 'windows', name), name)
  }
}

export function latestYml(installerPath, installerName, ver, date = new Date()) {
  const bytes = readFileSync(installerPath)
  const sha512 = createHash('sha512').update(bytes).digest('base64')
  // electron-builder (js-yaml dump) biçimi birebir: tarih tek tırnaklı ISO.
  return `version: ${ver}\nfiles:\n  - url: ${installerName}\n    sha512: ${sha512}\n    size: ${bytes.length}\npath: ${installerName}\nsha512: ${sha512}\nreleaseDate: '${date.toISOString()}'\n`
}

function build(product, tools) {
  const out = join(outRoot, product.kind)
  const stageDir = join(out, 'stage')
  stage(product, stageDir)
  for (const exe of product.kind === 'client' ? ['P2 Client.exe'] : ['P2 Server.exe', 'p2-server.exe']) {
    const found = exeVersion(join(stageDir, exe))
    if (found && found !== version) throw new Error(`${exe} sürümü ${found}, kurulum ${version}. --skip-cargo ile eski ikili paketlenmesin: önce derle.`)
  }
  const icon = product.kind === 'client' ? join(repo, 'build', 'icon.ico') : newestServerIcon()
  if (!icon || !existsSync(icon)) throw new Error(`${product.kind} simgesi bulunamadı (sunucu: p2-server-console derlemesinin OUT_DIR'ındaki icon-server.ico)`)
  const guid = uuidV5(product.appId)
  const quad = [...version.split(/[-+]/)[0].split('.'), '0', '0', '0', '0'].slice(0, 4).join('.')
  const outfile = join(out, product.installer)
  rmSync(outfile, { force: true })
  const defines = {
    PRODUCT_KIND: product.kind,
    PRODUCT_NAME: product.productName,
    PRODUCT_FILENAME: product.productName,
    APP_FILENAME: product.productName,
    APP_ID: product.appId,
    APP_GUID: guid,
    APP_DESCRIPTION: pkg.description || '',
    APP_PACKAGE_NAME: pkg.name,
    COMPANY_NAME: company,
    COPYRIGHT_YEAR: String(new Date().getFullYear()),
    VERSION: version,
    VERSION_QUAD: quad,
    SHORTCUT_NAME: product.shortcutName,
    UNINSTALL_DISPLAY_NAME: `${product.productName} ${version}`,
    INSTALLER_ICON: icon,
    STAGE_DIR: stageDir,
    OUTFILE: outfile,
    PLUGINS_DIR: tools.plugins,
    EB_INCLUDE_DIR: tools.include,
    UNPACKED_SIZE_KB: String(Math.ceil(dirSize(stageDir) / 1024)),
  }
  if (product.deleteAppData) defines.DELETE_APP_DATA_ON_UNINSTALL = ''
  const args = ['-INPUTCHARSET', 'UTF8', '-V3', '-WX']
  for (const [k, v] of Object.entries(defines)) args.push(v === '' ? `-D${k}` : `-D${k}=${v}`)
  args.push(join(here, 'nsis', 'p2-installer.nsi'))
  const log = join(out, 'makensis.log')
  console.log(`\n${product.productName} ${version}: makensis…`)
  let text = ''
  try {
    text = execFileSync(tools.makensis, args, { cwd: join(here, 'nsis'), encoding: 'utf8', windowsHide: true, maxBuffer: 64 * 1024 * 1024 })
  } catch (error) {
    writeFileSync(log, `${error.stdout || ''}\n${error.stderr || ''}`)
    throw new Error(`makensis başarısız (${log}):\n${String(error.stdout || '').split('\n').slice(-25).join('\n')}\n${error.stderr || ''}`)
  }
  writeFileSync(log, text)
  const yml = latestYml(outfile, product.installer, version)
  // latest.yml en son yazılır: yarım paket hiçbir zaman ilan edilmez.
  writeFileSync(join(out, 'latest.yml'), yml)
  writeFileSync(join(out, 'build-info.json'), JSON.stringify({ product: product.kind, version, appId: product.appId, guid, installer: product.installer, stage: stageDir, defines }, null, 2))
  const mb = (statSync(outfile).size / 1048576).toFixed(1)
  console.log(`  ${outfile} · ${mb} MB\n  ${join(out, 'latest.yml')}`)
}

const isMain = resolve(process.argv[1] || '') === resolve(here, 'build-installers.mjs')
if (isMain) {
  const tools = locateTools()
  console.log(`makensis: ${tools.makensis}\neklentiler: ${tools.plugins}\nbaşlıklar: ${tools.include}`)
  if (!flag('skip-cargo')) {
    const crates = products.flatMap(p => PRODUCTS[p].crates)
    console.log(`cargo build --release ${crates.map(c => `-p ${c}`).join(' ')} (P2_PRODUCT_VERSION=${version})`)
    run('cargo', ['build', '--release', '--target-dir', targetDir, ...crates.flatMap(c => ['-p', c])], { cwd: rustDir, env: { ...process.env, P2_PRODUCT_VERSION: version } })
  }
  for (const p of products) build(PRODUCTS[p], tools)
  console.log(`\nTamam. Yayınlamak için: node rust/packaging/publish.mjs --version ${version}`)
}
