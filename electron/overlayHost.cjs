// Oyun içi overlay'in Electron tarafı. Enjekte edilen DLL arayüzü ekran dışı
// (offscreen) bir BrowserWindow'da çizilir; bu modül kareleri paylaşılan belleğe
// pompalar ve DLL'den gelen girdiyi offscreen pencereye uygular.
//
// Süreç ağacı: Unreal gibi motorlarda kök exe yalnız bir başlatıcıdır (171 KB),
// asıl oyun Binaries/Win64 altındaki ALT SÜREÇTE açılır. Bu yüzden kök sürecin
// alt süreçleri her saniye taranır ve her yeni 64-bit alt sürece de DLL enjekte
// edilir (p2inject64 --attach). Hangisinin oyun olduğunu DLL'in Present sayacı
// (header.frames) söyler: ilk kareyi çizen süreç "hazır"dır; o anda başlatma
// ekranı kapanır ve oyun içi "başlatıldı" bildirimi gösterilir.
//
// GÜVENLİ DEVRE DIŞI: yerel ikililer (p2overlayhost.node, p2inject64.exe,
// p2overlay64.dll) yoksa bu modül hiçbir şey yapmaz ve oyun normal başlatılır.
const { app, BrowserWindow, desktopCapturer, nativeImage, session: electronSession } = require('electron')
const { spawn } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')

let host = null            // p2overlayhost.node
let loadRenderer = () => {} // main.cjs tarafından set edilir
let suggestedServer = () => ''
const sessions = new Map() // pid -> oturum (bkz. setupSession)
const roots = new Map()    // rootPid -> { projectId, projectName, timer, injected:Set, startedAt, readyFired, onReady, onNoOverlay }

// Alt süreç taramasında atlanacak yardımcı süreçler (çökme bildirici, CEF, kurulumlar, konsol...).
const SKIP_CHILD = /^(crashreportclient|crashreportclienteditor|unrealcefsubprocess|epicwebhelper|unitycrashhandler32|unitycrashhandler64|conhost|cmd|powershell|pwsh|werfault|dxsetup|msiexec|rundll32|dllhost|explorer|gameoverlayui|steamwebhelper)\.exe$|^(vc_?redist|ue4prereqsetup|ueprereqsetup|easyanticheat|eac_|beservice)/i
const NO_OVERLAY_AFTER_MS = 90000

function log(message) {
  try { fs.appendFileSync(path.join(app.getPath('appData'), 'P2', 'overlay.log'), `[${new Date().toTimeString().slice(0, 8)}] [host] ${message}\n`) } catch {}
}

function resourceDir() {
  // Paketli: resources/overlay; geliştirme: native/overlay/build/Release
  const packaged = path.join(process.resourcesPath || '', 'overlay')
  if (fs.existsSync(packaged)) return packaged
  return path.join(app.getAppPath(), 'native', 'overlay', 'build', 'Release')
}
function hostAddonPath() {
  for (const p of [path.join(resourceDir(), 'p2overlayhost.node'), path.join(app.getAppPath(), 'native', 'overlay', 'host', 'build', 'Release', 'p2overlayhost.node')]) if (fs.existsSync(p)) return p
  return ''
}
function injectorPath() { const p = path.join(resourceDir(), 'p2inject64.exe'); return fs.existsSync(p) ? p : '' }
function dllPath() { const p = path.join(resourceDir(), 'p2overlay64.dll'); return fs.existsSync(p) ? p : '' }

// Overlay bu makinede mümkün mü? (ikililer var mı)
function available() { return Boolean(hostAddonPath() && injectorPath() && dllPath()) }

function ensureHost() {
  if (host) return host
  const addon = hostAddonPath()
  if (!addon) return null
  try { host = require(addon); log(`host eklentisi yüklendi (şema ${host.schema || '3?'}): ${addon}`) } catch (error) { host = null; log(`host eklentisi yüklenemedi: ${error.message}`) }
  return host
}

// Anti-cheat tespiti: bu build'leri enjekte etmeyiz.
function hasAntiCheat(gameDir) {
  const markers = ['EasyAntiCheat', 'EasyAntiCheat_x64', 'BEService', 'BattlEye', 'GameGuard', 'vgc', 'vanguard']
  try {
    const walk = (dir, depth) => {
      if (depth > 3) return false
      for (const name of fs.readdirSync(dir)) {
        if (markers.some(m => name.toLowerCase().includes(m.toLowerCase()))) return true
        const full = path.join(dir, name)
        try { if (fs.statSync(full).isDirectory() && walk(full, depth + 1)) return true } catch {}
      }
      return false
    }
    return walk(gameDir, 0)
  } catch { return false }
}

// Enjektörü çalıştırır; stdout'taki pid'i ve çıkış kodunu döndürür.
function runInjector(args) {
  return new Promise(resolve => {
    try {
      const child = spawn(injectorPath(), args, { windowsHide: true })
      let out = '', err = ''
      child.stdout?.on('data', d => { out += String(d) })
      child.stderr?.on('data', d => { err += String(d) })
      child.on('exit', code => resolve({ code, pid: parseInt(out.trim(), 10), err: err.trim() }))
      child.on('error', error => resolve({ code: -1, pid: NaN, err: error.message }))
    } catch (error) { resolve({ code: -1, pid: NaN, err: error.message }) }
  })
}

// Oyun başlat (overlay enjekte ederek). Dönüş: { pid, overlay } ya da null (geri düşüş: düz başlat).
// pid döndüyse süreç BAŞLAMIŞTIR; overlay=false yalnız enjeksiyonun tutmadığını söyler.
function startForGame({ exePath, args = [], cwd, projectId, projectName = '', onReady = null, onNoOverlay = null }) {
  if (!available()) return null
  if (hasAntiCheat(cwd)) { log(`${projectName}: anti-cheat bulundu, enjeksiyon yok`); return null }
  const addon = ensureHost()
  if (!addon) return null
  return runInjector([dllPath(), exePath, cwd, ...args]).then(result => {
    if (!Number.isInteger(result.pid)) { log(`${projectName}: başlatılamadı (${result.err || 'pid yok'})`); return null }
    if (result.code !== 0) { log(`${projectName}: pid ${result.pid} başladı ama enjeksiyon tutmadı (kod ${result.code}: ${result.err})`); return { pid: result.pid, overlay: false } }
    const root = { projectId, projectName, timer: null, injected: new Set([result.pid]), tree: new Set([result.pid]), startedAt: Date.now(), readyFired: false, noOverlayFired: false, onReady, onNoOverlay }
    roots.set(result.pid, root)
    setupSession(result.pid, result.pid, projectId, projectName)
    log(`${projectName}: kök pid ${result.pid} (${path.basename(exePath)}) enjekte edildi; alt süreçler izleniyor`)
    root.timer = setInterval(() => trackRoot(result.pid), 1000)
    return { pid: result.pid, overlay: true }
  })
}

