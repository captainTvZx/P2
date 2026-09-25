const { contextBridge, ipcRenderer } = require('electron')

// Açılış penceresi yalnızca durum dinler; başka hiçbir yetkisi yok.
contextBridge.exposeInMainWorld('p2splash', {
  onState: (handler) => ipcRenderer.on('updates:state', (_event, state) => handler(state)),
  onText: (handler) => ipcRenderer.on('splash:text', (_event, text) => handler(text)),
})
