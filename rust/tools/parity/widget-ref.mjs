// Arama balonu (electron/main.cjs WIDGET_HTML) referans görüntüsü — I ajanı.
// Balon ayrı, ana süreçte üretilen bir HTML penceresi (ses/medya yok): HTML main.cjs'ten okunur,
// `window.p2widget` sahte özetle beslenir ve açık bir parite oturumunun (session.mjs) sayfasında
// 404×126 bir iframe'de (siyah zemin = Rust balon penceresi yakalamasındaki saydam alan) çizilir.
//   node rust/tools/parity/widget-ref.mjs --name callI --out C:/p2-pg/parity/widget-electron.png [--muted] [--deafened] [--ringing]
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { Cdp, waitTarget } from './lib/cdp.mjs'
import { REPO, PARITY_DIR, parseArgs } from './lib/common.mjs'

const args = parseArgs(process.argv.slice(2))
const require = createRequire(join(REPO, 'package.json'))
const { P2_MAVI_CSS } = require(join(REPO, 'electron', 'p2-mavi.cjs'))
const main = readFileSync(join(REPO, 'electron', 'main.cjs'), 'utf8')
const start = main.indexOf('const WIDGET_HTML = `') + 'const WIDGET_HTML = `'.length
const end = main.indexOf('</html>`', start) + '</html>'.length
// Şablon değişmezi aynen değerlendirilir (kaçışlar ve ${P2_MAVI_CSS} JS kurallarıyla).
const html = new Function('P2_MAVI_CSS', 'return `' + main.slice(start, end) + '`')(P2_MAVI_CSS)
const now = Date.now()
const summary = { active: true, label: args.label || 'Sesli oda', participants: Number(args.participants || 2), muted: Boolean(args.muted), deafened: Boolean(args.deafened), startedAt: now - 5000, connectedAt: args.ringing ? null : now - 5000, ringing: Boolean(args.ringing) }
const stub = `<script>window.p2widget={onUpdate:h=>setTimeout(()=>h(${JSON.stringify(summary)}),0),action:()=>{}}</script>`
const doc = html.replace('<script>', stub + '<script>')
writeFileSync(join(PARITY_DIR, 'widget-ref.html'), doc)

const state = JSON.parse(readFileSync(join(PARITY_DIR, `session-${args.name || 'default'}.json`), 'utf8'))
const target = await waitTarget(state.cdpPort, t => t.url.startsWith(state.renderer), 5000)
const cdp = await Cdp.connect(target.webSocketDebuggerUrl)
try {
  await cdp.eval(`(() => {
    document.getElementById('p2-widget-ref')?.remove()
    const wrap = document.createElement('div')
    wrap.id = 'p2-widget-ref'
    wrap.style.cssText = 'position:fixed;left:0;top:0;width:404px;height:126px;z-index:2147483647;background:#000'
    const frame = document.createElement('iframe')
    frame.style.cssText = 'border:0;width:404px;height:126px;background:transparent'
    frame.setAttribute('allowtransparency', 'true')
    frame.srcdoc = ${JSON.stringify(doc)}
    wrap.appendChild(frame)
    document.body.appendChild(wrap)
  })()`)
  await new Promise(r => setTimeout(r, Number(args.settle || 1500)))
  const { data } = await cdp.send('Page.captureScreenshot', { format: 'png', clip: { x: 0, y: 0, width: 404, height: 126, scale: 1 } })
  const out = args.out || join(PARITY_DIR, 'widget-electron.png')
  writeFileSync(out, Buffer.from(data, 'base64'))
  if (!args.keep) await cdp.eval(`document.getElementById('p2-widget-ref')?.remove()`)
  console.log(out)
} finally { cdp.close() }
