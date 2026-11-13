// Faz 8 testi: GEÇİCİ bir klasörde gerçek bir Electron profili üretir (kullanıcının %APPDATA%\P2'sine
// dokunmaz). Electron'un kendi safeStorage'ı ile session.bin + "Local State" (DPAPI korumalı
// os_crypt anahtarı) yazılır; yerel bir http kökeninde localStorage doldurulur ve köprü modülü
// (electron/migrate-bridge.cjs) migrate.json'u yazar — 0.10.x köprü sürümünün yaptığının aynısı.
//   electron.exe rust/tools/faz8/electron-profile.cjs <profil klasörü> <girdi.json>
const { app, BrowserWindow, safeStorage } = require('electron')
const fs = require('node:fs')
const http = require('node:http')
const path = require('node:path')

const [dir, inputFile] = process.argv.slice(-2)
app.setName('p2-faz8-test')
app.setPath('userData', dir)
app.setPath('sessionData', dir)
app.setPath('crashDumps', path.join(dir, 'crash'))
app.disableHardwareAcceleration()

app.whenReady().then(async () => {
  const input = JSON.parse(fs.readFileSync(inputFile, 'utf8'))
  const result = { encryption: safeStorage.isEncryptionAvailable() }
  if (input.refreshToken) fs.writeFileSync(path.join(dir, 'session.bin'), safeStorage.encryptString(input.refreshToken))
  for (const [name, content] of Object.entries(input.files || {})) {
    fs.mkdirSync(path.dirname(path.join(dir, name)), { recursive: true })
    fs.writeFileSync(path.join(dir, name), typeof content === 'string' ? content : JSON.stringify(content))
  }
  const server = http.createServer((_req, res) => { res.setHeader('content-type', 'text/html'); res.end('<!doctype html><title>p2</title>') })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const win = new BrowserWindow({ show: false, webPreferences: { sandbox: true, contextIsolation: true } })
  await win.loadURL(`http://127.0.0.1:${server.address().port}/`)
  await win.webContents.executeJavaScript(`(${(items => { for (const [k, v] of Object.entries(items)) localStorage.setItem(k, v) }).toString()})(${JSON.stringify(input.localStorage || {})})`)
  const bridge = require(path.join(__dirname, '..', '..', '..', 'electron', 'migrate-bridge.cjs'))
  result.snapshot = await bridge.snapshot(win.webContents, { userData: dir, appVersion: input.appVersion || '0.10.0' })
  // İkinci anlık görüntü değişiklik yoksa yeniden yazmaz; boş localStorage dolu dosyayı ezmez.
  result.unchanged = await bridge.snapshot(win.webContents, { userData: dir, appVersion: input.appVersion || '0.10.0' })
  win.destroy()
  server.close()
  process.stdout.write(JSON.stringify(result) + '\n')
  app.quit()
})
