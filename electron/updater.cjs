const { app, BrowserWindow } = require('electron')
const { execFile } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')

// Otomatik güncelleme. Güncelleme dosyaları, kullanıcının zaten bağlı olduğu
// P2 sunucusundan gelir (GET <api>/updates/latest.yml) — ayrı bir dağıtım
// servisi yok. Sessizce indirilir, hazır olunca arayüzde şerit gösterilir;
// kurulum kullanıcı "yeniden başlat" derse ya da uygulama kapanırken yapılır.
let updater = null
let state = { status: 'idle', version: '', percent: 0, error: '', feed: '', startup: false, text: '' }
let timer

// Açılışta hangi sunucuya bakılacağı: arayüz her bağlandığında kaydedilir,
// böylece uygulama daha pencere açılmadan güncelleme denetimi yapabilir.
function basePath() { return path.join(app.getPath('userData'), 'update-source.json') }
function saveBase(base) {
  try { fs.writeFileSync(basePath(), JSON.stringify({ base })) } catch {}
}
function savedBase() {
  try { return String(JSON.parse(fs.readFileSync(basePath(), 'utf8')).base || '') } catch { return '' }
}

function broadcast() {
  for (const window of BrowserWindow.getAllWindows()) {
    if (!window.isDestroyed()) window.webContents.send('updates:state', state)
  }
}
function setState(patch) { state = { ...state, ...patch }; broadcast() }

// Sunucu süreci aynı exe'yi çalıştırdığı için kurulum sırasında dosya kilitli
// kalır: önce zamanlanmış görev durdurulur, sonra kurulum başlatılır.
function stopServerTask() {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve()
    execFile('schtasks', ['/End', '/TN', 'P2 Server'], { windowsHide: true }, () => resolve())
  })
}

// Servis olarak kayıtlı olmayan (uygulamanın başlattığı) sunucu süreçleri.
// Kurulum aynı exe'yi değiştirdiği için bunlar da kapanmalı; güncelleme
// sonrası uygulama açılırken sunucu yeni sürümle yeniden başlatılır.
function stopServerProcesses() {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve()
    execFile('powershell', ['-NoProfile', '-Command',
      "Get-CimInstance Win32_Process -Filter \"Name='P2.exe'\" | Where-Object { $_.CommandLine -like '*--p2-server*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }",
    ], { windowsHide: true, timeout: 8000 }, () => resolve())
  })
}

function load(log) {
  if (updater) return updater
  try {
    updater = require('electron-updater').autoUpdater
  } catch (error) { log(`Güncelleyici yüklenemedi: ${error?.message || error}`); return null }
  updater.autoDownload = true
  updater.autoInstallOnAppQuit = true
  updater.logger = { info: log, warn: log, error: log, debug: () => {} }
  updater.on('checking-for-update', () => setState({ status: 'checking', error: '' }))
  updater.on('update-not-available', () => setState({ status: 'idle', percent: 0 }))
  updater.on('update-available', info => setState({ status: 'downloading', version: info?.version || '', percent: 0, error: '' }))
  updater.on('download-progress', progress => setState({ status: 'downloading', percent: Math.round(progress?.percent || 0) }))
  updater.on('update-downloaded', info => setState({ status: 'ready', version: info?.version || state.version, percent: 100 }))
  updater.on('error', error => {
    const message = String(error?.message || error)
    log(`Güncelleme hatası: ${message}`)
    // Sunucuda henüz sürüm yayınlanmamışsa (404) ya da ağ yoksa sessiz kalınır.
    const quiet = /404|ENOTFOUND|ECONNREFUSED|ETIMEDOUT|EAI_AGAIN|net::/i.test(message)
    setState({ status: 'idle', percent: 0, error: quiet ? '' : message })
  })
  return updater
}

