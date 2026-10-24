const { app, BrowserWindow, ipcMain, shell, nativeImage } = require('electron')
const { execFile, fork } = require('node:child_process')
const crypto = require('node:crypto')
const fs = require('node:fs')
const http = require('node:http')
const path = require('node:path')
const { logoDataUrl } = require('./p2-mavi.cjs')
const { Client } = require('pg')
const serverUpdates = require('./server-updater.cjs')
const serverBackup = require('../server/ops/backup.cjs')

// Client ve Server aynı Electron paket kökünden üretilse de kilit/ayar dizinleri
// çakışmamalı. Bu çağrı tek-örnek kilidi alınmadan önce yapılır.
app.setPath('userData', path.join(app.getPath('appData'), 'P2 Server'))
const headless = process.argv.includes('--p2-server') || process.argv.includes('--headless')
const dataDir = process.platform === 'win32'
  ? path.join(process.env.ProgramData || 'C:\\ProgramData', 'P2')
  : path.join(app.getPath('userData'), 'server')
const configFile = path.join(dataDir, 'server.json')
const logFile = path.join(dataDir, 'server.log')
let apiProcess = null
let mainWindow = null
let stopping = false
let lastError = ''
let quitting = false
// Konsol kapanırken kendi başlattığı API sürecini arkada bırakır (kullanıcı 'Durdur' demedikçe).
let keepApiOnQuit = false
let publicIp = ''

function serverIconPath() {
  const choices = [path.join(process.resourcesPath || '', 'icon-server.png'), path.join(__dirname, '..', 'build', 'icon-server.png')]
  return choices.find(file => fs.existsSync(file)) || ''
}
function ensureServiceDescriptor() {
  if (!app.isPackaged || process.platform !== 'win32') return true
  // Program Files normal kullanıcı için salt okunurdur. XML kurulum sırasında
  // yönetici yetkili betik tarafından zaten hazırlanır; çalışma anında tekrar
  // yazmaya çalışmak hizmeti durdurma ve güncelleme akışını EPERM ile kesiyordu.
  const descriptor = path.join(path.dirname(process.execPath), 'P2ServerService.xml')
  if (fs.existsSync(descriptor)) return true
  writeLog(`Windows hizmeti yapılandırması bulunamadı: ${descriptor}`)
  return false
}
async function resolvePublicIp() {
  try {
    const response = await fetch('https://api.ipify.org?format=json', { signal: AbortSignal.timeout(6000) })
    const value = String((await response.json())?.ip || '').trim()
    if (/^(?:\d{1,3}\.){3}\d{1,3}$/.test(value) || value.includes(':')) publicIp = value
  } catch {}
  await broadcast()
}