// Kök sürecin alt ağacını tarar: yeni 64-bit alt süreçlere enjekte eder, ilk kareyi bekler.
function trackRoot(rootPid) {
  const root = roots.get(rootPid); if (!root) return
  const addon = ensureHost(); if (!addon) return
  let list = []
  try { list = addon.processes() } catch { list = [] }
  const alive = new Set(list.map(p => p.pid))
  // Ağaçtaki bilinen süreçlerden yaşayanlar tohum olur: kök başlatıcı erken çıksa bile
  // (bazı launcher'lar oyunu açıp kapanır) asıl oyun izlenmeye devam eder.
  const descendants = new Set([...root.tree].filter(pid => alive.has(pid)))
  if (!descendants.size) { clearInterval(root.timer); root.timer = null; return }
  let grew = true
  while (grew) { grew = false; for (const p of list) if (descendants.has(p.ppid) && !descendants.has(p.pid)) { descendants.add(p.pid); grew = true } }
  for (const pid of descendants) root.tree.add(pid)
  for (const p of list) {
    if (!descendants.has(p.pid) || root.injected.has(p.pid)) continue
    root.injected.add(p.pid)
    if (p.wow64) { log(`${root.projectName}: alt süreç ${p.pid} (${p.name}) 32-bit, atlandı`); continue }
    if (SKIP_CHILD.test(p.name)) { log(`${root.projectName}: alt süreç ${p.pid} (${p.name}) yardımcı, atlandı`); continue }
    attachChild(rootPid, p.pid, p.name)
  }
  // Hazırlık: herhangi bir oturum kare çizmeye başladı mı?
  for (const [pid, session] of sessions) {
    if (session.rootPid !== rootPid || session.ready) continue
    let frames = 0
    try { frames = addon.frames(pid) } catch { frames = 0 }
    if (frames > 0) {
      session.ready = true
      log(`${root.projectName}: pid ${pid} ilk kareyi çizdi, overlay hazır`)
      notify(pid, { kind: 'info', title: `${root.projectName || 'Oyun'} başlatıldı`, text: 'P2 overlay hazır', keys: panelKeys(), duration: 9000 })
      try { onReadyHook?.({ projectId: session.projectId, pid, ready: true }) } catch {}
      if (!root.readyFired) { root.readyFired = true; try { root.onReady?.(pid) } catch {} }
    }
  }
  if (!root.readyFired && !root.noOverlayFired && Date.now() - root.startedAt > NO_OVERLAY_AFTER_MS) {
    root.noOverlayFired = true
    log(`${root.projectName}: ${NO_OVERLAY_AFTER_MS / 1000} sn içinde hiçbir süreç kare çizmedi (D3D12/Vulkan/OpenGL olabilir)`)
    try { root.onNoOverlay?.() } catch {}
  }
}

function attachChild(rootPid, pid, name) {
  const root = roots.get(rootPid); if (!root) return
  // Paylaşılan bellek DLL'den ÖNCE hazır olmalı (DLL 5 sn boyunca açmayı dener).
  setupSession(pid, rootPid, root.projectId, root.projectName)
  void runInjector(['--attach', dllPath(), String(pid)]).then(result => {
    if (result.code === 0) { log(`${root.projectName}: alt süreç ${pid} (${name}) enjekte edildi`); return }
    log(`${root.projectName}: alt süreç ${pid} (${name}) enjekte edilemedi (kod ${result.code}: ${result.err})`)
    destroySession(pid)
  })
}

// ---------------------------------------------------------------------------
// Oturumlar, girdi, çizim maskesi, kısayollar, bildirimler (şema 4)
// ---------------------------------------------------------------------------
const byWebContents = new Map() // webContents.id -> pid
let hotkeys = [{ id: 1, vk: 0x32, mods: 0x1 }] // varsayılan: Shift+2 panel; ana pencere ayarlardan günceller
let callStats = { rttMs: null, lossPct: null }
// Chromium imleç adı -> Win32 IDC_* kaynak kimliği (user32 LoadCursor). Listede olmayanlar (custom, cell…) ok olur.
const CURSOR_IDC = {
  default: 32512, pointer: 32649, hand: 32649, text: 32513, "vertical-text": 32513, wait: 32514, progress: 32650, help: 32651, crosshair: 32515,
  move: 32646, "all-scroll": 32646, "not-allowed": 32648, "no-drop": 32648,
  "n-resize": 32645, "s-resize": 32645, "ns-resize": 32645, "row-resize": 32645,
  "e-resize": 32644, "w-resize": 32644, "ew-resize": 32644, "col-resize": 32644,
  "ne-resize": 32643, "sw-resize": 32643, "nesw-resize": 32643,
  "nw-resize": 32642, "se-resize": 32642, "nwse-resize": 32642,
}
// Sistem ölçümleri: CPU (os.cpus deltası), RAM, GPU (nvidia-smi varsa). 1 Hz; yalnız HUD/panel açıkken örneklenir.
const os = require('node:os')
const { execFile } = require('node:child_process')
let cpuPrev = null, gpuInfo = null, gpuBusy = false, gpuMissing = false, gpuAt = 0
function cpuPercent() {
  const cpus = os.cpus(); let idle = 0, total = 0
  for (const cpu of cpus) { for (const key of Object.keys(cpu.times)) total += cpu.times[key]; idle += cpu.times.idle }
  const prev = cpuPrev; cpuPrev = { idle, total }
  if (!prev) return null
  const dt = total - prev.total, di = idle - prev.idle
  return dt > 0 ? Math.max(0, Math.min(100, Math.round(100 * (1 - di / dt)))) : null
}
function sampleGpu() {
  if (gpuMissing || gpuBusy || Date.now() - gpuAt < 2000) return
  gpuBusy = true; gpuAt = Date.now()
  execFile('nvidia-smi', ['--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total', '--format=csv,noheader,nounits'], { windowsHide: true, timeout: 1500 }, (error, stdout) => {
    gpuBusy = false
    if (error) { gpuMissing = true; gpuInfo = null; log(`GPU ölçümü yok (nvidia-smi: ${error.code || error.message})`); return }
    const parts = String(stdout).trim().split('\n')[0].split(',').map(part => Number(part.trim()))
    if (parts.length >= 4 && parts.every(Number.isFinite)) gpuInfo = { util: parts[0], temp: parts[1], memUsed: parts[2], memTotal: parts[3] }
  })
}
function systemStats() {
  sampleGpu()
  const total = os.totalmem(), free = os.freemem()
  return { cpu: cpuPercent(), ram: { used: Math.round((total - free) / 1048576), total: Math.round(total / 1048576) }, gpu: gpuInfo, loss: callStats.lossPct }
}
// Oyun sürecinin CPU'su ve belleği (eklenti GetProcessTimes/WorkingSet): iki örnek farkından yüzde. Enjekte edilen
// pid asıl oyun süreci olmayabilir (Unreal: başlatıcı → alt süreç); en çok CPU harcayan alt süreç dahil ölçülür.
function gameStats(session, pid) {
  const addon = ensureHost()
  if (!addon || typeof addon.processStats !== 'function') return null
  let best = null
  const pids = [pid, ...(session.childPids || [])]
  for (const target of pids) {
    let now = null; try { now = addon.processStats(target) } catch { now = null }
    if (!now) continue
    const prev = session.gamePrev?.[target]
    session.gamePrev = session.gamePrev || {}
    session.gamePrev[target] = now
    if (!prev) continue
    const dCpu = now.cpuMs - prev.cpuMs, dWall = now.wallMs - prev.wallMs
    const cpu = dWall > 0 ? Math.max(0, Math.min(100, Math.round((dCpu / dWall) / (now.cores || 1) * 100))) : null
    const ramMb = now.ramMb >= 0 ? Math.round(now.ramMb) : null
    if (!best || (cpu ?? -1) > (best.cpu ?? -1) || ((cpu ?? -1) === (best.cpu ?? -1) && (ramMb ?? 0) > (best.ramMb ?? 0))) best = { cpu, ramMb }
  }
  return best
}
let onReadyHook = null  // ({projectId,pid,ready}) => void  (main.cjs ana pencereye iletir)
let onHotkeyHook = null // ({id,down,pid,projectId}) => void

