// Yalnız Rust görüntüsü (Electron karşılığı otomatik açılamayan "elle" ekranlar için hızlı bakış):
//   node rust/tools/parity/rust-shot.mjs --out C:/p2-pg/parity/x.png [--theme dark] [--login false] [--ready "ekran: giriş"] [--env K=V,K2=V2]
import { RustSession } from './lib/rust.mjs'
import { API, parseArgs, sleep } from './lib/common.mjs'

const args = parseArgs(process.argv.slice(2))
const env = Object.fromEntries(String(args.env || '').split(',').filter(Boolean).map(kv => kv.split('=')))
const rust = await RustSession.start({ theme: args.theme || 'dark', api: API, env, login: args.login !== 'false', settings: true })
try {
  await rust.waitLine(args.ready || 'ekran: çalışma alanı', 30000)
  await sleep(Number(args.settle || 1500))
  console.log(rust.screenshot(args.out || 'C:/p2-pg/parity/rust-shot.png'))
} finally { await rust.stop() }
