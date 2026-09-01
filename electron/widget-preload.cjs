const { contextBridge, ipcRenderer } = require('electron')

// Masaüstündeki küçük arama balonu yalnızca gösterim yapar; eylemleri ana pencereye iletir.
contextBridge.exposeInMainWorld('p2widget', {
  onUpdate: (handler) => ipcRenderer.on('widget:update', (_, summary) => handler(summary)),
  action: (name) => ipcRenderer.send('widget:action', name),
})
