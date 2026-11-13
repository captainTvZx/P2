// Faz 8 uçtan uca testi — kurulum paketleri, yayın düzeni ve Electron → Rust taşıması.
// Hiçbir kurulum ÇALIŞTIRILMAZ; kullanıcının %APPDATA%\P2, ProgramData\P2 ve kurulu uygulamalarına
// dokunulmaz (her şey C:/p2-pg/faz8 altında).
//
// Önkoşul: test PostgreSQL (:55432, C:/p2-pg/test-env.json), `cargo build --release -p p2-client
// -p p2-server`, paketler: `node rust/packaging/build-installers.mjs` (C:/p2-rust-target/dist).
//   node rust/tools/faz8-test.mjs
//
// [1] Yayın: publish.mjs → geçici P2_UPDATE_DIR; Rust sunucusu (:4161) /updates/client|server/…
//     latest.yml + kurulum exe'sini sunar; sha512/boy tutar; electron-updater ayrıştırır.
// [2] Köprü: gerçek Electron (node_modules/electron) geçici profilde safeStorage ile oturum yazar,
//     localStorage doldurur, electron/migrate-bridge.cjs migrate.json'u yazar.
// [3] Rust içe alma: `p2-client --p2-import-electron` (P2_PROFILE_DIR + P2_ELECTRON_PROFILE_DIR):
//     ayarlar, oturum (v10/AES-GCM çözülüp DPAPI ile yeniden yazılır), library.json, arama balonu.
// [4] İstemci, taşınan oturumla giriş ekranı GÖSTERMEDEN çalışma alanına açılır.
// [5] Güvenlik: P2_PROFILE_DIR verilip P2_ELECTRON_PROFILE_DIR verilmezse gerçek profile bakılmaz;
//     taşıma ikinci açılışta tekrarlanmaz.

