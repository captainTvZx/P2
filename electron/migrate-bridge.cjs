// Electron → Rust taşıma köprüsü (0.10.x "köprü" sürümü; bkz. docs/rust-gecis-plani.md Faz 8).
//
// Arayüz tercihleri Chromium'un localStorage'ında (LevelDB) duruyor; Rust istemcisi onu okuyamaz.
// Bu modül ana pencerenin localStorage'ını %APPDATA%\P2\migrate.json dosyasına yazar. Rust istemcisi
// ilk açılışta (profili boşsa) bu dosyayı, safeStorage ile şifreli session.bin'i (anahtar "Local
// State"te, DPAPI korumalı), library/library.json'u ve call-widget.json'u kendi profiline alır
// (rust/crates/p2-client-core/src/migrate.rs). Oturum jetonu bu dosyaya YAZILMAZ: düz metin hiçbir
// zaman diske inmez, Rust tarafı session.bin'i doğrudan çözer.
//
// Ne zaman yazılır: ana pencere yüklendikten 3 sn sonra, sonra dakikada bir (yalnız değiştiyse) ve
// güncelleme kurulmadan hemen önce (updater.cjs `setBeforeInstall`). Boş localStorage asla dolu bir
// anlık görüntünün üzerine yazılmaz.

const fs = require('node:fs')
const path = require('node:path')

const FORMAT = 'p2-migrate'
const FORMAT_VERSION = 1

function migrationPath(userData) { return path.join(userData, 'migrate.json') }

// Renderer'daki localStorage → { anahtar: metin }.
async function readLocalStorage(webContents) {
  const raw = await webContents.executeJavaScript('JSON.stringify(Object.fromEntries(Object.keys(localStorage).map(key => [key, localStorage.getItem(key)])))', true)
  const parsed = JSON.parse(raw || '{}')
  return Object.fromEntries(Object.entries(parsed && typeof parsed === 'object' ? parsed : {}).filter(([, value]) => typeof value === 'string'))
}

function readJson(file) { try { return JSON.parse(fs.readFileSync(file, 'utf8')) } catch { return null } }

function buildDocument({ storage, userData, appVersion }) {
  const appearance = readJson(path.join(userData, 'appearance.json'))
  return {
    format: FORMAT,
    version: FORMAT_VERSION,
    app: appVersion || '',
    writtenAt: new Date().toISOString(),
    userData,
    localStorage: storage,
    // Etkin temanın açık/koyu kipi (appearance.json yoksa Rust varsayılanı koyu).
    appearanceMode: appearance?.mode === 'light' ? 'light' : appearance?.mode === 'dark' ? 'dark' : null,
    // Oturum: Rust tarafı bu dosyayı "Local State"teki anahtarla çözer (Chromium OSCrypt v10).
    session: fs.existsSync(path.join(userData, 'session.bin')) ? { kind: 'electron-safestorage', file: 'session.bin', keyFile: 'Local State' } : null,
    library: fs.existsSync(path.join(userData, 'library', 'library.json')) ? 'library/library.json' : null,
  }
}

function writeAtomic(file, text) {
  const tmp = `${file}.tmp`
  fs.writeFileSync(tmp, text)
  fs.renameSync(tmp, file)
}

let lastStorage = ''
// Anlık görüntü alır; true: dosya güncel.
async function snapshot(webContents, { userData, appVersion, log } = {}) {
  if (!webContents || webContents.isDestroyed?.() || !userData) return false
  let storage
  try { storage = await readLocalStorage(webContents) } catch (error) { log?.(`Taşıma dosyası için ayarlar okunamadı: ${error?.message || error}`); return false }
  if (!Object.keys(storage).length) return false
  const key = JSON.stringify(storage)
  const file = migrationPath(userData)
  if (key === lastStorage && fs.existsSync(file)) return true
  try {
    writeAtomic(file, JSON.stringify(buildDocument({ storage, userData, appVersion }), null, 2))
    lastStorage = key
    return true
  } catch (error) { log?.(`migrate.json yazılamadı: ${error?.message || error}`); return false }
}

// Ana pencereye bağlanır; dönen işlev anlık görüntüyü hemen alır (güncellemeden önce beklenir).
function attach(window, options) {
  const webContents = window.webContents
  const run = () => snapshot(webContents, options)
  webContents.on('did-finish-load', () => { setTimeout(() => { void run() }, 3000) })
  const timer = setInterval(() => { void run() }, 60_000)
  timer.unref?.()
  window.once('closed', () => clearInterval(timer))
  return run
}

module.exports = { FORMAT, FORMAT_VERSION, migrationPath, readLocalStorage, buildDocument, snapshot, attach }