const VK_NAMES = { 0x08: 'Backspace', 0x09: 'Tab', 0x0d: 'Return', 0x13: 'Pause', 0x14: 'CapsLock', 0x1b: 'Escape', 0x20: 'Space', 0x21: 'PageUp', 0x22: 'PageDown', 0x23: 'End', 0x24: 'Home', 0x25: 'Left', 0x26: 'Up', 0x27: 'Right', 0x28: 'Down', 0x2d: 'Insert', 0x2e: 'Delete', 0x6a: 'NumpadMultiply', 0x6b: 'NumpadAdd', 0x6d: 'NumpadSubtract', 0x6e: 'NumpadDecimal', 0x6f: 'NumpadDivide', 0x91: 'ScrollLock', 0xba: ';', 0xbb: '=', 0xbc: ',', 0xbd: '-', 0xbe: '.', 0xbf: '/', 0xc0: '`', 0xdb: '[', 0xdc: '\\', 0xdd: ']', 0xde: "'" }
function vkToKey(vk) {
  if (vk >= 0x30 && vk <= 0x5a) return String.fromCharCode(vk)
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`
  if (vk >= 0x60 && vk <= 0x69) return `Numpad${vk - 0x60}`
  return VK_NAMES[vk] || ''
}
// Kısayolu bildirim kartındaki tuş çiplerine çevirir: {vk:0x32,mods:1} -> ['Shift','2']
function hotkeyKeys(hotkey) {
  if (!hotkey) return []
  const keys = []
  if (hotkey.mods & 0x2) keys.push('Ctrl'); if (hotkey.mods & 0x4) keys.push('Alt'); if (hotkey.mods & 0x1) keys.push('Shift')
  const name = vkToKey(hotkey.vk)
  keys.push(name === 'Space' ? 'Boşluk' : name === 'Return' ? 'Enter' : name || '?')
  return keys
}
function panelKeys() { return hotkeyKeys(hotkeys.find(h => h.id === 1)) }

// Tarayıcı aracı: <webview> ekran dışı (OSR) pencerede çizilmediği için sayfa bir iframe'de açılır. Overlay
// pencerelerinin ALT ÇERÇEVE yanıtlarından X-Frame-Options ve CSP frame-ancestors kaldırılır; ana pencere etkilenmez.
let frameRelaxerInstalled = false
function installFrameRelaxer() {
  if (frameRelaxerInstalled) return
  frameRelaxerInstalled = true
  try {
    electronSession.defaultSession.webRequest.onHeadersReceived((details, callback) => {
      if (details.resourceType !== 'subFrame' || !byWebContents.has(details.webContentsId)) { callback({}); return }
      const headers = {}
      for (const [name, value] of Object.entries(details.responseHeaders || {})) {
        const key = name.toLowerCase()
        if (key === 'x-frame-options') continue
        if (key === 'content-security-policy' || key === 'content-security-policy-report-only') {
          headers[name] = (Array.isArray(value) ? value : [value]).map(item => String(item).split(';').filter(part => !/^\s*frame-ancestors/i.test(part)).join(';'))
          continue
        }
        headers[name] = value
      }
      callback({ responseHeaders: headers })
    })
  } catch (error) { log(`çerçeve başlık gevşetici kurulamadı: ${error.message}`) }
}

function setupSession(pid, rootPid, projectId, projectName) {
  const addon = ensureHost(); if (!addon) return
  if (sessions.has(pid)) return
  installFrameRelaxer()
  // Şeffaf: panel kapalıyken yalnız köşe bildirimleri / sabitlenmiş pencereler çizilir, gerisi alfa=0.
  const win = new BrowserWindow({
    show: false, width: 1280, height: 720, transparent: true, backgroundColor: '#00000000',
    webPreferences: {
      offscreen: true,
      preload: path.join(__dirname, 'overlay-ui-preload.cjs'),
      // sandbox: false — overlay-ui-preload.cjs ana preload'u require eder; sandbox'lı preload
      // dosya require edemez ("module not found: ./preload.cjs") ve p2overlay köprüsü hiç kurulmaz.
      contextIsolation: true, nodeIntegration: false, sandbox: false, backgroundThrottling: false,
      // WebGPU ekran dışı (OSR) yüzeyde renderer'ı çökertiyor (YouTube requestAdapter → 4 sn sonra "crashed").
      // Overlay'de WebGPU'ya gerek yok; sayfalar WebGL/Canvas yoluna düşer. Resim-içinde-resim de ayrı pencere ister, kapalı.
      disableBlinkFeatures: 'WebGPU,PictureInPicture',
      additionalArguments: [
        `--p2-suggested-server=${suggestedServer()}`, `--p2-version=${app.getVersion()}`,
        `--p2-overlay-pid=${pid}`, `--p2-overlay-project=${projectId || ''}`, `--p2-overlay-name=${encodeURIComponent(projectName || '')}`,
      ],
    },
  })
  win.webContents.setFrameRate(30)
  // Tarayıcı aracındaki target=_blank / window.open: ayrı pencere yerine overlay tarayıcısında açılır
  win.webContents.setWindowOpenHandler(({ url }) => { try { if (/^https?:/i.test(url)) win.webContents.send('overlay:event', { type: 'open-url', url }) } catch {} return { action: 'deny' } })
  let created = false
  try { created = addon.create(pid, { hotkeys }, msg => onInput(pid, msg)) } catch { created = false }
  if (!created) {
    // Eski (şema 3) eklenti: konumsal imza
    const panel = hotkeys.find(h => h.id === 1) || { vk: 0x32, mods: 1 }
    try { created = addon.create(pid, panel.vk, panel.mods, msg => onInput(pid, msg)) } catch { created = false }
  }
  if (!created) { win.destroy(); log(`pid ${pid}: paylaşılan bellek kurulamadı`); return }
  const session = {
    window: win, projectId, projectName, rootPid, ready: false, loaded: false, pendingToasts: [],
    draw: { toast: false, pinned: false, hud: false }, rendererDraw: false, toastSafety: null,
    scale: 1, bb: null, mods: { shift: false, control: false, alt: false }, buttons: { left: false, right: false, middle: false }, surrogate: 0, lastClick: { t: 0, x: 0, y: 0, n: 0 },
    visible: false, focus: true, perfTimer: null, lastFrames: 0, paints: 0, paintWindow: { t: Date.now(), n: 0 }, paintWarnedAt: 0,
    cursor: 0,
  }
  sessions.set(pid, session)
  byWebContents.set(win.webContents.id, pid)
  // Arayüz yüklenmeden gönderilen bildirimler kaybolmasın (ilk kare çok erken gelebilir).
  win.webContents.once('did-finish-load', () => {
    session.loaded = true; log(`pid ${pid}: overlay arayüzü yüklendi`)
    // Arayüz yüklenmeden kısayola basıldıysa görünürlük mesajı kaçmıştır; güncel durumu ilet.
    try { if (addon.visible(pid)) { session.visible = true; win.webContents.send('overlay:visible', true) } } catch {}
    for (const toast of session.pendingToasts.splice(0)) notify(pid, toast)
  })
  // İmleç şekli: Chromium'un seçtiği imleç (kenar tutamacı çift ok, metin I-beam, bağlantı el) Win32 IDC_* kimliğine
  // çevrilip paylaşılan başlığa yazılır; DLL overlay açıkken SetCursor'da onu kullanır. Özel (resim) imleçler ok olur.
  win.webContents.on('cursor-changed', (_event, type) => {
    const session = sessions.get(pid); if (!session) return
    const id = CURSOR_IDC[type] || CURSOR_IDC.default
    if (id === session.cursor) return
    session.cursor = id
    try { if (typeof addon.setCursor === 'function') addon.setCursor(pid, id) } catch {}
  })
  win.webContents.on('did-fail-load', (_event, code, desc, url) => log(`pid ${pid}: overlay arayüzü yüklenemedi (${code} ${desc}) ${url}`))
  // Arayüz çökerse (ör. iframe'de ağır video) oyun kilitli kalmasın: panel kapatılır (imleç/girdi oyuna döner),
  // arayüz 800 ms sonra yeniden yüklenir; sabitli pencereler ve bildirimler yeni yüklemeyle geri gelir.
  win.webContents.on('render-process-gone', (_event, details) => {
    log(`pid ${pid}: overlay arayüzü çöktü (${details.reason}, kod ${details.exitCode}); panel kapatılıp arayüz yeniden yükleniyor`)
    const s = sessions.get(pid); if (!s) return
    s.loaded = false; s.visible = false; s.lastFrameSize = 0; s.draw = { toast: false, pinned: false, hud: false }
    try { addon.setVisible(pid, false) } catch {}
    try { applyDraw(pid) } catch {}
    setTimeout(() => { try { if (!win.isDestroyed()) { win.webContents.once('did-finish-load', () => { s.loaded = true; log(`pid ${pid}: overlay arayüzü yeniden yüklendi`); notify(pid, { kind: 'error', title: 'Overlay yeniden başlatıldı', text: 'Açık bir sayfa arayüzü çökertti; tarayıcı kapatıldı.', duration: 7000 }) }); loadRenderer(win, 'overlay=1') } } catch (error) { log(`pid ${pid}: arayüz yeniden yüklenemedi: ${error.message}`) } }, 800)
  })
  win.webContents.on('console-message', (_event, level, message) => { if (level >= 2 && !/Electron Security Warning/.test(String(message))) log(`pid ${pid}: arayüz konsolu: ${String(message).slice(0, 300)}`) })
  win.webContents.on('paint', (_event, dirty, image) => {
    const session = sessions.get(pid); if (!session) return
    const size = image.getSize()
    // DPI: offscreen bitmap DIP×ölçek gelir; DLL backbuffer pikseli bekler. Ölçeği ilk kareden öğren.
    try { const [cw] = win.getSize(); const scale = cw > 0 ? size.width / cw : 1; if (scale > 0 && Math.abs(scale - session.scale) > 0.01) { session.scale = scale; log(`pid ${pid}: DPI ölçeği ${scale.toFixed(2)}`); if (session.bb) applyBackbufferSize(session, session.bb.w, session.bb.h) } } catch {}
    let ok = false, err = ''
    try {
      const dx = dirty.x || 0, dy = dirty.y || 0, dw = dirty.width || 0, dh = dirty.height || 0
      const partial = addon.partialFrames === true && dw > 0 && dh > 0 && (dw < size.width || dh < size.height) && session.lastFrameSize === size.width * 65536 + size.height
      if (partial) {
        // Yalnız değişen bölge kopyalanır: sürüklerken tüm yüzey yerine pencere kadar veri taşınır.
        ok = addon.writeFrame(pid, image.crop({ x: dx, y: dy, width: dw, height: dh }).toBitmap(), size.width, size.height, dx, dy, dw, dh)
        // Kısmi kare reddedildiyse (ör. boyut değişti) sonraki boyama tam kare olsun
        if (!ok) { session.lastFrameSize = 0; try { win.webContents.invalidate() } catch {} }
      } else {
        ok = addon.writeFrame(pid, image.toBitmap(), size.width, size.height, 0, 0, size.width, size.height)
        if (ok) session.lastFrameSize = size.width * 65536 + size.height
      }
      if (!session.partialSaid && addon.partialFrames === true) { session.partialSaid = true; log(`pid ${pid}: kısmi kare yolu etkin`) }
    } catch (error) { err = error.message }
    if (session.paints++ === 0 || (!ok && session.paints < 4)) log(`pid ${pid}: arayüz karesi ${size.width}x${size.height} -> paylaşılan belleğe ${ok ? 'yazıldı' : 'YAZILAMADI ' + err}`)
    // Panel kapalıyken sürekli kare üretimi = sabitlenmiş bir pencerede animasyon dönüyor demektir; uyar.
    const now = Date.now(); session.paintWindow.n++
    if (now - session.paintWindow.t >= 5000) {
      if (!session.visible && session.paintWindow.n > 125 && now - session.paintWarnedAt > 60000) { session.paintWarnedAt = now; log(`pid ${pid}: panel kapalıyken 5 sn'de ${session.paintWindow.n} kare çizildi (sabitli pencerede animasyon?)`) }
      session.paintWindow = { t: now, n: 0 }
    }
    // Tanı: P2_OVERLAY_DUMP=<klasör> ile her 30. kare PNG olarak yazılır (arayüzün gerçekten ne çizdiğini görmek için).
    if (process.env.P2_OVERLAY_DUMP && session.paints % 30 === 1) { try { fs.writeFileSync(path.join(process.env.P2_OVERLAY_DUMP, `overlay-${pid}-${session.paints}.png`), image.toPNG()) } catch {} }
  })
  // Performans göstergesi: saniyede bir kare sayısı (yalnız panel açıkken ya da HUD sabitliyken gönderilir).
  session.perfTimer = setInterval(() => {
    const s = sessions.get(pid); if (!s || s.window.isDestroyed()) return
    let frames = 0; try { frames = addon.frames(pid) } catch { frames = 0 }
    const fps = Math.max(0, frames - s.lastFrames); s.lastFrames = frames
    if (s.loaded && (s.visible || s.draw.hud)) {
      const sample = { fps, ping: callStats.rttMs, ...systemStats(), game: gameStats(s, pid) }
      if (!s.perfSaid && fps > 0) { s.perfSaid = true; log(`pid ${pid}: ilk performans örneği fps=${fps} cpu=${sample.cpu ?? '-'} oyun=${sample.game ? `${sample.game.cpu ?? '-'}% ${sample.game.ramMb ?? '-'}MB` : '-'} gpu=${sample.gpu ? sample.gpu.util + '%' : '-'}`) }
      try { s.window.webContents.send('overlay:perf', sample) } catch (error) { log(`pid ${pid}: performans örneği gönderilemedi: ${error.message}`) }
    }
  }, 1000)
  // İlk CPU örnekleri (sistem ve oyun) baştan alınır; böylece panel açılınca ilk tikte "—" yerine değer gelir.
  try { cpuPercent(); gameStats(session, pid) } catch {}
  loadRenderer(win, 'overlay=1')
}

