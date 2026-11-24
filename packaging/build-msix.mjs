// P2 Client — Microsoft Store paketi (.msix). İmza gerekmez: Store gönderimde paketi kendisi imzalar.
//
//   node packaging/build-msix.mjs                  → cargo build + overlay (yoksa) + paket
//   node packaging/build-msix.mjs --skip-cargo     → var olan ikililerle paketle
//   node packaging/build-msix.mjs --version 1.0.8  → sürüm (varsayılan kök Cargo.toml)
//
// Kimlik: packaging/msix/store.json (Partner Center > Product identity). Bildirim şablonu:
// packaging/msix/AppxManifest.xml. Store dört haneli sürüm ister ve son hane 0 olmalı: 1.0.8 → 1.0.8.0.
//
// Araçlar: Windows SDK makeappx.exe + makepri.exe (P2_WINSDK_BIN ile değiştirilebilir).
// Çıktı: <target>/dist/msix/PlayThrough-<v>.0-x64.msix + stage/ (gevşek düzen).
//
// Yerel deneme (Ayarlar > Sistem > Geliştiriciler için > Geliştirici Modu açık olmalı), imzasız:
//   Add-AppxPackage -Register "<stage-* klasörü>\AppxManifest.xml"  (betik sonunda tam yolu yazar; kaldırmak: Get-AppxPackage *PlayThrough* | Remove-AppxPackage)
// Gönderim: .msix dosyası Partner Center > Gönderim > Paketler'e yüklenir.

import { execFileSync } from 'node:child_process'
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { productVersion } from './version.mjs'
import { buildOverlay, overlayDist, overlayFiles } from './build-overlay.mjs'

const here = dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
const repo = resolve(here, '..')

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  return i >= 0 && process.argv[i + 1] && !process.argv[i + 1].startsWith('--') ? process.argv[i + 1] : fallback
}
const flag = name => process.argv.includes(`--${name}`)

const version = productVersion(arg('version'))
const m = version.match(/^(\d+)\.(\d+)\.(\d+)$/)
if (!m) throw new Error(`Store sürümü X.Y.Z olmalı (ön sürüm eki olmaz): ${version}`)
const quad = `${m[1]}.${m[2]}.${m[3]}.0`
const targetDir = resolve(arg('target-dir', process.env.CARGO_TARGET_DIR || 'C:/p2-rust-target'))
const release = join(targetDir, 'release')
const out = resolve(arg('out', join(targetDir, 'dist', 'msix')))
// Her derleme yeni bir gevşek düzen klasörü kullanır: yerel denemede `Add-AppxPackage -Register`
// paketi bu klasörden çalıştırır; açık uygulamanın klasörü silinmez/taşınmaz (Windows, içinde
// çalışan exe olan klasörü taşımaya izin veriyor, silme ise yarıda kalıp kurulumu bozuyor).
const stageDir = join(out, `stage-${Date.now()}`)
const store = JSON.parse(readFileSync(join(here, 'msix', 'store.json'), 'utf8'))

function sdkTool(name) {
  const roots = [process.env.P2_WINSDK_BIN, 'C:/Program Files (x86)/Windows Kits/10/bin'].filter(Boolean)
  for (const root of roots) {
    if (existsSync(join(root, name))) return join(root, name)
    const versions = existsSync(root) ? readdirSync(root).filter(v => /^10\./.test(v)).sort((a, b) => b.localeCompare(a, undefined, { numeric: true })) : []
    for (const v of versions) if (existsSync(join(root, v, 'x64', name))) return join(root, v, 'x64', name)
  }
  throw new Error(`${name} bulunamadı: Windows SDK kur ya da P2_WINSDK_BIN ver`)
}

function run(file, args, options = {}) {
  execFileSync(file, args, { stdio: 'inherit', windowsHide: true, ...options })
}

function exeVersion(file) {
  try {
    return execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-Item -LiteralPath '${file.replace(/'/g, "''")}').VersionInfo.ProductVersion`], { encoding: 'utf8', windowsHide: true }).trim()
  } catch { return '' }
}

