const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('p2', {
  platform: process.platform,
  version: '0.2.1',
  apiUrl: process.env.P2_API_URL || 'http://127.0.0.1:4000',
  session: {
    save: (token) => ipcRenderer.invoke('session:save', token),
    load: () => ipcRenderer.invoke('session:load'),
    clear: () => ipcRenderer.invoke('session:clear'),
  },
  database: {
    status: () => ipcRenderer.invoke('database:status'),
    install: (config) => ipcRenderer.invoke('database:install', config),
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
