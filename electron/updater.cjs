const { app, BrowserWindow } = require('electron')
const { execFile } = require('node:child_process')
const crypto = require('node:crypto')
const fs = require('node:fs')
const http = require('node:http')
const path = require('node:path')

// Otomatik güncelleme. Güncelleme dosyaları, kullanıcının zaten bağlı olduğu
// P2 sunucusundan gelir (GET <api>/updates/client/latest.yml). Kök kanal
// yalnız eski birleşik istemciler için geriye dönük uyumluluk amacıyla kalır.
// servisi yok. Sessizce indirilir, hazır olunca arayüzde şerit gösterilir;
// kurulum kullanıcı "yeniden başlat" derse ya da uygulama kapanırken yapılır.
let updater = null
// current: o an çalışan sürüm; arayüz ayarlarda bunu gösterir.
let state = { status: 'idle', version: '', percent: 0, error: '', feed: '', startup: false, text: '', current: app.getVersion() }
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

// Eski birleşik kurulumlarla geriye uyumluluk: 0.7.10 ve öncesindeki aynı EXE
// sunucu görevini güncellemeden önce durdurur. Yeni P2 Server ayrı kurulur.
function legacyServerTask() {
  return new Promise(resolve => {
    if (process.platform !== 'win32') return resolve()
    execFile('schtasks', ['/Query', '/TN', 'P2 Server', '/XML'], { windowsHide: true }, (error, stdout) => {
      if (error) return resolve(false)
      // Yeni bağımsız P2 Server istemci kurulumu tarafından durdurulmaz.
      resolve(!/P2 Server\.exe/i.test(String(stdout || '')))
    })
  })
}
async function stopServerTask() {
  if (!(await legacyServerTask())) return false
  return new Promise(resolve => {
    execFile('schtasks', ['/End', '/TN', 'P2 Server'], { windowsHide: true }, () => resolve(true))
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

// Sunucu servisi SYSTEM hesabıyla koşuyorsa dışarıdan kapatılamaz (yönetici
// hakkı ister) ve çalışan P2.exe kurulumu engeller. Sunucu bu isteği alınca
// kendini kapatır; anahtar, yalnızca bu makinedeki server.json'da duran gizli
// anahtardan türetilir.
function serverSecret() {
  try {
    const dosya = path.join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'server.json')
    return String(JSON.parse(fs.readFileSync(dosya, 'utf8')).accessSecret || '')
  } catch { return '' }
}

function apiPort() {
  try {
    const dosya = path.join(process.env.ProgramData || 'C:\\ProgramData', 'P2', 'server.json')
    return Number(JSON.parse(fs.readFileSync(dosya, 'utf8')).port) || 4000
  } catch { return 4000 }
}

function askServerToStop(log) {
  return new Promise(resolve => {
    const secret = serverSecret()
    if (!secret) return resolve(false)
    const token = crypto.createHmac('sha256', secret).update('p2-shutdown').digest('hex')
    const govde = JSON.stringify({ token })
    const request = http.request(
      { host: '127.0.0.1', port: apiPort(), path: '/updates/stop', method: 'POST', timeout: 4000,
        headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(govde) } },
      response => { response.resume(); resolve(response.statusCode === 200 || response.statusCode === 201) },
    )
    request.on('error', () => resolve(false))
    request.on('timeout', () => { request.destroy(); resolve(false) })
    request.end(govde)
  })
}

// Sunucu gerçekten kapandı mı? (kurulum ancak o zaman dosyaları değiştirebilir)
function serverGone(timeout = 9000) {
  const started = Date.now()
  const bak = () => new Promise(resolve => {
    const request = http.get({ host: '127.0.0.1', port: apiPort(), path: '/health', timeout: 800 }, response => { response.resume(); resolve(true) })
    request.on('error', () => resolve(false))
    request.on('timeout', () => { request.destroy(); resolve(false) })
  })
  return (async () => {
    while (Date.now() - started < timeout) {
      if (!(await bak())) return true
      await new Promise(resolve => setTimeout(resolve, 400))
    }
    return false
  })()
}

// Otomatik kurulum: indirme bitince, oyun/arama yokken 20 sn içinde sessizce kurulur ve uygulama yeniden açılır.
// Meşgulken (overlay'li oyun açık, aramada) her dakika yeniden denenir; kullanıcı kapatırsa kapanışta kurulur.
let busy = false, idleCheck = () => true, autoTimer = null, installing = false
function setBusy(flag) { busy = Boolean(flag) }
function setIdleCheck(fn) { if (typeof fn === 'function') idleCheck = fn }
// Kurulum başlamadan beklenen kanca (main.cjs: migrate.json anlık görüntüsü). En fazla 3 sn.
let beforeInstall = null
function setBeforeInstall(fn) { beforeInstall = typeof fn === 'function' ? fn : null }
async function runBeforeInstall(log) {
  if (!beforeInstall) return
  try { await Promise.race([Promise.resolve(beforeInstall()), new Promise(resolve => setTimeout(resolve, 3000))]) } catch (error) { log(`Kurulum öncesi adım başarısız: ${error?.message || error}`) }
}
function scheduleAutoInstall(log) {
  if (autoTimer) clearTimeout(autoTimer)
  autoTimer = setTimeout(() => {
    autoTimer = null
    if (state.status !== 'ready') return
    let idle = false; try { idle = !busy && idleCheck() } catch { idle = false }
    if (!idle) { log('Güncelleme hazır; oyun/arama sürdüğü için kurulum erteleniyor'); scheduleAutoInstall(log); return }
    log('Güncelleme hazır; kendiliğinden kuruluyor')
    void install(log)
  }, state.status === 'ready' && autoTimer === null ? 20000 : 60000)
}
// Kapanış kancası: hazır güncelleme varsa sessizce kurulur ve uygulama yeniden açılır.
function installIfReady(log) {
  if (installing || state.status !== 'ready' || !updater) return false
  installing = true
  try { updater.quitAndInstall(true, true); return true }
  catch (error) { installing = false; log(`Kapanışta kurulum başlatılamadı: ${error?.message || error}`); return false }
}

function load(log) {
  if (updater) return updater
  try {
    updater = require('electron-updater').autoUpdater
  } catch (error) { log(`Güncelleyici yüklenemedi: ${error?.message || error}`); return null }
  updater.autoDownload = true
  // Kapanışta kurulum kendi kancamızdan yapılır (installIfReady): uygulama kurulumdan sonra yeniden açılsın.
  updater.autoInstallOnAppQuit = false
  updater.logger = { info: log, warn: log, error: log, debug: () => {} }
  updater.on('checking-for-update', () => setState({ status: 'checking', error: '' }))
  updater.on('update-not-available', () => setState({ status: 'idle', percent: 0 }))
  updater.on('update-available', info => setState({ status: 'downloading', version: info?.version || '', percent: 0, error: '' }))
  updater.on('download-progress', progress => setState({ status: 'downloading', percent: Math.round(progress?.percent || 0) }))
  updater.on('update-downloaded', info => { setState({ status: 'ready', version: info?.version || state.version, percent: 100 }); scheduleAutoInstall(log) })
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
  const feed = `${base}/updates/client/`
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
  if (state.status !== 'ready' || !updater || installing) return { ok: false }
  installing = true
  log('Güncelleme kuruluyor: sunucu durduruluyor')
  const legacyTask = await stopServerTask()
  await stopServerProcesses()
  // Yukarıdakiler yönetici hakkı ister; servis SYSTEM ise sunucudan kendini
  // kapatmasını isteriz, yoksa kurulum kilitli dosyalara takılıyor.
  const istendi = legacyTask ? await askServerToStop(log) : false
  const kapandi = legacyTask ? await serverGone() : true
  log(legacyTask ? `Eski birleşik sunucu kapanışı: istek ${istendi ? 'kabul edildi' : 'gitmedi'}, port ${kapandi ? 'boşaldı' : 'hâlâ dolu'}` : 'Bağımsız P2 Server çalışmaya devam ediyor')
  await runBeforeInstall(log)
  await new Promise(resolve => setTimeout(resolve, 800))
  // Sessiz kurulum: kullanıcıya sihirbaz gösterilmez, kurulum bitince uygulama kendiliğinden açılır.
  try { updater.quitAndInstall(true, true) } catch (error) { installing = false; log(`Kurulum başlatılamadı: ${error?.message || error}`); return { ok: false } }
  return { ok: true }
}

// Kurulumdan sonra uygulama yeniden açıldığında sunucu görevi durmuş olur.
async function resumeServerTask() {
  if (process.platform !== 'win32') return
  if (!(await legacyServerTask())) return
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

module.exports = { configure, check, install, installIfReady, setBusy, setIdleCheck, setBeforeInstall, resumeServerTask, startupCheck, savedBase, stopServerProcesses, getState: () => state }