function readConfig() {
  try { return JSON.parse(fs.readFileSync(configFile, 'utf8')) } catch { return null }
}
function writeLog(value) {
  const clean = String(value || '').replace(/\x1b\[[0-9;]*m/g, '').trim()
  if (!clean) return
  const line = `[${new Date().toISOString()}] ${clean}\n`
  if (/error|hata|failed|refused|authentication|does not exist/i.test(clean)) lastError = clean.slice(0, 600)
  try { fs.mkdirSync(dataDir, { recursive: true }); fs.appendFileSync(logFile, line) } catch {}
  broadcast()
}
function port() { return Number(readConfig()?.port || 4000) }
function apiAlive(timeout = 900) {
  return new Promise(resolve => {
    const request = http.get({ host: '127.0.0.1', port: port(), path: '/health', timeout }, response => {
      let body = ''
      response.on('data', chunk => { body += chunk })
      response.on('end', () => { try { resolve(JSON.parse(body)?.status === 'ok') } catch { resolve(false) } })
    })
    request.on('error', () => resolve(false))
    request.on('timeout', () => { request.destroy(); resolve(false) })
  })
}
function recentLog() {
  try { return fs.readFileSync(logFile, 'utf8').split('\n').filter(Boolean).slice(-120).join('\n') } catch { return '' }
}
async function state() {
  const config = readConfig()
  const service = await serviceRegistered()
  return {
    version: app.getVersion(), configured: Boolean(config?.databaseUrl), running: await apiAlive(),
    managed: Boolean(apiProcess), port: Number(config?.port || 4000), publicIp,
    address: `http://${publicIp ? (publicIp.includes(':') ? `[${publicIp}]` : publicIp) : 'public-ip-bekleniyor'}:${Number(config?.port || 4000)}`,
    localAddress: `http://127.0.0.1:${Number(config?.port || 4000)}`,
    dataDir, updatesDir: path.join(dataDir, 'updates'), log: recentLog(), error: lastError, service,
    update: serverUpdates.getState(),
    iconUrl: serverIconDataUrl(),
  }
}
let serverIconCache = null
function serverIconDataUrl() { if (serverIconCache === null) serverIconCache = logoDataUrl('icon-server.png'); return serverIconCache }

function justUpdated() { try { return Date.now() - fs.statSync(process.execPath).mtimeMs < 3 * 60 * 1000 } catch { return false } }
function serviceRegistered() {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve(false)
    execFile('sc.exe', ['query', 'P2Server'], { windowsHide: true }, error => resolve(!error))
  })
}
function elevatedService(command) {
  return new Promise(resolve => {
    if (!ensureServiceDescriptor()) return resolve(false)
    const wrapper = path.join(path.dirname(process.execPath), 'P2ServerService.exe')
    if (!fs.existsSync(wrapper)) return resolve(false)
    const line = `$process = Start-Process -FilePath '${wrapper.replace(/'/g, "''")}' -ArgumentList '${command}' -Verb RunAs -Wait -PassThru; exit $process.ExitCode`
    execFile('powershell', ['-NoProfile', '-Command', line], { windowsHide: true }, error => resolve(!error))
  })
}
function elevatedScript(name) {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve(false)
    const script = path.join(path.dirname(process.execPath), name)
    if (!fs.existsSync(script)) return resolve(false)
    const command = `Start-Process -FilePath '${script.replace(/'/g, "''")}' -Verb RunAs -Wait`
    execFile('powershell', ['-NoProfile', '-Command', command], { windowsHide: true }, error => resolve(!error))
  })
}
async function installService() {
  if (!ensureServiceDescriptor()) return { ok: false, message: 'Windows hizmeti yapılandırması oluşturulamadı.' }
  await stopServer()
  const launched = await elevatedScript('p2-server-install.bat')
  for (let attempt = 0; attempt < 20; attempt++) {
    if (await serviceRegistered()) { writeLog('P2 Server Windows açılış servisi olarak kaydedildi'); await broadcast(); return { ok: true } }
    await new Promise(resolve => setTimeout(resolve, 500))
  }
  await startServer().catch(() => {})
  return { ok: false, message: launched ? 'Servis kaydı tamamlanamadı.' : 'Yönetici izni verilmedi veya servis betiği bulunamadı.' }
}
async function removeService() {
  const removed = await elevatedScript('p2-server-remove.bat')
  for (let attempt = 0; attempt < 20; attempt++) {
    if (!(await serviceRegistered())) break
    await new Promise(resolve => setTimeout(resolve, 500))
  }
  await startServer().catch(() => {})
  writeLog('P2 Server Windows açılış servisi kaldırıldı')
  await broadcast()
  return { ok: removed && !(await serviceRegistered()) }
}
async function broadcast() {
  if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send('server-console:state', await state())
}

