const { app } = require('electron')

let updater = null
let timer = null
let notify = () => {}
let state = { status: 'idle', current: app.getVersion(), version: '', percent: 0, error: '' }
const setState = patch => { state = { ...state, ...patch }; notify(state) }

function load() {
  if (updater) return updater
  try { updater = require('electron-updater').autoUpdater } catch (error) { setState({ error: String(error?.message || error) }); return null }
  updater.autoDownload = true
  updater.autoInstallOnAppQuit = false
  updater.on('checking-for-update', () => setState({ status: 'checking', error: '' }))
  updater.on('update-not-available', () => setState({ status: 'idle', percent: 0 }))
  updater.on('update-available', info => setState({ status: 'downloading', version: info?.version || '', percent: 0, error: '' }))
  updater.on('download-progress', progress => setState({ status: 'downloading', percent: Math.round(progress?.percent || 0) }))
  updater.on('update-downloaded', info => setState({ status: 'ready', version: info?.version || state.version, percent: 100 }))
  updater.on('error', error => {
    const message = String(error?.message || error)
    setState({ status: 'idle', percent: 0, error: /404|ECONNREFUSED|ETIMEDOUT|net::/i.test(message) ? '' : message })
  })
  return updater
}

function init(options) {
  if (!app.isPackaged) return
  notify = options.broadcast || notify
  const instance = load()
  if (!instance) return
  instance.setFeedURL({ provider: 'generic', url: `http://127.0.0.1:${options.port}/updates/server/`, channel: 'latest' })
  void check()
  if (timer) clearInterval(timer)
  timer = setInterval(() => void check(), 5 * 60 * 1000)
  timer.unref?.()
}

async function check() {
  if (!app.isPackaged || state.status === 'downloading' || state.status === 'ready') return { ok: true, status: state.status }
  const instance = load()
  if (!instance) return { ok: false }
  try { await instance.checkForUpdates() } catch {}
  return { ok: true, status: state.status }
}

async function install(stopServer) {
  if (state.status !== 'ready' || !updater) return { ok: false }
  setState({ status: 'installing' })
  const stopped = await stopServer()
  if (!stopped?.ok) { setState({ status: 'ready', error: stopped?.message || 'Sunucu durdurulamadı.' }); return { ok: false } }
  try { updater.quitAndInstall(true, true); return { ok: true } }
  catch (error) { setState({ status: 'ready', error: String(error?.message || error) }); return { ok: false } }
}

module.exports = { init, check, install, getState: () => state }
