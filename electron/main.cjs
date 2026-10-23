const { app, BrowserWindow, Menu, Notification, Tray, desktopCapturer, dialog, nativeImage, screen, session, shell, ipcMain, safeStorage, net } = require('electron')
const path = require('node:path')
const http = require('node:http')
const { startAppServer } = require('./appServer.cjs')
const updates = require('./updater.cjs')
const { P2_MAVI_CSS } = require('./p2-mavi.cjs')
const fs = require('node:fs')
const { fork, spawn, execFile, execFileSync } = require('node:child_process')
const { Client } = require('pg')
// Git arayüzü şimdilik pasif: köprü kaydedilmez, dosyalar ileride yeniden açılmak üzere duruyor.
const { registerLibraryIpc } = require('./patcher.cjs')
const overlayHost = require('./overlayHost.cjs')

const isDev = !app.isPackaged
const uiDemo = isDev && process.argv.includes('--ui-demo')
let apiProcess
let apiStarting = false
let widgetSkippedCall = false
// Paketlenmiş arayüzün adresi (bkz. appServer.cjs). Boşsa file:// kullanılır.
let appBase = ''
const PREFERRED_UI_PORT = 43217
function uiPortPath() { return path.join(app.getPath('userData'), 'ui-port.json') }
async function ensureAppServer() {
  if (isDev || appBase) return appBase
  let saved = PREFERRED_UI_PORT
  try { saved = Number(JSON.parse(fs.readFileSync(uiPortPath(), 'utf8')).port) || PREFERRED_UI_PORT } catch {}
  try {
    const started = await startAppServer(path.join(app.getAppPath(), 'dist'), saved)
    appBase = started.base
    // Tercih edilen port doluysa geçici bir port kullanılır ama kaydedilmez:
    // sonraki açılışta yine aynı köken denenir, ayarlar kalıcı olur.
    if (saved === PREFERRED_UI_PORT && started.port === saved) { try { fs.writeFileSync(uiPortPath(), JSON.stringify({ port: started.port })) } catch {} }
  } catch (error) { serverLog(`Arayüz sunucusu başlatılamadı: ${error?.message || error}`) }
  return appBase
}

// Arayüz eskiden file:// altında çalışıyordu; tarayıcı yerel ayarları kökene
// göre sakladığı için http kökenine geçerken bir kez taşınmaları gerekiyor.
// Gizli bir pencerede eski sayfa açılıp localStorage okunur ve saklanır.
function legacyPrefsPath() { return path.join(app.getPath('userData'), 'legacy-prefs.json') }
let legacyPrefs = null
function loadLegacyPrefs() {
  if (legacyPrefs) return legacyPrefs
  try { legacyPrefs = JSON.parse(fs.readFileSync(legacyPrefsPath(), 'utf8')) } catch { legacyPrefs = {} }
  return legacyPrefs
}
async function migrateLegacyPrefs() {
  if (isDev || fs.existsSync(legacyPrefsPath())) return
  const hidden = new BrowserWindow({ show: false, webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false } })
  try {
    await hidden.loadFile(path.join(__dirname, '..', 'dist', 'index.html'))
    const data = await hidden.webContents.executeJavaScript('JSON.stringify(Object.fromEntries(Object.entries(localStorage)))')
    fs.writeFileSync(legacyPrefsPath(), data || '{}')
  } catch (error) {
    serverLog(`Eski tercihler taşınamadı: ${error?.message || error}`)
    try { fs.writeFileSync(legacyPrefsPath(), '{}') } catch {}
  } finally { if (!hidden.isDestroyed()) hidden.destroy() }
}
// Arayüz yerel sunucudan (http://127.0.0.1) yüklenir. file:// kökeninde
// gömülü oynatıcılar tam ekrana geçemiyor ve bazı güvenli bağlam özellikleri
// kapalı kalıyordu; http kökeni normal bir web sayfası gibi davranır.
function loadRenderer(window, query = '') {
  if (isDev) { window.loadURL(`http://localhost:5173/${query ? '?' + query : ''}`); return }
  if (appBase) { window.loadURL(`${appBase}/index.html${query ? '?' + query : ''}`); return }
  window.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), query ? { search: query } : undefined)
}

// Sunucu ayrı bir süreçtir: "P2.exe --p2-server" penceresiz çalışır, tepsi
// simgesiyle yaşar ve P2 kapansa da sürer; açılışta kendiliğinden başlar.
// Kendi userData alt klasörünü kullanır ki arayüzle çakışmasın, ama
// veritabanı yapılandırmasını arayüzün yazdığı yerden okur.
// Not: sunucu süreci userData'yı DEĞİŞTİRMEZ. safeStorage'ın anahtarı userData'daki
// "Local State" dosyasında durur; ayrı klasör ayrı anahtar demek olur ve arayüzün
// yazdığı database.bin çözülemezdi. Tek örnek denetimi port yoklamasıyla yapılır.
const serverMode = process.argv.includes('--p2-server')
const CLIENT_APP_ID = isDev ? 'com.p2.desktop.dev' : 'com.p2.desktop'
const SERVER_APP_ID = 'com.p2.server'
// Bunu app.ready'den önce yap. Özellikle geliştirme modunda süreç electron.exe
// olsa bile Windows görev çubuğu pencereyi Electron yerine P2 kimliğiyle eşler.
app.setName(serverMode ? 'P2 Server' : 'P2')
if (process.platform === 'win32') app.setAppUserModelId(serverMode ? SERVER_APP_ID : CLIENT_APP_ID)
const baseUserData = app.getPath('userData')
const API_PORT = Number(process.env.PORT || 4000)
// 0.7.11'den itibaren dağıtılan P2 Client backend taşımaz. Kaynak ağacındaki
// eski host akışı geliştirme ve birleşik eski kurulumlarla uyumluluk içindir.
const embeddedServerAvailable = () => fs.existsSync(path.join(app.getAppPath(), 'server', 'dist', 'main.js'))

function tokenPath() { return path.join(app.getPath('userData'), 'session.bin') }
// Arama balonunun kullanıcı tarafından taşındığı konum ve gizleme tercihi.
function widgetStatePath() { return path.join(app.getPath('userData'), 'call-widget.json') }
let widgetState = null
function loadWidgetState() {
  if (widgetState) return widgetState
  try { widgetState = JSON.parse(fs.readFileSync(widgetStatePath(), 'utf8')) } catch { widgetState = {} }
  return widgetState
}
function saveWidgetState(patch) {
  widgetState = { ...loadWidgetState(), ...patch }
  try { fs.writeFileSync(widgetStatePath(), JSON.stringify(widgetState)) } catch {}
}
function databasePath() { return path.join(baseUserData, 'database.bin') }
// Servis SYSTEM hesabıyla koşar; kullanıcıya bağlı DPAPI ile şifreli database.bin'i
// açamaz. Bu yüzden yapılandırma ProgramData\P2\server.json'a da yazılır; kurulum
// betiği klasörü yalnızca yöneticiler/SYSTEM/kurulumu yapan kullanıcı okuyabilecek
// biçimde kilitler.
const serverDataDir = process.platform === 'win32' ? path.join(process.env.ProgramData || 'C:\\ProgramData', 'P2') : path.join(baseUserData, 'server')
const serverConfigPath = () => path.join(serverDataDir, 'server.json')
const serverLogPath = () => path.join(serverDataDir, 'server.log')
const fallbackLogPath = () => path.join(baseUserData, 'server.log')
let lastServerError = ''
function serverLog(line) {
  const text = `[${new Date().toISOString()}] ${String(line).replace(/\x1b\[[0-9;]*m/g, '')}\n`
  if (/error|hata|uygulanamadı|kapandı|does not exist|ECONNREFUSED|password authentication/i.test(text)) lastServerError = text.trim().slice(0, 600)
  try { fs.mkdirSync(serverDataDir, { recursive: true }); fs.appendFileSync(serverLogPath(), text); return } catch {}
  try { fs.appendFileSync(fallbackLogPath(), text) } catch {}
}
function readServerLog() {
  for (const file of [serverLogPath(), fallbackLogPath()]) {
    try { return fs.readFileSync(file, 'utf8').split('\n').filter(Boolean).slice(-25).join('\n') } catch {}
  }
  return ''
}
function loadServerConfig() {
  try { return JSON.parse(fs.readFileSync(serverConfigPath(), 'utf8')) } catch { return null }
}
function serviceInstallerPath(name) { return path.join(path.dirname(process.execPath), name) }
function serviceRegistered() {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve(false)
    // 0.7.14+ gerçek SCM hizmetini kullanır. Görev Zamanlayıcı kontrolü yalnız
    // eski birleşik kurulumlarla geriye dönük uyumluluk için kalır.
    execFile('sc.exe', ['query', 'P2Server'], { windowsHide: true }, serviceError => {
      if (!serviceError) return resolve(true)
      execFile('schtasks', ['/Query', '/TN', 'P2 Server'], { windowsHide: true }, taskError => resolve(!taskError))
    })
  })
}

// 4000 portunda bir P2 API yanıt veriyor mu? (sunucu süreci ya da dev sunucusu)
function apiAlive(timeout = 1200) {
  return new Promise(resolve => {
    const request = http.get({ host: '127.0.0.1', port: API_PORT, path: '/health', timeout }, response => {
      let body = ''
      response.on('data', chunk => { body += chunk })
      response.on('end', () => { try { resolve(JSON.parse(body)?.status === 'ok') } catch { resolve(false) } })
    })
    request.on('error', () => resolve(false))
    request.on('timeout', () => { request.destroy(); resolve(false) })
  })
}