async function applyMigrations(target) {
  await target.query('CREATE TABLE IF NOT EXISTS "_p2_migrations" ("name" TEXT PRIMARY KEY, "appliedAt" TIMESTAMPTZ NOT NULL DEFAULT NOW())')
  const root = path.join(app.getAppPath(), 'server', 'prisma', 'migrations')
  const names = fs.readdirSync(root, { withFileTypes: true }).filter(item => item.isDirectory()).map(item => item.name).sort()
  for (const name of names) {
    if ((await target.query('SELECT 1 FROM "_p2_migrations" WHERE "name" = $1', [name])).rowCount) continue
    const prismaTable = await target.query(`SELECT to_regclass('public."_prisma_migrations"') AS table_name`)
    if (prismaTable.rows[0]?.table_name) {
      const done = await target.query('SELECT 1 FROM "_prisma_migrations" WHERE "migration_name" = $1 AND "finished_at" IS NOT NULL AND "rolled_back_at" IS NULL', [name])
      if (done.rowCount) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
    }
    if (name.endsWith('_init')) {
      const legacy = await target.query(`SELECT to_regclass('public."User"') AS table_name`)
      if (legacy.rows[0]?.table_name) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
      await target.query('DROP TYPE IF EXISTS "UserRole" CASCADE; DROP TYPE IF EXISTS "ProjectRole" CASCADE; DROP TYPE IF EXISTS "FriendshipStatus" CASCADE; DROP TYPE IF EXISTS "UserStatus" CASCADE;')
    }
    await target.query('BEGIN')
    try {
      await target.query(fs.readFileSync(path.join(root, name, 'migration.sql'), 'utf8'))
      await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name])
      await target.query('COMMIT')
    } catch (error) { await target.query('ROLLBACK'); throw error }
  }
}

async function configure(input) {
  const host = String(input?.host || '').trim()
  const dbPort = Number(input?.dbPort)
  const apiPort = Number(input?.apiPort || 4000)
  const username = String(input?.username || '').trim()
  const password = String(input?.password || '')
  const database = String(input?.database || '').trim().toLowerCase()
  if (!host || !username || !password || !Number.isInteger(dbPort) || dbPort < 1 || dbPort > 65535 || !Number.isInteger(apiPort) || apiPort < 1 || apiPort > 65535 || !/^[a-z][a-z0-9_]{0,62}$/.test(database)) throw new Error('Veritabanı veya API portu ayarları geçersiz.')
  const connection = { host, port: dbPort, user: username, password, ssl: input.ssl ? { rejectUnauthorized: false } : false }
  const admin = new Client({ ...connection, database: 'postgres' })
  await admin.connect()
  try {
    if (!(await admin.query('SELECT 1 FROM pg_database WHERE datname = $1', [database])).rowCount) await admin.query(`CREATE DATABASE "${database}" ENCODING 'UTF8'`)
  } finally { await admin.end() }
  const target = new Client({ ...connection, database })
  await target.connect()
  try { await applyMigrations(target) } finally { await target.end() }
  const databaseUrl = `postgresql://${encodeURIComponent(username)}:${encodeURIComponent(password)}@${host}:${dbPort}/${database}?schema=public${input.ssl ? '&sslmode=require' : ''}`
  const previous = readConfig() || {}
  const config = {
    databaseUrl, port: apiPort,
    accessSecret: previous.accessSecret || crypto.randomBytes(48).toString('hex'),
    refreshSecret: previous.refreshSecret || crypto.randomBytes(48).toString('hex'),
  }
  fs.mkdirSync(dataDir, { recursive: true })
  fs.writeFileSync(configFile, JSON.stringify(config, null, 2), { mode: 0o600 })
  writeLog(`Yapılandırma kaydedildi; API portu ${apiPort}`)
  await restartServer()
  return { ok: true }
}

