const { contextBridge, ipcRenderer } = require('electron')

// Ekran kaplaması yalnızca çizim mesajlarını dinler; başka hiçbir yetkisi yok.
contextBridge.exposeInMainWorld('overlay', {
  onPaint: (handler) => {
    const listener = (_, message) => handler(message)
    ipcRenderer.on('overlay:paint', listener)
    return () => ipcRenderer.removeListener('overlay:paint', listener)
  },
})
