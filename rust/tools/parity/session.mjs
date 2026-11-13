// Etkileşimli Electron oturumu (parite ajanları için keşif aracı).
//   node rust/tools/parity/session.mjs start [--theme dark|light] [--name s1]     (arka planda açık kalır)
//   node rust/tools/parity/session.mjs eval  "<js ifadesi>"   [--name s1]          sonucu JSON yazar
//   node rust/tools/parity/session.mjs steps '<json adım dizisi>' [--name s1]      katalog adımları
//   node rust/tools/parity/session.mjs screen <ekran-id> [--name s1]              katalogdaki ekrana git
//   node rust/tools/parity/session.mjs shot  <çıktı.png> [--name s1]
//   node rust/tools/parity/session.mjs stop  [--name s1]
// Durum dosyası: C:/p2-pg/parity/session-<ad>.json (cdp portu, süreç kimlikleri).
import { readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { Cdp, waitTarget } from './lib/cdp.mjs'
import { ElectronSession, startVite } from './lib/electron.mjs'
import { PARITY_DIR, API, parseArgs, sleep, log } from './lib/common.mjs'
import { loadCatalog, electronSteps } from './lib/catalog.mjs'

const args = parseArgs(process.argv.slice(2))
const [cmd, ...rest] = args._
const name = args.name || 'default'
const stateFile = join(PARITY_DIR, `session-${name}.json`)

async function attach() {
  if (!existsSync(stateFile)) throw new Error('oturum yok: önce "session.mjs start"')
  const state = JSON.parse(readFileSync(stateFile, 'utf8'))
  const target = await waitTarget(state.cdpPort, t => t.url.startsWith(state.renderer), 5000)
  const s = Object.create(ElectronSession.prototype)
  s.cdp = await Cdp.connect(target.webSocketDebuggerUrl)
  s.cdpPort = state.cdpPort
  return { s, state }
}

if (cmd === 'start') {
  const vite = await startVite()
  // --fake-media: arama ekranları için aygıtsız sahte medya (electron-main.cjs / fake-media-preload.cjs).
  const s = await ElectronSession.start({ theme: args.theme || 'dark', api: args.api || API, renderer: vite.url, fakeMedia: Boolean(args['fake-media']) })
  writeFileSync(stateFile, JSON.stringify({ cdpPort: s.cdpPort, renderer: vite.url, electronPid: s.child.pid, vitePid: vite.child?.pid, nodePid: process.pid, profile: s.profile }, null, 2))
  log(`oturum hazır: ${stateFile} (cdp ${s.cdpPort})`)
  if (args.login !== 'false') { await s.login(); log('giriş yapıldı') }
  // Süreç açık kalır; "stop" ya da Electron kapanınca çıkar.
  s.child.on('exit', () => { vite.stop(); rmSync(stateFile, { force: true }); process.exit(0) })
  process.on('SIGTERM', async () => { await s.stop(); vite.stop(); process.exit(0) })
} else if (cmd === 'stop') {
  if (existsSync(stateFile)) {
    const state = JSON.parse(readFileSync(stateFile, 'utf8'))
    for (const pid of [state.electronPid, state.vitePid, state.nodePid].filter(Boolean)) spawn('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true })
    await sleep(800)
    rmSync(stateFile, { force: true })
  }
  log('durduruldu')
} else {
  const { s } = await attach()
  try {
    if (cmd === 'eval') console.log(JSON.stringify(await s.cdp.eval(rest.join(' ')), null, 2))
    else if (cmd === 'steps') await s.run(JSON.parse(rest.join(' ')))
    else if (cmd === 'screen') { const cat = loadCatalog(); const screen = cat.screens.find(x => x.id === rest[0]); if (!screen) throw new Error('ekran yok: ' + rest[0]); await s.run(electronSteps(cat, screen)) }
    else if (cmd === 'shot') { await s.screenshot(rest[0]); console.log(rest[0]) }
    else throw new Error('komut: start | eval | steps | screen | shot | stop')
  } finally { s.cdp.close() }
}
