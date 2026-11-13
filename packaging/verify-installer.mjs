// Kurulum paketini ÇALIŞTIRMADAN doğrular (bu makinede kurulum yapılmaz):
//   node packaging/verify-installer.mjs [--out DIR] [--product client|server|all]
//
// 1. PE sürüm bilgisi (ProductName/ProductVersion/CompanyName) — PowerShell VersionInfo.
// 2. NSIS veri bloğu açılır: ilk başlık ("NullsoftInst") bulunur, katı LZMA akışı 7za ile çözülür
//    (packaging/vendor/7zip/7za.exe; P2_7ZA ile değiştirilebilir).
// 3. Hazırlık klasöründeki her dosya bayt bayt (boy önekiyle) pakette mi.
// 4. Kurulum/kaldırma betiği metinleri: kayıt anahtarları (GUID), AppUserModelID, kısayol adı,
//    kaldırıcı adı, güncelleyici argümanları (--updated, --force-run, /S, /D=, --keep-shortcuts…).
// 5. latest.yml: biçim (electron-builder) + sha512/boy dosyayla aynı.

import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { fileURLToPath } from 'node:url'

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback
}
const outRoot = resolve(arg('out', 'C:/p2-rust-target/dist'))
const which = arg('product', 'all')
const products = which === 'all' ? ['client', 'server'] : [which]
const sevenZip = process.env.P2_7ZA || fileURLToPath(new URL('./vendor/7zip/7za.exe', import.meta.url))
if (!existsSync(sevenZip)) throw new Error(`7za bulunamadı: ${sevenZip} (P2_7ZA ver)`)

let failed = 0
let passed = 0
function check(ok, label, detail = '') {
  if (ok) { passed++; console.log(`  ✓ ${label}`) } else { failed++; console.log(`  ✗ ${label}${detail ? ` — ${detail}` : ''}`) }
}

function files(dir, base = dir) {
  return readdirSync(dir).flatMap(name => {
    const p = join(dir, name)
    return statSync(p).isDirectory() ? files(p, base) : [relative(base, p)]
  })
}

function versionInfo(file) {
  const ps = `$v=(Get-Item -LiteralPath '${file.replace(/'/g, "''")}').VersionInfo; @{p=$v.ProductName;v=$v.ProductVersion;c=$v.CompanyName;d=$v.FileDescription;f=$v.FileVersion} | ConvertTo-Json -Compress`
  return JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-Command', ps], { encoding: 'utf8', windowsHide: true }))
}

/** NSIS katı LZMA akışını çözer (ilk başlıktan sonra 5d… özellik baytlarıyla başlar). */
function inflate(stream, work, name) {
  if (stream[0] !== 0x5d) throw new Error(`katı LZMA değil (ilk bayt ${stream[0].toString(16)})`)
  mkdirSync(work, { recursive: true })
  const lzma = join(work, `${name}.lzma`)
  // .lzma kabı: 5 bayt özellik + 8 bayt "boyu bilinmiyor" + akış.
  writeFileSync(lzma, Buffer.concat([stream.subarray(0, 5), Buffer.alloc(8, 0xff), stream.subarray(5)]))
  try { execFileSync(sevenZip, ['e', lzma, `-o${work}`, '-y'], { stdio: 'ignore', windowsHide: true }) } catch { /* akış sonu CRC'si "hata" sayılır; çıktı tamdır */ }
  const out = join(work, name)
  if (!existsSync(out)) throw new Error('7za çözemedi')
  return readFileSync(out)
}

/** Kurulum: [u32 başlık boyu][başlık][u32 boy][dosya]…; kaldırıcı verisi (kurulumda WriteUninstaller
 *  ile yazılan exe'nin başlığı) bloğun içinde kendi ilk başlığı ve katı akışıyla durur. */
function unpackNsis(exe, work) {
  const bytes = readFileSync(exe)
  const sig = bytes.indexOf(Buffer.from('NullsoftInst'))
  if (sig < 8 || bytes.readUInt32LE(sig - 4) !== 0xdeadbeef) throw new Error('NSIS ilk başlığı yok')
  const blob = inflate(bytes.subarray(sig + 20), work, 'data')
  const unSig = blob.indexOf(Buffer.concat([Buffer.from('01000000efbeadde', 'hex'), Buffer.from('NullsoftInst')]))
  const uninstaller = unSig >= 0 ? inflate(blob.subarray(unSig + 28), work, 'uninst') : Buffer.alloc(0)
  return { blob, uninstaller }
}

const u16 = text => Buffer.from(text, 'utf16le')

