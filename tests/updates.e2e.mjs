// Güncelleme ucu: dosya yokken 404, varken içerik, Range ile kısmi indirme,
// dizin dışına çıkma denemesinin reddi. Gerçek NestJS uygulaması ayağa kaldırılır.
import { mkdtempSync, writeFileSync, mkdirSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const dir = mkdtempSync(join(tmpdir(), 'p2upd-'))
mkdirSync(dir, { recursive: true })
process.env.P2_UPDATE_DIR = dir

const { NestFactory } = await import('@nestjs/core')
const { UpdatesController } = await import('file://' + join(process.cwd(), 'server', 'dist', 'updates.js').replace(/\\/g, '/'))
const { RealtimeGateway } = await import('file://' + join(process.cwd(), 'server', 'dist', 'realtime.js').replace(/\\/g, '/'))
const { Module } = await import('@nestjs/common')
// Uç, yeni sürüm yayınlanınca duyuru yapıyor; testte sahte bir yayıncı yeter.
const duyurular = []
const sahteGateway = { provide: RealtimeGateway, useValue: { broadcast: (event, payload) => duyurular.push({ event, payload }), emit() {} } }

const ok = [], bad = []
const check = (name, condition, extra = '') => { (condition ? ok : bad).push(name); console.log((condition ? '✓ ' : '✗ ') + name + (condition ? '' : ' → ' + String(extra).slice(0, 180))) }

// Dekoratör yerine doğrudan metadata: bu betik saf JS.
class TestModule {}
Reflect.defineMetadata ? null : await import('reflect-metadata')
Module({ controllers: [UpdatesController], providers: [sahteGateway] })(TestModule)

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
// 7) P2_UPDATE_DIR verilmeden: servis olarak kalkan sunucu yayın dizinini
// kendiliğinden bulmalı (bu değişken unutulunca güncelleme sessizce 404 dönüyordu).
// Dizinler modül yüklenirken hesaplandığı için bu tur ayrı bir süreçte koşar.
{
  const home = mkdtempSync(join(tmpdir(), 'p2upd-home-'))
  const published = join(home, 'P2', 'updates')   // %ProgramData%/P2/updates karşılığı
  mkdirSync(published, { recursive: true })
  writeFileSync(join(published, 'latest.yml'), 'version: 9.9.9\n')
  const moduleUrl = 'file://' + join(process.cwd(), 'server', 'dist', 'updates.js').replace(/\\/g, '/')
  const gatewayUrl = 'file://' + join(process.cwd(), 'server', 'dist', 'realtime.js').replace(/\\/g, '/')
  const script = [
    "const { NestFactory } = await import('@nestjs/core')",
    "const { Module } = await import('@nestjs/common')",
    "const { UpdatesController } = await import(" + JSON.stringify(moduleUrl) + ")",
    "const { RealtimeGateway: UpdatesModuleGateway } = await import(" + JSON.stringify(gatewayUrl) + ")",
    'class M {}',
    'Module({ controllers: [UpdatesController], providers: [{ provide: UpdatesModuleGateway, useValue: { broadcast(){}, emit(){} } }] })(M)',
    "const app = await NestFactory.create(M, { logger: false })",
    "await app.listen(0, '127.0.0.1')",
    "const response = await fetch((await app.getUrl()) + '/updates/latest.yml')",
    "console.log(response.status + ' ' + (await response.text()).trim())",
    'await app.close()',
  ].join('\n')
  const env = { ...process.env, ProgramData: home }
  delete env.P2_UPDATE_DIR
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8', env })
  const line = String(child.stdout || '').trim().split('\n').pop() || String(child.stderr || '').slice(-160)
  check('P2_UPDATE_DIR olmadan yayın dizini bulunuyor', line.startsWith('200') && line.includes('9.9.9'), line)
}

console.log(`\n${ok.length} geçti, ${bad.length} kaldı`)
process.exitCode = bad.length ? 1 : 0
