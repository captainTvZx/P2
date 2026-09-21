const { app, BrowserWindow, Menu, Notification, Tray, desktopCapturer, dialog, nativeImage, screen, session, shell, ipcMain, safeStorage, net } = require('electron')
const path = require('node:path')
const http = require('node:http')
const fs = require('node:fs')
const { fork, spawn, execFile } = require('node:child_process')
const { Client } = require('pg')
const { registerGitIpc } = require('./git.cjs')

const isDev = !app.isPackaged
let apiProcess
let apiStarting = false

// Sunucu ayrı bir süreçtir: "P2.exe --p2-server" penceresiz çalışır, tepsi
// simgesiyle yaşar ve P2 kapansa da sürer; açılışta kendiliğinden başlar.
// Kendi userData alt klasörünü kullanır ki arayüzle çakışmasın, ama
// veritabanı yapılandırmasını arayüzün yazdığı yerden okur.
// Not: sunucu süreci userData'yı DEĞİŞTİRMEZ. safeStorage'ın anahtarı userData'daki
// "Local State" dosyasında durur; ayrı klasör ayrı anahtar demek olur ve arayüzün
// yazdığı database.bin çözülemezdi. Tek örnek denetimi port yoklamasıyla yapılır.
const serverMode = process.argv.includes('--p2-server')
const baseUserData = app.getPath('userData')
const API_PORT = Number(process.env.PORT || 4000)