function applyDraw(pid) {
  const session = sessions.get(pid); if (!session) return
  const addon = ensureHost(); if (!addon) return
  const mask = (session.draw.toast ? 1 : 0) | (session.draw.pinned ? 2 : 0) | (session.draw.hud ? 4 : 0)
  if (session.lastMask === mask) return
  session.lastMask = mask
  try { if (addon.setDrawMask) addon.setDrawMask(pid, mask); else addon.setNotify(pid, mask !== 0) } catch {}
  log(`pid ${pid}: çizim maskesi ${mask} (toast=${session.draw.toast ? 1 : 0} sabitli=${session.draw.pinned ? 1 : 0} hud=${session.draw.hud ? 1 : 0})`)
}

// Renderer çizim durumunun sahibidir: açık bildirim sayısı, sabitli pencere var mı, HUD açık mı.
function setDrawState(webContents, state) {
  const pid = byWebContents.get(webContents.id); const session = sessions.get(pid); if (!session) return
  session.rendererDraw = true
  session.draw = { toast: (Number(state?.toasts) || 0) > 0, pinned: Boolean(state?.pinned), hud: Boolean(state?.hud) }
  applyDraw(pid)
}

function applyBackbufferSize(session, w, h) {
  const width = Math.max(320, Math.min(3840, w | 0)), height = Math.max(240, Math.min(2160, h | 0))
  const dipW = Math.max(1, Math.round(width / session.scale)), dipH = Math.max(1, Math.round(height / session.scale))
  try { const [cw, ch] = session.window.getSize(); if (cw !== dipW || ch !== dipH) { session.window.setSize(dipW, dipH); log(`pid ${session.rootPid === undefined ? '?' : byPidLabel(session)}: overlay yüzeyi ${width}x${height} (pencere ${dipW}x${dipH})`) } } catch {}
}
function byPidLabel(session) { for (const [pid, s] of sessions) if (s === session) return pid; return '?' }

