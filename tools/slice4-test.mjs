// Dilim 4 uçtan uca testi: sesli arama. Karşı taraf Spike 2'nin Rust arama botu
// (p2-callbot; Electron 0.9.9 ile birlikte çalıştığı doğrulanmış protokol): 440 Hz bip
// gönderir, gelen sesi dBFS olarak ölçer. Rust istemcisi gerçek mikrofon/hoparlör
// KULLANMAZ: P2_TEST_TONE (sentetik 523 Hz) + P2_SILENT_AUDIO.
// UYARI: ses motorunu (WebRTC/ses yolu) gerçekten çalıştırır; geliştirme makinesinde Windows Audio'yu
// kilitleyebilir. Yalnız ayrı bir test makinesinde çalıştır; ses mantığı için: cargo test -p p2-media.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice4-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import { testServer, TARGET } from './lib.mjs'

const { api: API } = await testServer()
const OUT = 'C:\\p2-pg\\slice4'
const PROFILE = 'C:\\p2-pg\\rust-client-profile-s4'
mkdirSync(OUT, { recursive: true })
rmSync(PROFILE, { recursive: true, force: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }

function run(exe, env, tag) {
  const lines = []
  const proc = spawn(exe, [], { env: { ...process.env, ...env } })
  const take = d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (!/olay: durum|olay: activity|ses: mikrofon/.test(l) || process.env.VERBOSE) console.log(`    [${tag}] ${l}`) } }
  proc.stdout.on('data', take)
  proc.stderr.on('data', take)
  const wait = async (re, ms = 10000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(100) } return null }
  return { proc, lines, wait, kill: async () => { proc.kill(); await sleep(500) } }
}
const client = env => run(TARGET + '/p2-client.exe', { P2_PROFILE_DIR: PROFILE, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_TEST_TONE: '1', P2_SILENT_AUDIO: '1', P2_TRACE_AUDIO: '1', ...env }, 'istemci')
const bot = env => run(TARGET + '/p2-callbot.exe', { P2_API: API, P2_BOT_USER: 'rustbot', P2_BOT_PASS: 'Parola123', ...env }, 'bot')
const shot = (pid, name) => { try { execFileSync(TARGET + '/p2-wincap.exe', [String(pid), `${OUT}\\${name}.png`]); console.log(`    görüntü: ${OUT}\\${name}.png`) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim()) } }
// İstemcinin ölçtüğü gelen ses (botun bipi) ve botun ölçtüğü bizim sesimiz.
const incomingDb = lines => Math.max(-120, ...lines.map(l => /ses: mikrofon .* gelen \[[0-9a-f]+=(-?\d+)\]/.exec(l)).filter(Boolean).map(m => Number(m[1])))
const botHeard = lines => lines.map(l => /ses: (\d+) kare\/sn, tepe (-?[\d.]+) dBFS/.exec(l)).filter(Boolean).map(m => ({ fps: Number(m[1]), db: Number(m[2]) }))

// ============================================================ 1) Bot arar, istemci kabul eder
console.log('\n[1] Gelen bireysel arama (bot → istemci)')
let c = client({ P2_AUTOPILOT_ACCEPT: '1', P2_AUTOPILOT_MUTE_AFTER: '9', P2_AUTOPILOT_LEAVE_AFTER: '18' })
check('istemci bağlandı', !!(await c.wait(/olay: gerçek zamanlı bağlandı/)))
let b = bot({ P2_CALL_TARGET: 'p2test' })
check('gelen arama göründü', !!(await c.wait(/arama: gelen arama Rust Bot/)))
check('kabul edildi ve katıldı', !!(await c.wait(/arama: katıldın \(direct, 2 kişi\)/)))
check('WebRTC eşi bağlandı', !!(await c.wait(/arama: eş [0-9a-f]+ connected/, 15000)))
await sleep(4000)
check('istemci botun sesini alıyor', incomingDb(c.lines) > -40, `en yüksek ${incomingDb(c.lines)} dBFS`)
const heard = botHeard(b.lines)
check('bot istemcinin sesini alıyor', heard.some(h => h.fps > 50 && h.db > -30), JSON.stringify(heard.slice(-2)))
shot(c.proc.pid, '1-bireysel-arama')
check('susturuldu', !!(await c.wait(/arama: mikrofon kapalı/, 8000)))
// Bot 3 sn'lik pencerelerle ölçer: susturmadan sonraki ilk tam pencere sessiz olmalı.
const before = b.lines.length
await sleep(6500)
const afterMute = botHeard(b.lines.slice(before))
check('susturunca bota ses gitmiyor', afterMute.length > 0 && afterMute[afterMute.length - 1].db < -60, JSON.stringify(afterMute))
check('istemci ayrıldı', !!(await c.wait(/arama: ayrıldın/, 8000)))
check('bot aramanın bittiğini gördü', !!(await b.wait(/arama bitti/, 5000)))
await b.kill(); await c.kill()

