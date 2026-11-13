// Oyun içi overlay paritesi (J ajanı): her senaryo için Electron referansı (overlay-ref.mjs, açık parite
// oturumunda) ve Rust host'unun `p2-overlay --render` çıktısı (oyun/DLL yok) aynı 1280×720 siyah zeminde
// çizilir, karşılaştırılır. Çıktı: C:/p2-pg/parity/overlay/{e,r,diff,side}-<senaryo>.png + özet.
//   node rust/tools/parity/session.mjs start --name ovJ   (arka planda)
//   node rust/tools/parity/overlay-parity.mjs --name ovJ [--scenarios tasks,notes] [--skip-electron]
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, writeFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { compare, writeOutputs } from './lib/diff.mjs'
import { API, PARITY_DIR, RUST_BIN, REPO, parseArgs } from './lib/common.mjs'
import { SCENARIOS } from './overlay-scenarios.mjs'

const args = parseArgs(process.argv.slice(2))
const OUT = join(PARITY_DIR, 'overlay')
mkdirSync(OUT, { recursive: true })
const names = (args.scenarios ? String(args.scenarios).split(',') : Object.keys(SCENARIOS))

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return r.json().catch(() => null)
}
const login = await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })
const org = (await call('GET', '/organizations', null, login.accessToken))[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, login.accessToken))[0]

function rust(name, sc) {
  const out = join(OUT, `r-${name}.png`)
  const s = sc.settings || {}
  const look = { hud: !!s.overlayHud, hudPosition: s.overlayHudPosition || 'top-left', hudDetails: s.overlayHudDetails ?? true, toastCorner: s.overlayToastCorner || 'top-right', opacity: s.overlayOpacity ?? 0.92, scale: 1, reduceMotion: true }
  const init = { type: 'init', pid: 0, api: API, accessToken: login.accessToken, clientVersion: '0.9.9', org: org.id, project: project.id, projectName: project.name, userId: login.user.id, userName: login.user.displayName, hotkeys: [{ id: 1, vk: 0x32, mods: 1 }, { id: 2, vk: 0x7b, mods: 0 }, { id: 3, vk: 0x4d, mods: 1 }], look }
  const steps = [...(sc.visible === false ? [] : ['open']), ...(sc.tasksView ? [`tasks-view:${sc.tasksView}`] : []), ...(sc.rust || []), 'wait:800'].join(';')
  const argv = ['--render', out, '--size', '1280x720', '--steps', steps]
  if (sc.layout) {
    const file = join(OUT, `layout-${name}.json`)
    writeFileSync(file, JSON.stringify({ version: 1, viewport: { w: 1280, h: 720 }, focus: sc.layout.focus ?? null, windows: sc.layout.windows }))
    argv.push('--layout', file)
  }
  return new Promise(resolve => {
    const child = spawn(join(RUST_BIN, 'p2-overlay.exe'), argv, { stdio: ['pipe', 'pipe', 'inherit'] })
    let log = ''
    child.stdout.on('data', d => { log += d })
    child.stdin.write(JSON.stringify(init) + '\n')
    const timer = setTimeout(() => child.kill(), 90000)
    child.on('exit', () => { clearTimeout(timer); resolve({ out, log }) })
  })
}

const rows = []
for (const name of names) {
  const sc = SCENARIOS[name]
  if (!sc) { console.log('bilinmeyen senaryo', name); continue }
  const e = join(OUT, `e-${name}.png`)
  if (!args['skip-electron'] || !existsSync(e)) execFileSync(process.execPath, [join(REPO, 'rust/tools/parity/overlay-ref.mjs'), '--name', args.name || 'ovJ', '--scenario', name, '--out', e], { stdio: 'inherit' })
  const { out, log } = await rust(name, sc)
  if (!existsSync(out)) { console.log(`${name}: Rust çıktısı yok\n${log.slice(-1500)}`); continue }
  if (args.verbose) console.log(log)
  const res = compare(e, out, { threshold: Number(args.threshold || 24) })
  writeOutputs(res, { diffFile: join(OUT, `diff-${name}.png`), sideFile: join(OUT, `side-${name}.png`) })
  rows.push({ name, pct: res.mismatchPct, mae: res.meanAbsError })
  console.log(`${name.padEnd(12)} fark %${res.mismatchPct}  ort.mutlak ${res.meanAbsError}`)
}
writeFileSync(join(OUT, 'summary.json'), JSON.stringify(rows, null, 1))
