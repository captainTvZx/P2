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
    openPopout: (query, title) => ipcRenderer.send('window:open-popout', { query, title }),
    onCloseAction: (handler) => {
      const listener = (_, action) => handler(action)
      ipcRenderer.on('window:close-action', listener)
      return () => ipcRenderer.removeListener('window:close-action', listener)
    },
  },
  git: {
    pickFolder: () => ipcRenderer.invoke('git:pick-folder'),
    repoSource: (payload) => ipcRenderer.invoke('git:repo-source', payload),
    clone: (payload) => ipcRenderer.invoke('git:clone', payload),
    onCloneProgress: (handler) => {
      const listener = (_, progress) => handler(progress)
      ipcRenderer.on('git:clone-progress', listener)
      return () => ipcRenderer.removeListener('git:clone-progress', listener)
    },
    cloneUrl: (payload) => ipcRenderer.invoke('git:clone-url', payload),
    detect: (payload) => ipcRenderer.invoke('git:detect', payload),
    ensureAuth: (payload) => ipcRenderer.invoke('git:ensure-auth', payload),
    info: (payload) => ipcRenderer.invoke('git:info', payload),
    status: (payload) => ipcRenderer.invoke('git:status', payload),
    diff: (payload) => ipcRenderer.invoke('git:diff', payload),
    commitDiff: (payload) => ipcRenderer.invoke('git:commit-diff', payload),
    commitFiles: (payload) => ipcRenderer.invoke('git:commit-files', payload),
    commitTree: (payload) => ipcRenderer.invoke('git:commit-tree', payload),
    repositorySettings: (payload) => ipcRenderer.invoke('git:repository-settings', payload),
    saveRepositorySettings: (payload) => ipcRenderer.invoke('git:save-repository-settings', payload),
    stage: (payload) => ipcRenderer.invoke('git:stage', payload),
    unstage: (payload) => ipcRenderer.invoke('git:unstage', payload),
    discard: (payload) => ipcRenderer.invoke('git:discard', payload),
    commit: (payload) => ipcRenderer.invoke('git:commit', payload),
    sync: (payload) => ipcRenderer.invoke('git:sync', payload),
    branch: (payload) => ipcRenderer.invoke('git:branch', payload),
    stash: (payload) => ipcRenderer.invoke('git:stash', payload),
    lfs: (payload) => ipcRenderer.invoke('git:lfs', payload),
    reveal: (payload) => ipcRenderer.invoke('git:reveal', payload),
    connections: () => ipcRenderer.invoke('git:connections'),
    oauthApps: () => ipcRenderer.invoke('git:oauth-apps'),
    saveApp: (payload) => ipcRenderer.invoke('git:save-app', payload),
    login: (payload) => ipcRenderer.invoke('git:login', payload),
    deviceStart: (payload) => ipcRenderer.invoke('git:device-start', payload),
    devicePoll: (payload) => ipcRenderer.invoke('git:device-poll', payload),
    connectToken: (payload) => ipcRenderer.invoke('git:connect-token', payload),
    disconnect: (payload) => ipcRenderer.invoke('git:disconnect', payload),
    remoteRepos: (payload) => ipcRenderer.invoke('git:remote-repos', payload),
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