// Arayüz hangi sunucuya bağlıysa güncelleme de oradan gelir; adres istemci
// modunda yalnızca renderer tarafında bilindiği için oradan bildirilir.
function configure(apiBase, log) {
  if (!app.isPackaged) return { ok: false, reason: 'dev' }
  const base = String(apiBase || '').trim().replace(/\/+$/, '')
  if (!/^https?:\/\//i.test(base)) return { ok: false, reason: 'adres yok' }
  const feed = `${base}/updates/`
  saveBase(base)
  if (state.feed === feed && timer) return { ok: true, feed }
  const instance = load(log)
  if (!instance) return { ok: false, reason: 'yüklenemedi' }
  try { instance.setFeedURL({ provider: 'generic', url: feed, channel: 'latest' }) }
  catch (error) { log(`Güncelleme adresi ayarlanamadı: ${error?.message || error}`); return { ok: false, reason: 'adres' } }
  setState({ feed })
  log(`Güncelleme kaynağı: ${feed}`)
  void check(log)
  if (timer) clearInterval(timer)
  // Asıl haber sunucunun "update:published" duyurusundan gelir; bu tur yalnızca
  // duyuruyu kaçıranlar (ağ kesintisi, uyku) için yedek.
  timer = setInterval(() => void check(log), 5 * 60 * 1000)
  return { ok: true, feed }
}

async function check(log) {
  if (!app.isPackaged || !state.feed) return { ok: false }
  // İndirme sürüyorsa ya da kurulum hazırsa yeni kontrol anlamsız.
  if (state.status === 'downloading' || state.status === 'ready') return { ok: true, status: state.status }
  const instance = load(log)
  if (!instance) return { ok: false }
  try { await instance.checkForUpdates() } catch (error) { log(`Kontrol başarısız: ${error?.message || error}`) }
  return { ok: true, status: state.status }
}

async function install(log) {
  if (state.status !== 'ready' || !updater) return { ok: false }
  log('Güncelleme kuruluyor: sunucu durduruluyor')
  await stopServerTask()
  await stopServerProcesses()
  await new Promise(resolve => setTimeout(resolve, 800))
  // Sessiz kurulum: kullanıcıya sihirbaz gösterilmez, kurulum bitince uygulama kendiliğinden açılır.
  try { updater.quitAndInstall(true, true) } catch (error) { log(`Kurulum başlatılamadı: ${error?.message || error}`); return { ok: false } }
  return { ok: true }
}

// Kurulumdan sonra uygulama yeniden açıldığında sunucu görevi durmuş olur.
function resumeServerTask() {
  if (process.platform !== 'win32') return
  execFile('schtasks', ['/Query', '/TN', 'P2 Server'], { windowsHide: true }, error => {
    if (error) return
    execFile('schtasks', ['/Run', '/TN', 'P2 Server'], { windowsHide: true }, () => {})
  })
}

// Açılış turu: pencere açılmadan önce denetler, güncelleme varsa indirir ve
// kurar. Uzun sürmemesi için her adımda süre sınırı var; takılırsa uygulama
// normal şekilde açılır ve güncelleme arka planda sürer.
async function startupCheck(base, log) {
  if (!app.isPackaged) return 'none'
  // Açılış turu boyunca arayüz kendi ekranında ilerlemeyi gösterir.
  const finish = (outcome) => { setState({ startup: false, text: '' }); return outcome }
  setState({ startup: true, text: 'Güncellemeler denetleniyor…' })
  const ready = configure(base, log)
  if (!ready.ok) return finish('none')
  const instance = load(log)
  if (!instance) return finish('none')

  const waitFor = (test, timeout) => new Promise(resolve => {
    const started = Date.now()
    const tick = setInterval(() => {
      if (test()) { clearInterval(tick); resolve(true); return }
      if (Date.now() - started > timeout) { clearInterval(tick); resolve(false) }
    }, 200)
  })

  try { await instance.checkForUpdates() } catch (error) { log(`Açılış denetimi başarısız: ${error?.message || error}`); return finish('none') }
  // Güncelleme yoksa ya da denetim 12 saniyede sonuçlanmazsa beklemeden devam.
  const found = await waitFor(() => state.status === 'downloading' || state.status === 'ready', 12000)
  if (!found) return finish('none')

  setState({ text: 'Güncelleme indiriliyor' })
  let lastPercent = -1, lastMove = Date.now()
  const progressing = await waitFor(() => {
    if (state.percent !== lastPercent) { lastPercent = state.percent; lastMove = Date.now() }
    // İndirme 90 saniye boyunca ilerlemediyse uygulamayı bekletmeye değmez.
    if (Date.now() - lastMove > 90000) return true
    return state.status === 'ready'
  }, 30 * 60 * 1000)
  if (!progressing || state.status !== 'ready') return finish('none')

  setState({ text: 'Güncelleme kuruluyor, uygulama yeniden başlayacak…' })
  const done = await install(log)
  return done.ok ? 'installing' : finish('none')
}

module.exports = { configure, check, install, resumeServerTask, startupCheck, savedBase, stopServerProcesses, getState: () => state }