function tokenPath() { return path.join(app.getPath('userData'), 'session.bin') }
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
    execFile('schtasks', ['/Query', '/TN', 'P2 Server'], { windowsHide: true }, error => resolve(!error))
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
async function ensureServerRunning() {
  if (isDev || serverMode || process.env.P2_API_URL) return false
  const config = loadDatabaseConfig()
  if (!config) return false
  // Eski sürümden yalnızca şifreli kopya kaldıysa servis/sunucu süreci için düz kopyayı tamamla.
  if (!loadServerConfig()) { try { fs.mkdirSync(serverDataDir, { recursive: true }); fs.writeFileSync(serverConfigPath(), JSON.stringify({ ...config, port: API_PORT }, null, 2)) } catch (error) { serverLog(`server.json yazılamadı: ${error?.message || error}`) } }
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
async function waitForServer(timeout = 30000) {
  if (isDev || process.env.P2_API_URL) return { ok: true }
  if (!loadDatabaseConfig()) return { ok: false, reason: 'Sunucu yapılandırması yok.', configured: false }
  const started = Date.now()
  lastServerError = ''
  if (!serverChild || serverChild.exited) await ensureServerRunning()
  while (Date.now() - started < timeout) {
    if (await apiAlive(800)) return { ok: true }
    // Süreç çıktıysa daha fazla beklemenin anlamı yok; nedenini günlükten getir.
    if (serverChild?.exited) { const log = readServerLog(); return { ok: false, reason: lastServerError || reasonFromLog(log) || `Sunucu süreci ${serverChild.code} koduyla kapandı.`, configured: true, log } }
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
  ipcMain.handle('database:status', () => Boolean(loadDatabaseConfig()))
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
const ALLOWED_PERMISSIONS = ['media', 'display-capture', 'notifications', 'clipboard-sanitized-write']
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
function openPopoutWindow(query, title) {
  const existing = popouts.get(query)
  if (existing && !existing.isDestroyed()) { existing.show(); existing.focus(); return }
  const window = new BrowserWindow({
    width: 980,
    height: 760,
    minWidth: 620,
    minHeight: 480,
    backgroundColor: '#101114',
    title,
    frame: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      additionalArguments: [`--p2-suggested-server=${suggestedServer()}`],
    },
  })
  popouts.set(query, window)
  window.on('closed', () => popouts.delete(query))
  window.webContents.setWindowOpenHandler(({ url }) => handleWindowOpen(url))
  if (isDev) window.loadURL(`http://localhost:5173/?${query}`)
  else window.loadFile(path.join(__dirname, '..', 'dist', 'index.html'), { search: query })
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
  // Bir yükseltmeden sonra bekleyen migration'lar API açılmadan önce uygulanır.
  migrateConfiguredDatabase(config.databaseUrl).then(() => true, error => { serverLog(`Migration uygulanamadı: ${error?.message || error}`); console.error('Migration uygulanamadı:', error?.message || error); return false }).then(migrated => {
    apiStarting = false
    if (!migrated) { if (serverMode) app.quit(); return }
    if (apiProcess) return
    const serverFile = path.join(app.getAppPath(), 'server', 'dist', 'main.js')
    serverLog(`API başlatılıyor (${serverMode ? 'sunucu süreci' : 'arayüz'}), port ${API_PORT}`)
    apiProcess = fork(serverFile, [], { env: { ...process.env, DATABASE_URL: config.databaseUrl, JWT_ACCESS_SECRET: config.accessSecret, JWT_REFRESH_SECRET: config.refreshSecret, PORT: String(API_PORT) }, stdio: ['ignore', 'pipe', 'pipe', 'ipc'] })
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
  // Ayrı bir simge dosyası taşımamak için 16x16 basit bir işaret üretilir.
  const size = 16
  const buffer = Buffer.alloc(size * size * 4)
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const index = (y * size + x) * 4
      const edge = x === 0 || y === 0 || x === size - 1 || y === size - 1
      const bar = x >= 4 && x <= 5 && y >= 4 && y <= 11
      const head = y >= 4 && y <= 6 && x >= 6 && x <= 10
      const belly = y >= 7 && y <= 8 && x >= 9 && x <= 10
      const wave = y >= 9 && y <= 9 && x >= 6 && x <= 10
      const on = bar || head || belly || wave
      buffer[index] = on ? 0xf5 : 0x27
      buffer[index + 1] = on ? 0xf6 : 0x3c
      buffer[index + 2] = on ? 0xfa : 0x75
      buffer[index + 3] = on || edge ? 0xff : 0xe6
    }
  }
  return nativeImage.createFromBuffer(buffer, { width: size, height: size })
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
  const shouldShow = callSummary.active && hidden
  if (!shouldShow) {
    if (callWidget && !callWidget.isDestroyed()) callWidget.destroy()
    callWidget = null
    return
  }
  if (callWidget && !callWidget.isDestroyed()) { callWidget.webContents.send('widget:update', callSummary); return }
  const display = screen.getPrimaryDisplay().workArea
  callWidget = new BrowserWindow({
    width: 404, height: 94, x: display.x + display.width - 424, y: display.y + display.height - 116,
    frame: false, resizable: false, movable: true, skipTaskbar: true, alwaysOnTop: true,
    transparent: true, backgroundColor: '#00000000', focusable: true, show: false, hasShadow: false,
    webPreferences: { preload: path.join(__dirname, 'widget-preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  })
  callWidget.setAlwaysOnTop(true, 'screen-saver')
  callWidget.loadURL('data:text/html;charset=UTF-8,' + encodeURIComponent(WIDGET_HTML))
  callWidget.once('ready-to-show', () => {
    if (!callWidget || callWidget.isDestroyed()) return
    callWidget.showInactive()
    callWidget.webContents.send('widget:update', callSummary)
  })
}

const WIDGET_HTML = `<!doctype html><html lang="tr"><head><meta charset="utf-8"><style>
  *{box-sizing:border-box;margin:0}
  body{font-family:Inter,"Segoe UI",sans-serif;-webkit-app-region:drag;user-select:none}
  .bubble{display:grid;grid-template-columns:10px 1fr auto;align-items:center;gap:9px;height:64px;margin:15px;padding:0 10px;border:1px solid rgba(126,168,214,.24);border-radius:13px;background:linear-gradient(160deg,#252b35,#1b1f26);box-shadow:0 6px 16px rgba(0,0,0,.34), 0 1px 3px rgba(0,0,0,.28), inset 0 1px 0 rgba(255,255,255,.05)}
  .dot{width:9px;height:9px;border-radius:50%;background:#3fae61}
  .info{min-width:0}
  .info b{display:block;color:#e2e6ea;font-size:11px}
  .info small{color:#79818c;font-size:10px;font-variant-numeric:tabular-nums}
  .actions{display:flex;gap:5px;-webkit-app-region:no-drag}
  .actions button{width:32px;height:32px;display:grid;place-items:center;border:1px solid #3a4049;border-radius:9px;background:rgba(255,255,255,.05);color:#cbd1d8;cursor:pointer}
  .actions button.mute{width:auto;grid-template-columns:17px auto;gap:6px;padding:0 9px;font:600 9px Inter,"Segoe UI",sans-serif}
  .actions button svg{width:17px;height:17px;fill:currentColor}
  .actions button.on{border-color:#5c3036;background:#3a2126;color:#e6a2a8}
  .actions button:hover{background:rgba(255,255,255,.1)}
  .actions .leave{border-color:#5c3036;background:#48252b;color:#f0b8bd}
</style></head><body>
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
  </script>
</body></html>`

function registerWindowBehaviour(window) {
  window.on('minimize', () => syncCallWidget(window))
  window.on('restore', () => syncCallWidget(window))
  window.on('show', () => syncCallWidget(window))
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

function registerWindowBehaviourIpc() {
  ipcMain.on('window:set-close-action', (_, action) => {
    if (['ask', 'tray', 'minimize', 'quit'].includes(action)) closeAction = action
  })
  ipcMain.on('call:update', (event, summary) => {
    callSummary = { active: false, label: '', participants: 0, muted: false, startedAt: null, connectedAt: null, ringing: false, ...summary }
    syncCallWidget(BrowserWindow.fromWebContents(event.sender))
  })
  ipcMain.on('widget:action', (_, action) => {
    const main = mainWindow && !mainWindow.isDestroyed() ? mainWindow : null
    if (!main) return
    if (action === 'open') showMainWindow(main)
    main.webContents.send('call:widget-action', action)
  })
}

function registerYoutubeHeaders(){
  session.defaultSession.webRequest.onBeforeSendHeaders({urls:['*://*.youtube.com/*','*://*.youtube-nocookie.com/*']},(details,callback)=>{
    details.requestHeaders.Referer='https://p2.app/'
    details.requestHeaders.Origin='https://p2.app'
    callback({requestHeaders:details.requestHeaders})
  })
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 900,
    minHeight: 600,
    backgroundColor: '#101114',
    title: 'P2',
    frame: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      additionalArguments: [`--p2-suggested-server=${suggestedServer()}`],
    },
  })
  mainWindow = window
  window.once('closed',()=>{if(mainWindow===window)mainWindow=null})

  window.on('focus', () => window.flashFrame(false))
  registerWindowBehaviour(window)

  window.webContents.setWindowOpenHandler(({ url }) => handleWindowOpen(url))

  if (isDev) {
    window.loadURL('http://localhost:5173')
  } else {
    window.loadFile(path.join(__dirname, '..', 'dist', 'index.html'))
  }
}

app.whenReady().then(async () => {
  if (serverMode) {
    // İkinci bir sunucu süreci ya da zaten yanıt veren bir API varsa sessizce çekil.
    if (await apiAlive() || !loadDatabaseConfig()) { serverLog(await apiAlive() ? 'Zaten çalışan bir API var; bu kopya çekiliyor' : 'Yapılandırma yok; sunucu başlatılmadı'); app.quit(); return }
    app.setAppUserModelId('com.p2.server')
    serverLog('Sunucu süreci açıldı')
    startPackagedApi()
    return
  }
  app.setAppUserModelId('com.p2.desktop')
  registerMediaPermissions()
  registerYoutubeHeaders()
  registerScreenShare()
  registerOverlayIpc()
  registerSystemAudioIpc()
  registerWindowBehaviourIpc()
  registerSecureSessionIpc()
  registerFileIpc()
  registerWindowIpc()
  registerNotificationIpc()
  registerGitIpc()
  startPackagedApi()
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => { quitting = true; if (serverMode) { apiProcess?.kill(); apiProcess = undefined } })
app.on('window-all-closed', () => {
  if (serverMode) return
  apiProcess?.kill()
  if (process.platform !== 'darwin') app.quit()
})
