// Açılış duman testi: kurulu uygulama gerçekten çiziliyor mu?
// Boş pencereyle açılan sürümleri (örn. hook sırası bozulunca React hiç mount
// olmuyor) yayına çıkmadan yakalamak için. Uygulamayı hata ayıklama portuyla
// açar, arayüzün mount olmasını bekler ve renderer istisnalarını toplar.
import { spawn } from 'node:child_process'

const exe = process.env.P2_EXE || 'C:/Users/captainTvZ/AppData/Local/Programs/P2/P2.exe'
const port = Number(process.env.P2_DEBUG_PORT || 9351)
const sleep = ms => new Promise(r => setTimeout(r, ms))
const env = { ...process.env }
delete env.ELECTRON_RUN_AS_NODE

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

spawn(exe, [`--remote-debugging-port=${port}`], { detached: true, stdio: 'ignore', env }).unref()

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
if (!page) { console.log('\n1 başarısız'); process.exit(1) }

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
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
