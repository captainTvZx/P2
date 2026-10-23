// DevShowcase ekranlarının görüntüsünü alır (tasarım geçişinde görsel doğrulama).
//   env -u ELECTRON_RUN_AS_NODE npx electron scripts/ui-shot.mjs [--out <klasör>] [--ids a,b,c] [--motion] [--width 1440 --height 900]
// Vite dev sunucusu (5173) açık olmalı. Varsayılan: hareket kapalı (son durum), 1440×900, tüm demolar.
import { app, BrowserWindow } from 'electron'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const args = process.argv.slice(2)
const opt = (name, fallback) => { const i = args.indexOf(`--${name}`); return i >= 0 && args[i + 1] && !args[i + 1].startsWith('--') ? args[i + 1] : fallback }
const out = opt('out', join(process.cwd(), 'ui-shots'))
const width = Number(opt('width', 1440)), height = Number(opt('height', 900))
const motion = args.includes('--motion')
const ALL = ['ui-kit', 'loading', 'update-check', 'update-download', 'update-ready', 'mode', 'login', 'organization', 'setup', 'server-host', 'server-client', 'settings', 'profile', 'confirm', 'share', 'group', 'messages', 'dropzone', 'image', 'board']
const ids = (opt('ids', '') || '').split(',').filter(Boolean)
const list = ids.length ? ids : ALL
if (!motion) app.commandLine.appendSwitch('force-prefers-reduced-motion')

app.whenReady().then(async () => {
  mkdirSync(out, { recursive: true })
  const win = new BrowserWindow({ show: false, width, height, backgroundColor: '#070b14', webPreferences: { offscreen: true, contextIsolation: true, sandbox: true, backgroundThrottling: false } })
  win.webContents.setFrameRate(30)
  win.webContents.on('console-message', (_e, level, message) => { if (level >= 2) console.log('  konsol:', String(message).slice(0, 160)) })
  // İlk yakalama offscreen pencerede boş dönebilir (UnknownVizError): ısınma turu.
  try { await win.loadURL('http://localhost:5173/?ui-demo=loading&ui-menu=0'); await new Promise(r => setTimeout(r, 500)); await win.webContents.capturePage() } catch {}
  for (const id of list) {
    try {
      await win.loadURL(`http://localhost:5173/?ui-demo=${encodeURIComponent(id)}&ui-menu=0`)
      await win.webContents.executeJavaScript('document.fonts.ready.then(()=>true)').catch(() => {})
      await new Promise(r => setTimeout(r, motion ? 900 : 700))
      const image = await win.webContents.capturePage()
      const file = join(out, `${id}@${width}.png`)
      writeFileSync(file, image.toPNG())
      console.log('ok', id, '->', file)
    } catch (error) { console.log('HATA', id, error.message) }
  }
  app.quit()
})