function setVisibleFor(pid, visible) {
  const session = sessions.get(pid); if (!session || session.window.isDestroyed()) return
  const addon = ensureHost(); try { if (addon) addon.setVisible(pid, visible) } catch {}
  session.visible = visible
  try { session.window.webContents.send('overlay:visible', visible) } catch {}
  if (visible) { try { session.window.focus(); session.window.webContents.focus() } catch {} }
  log(`pid ${pid}: overlay ${visible ? 'açıldı' : 'kapandı'} (host)`)
}

const CLIPBOARD_KEYS = { V: 'paste', C: 'copy', X: 'cut', A: 'selectAll', Z: 'undo', Y: 'redo' }

function onInput(pid, msg) {
  const session = sessions.get(pid); if (!session || session.window.isDestroyed()) return
  const wc = session.window.webContents
  if (msg.type === 6) { // TOGGLE (DLL kısayolu): durum mesajdan alınır, başlık DLL tarafından zaten yazıldı
    const visible = (msg.flags & 1) === 1
    const addon = ensureHost(); try { if (addon) addon.setVisible(pid, visible) } catch {}
    session.visible = visible
    log(`pid ${pid}: overlay ${visible ? 'açıldı' : 'kapandı'} (kısayol)`)
    wc.send('overlay:visible', visible)
    if (visible) { try { session.window.focus(); wc.focus() } catch {} }
    return
  }
  if (msg.type === 8) { // RESIZE: overlay yüzeyi oyunun backbuffer'ıyla aynı boyuta (çizim ve fare 1:1)
    session.bb = { w: msg.x | 0, h: msg.y | 0 }
    applyBackbufferSize(session, msg.x, msg.y)
    return
  }
  if (msg.type === 7) { // FOCUS: odak kaybında basılı modifier'lar bırakılır
    const focused = (msg.flags & 1) === 1
    session.focus = focused
    if (!focused) {
      session.buttons = { left: false, right: false, middle: false }
      for (const [name, code] of [['shift', 'Shift'], ['control', 'Control'], ['alt', 'Alt']]) if (session.mods[name]) { session.mods[name] = false; try { wc.sendInputEvent({ type: 'keyUp', keyCode: code }) } catch {} }
      try { wc.sendInputEvent({ type: 'mouseMove', x: -1, y: -1 }) } catch {}
    }
    try { wc.send('overlay:focus', focused) } catch {}
    return
  }
  if (msg.type === 9) { // HOTKEY: id = msg.key
    const id = msg.key | 0, down = (msg.flags & 1) === 1
    log(`pid ${pid}: kısayol id=${id} ${down ? 'basıldı' : 'bırakıldı'}`)
    try { wc.send('overlay:hotkey', { id, down }) } catch {}
    try { onHotkeyHook?.({ id, down, pid, projectId: session.projectId }) } catch {}
    return
  }
  if (msg.type === 10) { // SHOT_READY: DLL arka tamponu yuvaya kopyaladı (ya da başaramadı)
    const failed = (msg.flags & 1) === 1
    log(`pid ${pid}: ekran görüntüsü ${failed ? 'DLL tarafında başarısız' : 'hazır'} (id ${msg.key}, ${msg.x}x${msg.y}, format ${msg.wheelDelta})`)
    if (session.shot && session.shot.id === (msg.key | 0) && session.shot.resolve) { clearTimeout(session.shot.timer); session.shot.resolve({ ready: !failed }) }
    return
  }
  const scale = session.scale || 1
  const x = Math.round(msg.x / scale), y = Math.round(msg.y / scale)
  session.mods.shift = Boolean(msg.flags & 0x2); session.mods.control = Boolean(msg.flags & 0x4); session.mods.alt = Boolean(msg.flags & 0x8)
  const mods = []
  if (session.mods.shift) mods.push('shift'); if (session.mods.control) mods.push('control'); if (session.mods.alt) mods.push('alt')
  // DLL'in bayrakları (bit5-7) ya da bizim takip ettiğimiz düğme durumu: hareket olayında "tuş basılı" bilgisi şart
  if ((msg.flags & 0x20) || session.buttons.left) mods.push('leftButtonDown')
  if ((msg.flags & 0x40) || session.buttons.right) mods.push('rightButtonDown')
  if ((msg.flags & 0x80) || session.buttons.middle) mods.push('middleButtonDown')
  if (msg.type === 1) wc.sendInputEvent({ type: 'mouseMove', x, y, modifiers: mods })
  else if (msg.type === 2) {
    const down = (msg.flags & 0x1) === 1
    if (down) { try { session.window.focus(); wc.focus() } catch {} }
    const button = msg.key === 2 ? 'right' : msg.key === 3 ? 'middle' : 'left'
    session.buttons[button] = down
    if (down) {
      const now = Date.now(), last = session.lastClick
      const n = (now - last.t <= 500 && Math.abs(x - last.x) <= 4 && Math.abs(y - last.y) <= 4) ? Math.min(3, last.n + 1) : 1
      session.lastClick = { t: now, x, y, n }
    }
    wc.sendInputEvent({ type: down ? 'mouseDown' : 'mouseUp', x, y, button, clickCount: session.lastClick.n || 1, modifiers: mods })
  }
  else if (msg.type === 3) {
    const horizontal = Boolean(msg.flags & 0x100)
    wc.sendInputEvent({ type: 'mouseWheel', x, y, deltaX: horizontal ? msg.wheelDelta : 0, deltaY: horizontal ? 0 : msg.wheelDelta, canScroll: true, modifiers: mods })
  }
  else if (msg.type === 4) {
    const down = (msg.flags & 0x1) === 1, repeat = Boolean(msg.flags & 0x100)
    const keyCode = vkToKey(msg.key)
    // Pano kısayolları: Ctrl+V/C/X/A/Z/Y doğrudan webContents komutlarıyla (DLL yalnız tuşu iletir)
    const command = session.mods.control && !session.mods.alt && down && !repeat ? CLIPBOARD_KEYS[keyCode] : null
    if (command) { try { wc[command](); log(`pid ${pid}: pano komutu ${command}`) } catch {} session.swallowChar = true }
    if (msg.key === 0x10 || msg.key === 0xa0 || msg.key === 0xa1) session.mods.shift = down
    if (msg.key === 0x11 || msg.key === 0xa2 || msg.key === 0xa3) session.mods.control = down
    if (msg.key === 0x12 || msg.key === 0xa4 || msg.key === 0xa5) session.mods.alt = down
    if (!keyCode) return
    const modifiers = [...mods]; if (repeat) modifiers.push('isAutoRepeat')
    wc.sendInputEvent({ type: down ? 'keyDown' : 'keyUp', keyCode, modifiers })
  }
  else if (msg.type === 5) {
    const unit = msg.key & 0xffff
    // Pano komutundan sonra gelen kontrol karakteri (örn. Ctrl+V -> 0x16) düşürülür
    if (unit < 0x20 && unit !== 0x0d && unit !== 0x09) { session.swallowChar = false; return }
    session.swallowChar = false
    // UTF-16 surrogate çiftleri birleştirilir (emoji vb.)
    if (unit >= 0xd800 && unit <= 0xdbff) { session.surrogate = unit; return }
    let text = ''
    if (unit >= 0xdc00 && unit <= 0xdfff && session.surrogate) { text = String.fromCharCode(session.surrogate, unit); session.surrogate = 0 }
    else text = String.fromCharCode(unit)
    wc.sendInputEvent({ type: 'char', keyCode: text, modifiers: mods })
  }
}