import { spawn, execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { startRust } from './server/lib.mjs'

const REPO = 'C:/Users/capta/OneDrive/Desktop/P2'
const BIN = 'C:/p2-rust-target/release/'
const DIST = 'C:/p2-rust-target/dist'
const BASE = 'C:/p2-pg/faz8'
const PORT = 4161
const version = JSON.parse(readFileSync(join(DIST, 'client', 'build-info.json'), 'utf8')).version
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE, { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
const sha512 = buf => createHash('sha512').update(buf).digest('base64')

// ---------------------------------------------------------------- [1] yayın + sunum
console.log('\n[1] Yayın düzeni ve sunucu')
const UPDATES = BASE + '/updates'
execFileSync(process.execPath, [join(REPO, 'rust/packaging/publish.mjs'), '--skip-build', '--version', version], { env: { ...process.env, P2_UPDATE_DIR: UPDATES }, stdio: 'ignore' })
process.env.P2_UPDATE_DIR = UPDATES
const server = await startRust(PORT)
const api = server.base
try {
  const provider = createRequire(join(REPO, 'package.json'))('electron-updater/out/providers/Provider')
  for (const product of ['client', 'server']) {
    const res = await fetch(`${api}/updates/${product}/latest.yml`)
    const yml = await res.text()
    const name = yml.match(/^path: (.+)$/m)?.[1]
    check(`${product}: /updates/${product}/latest.yml`, res.ok && yml.startsWith(`version: ${version}\n`), name)
    const exe = Buffer.from(await (await fetch(`${api}/updates/${product}/${name}`)).arrayBuffer())
    const local = readFileSync(join(DIST, product, name))
    check(`${product}: kurulum indirildi, sha512 + boy latest.yml ile aynı`, exe.equals(local) && yml.includes(`sha512: ${sha512(exe)}`) && yml.includes(`size: ${exe.length}`), `${(exe.length / 1048576).toFixed(1)} MB`)
    const feed = new URL(`${api}/updates/${product}/`)
    const info = provider.parseUpdateInfo(yml, 'latest.yml', new URL('latest.yml', feed))
    const file = provider.findFile(provider.resolveFiles(info, feed), 'exe')
    check(`${product}: electron-updater kabul ediyor`, info.version === version && file.info.sha512 === sha512(exe))
  }
  const root = await (await fetch(`${api}/updates/latest.yml`)).text()
  check('kök latest.yml istemcinin (eski istemciler)', root.includes(`path: P2-Client-Setup-${version}.exe`))
  check('sunucu exe kökte de var (0.7.10 ve eski sunucular)', (await fetch(`${api}/updates/P2-Server-Setup-${version}.exe`, { method: 'HEAD' })).ok || (await fetch(`${api}/updates/P2-Server-Setup-${version}.exe`)).ok)

  // ---------------------------------------------------------------- [2] Electron köprüsü
  console.log('\n[2] Electron köprüsü (gerçek Electron, geçici profil)')
  const login = await (await fetch(`${api}/auth/login`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ identifier: 'p2test', password: 'Parola123', rememberMe: true }) })).json()
  check('test kullanıcısı giriş yaptı (yenileme jetonu)', typeof login.refreshToken === 'string' && login.refreshToken.length > 20)
  const ELECTRON = BASE + '/electron-profile'
  mkdirSync(ELECTRON, { recursive: true })
  const electronSettings = {
    language: 'en', closeAction: 'tray', desktopNotifications: false, voiceMode: 'ptt', voiceKey: 'KeyB', inputVolume: 70,
    libraryAutoDownload: false, libraryBandwidthMbps: 12, libraryOverlay: false,
    overlayHotkeys: { panel: 'Shift+3', screenshot: 'F11', mute: 'Shift+M', deafen: '', pushToTalk: '' }, overlayToastCorner: 'bottom-left', overlayScale: 1.25, overlayHud: true,
    workDays: [1, 2, 3], workStart: '08:30', workEnd: '17:30', outsideHoursStatus: 'BUSY', appearance: { themeId: 'p2-mavi' },
  }
  const input = {
    refreshToken: login.refreshToken,
    appVersion: '0.10.0',
    localStorage: { 'p2:settings': JSON.stringify(electronSettings), 'p2:apiUrl': api, 'p2:mode': 'client', 'p2:board-view': 'list', 'p2:sidebar-width': '300' },
    files: {
      'appearance.json': { vars: {}, mode: 'light' },
      'library/library.json': { version: 1, deviceId: 'faz8-device', root: 'D:\\P2ProjectLibrary', projects: { 'proj-1': { installedVersion: '1.2.3', folder: 'Oyun' } } },
      'library/hash-cache.json': {},
      'call-widget.json': { x: 40, y: 60 },
    },
  }
  writeFileSync(BASE + '/electron-input.json', JSON.stringify(input))
  const electronEnv = { ...process.env }
  delete electronEnv.ELECTRON_RUN_AS_NODE
  const made = execFileSync(join(REPO, 'node_modules/electron/dist/electron.exe'), [join(REPO, 'rust/tools/faz8/electron-profile.cjs'), ELECTRON, BASE + '/electron-input.json'], { env: electronEnv, timeout: 60000 }).toString()
  const result = JSON.parse(made.trim().split(/\r?\n/).pop())
  check('Electron safeStorage kullanılabilir', result.encryption === true)
  check('köprü migrate.json yazdı', result.snapshot === true && existsSync(ELECTRON + '/migrate.json'))
  check('değişmeyen ayarla yeniden yazmıyor', result.unchanged === true)
  const migrate = JSON.parse(readFileSync(ELECTRON + '/migrate.json', 'utf8'))
  check('migrate.json biçimi', migrate.format === 'p2-migrate' && migrate.version === 1 && migrate.app === '0.10.0' && migrate.localStorage['p2:apiUrl'] === api && migrate.appearanceMode === 'light' && migrate.session?.file === 'session.bin')
  check('migrate.json oturum jetonu içermiyor', !JSON.stringify(migrate).includes(login.refreshToken))
  const sessionBlob = readFileSync(ELECTRON + '/session.bin')
  check('Electron session.bin v10 biçiminde, düz metin yok', sessionBlob.subarray(0, 3).toString() === 'v10' && !sessionBlob.includes(Buffer.from(login.refreshToken.slice(0, 16))))
  const localState = JSON.parse(readFileSync(ELECTRON + '/Local State', 'utf8'))
  check('"Local State" DPAPI anahtarı', Buffer.from(localState.os_crypt?.encrypted_key || '', 'base64').subarray(0, 5).toString() === 'DPAPI')

  // ---------------------------------------------------------------- [3] Rust içe alma
  console.log('\n[3] Rust istemcisi içe alma (--p2-import-electron)')
  const PROFILE = BASE + '/rust-profile'
  const importEnv = { ...process.env, P2_PROFILE_DIR: PROFILE, P2_ELECTRON_PROFILE_DIR: ELECTRON }
  const report = JSON.parse(execFileSync(BIN + 'p2-client.exe', ['--p2-import-electron'], { env: importEnv, timeout: 30000 }).toString().trim().split(/\r?\n/).pop())
  check('rapor: ayarlar + oturum + kütüphane', report?.settings && report.session && report.library, JSON.stringify(report?.notes || []))
  const settings = JSON.parse(readFileSync(PROFILE + '/settings.json', 'utf8'))
  check('sunucu adresi ve kip', settings.apiUrl === api && settings.mode === 'client')
  check('dil, tema (appearance.json → açık)', settings.language === 'en' && settings.theme === 'light')
  check('uygulama: kapat → tepsi, bildirim kapalı', settings.app.closeAction === 'tray' && settings.app.desktopNotifications === false)
  check('ses: bas-konuş B, giriş %70', settings.media.voiceMode === 'ptt' && settings.media.voiceKey === 'b' && settings.media.inputVolume === 70)
  check('kütüphane: oto indirme kapalı, 12 MB/s, overlay kapalı', settings.library.autoDownload === false && settings.library.bandwidthMbps === 12 && settings.library.overlay === false)
  check('overlay: Shift+3/F11, sol alt, 1.25, HUD', settings.overlay.hotkeys.panel === 'Shift+3' && settings.overlay.hotkeys.screenshot === 'F11' && settings.overlay.toastCorner === 'bottom-left' && Math.abs(settings.overlay.scale - 1.25) < 1e-6 && settings.overlay.hud === true)
  check('uygunluk: Pzt–Çar 08:30–17:30, MEŞGUL', JSON.stringify(settings.availability.workDays) === '[1,2,3]' && settings.availability.workStart === '08:30' && settings.availability.outsideHoursStatus === 'BUSY')
  const lib = JSON.parse(readFileSync(PROFILE + '/library/library.json', 'utf8'))
  check('library.json (kök + kurulu oyun)', lib.root === 'D:\\P2ProjectLibrary' && lib.projects['proj-1']?.installedVersion === '1.2.3' && lib.deviceId === 'faz8-device')
  check('arama balonu konumu + ham localStorage', existsSync(PROFILE + '/call-widget.json') && JSON.parse(readFileSync(PROFILE + '/electron-localstorage.json', 'utf8'))['p2:board-view'] === 'list')
  const rustSession = readFileSync(PROFILE + '/session.bin')
  check('Rust session.bin DPAPI (düz metin yok, v10 değil)', !rustSession.includes(Buffer.from(login.refreshToken.slice(0, 16))) && rustSession.subarray(0, 3).toString() !== 'v10')
  const again = execFileSync(BIN + 'p2-client.exe', ['--p2-import-electron'], { env: importEnv, timeout: 30000 }).toString().trim().split(/\r?\n/).pop()
  check('ikinci açılışta taşıma tekrarlanmıyor', again === 'null')
  const isolated = execFileSync(BIN + 'p2-client.exe', ['--p2-import-electron'], { env: { ...process.env, P2_PROFILE_DIR: BASE + '/empty-profile', P2_ELECTRON_PROFILE_DIR: '' }, timeout: 30000 }).toString().trim().split(/\r?\n/).pop()
  check('ayrı profilde gerçek Electron profiline bakılmıyor', isolated === 'null' && !existsSync(BASE + '/empty-profile/settings.json'))

  // ---------------------------------------------------------------- [4] taşınan oturumla açılış
  console.log('\n[4] Taşınan oturumla açılış (giriş ekranı yok)')
  const lines = []
  const client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_ELECTRON_PROFILE_DIR: ELECTRON } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) lines.push(l) })
  const wait = async (re, ms) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(150) } return null }
  const opened = await wait(/ekran: (çalışma alanı|giriş|kurulum türü|sunucuya bağlanamadık)/, 30000)
  check('çalışma alanı açıldı (kullanıcı yeniden giriş yapmadı)', /ekran: çalışma alanı \(p2test/.test(opened || ''), opened || 'zaman aşımı')
  check('Windows alt sisteminde de izler stdout\'a geliyor', lines.length > 0)
  client.kill()
} finally {
  server.stop()
}

console.log(`\n${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
