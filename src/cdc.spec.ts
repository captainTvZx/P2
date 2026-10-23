import { describe, expect, it } from 'vitest'
import { createRequire } from 'node:module'
import { createHash } from 'node:crypto'

// İçerik tanımlı parçalayıcı ve kurulum planı: Electron'suz saf modüller.
const require = createRequire(import.meta.url)
const cdc = require('../electron/cdc.cjs') as { MIN: number; MAX: number; AVG: number; Chunker: new () => { feed(b: Buffer, final?: boolean): Buffer[]; finish(): Buffer[] }; splitBuffer(b: Buffer): Buffer[] }
const manifest = require('../electron/patcherManifest.cjs') as { planInstall: (a: any, b: any) => any; chunkRanges: (f: any) => Array<{ offset: number; length: number }>; CHUNK_SIZE: number }

// Deterministik sözde rastgele veri (sıkıştırılamaz, oyun pak'ına benzer).
function noise(bytes: number, seed: number) {
  const out = Buffer.allocUnsafe(bytes)
  let x = seed >>> 0
  for (let i = 0; i < bytes; i++) { x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0; out[i] = x & 0xff }
  return out
}
const sha = (b: Buffer) => createHash('sha256').update(b).digest('hex')
const fileOf = (path: string, data: Buffer) => { const pieces = cdc.splitBuffer(data); return { path, size: data.length, chunks: pieces.map(sha), lengths: pieces.map(p => p.length) } }

describe('içerik tanımlı parçalama (cdc.cjs)', () => {
  it('parçalar MIN..MAX aralığında kalır ve toplamı dosyayı verir', () => {
    const data = noise(37 * 1024 * 1024 + 123, 7)
    const pieces = cdc.splitBuffer(data)
    expect(Buffer.concat(pieces).equals(data)).toBe(true)
    for (const piece of pieces.slice(0, -1)) { expect(piece.length).toBeGreaterThanOrEqual(cdc.MIN); expect(piece.length).toBeLessThanOrEqual(cdc.MAX) }
    expect(pieces[pieces.length - 1].length).toBeLessThanOrEqual(cdc.MAX)
    // Ortalama hedefe yakın (1.3–3 MB arası makul).
    const avg = data.length / pieces.length
    expect(avg).toBeGreaterThan(1.3 * 1024 * 1024); expect(avg).toBeLessThan(3 * 1024 * 1024)
  })

  it('akış ve tek seferlik bölme aynı kesimleri verir (okuma bloğundan bağımsız)', () => {
    const data = noise(21 * 1024 * 1024 + 999, 11)
    const whole = cdc.splitBuffer(data).map(p => p.length)
    for (const block of [1 << 20, 3 * (1 << 20) + 7, 8 * (1 << 20)]) {
      const chunker = new cdc.Chunker(); const lengths: number[] = []
      for (let o = 0; o < data.length; o += block) for (const p of chunker.feed(Buffer.from(data.subarray(o, Math.min(data.length, o + block))))) lengths.push(p.length)
      for (const p of chunker.finish()) lengths.push(p.length)
      expect(lengths).toEqual(whole)
    }
  })

  it('başa bayt eklenince (kayma) parçaların büyük çoğunluğu aynı kalır', () => {
    const base = noise(64 * 1024 * 1024, 3)
    const shifted = Buffer.concat([Buffer.from('yeni baslik verisi 1234'), base])
    const a = new Set(cdc.splitBuffer(base).map(sha))
    const b = cdc.splitBuffer(shifted).map(sha)
    const reused = b.filter(h => a.has(h)).length
    // Sabit dilimlemede 0 olurdu; burada en fazla ilk 1-2 parça değişir.
    expect(b.length - reused).toBeLessThanOrEqual(2)
  })

  it('ortadaki küçük bir değişiklik yalnız o civardaki parçayı etkiler', () => {
    const base = noise(48 * 1024 * 1024, 5)
    const edited = Buffer.from(base); edited.fill(0xab, 20 * 1024 * 1024, 20 * 1024 * 1024 + 64)
    const a = new Set(cdc.splitBuffer(base).map(sha))
    const b = cdc.splitBuffer(edited).map(sha)
    expect(b.filter(h => !a.has(h)).length).toBeLessThanOrEqual(2)
  })

  it('boş dosya parça üretmez', () => { expect(cdc.splitBuffer(Buffer.alloc(0))).toEqual([]) })
})

describe('planInstall değişken boylu parçalarla', () => {
  it('kaymış dosyada yalnız değişen parçalar ağdan iner, geri kalan yerelden kopyalanır', () => {
    const base = noise(40 * 1024 * 1024, 9)
    const next = Buffer.concat([Buffer.from('v2 header'), base])
    const installed = { files: [fileOf('Game/data.pak', base)] }
    const target = { files: [fileOf('Game/data.pak', next)] }
    const plan = manifest.planInstall(installed, target)
    expect(plan.builds).toHaveLength(1)
    const build = plan.builds[0]
    const fromNet = build.chunks.filter((c: any) => c.from === 'net')
    expect(fromNet.length).toBeLessThanOrEqual(2)
    expect(plan.bytesToDownload).toBeLessThan(3 * cdc.MAX)
    // Her parçanın hedef ofseti ardışık ve kaynak ofseti kaynak dosyadaki gerçek yeri.
    let offset = 0
    for (const chunk of build.chunks) { expect(chunk.offset).toBe(offset); offset += chunk.length; if (chunk.from === 'self') expect(chunk.srcOffset).toBeTypeOf('number') }
    expect(offset).toBe(next.length)
  })

  it('eski sabit düzenli kurulu manifestten yeni düzene geçiş çalışır', () => {
    const data = noise(10 * 1024 * 1024, 13)
    // Eski: 4 MB sabit dilimler, lengths yok.
    const oldChunks: string[] = []; for (let o = 0; o < data.length; o += manifest.CHUNK_SIZE) oldChunks.push(sha(data.subarray(o, Math.min(data.length, o + manifest.CHUNK_SIZE))))
    const installed = { files: [{ path: 'a.bin', size: data.length, chunks: oldChunks }] }
    expect(manifest.chunkRanges(installed.files[0]).map(r => r.length)).toEqual([4194304, 4194304, 2097152])
    const target = { files: [fileOf('a.bin', data)] }
    const plan = manifest.planInstall(installed, target)
    // İçerik aynı ama düzen farklı: parçalar eşleşmez, dosya yeniden indirilir; plan tutarlı.
    const build = plan.builds[0]
    expect(build.chunks.reduce((s: number, c: any) => s + c.length, 0)).toBe(data.length)
  })
})