function destroySession(pid) {
  const session = sessions.get(pid); if (!session) return
  if (session.toastSafety) clearTimeout(session.toastSafety)
  if (session.perfTimer) clearInterval(session.perfTimer)
  const addon = ensureHost(); try { if (addon) addon.destroy(pid) } catch {}
  try { byWebContents.delete(session.window.webContents.id) } catch {}
  if (!session.window.isDestroyed()) session.window.destroy()
  sessions.delete(pid)
  if (session.ready) { try { onReadyHook?.({ projectId: session.projectId, pid, ready: false }) } catch {} }
}

// Kök pid ile çağrılır (patcher oyunun kök sürecini izler): ağaçtaki tüm oturumlar kapanır.
function stopForGame(pid) {
  const root = roots.get(pid)
  if (root) { if (root.timer) clearInterval(root.timer); roots.delete(pid) }
  for (const [sessionPid, session] of [...sessions]) if (sessionPid === pid || session.rootPid === pid) destroySession(sessionPid)
}

// Kök süreç kapansa da ağaçtan bir süreç (asıl oyun) hâlâ çalışıyor mu? (patcher'ın oyun-bitti kararı için)
function treeAlive(rootPid) {
  const root = roots.get(rootPid); if (!root) return false
  for (const pid of root.tree) { try { process.kill(pid, 0); return true } catch {} }
  return false
}