const xml = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

// Store simgeleri assets/icon.png'den (System.Drawing ile). Kare olmayanlar ortalanır, kenar saydam.
// targetsize-* / altform-unplated: görev çubuğu ve Başlat'ta plakasız, keskin simge (resources.pri ile seçilir).
function renderAssets(dir) {
  mkdirSync(dir, { recursive: true })
  const src = join(repo, 'assets', 'icon.png')
  const jobs = [
    ['StoreLogo.png', 50, 50, 1],
    ['Square44x44Logo.png', 44, 44, 1],
    ['Square150x150Logo.png', 150, 150, 0.66],
    ['Wide310x150Logo.png', 310, 150, 0.66],
  ]
  for (const size of [16, 24, 32, 48, 256]) {
    jobs.push([`Square44x44Logo.targetsize-${size}.png`, size, size, 1])
    jobs.push([`Square44x44Logo.targetsize-${size}_altform-unplated.png`, size, size, 1])
  }
  const lines = jobs.map(([name, w, h, scale]) => `Write-P2Logo '${join(dir, name).replace(/'/g, "''")}' ${w} ${h} ${scale}`)
  const script = [
    'Add-Type -AssemblyName System.Drawing',
    `$src=[System.Drawing.Image]::FromFile('${src.replace(/'/g, "''")}')`,
    'function Write-P2Logo($path,$w,$h,$scale){',
    '  $bmp=New-Object System.Drawing.Bitmap $w,$h',
    '  $g=[System.Drawing.Graphics]::FromImage($bmp)',
    '  $g.InterpolationMode=[System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic',
    '  $g.SmoothingMode=[System.Drawing.Drawing2D.SmoothingMode]::HighQuality',
    '  $g.PixelOffsetMode=[System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality',
    '  $g.Clear([System.Drawing.Color]::Transparent)',
    '  $s=[Math]::Round([Math]::Min($w,$h)*$scale)',
    '  $g.DrawImage($src,[int](($w-$s)/2),[int](($h-$s)/2),$s,$s)',
    '  $g.Dispose(); $bmp.Save($path,[System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose() }',
    ...lines,
    '$src.Dispose()',
  ].join('\n')
  const ps1 = join(out, 'render-assets.ps1')
  writeFileSync(ps1, '\ufeff' + script)
  run('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ps1])
  rmSync(ps1, { force: true })
}

// Yerelde kayıtlı deneme paketinin klasörü (yoksa boş).
function registeredLocation() {
  try {
    return execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-AppxPackage -Name '${store.identityName}').InstallLocation`], { encoding: 'utf8', windowsHide: true }).trim()
  } catch { return '' }
}

// Klasörde açık (yazmaya kilitli) bir dosya var mı: her dosya r+ ile açılmayı dener.
function anyLocked(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) { if (anyLocked(p)) return true; continue }
    try { closeSync(openSync(p, 'r+')) } catch { return true }
  }
  return false
}

function stage() {
  // Eski düzen klasörleri: yalnız kayıtlı deneme kurulumunun klasörü değilse ve hiç kilitli dosya
  // yoksa silinir. Önce bir dosyaya yazma kilidi denenir; açık exe varsa klasöre dokunulmaz.
  const registered = registeredLocation()
  for (const name of readdirSync(out)) {
    const dir = join(out, name)
    if (!/^stage(-\d+|\.old-\d+)?$/.test(name) || dir === stageDir || (registered && resolve(registered).toLowerCase() === resolve(dir).toLowerCase())) continue
    if (anyLocked(dir)) { console.log(`  atlandı (kullanımda): ${dir}`); continue }
    try { rmSync(dir, { recursive: true, force: true }) } catch { console.log(`  silinemedi: ${dir}`) }
  }
  mkdirSync(stageDir, { recursive: true })
  const put = (from, to) => {
    if (!existsSync(from)) throw new Error(`Paket girdisi eksik: ${from}`)
    mkdirSync(dirname(join(stageDir, to)), { recursive: true })
    copyFileSync(from, join(stageDir, to))
  }
  // NSIS kurulumuyla aynı düzen (p2-overlay-client Paths::locate: <exe klasörü>\overlay).
  put(join(release, 'p2-client.exe'), 'P2 Client.exe')
  put(join(release, 'p2-overlay.exe'), 'p2-overlay.exe')
  const overlay = process.env.P2_OVERLAY_DIST || overlayDist
  put(join(overlay, 'p2inject64.exe'), 'overlay/p2inject64.exe')
  put(join(overlay, 'p2overlay64.dll'), 'overlay/p2overlay64.dll')
  // Masaüstü oyun kısayollarının simgesi (library_dialogs.rs create_shortcut LocalState'e kopyalar).
  put(join(repo, 'assets', 'icon.ico'), 'p2.ico')
  renderAssets(join(stageDir, 'Assets'))
  const template = readFileSync(join(here, 'msix', 'AppxManifest.xml'), 'utf8')
  const fields = {
    IDENTITY_NAME: store.identityName,
    PUBLISHER: store.publisher,
    PUBLISHER_DISPLAY_NAME: store.publisherDisplayName,
    DISPLAY_NAME: store.displayName,
    DESCRIPTION: store.description,
    VERSION: quad,
  }
  const manifest = template.replace(/\{\{(\w+)\}\}/g, (_, k) => {
    if (!(k in fields)) throw new Error(`AppxManifest alanı bilinmiyor: ${k}`)
    return xml(fields[k])
  })
  writeFileSync(join(stageDir, 'AppxManifest.xml'), manifest)
}

function makePri() {
  const makepri = sdkTool('makepri.exe')
  const cfg = join(out, 'priconfig.xml')
  run(makepri, ['createconfig', '/cf', cfg, '/dq', 'tr-TR_en-US', '/pv', '10.0.0', '/o'])
  run(makepri, ['new', '/pr', stageDir, '/cf', cfg, '/mn', join(stageDir, 'AppxManifest.xml'), '/of', join(stageDir, 'resources.pri'), '/o'])
  rmSync(cfg, { force: true })
}

function pack() {
  const makeappx = sdkTool('makeappx.exe')
  const file = join(out, `${store.displayName.replace(/[^A-Za-z0-9]+/g, "")}-${quad}-x64.msix`)
  rmSync(file, { force: true })
  run(makeappx, ['pack', '/d', stageDir, '/p', file, '/o'])
  return file
}

const isMain = resolve(process.argv[1] || '') === resolve(here, 'build-msix.mjs')
if (isMain) {
  mkdirSync(out, { recursive: true })
  if (!process.env.P2_OVERLAY_DIST && (flag('rebuild-overlay') || overlayFiles.some(n => !existsSync(join(overlayDist, n))))) {
    console.log('overlay DLL + enjektör derleniyor (packaging/build-overlay.mjs)')
    buildOverlay()
  }
  if (!flag('skip-cargo')) {
    console.log(`cargo build --release -p p2-client -p p2-overlay (P2_PRODUCT_VERSION=${version})`)
    run('cargo', ['build', '--release', '--target-dir', targetDir, '-p', 'p2-client', '-p', 'p2-overlay'], { cwd: repo, env: { ...process.env, P2_PRODUCT_VERSION: version } })
  }
  const found = exeVersion(join(release, 'p2-client.exe'))
  if (found && found !== version) throw new Error(`p2-client.exe sürümü ${found}, paket ${version}: önce derle (--skip-cargo kullanma).`)
  console.log(`\n${store.displayName} ${quad}: hazırlanıyor → ${stageDir}`)
  stage()
  makePri()
  const file = pack()
  const mb = (statSync(file).size / 1048576).toFixed(1)
  console.log(`\nTamam: ${file} · ${mb} MB`)
  console.log(`Yerel deneme: Add-AppxPackage -Register "${join(stageDir, "AppxManifest.xml")}"`)
  console.log("Gönderim: .msix dosyasını Partner Center > Paketler bölümüne yükleyin.")
}
