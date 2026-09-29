// Kurulum, çalışan sunucu P2.exe'yi kilitlediği için başarısız oluyordu
// ("failed to uninstall old application files"). Servis SYSTEM hesabıyla
// koştuğunda dışarıdan durdurulamadığı için sunucu, aynı makineden gelen
// imzalı bir istekle kendini kapatıyor. Bu testte kapanma ucu doğrulanır.
// Kendi başlattığın bir sunucuya karşı çalıştır: P2_API ile adresini ver.
import fs from 'node:fs'
import { createHmac } from 'node:crypto'

const api = process.env.P2_API || 'http://127.0.0.1:4100'
const secret = process.env.P2_SECRET || (fs.readFileSync('.env', 'utf8').match(/JWT_ACCESS_SECRET="?([^"\r\n]+)/) || [])[1]
if (!secret) { console.error('JWT_ACCESS_SECRET bulunamadı'); process.exit(1) }

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }
const canli = async () => { try { const r = await fetch(`${api}/health`, { signal: AbortSignal.timeout(1500) }); return r.ok } catch { return false } }

check('sunucu ayakta', await canli())

const gonder = async token => {
  const response = await fetch(`${api}/updates/stop`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token }),
  })
  return response.status
}

check('yanlış anahtar reddediliyor', await gonder('yanlis-anahtar') === 403)
check('anahtarsız istek reddediliyor', await gonder('') === 403)
check('sunucu hâlâ ayakta', await canli())

const token = createHmac('sha256', secret).update('p2-shutdown').digest('hex')
const durum = await gonder(token)
check('doğru anahtar kabul ediliyor', durum === 200 || durum === 201, String(durum))

let kapandi = false
for (let tur = 0; tur < 20; tur++) {
  await new Promise(r => setTimeout(r, 400))
  if (!(await canli())) { kapandi = true; break }
}
check('sunucu kendini kapattı', kapandi, kapandi ? '' : '8 saniyede kapanmadı')

console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
