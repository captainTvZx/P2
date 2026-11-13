// Dosya yükleme/sunma (uploads.ts): aynı yüklemeler iki sunucuya yapılır, cevaplar, meta dosyaları ve
// sunulan dosyaların başlıkları/baytları (aralık istekleri dahil) çapraz karşılaştırılır.
// İki sunucu aynı yükleme kökünü kullanır (P2_UPLOAD_DIR yoksa <cwd>/.p2-data/uploads): NestJS
// depo kökünden çalışıyor, Rust da depo kökünde başlatılır.
import { existsSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { NEST, startRust, call, login, same, check, done } from './lib.mjs'

const repo = 'C:/Users/capta/OneDrive/Desktop/P2'
process.chdir(repo)
const dir = process.env.P2_UPLOAD_DIR || join(repo, '.p2-data', 'uploads')
const rust = await startRust(4123)
const t = (await login(NEST, 'p2test')).accessToken

const b64 = buf => Buffer.from(buf).toString('base64')
const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
const wav = Buffer.concat([Buffer.from('RIFF'), Buffer.from([0, 0, 0, 0]), Buffer.from('WAVEfmt '), Buffer.alloc(8)])
const mp4 = Buffer.concat([Buffer.from([0, 0, 0, 0x18]), Buffer.from('ftypM4A '), Buffer.alloc(8)])
const random = Buffer.from(Array.from({ length: 300 }, (_, i) => (i * 37 + 11) % 256).map(v => v || 1).map(v => v < 32 ? v : v))
const big = Buffer.alloc(5 * 1024 * 1024, 'a')

const cases = [
  ['jetonsuz', { name: 'a.txt', data: 'data:text/plain;base64,' + b64('a') }, null],
  ['boş gövde', {}],
  ['veri yok', { name: 'a.txt', data: 'data:text/plain;base64,' }],
  ['veri öneksiz', { name: 'a.txt', data: b64('merhaba') }],
  ['veri satır sonu', { name: 'a.txt', data: 'data:text/plain;base64,YQ==\nYQ==' }],
  ['boş dosya', { name: 'a.txt', data: 'data:text/plain;base64,====' }],
  ['png', { name: 'resim.PNG', type: 'image/png', data: 'data:image/png;base64,' + b64(png) }],
  ['png yanlış ad', { name: 'çizim.txt', type: 'text/plain', data: 'data:;base64,' + b64(png) }],
  ['pdf', { name: 'belge', data: 'data:application/pdf;base64,' + b64('%PDF-1.7 deneme') }],
  ['metin html', { name: 'not', type: 'text/html', data: 'data:text/html;base64,' + b64('<b>merhaba</b> dünya') }],
  ['kaynak kodu', { name: 'kod.ts', type: 'video/mp2t', data: 'data:video/mp2t;base64,' + b64('export const x = 1') }],
  ['svg', { name: 'logo.svg', data: 'data:image/svg+xml;base64,' + b64('<svg/>') }],
  ['wav', { name: 'ses.bin', data: 'data:audio/wav;base64,' + b64(wav) }],
  ['m4a', { name: 'ses.m4a', data: 'data:audio/mp4;base64,' + b64(mp4) }],
  ['ikili', { name: 'veri', type: 'application/x-foo', data: 'data:application/x-foo;base64,' + b64(random) }],
  ['garip ad', { name: 'a/b\\c:d*e?"<>|\u0001.tar.GZ', type: 123, data: 'data:x;base64,' + b64('düz metin') }],
  ['ad sayı', { name: 12.0, data: 'data:x;base64,' + b64('a') }],
  ['ad yok, url-safe base64', { data: 'data:x;base64,' + b64('ab?>').replace(/\+/g, '-').replace(/\//g, '_') + '==ignored' }],
  ['ad nokta', { name: '.gizli', data: 'data:x;base64,' + b64('gizli') }],
  ['büyük (5 MB)', { name: 'büyük.txt', data: 'data:text/plain;base64,' + b64(big) }],
]

const uploaded = []
const norm = res => (res.body && typeof res.body.url === 'string' ? { ...res, body: { ...res.body, url: res.body.url.replace(/^\/uploads\/[0-9a-f-]{36}/, '/uploads/<id>') } } : res)
for (const [name, body, token] of cases) {
  const a = await call(NEST, 'POST', '/uploads', body, token === null ? undefined : t)
  const b = await call(rust.base, 'POST', '/uploads', body, token === null ? undefined : t)
  same(name, norm(a), norm(b))
  if (a.body?.url) uploaded.push([name, a.body.url, b.body.url])
}

// Meta dosyaları aynı biçimde.
for (const [name, a, b] of uploaded) {
  const ma = join(dir, a.slice(9) + '.json'), mb = join(dir, b.slice(9) + '.json')
  if (!existsSync(ma) || !existsSync(mb)) { check('meta ' + name, false, 'dosya yok ' + dir); continue }
  check('meta ' + name, readFileSync(ma, 'utf8') === readFileSync(mb, 'utf8'))
}

const HEADERS = ['transfer-encoding', 'content-type', 'content-disposition', 'content-length', 'content-range', 'accept-ranges', 'cache-control', 'x-content-type-options']
async function fetchFile(base, path, method = 'GET', range) {
  const res = await fetch(base + path, { method, headers: range ? { range } : {} })
  const bytes = Buffer.from(await res.arrayBuffer())
  return { status: res.status, headers: Object.fromEntries(HEADERS.map(h => [h, res.headers.get(h)])), body: bytes.toString('base64') }
}
// NestJS'in yazdığını iki sunucu da aynı sunar (ve tersi).
for (const [name, a, b] of uploaded) {
  for (const [label, path] of [['nest dosyası', a], ['rust dosyası', b]]) {
    same(`sun ${name} (${label})`, await fetchFile(NEST, path), await fetchFile(rust.base, path))
  }
}
const [, p] = uploaded.find(([n]) => n === 'metin html')
for (const range of ['bytes=0-3', 'bytes=5-', 'bytes=-4', 'bytes=-0', 'bytes=2-1', 'bytes=3-999', 'bytes=999-', 'items=0-1', ' bytes=1-2 ', 'bytes=0-1,3-4']) {
  same(`aralık ${range}`, await fetchFile(NEST, p, 'GET', range), await fetchFile(rust.base, p, 'GET', range))
  same(`HEAD aralık ${range}`, await fetchFile(NEST, p, 'HEAD', range), await fetchFile(rust.base, p, 'HEAD', range))
}
same('HEAD', await fetchFile(NEST, p, 'HEAD'), await fetchFile(rust.base, p, 'HEAD'))
for (const path of ['/uploads/yok', '/uploads/00000000-0000-0000-0000-000000000000', '/uploads/00000000-0000-0000-0000-000000000000.png', '/uploads/ABCDEF00-0000-0000-0000-000000000000']) {
  same('bulunamadı ' + path, await fetchFile(NEST, path), await fetchFile(rust.base, path))
}

for (const [, a, b] of uploaded) for (const u of [a, b]) for (const f of [u.slice(9), u.slice(9) + '.json']) rmSync(join(dir, f), { force: true })
rust.stop()
done()