async function startServer() {
  const config = readConfig()
  if (!config?.databaseUrl) return { ok: false, message: 'Önce PostgreSQL bağlantısını yapılandırın.' }
  if (await apiAlive()) { await broadcast(); return { ok: true, alreadyRunning: true } }
  if (apiProcess) return { ok: true }
  // Görünür konsol kayıtlı hizmeti yönetir. WinSW tarafından --p2-server ile
  // başlatılan süreç ise hizmetin kendisidir; burada tekrar WinSW'yi çağırmak
  // hizmeti sonsuz bir kendi-kendini başlatma döngüsüne sokar.
  if (!headless && await serviceRegistered()) {
    // Az önce güncellendiysek (exe yeni yazılmış) hizmet WinSW tarafından yeniden başlatılıyordur: yönetici izni
    // istemeden önce 60 sn beklenir.
    if (justUpdated()) for (let attempt = 0; attempt < 150; attempt++) { if (await apiAlive()) { await broadcast(); return { ok: true, service: true } } await new Promise(resolve => setTimeout(resolve, 400)) }
    const started = await elevatedService('start')
    for (let attempt = 0; attempt < 30; attempt++) {
      if (await apiAlive()) { await broadcast(); return { ok: true, service: true } }
      await new Promise(resolve => setTimeout(resolve, 400))
    }
    return { ok: false, message: started ? 'Windows hizmeti başladı ancak API yanıt vermedi.' : 'Windows hizmeti başlatılamadı.' }
  }
  const target = new Client({ connectionString: config.databaseUrl })
  try { await target.connect(); await applyMigrations(target) } finally { await target.end().catch(() => {}) }
  const entry = path.join(app.getAppPath(), 'server', 'dist', 'main.js')
  writeLog(`P2 API başlatılıyor: 0.0.0.0:${config.port || 4000}`)
  // Çıktı doğrudan server.log'a yazılır (boru değil): konsol kapanınca da süreç yazmaya devam edebilir.
  fs.mkdirSync(dataDir, { recursive: true })
  const logFd = fs.openSync(logFile, 'a')
  apiProcess = fork(entry, [], {
    env: {
      ...process.env, DATABASE_URL: config.databaseUrl,
      JWT_ACCESS_SECRET: config.accessSecret, JWT_REFRESH_SECRET: config.refreshSecret,
      P2_SERVER_VERSION: app.getVersion(),
      PORT: String(config.port || 4000), P2_UPDATE_DIR: path.join(dataDir, 'updates'),
      P2_UPLOAD_DIR: process.env.P2_UPLOAD_DIR || path.join(dataDir, 'uploads'),
      P2_RELEASE_DIR: process.env.P2_RELEASE_DIR || path.join(dataDir, 'releases'),
    }, stdio: ['ignore', logFd, logFd, 'ipc'], detached: !headless, windowsHide: true,
  })
  try { fs.closeSync(logFd) } catch {}
  apiProcess.once('exit', code => {
    apiProcess = null
    if (!stopping) writeLog(`P2 API kapandı (kod ${code})`)
    void broadcast()
    // WinSW gerçek hizmet durumunu API ile aynı tutabilsin ve beklenmeyen
    // kapanışta XML'deki yeniden başlatma ilkesi devreye girebilsin.
    if (headless) app.quit()
  })
  setTimeout(() => void broadcast(), 1000)
  return { ok: true }
}
// options.selfStop: hizmet kayıtlıysa `sc stop` (yönetici izni ister) yerine API'ye HMAC'li kapanma isteği gönderilir;
// hizmet süreci çıkar, WinSW kurulum bittikten sonra yeniden başlatır. Güncelleme bu yolu kullanır (UAC yok).
async function stopServer(options = {}) {
  stopping = true
  const child = apiProcess
  apiProcess = null
  if (child) {
    child.kill()
    await new Promise(resolve => { const timer = setTimeout(resolve, 2500); child.once('exit', () => { clearTimeout(timer); resolve() }) })
  } else if (!options.selfStop && await serviceRegistered()) {
    const stopped = await elevatedService('stop')
    if (!stopped) { stopping = false; return { ok: false, message: 'Windows hizmeti durdurulamadı.' } }
    for (let attempt = 0; attempt < 30 && await apiAlive(); attempt++) await new Promise(resolve => setTimeout(resolve, 300))
  } else if (await apiAlive()) {
    const config = readConfig()
    await new Promise(resolve => {
      const token = config?.accessSecret ? crypto.createHmac('sha256', config.accessSecret).update('p2-shutdown').digest('hex') : ''
      const body = JSON.stringify({ token })
      const request = http.request({ host: '127.0.0.1', port: port(), path: '/updates/stop', method: 'POST', timeout: 4000, headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) } }, response => { response.resume(); resolve() })
      request.on('error', () => resolve()); request.on('timeout', () => { request.destroy(); resolve() }); request.end(body)
    })
    for (let attempt = 0; attempt < 20 && await apiAlive(); attempt++) await new Promise(resolve => setTimeout(resolve, 300))
  }
  stopping = false
  writeLog('P2 API yönetim konsolundan durduruldu')
  await broadcast()
  const running = await apiAlive()
  return { ok: !running, message: running ? 'API süreci durmadı.' : undefined }
}
async function restartServer() {
  const stopped = await stopServer()
  if (!stopped.ok) return stopped
  return startServer()
}