function serverLoginItem(enabled) {
  try { app.setLoginItemSettings({ openAtLogin: enabled, path: process.execPath, args: ['--p2-server'] }) } catch {}
}

// Arayüzden çağrılır: sunucu süreci yoksa bağımsız olarak başlatır.
// Aynı anda birden çok yerden çağrılabildiği için (açılış + bekleme turu)
// tek uçuşludur: ikinci çağrı aynı sözü bekler, ikinci süreç açılmaz. Eskiden
// iki sunucu birden açılıyor, ikincisi 4000 portunu dolu bulup çöküyordu.
let startingServer = null
function ensureServerRunning() {
  if (startingServer) return startingServer
  if (serverChild && !serverChild.exited) return Promise.resolve(true)
  startingServer = startServerOnce().finally(() => { startingServer = null })
  return startingServer
}

async function startServerOnce() {
  if (isDev || serverMode || process.env.P2_API_URL || !embeddedServerAvailable()) return false
  const config = loadDatabaseConfig()
  if (!config) return false
  // Eski sürümden yalnızca şifreli kopya kaldıysa servis/sunucu süreci için düz kopyayı tamamla.
  if (!loadServerConfig()) { try { fs.mkdirSync(serverDataDir, { recursive: true }); fs.writeFileSync(serverConfigPath(), JSON.stringify({ ...config, port: API_PORT }, null, 2)) } catch (error) { serverLog(`server.json yazılamadı: ${error?.message || error}`) } }
  if (await apiAlive()) return true
  // Windows görevi ("P2 Server") kayıtlıysa sunucuyu o yönetir: kendi sürecimizi
  // açmadan önce görevi çalıştırıp kısa süre bekleriz.
  if (await serviceRegistered()) {
    await new Promise(resolve => execFile('sc.exe', ['start', 'P2Server'], { windowsHide: true }, () => resolve()))
    await new Promise(resolve => execFile('schtasks', ['/Run', '/TN', 'P2 Server'], { windowsHide: true }, () => resolve()))
    for (let tur = 0; tur < 10; tur++) {
      if (await apiAlive(600)) return true
      await new Promise(resolve => setTimeout(resolve, 400))
    }
  }
  if (await apiAlive()) return true
  try {
    const child = spawn(process.execPath, ['--p2-server'], { detached: true, stdio: 'ignore', windowsHide: true })
    child.on('error', error => serverLog(`sunucu süreci başlatılamadı: ${error?.message || error}`))
    serverChild = { child, exited: false, code: null }
    child.on('exit', code => { if (serverChild?.child === child) { serverChild.exited = true; serverChild.code = code } })
    child.unref()
    return true
  } catch (error) { serverLog(`sunucu süreci başlatılamadı: ${error?.message || error}`); return false }
}
let serverChild = null
// Sunucu süreci ayrı bir P2.exe olarak yaşıyor. Windows görevi ("P2 Server")
// kayıtlıysa orası yönetir, dokunmayız. Kayıtlı değilse sunucuyu bu uygulama
// başlatmıştır: uygulamadan çıkınca arkada öksüz süreç kalmasın diye kapatılır.
// Kapanış anında asenkron iş bitmeden süreç ölebildiği için hepsi senkron.
function stopOwnServerSync() {
  if (process.platform !== 'win32') { try { serverChild?.child?.kill() } catch {} ; return }
  try {
    execFileSync('sc.exe', ['query', 'P2Server'], { windowsHide: true, stdio: 'ignore' })
    return // gerçek Windows hizmeti kayıtlı: Client hiçbir koşulda dokunmaz
  } catch {}
  try {
    execFileSync('schtasks', ['/Query', '/TN', 'P2 Server'], { windowsHide: true, stdio: 'ignore' })
    return // görev kayıtlı: sunucu servis olarak yaşamaya devam etsin
  } catch {}
  try {
    // Bu oturumda başlatılan sunucu ve varsa önceki oturumlardan kalanlar.
    execFileSync('powershell', ['-NoProfile', '-Command',
      "Get-CimInstance Win32_Process -Filter \"Name='P2.exe'\" | Where-Object { $_.CommandLine -like '*--p2-server*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }",
    ], { windowsHide: true, stdio: 'ignore', timeout: 8000 })
    serverLog('Uygulamadan çıkıldı: sunucu süreçleri durduruldu')
  } catch (error) { serverLog(`Sunucu süreci durdurulamadı: ${error?.message || error}`) }
}

async function waitForServer(timeout = 30000) {
  if (isDev || process.env.P2_API_URL) return { ok: true }
  if (!loadDatabaseConfig()) return { ok: false, reason: 'Sunucu yapılandırması yok.', configured: false }
  const started = Date.now()
  lastServerError = ''
  if (!serverChild || serverChild.exited) await ensureServerRunning()
  while (Date.now() - started < timeout) {
    if (await apiAlive(800)) return { ok: true }
    // Süreç çıktıysa daha fazla beklemenin anlamı yok; nedenini günlükten getir.
    // Ama önce API'ye bir kez daha bakılır: başka bir sunucu süreci ayakta
    // olabilir (bizimki fazlalık olduğu için çekilmiş olabilir).
    if (serverChild?.exited) {
      if (await apiAlive(800)) return { ok: true }
      const log = readServerLog()
      return { ok: false, reason: lastServerError || reasonFromLog(log) || `Sunucu süreci ${serverChild.code} koduyla kapandı.`, configured: true, log }
    }
    await new Promise(resolve => setTimeout(resolve, 400))
  }
  const log = readServerLog()
  return { ok: false, reason: lastServerError || reasonFromLog(log) || 'Sunucu 30 saniyede yanıt vermedi.', configured: true, log }
}
// Sunucu süreci ayrı olduğu için hatası yalnızca günlükte görünür; son hata satırı nedene çevrilir.
function reasonFromLog(log) {
  const lines = String(log || '').split('\n').filter(line => /Migration uygulanamadı|Error|ECONNREFUSED|password authentication|does not exist|EADDRINUSE/i.test(line))
  return lines.length ? lines[lines.length - 1].replace(/^\[[^\]]+\]\s*/, '').slice(0, 400) : ''
}
function loadDatabaseConfig() {
  // Sunucu süreci önce ProgramData'daki düz yapılandırmayı dener (servis için tek yol),
  // sonra kullanıcının şifreli kopyasını.
  if (serverMode) { const plain = loadServerConfig(); if (plain?.databaseUrl) return plain }
  try { return JSON.parse(safeStorage.decryptString(fs.readFileSync(databasePath()))) } catch { return serverMode ? null : loadServerConfig() }
}
async function applyMigrations(target) {
  await target.query('CREATE TABLE IF NOT EXISTS "_p2_migrations" ("name" TEXT PRIMARY KEY, "appliedAt" TIMESTAMPTZ NOT NULL DEFAULT NOW())')
  const migrationsRoot = path.join(app.getAppPath(), 'server', 'prisma', 'migrations')
  const names = fs.readdirSync(migrationsRoot, { withFileTypes: true }).filter(x => x.isDirectory()).map(x => x.name).sort()
  for (const name of names) {
    const done = await target.query('SELECT 1 FROM "_p2_migrations" WHERE "name" = $1', [name])
    if (done.rowCount) continue
    const prismaTable = await target.query(`SELECT to_regclass('public."_prisma_migrations"') AS table_name`)
    if (prismaTable.rows[0]?.table_name) {
      const prismaDone = await target.query('SELECT 1 FROM "_prisma_migrations" WHERE "migration_name" = $1 AND "finished_at" IS NOT NULL AND "rolled_back_at" IS NULL', [name])
      if (prismaDone.rowCount) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
    }
    if (name.endsWith('_init')) {
      const legacy = await target.query(`SELECT to_regclass('public."User"') AS table_name`)
      if (legacy.rows[0]?.table_name) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
      // A failed/aborted first install can leave enum types behind even though
      // the application tables were never created. Only clean P2-owned types
      // when the User table is absent, then retry the transactional migration.
      await target.query('DROP TYPE IF EXISTS "UserRole" CASCADE; DROP TYPE IF EXISTS "ProjectRole" CASCADE; DROP TYPE IF EXISTS "FriendshipStatus" CASCADE; DROP TYPE IF EXISTS "UserStatus" CASCADE;')
    }
    const sql = fs.readFileSync(path.join(migrationsRoot, name, 'migration.sql'), 'utf8')
    await target.query('BEGIN')
    try { await target.query(sql); await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); await target.query('COMMIT') }
    catch (error) { await target.query('ROLLBACK'); throw error }
  }
}
async function migrateConfiguredDatabase(databaseUrl) {
  const target = new Client({ connectionString: databaseUrl })
  await target.connect()
  try { await applyMigrations(target) } finally { await target.end() }
}
async function installDatabase(input) {
  const host = String(input.host || '').trim()
  const port = Number(input.port)
  const username = String(input.username || '').trim()
  const password = String(input.password || '')
  const database = String(input.database || '').trim().toLowerCase()
  if (!host || !username || !password || !Number.isInteger(port) || port < 1 || port > 65535 || !/^[a-z][a-z0-9_]{0,62}$/.test(database)) throw new Error('Veritabanı ayarları geçersiz.')
  const connection = { host, port, user: username, password, ssl: input.ssl ? { rejectUnauthorized: false } : false }
  const admin = new Client({ ...connection, database: 'postgres' })
  await admin.connect()
  try {
    const exists = await admin.query('SELECT 1 FROM pg_database WHERE datname = $1', [database])
    if (!exists.rowCount) await admin.query(`CREATE DATABASE "${database}" ENCODING 'UTF8'`)
  } finally { await admin.end() }

  const target = new Client({ ...connection, database })
  await target.connect()
  try {
    await applyMigrations(target)
  } finally { await target.end() }

  const databaseUrl = `postgresql://${encodeURIComponent(username)}:${encodeURIComponent(password)}@${host}:${port}/${database}?schema=public${input.ssl ? '&sslmode=require' : ''}`
  const config = { databaseUrl, accessSecret: require('node:crypto').randomBytes(48).toString('hex'), refreshSecret: require('node:crypto').randomBytes(48).toString('hex') }
  fs.writeFileSync(databasePath(), safeStorage.encryptString(JSON.stringify(config)))
  try { fs.mkdirSync(serverDataDir, { recursive: true }); fs.writeFileSync(serverConfigPath(), JSON.stringify({ ...config, port: API_PORT }, null, 2)) } catch (error) { serverLog(`server.json yazılamadı: ${error?.message || error}`) }
  apiProcess?.kill(); apiProcess = undefined
  if (input.installService) {
    await installService()
    // Yükseltme reddedildiyse ya da servis kalkmadıysa uygulama yine de çalışsın:
    // 15 sn içinde API gelmezse kullanıcı düzeyinde sunucu süreci başlatılır.
    for (let attempt = 0; attempt < 30; attempt++) { if (await apiAlive(500)) return; await new Promise(resolve => setTimeout(resolve, 500)) }
    serverLog('Servis 15 sn içinde yanıt vermedi; kullanıcı düzeyinde sunucu başlatılıyor')
  }
  await ensureServerRunning()
}

