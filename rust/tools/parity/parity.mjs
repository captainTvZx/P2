// Electron ↔ Rust görsel parite koşusu.
//   node rust/tools/parity/parity.mjs [--screens login,workspace-chat] [--group auth] [--themes dark,light]
//                                     [--threshold 24] [--settle 1200] [--skip-seed] [--list]
// Çıktı: C:/p2-pg/parity/runs/<zaman>/ (electron/rust/diff/side PNG, report.html, report.json),
//        C:/p2-pg/parity/index.html (son koşu + ekran başına geçmiş), history.json.
// Önkoşul: test sunucusu (P2_API, varsayılan NestJS :4110) + test PostgreSQL; Rust istemcisi derli
// (C:/p2-rust-target/release/p2-client.exe, P2_RUST_BIN ile değişir).
import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { ElectronSession, startVite } from './lib/electron.mjs'
import { RustSession } from './lib/rust.mjs'
import { compare, writeOutputs } from './lib/diff.mjs'
import { loadCatalog, electronSteps, rustEnv } from './lib/catalog.mjs'
import { seed } from './lib/seed.mjs'
import { seedMedia } from './lib/seed-media.mjs' // L ajanı: medya/bağlantı/forum kanalları
import { CallBot, clearRooms } from './lib/bot.mjs'
import { PARITY_DIR, API, parseArgs, sleep, log } from './lib/common.mjs'
import { renderReport, renderIndex } from './lib/report.mjs'

const args = parseArgs(process.argv.slice(2))
const catalog = loadCatalog()
if (args.list) {
  for (const s of catalog.screens) console.log(`${s.id.padEnd(28)} ${String(s.group).padEnd(10)} ${s.manual ? '[elle] ' : ''}${s.rust?.status === 'missing' ? '[Rust kancası yok] ' : ''}${s.verified === false ? '[doğrulanmadı] ' : ''}${s.title}`)
  process.exit(0)
}
let screens = catalog.screens.filter(s => !s.manual)
if (args.screens) { const want = String(args.screens).split(','); screens = want.map(id => catalog.screens.find(s => s.id === id) || (() => { throw new Error('ekran yok: ' + id) })()) }
if (args.group) screens = screens.filter(s => String(args.group).split(',').includes(s.group))
const themes = String(args.themes || 'dark').split(',')
const threshold = Number(args.threshold || catalog.defaults?.threshold || 24)
const settle = Number(args.settle || catalog.defaults?.settle || 1200)

const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
const RUN = join(PARITY_DIR, 'runs', stamp)
mkdirSync(RUN, { recursive: true })
log(`koşu: ${RUN} — ${screens.length} ekran × ${themes.join('/')}`)

