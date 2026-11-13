// Parite aracı: Electron istemcisini (electron/main.cjs, geliştirme modu) YALITILMIŞ biçimde çalıştırır.
// parity.mjs bunu geçici bir uygulama klasöründen (package.json + main.cjs → bu dosya) başlatır.
//
// Güvenlik (kullanıcının gerçek kurulumuna dokunmamak için):
//  - userData/sessionData geçici profile (P2_PARITY_PROFILE) yönlendirilir; gerçek %APPDATA%\P2 hiç açılmaz.
//    Tek örnek kilidi de userData'ya bağlı olduğundan kullanıcının açık P2'siyle çakışmaz.
//  - p2:// kaydı (setAsDefaultProtocolClient), Başlat menüsü kısayolu onarımı (writeShortcutLink),
//    schtasks/sc.exe/powershell/taskkill/reg çağrıları (sunucu görevini başlatma/durdurma) etkisizdir.
//  - Masaüstü bildirimleri gösterilmez; pencere odak çalmadan (showInactive) açılır.
//  - Renderer, http://localhost:5173 yerine parity.mjs'nin açtığı Vite adresinden (P2_PARITY_RENDERER) yüklenir
//    ve ?api=<test sunucusu> ile test sunucusuna bağlanır.
const path = require('node:path')
const fs = require('node:fs')
const Module = require('node:module')
const childProcess = require('node:child_process')
const electron = require('electron')
const { app } = electron

const REPO = path.resolve(__dirname, '..', '..', '..')
const profile = process.env.P2_PARITY_PROFILE || ''
const realAppData = path.join(process.env.APPDATA || '', 'P2').toLowerCase()
if (!profile || path.resolve(profile).toLowerCase() === realAppData || path.resolve(profile).toLowerCase().startsWith(path.join(process.env.APPDATA || '').toLowerCase())) {
  console.error('[parite] P2_PARITY_PROFILE geçici bir klasör olmalı (APPDATA altında olamaz)')
  process.exit(3)
}
fs.mkdirSync(profile, { recursive: true })
app.setPath('userData', profile)
app.setPath('sessionData', profile)
app.setPath('crashDumps', path.join(profile, 'crash'))
app.setPath('logs', path.join(profile, 'logs'))

const cdpPort = process.env.P2_PARITY_CDP_PORT || '9333'
app.commandLine.appendSwitch('remote-debugging-port', cdpPort)
app.commandLine.appendSwitch('remote-allow-origins', '*')
// Ölçek 1: Rust penceresiyle aynı fiziksel piksel boyutu (1280×800).
app.commandLine.appendSwitch('force-device-scale-factor', '1')
app.commandLine.appendSwitch('high-dpi-support', '1')
// Pencere başka pencerelerin arkasındayken de çizilsin (CDP görüntüsü takılmasın).
app.commandLine.appendSwitch('disable-features', 'CalculateNativeWinOcclusion')
app.commandLine.appendSwitch('disable-backgrounding-occluded-windows')
app.commandLine.appendSwitch('disable-renderer-backgrounding')