for (const product of products) {
  const out = join(outRoot, product)
  const info = JSON.parse(readFileSync(join(out, 'build-info.json'), 'utf8'))
  const exe = join(out, info.installer)
  const d = info.defines
  console.log(`\n${d.PRODUCT_NAME} ${info.version} — ${exe}`)

  const vi = versionInfo(exe)
  check(vi.p === d.PRODUCT_NAME && vi.c === d.COMPANY_NAME && vi.f === info.version, `sürüm bilgisi: ${vi.p} ${vi.v} / ${vi.c}`)

  const work = join(tmpdir(), `p2-verify-${product}-${process.pid}`)
  rmSync(work, { recursive: true, force: true })
  const { blob, uninstaller } = unpackNsis(exe, work)
  check(uninstaller.length > 0, `kaldırıcı başlığı (${(uninstaller.length / 1024).toFixed(0)} KB)`)
  const anywhere = text => blob.includes(u16(text)) || uninstaller.includes(u16(text))
  console.log(`  (açılan veri ${(blob.length / 1048576).toFixed(1)} MB)`)

  // Dosyalar: hazırlık klasöründeki her dosya boy önekiyle pakette.
  const staged = files(info.stage)
  for (const name of staged) {
    const content = readFileSync(join(info.stage, name))
    const prefix = Buffer.alloc(4)
    prefix.writeUInt32LE(content.length)
    const at = blob.indexOf(Buffer.concat([prefix, content.subarray(0, Math.min(content.length, 4096))]))
    const whole = at >= 0 && blob.subarray(at + 4, at + 4 + content.length).equals(content)
    const fileName = name.split(/[\\/]/).pop()
    check(whole && blob.includes(u16(fileName)), `dosya: ${name} (${content.length} bayt)`)
  }

  // Betik metinleri (NSIS Unicode dize tablosu; değişkenler özel kodla bölünür, sabit parçalar aranır).
  const strings = [
    `Software\\${d.APP_GUID}`,
    `Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${d.APP_GUID}`,
    d.APP_ID,
    `\\${d.SHORTCUT_NAME}.lnk`,
    `Uninstall ${d.PRODUCT_FILENAME}.exe`,
    `${d.PRODUCT_FILENAME}.exe`,
    d.UNINSTALL_DISPLAY_NAME,
    d.COMPANY_NAME,
    'InstallLocation', 'KeepShortcuts', 'ShortcutName', 'UninstallString', 'QuietUninstallString', 'DisplayIcon', 'EstimatedSize',
    'updated', 'force-run', 'keep-shortcuts', 'delete-app-data', 'no-desktop-shortcut', 'allusers', 'currentuser',
    '/D=', '/currentuser', '/allusers', ' --keep-shortcuts', ' --updated', ' /S /KEEP_APP_DATA ', '/S',
    `\\Programs`, d.APP_FILENAME, '--updated', 'old-uninstaller.exe',
  ]
  if (product === 'client') strings.push('Software\\Classes\\p2', 'p2-desktop')
  if (product === 'server') strings.push('P2ServerService.xml', '<executable>', '\\p2-server.exe</executable>', '<arguments>--p2-server</arguments>', 'roll-by-size', 'ProgramData')
  for (const s of strings) check(anywhere(s), `metin: ${JSON.stringify(s)}`)
  // ProgramData'ya dokunan bir silme komutu olmamalı (sunucu verisi).
  if (product === 'server') check(!blob.includes(u16('ProgramData\\P2"')) , 'ProgramData\\P2 silinmiyor')

  // latest.yml
  const yml = readFileSync(join(out, 'latest.yml'), 'utf8')
  const bytes = readFileSync(exe)
  const sha = createHash('sha512').update(bytes).digest('base64')
  const re = new RegExp(`^version: ${info.version.replace(/\./g, '\\.')}\\nfiles:\\n  - url: ${info.installer}\\n    sha512: ([A-Za-z0-9+/=]+)\\n    size: (\\d+)\\npath: ${info.installer}\\nsha512: ([A-Za-z0-9+/=]+)\\nreleaseDate: '\\d{4}-\\d\\d-\\d\\dT\\d\\d:\\d\\d:\\d\\d\\.\\d{3}Z'\\n$`)
  const m = yml.match(re)
  check(Boolean(m), 'latest.yml biçimi (electron-builder)')
  check(m && m[1] === sha && m[3] === sha && Number(m[2]) === bytes.length, 'latest.yml sha512 + boy dosyayla aynı')

  rmSync(work, { recursive: true, force: true })
}

console.log(`\n${passed} geçti, ${failed} kaldı`)
process.exit(failed ? 1 : 0)
