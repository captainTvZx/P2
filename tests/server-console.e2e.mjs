import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const exe = process.env.P2_SERVER_EXE || join(process.cwd(), 'release-package', 'server', 'win-unpacked', 'P2 Server.exe')
const port = Number(process.env.P2_SERVER_DEBUG_PORT || 9352)
const profile = mkdtempSync(join(tmpdir(), 'p2-server-console-'))
const env = { ...process.env }
delete env.ELECTRON_RUN_AS_NODE
const child = spawn(exe, ['--p2-test-multiple', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`], { stdio: 'ignore', env })
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const cleanup = () => { try { child.kill() } catch {}; try { rmSync(profile, { recursive: true, force: true }) } catch {} }
process.on('exit', cleanup)

let page
for (let attempt = 0; attempt < 30; attempt++) {
  await sleep(500)
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
    page = targets.find(item => item.type === 'page' && item.url.includes('server-console.html'))
    if (page) break
  } catch {}
}
if (!page) { console.error('FAIL P2 Server Console penceresi 15 saniyede açılmadı'); cleanup(); process.exit(1) }
const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise(resolve => { ws.onopen = resolve })
let requestId = 0
const evaluate = expression => new Promise(resolve => {
  const id = ++requestId
  const listener = event => {
    const message = JSON.parse(event.data)
    if (message.id !== id) return
    ws.removeEventListener('message', listener)
    resolve(message.result?.result?.value || '')
  }
  ws.addEventListener('message', listener)
  ws.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true } }))
})
let text = ''
for (let attempt = 0; attempt < 20; attempt++) {
  text = await evaluate('document.body?.textContent || ""')
  if (/P2 Server Console/.test(text)) break
  await sleep(250)
}
const ok = /P2 Server Console/.test(text) && /PostgreSQL yapılandırması/.test(text) && /Yedekleme ve taşıma/.test(text) && /Yedek al/.test(text) && /Yedek yükle/.test(text) && /canlı günlük/i.test(text) && /Windows servisi olarak kaydet/.test(text) && /Client update URL/i.test(text)
console.log(`${ok ? 'OK  ' : 'FAIL'} yönetim konsolu çizildi${ok ? '' : ` · ${String(text).slice(0, 160)}`}`)
ws.close(); cleanup(); process.exitCode = ok ? 0 : 1
