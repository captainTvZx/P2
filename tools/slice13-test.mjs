// Dilim 13 uçtan uca testi: açılış penceresi + istemci güncelleyicisi, oyun başlatma kartı,
// overlay görünüm ayarları (ölçek, saydamlık, gösterge yeri/ayrıntısı, hareketi azalt) ve
// kütüphane otomatik güncellemesi (açılış denetimi, release:live, oyun açıkken montaj bekler,
// ayar kapalıyken indirmez, hız sınırı).
//
// Güncelleyici bölümü kendi Rust sunucusunu (:4151) sahte bir güncelleme klasörüyle açar; istemci
// P2_UPDATER_TEST kipinde çalışır: kurucu ASLA çalıştırılmaz (sahte başlatıcı iz düşer, işaret
// dosyası yazar). Masaüstüne girdi yok: overlay girdisi yalnız oyun penceresine PostMessage.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice13-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, readFileSync, writeFileSync, existsSync, copyFileSync, readdirSync } from 'node:fs'
import { createHash, randomBytes } from 'node:crypto'
import { testServer, startRust } from './lib.mjs'

const { api: API } = await testServer()
const BIN = 'C:/p2-rust-target/release/'
const BASE = 'C:/p2-pg/slice13'
const OUT = BASE + '/out'
rmSync(BASE, { recursive: true, force: true })
for (const d of [OUT, BASE + '/updates/client', BASE + '/server', BASE + '/games', BASE + '/shots', BASE + '/build1', BASE + '/build2']) mkdirSync(d, { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(base, method, path, body, token) {
  const r = await fetch(base + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

let lines = [], client, exited = null
function start(profile, server, env, show = /açılış|güncelleyici|ayarlar: güncelleme|başlatma kartı|overlay(\[\d+\])?: (görünüm|sürüm|indirme|oyun hazır|araç)|kütüphane: (otomatik|iş|yeni sürüm|oyun)|olay: (release|update)|bildirim:/) {
  lines = []; exited = null
  client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: profile, P2_AUTOPILOT_SERVER: server, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', ...env } })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (show.test(l)) console.log('    [istemci] ' + l.slice(0, 230)) } })
  client.on('exit', code => { exited = code })
}
const stop = async () => { if (exited === null) { client.kill(); await sleep(700) } }
const wait = async (re, ms = 20000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const waitExit = async ms => { const end = Date.now() + ms; while (Date.now() < end && exited === null) await sleep(150); return exited }
const capture = (args, name) => { try { execFileSync(BIN + 'p2-wincap.exe', [...args.map(String), `${OUT}/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
const sha512 = buf => createHash('sha512').update(buf).digest('base64')
// Sahte istemci sürümü yayını: <klasör>/client/latest.yml (+ kökteki latest.yml: sunucu 5 sn'de bir
// izler, değişince bağlı herkese update:published yayar).
function publish(version, installer) {
  const yml = installer
    ? `version: ${version}\nfiles:\n  - url: ${installer.name}\n    sha512: ${installer.sha}\n    size: ${installer.size}\npath: ${installer.name}\nsha512: ${installer.sha}\nreleaseDate: '${new Date().toISOString()}'\n`
    : `version: ${version}\nfiles: []\nreleaseDate: '${new Date().toISOString()}'\n`
  writeFileSync(BASE + '/updates/client/latest.yml', yml)
  writeFileSync(BASE + '/updates/latest.yml', `version: ${version}\n`)
}

// ============================================================ 1) Güncelleyici + açılış penceresi
console.log('\n[1] İstemci güncelleyicisi ve açılış penceresi (Rust sunucusu :4151, sahte yayın)')
const UPROFILE = BASE + '/profile-upd'
publish('0.9.9')
const exeBytes = randomBytes(6 * 1024 * 1024)
const installer = { name: 'P2-Client-Setup-9.9.9.exe', size: exeBytes.length, sha: sha512(exeBytes) }
writeFileSync(BASE + '/updates/client/' + installer.name, exeBytes)
process.env.P2_UPDATE_DIR = BASE + '/updates'
const srv = await startRust(4151, { cwd: BASE + '/server' })
const UAPI = srv.base
try {
  // a) İlk açılış: kayıtlı kaynak yok → açılış penceresi yok; bağlanınca kaynak kaydedilir.
  start(UPROFILE, UAPI, { P2_UPDATER_TEST: '1', P2_AUTOPILOT_SETTINGS: 'app' })
  check('güncelleyici test kipinde, sürüm 0.9.9', !!(await wait(/güncelleyici: sürüm 0\.9\.9 \(test kipi\)/)))
  check('ilk açılışta açılış penceresi yok (kaynak kayıtlı değil)', !!(await wait(/açılış: kayıtlı güncelleme kaynağı yok/)))
  check('bağlanınca kaynak yapılandırıldı', !!(await wait(new RegExp('güncelleyici: kaynak ' + UAPI.replace(/[.:/]/g, '\\$&') + '/updates/client/'))))
  check('update-source.json profil klasöründe', existsSync(UPROFILE + '/update-source.json') && JSON.parse(readFileSync(UPROFILE + '/update-source.json', 'utf8')).base === UAPI)
  check('ayarlar > uygulama açıldı', !!(await wait(/ayarlar: açıldı \(App\)/)))
  await sleep(2500)
  capture([client.pid], '1a-ayarlar-guncel')
  // Yeni sürüm yayınlanır: sunucu update:published yayar → denetim → indirme → hazır.
  publish('9.9.9', installer)
  check('update:published geldi', !!(await wait(/olay: update:published 9\.9\.9/, 15000)))
  check('indirildi, durum hazır', !!(await wait(/güncelleyici: durum Ready sürüm=9\.9\.9/, 30000)))
  await sleep(800)
  capture([client.pid], '1b-ayarlar-hazir')
  check('hazırken 20 sn sonra kendiliğinden kurulum (sahte başlatıcı)', !!(await wait(/güncelleyici: \[test\] kurucu çalıştırılmadı: .*P2-Client-Setup-9\.9\.9\.exe --updated \/S --force-run/, 35000)))
  check('kurulumdan sonra uygulama kapandı', (await waitExit(8000)) !== null)
  check('işaret dosyası (gerçek kurucu çalışmadı)', existsSync(UPROFILE + '/update-install.txt'))
  check('kurucu profil önbelleğinde (LOCALAPPDATA değil)', existsSync(UPROFILE + '/update-cache'))
  await stop()

  // b) Açılış turu: güncelleme var → açılış penceresi denetler, indirir, kurar; uygulama açılmaz.
  rmSync(UPROFILE + '/update-install.txt', { force: true })
  rmSync(UPROFILE + '/update-cache', { recursive: true, force: true })
  start(UPROFILE, UAPI, { P2_UPDATER_TEST: '1' })
  check('açılış penceresi gösterildi', !!(await wait(/açılış: pencere gösterildi/)))
  check('"Güncellemeler denetleniyor…"', !!(await wait(/güncelleyici: durum [A-Za-z]+ .*metin="Güncellemeler denetleniyor…"/)))
  if (await wait(/açılış: Güncelleme indiriliyor/, 15000)) { capture(['--title', 'P2', client.pid], '1c-acilis-indiriliyor'); check('"Güncelleme indiriliyor"', true) } else check('"Güncelleme indiriliyor"', false)
  check('"Güncelleme kuruluyor, uygulama yeniden başlayacak…"', !!(await wait(/açılış: Güncelleme kuruluyor, uygulama yeniden başlayacak…/, 30000)))
  check('açılış turu: kurulum (sahte başlatıcı)', !!(await wait(/kurucu çalıştırılmadı/, 10000)))
  check('ana pencere hiç açılmadı', !lines.some(l => /ekran: |açılış: pencere kapandı/.test(l)))
  check('süreç kapandı', (await waitExit(8000)) !== null)
  await stop()

  // c) Güncelleme yok: açılış penceresi kısa süre görünür, sonra ana pencere açılır.
  publish('0.9.9')
  rmSync(UPROFILE + '/update-install.txt', { force: true })
  start(UPROFILE, UAPI, { P2_UPDATER_TEST: '1', P2_SPLASH_HOLD_MS: '2500' })
  check('açılış penceresi (güncelleme yok)', !!(await wait(/açılış: güncelleme turu bitti \(None\)/, 20000)))
  await sleep(700)
  capture([client.pid], '1d-acilis')
  check('açılış penceresi kapandı, ana pencere açıldı', !!(await wait(/açılış: pencere kapandı/, 8000)) && !!(await wait(/çalışma alanı/, 15000)))
  check('kurulum yapılmadı', !existsSync(UPROFILE + '/update-install.txt'))
  await stop()

  // d) Geliştirme derlemesi (test kipi yok): güncelleyici kapalı, açılış penceresi yok.
  start(UPROFILE, UAPI, {})
  check('geliştirme derlemesinde güncelleyici kapalı', !!(await wait(/güncelleyici: sürüm 0\.9\.9 \(geliştirme — kapalı\)/)) && !!(await wait(/güncelleyici: yapılandırılmadı \(dev\)/)))
  check('geliştirme derlemesinde açılış penceresi yok', !lines.some(l => /açılış: pencere gösterildi/.test(l)))
  await stop()
} finally { srv.stop() }

// ============================================================ 2) Kütüphane: otomatik güncelleme, başlatma kartı, overlay görünümü
console.log('\n[2] Kütüphane otomatik güncellemesi, başlatma kartı, overlay görünümü')
const PROFILE = BASE + '/profile'
const GAMES = BASE + '/games'
const owner = (await call(API, 'POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const org = (await call(API, 'GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call(API, 'GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const root = `/organizations/${org.id}/projects/${project.id}`
const stamp = Date.now().toString().slice(-6)
const channel = 'rusttest'
const v1 = `13.1.${stamp}`, v2 = `13.2.${stamp}`
copyFileSync(BIN + 'p2-d3dtest.exe', BASE + '/build1/Game.exe')
copyFileSync(BIN + 'p2-d3dtest.exe', BASE + '/build2/Game.exe')
writeFileSync(BASE + '/build2/data.bin', randomBytes(3 * 1024 * 1024))
const libEnv = { P2_AUTOPILOT_PAGE: 'library', P2_LIBRARY_ROOT: GAMES, P2_SHOT_DIR: BASE + '/shots' }
const settingsFile = PROFILE + '/settings.json'
const editSettings = fn => { const s = JSON.parse(readFileSync(settingsFile, 'utf8').replace(/^\uFEFF/, '')); fn(s); writeFileSync(settingsFile, JSON.stringify(s, null, 2)) }
const installedVersion = () => { try { const lib = JSON.parse(readFileSync(PROFILE + '/library/library.json', 'utf8')); return String(lib.projects?.[project.id]?.version ?? '') } catch { return '' } }

for (const [dir, version] of [['build1', v1], ['build2', v2]]) {
  start(PROFILE, API, { ...libEnv, P2_AUTOPILOT_UPLOAD: `${BASE}/${dir}|Game.exe|${channel}|${version}` })
  check(`sürüm ${version} yüklendi`, !!(await wait(/kütüphane: yükleme (tamam|ready)/, 90000)))
  await stop()
}
const rels = (await call(API, 'GET', `${root}/releases`, null, owner.accessToken)).body.releases
const r1 = rels.find(r => r.version === v1), r2 = rels.find(r => r.version === v2)
await call(API, 'POST', `${root}/releases/${r1.id}/publish`, {}, owner.accessToken)
await call(API, 'POST', `${root}/releases/${r1.id}/live`, {}, owner.accessToken)
start(PROFILE, API, { ...libEnv, P2_AUTOPILOT_LIBRARY: 'install' })
check('v1 kuruldu', !!(await wait(/kütüphane: iş done/, 90000)))
await stop()
check('library.json v1', installedVersion() === v1, installedVersion())
await call(API, 'POST', `${root}/releases/${r2.id}/publish`, {}, owner.accessToken)

// Görünüm: ölçek 1.25, saydamlık .6, gösterge sağ üstte, ayrıntı kapalı, hareketi azalt; 5 MB/s
editSettings(s => { s.overlay = { ...s.overlay, hud: true, scale: 1.25, opacity: 0.6, hudPosition: 'top-right', hudDetails: false, reduceMotion: true, toastCorner: 'top-right' }; s.library = { ...s.library, bandwidthMbps: 5, autoDownload: true } })
const steps = ['state', 'open', 'tool:notes', 'wait:2500', 'state', 'close', 'wait:1500'].join(';')
start(PROFILE, API, { ...libEnv, P2_AUTOPILOT_LIBRARY: 'play', P2_OVERLAY_AUTOPILOT: steps, P2_AUTOPILOT_OVERLAY_LOOK: '9000|scale=1,opacity=0.9,hudpos=top-center,details=on,motion=full' })
check('başlatma kartı açıldı (overlay bekler)', !!(await wait(/başlatma kartı: açıldı «.+» \(overlay hazır olunca kapanır\)/, 30000)))
await sleep(500)
capture(['--title', 'Başlatılıyor', client.pid], '2a-baslatma-karti')
const injected = await wait(/overlay\[(\d+)\]: .*kök süreç \d+ enjekte edildi/, 20000)
const gamePid = Number(/kök süreç (\d+)/.exec(injected || '')?.[1] || 0)
check('overlay hazır → kart kapandı', !!(await wait(/başlatma kartı: kapandı \(overlay hazır\)/, 30000)))
const look1 = await wait(/overlay\[\d+\]: görünüm: ölçek=1\.25 saydamlık=0\.60 hud=true yer=top-right ayrıntı=false köşe=top-right hareket-azalt=true/, 10000)
check('host görünümü init ile aldı', !!look1, look1?.replace(/.*görünüm: /, '') || '')
check('ölçekli yüzey (mantıksal = fiziksel / 1.25)', !!(await wait(/overlay\[\d+\]: durum: .*ölçek=1\.25 mantıksal=\d+x\d+/, 10000)))
check('ölçekli girdi: 2. düğme Notlar\'ı açtı', !!(await wait(/overlay\[\d+\]: araç: Notes/, 15000)))
await sleep(1500)
capture([gamePid], '2b-hud-sag-ust')
// Oyun açıkken v2 canlıya alınır → indirme başlar, montaj oyun kapanana dek bekler.
await call(API, 'POST', `${root}/releases/${r2.id}/live`, {}, owner.accessToken)
check('release:live olayı', !!(await wait(new RegExp('olay: release:live .*' + v2.replace(/\./g, '\\.')), 10000)))
check('oyun içi sürüm kartı', !!(await wait(new RegExp('overlay: sürüm bildirimi .*' + v2.replace(/\./g, '\\.') + ' \\(Sürüm canlıya alındı\\.\\)'), 5000)))
const startLine = await wait(/kütüphane: otomatik güncelleme başladı .*\(hız sınırı 5 MB\/s, oyun açık — montaj kapanınca\)/, 10000)
check('otomatik güncelleme başladı (5 MB/s, oyun açık)', !!startLine)
check('montaj oyunun kapanmasını bekliyor', !!(await wait(/kütüphane: iş waiting-for-exit/, 30000)))
const look2 = await wait(/overlay\[\d+\]: görünüm: ölçek=1\.00 saydamlık=0\.90 hud=true yer=top-center ayrıntı=true .*hareket-azalt=false/, 15000)
check('canlı görünüm değişikliği host\'a ulaştı', !!look2)
check('ölçek değişince yüzey yeniden kuruldu', !!(await wait(/overlay\[\d+\]: overlay yüzeyi \d+x\d+ \(ölçek 1\.00/, 5000)))
await sleep(1500)
capture([gamePid], '2c-hud-ust-orta-ayrintili')
check('v2 henüz kurulmadı (oyun açık)', installedVersion() === v1, installedVersion())
try { execFileSync('taskkill', ['/PID', String(gamePid), '/F']) } catch {}
check('oyun kapandı → kurulum bitti', !!(await wait(/kütüphane: otomatik güncelleme bitti \(done\)/, 60000)))
check('library.json v2', installedVersion() === v2, installedVersion())
await stop()

// Açılış denetimi: istemci kapalıyken v1 yeniden canlıya alınır → açılışta otomatik iner.
await call(API, 'POST', `${root}/releases/${r1.id}/live`, {}, owner.accessToken)
start(PROFILE, API, {})
check('açılış denetimi (1 kurulu)', !!(await wait(/kütüphane: otomatik güncelleme denetimi \(1 kurulu\)/, 30000)))
check('açılışta canlı sürüme dönüldü', !!(await wait(/kütüphane: otomatik güncelleme bitti \(done\)/, 60000)) && installedVersion() === v1, installedVersion())
// Ayar kapalıyken: canlı sürüm değişir ama indirilmez.
await stop()
editSettings(s => { s.library.autoDownload = false })
start(PROFILE, API, {})
await wait(/olay: gerçek zamanlı bağlandı/, 20000)
await sleep(1500)
await call(API, 'POST', `${root}/releases/${r2.id}/live`, {}, owner.accessToken)
check('ayar kapalı: indirilmedi', !!(await wait(new RegExp('kütüphane: yeni sürüm ' + v2.replace(/\./g, '\\.') + ' var, otomatik indirme kapalı'), 15000)))
await sleep(1500)
check('library.json hâlâ v1', installedVersion() === v1)
await stop()

// Düz başlatma (overlay yok): kart 4 sn sonra kapanır + "başlatıldı" bildirimi.
start(PROFILE, API, { ...libEnv, P2_AUTOPILOT_LIBRARY: 'play', P2_OVERLAY_DISABLE: '1', P2_FORCE_NOTIFY: '1' })
check('düz başlatmada kart', !!(await wait(/başlatma kartı: açıldı «.+» \(düz başlatma\)/, 30000)))
check('4 sn sonra kapandı', !!(await wait(/başlatma kartı: kapandı \(düz başlatma \(4 sn\)\)/, 8000)))
check('"… başlatıldı" bildirimi', !!(await wait(/bildirim: .+ başlatıldı — Bu oyunda P2 overlay kullanılamıyor/, 3000)))
const plainGame = Number(/kök süreç (\d+)/.exec(lines.find(l => /kök süreç/.test(l)) || '')?.[1] || 0)
await sleep(500)
try { execFileSync('taskkill', ['/IM', 'Game.exe', '/F']) } catch {}
await stop()
void plainGame

console.log(`\ngörüntüler: ${OUT} (${readdirSync(OUT).join(', ')})`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
