// Arama balonu — Rust görüntüsü (I ajanı). widget-ref.mjs'in (Electron) karşılığı.
// Aygıtsız bot ses odasına girer, istemci (P2_NO_AUDIO_DEVICE) katılır, ana pencere küçültülür ve
// balon penceresi yakalanır. Girdi yalnız kendi pencerelerimize (p2-wincap PostMessage).
//   node rust/tools/parity/widget-shot.mjs --out C:/p2-pg/parity/widget-rust.png [--muted]
import { execFileSync } from 'node:child_process'
import { join } from 'node:path'
import { RustSession } from './lib/rust.mjs'
import { CallBot } from './lib/bot.mjs'
import { API, PARITY_DIR, RUST_BIN, parseArgs, sleep } from './lib/common.mjs'

const args = parseArgs(process.argv.slice(2))
const wincap = (...a) => execFileSync(join(RUST_BIN, 'p2-wincap.exe'), a.map(String), { timeout: 15000 }).toString()
const bot = await CallBot.start({ P2_CALL_CHANNEL: 'ekip-odası', P2_BOT_SILENT: '1', P2_BOT_NO_SCREEN: '1' })
const rust = await RustSession.start({ theme: args.theme || 'dark', api: API, env: { P2_AUTOPILOT_VOICE: 'ekip-odası', P2_TRACE_WIDGET: '1', ...(args.muted ? { P2_AUTOPILOT_MUTE_AFTER: '1' } : {}) } })
try {
  await rust.waitLine('arama: eş [0-9a-f]{8} connected', 40000)
  if (args.muted) await rust.waitLine('arama: mikrofon kapalı', 10000)
  const pid = rust.child.pid
  const main = wincap('--list', pid).split(/\r?\n/).filter(Boolean).map(l => { const [hwnd, title, rect] = l.split('\t'); const [x, y, w, h] = (rect || '').split(',').map(Number); return { hwnd, title, w, h } }).filter(w => !/balon|kaplama/.test(w.title)).sort((a, b) => b.w * b.h - a.w * a.h)[0]
  wincap('--minimize', main.hwnd, pid)
  await rust.waitLine('balon: göründü', 8000)
  await sleep(Number(args.settle || 1800))
  const out = args.out || join(PARITY_DIR, 'widget-rust.png')
  wincap('--title', 'arama balonu', pid, out)
  console.log(out)
} finally {
  await rust.stop()
  await bot.stop()
}
