const { app, BrowserWindow } = require('electron')
const { execFile } = require('node:child_process')

// Otomatik güncelleme. Güncelleme dosyaları, kullanıcının zaten bağlı olduğu
// P2 sunucusundan gelir (GET <api>/updates/latest.yml) — ayrı bir dağıtım
// servisi yok. Sessizce indirilir, hazır olunca arayüzde şerit gösterilir;
// kurulum kullanıcı "yeniden başlat" derse ya da uygulama kapanırken yapılır.
let updater = null
let state = { status: 'idle', version: '', percent: 0, error: '', feed: '' }
let timer

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
  if (state.feed === feed && timer) return { ok: true, feed }
  const instance = load(log)
  if (!instance) return { ok: false, reason: 'yüklenemedi' }
  try { instance.setFeedURL({ provider: 'generic', url: feed, channel: 'latest' }) }
  catch (error) { log(`Güncelleme adresi ayarlanamadı: ${error?.message || error}`); return { ok: false, reason: 'adres' } }
  setState({ feed })
  log(`Güncelleme kaynağı: ${feed}`)
  void check(log)
  if (timer) clearInterval(timer)
  timer = setInterval(() => void check(log), 30 * 60 * 1000)
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
  log('Güncelleme kuruluyor: sunucu görevi durduruluyor')
  await stopServerTask()
  await new Promise(resolve => setTimeout(resolve, 800))
  try { updater.quitAndInstall(false, true) } catch (error) { log(`Kurulum başlatılamadı: ${error?.message || error}`); return { ok: false } }
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

module.exports = { configure, check, install, resumeServerTask, getState: () => state }
