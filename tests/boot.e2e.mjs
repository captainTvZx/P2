// Açılış duman testi: kurulu uygulama gerçekten çiziliyor mu?
// Boş pencereyle açılan sürümleri (örn. hook sırası bozulunca React hiç mount
// olmuyor) yayına çıkmadan yakalamak için. Uygulamayı hata ayıklama portuyla
// açar, arayüzün mount olmasını bekler ve renderer istisnalarını toplar.
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'

// Kurulum yeri makineden makineye değişir; 0.7 ile ürün adı da "P2 Client"
// oldu. Sabit bir yol aramak testi tek bir bilgisayara bağlıyordu.
function installedClient() {
  const programs = join(process.env.LOCALAPPDATA || join(homedir(), 'AppData', 'Local'), 'Programs')
  const known = [
    join(programs, 'P2 Client', 'P2 Client.exe'),
    join(programs, 'P2', 'P2.exe'),
  ]
  return known.find(existsSync) || known[0]
}

const exe = process.env.P2_EXE || installedClient()
if (!existsSync(exe)) {
  console.error(`P2 istemcisi kurulu değil: ${exe}\nKurulumu yapın ya da P2_EXE ile yolu belirtin.`)
  process.exit(1)
}
const port = Number(process.env.P2_DEBUG_PORT || 9351)
const sleep = ms => new Promise(r => setTimeout(r, ms))
const env = { ...process.env }
delete env.ELECTRON_RUN_AS_NODE
const profile = mkdtempSync(join(tmpdir(), 'p2-boot-'))

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

const child = spawn(exe, [`--remote-debugging-port=${port}`, `--user-data-dir=${profile}`], { stdio: 'ignore', env })
const cleanup = () => { try { child.kill() } catch {}; try { rmSync(profile, { recursive: true, force: true }) } catch {} }
process.on('exit', cleanup)

// Ana pencere açılış turundan (güncelleme denetimi) sonra gelir.
let page = null
for (let tur = 0; tur < 90; tur++) {
  await sleep(1000)
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json()
    page = targets.find(t => t.type === 'page' && t.url.includes('index.html')) || null
    if (page) break
  } catch {}
}
check('ana pencere açıldı', Boolean(page), page ? '' : '90 saniyede pencere yok')
if (!page) { console.log('\n1 başarısız'); cleanup(); process.exit(1) }

const ws = new WebSocket(page.webSocketDebuggerUrl)
await new Promise(r => { ws.onopen = r })
let id = 0
const istisnalar = []
ws.addEventListener('message', ev => {
  const m = JSON.parse(ev.data)
  if (m.method === 'Runtime.exceptionThrown') istisnalar.push((m.params.exceptionDetails?.exception?.description || m.params.exceptionDetails?.text || '').split('\n')[0])
})
ws.send(JSON.stringify({ id: ++id, method: 'Runtime.enable' }))
const call = (method, params = {}) => new Promise(resolve => {
  const me = ++id
  const on = ev => { const m = JSON.parse(ev.data); if (m.id === me) { ws.removeEventListener('message', on); resolve(m.result) } }
  ws.addEventListener('message', on)
  ws.send(JSON.stringify({ id: me, method, params }))
})
const js = async expr => (await call('Runtime.evaluate', { expression: expr, returnByValue: true }))?.result?.value

let uzunluk = 0
for (let tur = 0; tur < 30; tur++) {
  uzunluk = Number(await js('document.getElementById("root")?.innerHTML.length ?? 0'))
  if (uzunluk > 0) break
  await sleep(1000)
}
check('arayüz çizildi', uzunluk > 0, `#root içeriği ${uzunluk} karakter`)
check('renderer istisnası yok', !istisnalar.length, istisnalar.slice(0, 3).join(' | '))
console.log('ekran:', JSON.stringify(String(await js('document.body.innerText')).replace(/\s+/g, ' ').slice(0, 120)))

ws.close()
cleanup()
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