// ---- Sahte medya (P2_PARITY_FAKE_MEDIA=1; arama ekranları): GERÇEK ses/görüntü aygıtı hiç açılmaz.
//  1) Chromium: sahte yakalama aygıtları, ses giriş/çıkışı kapalı, sekme sessiz.
//  2) Sayfa betiklerinden önce fake-media-preload.cjs (getUserMedia/getDisplayMedia/AudioContext/medya öğeleri sahte).
//  3) 'media'/'display-capture' İSTEKLERİ reddedilir (taklit atlanırsa bile gerçek istek başarısız olur). İzin
//     DENETİMİ (check) olduğu gibi kalır: Chromium yerel IP adaylarını yalnız medya izni varsa açar; yoksa mDNS
//     (.local) adayları verir ve libwebrtc botu bunları çözemez (bağlantı kurulmaz).
//  4) desktopCapturer sentetik kaynaklar döner (gerçek pencereleriniz görünmez); WASAPI loopback modülü yüklenmez.
const FAKE_MEDIA = process.env.P2_PARITY_FAKE_MEDIA === '1'
const FAKE_SOURCES = [
  { id: 'screen:0:0', name: 'Ekran 1', rgb: [0x1d, 0x3b, 0x66] },
  { id: 'window:101:0', name: 'P2 — parite', rgb: [0x2b, 0x4c, 0x3a] },
  { id: 'window:102:0', name: 'Not Defteri', rgb: [0x4a, 0x2f, 0x55] },
]
if (FAKE_MEDIA) {
  for (const sw of ['use-fake-device-for-media-stream', 'use-fake-ui-for-media-stream', 'disable-audio-output', 'disable-audio-input', 'mute-audio']) app.commandLine.appendSwitch(sw)
}
const DENIED = new Set(['media', 'display-capture', 'speaker-selection'])
function wrapSession(target) {
  return new Proxy(target, {
    get(t, k) {
      if (k === 'setPermissionRequestHandler') return h => t.setPermissionRequestHandler((wc, perm, cb, d) => DENIED.has(perm) ? cb(false) : h ? h(wc, perm, cb, d) : cb(true))
      if (k === 'setDisplayMediaRequestHandler') return (_h, o) => t.setDisplayMediaRequestHandler((_r, cb) => cb({}), o)
      const v = Reflect.get(t, k)
      return typeof v === 'function' ? v.bind(t) : v
    },
  })
}
const fakeSession = new Proxy(electron.session, {
  get(t, k) {
    if (k === 'defaultSession') return wrapSession(t.defaultSession)
    const v = Reflect.get(t, k)
    return typeof v === 'function' ? v.bind(t) : v
  },
})
function fakeThumb([r, g, b], width, height) {
  const buf = Buffer.alloc(width * height * 4)
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const i = (y * width + x) * 4
    const inner = x > width * 0.1 && x < width * 0.9 && y > height * 0.12 && y < height * 0.3
    buf[i] = inner ? 0xee : b; buf[i + 1] = inner ? 0xee : g; buf[i + 2] = inner ? 0xee : r; buf[i + 3] = 255
  }
  return electron.nativeImage.createFromBitmap(buf, { width, height })
}
const fakeCapturer = {
  async getSources(options = {}) {
    const size = options.thumbnailSize || { width: 150, height: 150 }
    const types = options.types || ['screen', 'window']
    return FAKE_SOURCES.filter(s => types.includes(s.id.split(':')[0])).map(s => ({
      id: s.id, name: s.name, display_id: s.id.startsWith('screen') ? '1' : '',
      thumbnail: size.width > 0 ? fakeThumb(s.rgb, size.width, size.height) : electron.nativeImage.createEmpty(),
      appIcon: null,
    }))
  },
}
if (FAKE_MEDIA) {
  app.whenReady().then(() => {
    const ses = electron.session.defaultSession
    ses.setPermissionRequestHandler((_wc, perm, cb) => cb(!DENIED.has(perm)))
    ses.registerPreloadScript({ type: 'frame', id: 'p2-parity-fake-media', filePath: path.join(__dirname, 'fake-media-preload.cjs') })
    console.log('[parite] sahte medya etkin (aygıt yok)')
  })
}

// Ana süreçte yakalanmamış hata kullanıcının masaüstünde modal hata kutusu açar ve Electron'u kilitler:
// yalnız günlüğe yazılır.
process.on('uncaughtException', error => console.error('[parite] yakalanmamış hata: ' + (error?.stack || error)))
process.on('unhandledRejection', error => console.error('[parite] yakalanmamış ret: ' + (error?.stack || error)))

