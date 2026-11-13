// P2 Server Console (Rust) uçtan uca testi — otomatik pilot.
//   node rust/tools/server-ops/console-e2e.mjs [görüntü-klasörü]
// Geçici kurulum klasörüne p2-server-console.exe + p2-server.exe kopyalanır, geçici veri klasörü
// (P2_SERVER_DATA_DIR) ve test PostgreSQL'inde (:55432) yeni bir karalama veritabanı kullanılır.
// Konsol formu doldurur (CREATE DATABASE + göçler), API'yi başlatır (/health), durdurur, yeniden başlatır,
// yedek alır/yükler, form doğrulamasını ve güncelleme katmanını gösterir; her adımda kendi penceresinin
// görüntüsünü alır (p2-wincap). Windows hizmeti/UAC yok (P2_SERVER_NO_SERVICE=1), API 127.0.0.1:4140.
import { spawn, execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readFileSync, existsSync, rmSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomBytes } from 'node:crypto'

const target = 'C:/p2-rust-target/release'
const shots = process.argv[2] || join(tmpdir(), 'p2-console-shots')
const install = mkdtempSync(join(tmpdir(), 'p2-console-install-'))
// Sabit veri klasörü: görüntüler Electron referansıyla (aynı yol metni) karşılaştırılabilsin.
const data = String.raw`C:\p2-pg\console-e2e\data`
rmSync(data, { recursive: true, force: true })
mkdirSync(data, { recursive: true })
for (const exe of ['p2-server-console.exe', 'p2-server.exe']) copyFileSync(join(target, exe), join(install, exe))
mkdirSync(shots, { recursive: true })
const env = JSON.parse(readFileSync('C:/p2-pg/test-env.json', 'utf8').replace(/^\uFEFF/, ''))
const base = new URL(env.DATABASE_URL)
const db = `p2_console_e2e_${randomBytes(3).toString('hex')}`
const psql = 'C:/p2-pg/pgsql/bin/psql.exe'
const admin = `postgresql://${base.username}@${base.hostname}:${base.port}/postgres`
const backupFile = join(data, 'e2e.p2backup')

const child = spawn(join(install, 'p2-server-console.exe'), [], {
  env: {
    ...process.env,
    P2_SERVER_DATA_DIR: data, P2_SERVER_NO_SERVICE: '1', P2_CONSOLE_AUTOPILOT: '1',
    P2_CONSOLE_AUTOPILOT_DB: `${base.hostname}:${base.port}:${base.username}:${db}`, P2_CONSOLE_AUTOPILOT_PORT: '4140',
    P2_PUBLIC_IP: '203.0.113.7', HOST: '127.0.0.1', P2_CONSOLE_SHOTS: shots, P2_CONSOLE_NO_SHELL: '1',
    P2_CONSOLE_BACKUP_PATH: backupFile, P2_CONSOLE_RESTORE_PATH: backupFile, P2_PG_BIN: 'C:/p2-pg/pgsql/bin',
    ...(process.env.P2_CONSOLE_WINDOW_HEIGHT ? { P2_CONSOLE_WINDOW_HEIGHT: process.env.P2_CONSOLE_WINDOW_HEIGHT } : {}),
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})
let out = ''
child.stdout.on('data', chunk => { out += chunk; process.stdout.write(chunk) })
child.stderr.on('data', chunk => process.stderr.write(chunk))
const timer = setTimeout(() => { console.error('ZAMAN AŞIMI'); child.kill() }, 5 * 60 * 1000)
const code = await new Promise(resolve => child.on('exit', resolve))
clearTimeout(timer)

const checks = []
const check = (name, ok) => { checks.push([name, ok]); console.log(`${ok ? 'GEÇTİ' : 'KALDI'}  ${name}`) }
check('otomatik pilot bitti (çıkış 0)', code === 0 && out.includes('[autopilot] bitti'))
check('/health yanıt verdi', /\/health: Some\("\{\\"status\\":\\"ok\\"/.test(out))
check('server.json yazıldı', existsSync(join(data, 'server.json')))
const log = existsSync(join(data, 'server.log')) ? readFileSync(join(data, 'server.log'), 'utf8') : ''
check('günlük: yapılandırma', log.includes('Yapılandırma kaydedildi; API portu 4140'))
check('günlük: başlatma', log.includes('P2 API başlatılıyor: 0.0.0.0:4140'))
check('günlük: durdurma', log.includes('P2 API yönetim konsolundan durduruldu'))
check('günlük: yedek', log.includes('Yedek oluşturuldu:') && log.includes('Yedek başarıyla yüklendi ('))
check('yedek dosyası', existsSync(backupFile))
let alive = false
try { alive = (await fetch('http://127.0.0.1:4140/health')).ok } catch {}
check('sonda API durmuş', !alive)
for (const name of ['01-baslangic', '02-calisiyor', '03-durduruldu', '04-yedek', '05-dogrulama', '06-guncelleme', '07-guncelleme-hata']) check(`görüntü ${name}`, existsSync(join(shots, `${name}.png`)))

try { execFileSync(psql, [admin, '-c', `DROP DATABASE IF EXISTS "${db}" WITH (FORCE)`], { stdio: 'ignore' }) } catch {}
for (const dir of [install, data]) { try { rmSync(dir, { recursive: true, force: true }) } catch (error) { console.log('temizlenemedi: ' + dir + ' — ' + error.code) } }
console.log(`görüntüler: ${shots}`)
const failed = checks.filter(([, ok]) => !ok).length
console.log(failed ? `${failed} denetim KALDI` : 'hepsi geçti')
process.exit(failed ? 1 : 0)