// Kurulum klasöründeki betik servisi kaydeder (UAC ister) ve başlatır. Betik
// bitene kadar beklenir; sonra API'nin yanıt vermesi ayrıca beklenir.
function installService() {
  return new Promise(resolve => {
    const script = serviceInstallerPath('p2-server-install.bat')
    if (!fs.existsSync(script)) { serverLog(`servis betiği yok: ${script}`); return resolve(false) }
    const child = spawn('cmd.exe', ['/c', 'start', '/wait', '', script], { windowsHide: false, stdio: 'ignore' })
    child.on('error', error => { serverLog(`servis betiği çalıştırılamadı: ${error?.message || error}`); resolve(false) })
    child.on('exit', async () => resolve(await serviceRegistered()))
  })
}
function registerUpdaterIpc() {
  ipcMain.handle('updates:configure', (_, apiBase) => updates.configure(apiBase, serverLog))
  ipcMain.handle('updates:check', () => updates.check(serverLog))
  ipcMain.handle('updates:install', () => updates.install(serverLog))
  ipcMain.handle('updates:state', () => ({ ...updates.getState(), current: app.getVersion() }))
}

function registerLegacyPrefsIpc() {
  ipcMain.on('prefs:legacy', event => { event.returnValue = loadLegacyPrefs() })
}

function registerSecureSessionIpc() {
  ipcMain.handle('session:save', (_, value) => {
    if (!safeStorage.isEncryptionAvailable()) return false
    fs.writeFileSync(tokenPath(), safeStorage.encryptString(value))
    return true
  })
  ipcMain.handle('session:load', () => {
    try { return safeStorage.decryptString(fs.readFileSync(tokenPath())) } catch { return null }
  })
  ipcMain.handle('session:clear', () => {
    try { fs.unlinkSync(tokenPath()) } catch {}
    return true
  })
  ipcMain.handle('database:status', () => embeddedServerAvailable() && Boolean(loadDatabaseConfig()))
  // Bu bilgisayardaki sunucu kurulumunu unut: yapılandırma ve oturum silinir,
  // gömülü API durdurulur. Veritabanının kendisine dokunulmaz.
  ipcMain.handle('database:reset', () => {
    try { fs.unlinkSync(databasePath()) } catch {}
    try { fs.unlinkSync(tokenPath()) } catch {}
    apiProcess?.kill(); apiProcess = undefined
    serverLoginItem(false)
    try { fs.unlinkSync(serverConfigPath()) } catch {}
    return true
  })
  ipcMain.handle('server:status', async () => ({ alive: await apiAlive(), configured: Boolean(loadDatabaseConfig()), openAtLogin: (() => { try { return app.getLoginItemSettings({ path: process.execPath, args: ['--p2-server'] }).openAtLogin } catch { return false } })(), port: API_PORT }))
  ipcMain.handle('server:start', async () => ({ ok: await ensureServerRunning() }))
  ipcMain.handle('server:install-service', async () => ({ ok: await installService() }))
  ipcMain.handle('server:service-status', async () => ({ registered: await serviceRegistered(), alive: await apiAlive(), script: serviceInstallerPath('p2-server-install.bat') }))
  ipcMain.handle('server:log', () => readServerLog())
  // Arayüz körlemesine port yoklamaz: sunucu süreci ya yanıt verir ya da erken çıkar;
  // hangisi önce olursa onunla döner (en fazla 30 sn).
  ipcMain.handle('server:wait', async () => waitForServer())
  ipcMain.handle('server:open-folder', () => { try { shell.showItemInFolder(serviceInstallerPath('p2-server-install.bat')) } catch {} return true })
  ipcMain.handle('server:login-item', (_, enabled) => { serverLoginItem(Boolean(enabled)); return true })
  ipcMain.handle('database:install', async (_, input) => {
    if (!embeddedServerAvailable()) return { ok: false, message: 'Sunucu kurulumu P2 Server uygulamasına taşındı.' }
    if (!safeStorage.isEncryptionAvailable()) return { ok: false, message: 'Windows güvenli kasası kullanılamıyor.' }
    try { await installDatabase(input); return { ok: true } }
    catch (error) { return { ok: false, message: error?.message || 'Veritabanı kurulamadı.' } }
  })
}