// ---- Yan etkisi olan çağrılar etkisiz
app.setAsDefaultProtocolClient = () => false
app.removeAsDefaultProtocolClient = () => false
try { electron.shell.writeShortcutLink = () => false } catch {}
const BLOCKED = /(^|[\\/])(schtasks|sc|powershell|pwsh|taskkill|reg|wmic|netsh)(\.exe)?$/i
const blockedError = cmd => Object.assign(new Error(`[parite] engellendi: ${cmd}`), { code: 'P2_PARITY_BLOCKED', status: 1 })
for (const name of ['execFile', 'execFileSync', 'spawn', 'spawnSync', 'exec', 'execSync']) {
  const original = childProcess[name]
  childProcess[name] = function (cmd, ...rest) {
    const file = String(cmd || '').trim().split(/\s+/)[0].replace(/"/g, '')
    if (BLOCKED.test(file)) {
      console.log(`[parite] engellendi: ${name} ${file}`)
      if (name.endsWith('Sync')) throw blockedError(file)
      const callback = rest.find(arg => typeof arg === 'function')
      if (callback) setImmediate(() => callback(blockedError(file), '', ''))
      const fake = new (require('node:events').EventEmitter)()
      fake.kill = () => {}
      fake.pid = 0
      // 'error' yalnız dinleyici varsa (yoksa yakalanmamış istisna olur); her durumda süreç 1 koduyla 'bitmiş' sayılır.
      fake.stdout = new (require('node:events').EventEmitter)()
      fake.stderr = new (require('node:events').EventEmitter)()
      setImmediate(() => { if (fake.listenerCount('error')) fake.emit('error', blockedError(file)); fake.emit('exit', 1, null); fake.emit('close', 1, null) })
      return fake
    }
    return original.call(this, cmd, ...rest)
  }
}
// Güncelleyici: sunucu görevini sürdürme/durdurma ve kurulum yok.
try {
  const updates = require(path.join(REPO, 'electron', 'updater.cjs'))
  updates.resumeServerTask = async () => {}
  updates.stopServerProcesses = async () => {}
  updates.installIfReady = () => {}
  updates.startupCheck = async () => 'none'
} catch (error) { console.log('[parite] updater yamalanamadı: ' + error.message) }

// ---- BrowserWindow: odak çalmadan göster, renderer adresini çevir
const RENDERER = (process.env.P2_PARITY_RENDERER || 'http://127.0.0.1:5199').replace(/\/+$/, '')
const API = process.env.P2_PARITY_API || 'http://127.0.0.1:4110'
const WIN_X = Number(process.env.P2_PARITY_WIN_X || NaN)
const WIN_Y = Number(process.env.P2_PARITY_WIN_Y || NaN)
function rewrite(url) {
  const text = String(url)
  if (!/^https?:\/\/localhost:5173/.test(text)) return url
  const target = new URL(text.replace(/^https?:\/\/localhost:5173/, RENDERER))
  // API adresi sorguyla değil, sayfa önyükleme betiğiyle (localStorage) verilir; böylece ekranlar geçersiz kılabilir.
  return target.toString()
}
class ParityWindow extends electron.BrowserWindow {
  constructor(options = {}) {
    const main = Number(options.width) === 1280 && Number(options.height) === 800
    const placed = main && Number.isFinite(WIN_X) && Number.isFinite(WIN_Y) ? { x: WIN_X, y: WIN_Y } : {}
    super({ ...options, ...placed, show: false })
    if (options.show !== false) this.showInactive()
  }
  loadURL(url, ...rest) { return super.loadURL(rewrite(url), ...rest) }
  show() { return this.showInactive() }
  focus() {}
}
class SilentNotification extends require('node:events').EventEmitter {
  constructor(options) { super(); this.options = options }
  show() { console.log('[parite] bildirim (gösterilmedi): ' + (this.options?.title || '')) }
  close() {}
  static isSupported() { return false }
}
const patched = new Proxy(electron, {
  get(target, key) {
    if (key === 'BrowserWindow') return ParityWindow
    if (key === 'Notification') return SilentNotification
    if (FAKE_MEDIA && key === 'session') return fakeSession
    if (FAKE_MEDIA && key === 'desktopCapturer') return fakeCapturer
    return Reflect.get(target, key)
  },
})
const originalLoad = Module._load
Module._load = function (request, parent, ...rest) {
  if (FAKE_MEDIA && /loopback\.node$/i.test(String(request))) throw new Error('[parite] sahte medya: loopback modülü yüklenmez')
  if (request === 'electron' && parent && String(parent.filename || '').startsWith(path.join(REPO, 'electron'))) return patched
  return originalLoad.call(this, request, parent, ...rest)
}

console.log(`[parite] Electron profil=${profile} cdp=${cdpPort} renderer=${RENDERER} api=${API}`)
require(path.join(REPO, 'electron', 'main.cjs'))
