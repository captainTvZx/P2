// Kütüphane sürüm manifesti: saf doğrulama/normalleştirme, Nest'e bağımlı değil.
// Aynı kurallar istemcide electron/patcherManifest.cjs içinde de uygulanır;
// vitest uyumluluk testi iki kopyanın aynı sonucu verdiğini doğrular.

export const CHUNK_SIZE = 4 * 1024 * 1024
export const MANIFEST_VERSION = 1

// `lengths`: içerik tanımlı (değişken boylu, en çok CHUNK_SIZE) parçaların bayt
// uzunlukları; yoksa eski sabit 4 MB düzen. Sabit düzende dosyanın başına bir bayt
// girince sonraki bütün parçalar değişiyordu; içerik tanımlı kesimde yalnız 1-2 parça.
export type ManifestFile = { path: string; size: number; chunks: string[]; lengths?: number[] }
export function chunkLength(file: ManifestFile, index: number) {
  if (Array.isArray(file.lengths)) return file.lengths[index] || 0
  return Math.max(0, Math.min(file.size - index * CHUNK_SIZE, CHUNK_SIZE))
}
export type Manifest = {
  version: typeof MANIFEST_VERSION
  chunkSize: number
  platform: string
  executable: string
  launchArgs?: string
  files: ManifestFile[]
  totalSize: number
  fileCount: number
  chunkCount: number
}

export type ManifestLimits = { files: number; chunks: number; bytes: number }
export const DEFAULT_LIMITS: ManifestLimits = { files: 200_000, chunks: 400_000, bytes: 200 * 1024 ** 3 }

const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i
export const isHash = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)

