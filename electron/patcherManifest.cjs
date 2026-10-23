// Kütüphane manifest yardımcıları: saf, Electron'suz (vitest'li). Sunucudaki
// server/src/releaseManifest.ts ile aynı kurallar; uyumluluk testi doğrular.
//
// Parça düzeni: dosya kaydında `lengths` varsa parçalar içerik tanımlı (değişken
// boy, en çok CHUNK_SIZE); yoksa eski sabit 4 MB düzen. Kurulu eski manifestler
// bu sayede okunmaya devam eder.

const CHUNK_SIZE = 4 * 1024 * 1024
const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i

function isHash(value) { return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value) }

// Göreli, güvenli yol: mutlak/sürücü/UNC yok, "..".yok, yasak karakter yok,
// parça sonunda nokta/boşluk ve ayrılmış cihaz adı yok.
function safeRelativePath(input) {
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

function chunkCountFor(size) { return Math.ceil(size / CHUNK_SIZE) }

// Bir dosyanın parça aralıkları (offset, length): `lengths` varsa oradan, yoksa sabit.
function chunkRanges(file) {
  if (typeof file === 'number') file = { size: file }
  const ranges = []
  if (Array.isArray(file.lengths)) {
    let offset = 0
    for (const length of file.lengths) { ranges.push({ offset, length }); offset += length }
    return ranges
  }
  for (let offset = 0; offset < file.size; offset += CHUNK_SIZE) ranges.push({ offset, length: Math.min(CHUNK_SIZE, file.size - offset) })
  return ranges
}
function chunkLength(file, index) {
  if (Array.isArray(file.lengths)) return file.lengths[index] || 0
  return Math.max(0, Math.min(CHUNK_SIZE, file.size - index * CHUNK_SIZE))
}

// Proje adından güvenli klasör adı; Windows'un yasak karakterleri atılır.
function projectFolderName(name, suffix) {
  let cleaned = String(name || '').replace(/[\x00-\x1f<>:"/\\|?*]/g, ' ').replace(/\s+/g, ' ').trim().replace(/[. ]+$/, '')
  if (RESERVED.test(cleaned)) cleaned = `_${cleaned}`
  cleaned = cleaned.slice(0, 80).trim() || 'Proje'
  return suffix ? `${cleaned}-${suffix}` : cleaned
}

function sameFile(a, b) {
  if (!a || !b || a.size !== b.size || a.chunks.length !== b.chunks.length) return false
  for (let i = 0; i < a.chunks.length; i++) if (a.chunks[i] !== b.chunks[i]) return false
  return true
}

// Kurulu manifest ile hedef manifesti karşılaştırıp kurulum planı üretir.
// builds: kurulacak/değişen dosyalar; her parça yerel dosyadan (aynı dosyada aynı
// özet → self; başka dosyada → local), yoksa ağdan gelir. Konum bilgisi (offset,
// length) her kaynak için ayrı taşınır: parçalar artık eşit boyda değil.
// deletes: hedefte olmayan dosyalar.
function planInstall(installed, target) {
  const installedFiles = new Map((installed?.files || []).map(file => [file.path, file]))
  // hash -> {path, offset, length}: kurulu dosyalardaki bir parçayı yerelden okumak için.
  const localByHash = new Map()
  for (const file of installed?.files || []) {
    const ranges = chunkRanges(file)
    file.chunks.forEach((hash, index) => { if (!localByHash.has(hash)) localByHash.set(hash, { path: file.path, offset: ranges[index].offset, length: ranges[index].length }) })
  }

  const builds = []
  const downloads = new Set()
  const keep = new Set()
  let bytesToDownload = 0
  let bytesToWrite = 0
  for (const file of target.files) {
    keep.add(file.path)
    const current = installedFiles.get(file.path)
    if (sameFile(current, file)) continue
    const ranges = chunkRanges(file)
    const currentByHash = new Map()
    if (current) { const cr = chunkRanges(current); current.chunks.forEach((hash, index) => { if (!currentByHash.has(hash)) currentByHash.set(hash, cr[index]) }) }
    const chunks = file.chunks.map((hash, index) => {
      const { offset, length } = ranges[index]
      // Aynı parça aynı dosyada (yeri kaymış olsa da) varsa oradan kopyalanır.
      const self = currentByHash.get(hash)
      if (self && self.length === length) return { hash, from: 'self', offset, length, srcOffset: self.offset }
      const local = localByHash.get(hash)
      if (local && local.length === length) return { hash, from: 'local', srcPath: local.path, srcOffset: local.offset, offset, length }
      downloads.add(hash)
      bytesToDownload += length
      return { hash, from: 'net', offset, length }
    })
    bytesToWrite += file.size
    builds.push({ path: file.path, size: file.size, chunks })
  }
  const deletes = (installed?.files || []).map(file => file.path).filter(path => !keep.has(path))
  return { builds, deletes, downloads: [...downloads], bytesToDownload, bytesToWrite }
}

module.exports = { CHUNK_SIZE, isHash, safeRelativePath, chunkCountFor, chunkRanges, chunkLength, projectFolderName, planInstall, sameFile }