function setVisible(pid, visible) { setVisibleFor(pid, Boolean(visible)) }
// Overlay penceresinden (Esc/kapat) ya da ana pencereden ({pid,visible}) gelen görünürlük isteği
function setVisibleFrom(webContents, arg) {
  if (arg && typeof arg === 'object') { if (sessions.has(arg.pid)) setVisibleFor(arg.pid, Boolean(arg.visible)); return }
  const pid = byWebContents.get(webContents.id); if (pid) setVisibleFor(pid, Boolean(arg))
}
function sessionOf(webContents) {
  const pid = byWebContents.get(webContents.id); const session = sessions.get(pid)
  return session ? { pid, projectId: session.projectId, projectName: session.projectName, ready: session.ready } : null
}

// Oyun içi köşe bildirimi. Panel kapalıyken DLL'e "yine de çiz" (toast biti) verilir; renderer bildirimler
// bitince draw-state ile kaldırır. Renderer hiç yanıt vermezse (çökme) emniyet zamanlayıcısı kaldırır.
function notify(pid, payload) {
  const session = sessions.get(pid); if (!session || session.window.isDestroyed()) return false
  const addon = ensureHost(); if (!addon) return false
  if (!session.loaded) { session.pendingToasts.push(payload); return true }
  const toast = { id: payload.id || `${Date.now()}-${Math.floor(Math.random() * 1e6)}`, ...payload }
  try { session.window.webContents.send('overlay:toast', toast) } catch { return false }
  session.draw.toast = true; applyDraw(pid)
  if (session.toastSafety) clearTimeout(session.toastSafety)
  const duration = toast.sticky ? 30000 : Math.max(2000, Math.min(20000, Number(toast.duration) || 7000)) + 1500
  session.toastSafety = setTimeout(() => { session.toastSafety = null; if (!session.rendererDraw) { session.draw.toast = false; applyDraw(pid) } }, duration)
  return true
}
function targetSessions(target) {
  const list = []
  for (const [pid, session] of sessions) {
    if (!session.ready) continue
    if (target === 'all' || target === undefined || target === null) list.push(pid)
    else if (typeof target === 'number') { if (pid === target) list.push(pid) }
    else if (target && typeof target === 'object' && target.projectId) { if (session.projectId === target.projectId) list.push(pid) }
  }
  return list
}
// target: 'all' | pid | {projectId}
function toast(target, payload) {
  let sent = false
  for (const pid of targetSessions(target)) sent = notify(pid, payload) || sent
  return sent
}
// Projeye göre bildirim (geriye uyum; kütüphane olayları için).
function notifyProject(projectId, payload) { return toast({ projectId }, payload) }
function dismiss(target, tag) {
  for (const pid of targetSessions(target)) { const s = sessions.get(pid); try { if (s.loaded) s.window.webContents.send('overlay:toast-dismiss', tag) } catch {} }
}
function broadcast(channel, payload) {
  for (const [, session] of sessions) { if (!session.loaded || session.window.isDestroyed()) continue; try { session.window.webContents.send(channel, payload) } catch {} }
}
// Ana pencereden gelen yapısal olaylar: gelen arama yapışkan karta dönüşür, kapanınca kart kaldırılır; gerisi aynen iletilir.
function event(ev) {
  if (!ev || typeof ev !== 'object') return
  if (ev.type === 'call-incoming') {
    const from = ev.from?.name || ev.from?.displayName || 'Biri'
    toast('all', { kind: 'call', sticky: true, tag: `call:${ev.callId}`, title: `${from} arıyor`, text: ev.label || 'Sesli arama', avatar: ev.from?.id || ev.from?.userId ? { userId: ev.from.id || ev.from.userId, name: from, url: ev.from.avatarUrl || null } : undefined, actions: [{ id: 'accept', label: 'Kabul et', primary: true }, { id: 'decline', label: 'Reddet' }], payload: { callId: ev.callId } })
    return
  }
  if (ev.type === 'call-ended') { dismiss('all', `call:${ev.callId}`); return }
  if (ev.type === 'toast-dismiss') { if (ev.tag) dismiss(ev.target ?? 'all', String(ev.tag)); return }
  broadcast('overlay:event', ev)
}

