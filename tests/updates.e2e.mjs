// Güncelleme ucu: dosya yokken 404, varken içerik, Range ile kısmi indirme,
// dizin dışına çıkma denemesinin reddi. Gerçek NestJS uygulaması ayağa kaldırılır.
import { mkdtempSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const dir = mkdtempSync(join(tmpdir(), 'p2upd-'))
mkdirSync(dir, { recursive: true })
process.env.P2_UPDATE_DIR = dir

const { NestFactory } = await import('@nestjs/core')
const { UpdatesController } = await import('file://' + join(process.cwd(), 'server', 'dist', 'updates.js').replace(/\\/g, '/'))
const { Module } = await import('@nestjs/common')

const ok = [], bad = []
const check = (name, condition, extra = '') => { (condition ? ok : bad).push(name); console.log((condition ? '✓ ' : '✗ ') + name + (condition ? '' : ' → ' + String(extra).slice(0, 180))) }

// Dekoratör yerine doğrudan metadata: bu betik saf JS.
class TestModule {}
Reflect.defineMetadata ? null : await import('reflect-metadata')
Module({ controllers: [UpdatesController] })(TestModule)

const app = await NestFactory.create(TestModule, { logger: false })
await app.listen(0, '127.0.0.1')
const base = await app.getUrl()

// 1) Henüz sürüm yayınlanmamış
let response = await fetch(`${base}/updates/latest.yml`)
check('sürüm yokken 404', response.status === 404, response.status)

// 2) Yayınlanmış sürüm
const yml = 'version: 0.4.4\nfiles:\n  - url: P2-Setup-0.4.4.exe\n    sha512: abc\n    size: 12\npath: P2-Setup-0.4.4.exe\n'
writeFileSync(join(dir, 'latest.yml'), yml)
const payload = Buffer.from('P2 KURULUM DOSYASI!!')
writeFileSync(join(dir, 'P2-Setup-0.4.4.exe'), payload)

response = await fetch(`${base}/updates/latest.yml`)
const body = await response.text()
check('latest.yml sunuluyor', response.status === 200 && body === yml, `${response.status} ${body.slice(0, 40)}`)
check('yml içerik türü ve önbellek kapalı', /yaml/.test(response.headers.get('content-type') || '') && /no-store/.test(response.headers.get('cache-control') || ''), response.headers.get('content-type'))

response = await fetch(`${base}/updates/P2-Setup-0.4.4.exe`)
const buffer = Buffer.from(await response.arrayBuffer())
check('kurulum dosyası tam iniyor', response.status === 200 && buffer.equals(payload), `${response.status} ${buffer.length}`)
check('boyut ve Range desteği bildiriliyor', response.headers.get('content-length') === String(payload.length) && response.headers.get('accept-ranges') === 'bytes', response.headers.get('content-length'))

// 3) Kısmi indirme (kesilen indirmeyi sürdürme)
response = await fetch(`${base}/updates/P2-Setup-0.4.4.exe`, { headers: { Range: 'bytes=3-8' } })
const part = Buffer.from(await response.arrayBuffer())
check('Range 206 ve doğru parça', response.status === 206 && part.toString() === payload.subarray(3, 9).toString(), `${response.status} ${part.toString()}`)
check('Content-Range başlığı doğru', response.headers.get('content-range') === `bytes 3-8/${payload.length}`, response.headers.get('content-range'))

response = await fetch(`${base}/updates/P2-Setup-0.4.4.exe`, { headers: { Range: 'bytes=5-' } })
const tail = Buffer.from(await response.arrayBuffer())
check('açık uçlu Range dosyanın sonunu veriyor', response.status === 206 && tail.toString() === payload.subarray(5).toString(), tail.toString())

response = await fetch(`${base}/updates/P2-Setup-0.4.4.exe`, { headers: { Range: 'bytes=999-1200' } })
check('aralık dışı istek 416', response.status === 416, response.status)

// 4) Güvenlik
writeFileSync(join(dir, '..', 'gizli.json'), '{"parola":"acik"}')
for (const attempt of ['..%2Fgizli.json', '%2e%2e%2fgizli.json', 'alt%2Fdosya.exe']) {
  response = await fetch(`${base}/updates/${attempt}`)
  check(`dizin dışına çıkma engellendi (${attempt})`, response.status === 400 || response.status === 404, response.status)
}
response = await fetch(`${base}/updates/yok.exe`)
check('olmayan dosya 404', response.status === 404, response.status)

await app.close()
console.log(`\n${ok.length} geçti, ${bad.length} kaldı`)
process.exitCode = bad.length ? 1 : 0