if (!args['skip-seed']) { await seed(); await seedMedia() }
const vite = await startVite()
const results = []
try {
  for (const theme of themes) {
    let electron = null
    const ensureElectron = async () => {
      if (!electron || electron.child.exitCode !== null) electron = await ElectronSession.start({ theme, api: API, renderer: vite.url })
      return electron
    }
    for (const screen of screens) {
      const id = `${screen.id}-${theme}`
      const entry = { id: screen.id, theme, title: screen.title, group: screen.group, files: {} }
      results.push(entry)
      log(`— ${id}`)
      // I ajanı — arama ekranları: `electron.fakeMedia` ekran başına ayrı, aygıtsız sahte medyalı Electron
      // (lib/electron.mjs + fake-media-preload.cjs; gerçek mikrofon/hoparlör açılmaz) ve `bot` aygıtsız arama
      // botu (lib/bot.mjs). Bot her taraf için yeniden başlar; `bot.onStep` ise adım {"startBot":true} ile.
      let bot = null
      const startBot = async () => { if (screen.bot && !bot) bot = await CallBot.start(screen.bot.env) }
      const stopBot = async () => { await bot?.stop(); bot = null }
      // Electron
      let fakeE = null
      try {
        if (screen.group === 'call') await clearRooms()
        if (screen.bot && !screen.bot.onStep) await startBot()
        let e
        if (screen.electron?.fakeMedia) {
          e = fakeE = await ElectronSession.start({ theme, api: API, renderer: vite.url, fakeMedia: true })
        } else {
          e = await ensureElectron()
          await e.cdp.eval('sessionStorage.clear()').catch(() => {})
          await e.reload()
        }
        e.onStartBot = startBot
        await e.mouse('mouseMoved', 1279, 799)
        await e.run(electronSteps(catalog, screen))
        await sleep(screen.settle ?? settle)
        entry.files.electron = await e.screenshot(join(RUN, `${id}-electron.png`))
      } catch (err) {
        entry.electronError = err.message
        log(`  Electron hata: ${err.message.split('\n')[0]}`)
        await (fakeE || electron)?.screenshot(join(RUN, `${id}-electron-hata.png`)).catch(() => {})
        if (electron && !fakeE) { await electron.stop(); electron = null }
      } finally {
        await fakeE?.stop()
        await stopBot()
      }
      // Rust
      if (screen.rust?.status === 'missing') entry.rustError = 'Rust tarafında bu ekrana otomatik pilot kancası yok (catalog: rust.status=missing)'
      else {
        let rust = null
        try {
          if (screen.group === 'call') await clearRooms()
          if (screen.bot && !screen.bot.onStep) await startBot()
          rust = await RustSession.start({ theme, api: API, env: rustEnv(catalog, screen), login: screen.rust?.login !== false, settings: screen.rust?.settings !== false, apiOverride: screen.rust?.api })
          rust.onStartBot = startBot
          await rust.waitLine(screen.rust?.ready || 'ekran: çalışma alanı', screen.rust?.timeout || 25000)
          await rust.run(screen.rust?.steps || [])
          await sleep(screen.rust?.settle ?? screen.settle ?? settle)
          entry.files.rust = rust.screenshot(join(RUN, `${id}-rust.png`))
        } catch (err) {
          entry.rustError = err.message
          log(`  Rust hata: ${err.message.split('\n')[0]}`)
        } finally { await rust?.stop(); await stopBot() }
      }
      if (entry.files.electron && entry.files.rust) {
        const r = compare(entry.files.electron, entry.files.rust, { threshold, regions: screen.regions || catalog.regions?.[screen.layout] || [] })
        entry.files.diff = join(RUN, `${id}-diff.png`)
        entry.files.side = join(RUN, `${id}-side.png`)
        writeOutputs(r, { diffFile: entry.files.diff, sideFile: entry.files.side })
        Object.assign(entry, { mismatchPct: r.mismatchPct, meanAbsError: r.meanAbsError, sizeA: r.sizeA, sizeB: r.sizeB, regions: r.regions, worstTiles: r.worstTiles })
        log(`  uyumsuzluk %${r.mismatchPct} (ort. fark ${r.meanAbsError})`)
      }
    }
    await electron?.stop()
  }
} finally { vite.stop() }

const report = { run: stamp, api: API, threshold, settle, themes, results: results.map(({ files, ...rest }) => ({ ...rest, files: Object.fromEntries(Object.entries(files).map(([k, v]) => [k, v.split(/[\\/]/).pop()])) })) }
writeFileSync(join(RUN, 'report.json'), JSON.stringify(report, null, 2))
writeFileSync(join(RUN, 'report.html'), renderReport(report))
// Geçmiş: ekran+tema başına zaman serisi
const historyFile = join(PARITY_DIR, 'history.json')
const history = existsSync(historyFile) ? JSON.parse(readFileSync(historyFile, 'utf8')) : {}
for (const r of report.results) if (r.mismatchPct !== undefined) (history[`${r.id}-${r.theme}`] ||= []).push({ run: stamp, mismatchPct: r.mismatchPct })
writeFileSync(historyFile, JSON.stringify(history, null, 2))
writeFileSync(join(PARITY_DIR, 'index.html'), renderIndex(history, stamp))
log(`rapor: ${join(RUN, 'report.html')}`)
const failed = report.results.filter(r => r.electronError || r.rustError)
if (failed.length) log(`${failed.length} ekranda hata (rapora bakın)`)
