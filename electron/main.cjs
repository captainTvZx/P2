const { app, BrowserWindow, Menu, Notification, Tray, dialog, nativeImage, screen, session, shell, ipcMain, safeStorage } = require('electron')
const path = require('node:path')
const fs = require('node:fs')
const { fork } = require('node:child_process')
const { Client } = require('pg')

const isDev = !app.isPackaged
let apiProcess
let apiStarting = false

function tokenPath() { return path.join(app.getPath('userData'), 'session.bin') }
function databasePath() { return path.join(app.getPath('userData'), 'database.bin') }
function loadDatabaseConfig() {
  try { return JSON.parse(safeStorage.decryptString(fs.readFileSync(databasePath()))) } catch { return null }
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
  apiProcess?.kill(); apiProcess = undefined; startPackagedApi()
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
  ipcMain.handle('database:install', async (_, input) => {
    if (!safeStorage.isEncryptionAvailable()) return { ok: false, message: 'Windows güvenli kasası kullanılamıyor.' }
    try { await installDatabase(input); return { ok: true } }
    catch (error) { return { ok: false, message: error?.message || 'Veritabanı kurulamadı.' } }
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
}

function startPackagedApi() {
  if (isDev || process.env.P2_API_URL || apiProcess || apiStarting) return
  const config = loadDatabaseConfig()
  if (!config) return
  apiStarting = true
  // Bir yükseltmeden sonra bekleyen migration'lar API açılmadan önce uygulanır.
  migrateConfiguredDatabase(config.databaseUrl).catch(error => { console.error('Migration uygulanamadı:', error?.message || error) }).then(() => {
    apiStarting = false
    if (apiProcess) return
    const serverFile = path.join(app.getAppPath(), 'server', 'dist', 'main.js')
    apiProcess = fork(serverFile, [], { env: { ...process.env, DATABASE_URL: config.databaseUrl, JWT_ACCESS_SECRET: config.accessSecret, JWT_REFRESH_SECRET: config.refreshSecret, PORT: '4000' }, stdio: 'ignore' })
    apiProcess.once('exit', () => { apiProcess = undefined })
  })
}

// ---- Kapatma davranışı, sistem tepsisi ve masaüstü arama balonu ----
let closeAction = 'ask'
let quitting = false
let tray = null
let callWidget = null
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
    width: 310, height: 74, x: display.x + display.width - 330, y: display.y + display.height - 96,
    frame: false, resizable: false, movable: true, skipTaskbar: true, alwaysOnTop: true,
    transparent: true, backgroundColor: '#00000000', focusable: true, show: false,
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
  .bubble{display:grid;grid-template-columns:10px 1fr auto;align-items:center;gap:9px;height:64px;margin:5px;padding:0 10px;border:1px solid rgba(126,168,214,.28);border-radius:12px;background:linear-gradient(160deg,#252b35,#1b1f26);box-shadow:0 10px 26px rgba(0,0,0,.55)}
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
      <button id="open" title="P2'yi aç"><svg viewBox="0 0 24 24"><path d="M4 4h7v2H6v5H4V4Zm16 16h-7v-2h5v-5h2v7Z"/><path d="M4 20v-7h2v5h5v2H4ZM20 4v7h-2V6h-5V4h7Z"/></svg></button>
      <button id="leave" class="leave" title="Aramadan ayrıl"><svg viewBox="0 0 24 24" style="transform:rotate(133deg)"><path d="M12 7.4c-3.3 0-6.4.9-8.9 2.5-.9.6-1.2 1.8-.7 2.7l1 1.8c.5.9 1.7 1.3 2.6.8l2.2-1.1c.6-.3 1-1 1-1.7v-1c1.8-.5 3.8-.5 5.6 0v1c0 .7.4 1.4 1 1.7l2.2 1.1c1 .5 2.1.1 2.6-.8l1-1.8c.5-.9.2-2.1-.7-2.7A16.4 16.4 0 0 0 12 7.4Z"/></svg></button>
    </div>
  </div>
  <script>
    let state={startedAt:null,connectedAt:null,ringing:false,participants:0}
    const label=document.getElementById('label'),meta=document.getElementById('meta'),mute=document.getElementById('mute'),muteLabel=document.getElementById('muteLabel'),slash=document.getElementById('micSlash')
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
      tick()
    })
    mute.onclick=()=>window.p2widget.action('mute')
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
    if (closeAction === 'quit') { quitting = true; return }
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
      if (picked === 'quit') { quitting = true; app.quit(); return }
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
    const [main] = BrowserWindow.getAllWindows().filter(item => item !== callWidget)
    if (!main) return
    if (action === 'open') showMainWindow(main)
    main.webContents.send('call:widget-action', action)
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
    },
  })

  window.on('focus', () => window.flashFrame(false))
  registerWindowBehaviour(window)

  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https://')) shell.openExternal(url)
    return { action: 'deny' }
  })

  if (isDev) {
    window.loadURL('http://localhost:5173')
  } else {
    window.loadFile(path.join(__dirname, '..', 'dist', 'index.html'))
  }
}

app.whenReady().then(() => {
  app.setAppUserModelId('com.p2.desktop')
  registerMediaPermissions()
  registerWindowBehaviourIpc()
  registerSecureSessionIpc()
  registerWindowIpc()
  registerNotificationIpc()
  startPackagedApi()
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => { quitting = true })
app.on('window-all-closed', () => {
  apiProcess?.kill()
  if (process.platform !== 'darwin') app.quit()
})