function backupName() {
  const date = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '')
  return `P2-Server-Backup-${date}.p2backup`
}
async function createServerBackup(options) {
  const config = readConfig()
  if (!config?.databaseUrl) return { ok: false, message: 'Önce PostgreSQL bağlantısını yapılandır.' }
  const choice = await dialog.showSaveDialog(mainWindow || undefined, {
    title: 'P2 Server yedeğini kaydet', defaultPath: path.join(app.getPath('documents'), backupName()),
    filters: [{ name: 'P2 Server yedeği', extensions: ['p2backup'] }],
  })
  if (choice.canceled || !choice.filePath) return { ok: false, canceled: true }
  try {
    writeLog('P2 Server yedeği hazırlanıyor')
    const result = await serverBackup.createBackup({ databaseUrl: config.databaseUrl, dataDir, destination: choice.filePath, version: app.getVersion(), includeReleases: Boolean(options && options.includeReleases) })
    writeLog(`Yedek oluşturuldu: ${result.path}`)
    return result
  } catch (error) {
    writeLog(`Yedek oluşturulamadı: ${error?.stack || error}`)
    return { ok: false, message: String(error?.message || error) }
  }
}
async function restoreServerBackup() {
  const config = readConfig()
  if (!config?.databaseUrl) return { ok: false, message: 'Önce hedef PostgreSQL bağlantısını yapılandır.' }
  const choice = await dialog.showOpenDialog(mainWindow || undefined, {
    title: 'P2 Server yedeğini yükle', properties: ['openFile'],
    filters: [{ name: 'P2 Server yedeği', extensions: ['p2backup'] }],
  })
  if (choice.canceled || !choice.filePaths[0]) return { ok: false, canceled: true }
  const confirmation = await dialog.showMessageBox(mainWindow || undefined, {
    type: 'warning', title: 'P2 Server yedeğini yükle',
    message: 'Mevcut veritabanı seçilen yedekle değiştirilecek.',
    detail: 'Sunucu işlem sırasında durdurulur. Mevcut uploads klasörü silinmez; geri alınabilmesi için tarihli bir klasöre taşınır.',
    buttons: ['Yedeği yükle', 'Vazgeç'], defaultId: 1, cancelId: 1, noLink: true,
  })
  if (confirmation.response !== 0) return { ok: false, canceled: true }
  try {
    writeLog(`Yedek yükleniyor: ${choice.filePaths[0]}`)
    const result = await serverBackup.restoreBackup({ databaseUrl: config.databaseUrl, dataDir, source: choice.filePaths[0], stop: stopServer, start: startServer })
    writeLog(`Yedek başarıyla yüklendi (${result.manifest?.createdAt || 'tarih bilinmiyor'})`)
    return result
  } catch (error) {
    writeLog(`Yedek yüklenemedi: ${error?.stack || error}`)
    return { ok: false, message: String(error?.message || error) }
  }
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1000, height: 720, minWidth: 780, minHeight: 560, backgroundColor: '#0d1016',
    title: 'P2 Server Console', autoHideMenuBar: true, icon: serverIconPath() || undefined,
    webPreferences: { preload: path.join(__dirname, 'server-preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  })
  mainWindow.loadFile(path.join(__dirname, 'server-console.html'))
  // Kapat: soru sorulmaz, konsol kapanır. Sunucu (Windows hizmeti ya da konsolun başlattığı API süreci)
  // arka planda çalışmaya devam eder; durdurmak için konsoldaki "Durdur" kullanılır.
  mainWindow.on('close', () => { quitting = true; keepApiOnQuit = Boolean(apiProcess) })
  mainWindow.on('closed', () => { mainWindow = null })
}

function registerIpc() {
  ipcMain.handle('server-console:state', () => state())
  ipcMain.handle('server-console:configure', async (_event, input) => { try { return await configure(input) } catch (error) { writeLog(error?.stack || error); return { ok: false, message: String(error?.message || error) } } })
  ipcMain.handle('server-console:start', async () => { try { return await startServer() } catch (error) { writeLog(error?.stack || error); return { ok: false, message: String(error?.message || error) } } })
  ipcMain.handle('server-console:stop', () => stopServer())
  ipcMain.handle('server-console:restart', async () => { try { return await restartServer() } catch (error) { writeLog(error?.stack || error); return { ok: false, message: String(error?.message || error) } } })
  ipcMain.handle('server-console:service-install', () => installService())
  ipcMain.handle('server-console:service-remove', () => removeService())
  ipcMain.handle('server-console:update-check', () => serverUpdates.check())
  ipcMain.handle('server-console:update-install', () => serverUpdates.install(stopServer))
  ipcMain.handle('server-console:backup-create', (_event, options) => createServerBackup(options))
  ipcMain.handle('server-console:backup-restore', () => restoreServerBackup())
  ipcMain.handle('server-console:open-data', () => { fs.mkdirSync(dataDir, { recursive: true }); shell.openPath(dataDir); return true })
}

// Arka plan servisi ve görünür konsol aynı anda çalışabilir. Yalnız görünür
// konsol için tek örnek kilidi kullanılır; API çoğalmasını port denetimi önler.
const testMultiple = process.argv.includes('--p2-test-multiple')
const lock = headless || testMultiple ? true : app.requestSingleInstanceLock()
if (!lock) app.quit()
app.on('second-instance', () => { if (mainWindow) { mainWindow.show(); mainWindow.focus() } })
app.whenReady().then(async () => {
  if (!lock) return
  app.setAppUserModelId('com.p2.server')
  ensureServiceDescriptor()
  registerIpc()
  if (!headless) { createWindow(); void resolvePublicIp() }
  try {
    const started = await startServer()
    if (headless && started?.alreadyRunning) app.quit()
    if (!headless) serverUpdates.init({ port: port(), version: app.getVersion(), stopServer, beforeQuit: () => { quitting = true }, broadcast: state => { if (mainWindow && !mainWindow.isDestroyed()) mainWindow.webContents.send('server-console:update', state) } })
  } catch (error) { writeLog(error?.stack || error); if (headless) app.quit() }
})
app.on('before-quit', () => {
  if (headless || !keepApiOnQuit) { stopping = true; try { apiProcess?.kill() } catch {}; return }
  // Konsol kapanıyor, API sürüyor: IPC kanalı kapatılır, süreç referansı bırakılır (Windows'ta bağımsız yaşar).
  try { apiProcess?.removeAllListeners('exit'); apiProcess?.disconnect?.() } catch {}
  try { apiProcess?.unref() } catch {}
  apiProcess = null
})
app.on('window-all-closed', () => { if (!headless) app.quit() })
