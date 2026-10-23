// Kütüphane parça deposu: içerik adresli (sha256) 4 MB parçalar diskte tutulur,
// aynı parça bütün sürümler/projeler arasında tek kez saklanır.
//   <P2_RELEASE_DIR>/chunks/ab/cd/<hash>   kalıcı, değişmez
//   <P2_RELEASE_DIR>/tmp/<hash>.<uuid>.part yazılırken; açılışta süpürülür
import { BadRequestException, PayloadTooLargeException, UnprocessableEntityException } from '@nestjs/common'
import { createHash, randomUUID } from 'node:crypto'
import { createWriteStream } from 'node:fs'
import { mkdir, readdir, rename, rm, stat, unlink } from 'node:fs/promises'
import type { IncomingMessage } from 'node:http'
import { dirname, join } from 'node:path'
import { PrismaService } from './prisma.service'
import { CHUNK_SIZE, isHash } from './releaseManifest'

export { CHUNK_SIZE, isHash }
export const root = process.env.P2_RELEASE_DIR || join(process.cwd(), '.p2-data', 'releases')
export const chunkPath = (hash: string) => join(root, 'chunks', hash.slice(0, 2), hash.slice(2, 4), hash)
const tmpDir = () => join(root, 'tmp')

export async function ensureDirs() {
  await mkdir(join(root, 'chunks'), { recursive: true })
  await mkdir(tmpDir(), { recursive: true })
}

// Yarım kalan yazımlar sunucu yeniden başlayınca anlamsızdır; istemci zaten
// "eksik parçalar" sorusuyla gerçek durumu öğrenip devam eder.
export async function sweepTmp() {
  try { await rm(tmpDir(), { recursive: true, force: true }) } catch {}
  await mkdir(tmpDir(), { recursive: true })
}

export async function chunkExists(hash: string) {
  try { const info = await stat(chunkPath(hash)); return info.isFile() && info.size > 0 } catch { return false }
}

export async function missingChunks(hashes: string[]) {
  const missing: string[] = []
  for (let index = 0; index < hashes.length; index += 64) {
    const batch = hashes.slice(index, index + 64)
    const present = await Promise.all(batch.map(chunkExists))
    batch.forEach((hash, position) => { if (!present[position]) missing.push(hash) })
  }
  return missing
}

// Ham gövdeyi geçici dosyaya akıtır, sha256'yı yolda hesaplar; özet eşleşirse
// parça atomik olarak kalıcı yerine taşınır. Aynı parçayı iki istemci aynı anda
// yüklerse ikisi de başarılı sayılır, diskte tek dosya kalır.
export function writeChunk(req: IncomingMessage, hash: string, maxBytes = CHUNK_SIZE): Promise<{ size: number }> {
  return new Promise<{ size: number }>((resolve, reject) => {
    void (async () => {
      const target = chunkPath(hash)
      await mkdir(dirname(target), { recursive: true })
      await mkdir(tmpDir(), { recursive: true })
      const part = join(tmpDir(), `${hash}.${randomUUID()}.part`)
      const out = createWriteStream(part, { flags: 'wx' })
      const digest = createHash('sha256')
      let size = 0
      let failed = false
      const fail = (error: Error) => {
        if (failed) return
        failed = true
        try { req.unpipe(out) } catch {}
        out.destroy()
        void unlink(part).catch(() => {}).then(() => reject(error))
      }
      req.on('data', (chunk: Buffer) => {
        if (failed) return
        size += chunk.length
        if (size > maxBytes) { req.destroy(); fail(new PayloadTooLargeException({ code: 'CHUNK_TOO_LARGE', message: 'Parça 4 MB sınırını aşıyor.' })); return }
        digest.update(chunk)
      })
      req.on('error', error => fail(error))
      req.on('aborted', () => fail(new BadRequestException({ code: 'CHUNK_ABORTED', message: 'Yükleme kesildi.' })))
      out.on('error', error => fail(error))
      out.on('finish', () => {
        if (failed) return
        void (async () => {
          if (!size) { await unlink(part).catch(() => {}); return reject(new BadRequestException({ code: 'CHUNK_EMPTY', message: 'Boş parça gönderilemez.' })) }
          if (digest.digest('hex') !== hash) { await unlink(part).catch(() => {}); return reject(new UnprocessableEntityException({ code: 'CHUNK_HASH_MISMATCH', message: 'Parça içeriği beklenen özetle uyuşmuyor.' })) }
          try { await rename(part, target) }
          catch {
            await unlink(part).catch(() => {})
            if (!(await chunkExists(hash))) return reject(new Error('CHUNK_WRITE_FAILED'))
          }
          resolve({ size })
        })()
      })
      req.pipe(out)
    })().catch(reject)
  })
}

// ---- çöp toplama ----
// Referans sayısı ReleaseChunkRef tablosundan türetilir: hiçbir sürümün
// başvurmadığı parça satırı silinir, dosyası kaldırılır. Ayrıca diskte olup
// tabloda olmayan (çökme artığı) dosyalar süpürülür. Tek seferde tek çalışır.
let gcRunning: Promise<{ removed: number }> | null = null
let gcTimer: NodeJS.Timeout | undefined
const YOUNG = 10 * 60 * 1000

export function scheduleGc(db: PrismaService, delay = 2000) {
  if (gcTimer) clearTimeout(gcTimer)
  gcTimer = setTimeout(() => { gcTimer = undefined; void gcChunks(db) }, delay)
  gcTimer.unref?.()
}

export async function gcChunks(db: PrismaService, deep = false) {
  if (gcRunning) return gcRunning
  gcRunning = (async () => {
    let removed = 0
    try {
      // Bir hafta dokunulmamış taslaklar terk edilmiş sayılır.
      await db.release.deleteMany({ where: { status: 'DRAFT', updatedAt: { lt: new Date(Date.now() - 7 * 24 * 3600 * 1000) } } })
      const rows = await db.$queryRaw<{ hash: string }[]>`DELETE FROM "ReleaseChunk" c WHERE NOT EXISTS (SELECT 1 FROM "ReleaseChunkRef" r WHERE r."hash" = c."hash") AND c."createdAt" < NOW() - INTERVAL '10 minutes' RETURNING "hash"`
      for (const row of rows) { try { await unlink(chunkPath(row.hash)); removed++ } catch {} }
      if (deep) removed += await sweepOrphans(db)
    } catch {}
    return { removed }
  })()
  try { return await gcRunning } finally { gcRunning = null }
}

async function sweepOrphans(db: PrismaService) {
  let removed = 0
  const base = join(root, 'chunks')
  let level1: string[] = []
  try { level1 = await readdir(base) } catch { return 0 }
  for (const a of level1) {
    let level2: string[] = []
    try { level2 = await readdir(join(base, a)) } catch { continue }
    for (const b of level2) {
      let names: string[] = []
      try { names = (await readdir(join(base, a, b))).filter(isHash) } catch { continue }
      for (let index = 0; index < names.length; index += 500) {
        const batch = names.slice(index, index + 500)
        const known = new Set((await db.releaseChunk.findMany({ where: { hash: { in: batch } }, select: { hash: true } })).map(row => row.hash))
        for (const hash of batch) {
          if (known.has(hash)) continue
          const file = join(base, a, b, hash)
          try {
            const info = await stat(file)
            if (Date.now() - info.mtimeMs < YOUNG) continue
            await unlink(file); removed++
          } catch {}
        }
      }
    }
  }
  return removed
}
