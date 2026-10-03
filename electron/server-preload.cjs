const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('p2server', {
  state: () => ipcRenderer.invoke('server-console:state'),
  configure: config => ipcRenderer.invoke('server-console:configure', config),
  start: () => ipcRenderer.invoke('server-console:start'),
  stop: () => ipcRenderer.invoke('server-console:stop'),
  restart: () => ipcRenderer.invoke('server-console:restart'),
  serviceInstall: () => ipcRenderer.invoke('server-console:service-install'),
  serviceRemove: () => ipcRenderer.invoke('server-console:service-remove'),
  openData: () => ipcRenderer.invoke('server-console:open-data'),
  updateCheck: () => ipcRenderer.invoke('server-console:update-check'),
  updateInstall: () => ipcRenderer.invoke('server-console:update-install'),
  backupCreate: () => ipcRenderer.invoke('server-console:backup-create'),
  backupRestore: () => ipcRenderer.invoke('server-console:backup-restore'),
  onState: handler => {
    const listener = (_event, value) => handler(value)
    ipcRenderer.on('server-console:state', listener)
    return () => ipcRenderer.removeListener('server-console:state', listener)
  },
  onUpdate: handler => {
    const listener = (_event, value) => handler(value)
    ipcRenderer.on('server-console:update', listener)
    return () => ipcRenderer.removeListener('server-console:update', listener)
  },
})