// ============================================================ 2) İstemci arar, bot kabul eder
console.log('\n[2] Giden bireysel arama (istemci → bot)')
b = bot({})
await b.wait(/gelen arama bekleniyor/)
c = client({ P2_AUTOPILOT_CALL: 'rustbot' })
check('arama başlatıldı', !!(await c.wait(/arama: bireysel arama başlatıldı/, 20000)))
check('bot aramayı aldı', !!(await b.wait(/gelen arama: .*kabul ediliyor/)))
check('katılım (2 kişi)', !!(await c.wait(/olay: arama durumu direct \(2 kişi\)/, 10000)))
check('WebRTC eşi bağlandı', !!(await c.wait(/arama: eş [0-9a-f]+ connected/, 15000)))
await sleep(4000)
check('iki yönde ses', incomingDb(c.lines) > -40 && botHeard(b.lines).some(h => h.db > -30), `istemci ${incomingDb(c.lines)} dBFS, bot ${JSON.stringify(botHeard(b.lines).slice(-1))}`)
await c.kill(); await b.kill()

// ============================================================ 3) Sesli oda
console.log('\n[3] Sesli oda (#ekip-odası)')
b = bot({ P2_CALL_CHANNEL: 'ekip-odası' })
await b.wait(/sesli odasına katılınıyor/)
c = client({ P2_AUTOPILOT_VOICE: 'ekip-odası', P2_AUTOPILOT_CHANNEL: 'genel' })
check('sesli odaya girildi', !!(await c.wait(/arama: katıldın \(channel, 2 kişi\)/, 12000)))
check('WebRTC eşi bağlandı', !!(await c.wait(/arama: eş [0-9a-f]+ connected/, 15000)))
await sleep(4000)
check('odada iki yönde ses', incomingDb(c.lines) > -40 && botHeard(b.lines).some(h => h.db > -30), `istemci ${incomingDb(c.lines)} dBFS`)
await sleep(500)
shot(c.proc.pid, '2-sesli-oda')
await c.kill(); await b.kill()

// ============================================================ 4b) Ekran paylaşımı (iki yön)
console.log('\n[4b] Ekran paylaşımı')
b = bot({})
await b.wait(/gelen arama bekleniyor/)
c = client({ P2_AUTOPILOT_CALL: 'rustbot', P2_AUTOPILOT_STAGE: '1', P2_AUTOPILOT_SCREEN: '4', P2_AUTOPILOT_SCREEN_STOP: '9' })
const botVideo = from => b.lines.slice(from).map(l => /görüntü: ([\d.]+) kare\/sn (\d+)x(\d+)/.exec(l)).filter(Boolean).map(m => ({ fps: Number(m[1]), w: Number(m[2]), h: Number(m[3]) }))
check('aramaya girildi', !!(await c.wait(/arama: katıldın \(direct/, 20000)))
check('WebRTC eşi bağlandı', !!(await c.wait(/arama: eş [0-9a-f]+ connected/, 15000)))
await sleep(3000)
check('yayın yokken bota (siyah) görüntü gitmiyor', botVideo(0).length > 0 && botVideo(0).every(v => v.fps === 0), JSON.stringify(botVideo(0)))
const vline = await c.wait(/görüntü: 9638da5e 1280x720 (\d+) kare\/sn/, 15000)
check('botun yayını istemcide oynuyor (1280x720)', !!vline && Number(/(\d+) kare\/sn/.exec(vline)[1]) >= 8, vline || '')
check('yayın açıldı (yeniden pazarlıkla)', !!(await c.wait(/arama: ekran paylaşımı açık/, 10000)))
const mark = b.lines.length
await sleep(4500)
check('istemcinin yayını botta (1280x720)', botVideo(mark).some(v => v.fps >= 8 && v.w === 1280 && v.h === 720), JSON.stringify(botVideo(mark).slice(-2)))
shot(c.proc.pid, '4-ekran-paylasimi')
check('yayın kapandı', !!(await c.wait(/arama: ekran paylaşımı kapalı/, 10000)))
const mark2 = b.lines.length
await sleep(6500)
const after = botVideo(mark2)
check('kapatınca görüntü durdu', after.length > 0 && after[after.length - 1].fps === 0, JSON.stringify(after))
check('ses hâlâ akıyor', botHeard(b.lines.slice(mark2)).some(h => h.fps > 50), JSON.stringify(botHeard(b.lines.slice(mark2)).slice(-1)))
await c.kill(); await b.kill()

// ============================================================ 4) Ses ayarları + mikrofon testi
console.log('\n[4] Ayarlar → Ses ve Görüntü')
c = client({ P2_AUTOPILOT_SETTINGS: 'media', P2_AUTOPILOT_MIC_TEST: '1' })
check('ses ayarları açıldı', !!(await c.wait(/ayarlar: açıldı \(Media\)/)))
check('aygıtlar listelendi', !!(await c.wait(/ayarlar: \d+ giriş, \d+ çıkış aygıtı/)))
check('mikrofon testi seviye ölçüyor', !!(await c.wait(/ses: mikrofon -1\d dBFS/, 8000)))
await sleep(1200)
shot(c.proc.pid, '3-ses-ayarlari')
await c.kill()

console.log(`\nSONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