// Göreli, güvenli bir yol: mutlak/sürücü/UNC yok, ".." yok, Windows'un yasak
// karakterleri ve ayrılmış cihaz adları yok, parça sonunda nokta/boşluk yok.
export function safeRelativePath(input: unknown): string | null {
  if (typeof input !== 'string') return null
  if (!input.length || input.length > 1024) return null
  if (/[\x00-\x1f<>:"|?*]/.test(input)) return null
  const value = input.replace(/\\/g, '/')
  if (value.startsWith('/') || /^[a-zA-Z]:/.test(value)) return null
  const parts = value.split('/')
  if (parts.length > 64) return null
  for (const part of parts) {
    if (!part || part === '.' || part === '..') return null
    if (/[. ]$/.test(part)) return null
    if (RESERVED.test(part)) return null
    if (part.length > 255) return null
  }
  return parts.join('/')
}

export const chunkCountFor = (size: number) => Math.ceil(size / CHUNK_SIZE)

export type ManifestInput = { executable?: unknown; launchArgs?: unknown; platform?: unknown; files?: unknown }
export type ManifestResult = { manifest: Manifest; uniqueHashes: string[] } | { error: string; code: string }

// İstemcinin gönderdiği dosya listesini doğrular; toplamlar sunucuda yeniden
// hesaplanır, istemciye güvenilmez.
export function validateManifestInput(input: ManifestInput, limits: ManifestLimits = DEFAULT_LIMITS): ManifestResult {
  const files = input.files
  if (!Array.isArray(files) || !files.length) return { error: 'Sürümde en az bir dosya olmalı.', code: 'MANIFEST_EMPTY' }
  if (files.length > limits.files) return { error: `Dosya sayısı ${limits.files} sınırını aşıyor.`, code: 'MANIFEST_TOO_MANY_FILES' }
  const executable = safeRelativePath(input.executable)
  if (!executable || !/\.exe$/i.test(executable)) return { error: 'Başlatılacak dosya .exe uzantılı geçerli bir yol olmalı.', code: 'MANIFEST_EXECUTABLE' }
  const launchArgs = typeof input.launchArgs === 'string' ? input.launchArgs.trim().slice(0, 500) : ''
  const platform = typeof input.platform === 'string' && /^[a-z0-9-]{3,20}$/.test(input.platform) ? input.platform : 'win-x64'
  const seen = new Set<string>()
  const unique = new Set<string>()
  const out: ManifestFile[] = []
  let totalSize = 0
  let hasExecutable = false
  for (const row of files as unknown[]) {
    if (!row || typeof row !== 'object') return { error: 'Dosya kaydı geçersiz.', code: 'MANIFEST_FILE' }
    const item = row as { path?: unknown; size?: unknown; chunks?: unknown; lengths?: unknown }
    const path = safeRelativePath(item.path)
    if (!path) return { error: `Geçersiz dosya yolu: ${String(item.path).slice(0, 120)}`, code: 'MANIFEST_PATH' }
    const key = path.toLowerCase()
    if (seen.has(key)) return { error: `Aynı yol iki kez geçiyor: ${path}`, code: 'MANIFEST_DUPLICATE' }
    seen.add(key)
    const size = item.size
    if (typeof size !== 'number' || !Number.isSafeInteger(size) || size < 0) return { error: `Dosya boyutu geçersiz: ${path}`, code: 'MANIFEST_SIZE' }
    const chunks = item.chunks
    if (!Array.isArray(chunks)) return { error: `Parça listesi geçersiz: ${path}`, code: 'MANIFEST_CHUNKS' }
    let lengths: number[] | undefined
    if (item.lengths !== undefined) {
      // İçerik tanımlı düzen: her parça 1..CHUNK_SIZE bayt, toplam dosya boyutuna eşit.
      if (!Array.isArray(item.lengths) || item.lengths.length !== chunks.length) return { error: `Parça uzunlukları parça sayısıyla uyuşmuyor: ${path}`, code: 'MANIFEST_CHUNKS' }
      let sum = 0
      for (const length of item.lengths as unknown[]) {
        if (typeof length !== 'number' || !Number.isSafeInteger(length) || length < 1 || length > CHUNK_SIZE) return { error: `Parça uzunluğu geçersiz: ${path}`, code: 'MANIFEST_CHUNKS' }
        sum += length
      }
      if (sum !== size) return { error: `Parça uzunlukları boyutla uyuşmuyor: ${path}`, code: 'MANIFEST_CHUNKS' }
      lengths = item.lengths as number[]
    } else if (chunks.length !== chunkCountFor(size)) return { error: `Parça sayısı boyutla uyuşmuyor: ${path}`, code: 'MANIFEST_CHUNKS' }
    for (const hash of chunks) {
      if (!isHash(hash)) return { error: `Parça özeti geçersiz: ${path}`, code: 'MANIFEST_HASH' }
      unique.add(hash)
      if (unique.size > limits.chunks) return { error: `Parça sayısı ${limits.chunks} sınırını aşıyor.`, code: 'MANIFEST_TOO_MANY_CHUNKS' }
    }
    totalSize += size
    if (totalSize > limits.bytes) return { error: 'Sürüm boyutu sınırı aşıyor.', code: 'MANIFEST_TOO_LARGE' }
    if (path === executable) hasExecutable = true
    out.push({ path, size, chunks: chunks as string[], ...(lengths ? { lengths } : {}) })
  }
  if (!hasExecutable) return { error: 'Başlatılacak dosya listede yok.', code: 'MANIFEST_EXECUTABLE_MISSING' }
  const manifest: Manifest = {
    version: MANIFEST_VERSION, chunkSize: CHUNK_SIZE, platform, executable,
    ...(launchArgs ? { launchArgs } : {}),
    files: out, totalSize, fileCount: out.length, chunkCount: unique.size,
  }
  return { manifest, uniqueHashes: [...unique] }
}

// İki paket arasındaki fark (Steam'in "ne değişti" özeti): dosya bazında eklendi /
// silindi / değişti ve önceki paketten geçecek birinin gerçekten indireceği bayt
// (yalnız önceki pakette bulunmayan parçalar).
export type ManifestDiff = {
  added: number; removed: number; changed: number; unchanged: number
  addedBytes: number; removedBytes: number; changedBytes: number; downloadBytes: number
  files: {
    added: Array<{ path: string; size: number }>
    removed: Array<{ path: string; size: number }>
    changed: Array<{ path: string; size: number; previousSize: number }>
  }
  truncated: boolean
}
export const DIFF_LIST_LIMIT = 300

export function diffManifests(previous: Manifest | null, next: Manifest, limit = DIFF_LIST_LIMIT): ManifestDiff {
  const chunkBytes = chunkLength
  const before = new Map((previous?.files || []).map(file => [file.path, file]))
  const had = new Set<string>()
  for (const file of previous?.files || []) for (const hash of file.chunks) had.add(hash)
  const out: ManifestDiff = { added: 0, removed: 0, changed: 0, unchanged: 0, addedBytes: 0, removedBytes: 0, changedBytes: 0, downloadBytes: 0, files: { added: [], removed: [], changed: [] }, truncated: false }
  const counted = new Set<string>()
  const push = <T>(list: T[], item: T) => { if (list.length < limit) list.push(item); else out.truncated = true }
  for (const file of next.files) {
    file.chunks.forEach((hash, index) => { if (!had.has(hash) && !counted.has(hash)) { counted.add(hash); out.downloadBytes += chunkBytes(file, index) } })
    const old = before.get(file.path)
    if (!old) { out.added++; out.addedBytes += file.size; push(out.files.added, { path: file.path, size: file.size }); continue }
    before.delete(file.path)
    const same = old.size === file.size && old.chunks.length === file.chunks.length && old.chunks.every((hash, i) => hash === file.chunks[i])
    if (same) { out.unchanged++; continue }
    out.changed++; out.changedBytes += file.size
    push(out.files.changed, { path: file.path, size: file.size, previousSize: old.size })
  }
  for (const old of before.values()) { out.removed++; out.removedBytes += old.size; push(out.files.removed, { path: old.path, size: old.size }) }
  return out
}

// İki manifest aynı içeriği mi tanımlıyor (yeniden yüklemeye devam kararı için).
export function sameManifest(a: Manifest, b: Manifest) {
  if (a.executable !== b.executable || a.totalSize !== b.totalSize || a.fileCount !== b.fileCount || a.chunkCount !== b.chunkCount) return false
  const index = new Map(a.files.map(file => [file.path, file]))
  for (const file of b.files) {
    const other = index.get(file.path)
    if (!other || other.size !== file.size || other.chunks.length !== file.chunks.length) return false
    for (let i = 0; i < file.chunks.length; i++) if (other.chunks[i] !== file.chunks[i]) return false
  }
  return true
}
