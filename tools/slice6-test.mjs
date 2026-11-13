// Dilim 6 uçtan uca testi: kütüphane (yama motoru). Rust istemcisi (p2test, sürüm yayınlama
// yetkili) bir "oyun" klasörünü yükler; sürüm yayınlanıp canlıya alınır; istemci kurar,
// güncellemede yalnız değişen parçaları indirir, bozulan dosyayı doğrula/onar ile düzeltir,
// oyunu başlatır (oynama süresi), kaldırır. Parçalamanın eski Electron (cdc.cjs) ile uyumu
// p2-patcher birim testlerinde sabit vektörlerle denetlenir (cdc.rs cross_check_with_js).
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice6-test.mjs
import { spawn } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, readFileSync, existsSync, copyFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { testServer } from './lib.mjs'

const { api: API } = await testServer()
const BASE = 'C:\\p2-pg\\slice6'
const PROFILE = BASE + '\\profile'
const GAMES = BASE + '\\games'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(GAMES, { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
const sha = b => createHash('sha256').update(b).digest('hex')
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

// --- "oyun" klasörü: gerçek bir exe (whoami: hemen çıkar), büyük veri dosyası, alt klasör
function sample(len, seed) { let x = seed >>> 0; const b = Buffer.alloc(len); for (let i = 0; i < len; i++) { x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0; b[i] = x >>> 24 } return b }
function makeBuild(dir, data) {
  mkdirSync(dir + '\\Content', { recursive: true })
  copyFileSync('C:\\Windows\\System32\\whoami.exe', dir + '\\Game.exe')
  writeFileSync(dir + '\\Content\\data.pak', data)
  writeFileSync(dir + '\\Content\\config.ini', '[oyun]\nzorluk=normal\n')
  writeFileSync(dir + '\\Game.pdb', 'hata ayıklama: yüklenmemeli')
}
const v1data = sample(9 * 1024 * 1024 + 777, 42)
makeBuild(BASE + '\\build-v1', v1data)
// v2: dosyanın ortasına bayt girer → içerik tanımlı parçalamada yalnız civarı değişir
const v2data = Buffer.concat([v1data.subarray(0, 4_500_000), Buffer.from('YAMA 1.1 — yeni seviye verisi'), v1data.subarray(4_500_000)])
makeBuild(BASE + '\\build-v2', v2data)

const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const rbase = `/organizations/${org.id}/projects/${project.id}/releases`
const stamp = Date.now().toString().slice(-6)
const channel = 'rusttest'

let lines = [], client
function start(env) {
  lines = []
  client = spawn('C:\\p2-rust-target\\release\\p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_PAGE: 'library', P2_LIBRARY_ROOT: GAMES, ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/kütüphane|sayfa:/.test(l)) console.log('    [istemci] ' + l) } })
}
const stop = async () => { client.kill(); await sleep(700) }
const waitLine = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(150) } return null }
async function publishLive(version) {
  const list = (await call('GET', rbase, null, owner.accessToken)).body
  const r = list.releases.find(x => x.version === version && x.channel === channel)
  if (!r) return null
  await call('POST', `${rbase}/${r.id}/publish`, {}, owner.accessToken)
  await call('POST', `${rbase}/${r.id}/live`, {}, owner.accessToken)
  return r
}

// ============================================================ 1) Yükleme (Rust → sunucu)
console.log('\n[1] Sürüm yükle')
const v1 = `1.0.${stamp}`
start({ P2_AUTOPILOT_UPLOAD: `${BASE}\\build-v1|Game.exe|${channel}|${v1}` })
check('kütüphane açıldı', !!(await waitLine(/sayfa: kütüphane/, 15000)))
check('yükleme tamamlandı', !!(await waitLine(/kütüphane: yükleme tamam/, 60000)))
await stop()
const r1 = await publishLive(v1)
check('sürüm sunucuda (taslak → yayın → canlı)', !!r1)
const manifest1 = (await call('GET', `${rbase}/${r1.id}/manifest`, null, owner.accessToken)).body
check('.pdb dışlandı, 3 dosya', manifest1.files.length === 3 && !manifest1.files.some(f => f.path.endsWith('.pdb')), manifest1.files.map(f => f.path).join(', '))
// (Content/data.pak parçaları sürüm manifestinde; kesimlerin doğruluğu p2-patcher birim testlerinde)

// ============================================================ 2) Kurulum
console.log('\n[2] Kur')
start({ P2_AUTOPILOT_LIBRARY: 'install' })
const done1 = await waitLine(/kütüphane: iş done/, 60000)
check('kurulum bitti', !!done1, done1 || '')
await stop()
const lib = JSON.parse(readFileSync(PROFILE + '\\library\\library.json', 'utf8'))
const info = lib.projects[project.id]
const dir = `${GAMES}\\${info?.folder}`
check('library.json (Electron biçimi)', info?.version === v1 && info?.channel === channel && lib.deviceId)
check('dosyalar bit bit aynı', existsSync(dir + '\\Game.exe') && sha(readFileSync(dir + '\\Content\\data.pak')) === sha(v1data) && readFileSync(dir + '\\Content\\config.ini', 'utf8').includes('zorluk'))
check('.p2/manifest.json yazıldı', existsSync(dir + '\\.p2\\manifest.json') && !existsSync(dir + '\\.p2\\chunks'))

// ============================================================ 3) Güncelleme: yalnız değişen parçalar
console.log('\n[3] Güncelle')
const v2 = `1.1.${stamp}`
start({ P2_AUTOPILOT_UPLOAD: `${BASE}\\build-v2|Game.exe|${channel}|${v2}` })
check('v2 yüklendi', !!(await waitLine(/kütüphane: yükleme tamam/, 60000)))
await stop()
await publishLive(v2)
start({ P2_AUTOPILOT_LIBRARY: 'install' })
const done2 = await waitLine(/kütüphane: iş done/, 60000)
const downloaded = Number(/indirilen=(\d+)/.exec(done2 || '')?.[1] || -1)
check('güncelleme bitti', !!done2)
check('yalnız değişen parçalar indi', downloaded > 0 && downloaded < v2data.length / 2, `${(downloaded / 1048576).toFixed(1)} MB / ${(v2data.length / 1048576).toFixed(1)} MB`)
check('güncel dosya doğru', sha(readFileSync(dir + '\\Content\\data.pak')) === sha(v2data))
await stop()

// ============================================================ 4) Doğrula / onar
console.log('\n[4] Doğrula ve onar')
const broken = readFileSync(dir + '\\Content\\data.pak'); broken[123456] ^= 0xff; writeFileSync(dir + '\\Content\\data.pak', broken)
start({ P2_AUTOPILOT_LIBRARY: 'verify' })
check('bozuk dosya bulundu', !!(await waitLine(/doğrulama 1 dosya bozuk/, 30000)))
check('onarım bitti', !!(await waitLine(/kütüphane: iş done/, 60000)))
check('dosya yeniden sağlam', sha(readFileSync(dir + '\\Content\\data.pak')) === sha(v2data))
await stop()

// ============================================================ 5) Oyna
console.log('\n[5] Oyna')
start({ P2_AUTOPILOT_LIBRARY: 'play' })
check('oyun başlatıldı', !!(await waitLine(/kütüphane: oyun başlatıldı/, 20000)))
check('oyun kapandı (oynama süresi)', !!(await waitLine(/kütüphane: oyun kapandı/, 20000)))
await stop()

// ============================================================ 6) Kaldır
console.log('\n[6] Kaldır')
start({ P2_AUTOPILOT_LIBRARY: 'uninstall' })
check('kaldırıldı', !!(await waitLine(/kütüphane: kaldırıldı/, 20000)))
await sleep(500)
check('klasör silindi', !existsSync(dir))
await stop()

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
