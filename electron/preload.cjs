const { contextBridge, ipcRenderer } = require('electron')

// Kurulumda "yalnızca istemci" seçilirse forma yazılacak hazır sunucu adresi.
// Preload sandbox içinde çalıştığı için dosya okuyamaz; adres ana süreçten argümanla gelir.
const SUGGESTED_FLAG = '--p2-suggested-server='
function suggestedServer() {
  const argument = process.argv.find(item => item.startsWith(SUGGESTED_FLAG))
  return argument ? argument.slice(SUGGESTED_FLAG.length) : ''
}

contextBridge.exposeInMainWorld('p2', {
  platform: process.platform,
  version: '0.2.1',
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
    minimize: () => ipcRenderer.send('window:minimize'),
    toggleMaximize: () => ipcRenderer.send('window:toggle-maximize'),
    close: () => ipcRenderer.send('window:close'),
    isMaximized: () => ipcRenderer.invoke('window:is-maximized'),
    setFullScreen: (on) => ipcRenderer.send('window:set-fullscreen', on),
    setCloseAction: (action) => ipcRenderer.send('window:set-close-action', action),
    onCloseAction: (handler) => {
      const listener = (_, action) => handler(action)
      ipcRenderer.on('window:close-action', listener)
      return () => ipcRenderer.removeListener('window:close-action', listener)
    },
  },
  call: {
    update: (summary) => ipcRenderer.send('call:update', summary),
    onWidgetAction: (handler) => {
      const listener = (_, action) => handler(action)
      ipcRenderer.on('call:widget-action', listener)
      return () => ipcRenderer.removeListener('call:widget-action', listener)
    },
  },
})
