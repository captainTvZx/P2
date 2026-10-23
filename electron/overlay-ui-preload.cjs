// Overlay penceresi preload'u: ana pencereyle aynı p2 köprüsünü kurar, üstüne
// overlay'e özel kanalları (p2overlay) ekler. Sözleşme: src/overlay/overlayTypes.ts P2OverlayBridge.
require('./preload.cjs')
const { contextBridge, ipcRenderer } = require('electron')

const arg = name => { const found = process.argv.find(item => item.startsWith(`--${name}=`)); return found ? found.slice(name.length + 3) : '' }
const session = (() => {
  const pid = Number(arg('p2-overlay-pid'))
  if (!pid) return null
  let projectName = ''
  try { projectName = decodeURIComponent(arg('p2-overlay-name')) } catch { projectName = arg('p2-overlay-name') }
  return { pid, projectId: arg('p2-overlay-project'), projectName }
})()

// Tek argümanlı olay dinleyicisi üreticisi: abone ol, çıkış fonksiyonu döndür.
const on = channel => handler => {
  const listener = (_event, payload) => handler(payload)
  ipcRenderer.on(channel, listener)
  return () => ipcRenderer.removeListener(channel, listener)
}

contextBridge.exposeInMainWorld('p2overlay', {
  session: () => session,
  onVisible: on('overlay:visible'),
  setVisible: visible => ipcRenderer.send('overlay:set-visible', Boolean(visible)),
  drawState: state => ipcRenderer.send('overlay:draw-state', { toasts: Number(state?.toasts) || 0, pinned: Boolean(state?.pinned), hud: Boolean(state?.hud) }),
  onFocus: on('overlay:focus'),
  onPerf: on('overlay:perf'),
  onHotkey: on('overlay:hotkey'),
  onScreenshot: on('overlay:screenshot'),
  onEvent: on('overlay:event'),
  onToast: on('overlay:toast'),
  onToastDismiss: on('overlay:toast-dismiss'),
  toastAction: detail => ipcRenderer.send('overlay:toast-action', detail),
  screenshot: () => ipcRenderer.invoke('overlay:screenshot'),
  setZoom: factor => ipcRenderer.send('overlay:set-zoom', Number(factor) || 1),
  openInMain: detail => ipcRenderer.send('overlay:send', { topic: 'open-in-app', payload: detail }),
  send: (topic, payload) => ipcRenderer.send('overlay:send', { topic, payload }),
  onPush: on('overlay:push'),
  // Eski kanallar: çağrı özeti (masaüstü baloncuğuyla aynı yük) ve basit eylemler
  onCallState: on('overlay:callstate'),
  callAction: action => ipcRenderer.send('overlay:call-action', action),
})