function registerFileIpc() {
  ipcMain.handle('file:save-as', async (event, input) => {
    const url = String(input?.url || '')
    const name = path.basename(String(input?.name || 'dosya')).replace(/[<>:"/\\|?*]/g, '_')
    if (!/^https?:\/\//i.test(url)) return false
    const owner = BrowserWindow.fromWebContents(event.sender)
    const choice = await dialog.showSaveDialog(owner, { defaultPath: name })
    if (choice.canceled || !choice.filePath) return false
    const response = await net.fetch(url)
    if (!response.ok) throw new Error('Dosya indirilemedi.')
    fs.writeFileSync(choice.filePath, Buffer.from(await response.arrayBuffer()))
    return true
  })
}

// Mikrofon, kamera ve ekran paylaşımı izinleri yalnızca kendi arayüzümüze verilir.
// 'fullscreen' listede olmadığı için gömülü oynatıcıların (YouTube) tam ekran
// isteği sessizce reddediliyordu; düğme hiçbir şey yapmıyor gibi görünüyordu.
const ALLOWED_PERMISSIONS = ['media', 'display-capture', 'notifications', 'clipboard-sanitized-write', 'fullscreen', 'pointerLock', 'keyboardLock']
function registerMediaPermissions() {
  session.defaultSession.setPermissionRequestHandler((_contents, permission, callback) => {
    callback(ALLOWED_PERMISSIONS.includes(permission))
  })
  session.defaultSession.setPermissionCheckHandler((_contents, permission) => ALLOWED_PERMISSIONS.includes(permission))
}

// Ekran paylaşımı: Chromium'un kendi seçicisi yerine uygulamanın seçim penceresi kullanılır.
// Renderer önce kaynağı seçip 'screen:select' ile bildirir, ardından getDisplayMedia çağırır.
let pendingShare = null
function registerScreenShare() {
  session.defaultSession.setDisplayMediaRequestHandler((_request, callback) => {
    const choice = pendingShare
    pendingShare = null
    desktopCapturer.getSources({ types: ['screen', 'window'] }).then(sources => {
      const source = (choice && sources.find(item => item.id === choice.sourceId)) || sources[0]
      if (!source) { callback({}); return }
      // 'loopback': Windows'ta bilgisayarın kendi sesi yayına eklenir.
      callback(choice?.audio ? { video: source, audio: 'loopback' } : { video: source })
    }).catch(() => callback({}))
  }, { useSystemPicker: false })

  ipcMain.handle('screen:sources', async () => {
    const sources = await desktopCapturer.getSources({ types: ['screen', 'window'], thumbnailSize: { width: 320, height: 180 }, fetchWindowIcons: true })
    return sources.map(item => ({
      id: item.id,
      name: item.name,
      kind: item.id.startsWith('screen:') ? 'screen' : 'window',
      thumbnail: item.thumbnail?.isEmpty() ? '' : item.thumbnail.toDataURL(),
      icon: item.appIcon && !item.appIcon.isEmpty() ? item.appIcon.toDataURL() : '',
    }))
  })
  ipcMain.on('screen:select', (_event, choice) => {
    pendingShare = { sourceId: String(choice?.sourceId || ''), audio: Boolean(choice?.audio) }
  })
}

// ---- Sistem sesi (WASAPI loopback) ----
// Chromium'un getDisplayMedia sesi bazı cihaz biçimlerinde (7.1, 32-bit float) açılmıyor.
// Yerel modül sesi cihazın kendi biçiminde alıp stereo float'a indiriyor.
let loopbackModule = null
let loopbackTarget = null
function loadLoopback() {
  if (loopbackModule !== null) return loopbackModule
  const candidates = [
    path.join(__dirname, '..', 'native', 'loopback', 'build', 'Release', 'loopback.node'),
    // Paketlenmiş sürümde yerel modüller asar dışında durur.
    path.join(__dirname, '..', 'native', 'loopback', 'build', 'Release', 'loopback.node').replace('app.asar', 'app.asar.unpacked'),
  ]
  try {
    let lastError = null
    for (const candidate of candidates) {
      try { loopbackModule = require(candidate); lastError = null; break } catch (error) { lastError = error }
    }
    if (!loopbackModule && lastError) throw lastError
  } catch (error) {
    console.error('Sistem sesi modülü yüklenemedi:', error?.message || error)
    loopbackModule = false
  }
  return loopbackModule
}
function registerSystemAudioIpc() {
  ipcMain.handle('systemAudio:start', event => {
    const module = loadLoopback()
    if (!module) return { ok: false, message: 'Sistem sesi modülü bu kurulumda yok.' }
    loopbackTarget = event.sender
    try {
      module.start(chunk => {
        if (!loopbackTarget || loopbackTarget.isDestroyed()) return
        loopbackTarget.send('systemAudio:chunk', chunk)
      })
      const error = module.lastError()
      if (error) return { ok: false, message: error }
      return { ok: true }
    } catch (error) {
      return { ok: false, message: error?.message || 'Sistem sesi başlatılamadı.' }
    }
  })
  ipcMain.on('systemAudio:stop', () => {
    loopbackTarget = null
    const module = loadLoopback()
    try { if (module) module.stop() } catch {}
  })
  app.on('before-quit', () => { try { const module = loadLoopback(); if (module) module.stop() } catch {} })
}

function registerNotificationIpc() {
  ipcMain.handle('notification:show', (event, payload) => {
    if (!Notification.isSupported()) return false
    const window = BrowserWindow.fromWebContents(event.sender)
    const notification = new Notification({
      title: String(payload?.title || 'P2'),
      body: String(payload?.body || '').slice(0, 240),
      // Sesi uygulama kendi çalıyor, Windows'un varsayılan sesi ikinci kez çalmasın.
      silent: true,
    })
    notification.on('click', () => {
      if (!window) return
      if (window.isMinimized()) window.restore()
      window.show(); window.focus(); window.flashFrame(false)
      window.webContents.send('notification:activate', payload?.data ?? null)
    })
    notification.show()
    if (window && !window.isFocused()) window.flashFrame(true)
    return true
  })
}

function registerWindowIpc() {
  ipcMain.on('window:minimize', event => BrowserWindow.fromWebContents(event.sender)?.minimize())
  ipcMain.on('window:toggle-maximize', event => {
    const window = BrowserWindow.fromWebContents(event.sender)
    if (!window) return
    window.isMaximized() ? window.unmaximize() : window.maximize()
  })
  ipcMain.on('window:close', event => BrowserWindow.fromWebContents(event.sender)?.close())
  ipcMain.handle('window:is-maximized', event => BrowserWindow.fromWebContents(event.sender)?.isMaximized() || false)
  // Yayını tam ekran izlerken pencerenin kendisi tam ekrana alınır (F11 davranışı).
  ipcMain.on('window:set-fullscreen', (event, on) => {
    const window = BrowserWindow.fromWebContents(event.sender)
    if (window) window.setFullScreen(Boolean(on))
  })
  // "Yeni pencerede aç": aynı arayüz, yalnızca seçilen sohbeti gösteren popout kipiyle.
  ipcMain.on('window:open-popout', (_event, payload) => openPopoutWindow(payload?.query || '', payload?.title || 'P2'))
}

// window.open çağrıları: kendi popout adresimizse pencereyi biz açarız, dış
// bağlantılar tarayıcıya gider, gerisi engellenir.
function handleWindowOpen(url) {
  try {
    const parsed = new URL(url)
    if (parsed.searchParams.get('popout') === '1') {
      openPopoutWindow(parsed.searchParams.toString(), 'P2')
      return { action: 'deny' }
    }
  } catch {}
  if (url.startsWith('https://')) shell.openExternal(url)
  return { action: 'deny' }
}

const popouts = new Map()
function clientWindowIcon() {
  const candidates = [
    path.join(process.resourcesPath || '', 'icon.ico'),
    path.join(path.dirname(process.execPath), 'resources', 'icon.ico'),
    path.join(app.getAppPath(), 'build', 'icon.ico'),
    path.join(__dirname, '..', 'build', 'icon.ico'),
    path.join(process.resourcesPath || '', 'icon.png'),
    path.join(path.dirname(process.execPath), 'resources', 'icon.png'),
    path.join(app.getAppPath(), 'build', 'icon.png'),
    path.join(__dirname, '..', 'build', 'icon.png'),
  ]
  for (const file of candidates) {
    try {
      const image = nativeImage.createFromPath(file)
      // Windows görev çubuğu ICO'nun mutlak dosya yoluyla verildiğinde simgeyi
      // en güvenilir şekilde kabuk sürecine bağlıyor. NativeImage'i burada
      // yalnızca dosyanın gerçekten okunabilir olduğunu doğrulamak için kullan.
      if (!image.isEmpty()) return file
    } catch {}
  }
  return undefined
}
function repairWindowsShortcutIdentity() {
  if (process.platform !== 'win32') return
  const shortcut = path.join(app.getPath('appData'), 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Electron.lnk')
  try {
    if (!fs.existsSync(shortcut)) return
    const details = shell.readShortcutLink(shortcut)
    if (path.basename(details.target || '').toLowerCase() !== 'electron.exe') return
    // P2 geliştirme çalıştırması Electron.lnk'ye üretim AppUserModelID'sini
    // yazmışsa Windows kurulu P2 için atom ikonunu seçer. Kimlikleri ayır.
    if (details.appUserModelId !== 'com.p2.desktop' && details.appUserModelId !== 'com.p2.desktop.dev') return
    const projectRoot = path.resolve(path.dirname(details.target), '..', '..', '..')
    const devIcon = path.join(projectRoot, 'build', 'icon.ico')
    shell.writeShortcutLink(shortcut, 'update', {
      ...details,
      appUserModelId: 'com.p2.desktop.dev',
      ...(fs.existsSync(devIcon) ? { icon: devIcon, iconIndex: 0 } : {}),
    })
  } catch (error) {
    serverLog(`Windows kısayol kimliği düzeltilemedi: ${error?.message || error}`)
  }
}
function pinWindowIcon(window) {
  const icon = clientWindowIcon()
  if (!icon || !window || window.isDestroyed()) return
  const applyIcon = () => {
    if (window.isDestroyed()) return
    try { window.setIcon(icon) } catch {}
    // BrowserWindow.icon pencere HICON'unu ayarlar; Windows 11 görev çubuğu
    // düğmesi ise AppUserModelID ayrıntılarını kullanabilir. İkisini birlikte
    // ayarlayarak electron.exe ile çalışan geliştirme sürümünü de P2'ye bağla.
    if (process.platform === 'win32') {
      try {
        window.setAppDetails({
          appId: CLIENT_APP_ID,
          appIconPath: icon,
          appIconIndex: 0,
          relaunchCommand: app.isPackaged
            ? `"${process.execPath}"`
            : `"${process.execPath}" "${app.getAppPath()}"`,
          relaunchDisplayName: 'P2',
        })
      } catch {}
    }
  }
  applyIcon()
  window.once('ready-to-show', applyIcon)
}
function openPopoutWindow(query, title) {
  const existing = popouts.get(query)
  if (existing && !existing.isDestroyed()) { existing.show(); existing.focus(); return }
  const window = new BrowserWindow({
    width: 980,
    height: 760,
    minWidth: 620,
    minHeight: 480,
    backgroundColor: '#070b14',
    title,
    appId: CLIENT_APP_ID,
    icon: clientWindowIcon(),
    frame: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      additionalArguments: [`--p2-suggested-server=${suggestedServer()}`, `--p2-version=${app.getVersion()}`],
      // Geliştirmede Vite kendi kökeninden sunduğu için gömülü oynatıcı doğrudan çalışır.
      ...(isDev ? {} : {}),
    },
  })
  pinWindowIcon(window)
  popouts.set(query, window)
  window.on('closed', () => popouts.delete(query))
  window.webContents.setWindowOpenHandler(({ url }) => handleWindowOpen(url))
  loadRenderer(window, query)
}

// "Yalnızca istemci" kurulumunda önerilecek sunucu adresi; paketlenmiş sürümde
// electron/api-config.json'dan okunur, geliştirmede boştur.
function suggestedServer() {
  if (!app.isPackaged) return ''
  try { return String(require('./api-config.json').apiUrl || '') } catch { return '' }
}

function startPackagedApi() {
  if (isDev || process.env.P2_API_URL || apiProcess || apiStarting) return
  // Arayüz süreci API'yi kendi içinde çalıştırmaz; bağımsız sunucu sürecini kaldırır.
  if (!serverMode) { void ensureServerRunning(); return }
  const config = loadDatabaseConfig()
  if (!config) return
  apiStarting = true
  // Başka bir sunucu süreci portu zaten dinliyorsa bu süreç fazladır: API'yi
  // açmaya çalışıp EADDRINUSE ile çökmek yerine sessizce çekilir.
  void apiAlive().then(alive => {
    if (!alive) return
    serverLog('Sunucu zaten çalışıyor; bu ikinci süreç kapanıyor.')
    apiStarting = false
    app.quit()
  })
  // Bir yükseltmeden sonra bekleyen migration'lar API açılmadan önce uygulanır.
  migrateConfiguredDatabase(config.databaseUrl).then(() => true, error => { serverLog(`Migration uygulanamadı: ${error?.message || error}`); console.error('Migration uygulanamadı:', error?.message || error); return false }).then(async migrated => {
    apiStarting = false
    if (!migrated) { if (serverMode) app.quit(); return }
    if (apiProcess) return
    // Migration sürerken başka bir süreç portu almış olabilir.
    if (await apiAlive()) { serverLog('Sunucu zaten çalışıyor; bu ikinci süreç kapanıyor.'); app.quit(); return }
    const serverFile = path.join(app.getAppPath(), 'server', 'dist', 'main.js')
    serverLog(`API başlatılıyor (${serverMode ? 'sunucu süreci' : 'arayüz'}), port ${API_PORT}`)
    apiProcess = fork(serverFile, [], { env: { ...process.env, DATABASE_URL: config.databaseUrl, JWT_ACCESS_SECRET: config.accessSecret, JWT_REFRESH_SECRET: config.refreshSecret, PORT: String(API_PORT), P2_UPDATE_DIR: path.join(serverDataDir, 'updates'), P2_UPLOAD_DIR: process.env.P2_UPLOAD_DIR || path.join(serverDataDir, 'uploads') }, stdio: ['ignore', 'pipe', 'pipe', 'ipc'] })
    apiProcess.stdout?.on('data', chunk => serverLog(String(chunk).trim()))
    apiProcess.stderr?.on('data', chunk => serverLog(String(chunk).trim()))
    apiProcess.once('exit', code => { serverLog(`API kapandı (kod ${code})`); apiProcess = undefined; if (serverMode) app.quit() })
  })
}


// ---- Ekran üstü çizim kaplaması ----
// Paylaşım sürerken şeffaf, tıklamayı geçiren bir pencere paylaşılan ekranı kaplar;
// izleyenlerin çizdikleri ve kalem imleçleri yayıncının gerçek masaüstünde görünür
// (ekran yakalandığı için izleyenler de aynı çizimi görür).
let overlayWindow = null
async function overlayDisplay(sourceId) {
  const displays = screen.getAllDisplays()
  try {
    const sources = await desktopCapturer.getSources({ types: ['screen'] })
    const source = sources.find(item => item.id === sourceId)
    const match = source && source.display_id && displays.find(item => String(item.id) === String(source.display_id))
    if (match) return match
    // display_id boşsa kaynak sırasına göre eşleştir: "screen:0:0" ilk ekran.
    const index = sources.findIndex(item => item.id === sourceId)
    if (index >= 0 && displays[index]) return displays[index]
  } catch {}
  return screen.getPrimaryDisplay()
}
async function showOverlay(sourceId) {
  const bounds = (await overlayDisplay(sourceId)).bounds
  if (overlayWindow && !overlayWindow.isDestroyed()) { overlayWindow.setBounds(bounds); overlayWindow.showInactive(); return }
  overlayWindow = new BrowserWindow({
    ...bounds,
    frame: false, transparent: true, backgroundColor: '#00000000', hasShadow: false,
    alwaysOnTop: true, skipTaskbar: true, focusable: false, resizable: false, movable: false,
    fullscreenable: false, show: false, enableLargerThanScreen: true,
    webPreferences: { preload: path.join(__dirname, 'overlay-preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  })
  overlayWindow.setAlwaysOnTop(true, 'screen-saver')
  overlayWindow.setIgnoreMouseEvents(true, { forward: true })   // fare kaplamanın içinden geçer
  overlayWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true })
  overlayWindow.loadURL('data:text/html;charset=UTF-8,' + encodeURIComponent(OVERLAY_HTML))
  overlayWindow.once('ready-to-show', () => {
    if (!overlayWindow || overlayWindow.isDestroyed()) return
    overlayWindow.showInactive()
    // Görev çubuğu yüzünden pencere çalışma alanına sıkışabiliyor; ekranın tamamına zorlanır.
    overlayWindow.setBounds(bounds)
  })
  overlayWindow.on('closed', () => { overlayWindow = null })
}
function hideOverlay() {
  if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.destroy()
  overlayWindow = null
}
function registerOverlayIpc() {
  ipcMain.on('overlay:show', (_event, payload) => { void showOverlay(String(payload?.sourceId || '')) })
  ipcMain.on('overlay:hide', () => hideOverlay())
  ipcMain.on('overlay:paint', (_event, message) => {
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.webContents.send('overlay:paint', message)
  })
}

const OVERLAY_HTML = `<!doctype html><html lang="tr"><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;background:transparent;overflow:hidden}
  canvas{display:block;width:100vw;height:100vh}
</style></head><body><canvas id="board"></canvas><script>
  const HOLD=2200, FADE=2600, CURSOR_LIFE=2600
  const board=document.getElementById('board'), context=board.getContext('2d')
  const strokes=new Map(), cursors=new Map()
  function size(){
    const ratio=window.devicePixelRatio||1
    board.width=Math.round(innerWidth*ratio); board.height=Math.round(innerHeight*ratio)
    context.setTransform(ratio,0,0,ratio,0,0)
  }
  size(); addEventListener('resize',size)
  window.overlay.onPaint(message=>{
    if(message.kind==='stroke'){
      const existing=strokes.get(message.id)
      if(existing){existing.points.push(...message.points);existing.touched=performance.now()}
      else strokes.set(message.id,{color:message.color,width:message.width||4,points:message.points.slice(),touched:performance.now()})
    }else if(message.kind==='cursor'){
      if(message.gone)cursors.delete(message.from)
      else cursors.set(message.from,{x:message.x,y:message.y,name:message.name,color:message.color,at:performance.now()})
    }else if(message.kind==='clear'){strokes.clear();cursors.clear()}
  })
  function paint(){
    requestAnimationFrame(paint)
    const width=innerWidth, height=innerHeight, now=performance.now()
    context.clearRect(0,0,width,height)
    context.lineCap='round'; context.lineJoin='round'; context.lineWidth=4
    context.shadowColor='rgba(0,0,0,.5)'; context.shadowBlur=5
    for(const[id,stroke]of[...strokes.entries()]){
      const age=now-stroke.touched
      if(age>HOLD+FADE){strokes.delete(id);continue}
      context.globalAlpha=age<HOLD?1:Math.max(0,1-(age-HOLD)/FADE)
      context.strokeStyle=stroke.color
      context.lineWidth=stroke.width||4
      context.beginPath()
      stroke.points.forEach((point,index)=>{
        const x=point.x*width, y=point.y*height
        if(index===0)context.moveTo(x,y); else context.lineTo(x,y)
      })
      context.stroke()
    }
    context.shadowBlur=0
    for(const[id,cursor]of[...cursors.entries()]){
      const age=now-cursor.at
      if(age>CURSOR_LIFE){cursors.delete(id);continue}
      context.globalAlpha=age>CURSOR_LIFE-600?Math.max(0,(CURSOR_LIFE-age)/600):1
      const x=cursor.x*width, y=cursor.y*height
      context.fillStyle=cursor.color
      context.beginPath()
      context.moveTo(x,y); context.lineTo(x+13,y+5); context.lineTo(x+5.5,y+7); context.lineTo(x+3.5,y+14)
      context.closePath(); context.fill()
      context.font='600 13px Inter,"Segoe UI",sans-serif'
      const textWidth=context.measureText(cursor.name).width
      const left=Math.min(x+14,width-textWidth-18), top=y+12
      context.fillStyle=cursor.color
      context.beginPath()
      if(context.roundRect)context.roundRect(left,top,textWidth+14,20,10); else context.rect(left,top,textWidth+14,20)
      context.fill()
      context.fillStyle='#14181d'
      context.fillText(cursor.name,left+7,top+14)
    }
    context.globalAlpha=1
  }
  paint()
</script></body></html>`

// ---- Kapatma davranışı, sistem tepsisi ve masaüstü arama balonu ----
let closeAction = 'ask'
let quitting = false
let tray = null
let callWidget = null
let mainWindow = null
let callSummary = { active: false, label: '', participants: 0, muted: false, startedAt: null, connectedAt: null, ringing: false }

function trayIcon() {
  // Uygulamanın kendi simgesi; paketli sürümde kaynaklardan, geliştirmede build/ içinden.
  for (const file of [path.join(process.resourcesPath || '', 'icon.ico'), path.join(process.resourcesPath || '', 'icon.png'), path.join(app.getAppPath(), 'build', 'icon.ico'), path.join(app.getAppPath(), 'build', 'icon.png')]) {
    try {
      const image = nativeImage.createFromPath(file)
      if (!image.isEmpty()) return image.resize({ width: 16, height: 16, quality: 'best' })
    } catch {}
  }
  return nativeImage.createEmpty()
}

function ensureTray(window) {
  if (tray) return tray
  tray = new Tray(trayIcon())
  tray.setToolTip('P2')
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: 'P2\'yi aç', click: () => showMainWindow(window) },
    { type: 'separator' },
    { label: 'Uygulamadan çık', click: () => { quitting = true; app.quit() } },
  ]))
  tray.on('click', () => showMainWindow(window))
  return tray
}

function showMainWindow(window) {
  if (!window || window.isDestroyed()) return
  if (window.isMinimized()) window.restore()
  window.show()
  window.focus()
  syncCallWidget(window)
}

// Ana pencere gizli/simge durumundayken ve aktif arama varken balon görünür.
function syncCallWidget(window) {
  const hidden = !window || window.isDestroyed() || !window.isVisible() || window.isMinimized()
  const state = loadWidgetState()
  // "Bir daha gösterme" kalıcı; "bu seferlik" yalnızca bu arama boyunca geçerli.
  const shouldShow = callSummary.active && hidden && !state.hidden && !widgetSkippedCall
  if (!shouldShow) {
    if (callWidget && !callWidget.isDestroyed()) callWidget.destroy()
    callWidget = null
    return
  }
  if (callWidget && !callWidget.isDestroyed()) { callWidget.webContents.send('widget:update', callSummary); return }
  const display = screen.getPrimaryDisplay().workArea
  // Kayıtlı konum ekran dışında kalmışsa (monitör değişmiş olabilir) sağ alta döner.
  const saved = Number.isFinite(state.x) && Number.isFinite(state.y) && screen.getAllDisplays().some(item => {
    const area = item.workArea
    return state.x + 380 > area.x && state.x < area.x + area.width && state.y + 80 > area.y && state.y < area.y + area.height
  }) ? state : null
  callWidget = new BrowserWindow({
    width: 404, height: 126,
    x: saved ? saved.x : display.x + display.width - 424,
    y: saved ? saved.y : display.y + display.height - 148,
    frame: false, resizable: false, movable: true, skipTaskbar: true, alwaysOnTop: true,
    transparent: true, backgroundColor: '#00000000', focusable: true, show: false, hasShadow: false,
    webPreferences: { preload: path.join(__dirname, 'widget-preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  })
  callWidget.setAlwaysOnTop(true, 'screen-saver')
  callWidget.on('moved', () => {
    if (!callWidget || callWidget.isDestroyed()) return
    const [x, y] = callWidget.getPosition()
    saveWidgetState({ x, y })
  })
  callWidget.loadURL('data:text/html;charset=UTF-8,' + encodeURIComponent(WIDGET_HTML))
  callWidget.once('ready-to-show', () => {
    if (!callWidget || callWidget.isDestroyed()) return
    callWidget.showInactive()
    callWidget.webContents.send('widget:update', callSummary)
  })
}

const WIDGET_HTML = `<!doctype html><html lang="tr"><head><meta charset="utf-8"><style>${P2_MAVI_CSS}
  body{-webkit-app-region:drag;user-select:none;background:transparent}
  /* Balonun üstündeki boşluk × düğmesi için; × konumu aşağıdaki .close kuralında. */
  .bubble{display:grid;grid-template-columns:10px 1fr auto;align-items:center;gap:9px;height:64px;margin:47px 15px 15px;padding:0 10px;border:1px solid var(--p2-border-strong);border-radius:var(--p2-r-lg);background:var(--p2-glass);box-shadow:var(--p2-shadow-lg);animation:p2-pop-elastic .62s var(--ease-elastic) both}
  .dot{width:9px;height:9px;border-radius:50%;background:var(--p2-success);box-shadow:0 0 8px var(--p2-success);animation:p2-pulse 2.2s ease-in-out infinite}
  .info{min-width:0}
  .info b{display:block;color:var(--p2-text-strong);font-size:11px}
  .info small{color:var(--p2-text-3);font-size:10px;font-variant-numeric:tabular-nums}
  .actions{display:flex;gap:5px;-webkit-app-region:no-drag}
  .actions button{width:32px;height:32px;display:grid;place-items:center;border:1px solid var(--p2-border);border-radius:var(--p2-r-sm);background:var(--p2-bg-3);color:var(--p2-text-2);cursor:pointer}
  .actions button.mute{width:auto;grid-template-columns:17px auto;gap:6px;padding:0 9px;font:600 9px Inter,"Segoe UI",sans-serif}
  .actions button svg{width:17px;height:17px;fill:currentColor}
  .actions button:hover{background:var(--p2-bg-hover);border-color:var(--p2-border-strong);color:var(--p2-text-strong)}
  .actions button.on{border-color:var(--p2-danger-border);background:var(--p2-danger-soft);color:var(--p2-danger-text)}
  .actions .leave{border-color:var(--p2-danger-border);background:var(--p2-danger-soft);color:var(--p2-danger-text)}
  .actions .leave:hover{background:var(--p2-danger);border-color:var(--p2-danger);color:#fff}
  .close{position:absolute;top:0px;right:4px;width:20px;height:20px;display:flex;align-items:center;justify-content:center;padding:0 0 1px;border:1px solid var(--p2-border);border-radius:6px;background:var(--p2-bg-2);color:var(--p2-text-3);font:500 14px/1 Inter,"Segoe UI",sans-serif;cursor:pointer;-webkit-app-region:no-drag}
  .close:hover{background:var(--p2-bg-hover);color:var(--p2-text-strong)}
  .wrap{position:relative}
</style></head><body>
  <div class="wrap">
  <button class="close" id="close" title="Balonu kapat">×</button>
  <div class="bubble">
    <span class="dot"></span>
    <div class="info"><b id="label">Sesli arama</b><small id="meta">00:00</small></div>
    <div class="actions">
      <button id="mute" class="mute" title="Mikrofonu kapat"><svg viewBox="0 0 24 24"><path id="micPath" d="M12 14.2a3.4 3.4 0 0 0 3.4-3.4V6.2a3.4 3.4 0 1 0-6.8 0v4.6a3.4 3.4 0 0 0 3.4 3.4Z"/><path d="M18.2 10.6a.9.9 0 0 0-1.8 0 4.4 4.4 0 0 1-8.8 0 .9.9 0 0 0-1.8 0 6.2 6.2 0 0 0 5.3 6.1v2.1H9.4a.9.9 0 1 0 0 1.8h5.2a.9.9 0 1 0 0-1.8h-1.7v-2.1a6.2 6.2 0 0 0 5.3-6.1Z"/><path id="micSlash" style="display:none" d="M4.6 3.5a.9.9 0 0 0-1.2 1.4l16 14a.9.9 0 0 0 1.2-1.4l-16-14Z"/></svg><span id="muteLabel">Kapat</span></button>
      <button id="deafen" class="mute" title="Sesi kapat"><svg viewBox="0 0 24 24"><path d="M12 3.2A9.2 9.2 0 0 0 2.8 12.4v4.4a3.2 3.2 0 0 0 3.2 3.2h1.1a1.3 1.3 0 0 0 1.3-1.3v-5.3a1.3 1.3 0 0 0-1.3-1.3H4.9v-.7a7.1 7.1 0 0 1 14.2 0v.7h-2.2a1.3 1.3 0 0 0-1.3 1.3v5.3a1.3 1.3 0 0 0 1.3 1.3H18a3.2 3.2 0 0 0 3.2-3.2v-4.4A9.2 9.2 0 0 0 12 3.2Z"/><path id="deafSlash" style="display:none" d="M4.6 3.5a.9.9 0 0 0-1.2 1.4l16 14a.9.9 0 0 0 1.2-1.4l-16-14Z"/></svg><span id="deafLabel">Kapat</span></button>
      <button id="open" title="P2'yi aç"><svg viewBox="0 0 24 24"><path d="M4 4h7v2H6v5H4V4Zm16 16h-7v-2h5v-5h2v7Z"/><path d="M4 20v-7h2v5h5v2H4ZM20 4v7h-2V6h-5V4h7Z"/></svg></button>
      <button id="leave" class="leave" title="Aramadan ayrıl"><svg viewBox="0 0 24 24" style="transform:rotate(133deg)"><path d="M12 7.4c-3.3 0-6.4.9-8.9 2.5-.9.6-1.2 1.8-.7 2.7l1 1.8c.5.9 1.7 1.3 2.6.8l2.2-1.1c.6-.3 1-1 1-1.7v-1c1.8-.5 3.8-.5 5.6 0v1c0 .7.4 1.4 1 1.7l2.2 1.1c1 .5 2.1.1 2.6-.8l1-1.8c.5-.9.2-2.1-.7-2.7A16.4 16.4 0 0 0 12 7.4Z"/></svg></button>
    </div>
  </div>
  </div>
  <script>
    let state={startedAt:null,connectedAt:null,ringing:false,participants:0}
    const label=document.getElementById('label'),meta=document.getElementById('meta'),mute=document.getElementById('mute'),muteLabel=document.getElementById('muteLabel'),slash=document.getElementById('micSlash'),deafen=document.getElementById('deafen'),deafLabel=document.getElementById('deafLabel'),deafSlash=document.getElementById('deafSlash')
    const clock=value=>{
      const total=Math.max(0,Math.floor((Date.now()-value)/1000))
      return String(Math.floor(total/60)).padStart(2,'0')+':'+String(total%60).padStart(2,'0')
    }
    const tick=()=>{
      if(state.ringing){meta.textContent='Çalıyor… '+(state.startedAt?clock(state.startedAt):'00:00');return}
      meta.textContent=(state.connectedAt?clock(state.connectedAt):'00:00')+' · '+state.participants+' kişi'
    }
    setInterval(tick,1000)
    window.p2widget.onUpdate(summary=>{
      label.textContent=summary.label||'Sesli arama'
      state={startedAt:summary.startedAt,connectedAt:summary.connectedAt,ringing:summary.ringing,participants:summary.participants}
      slash.style.display=summary.muted?'':'none'
      mute.classList.toggle('on',Boolean(summary.muted))
      muteLabel.textContent=summary.muted?'Aç':'Kapat'
      mute.title=summary.muted?'Mikrofonu aç':'Mikrofonu kapat'
      deafSlash.style.display=summary.deafened?'':'none'
      deafen.classList.toggle('on',Boolean(summary.deafened))
      deafLabel.textContent=summary.deafened?'Aç':'Kapat'
      deafen.title=summary.deafened?'Sesi aç':'Sesi kapat'
      tick()
    })
    mute.onclick=()=>window.p2widget.action('mute')
    deafen.onclick=()=>window.p2widget.action('deafen')
    document.getElementById('open').onclick=()=>window.p2widget.action('open')
    document.getElementById('leave').onclick=()=>window.p2widget.action('leave')
    document.getElementById('close').onclick=()=>window.p2widget.action('close')
  </script>
</body></html>`

function registerWindowBehaviour(window) {
  // Arayüz "geri açıldı" bilgisini yalnızca gerçek simge/tepsi dönüşlerinde alır;
  // pencereye tıklayarak odaklanmak bu olayı tetiklemez.
  const restored = () => { syncCallWidget(window); if (!window.isDestroyed()) window.webContents.send('window:restored') }
  window.on('minimize', () => syncCallWidget(window))
  window.on('restore', restored)
  window.on('show', restored)
  window.on('hide', () => syncCallWidget(window))
  window.on('close', event => {
    if (quitting) return
    if (closeAction === 'quit') {
      event.preventDefault()
      window.webContents.send('call:widget-action','leave')
      setTimeout(()=>{quitting=true;app.quit()},120)
      return
    }
    event.preventDefault()
    if (closeAction === 'ask') {
      const choice = dialog.showMessageBoxSync(window, {
        type: 'question',
        buttons: ['Sistem tepsisine küçült', 'Simge durumuna küçült', 'Uygulamadan çık'],
        defaultId: 0,
        cancelId: 1,
        title: 'P2 kapatılıyor',
        message: 'Kapat düğmesine basınca ne olsun?',
        detail: 'Tepside kalırsa aramalar ve bildirimler çalışmaya devam eder. Bu seçimi daha sonra Ayarlar > Uygulama bölümünden değiştirebilirsin.',
        checkboxLabel: 'Bu seçimi hatırla',
        checkboxChecked: true,
      })
      const picked = choice === 0 ? 'tray' : choice === 1 ? 'minimize' : 'quit'
      closeAction = picked
      window.webContents.send('window:close-action', picked)
      if (picked === 'quit') { window.webContents.send('call:widget-action','leave');setTimeout(()=>{quitting=true;app.quit()},120);return }
    }
    if (closeAction === 'tray') { ensureTray(window); window.hide() } else window.minimize()
    syncCallWidget(window)
  })
}

// Balonun × düğmesi: bu arama için mi, kalıcı olarak mı kapansın?
function askWidgetClose(owner) {
  const choice = dialog.showMessageBoxSync(callWidget && !callWidget.isDestroyed() ? callWidget : owner, {
    type: 'question',
    buttons: ['Bu seferlik gizle', 'Bir daha gösterme', 'Vazgeç'],
    defaultId: 0,
    cancelId: 2,
    title: 'Arama balonu',
    message: 'Arama balonu kapatılsın mı?',
    detail: 'Bu seferlik gizlersen sonraki aramada yine görünür. Bir daha gösterme dersen Ayarlar > Uygulama bölümünden geri açabilirsin.',
  })
  if (choice === 2) return
  if (choice === 1) saveWidgetState({ hidden: true })
  else widgetSkippedCall = true
  syncCallWidget(owner)
}

// Oyun içi overlay IPC'si. Ana pencere <-> overlay oturumları arasındaki köprü ana süreçten geçer:
// overlay:push (ana pencere -> tüm oturumlar), overlay:send (oturum -> ana pencere). Sözleşme: src/overlay/overlayTypes.ts.
// (overlay:show/hide/paint ekran paylaşımı işaretçi katmanına aittir; buradaki kanallar farklıdır.)
function registerGameOverlayIpc() {
  const toMain = (channel, payload) => { if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send(channel, payload) }
  overlayHost.configure({
    onReady: info => toMain('overlay:ready', info),
    onScreenshot: shot => toMain('overlay:screenshot', shot),
    onHotkey: ({ id, down, pid }) => {
      if (id === 2) { if (down) void overlayHost.takeScreenshot(pid, 'hotkey') }
      else if (id === 3) { if (down) toMain('call:widget-action', 'mute') }
      else if (id === 5) { if (down) toMain('call:widget-action', 'deafen') }
      else if (id === 4) toMain('call:ptt', down)
    },
  })
  // Eski basit eylemler (sus/sağır/ayrıl/aç) ve yeni kabul/reddet aynı kanaldan ana pencereye
  ipcMain.on('overlay:call-action', (_event, action) => toMain('call:widget-action', action))
  ipcMain.on('overlay:push', (_event, message) => {
    const topic = message?.topic, payload = message?.payload
    if (topic === 'notify') { overlayHost.toast(payload?.target ?? 'all', payload?.toast ?? payload); return }
    if (topic === 'open' && payload && typeof payload.pid === 'number') overlayHost.setVisible(payload.pid, true)
    overlayHost.broadcast('overlay:push', { topic, payload })
  })
  ipcMain.on('overlay:send', (event, message) => { const info = overlayHost.sessionOf(event.sender); toMain('overlay:send', { ...(message || {}), pid: info?.pid, projectId: info?.projectId }) })
  ipcMain.on('overlay:toast', (_event, { target, toast } = {}) => overlayHost.toast(target ?? 'all', toast))
  ipcMain.on('overlay:event', (_event, ev) => overlayHost.event(ev))
  ipcMain.on('overlay:toast-action', (event, detail) => {
    const info = overlayHost.sessionOf(event.sender)
    toMain('overlay:toast-action', { ...(detail || {}), pid: info?.pid })
    const tag = String(detail?.tag || '')
    if (tag.startsWith('call:') && (detail?.actionId === 'accept' || detail?.actionId === 'decline')) { toMain('call:widget-action', detail.actionId); overlayHost.dismiss('all', tag) }
    // Ekran görüntüsü kartı: klasörü aç / paylaş (paylaşım overlay'in Ekran görüntüsü penceresinde açılır)
    if (tag.startsWith('shot:')) {
      const payload = detail?.payload || {}
      if (detail?.actionId === 'open-folder' && payload.path) { try { shell.showItemInFolder(String(payload.path)) } catch {} }
      if (detail?.actionId === 'share' && info?.pid) { overlayHost.setVisible(info.pid, true); overlayHost.broadcast('overlay:push', { topic: 'open', payload: { window: 'capture', target: payload } }) }
    }
  })
  ipcMain.handle('overlay:set-hotkeys', (_event, list) => overlayHost.setHotkeys(list))
  ipcMain.on('overlay:configure', (_event, options) => { if (options && typeof options === 'object' && 'screenshotDir' in options) overlayHost.setScreenshotDir(options.screenshotDir) })
  ipcMain.on('overlay:call-stats', (_event, stats) => overlayHost.setCallStats(stats))
  ipcMain.on('overlay:set-visible', (event, arg) => overlayHost.setVisibleFrom(event.sender, arg))
  ipcMain.on('overlay:draw-state', (event, state) => overlayHost.setDrawState(event.sender, state))
  ipcMain.on('overlay:set-zoom', (event, factor) => { try { event.sender.setZoomFactor(Math.max(0.5, Math.min(2, Number(factor) || 1))) } catch {} })
  ipcMain.handle('overlay:screenshot', event => overlayHost.screenshotFor(event.sender))
  ipcMain.handle('overlay:pick-screenshot-dir', async () => {
    const result = await dialog.showOpenDialog({ title: 'Ekran görüntüsü klasörü', properties: ['openDirectory', 'createDirectory'] })
    return result.canceled ? null : (result.filePaths[0] || null)
  })
}

function registerWindowBehaviourIpc() {
  ipcMain.on('window:set-close-action', (_, action) => {
    if (['ask', 'tray', 'minimize', 'quit'].includes(action)) closeAction = action
  })
  ipcMain.on('call:update', (event, summary) => {
    if (summary && !summary.active) widgetSkippedCall = false
    for (const window of BrowserWindow.getAllWindows()) { if (!window.isDestroyed()) window.webContents.send('overlay:callstate', summary) }
    callSummary = { active: false, label: '', participants: 0, muted: false, startedAt: null, connectedAt: null, ringing: false, ...summary }
    syncCallWidget(BrowserWindow.fromWebContents(event.sender))
  })
  ipcMain.on('widget:action', (_, action) => {
    const main = mainWindow && !mainWindow.isDestroyed() ? mainWindow : null
    if (!main) return
    // "P2'yi aç" yalnızca pencereyi öne getirir; kullanıcı bıraktığı ekranda kalır.
    if (action === 'open') { showMainWindow(main); return }
    if (action === 'close') { askWidgetClose(main); return }
    main.webContents.send('call:widget-action', action)
  })
  ipcMain.handle('media:open-video', (_, id) => openVideoWindow(id))
  ipcMain.handle('call:widget-visible', () => !loadWidgetState().hidden)
  ipcMain.handle('call:widget-set-visible', (_, visible) => {
    saveWidgetState({ hidden: !visible })
    if (visible) widgetSkippedCall = false
    syncCallWidget(mainWindow)
    return true
  })
}

const videoWindows = new Map()
function openVideoWindow(id) {
  if (!/^[A-Za-z0-9_-]{6,20}$/.test(String(id || ''))) return false
  const existing = videoWindows.get(id)
  if (existing && !existing.isDestroyed()) { existing.show(); existing.focus(); return true }
  const window = new BrowserWindow({
    width: 960, height: 600, backgroundColor: '#0f0f0f', title: 'Video',
    autoHideMenuBar: true, appId: CLIENT_APP_ID, icon: clientWindowIcon(),
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true, partition: 'persist:video' },
  })
  pinWindowIcon(window)
  videoWindows.set(id, window)
  window.once('closed', () => videoWindows.delete(id))
  // Oynatıcının açtığı bağlantılar (kanal, paylaş) sistem tarayıcısına gider.
  window.webContents.setWindowOpenHandler(({ url }) => { if (url.startsWith('https://')) shell.openExternal(url); return { action: 'deny' } })
  void window.loadURL(`https://www.youtube.com/watch?v=${id}`)
  return true
}

// Discord'daki gibi: uygulama açılmadan önce ortada küçük bir pencere çıkar,
// güncelleme denetler, varsa indirip kurar.
let splashWindow = null
function splashIcon() {
  // Simge 96 piksele küçültülüp gömülür: 512x512 PNG'yi olduğu gibi data-URL
  // yapmak sayfa adresini megabaytlara çıkarıyor ve simge hiç görünmüyordu.
  const candidates = [
    path.join(process.resourcesPath || '', 'icon.png'),
    path.join(path.dirname(process.execPath), 'resources', 'icon.png'),
    path.join(app.getAppPath(), 'build', 'icon.png'),
    path.join(__dirname, '..', 'build', 'icon.png'),
  ]
  for (const file of candidates) {
    try {
      const image = nativeImage.createFromPath(file)
      if (!image.isEmpty()) return image.resize({ width: 96, height: 96, quality: 'best' }).toDataURL()
    } catch {}
  }
  serverLog(`Açılış simgesi bulunamadı: ${candidates.join(' | ')}`)
  return ''
}

function showSplash() {
  if (splashWindow && !splashWindow.isDestroyed()) return splashWindow
  splashWindow = new BrowserWindow({
    width: 300, height: 340, frame: false, resizable: false, movable: true, center: true,
    show: false, backgroundColor: '#070b14', title: 'P2', appId: CLIENT_APP_ID, icon: clientWindowIcon(), skipTaskbar: false, alwaysOnTop: true,
    webPreferences: { preload: path.join(__dirname, 'splash-preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  })
  pinWindowIcon(splashWindow)
  splashWindow.loadURL('data:text/html;charset=UTF-8,' + encodeURIComponent(SPLASH_HTML(splashIcon())))
  splashWindow.once('ready-to-show', () => { if (splashWindow && !splashWindow.isDestroyed()) splashWindow.show() })
  splashWindow.on('closed', () => { splashWindow = null })
  return splashWindow
}
function closeSplash() {
  if (splashWindow && !splashWindow.isDestroyed()) splashWindow.destroy()
  splashWindow = null
}

const SPLASH_HTML = (icon) => `<!doctype html><html lang="tr"><head><meta charset="utf-8"><style>${P2_MAVI_CSS}
  html,body{width:100%;height:100%;overflow:hidden}
  body{display:grid;place-items:center;background:radial-gradient(circle at 50% 40%,var(--p2-accent-soft) 0,transparent 55%),var(--p2-bg-0);-webkit-app-region:drag;user-select:none}
  .wrap{display:grid;justify-items:center;gap:18px;padding:0 26px;text-align:center}
  .wrap img,.wrap .p2-logo-fallback{width:96px;height:96px;font-size:34px}
  .kicker{margin:-6px 0 0}
  p{min-height:17px;color:var(--p2-text-3);font-size:12px;line-height:1.45;animation:p2-pop-elastic .5s var(--ease-out) .15s both}
  .bar{width:180px}
</style></head><body>
  <div class="wrap">
    ${icon ? `<img class="p2-logo pop" src="${icon}" alt="">` : '<div class="p2-logo-fallback p2-logo pop">P2</div>'}
    <span class="p2-kicker kicker">P2</span>
    <p id="text">Başlatılıyor…</p>
    <div class="p2-bar bar indeterminate" id="bar"><i id="fill"></i></div>
  </div>
  <script>
    const text=document.getElementById('text'),bar=document.getElementById('bar'),fill=document.getElementById('fill')
    let label='Başlatılıyor…'
    window.p2splash.onText(value=>{label=value;text.textContent=value})
    window.p2splash.onState(state=>{
      if(state.text){label=state.text;text.textContent=state.text}
      if(state.status==='downloading'&&state.percent>0){
        bar.classList.remove('indeterminate')
        fill.style.width=state.percent+'%'
        text.textContent=label.replace(/…$/,'')+' · %'+state.percent
      }else{
        bar.classList.add('indeterminate')
        fill.style.width=''
      }
    })
  </script>
</body></html>`

function createWindow() {
  const window = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 900,
    minHeight: 600,
    backgroundColor: '#070b14',
    title: 'P2',
    appId: CLIENT_APP_ID,
    icon: clientWindowIcon(),
    frame: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      // Pencere simge durumundayken/tepsideyken Chromium zamanlayıcıları ve çizimi kısıyordu:
      // bildirimler gecikiyor, sesli odadaki ölçümler duruyor ve uzun aradan sonra geri
      // dönüşte birikmiş iş yüzünden pencere donuyordu. Arka planda da tam hızda çalışır.
      backgroundThrottling: false,
      additionalArguments: [`--p2-suggested-server=${suggestedServer()}`, `--p2-version=${app.getVersion()}`],
    },
  })
  pinWindowIcon(window)
  mainWindow = window
  window.once('closed',()=>{if(mainWindow===window)mainWindow=null})

  window.on('focus', () => window.flashFrame(false))
  registerWindowBehaviour(window)

  window.webContents.setWindowOpenHandler(({ url }) => handleWindowOpen(url))

  if (isDev) {
    window.loadURL(uiDemo?'http://localhost:5173/?ui-demo=mode':'http://localhost:5173')
  } else {
    loadRenderer(window)
  }
}

// İkinci bir kopya açılırsa yeni bir arayüz portu (dolayısıyla yeni köken ve
// sıfırlanmış yerel ayarlar) doğar. Bu yüzden tek örnek: ikinci açılış var olan
// pencereyi öne getirir.
let booting = true
if (serverMode) app.disableHardwareAcceleration()
const singleInstance = serverMode || app.requestSingleInstanceLock()
if (!singleInstance) app.quit()
if (process.platform === 'win32' && !serverMode) { try { app.setAsDefaultProtocolClient('p2') } catch {} }
function playFromArgs(argv) {
  const found = (argv || []).map(String).find(arg => arg.startsWith('--p2-play=') || arg.startsWith('p2://play/'))
  if (!found) return
  const projectId = found.startsWith('p2://play/') ? found.slice('p2://play/'.length).replace(/\/$/, '') : found.slice('--p2-play='.length)
  if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send('library:play', { projectId })
}
app.on('second-instance', (_event, argv) => { if (mainWindow && !mainWindow.isDestroyed()) showMainWindow(mainWindow); playFromArgs(argv) })
app.on('open-url', (_event, url) => { if (mainWindow && !mainWindow.isDestroyed()) showMainWindow(mainWindow); playFromArgs([url]) })

// Oyun içi overlay tarayıcısı: ekran dışı (OSR) pencerede site izolasyonu açıkken çapraz-kaynak iframe'e
// (ayrı süreç) fare/klavye girdisi ulaşmıyor (scratchpad/iframe-click.cjs ile doğrulandı). Overlay tarayıcı
// sekmesi iframe'le çalıştığından izolasyon denemeleri kapatılır; ana pencere kendi içeriğini yükler.
app.commandLine.appendSwitch('disable-site-isolation-trials')
app.whenReady().then(async () => {
  // Kilidi alamadıysak ikinci kopyayız: hiçbir şey açmadan çekiliriz.
  if (!singleInstance) return
  repairWindowsShortcutIdentity()
  if (!serverMode) {
    updates.resumeServerTask()
    await migrateLegacyPrefs()
    await ensureAppServer()
  }
  if (serverMode) {
    // İkinci bir sunucu süreci ya da zaten yanıt veren bir API varsa sessizce çekil.
    if (await apiAlive() || !loadDatabaseConfig()) { serverLog(await apiAlive() ? 'Zaten çalışan bir API var; bu kopya çekiliyor' : 'Yapılandırma yok; sunucu başlatılmadı'); app.quit(); return }
    serverLog('Sunucu süreci açıldı')
    startPackagedApi()
    return
  }
  registerMediaPermissions()
  registerScreenShare()
  registerOverlayIpc()
  registerSystemAudioIpc()
  registerWindowBehaviourIpc()
  registerSecureSessionIpc()
  registerLegacyPrefsIpc()
  registerUpdaterIpc()
  registerFileIpc()
  registerWindowIpc()
  registerNotificationIpc()
  registerLibraryIpc(() => mainWindow)
  overlayHost.configure({ loadRenderer, suggestedServer })
  registerGameOverlayIpc()
  startPackagedApi()
  // Güncelleme turu kendi küçük penceresinde yapılır; uygulama ancak bittikten
  // sonra açılır (güncelleme kurulacaksa uygulama hiç açılmaz, kurulum başlar).
  const source = updates.savedBase()
  if (app.isPackaged && source) {
    showSplash()
    // Kendi sunucumuzsa API'nin kalkmasını kısa süre bekle, yoksa denetim boşa gider.
    if (/127\.0\.0\.1|localhost/.test(source)) await waitForServer(12000).catch(() => {})
    const outcome = await updates.startupCheck(source, serverLog)
    if (outcome === 'installing') return
    closeSplash()
  }
  createWindow()
  booting = false
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => {
  quitting = true
  if (serverMode) { apiProcess?.kill(); apiProcess = undefined; return }
  stopOwnServerSync()
})
app.on('window-all-closed', () => {
  // Açılışta yalnızca güncelleme penceresi açıkken o kapanınca "tüm pencereler
  // kapandı" tetikleniyor; ana pencere daha oluşmadan uygulama çıkmasın.
  if (serverMode || booting) return
  apiProcess?.kill()
  if (process.platform !== 'darwin') app.quit()
})