// Kısayol tablosu (ana pencere ayarlardan gönderir): canlı oturumlara da uygulanır.
function setHotkeys(list) {
  const clean = (Array.isArray(list) ? list : []).map(h => ({ id: h.id | 0, vk: h.vk | 0, mods: (h.mods | 0) | (h.passThrough ? 0x80 : 0) })).filter(h => h.id > 0 && h.vk > 0).slice(0, 8)
  if (!clean.some(h => h.id === 1)) clean.unshift({ id: 1, vk: 0x32, mods: 0x1 })
  hotkeys = clean
  const addon = ensureHost(); if (!addon) return { ok: true, sessions: 0 }
  let applied = 0
  for (const [pid] of sessions) { try { if (addon.setHotkeys) { addon.setHotkeys(pid, hotkeys); applied++ } } catch {} }
  log(`kısayol tablosu: ${hotkeys.map(h => `${h.id}:${hotkeyKeys(h).join('+')}${h.mods & 0x80 ? '(geçirgen)' : ''}`).join(' ')} (${applied} oturum)`)
  return { ok: true, sessions: applied }
}
function setCallStats(stats) { callStats = { rttMs: Number.isFinite(stats?.rttMs) ? Math.round(stats.rttMs) : null, lossPct: Number.isFinite(stats?.lossPct) ? stats.lossPct : null } }
// ---- Ekran görüntüsü (Faz 4) ----
// Akış: requestShot → DLL arka tamponu yuvaya kopyalar → SHOT_READY → readShot → PNG (yerel) + JPEG (yükleme için).
// DLL başaramazsa ya da 1500 ms'de yanıt gelmezse desktopCapturer ile oyun penceresi yakalanır (yedek yol).
let onScreenshotHook = null // (shot) => void (main.cjs ana pencereye iletir)
let screenshotDir = ''      // boş = Resimler\P2
function setScreenshotDir(dir) { screenshotDir = typeof dir === 'string' ? dir : '' }
function safeName(name) { return String(name || 'Oyun').replace(/[\\/:*?"<>|]+/g, '-').replace(/\s+/g, ' ').trim().slice(0, 60) || 'Oyun' }
function stamp() { const d = new Date(), p = n => String(n).padStart(2, '0'); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}_${p(d.getHours())}-${p(d.getMinutes())}-${p(d.getSeconds())}` }
async function captureFallback(pid, session) {
  const addon = ensureHost()
  let title = ''; try { title = addon && addon.windowTitle ? addon.windowTitle(pid) : '' } catch { title = '' }
  const size = session.bb || { w: 1920, h: 1080 }
  const sources = await desktopCapturer.getSources({ types: ['window', 'screen'], thumbnailSize: { width: size.w, height: size.h } })
  const byTitle = title ? sources.find(source => source.name === title) : null
  const byProject = sources.find(source => source.name && session.projectName && source.name.toLowerCase().includes(session.projectName.toLowerCase()))
  const screen = sources.find(source => source.id.startsWith('screen:'))
  const picked = byTitle || byProject || screen
  if (!picked || picked.thumbnail.isEmpty()) throw new Error('kaynak yok')
  log(`pid ${pid}: ekran görüntüsü yedek yol (${picked.name})`)
  return picked.thumbnail
}
async function takeScreenshot(pid, origin = 'hotkey') {
  const session = sessions.get(pid); if (!session || session.window.isDestroyed()) return { ok: false, error: 'Overlay oturumu yok.' }
  if (session.shot) return { ok: false, error: 'Ekran görüntüsü alınıyor.' }
  const addon = ensureHost(); if (!addon) return { ok: false, error: 'Overlay eklentisi yok.' }
  let id = 0
  try { id = addon.requestShot ? addon.requestShot(pid) : 0 } catch { id = 0 }
  session.shot = { id, startedAt: Date.now(), resolve: null, timer: null }
  try {
    let image = null, fallback = false
    if (id > 0) {
      const result = await new Promise(resolve => { session.shot.resolve = resolve; session.shot.timer = setTimeout(() => resolve({ ready: false, timeout: true }), 1500) })
      if (result.ready) { try { const raw = addon.readShot(pid); if (raw && raw.buffer) image = nativeImage.createFromBitmap(raw.buffer, { width: raw.width, height: raw.height }) } catch (error) { log(`pid ${pid}: readShot hatası ${error.message}`) } }
      else log(`pid ${pid}: DLL ekran görüntüsü ${result.timeout ? 'zaman aşımı' : 'başarısız'}; yedek yola geçiliyor`)
    }
    if (!image) { fallback = true; image = await captureFallback(pid, session) }
    const size = image.getSize()
    const dir = path.join(screenshotDir || path.join(app.getPath('pictures'), 'P2'), safeName(session.projectName))
    fs.mkdirSync(dir, { recursive: true })
    const file = path.join(dir, `${stamp()}.png`)
    await fs.promises.writeFile(file, image.toPNG())
    const jpegDataUrl = `data:image/jpeg;base64,${image.toJPEG(90).toString('base64')}`
    const shot = { id: id || Date.now(), path: file, width: size.width, height: size.height, projectId: session.projectId, fallback, jpegDataUrl, origin }
    log(`pid ${pid}: ekran görüntüsü kaydedildi ${file} (${size.width}x${size.height}${fallback ? ', yedek yol' : ''})`)
    try { session.window.webContents.send('overlay:screenshot', shot) } catch {}
    try { onScreenshotHook?.({ ...shot, pid }) } catch {}
    notify(pid, { kind: 'screenshot', title: 'Ekran görüntüsü alındı', text: path.basename(file), duration: 8000, tag: `shot:${shot.id}`, actions: [{ id: 'share', label: 'Paylaş', primary: true }, { id: 'task', label: 'Görev oluştur' }, { id: 'copy', label: 'Kopyala' }], payload: { shotId: shot.id, path: file, width: size.width, height: size.height, projectId: session.projectId } })
    return { ok: true, path: file, width: size.width, height: size.height, fallback }
  } catch (error) {
    log(`pid ${pid}: ekran görüntüsü alınamadı: ${error.message}`)
    notify(pid, { kind: 'error', title: 'Ekran görüntüsü alınamadı', text: error.message, duration: 4000 })
    return { ok: false, error: `Ekran görüntüsü alınamadı: ${error.message}` }
  } finally {
    if (session.shot && session.shot.timer) clearTimeout(session.shot.timer)
    session.shot = null
  }
}
// Araç çubuğu düğmesi (overlay penceresinden)
function screenshotFor(webContents) {
  const pid = byWebContents.get(webContents.id)
  if (!pid) return { ok: false, error: 'Overlay oturumu yok.' }
  return takeScreenshot(pid, 'button')
}

function configure(options) {
  if (options.loadRenderer) loadRenderer = options.loadRenderer
  if (options.suggestedServer) suggestedServer = options.suggestedServer
  if (options.onReady) onReadyHook = options.onReady
  if (options.onHotkey) onHotkeyHook = options.onHotkey
  if (options.onScreenshot) onScreenshotHook = options.onScreenshot
}

// activeSessions(): enjekte edilmiş oyun sayısı (otomatik güncelleme oyun sürerken kurulum yapmaz)
function activeSessions() { return sessions.size }
module.exports = { available, activeSessions, startForGame, stopForGame, treeAlive, setVisible, setVisibleFrom, sessionOf, setDrawState, notify, notifyProject, toast, dismiss, broadcast, event, setHotkeys, setCallStats, screenshotFor, takeScreenshot, setScreenshotDir, configure }
