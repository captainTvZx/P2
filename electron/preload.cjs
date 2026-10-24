const { contextBridge, ipcRenderer } = require('electron')

// Kurulumda "yalnızca istemci" seçilirse forma yazılacak hazır sunucu adresi.
// Preload sandbox içinde çalıştığı için dosya okuyamaz; adres ana süreçten argümanla gelir.
const SUGGESTED_FLAG = '--p2-suggested-server='
function suggestedServer() {
  const argument = process.argv.find(item => item.startsWith(SUGGESTED_FLAG))
  return argument ? argument.slice(SUGGESTED_FLAG.length) : ''
}

// file:// kökeninden http kökenine geçişte yerel ayarlar bir kez taşınır.
try {
  if (!window.localStorage.length) {
    const legacy = ipcRenderer.sendSync('prefs:legacy') || {}
    for (const [key, value] of Object.entries(legacy)) window.localStorage.setItem(key, String(value))
  }
} catch {}

contextBridge.exposeInMainWorld('p2', {
  platform: process.platform,
  // Sürüm ana süreçten geliyor (app.getVersion); elle yazılan değer güncellemede yanıltıcı olur.
  version: (process.argv.find(item => item.startsWith('--p2-version=')) || '').split('=')[1] || '',
  apiUrl: process.env.P2_API_URL || 'http://127.0.0.1:4000',
  suggestedServer: suggestedServer(),
  session: {
    save: (token) => ipcRenderer.invoke('session:save', token),
    load: () => ipcRenderer.invoke('session:load'),
    clear: () => ipcRenderer.invoke('session:clear'),
  },
  database: {
    status: () => ipcRenderer.invoke('database:status'),
    install: (config) => ipcRenderer.invoke('database:install', config),
    reset: () => ipcRenderer.invoke('database:reset'),
    serverStatus: () => ipcRenderer.invoke('server:status'),
    serverStart: () => ipcRenderer.invoke('server:start'),
    serverLoginItem: (enabled) => ipcRenderer.invoke('server:login-item', enabled),
    serviceStatus: () => ipcRenderer.invoke('server:service-status'),
    installService: () => ipcRenderer.invoke('server:install-service'),
    serverLog: () => ipcRenderer.invoke('server:log'),
    serverWait: () => ipcRenderer.invoke('server:wait'),
    openServerFolder: () => ipcRenderer.invoke('server:open-folder'),
  },
  files: {
    saveAs: (file) => ipcRenderer.invoke('file:save-as', file),
  },
  systemAudio: {
    start: () => ipcRenderer.invoke('systemAudio:start'),
    stop: () => ipcRenderer.send('systemAudio:stop'),
    onChunk: (handler) => {
      const listener = (_, chunk) => handler(chunk)
      ipcRenderer.on('systemAudio:chunk', listener)
      return () => ipcRenderer.removeListener('systemAudio:chunk', listener)
    },
  },
  overlay: {
    show: (sourceId) => ipcRenderer.send('overlay:show', { sourceId }),
    hide: () => ipcRenderer.send('overlay:hide'),
    paint: (message) => ipcRenderer.send('overlay:paint', message),
  },
  screen: {
    sources: () => ipcRenderer.invoke('screen:sources'),
    select: (choice) => ipcRenderer.send('screen:select', choice),
  },
  notifications: {
    show: (payload) => ipcRenderer.invoke('notification:show', payload),
    onActivate: (handler) => {
      const listener = (_, data) => handler(data)
      ipcRenderer.on('notification:activate', listener)
      return () => ipcRenderer.removeListener('notification:activate', listener)
    },
  },
  windowControls: {
    onRestored: (handler) => {
      const listener = () => handler()
      ipcRenderer.on('window:restored', listener)
      return () => ipcRenderer.removeListener('window:restored', listener)
    },
    minimize: () => ipcRenderer.send('window:minimize'),
    toggleMaximize: () => ipcRenderer.send('window:toggle-maximize'),
    close: () => ipcRenderer.send('window:close'),
    isMaximized: () => ipcRenderer.invoke('window:is-maximized'),
    setFullScreen: (on) => ipcRenderer.send('window:set-fullscreen', on),
    setCloseAction: (action) => ipcRenderer.send('window:set-close-action', action),
    openPopout: (query, title) => ipcRenderer.send('window:open-popout', { query, title }),
    onCloseAction: (handler) => {
      const listener = (_, action) => handler(action)
      ipcRenderer.on('window:close-action', listener)
      return () => ipcRenderer.removeListener('window:close-action', listener)
    },
  },
  updates: {
    configure: (apiBase) => ipcRenderer.invoke('updates:configure', apiBase),
    check: () => ipcRenderer.invoke('updates:check'),
    install: () => ipcRenderer.invoke('updates:install'),
    state: () => ipcRenderer.invoke('updates:state'),
    busy: (flag) => ipcRenderer.send('updates:busy', Boolean(flag)),
    onState: (handler) => {
      const listener = (_event, value) => handler(value)
      ipcRenderer.on('updates:state', listener)
      return () => ipcRenderer.removeListener('updates:state', listener)
    },
  },
  // Etkin temanın token'ları: açılış penceresi (renderer yüklenmeden) aynı renklerle çizilsin diye kalıcı dosyaya yazılır.
  appearance: {
    save: (data) => ipcRenderer.send('appearance:save', data),
  },
  media: {
    openVideo: (id) => ipcRenderer.invoke('media:open-video', id),
  },
  library: {
    configure: (payload) => ipcRenderer.invoke('library:configure', payload),
    state: () => ipcRenderer.invoke('library:state'),
    chooseRoot: () => ipcRenderer.invoke('library:choose-root'),
    setChannel: (payload) => ipcRenderer.invoke('library:set-channel', payload),
    setLimit: (payload) => ipcRenderer.invoke('library:set-limit', payload),
    setArgs: (payload) => ipcRenderer.invoke('library:set-args', payload),
    install: (payload) => ipcRenderer.invoke('library:install', payload),
    pause: (payload) => ipcRenderer.invoke('library:pause', payload),
    resume: (payload) => ipcRenderer.invoke('library:resume', payload),
    cancel: (payload) => ipcRenderer.invoke('library:cancel', payload),
    launch: (payload) => ipcRenderer.invoke('library:launch', payload),
    stop: (payload) => ipcRenderer.invoke('library:stop', payload),
    uninstall: (payload) => ipcRenderer.invoke('library:uninstall', payload),
    verify: (payload) => ipcRenderer.invoke('library:verify', payload),
    openFolder: (payload) => ipcRenderer.invoke('library:open-folder', payload),
    createShortcut: (payload) => ipcRenderer.invoke('library:create-shortcut', payload),
    pickBuildFolder: () => ipcRenderer.invoke('library:pick-build-folder'),
    upload: (payload) => ipcRenderer.invoke('library:upload', payload),
    uploadCancel: (payload) => ipcRenderer.invoke('library:upload-cancel', payload),
    uploads: () => ipcRenderer.invoke('library:uploads'),
    uploadResume: (payload) => ipcRenderer.invoke('library:upload-resume', payload),
    uploadForget: (payload) => ipcRenderer.invoke('library:upload-forget', payload),
    onEvent: (handler) => {
      const listener = (_event, value) => handler(value)
      ipcRenderer.on('library:event', listener)
      return () => ipcRenderer.removeListener('library:event', listener)
    },
    onPlay: (handler) => {
      const listener = (_event, value) => handler(value)
      ipcRenderer.on('library:play', listener)
      return () => ipcRenderer.removeListener('library:play', listener)
    },
  },
  call: {
    widgetVisible: () => ipcRenderer.invoke('call:widget-visible'),
    setWidgetVisible: (visible) => ipcRenderer.invoke('call:widget-set-visible', visible),
    update: (summary) => ipcRenderer.send('call:update', summary),
    onWidgetAction: (handler) => {
      const listener = (_, action) => handler(action)
      ipcRenderer.on('call:widget-action', listener)
      return () => ipcRenderer.removeListener('call:widget-action', listener)
    },
  },
  // Oyun içi overlay köprüsü (ana pencere tarafı). Sözleşme: src/overlay/overlayTypes.ts GameOverlayApi.
  // (p2.overlay ekran paylaşımı işaretçi katmanına ait; karıştırma.)
  gameOverlay: {
    push: (topic, payload) => ipcRenderer.send('overlay:push', { topic, payload }),
    onSend: (handler) => {
      const listener = (_, message) => handler(message)
      ipcRenderer.on('overlay:send', listener)
      return () => ipcRenderer.removeListener('overlay:send', listener)
    },
    setHotkeys: (list) => ipcRenderer.invoke('overlay:set-hotkeys', list),
    callStats: (stats) => ipcRenderer.send('overlay:call-stats', stats),
    toast: (target, toast) => ipcRenderer.send('overlay:toast', { target, toast }),
    event: (event) => ipcRenderer.send('overlay:event', event),
    onToastAction: (handler) => {
      const listener = (_, detail) => handler(detail)
      ipcRenderer.on('overlay:toast-action', listener)
      return () => ipcRenderer.removeListener('overlay:toast-action', listener)
    },
    onPtt: (handler) => {
      const listener = (_, down) => handler(Boolean(down))
      ipcRenderer.on('call:ptt', listener)
      return () => ipcRenderer.removeListener('call:ptt', listener)
    },
    onReady: (handler) => {
      const listener = (_, info) => handler(info)
      ipcRenderer.on('overlay:ready', listener)
      return () => ipcRenderer.removeListener('overlay:ready', listener)
    },
    pickScreenshotDir: () => ipcRenderer.invoke('overlay:pick-screenshot-dir'),
    setVisible: (pid, visible) => ipcRenderer.send('overlay:set-visible', { pid, visible: Boolean(visible) }),
    configure: (options) => ipcRenderer.send('overlay:configure', options || {}),
    onScreenshot: (handler) => {
      const listener = (_, shot) => handler(shot)
      ipcRenderer.on('overlay:screenshot', listener)
      return () => ipcRenderer.removeListener('overlay:screenshot', listener)
    },
  },
})
