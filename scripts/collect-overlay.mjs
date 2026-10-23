// Derlenen overlay ikililerini (DLL + enjektör + host eklentisi) native/overlay/dist'e toplar;
// electron-builder bu klasörü resources/overlay olarak pakete koyar.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')
const files = [
  ['native/overlay/build/Release/p2overlay64.dll', 'p2overlay64.dll'],
  ['native/overlay/build/Release/p2inject64.exe', 'p2inject64.exe'],
  ['native/overlay/host/build/Release/p2overlayhost.node', 'p2overlayhost.node'],
]
const out = join(root, 'native/overlay/dist'); mkdirSync(out, { recursive: true })
let missing = 0
for (const [from, name] of files) { const src = join(root, from); if (!existsSync(src)) { console.error('eksik: ' + from); missing++; continue } copyFileSync(src, join(out, name)); console.log('kopyalandı: ' + name) }
process.exit(missing ? 1 : 0)
