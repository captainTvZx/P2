// Kütüphane (patcher) motoru: build klasörünü parçalayıp yükler, sürümleri
// içerik adresli parçalarla indirir/günceller, oyunu başlatır. Ağır iş (hash,
// indirme, disk yazımı) burada, ana süreçte yapılır.
const { app, BrowserWindow, Notification, dialog, ipcMain, nativeImage, net, shell, screen } = require('electron')
const crypto = require('node:crypto')
const fs = require('node:fs')
const fsp = require('node:fs/promises')
const path = require('node:path')
const { P2_MAVI_CSS } = require('./p2-mavi.cjs')
const { spawn } = require('node:child_process')
const { CHUNK_SIZE, isHash, safeRelativePath, chunkRanges, projectFolderName, planInstall } = require('./patcherManifest.cjs')
const { Chunker } = require('./cdc.cjs')
let overlayHost = null
try { overlayHost = require('./overlayHost.cjs') } catch {}

const EXCLUDE = [/\.pdb$/i, /\.ilk$/i, /\.exp$/i, /\.lib$/i, /\.map$/i, /(^|\/)crashes\//i, /(^|\/)\.p2\//i]
const UPLOAD_PARALLEL = 4
const DOWNLOAD_PARALLEL = 4

// ---- durum / kimlik ----
let apiBase = ''
let accessToken = ''
let refreshToken = ''
let getMainWindow = () => null
const jobs = new Map()        // projectId -> job state
const uploadsRunning = new Map() // requestId -> { controller }
const running = new Map()      // projectId -> ChildProcess
const limiter = { kbps: 0, tokens: 0, last: Date.now() }

function libDir() { return path.join(app.getPath('userData'), 'library') }
function libFile() { return path.join(libDir(), 'library.json') }
function uploadsDir() { return path.join(libDir(), 'uploads') }
function hashCacheFile() { return path.join(libDir(), 'hash-cache.json') }
function overlayLog() { return path.join(app.getPath('appData'), 'P2', 'overlay.log') }

function logLine(message) { try { fs.appendFileSync(path.join(libDir(), 'library.log'), `[${new Date().toISOString()}] ${message}\n`) } catch {} }

function emit(event) {
  for (const window of BrowserWindow.getAllWindows()) {
    if (!window.isDestroyed()) window.webContents.send('library:event', event)
  }
}

// ---- library.json ----
let library = { version: 1, deviceId: '', root: '', projects: {} }
function loadLibrary() {
  try { library = { version: 1, deviceId: '', root: '', projects: {}, ...JSON.parse(fs.readFileSync(libFile(), 'utf8')) } } catch {}
  if (!library.deviceId) { library.deviceId = crypto.randomUUID(); saveLibrary() }
  return library
}
function saveLibrary() {
  try {
    fs.mkdirSync(libDir(), { recursive: true })
    const tmp = libFile() + '.tmp'
    fs.writeFileSync(tmp, JSON.stringify(library))
    fs.renameSync(tmp, libFile())
    emit({ type: 'state' }) // renderer kurulu sürüm / klasör değişimini anında görsün
  } catch (error) { logLine(`library.json yazılamadı: ${error?.message || error}`) }
}

// ---- kimlik / kimlik doğrulama ----
function tokenExp(token) { try { return JSON.parse(Buffer.from(token.split('.')[1], 'base64').toString()).exp * 1000 } catch { return 0 } }
async function ensureToken() {
  if (!accessToken) throw new Error('AUTH_REQUIRED')
  if (tokenExp(accessToken) - Date.now() > 120_000) return accessToken
  if (!refreshToken) { emit({ type: 'auth-required' }); throw new Error('AUTH_REQUIRED') }
  try {
    const response = await net.fetch(`${apiBase}/auth/refresh`, { method: 'POST', headers: { 'Content-Type': 'application/json', ...(app.getVersion() ? { 'X-P2-Client-Version': app.getVersion() } : {}) }, body: JSON.stringify({ refreshToken }) })
    if (!response.ok) throw new Error('refresh failed')
    const data = await response.json()
    accessToken = data.accessToken || accessToken
    if (data.refreshToken) refreshToken = data.refreshToken
    emit({ type: 'auth', accessToken })
    return accessToken
  } catch { emit({ type: 'auth-required' }); throw new Error('AUTH_REQUIRED') }
}
async function authHeaders(extra = {}) { return { Authorization: `Bearer ${await ensureToken()}`, ...extra } }

async function apiJson(url, init = {}) {
  const response = await net.fetch(apiBase + url, { ...init, headers: await authHeaders(init.headers || {}) })
  const data = await response.json().catch(() => ({}))
  if (response.status === 401) { await ensureToken(); throw new Error('RETRY') }
  if (!response.ok) { const error = new Error(data.message || `HTTP ${response.status}`); error.code = data.code; error.status = response.status; throw error }
  return data
}

// ---- hız sınırı ----
async function throttle(bytes) {
  if (!limiter.kbps) return
  const now = Date.now()
  limiter.tokens += ((now - limiter.last) / 1000) * limiter.kbps * 1024
  limiter.last = now
  const cap = limiter.kbps * 1024
  if (limiter.tokens > cap) limiter.tokens = cap
  if (limiter.tokens < bytes) {
    const wait = ((bytes - limiter.tokens) / (limiter.kbps * 1024)) * 1000
    await new Promise(resolve => setTimeout(resolve, Math.min(5000, wait)))
    limiter.tokens = 0; limiter.last = Date.now()
  } else limiter.tokens -= bytes
}

// ---- hash önbelleği ----
let hashCache = null
function loadHashCache() { if (hashCache) return hashCache; try { hashCache = JSON.parse(fs.readFileSync(hashCacheFile(), 'utf8')) } catch { hashCache = {} } return hashCache }
function saveHashCache() { try { const entries = Object.entries(hashCache); if (entries.length > 20000) hashCache = Object.fromEntries(entries.slice(-20000)); fs.writeFileSync(hashCacheFile(), JSON.stringify(hashCache)) } catch {} }

// Bir dosyayı içerik tanımlı parçalara (1-4 MB, ortalama 2 MB) bölüp her parçanın
// sha256'sını ve uzunluğunu üretir. Kesimler içerikten geldiği için dosyanın bir
// yerine bayt girmesi yalnız o civardaki parçayı değiştirir (küçük yamalar).
const READ_BLOCK = 8 * 1024 * 1024
async function hashFile(file) {
  const info = await fsp.stat(file)
  const chunks = [], lengths = []
  const push = (piece) => { chunks.push(crypto.createHash('sha256').update(piece).digest('hex')); lengths.push(piece.length) }
  const handle = await fsp.open(file, 'r')
  try {
    const chunker = new Chunker()
    const block = Buffer.allocUnsafe(READ_BLOCK)
    for (let offset = 0; offset < info.size; offset += READ_BLOCK) {
      const { bytesRead } = await handle.read(block, 0, Math.min(READ_BLOCK, info.size - offset), offset)
      for (const piece of chunker.feed(Buffer.from(block.subarray(0, bytesRead)))) push(piece)
    }
    for (const piece of chunker.finish()) push(piece)
  } finally { await handle.close() }
  return { size: info.size, chunks, lengths }
}
// Kurulu dosyayı manifestteki düzene göre (parça uzunluklarıyla) özetler: doğrulama için.
async function hashFileLike(file, layout) {
  const handle = await fsp.open(file, 'r')
  const chunks = []
  try {
    for (const { offset, length } of chunkRanges(layout)) {
      const buffer = Buffer.allocUnsafe(length)
      const { bytesRead } = await handle.read(buffer, 0, length, offset)
      chunks.push(crypto.createHash('sha256').update(buffer.subarray(0, bytesRead)).digest('hex'))
    }
  } finally { await handle.close() }
  return { chunks }
}

async function walkFiles(dir, base = dir, out = []) {
  let entries
  try { entries = await fsp.readdir(dir, { withFileTypes: true }) } catch { return out }
  for (const entry of entries) {
    if (entry.isSymbolicLink()) continue
    const full = path.join(dir, entry.name)
    const rel = path.relative(base, full).replace(/\\/g, '/')
    if (EXCLUDE.some(pattern => pattern.test(rel + (entry.isDirectory() ? '/' : '')))) continue
    if (entry.isDirectory()) await walkFiles(full, base, out)
    else if (entry.isFile()) out.push({ full, rel })
  }
  return out
}

// Klasörü manifest dosyalarına çevirir; değişmeyen dosyalar önbellekten gelir.
async function hashFolder(dir, { signal, onProgress } = {}) {
  const cache = loadHashCache()
  const found = await walkFiles(dir)
  const files = []
  let doneBytes = 0
  let totalBytes = 0
  for (const item of found) { try { totalBytes += (await fsp.stat(item.full)).size } catch {} }
  for (const item of found) {
    if (signal?.aborted) throw new Error('CANCELED')
    const info = await fsp.stat(item.full)
    const key = `${dir}|${item.rel}|${info.size}|${info.mtimeMs}`
    let entry = cache[key]
    // Eski (sabit dilimli, `lengths`siz) önbellek kayıtları yeniden hesaplanır.
    if (!entry || !Array.isArray(entry.lengths) || entry.lengths.length !== entry.chunks.length) { entry = await hashFile(item.full); cache[key] = entry }
    const safe = safeRelativePath(item.rel)
    if (!safe) throw new Error(`Geçersiz dosya yolu: ${item.rel}`)
    files.push({ path: safe, size: entry.size, chunks: entry.chunks, lengths: entry.lengths })
    doneBytes += info.size
    onProgress?.({ doneBytes, totalBytes })
  }
  saveHashCache()
  return files
}

// ---- yükleme ----
async function startUpload(payload) {
  const { requestId, organizationId, projectId, projectName, sourceDir, executable, launchArgs, channel, version, notes, overlay } = payload
  const controller = new AbortController()
  uploadsRunning.set(requestId, { controller })
  const send = (patch) => emit({ type: 'upload', requestId, releaseId: patch.releaseId, projectId, channel, version, ...patch })
  try {
    send({ phase: 'scanning', percent: 0 })
    const files = await hashFolder(sourceDir, { signal: controller.signal, onProgress: ({ doneBytes, totalBytes }) => send({ phase: 'hashing', percent: totalBytes ? Math.round((doneBytes / totalBytes) * 100) : 0, bytesDone: doneBytes, bytesTotal: totalBytes }) })
    if (!files.some(file => file.path === safeRelativePath(executable))) throw new Error('Başlatılacak dosya klasörde bulunamadı.')
    send({ phase: 'registering', percent: 100 })
    let created
    for (let attempt = 0; attempt < 2; attempt++) {
      try { created = await apiJson(`/organizations/${organizationId}/projects/${projectId}/releases`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ channel, version, notes, executable: safeRelativePath(executable), launchArgs, overlay, files }) }); break }
      catch (error) { if (error.message === 'RETRY' && attempt === 0) continue; throw error }
    }
    const releaseId = created.release.id
    // Durum dosyası: yeniden başlatmada devam.
    // hash -> [yol, offset, uzunluk]; parçalar değişken boylu olduğundan konum saklanır.
    const chunkIndex = {}
    for (const file of files) { const ranges = chunkRanges(file); file.chunks.forEach((hash, index) => { if (!chunkIndex[hash]) chunkIndex[hash] = [file.path, ranges[index].offset, ranges[index].length] }) }
    const state = { version: 1, releaseId, organizationId, projectId, projectName, apiBase, sourceDir, channel, releaseVersion: version, executable, notes, overlay, createdAt: Date.now(), chunkIndex, pending: created.missing.slice() }
    await fsp.mkdir(uploadsDir(), { recursive: true })
    await fsp.writeFile(path.join(uploadsDir(), `${releaseId}.json`), JSON.stringify(state))
    await runUploadJob(state, controller, send)
  } catch (error) {
    if (error.message === 'CANCELED') send({ phase: 'canceled' })
    else { logLine(`yükleme hatası: ${error?.stack || error}`); send({ phase: 'failed', error: error.message, code: error.code }) }
    throw error
  } finally { uploadsRunning.delete(requestId) }
}

async function runUploadJob(state, controller, send) {
  const { organizationId, projectId, releaseId, sourceDir } = state
  const total = Object.keys(state.chunkIndex).length
  let done = total - state.pending.length
  let bytesTotal = 0, bytesDone = 0
  const sizeOf = (hash) => Math.min(CHUNK_SIZE, 1) // gerçek boyut parça okunurken bilinir
  const queue = state.pending.slice()
  let persistAt = Date.now()
  const persist = async (force) => { if (force || Date.now() - persistAt > 2000) { persistAt = Date.now(); await fsp.writeFile(path.join(uploadsDir(), `${releaseId}.json`), JSON.stringify(state)).catch(() => {}) } }

  async function uploadOne(hash) {
    const entry = state.chunkIndex[hash]
    // Eski durum dosyaları [yol, indeks] taşır (sabit 4 MB düzen); yenileri [yol, offset, uzunluk].
    const rel = entry[0]
    const file = path.join(sourceDir, rel)
    for (let attempt = 0; attempt < 5; attempt++) {
      if (controller.signal.aborted) throw new Error('CANCELED')
      try {
        const handle = await fsp.open(file, 'r')
        let buffer
        try {
          const info = await handle.stat()
          const offset = entry.length === 3 ? entry[1] : entry[1] * CHUNK_SIZE
          const length = entry.length === 3 ? entry[2] : Math.min(CHUNK_SIZE, info.size - offset)
          buffer = Buffer.allocUnsafe(length)
          await handle.read(buffer, 0, length, offset)
        } finally { await handle.close() }
        await throttle(buffer.length)
        const response = await net.fetch(`${apiBase}/organizations/${organizationId}/projects/${projectId}/releases/${releaseId}/chunks/${hash}`, {
          method: 'PUT', headers: await authHeaders({ 'Content-Type': 'application/octet-stream' }), body: buffer, signal: controller.signal,
        })
        if (response.status === 401) { await ensureToken(); continue }
        if (response.status === 422) { if (attempt < 1) continue; throw new Error(`Parça içeriği değişti: ${rel}`) }
        if (!response.ok) throw new Error(`HTTP ${response.status}`)
        bytesDone += buffer.length
        return
      } catch (error) {
        if (controller.signal.aborted) throw new Error('CANCELED')
        if (attempt === 4) throw error
        await new Promise(resolve => setTimeout(resolve, Math.min(16000, 1000 * 2 ** attempt)))
      }
    }
  }

  // Paralel havuz
  let cursor = 0
  const workers = Array.from({ length: UPLOAD_PARALLEL }, async () => {
    while (cursor < queue.length) {
      const hash = queue[cursor++]
      await uploadOne(hash)
      state.pending = state.pending.filter(h => h !== hash)
      done++
      send({ phase: 'uploading', releaseId, percent: total ? Math.round((done / total) * 100) : 100, done, total, bytesDone, bytesTotal })
      await persist(false)
    }
  })
  await Promise.all(workers)
  await persist(true)
  // Doğrulama: sunucuya göre hâlâ eksik var mı?
  const check = await apiJson(`/organizations/${organizationId}/projects/${projectId}/releases/${releaseId}/missing`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' })
  if (check.missing.length) { state.pending = check.missing; await persist(true); send({ phase: 'uploading', releaseId, percent: Math.round(((check.total - check.missing.length) / check.total) * 100), done: check.total - check.missing.length, total: check.total }); return runUploadJob(state, controller, send) }
  // Tamamlanan yüklemenin devam dosyası kalmasın; yoksa "devam eden yüklemeler"de 601/601 diye görünür.
  await fsp.rm(path.join(uploadsDir(), `${releaseId}.json`), { force: true }).catch(() => {})
  send({ phase: 'ready', releaseId, percent: 100 })
}

async function listUploads() {
  try {
    const names = await fsp.readdir(uploadsDir())
    const out = []
    for (const name of names.filter(n => n.endsWith('.json'))) {
      try {
        const state = JSON.parse(await fsp.readFile(path.join(uploadsDir(), name), 'utf8'))
        // Eksik parçası kalmamış kayıt eski bir sürümden artık; devam edilecek bir şey yok.
        if (!state.pending.length) { await fsp.rm(path.join(uploadsDir(), name), { force: true }).catch(() => {}); continue }
        if (fs.existsSync(state.sourceDir)) out.push({ releaseId: state.releaseId, projectId: state.projectId, projectName: state.projectName, channel: state.channel, version: state.releaseVersion, pending: state.pending.length, total: Object.keys(state.chunkIndex).length })
      } catch {}
    }
    return out
  } catch { return [] }
}

// ---- kurulum / indirme ----
function projectDir(projectName, existingFolder) { return path.join(library.root, existingFolder || projectFolderName(projectName)) }
function metaDir(dir) { return path.join(dir, '.p2') }

async function readInstalledManifest(dir) { try { return JSON.parse(await fsp.readFile(path.join(metaDir(dir), 'manifest.json'), 'utf8')) } catch { return null } }

async function freeSpace(dir) {
  try { const stats = await fsp.statfs(dir); return stats.bavail * stats.bsize } catch { return Number.MAX_SAFE_INTEGER }
}

async function installRelease(payload) {
  const { organizationId, projectId, projectName, releaseId, channel } = payload
  if (!library.root) throw new Error('NO_ROOT')
  if (jobs.get(projectId)?.active) return
  const existing = library.projects[projectId]
  // Başarısız kurulumun klasörü hatırlanır; "yeniden dene" her seferinde yeni klasör açmasın.
  const folder = existing?.folder || pendingFolders.get(projectId) || ensureFolder(projectName)
  pendingFolders.set(projectId, folder)
  const dir = projectDir(projectName, folder)
  const job = { projectId, releaseId, active: true, phase: 'preparing', done: 0, total: 0, bytesDone: 0, bytesTotal: 0, bytesWritten: 0, bytesToWrite: 0, speed: 0, diskSpeed: 0, eta: 0, canceled: false, paused: false }
  jobs.set(projectId, job)
  const send = () => emit({ type: 'job', ...job })
  // Parça başına olay yerine en çok 4 Hz; hızlar saniyede bir yumuşatılarak hesaplanır.
  let lastSent = 0
  const tick = () => { const now = Date.now(); if (now - lastSent >= 250) { lastSent = now; send() } }
  let sample = { at: Date.now(), down: 0, disk: 0 }
  const sampler = setInterval(() => {
    const now = Date.now(), dt = Math.max(0.2, (now - sample.at) / 1000)
    const down = (job.bytesDone - sample.down) / dt, disk = (job.bytesWritten - sample.disk) / dt
    job.speed = job.paused ? 0 : Math.round(job.speed ? job.speed * 0.6 + down * 0.4 : down)
    job.diskSpeed = Math.round(job.diskSpeed ? job.diskSpeed * 0.6 + disk * 0.4 : disk)
    job.eta = job.phase === 'downloading' && job.speed > 0 ? Math.round((job.bytesTotal - job.bytesDone) / job.speed) : 0
    sample = { at: now, down: job.bytesDone, disk: job.bytesWritten }
    send()
  }, 1000)
  send()
  try {
    const manifest = await apiJson(`/organizations/${organizationId}/projects/${projectId}/releases/${releaseId}/manifest`)
    const installed = await readInstalledManifest(dir)
    const plan = planInstall(installed, manifest)
    job.total = plan.downloads.length; job.bytesTotal = plan.bytesToDownload; job.bytesToWrite = plan.bytesToWrite
    // Disk alanı
    const free = await freeSpace(library.root)
    if (free < plan.bytesToDownload + plan.bytesToWrite + 256 * 1024 * 1024) { job.phase = 'failed'; job.error = 'Diskte yeterli alan yok.'; job.code = 'DISK_FULL'; job.active = false; send(); return }
    await fsp.mkdir(metaDir(dir), { recursive: true })
    const chunkCacheDir = path.join(metaDir(dir), 'chunks')
    await fsp.mkdir(chunkCacheDir, { recursive: true })
    await markInstalled(organizationId, projectId, releaseId, 'UPDATING')
    // İndirme
    job.phase = 'downloading'; send()
    const downloads = plan.downloads.slice()
    let cursor = 0
    const downloadOne = async (hash) => {
      const cachePath = path.join(chunkCacheDir, hash)
      if (fs.existsSync(cachePath)) return
      for (let attempt = 0; attempt < 3; attempt++) {
        if (job.canceled) throw new Error('CANCELED')
        while (job.paused) { await new Promise(resolve => setTimeout(resolve, 400)); if (job.canceled) throw new Error('CANCELED') }
        try {
          await throttle(CHUNK_SIZE)
          // Parça ucu sürümden bağımsızdır (içerik adresli): /releases/chunks/:hash
          const response = await net.fetch(`${apiBase}/organizations/${organizationId}/projects/${projectId}/releases/chunks/${hash}`, { headers: await authHeaders() })
          if (response.status === 401) { await ensureToken(); continue }
          if (!response.ok) throw new Error(`HTTP ${response.status}`)
          const buffer = Buffer.from(await response.arrayBuffer())
          if (crypto.createHash('sha256').update(buffer).digest('hex') !== hash) throw new Error('CHUNK_HASH_MISMATCH')
          const tmp = cachePath + '.part'
          await fsp.writeFile(tmp, buffer)
          await fsp.rename(tmp, cachePath)
          job.bytesDone += buffer.length; job.bytesWritten += buffer.length; job.done++; tick()
          return
        } catch (error) { if (job.canceled) throw new Error('CANCELED'); if (attempt === 2) throw error }
      }
    }
    const workers = Array.from({ length: DOWNLOAD_PARALLEL }, async () => { while (cursor < downloads.length) { await downloadOne(downloads[cursor++]) } })
    await Promise.all(workers)
    // Oyun çalışıyorsa montajı bekle
    if (running.has(projectId)) { job.phase = 'waiting-for-exit'; send(); await new Promise(resolve => { const child = running.get(projectId); if (!child) return resolve(); child.once('exit', resolve) }) }
    // Montaj
    job.phase = 'installing'; send()
    for (const build of plan.builds) {
      const outPath = path.join(dir, build.path.replace(/\//g, path.sep))
      const resolved = path.resolve(outPath)
      if (!resolved.startsWith(path.resolve(dir) + path.sep)) throw new Error(`Güvenli olmayan yol: ${build.path}`)
      await fsp.mkdir(path.dirname(outPath), { recursive: true })
      const tmp = outPath + '.p2new'
      const handle = await fsp.open(tmp, 'w')
      try {
        for (const chunk of build.chunks) {
          let data
          if (chunk.from === 'net') data = await fsp.readFile(path.join(chunkCacheDir, chunk.hash))
          else if (chunk.from === 'local') { const src = await fsp.open(path.join(dir, chunk.srcPath.replace(/\//g, path.sep)), 'r'); try { data = Buffer.allocUnsafe(chunk.length); await src.read(data, 0, chunk.length, chunk.srcOffset) } finally { await src.close() } }
          else { const src = await fsp.open(outPath, 'r').catch(() => null); if (src) { try { data = Buffer.allocUnsafe(chunk.length); await src.read(data, 0, chunk.length, chunk.srcOffset) } finally { await src.close() } } else data = await fsp.readFile(path.join(chunkCacheDir, chunk.hash)).catch(() => Buffer.alloc(0)) }
          if (crypto.createHash('sha256').update(data).digest('hex') !== chunk.hash) throw new Error(`Parça doğrulanamadı: ${build.path}`)
          await handle.write(data); job.bytesWritten += data.length; tick()
        }
        await handle.sync()
      } finally { await handle.close() }
      for (let attempt = 0; attempt < 5; attempt++) { try { await fsp.rename(tmp, outPath); break } catch (error) { if (attempt === 4) throw new Error(`Dosya değiştirilemedi (oyun açık olabilir): ${build.path}`); await new Promise(resolve => setTimeout(resolve, 600)) } }
    }
    // Silinenler
    for (const rel of plan.deletes) { try { await fsp.rm(path.join(dir, rel.replace(/\//g, path.sep)), { force: true }) } catch {} }
    await pruneEmptyDirs(dir)
    // Manifesti yaz
    const tmpManifest = path.join(metaDir(dir), 'manifest.json.tmp')
    await fsp.writeFile(tmpManifest, JSON.stringify({ ...manifest, releaseId, channel, installedAt: Date.now() }))
    await fsp.rename(tmpManifest, path.join(metaDir(dir), 'manifest.json'))
    await fsp.rm(chunkCacheDir, { recursive: true, force: true }).catch(() => {})
    // pinned: belirli bir sürüm elle seçildi (canlıya otomatik geçmez); canlı sürüm kurulunca sabit kalkar.
    const pinned = payload.pinned !== undefined ? Boolean(payload.pinned) : (existing?.pinned || false)
    library.projects[projectId] = { folder, channel, releaseId, version: manifest.version, installedAt: Date.now(), pinned, executable: manifest.executable, launchArgs: manifest.launchArgs || '', userArgs: existing?.userArgs || '', playSeconds: existing?.playSeconds || 0, lastPlayedAt: existing?.lastPlayedAt }
    pendingFolders.delete(projectId)
    saveLibrary()
    await markInstalled(organizationId, projectId, releaseId, 'INSTALLED', { pinned })
    job.phase = 'done'; job.active = false; send()
  } catch (error) {
    job.active = false
    if (error.message === 'CANCELED') { job.phase = 'canceled'; send() }
    else { logLine(`kurulum hatası: ${error?.stack || error}`); job.phase = 'failed'; job.error = error.message; job.code = error.code; send(); await markInstalled(organizationId, projectId, releaseId, 'FAILED').catch(() => {}) }
  } finally { clearInterval(sampler); if (!jobs.get(projectId)?.active) setTimeout(() => { if (!jobs.get(projectId)?.active) jobs.delete(projectId) }, 8000) }
}

const pendingFolders = new Map() // projectId -> klasör (kurulum tamamlanana kadar)
function ensureFolder(projectName) {
  let folder = projectFolderName(projectName)
  const taken = new Set(Object.values(library.projects).map(p => p.folder))
  // Diskte duran ama hiçbir projeye kayıtlı olmayan ve kurulumu bitmemiş (.p2/manifest.json yok)
  // klasör, önceki başarısız denemenin artığıdır; yeniden kullanılır.
  const leftover = !taken.has(folder) && fs.existsSync(path.join(library.root, folder)) && !fs.existsSync(path.join(library.root, folder, '.p2', 'manifest.json'))
  if (taken.has(folder) || (fs.existsSync(path.join(library.root, folder)) && !leftover)) {
    let suffix = crypto.randomUUID().slice(0, 8)
    folder = projectFolderName(projectName, suffix)
  }
  return folder
}

async function pruneEmptyDirs(dir) {
  async function walk(current) {
    let entries
    try { entries = await fsp.readdir(current, { withFileTypes: true }) } catch { return }
    for (const entry of entries) { if (entry.isDirectory() && entry.name !== '.p2') await walk(path.join(current, entry.name)) }
    if (current !== dir) { try { if (!(await fsp.readdir(current)).length) await fsp.rmdir(current) } catch {} }
  }
  await walk(dir)
}

async function markInstalled(organizationId, projectId, releaseId, status, extra = {}) {
  try { await apiJson(`/organizations/${organizationId}/projects/${projectId}/releases/${releaseId}/installed`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ deviceId: library.deviceId, status, clientVersion: app.getVersion(), ...extra }) }) } catch {}
}

// ---- doğrula / kaldır / başlat ----
async function verifyInstall(payload) {
  const info = library.projects[payload.projectId]
  if (!info) return { ok: false }
  const dir = projectDir(payload.projectName, info.folder)
  const manifest = await readInstalledManifest(dir)
  if (!manifest) return { ok: false }
  // Bozuk dosyaları eksik sayıp yeniden kur
  const broken = []
  for (const file of manifest.files) {
    const full = path.join(dir, file.path.replace(/\//g, path.sep))
    try { const result = await hashFileLike(full, file); if (result.chunks.length !== file.chunks.length || result.chunks.some((h, i) => h !== file.chunks[i])) broken.push(file.path) } catch { broken.push(file.path) }
  }
  if (!broken.length) return { ok: true, repaired: 0 }
  // Bozuk dosyaları manifest'ten düşerek planInstall yeniden indirir
  const reduced = { ...manifest, files: manifest.files.filter(file => !broken.includes(file.path)) }
  await fsp.writeFile(path.join(metaDir(dir), 'manifest.json'), JSON.stringify(reduced))
  await installRelease({ organizationId: payload.organizationId, projectId: payload.projectId, projectName: payload.projectName, releaseId: info.releaseId, channel: info.channel })
  return { ok: true, repaired: broken.length }
}

async function uninstall(payload) {
  if (running.has(payload.projectId)) return { ok: false, error: 'Oyun açıkken kaldırılamaz.' }
  const info = library.projects[payload.projectId]
  if (info) { try { await fsp.rm(projectDir(payload.projectName, info.folder), { recursive: true, force: true }) } catch {} ; if (info.releaseId) await markInstalled(payload.organizationId, payload.projectId, info.releaseId, 'REMOVED') }
  delete library.projects[payload.projectId]
  saveLibrary()
  emit({ type: 'state' })
  return { ok: true }
}

function onGameExit(payload, info, dir, started, code, pid) {
  running.delete(payload.projectId)
  closeSplash(payload.projectId)
  if (pid && overlayHost) { try { overlayHost.stopForGame(pid) } catch {} }
  const seconds = Math.max(0, Math.round((Date.now() - started) / 1000))
  const entry = library.projects[payload.projectId]
  if (entry) { entry.playSeconds = (entry.playSeconds || 0) + seconds; entry.lastPlayedAt = Date.now(); saveLibrary() }
  emit({ type: 'running', projectId: payload.projectId, running: false })
  emit({ type: 'played', projectId: payload.projectId, seconds })
  void markInstalled(payload.organizationId, payload.projectId, info.releaseId, 'INSTALLED', { launched: true, playSeconds: seconds })
  if (code) emit({ type: 'crash', projectId: payload.projectId, projectName: payload.projectName, code, seconds: Math.round((Date.now() - started) / 1000), log: findLatestLog(dir) })
  const w = getMainWindow(); if (w && !w.isDestroyed()) { if (w.isMinimized()) w.restore(); w.show(); w.focus() }
}

// Enjektör ayrı bir süreç başlattığı için oyunun kendi pid'i izlenir (ChildProcess yok).
function watchPid(payload, info, dir, started, pid) {
  running.set(payload.projectId, { pid })
  // Kök süreç başlatıcıysa (Unreal) erken kapanabilir; overlay ağaçtaki asıl oyunu izlemeye devam eder.
  const alive = () => { try { process.kill(pid, 0); return true } catch { return Boolean(overlayHost && overlayHost.treeAlive(pid)) } }
  const timer = setInterval(() => {
    if (!alive()) { clearInterval(timer); onGameExit(payload, info, dir, started, 0, pid) }
  }, 2000)
  if (timer.unref) timer.unref()
}

function plainLaunch(payload, info, exe, dir, args, started) {
  const child = spawn(exe, args, { cwd: dir, detached: true, stdio: 'ignore', windowsHide: false })
  running.set(payload.projectId, child)
  emit({ type: 'running', projectId: payload.projectId, running: true })
  // Overlay yok (anti-cheat, desteklenmeyen API ya da ikili eksik): Windows bildirimiyle haber ver.
  setTimeout(() => { closeSplash(payload.projectId); desktopNotice(`${payload.projectName} başlatıldı`, 'Bu oyunda P2 overlay kullanılamıyor; oyun normal çalışıyor.') }, 4000)
  const main = getMainWindow()
  if (main && !main.isDestroyed()) main.minimize()
  child.on('error', (error) => { closeSplash(payload.projectId); desktopNotice(`${payload.projectName} başlatılamadı`, error.message); running.delete(payload.projectId); emit({ type: 'running', projectId: payload.projectId, running: false }) })
  child.on('exit', (code) => onGameExit(payload, info, dir, started, code, null))
  child.unref()
}

// Çalışan oyunu kapat: Windows'ta ayrık (detached) süreç ağacı taskkill ile düşürülür.
function stopProject(payload) {
  const entry = running.get(payload.projectId)
  if (!entry) return { ok: false, error: 'Çalışmıyor.' }
  const pid = typeof entry.pid === 'number' ? entry.pid : null
  try {
    if (process.platform === 'win32' && pid) spawn('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true }).unref()
    else if (typeof entry.kill === 'function') entry.kill()
    else if (pid) process.kill(pid)
  } catch (error) { return { ok: false, error: error.message } }
  return { ok: true }
}

// P2'den bağımsız, ekranın ortasında küçük "başlatılıyor" penceresi (Steam'in Launching… kutusu gibi).
// Oyun ilk karesini çizince (overlay 'hazır' sinyali) kapanır; emniyet için 20 sn sonra kendiliğinden, hata/çıkışta hemen.
const splashes = new Map() // projectId -> BrowserWindow
function escapeHtml(value) { return String(value || '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])) }
function closeSplash(projectId) { const w = splashes.get(projectId); splashes.delete(projectId); if (w && !w.isDestroyed()) w.close() }
function showLaunchSplash(payload) {
  try {
    closeSplash(payload.projectId)
    const display = screen.getPrimaryDisplay().workArea
    const width = 440, height = 150
    const win = new BrowserWindow({
      width, height, x: Math.round(display.x + (display.width - width) / 2), y: Math.round(display.y + (display.height - height) / 2),
      frame: false, transparent: true, resizable: false, movable: false, minimizable: false, maximizable: false, skipTaskbar: true, alwaysOnTop: true, focusable: false, show: false,
      webPreferences: { sandbox: true, contextIsolation: true },
    })
    win.setAlwaysOnTop(true, 'screen-saver')
    const initials = escapeHtml(String(payload.projectName || 'P').trim().split(/\s+/).slice(0, 2).map(w => w[0]).join('').toUpperCase())
    const icon = payload.iconUrl ? `<img src="${escapeHtml(payload.iconUrl)}" alt="">` : `<span class="ini">${initials}</span>`
    // Etkin tema: arayüz başlatma isteğiyle --p2-* token'larını yollar; P2 Mavi dışındaki temalarda kart aynı renklerde çizilir.
    const themeVars = payload.theme && typeof payload.theme === 'object' ? Object.entries(payload.theme).filter(([key, value]) => /^--p2-[a-z0-9-]+$/.test(key) && typeof value === 'string' && value.length < 80 && !/[;{}<>]/.test(value)).map(([key, value]) => `${key}:${value}`).join(';') : ''
    const html = `<!doctype html><html lang="tr"${payload.themeMode === 'light' ? ' data-theme="light"' : ''}><head><meta charset="utf-8"><style>${P2_MAVI_CSS}
      html{${themeVars}}
      html,body{margin:0;height:100%;background:transparent;overflow:hidden;-webkit-user-select:none}
      .card{box-sizing:border-box;height:100%;display:grid;grid-template-columns:78px minmax(0,1fr);gap:18px;align-items:center;padding:18px 24px;border-radius:var(--p2-r-lg)}
      .ico{width:78px;height:78px;border-radius:18px;overflow:hidden;display:grid;place-items:center;background:var(--p2-accent-grad);box-shadow:0 10px 30px rgba(20,90,220,.4),var(--p2-inset-top);animation:p2-logo-pop .6s var(--ease-pop) .1s both}
      .ico img{width:100%;height:100%;object-fit:cover}.ini{font-size:26px;font-weight:800;letter-spacing:.04em;color:#fff}
      .txt{min-width:0}.eyebrow{margin:0 0 4px}
      h1{margin:0;color:var(--p2-text-strong);font-size:19px;font-weight:800;letter-spacing:-.01em;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
      .bar{margin-top:12px}
    </style></head><body><div class="card p2-card shine p2-in"><div class="ico">${icon}</div><div class="txt"><p class="eyebrow p2-kicker">P2 · Başlatılıyor</p><h1>${escapeHtml(payload.projectName)}</h1><div class="bar p2-bar indeterminate"><i></i></div></div></div></body></html>`
    win.loadURL('data:text/html;charset=UTF-8,' + encodeURIComponent(html))
    win.once('ready-to-show', () => { if (!win.isDestroyed()) win.showInactive() })
    win.on('closed', () => { if (splashes.get(payload.projectId) === win) splashes.delete(payload.projectId) })
    splashes.set(payload.projectId, win)
    setTimeout(() => { if (splashes.get(payload.projectId) === win) closeSplash(payload.projectId) }, 20000)
  } catch (error) { logLine(`başlatma ekranı açılamadı: ${error.message}`) }
}
// Windows bildirimi (overlay yoksa oyunun başladığını yine de duyurur).
function desktopNotice(title, body) {
  try { if (Notification.isSupported()) new Notification({ title, body, silent: true }).show() } catch {}
}

function launchProject(payload) {
  const info = library.projects[payload.projectId]
  if (!info) return { ok: false, error: 'Kurulu değil.' }
  if (jobs.get(payload.projectId)?.phase === 'installing') return { ok: false, error: 'Kurulum sürüyor.' }
  const dir = projectDir(payload.projectName, info.folder)
  const exe = path.join(dir, info.executable.replace(/\//g, path.sep))
  if (!fs.existsSync(exe)) return { ok: false, error: 'Çalıştırılabilir dosya bulunamadı.' }
  const args = [...splitArgs(info.launchArgs), ...splitArgs(info.userArgs)]
  const started = Date.now()
  showLaunchSplash(payload)
  // Overlay ikilileri varsa enjekte ederek başlat; yoksa (varsayılan/shipped) düz başlat.
  if (overlayHost && overlayHost.available() && info.overlay !== false && payload.overlay !== false) {
    emit({ type: 'running', projectId: payload.projectId, running: true })
    const main = getMainWindow(); if (main && !main.isDestroyed()) main.minimize()
    const noOverlay = () => { closeSplash(payload.projectId); desktopNotice(`${payload.projectName} çalışıyor`, 'Bu oyunda P2 overlay bağlanamadı; oyun normal çalışıyor.') }
    Promise.resolve(overlayHost.startForGame({ exePath: exe, args, cwd: dir, projectId: payload.projectId, projectName: payload.projectName, onReady: () => closeSplash(payload.projectId), onNoOverlay: noOverlay }))
      .then(result => {
        if (!result || !result.pid) { plainLaunch(payload, info, exe, dir, args, started); return }
        watchPid(payload, info, dir, started, result.pid)
        if (result.overlay === false) setTimeout(noOverlay, 4000)
      })
      .catch(() => plainLaunch(payload, info, exe, dir, args, started))
    return { ok: true }
  }
  plainLaunch(payload, info, exe, dir, args, started)
  return { ok: true }
}
function splitArgs(value) { return String(value || '').match(/"[^"]*"|\S+/g)?.map(a => a.replace(/^"|"$/g, '')) || [] }
function findLatestLog(dir) {
  const candidates = []
  for (const sub of ['', 'Saved/Logs', 'Logs']) { try { const base = path.join(dir, sub); for (const name of fs.readdirSync(base)) if (/\.(log|txt)$/i.test(name)) candidates.push(path.join(base, name)) } catch {} }
  let best = null, bestTime = 0
  for (const file of candidates) { try { const t = fs.statSync(file).mtimeMs; if (t > bestTime) { bestTime = t; best = file } } catch {} }
  return best
}

// ---- disk seçimi ----
function isOneDrivePath(target) {
  const roots = [process.env.OneDrive, process.env.OneDriveConsumer, process.env.OneDriveCommercial].filter(Boolean)
  const normal = target.replace(/\\/g, '/').toLowerCase()
  return roots.some(root => normal.startsWith(root.replace(/\\/g, '/').toLowerCase())) || /\/onedrive[\/ ]/.test(normal)
}
async function chooseRoot() {
  const owner = getMainWindow()
  const choice = await dialog.showOpenDialog(owner && !owner.isDestroyed() ? owner : undefined, { title: 'Kütüphane sürücüsünü/klasörünü seç', properties: ['openDirectory', 'createDirectory'], defaultPath: 'C:\\' })
  if (choice.canceled || !choice.filePaths[0]) return { ok: false }
  const picked = choice.filePaths[0]
  const root = /p2projectlibrary$/i.test(picked) ? picked : path.join(picked, 'P2ProjectLibrary')
  if (isOneDrivePath(root)) return { ok: false, error: 'Bulut eşitlenen (OneDrive) klasörde oyun kütüphanesi tutulamaz.' }
  try { await fsp.mkdir(root, { recursive: true }); await fsp.access(root, fs.constants.W_OK) } catch { return { ok: false, error: 'Bu klasöre yazılamıyor.' } }
  library.root = root; saveLibrary()
  emit({ type: 'state' })
  return { ok: true, root, free: await freeSpace(root) }
}

function state() {
  return { root: library.root, deviceId: library.deviceId, projects: library.projects, jobs: Object.fromEntries([...jobs].map(([id, job]) => [id, job])), running: [...running.keys()] }
}

function registerLibraryIpc(mainWindowGetter) {
  getMainWindow = mainWindowGetter || (() => null)
  loadLibrary()
  fsp.mkdir(uploadsDir(), { recursive: true }).catch(() => {})
  const handle = (name, fn) => ipcMain.handle(name, async (_event, payload) => { try { return await fn(payload) } catch (error) { logLine(`${name}: ${error?.stack || error}`); return { ok: false, error: error.message, code: error.code } } })
  handle('library:configure', async (payload) => { apiBase = String(payload?.apiBase || '').replace(/\/+$/, ''); accessToken = payload?.accessToken || ''; refreshToken = payload?.refreshToken || ''; return { ok: true } })
  handle('library:state', async () => state())
  handle('library:choose-root', () => chooseRoot())
  handle('library:set-channel', async (payload) => { if (library.projects[payload.projectId]) { library.projects[payload.projectId].channel = payload.channel; saveLibrary() } emit({ type: 'state' }); return { ok: true } })
  handle('library:set-limit', async (payload) => { limiter.kbps = Math.max(0, Number(payload?.mbps || 0)) * 1024; return { ok: true } })
  handle('library:install', (payload) => installRelease(payload).then(() => ({ ok: true })))
  handle('library:pause', async (payload) => { const job = jobs.get(payload.projectId); if (job) job.paused = true; return { ok: true } })
  handle('library:resume', async (payload) => { const job = jobs.get(payload.projectId); if (job) job.paused = false; return { ok: true } })
  handle('library:cancel', async (payload) => { const job = jobs.get(payload.projectId); if (job) job.canceled = true; return { ok: true } })
  handle('library:launch', (payload) => launchProject(payload))
  handle('library:stop', (payload) => stopProject(payload))
  handle('library:uninstall', (payload) => uninstall(payload))
  handle('library:verify', (payload) => verifyInstall(payload))
  handle('library:set-args', async (payload) => { if (library.projects[payload.projectId]) { library.projects[payload.projectId].userArgs = String(payload.userArgs || '').slice(0, 500); saveLibrary() } return { ok: true } })
  handle('library:open-folder', async (payload) => { const info = library.projects[payload.projectId]; if (info) await shell.openPath(projectDir(payload.projectName, info.folder)); return { ok: Boolean(info) } })
  handle('library:create-shortcut', (payload) => createShortcut(payload))
  handle('library:pick-build-folder', () => pickBuildFolder())
  handle('library:upload', (payload) => startUpload(payload).then(() => ({ ok: true })).catch(error => ({ ok: false, error: error.message, code: error.code })))
  handle('library:upload-cancel', async (payload) => { uploadsRunning.get(payload.requestId)?.controller.abort(); if (payload.discard && payload.releaseId) { try { await apiJson(`/organizations/${payload.organizationId}/projects/${payload.projectId}/releases/${payload.releaseId}`, { method: 'DELETE' }) } catch {} ; await fsp.rm(path.join(uploadsDir(), `${payload.releaseId}.json`), { force: true }).catch(() => {}) } return { ok: true } })
  handle('library:uploads', () => listUploads())
  handle('library:upload-resume', (payload) => resumeUpload(payload))
  handle('library:upload-forget', async (payload) => { await fsp.rm(path.join(uploadsDir(), `${payload.releaseId}.json`), { force: true }).catch(() => {}); return { ok: true } })
}

async function resumeUpload(payload) {
  const file = path.join(uploadsDir(), `${payload.releaseId}.json`)
  let state
  try { state = JSON.parse(await fsp.readFile(file, 'utf8')) } catch { return { ok: false, error: 'Yükleme durumu bulunamadı.' } }
  const controller = new AbortController()
  uploadsRunning.set(payload.requestId, { controller })
  const send = (patch) => emit({ type: 'upload', requestId: payload.requestId, projectId: state.projectId, channel: state.channel, version: state.releaseVersion, ...patch })
  try {
    const check = await apiJson(`/organizations/${state.organizationId}/projects/${state.projectId}/releases/${state.releaseId}/missing`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' })
    state.pending = check.missing
    await runUploadJob(state, controller, send)
    return { ok: true }
  } catch (error) { if (error.message === 'CANCELED') send({ phase: 'canceled' }); else send({ phase: 'failed', error: error.message }); return { ok: false, error: error.message } }
  finally { uploadsRunning.delete(payload.requestId) }
}

async function pickBuildFolder() {
  const owner = getMainWindow()
  const choice = await dialog.showOpenDialog(owner && !owner.isDestroyed() ? owner : undefined, { title: 'Build klasörünü seç', properties: ['openDirectory'] })
  if (choice.canceled || !choice.filePaths[0]) return null
  const dir = choice.filePaths[0]
  const files = await walkFiles(dir)
  const executables = files.filter(file => /\.exe$/i.test(file.rel)).map(file => file.rel).sort((a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b))
  let totalSize = 0
  for (const file of files) { try { totalSize += (await fsp.stat(file.full)).size } catch {} }
  const warnings = []
  if (isOneDrivePath(dir)) warnings.push('Klasör OneDrive altında; eşitlenen dosyalar yavaş olabilir.')
  if (!executables.length) warnings.push('Klasörde .exe bulunamadı.')
  return { ok: true, path: dir, executables, fileCount: files.length, totalSize }
}

// PNG → ICO: Vista+ ICO kapsayıcısı PNG verisini olduğu gibi taşır (6 baytlık başlık + 16 baytlık giriş + PNG).
function pngToIco(png, size) {
  const header = Buffer.alloc(6); header.writeUInt16LE(0, 0); header.writeUInt16LE(1, 2); header.writeUInt16LE(1, 4)
  const entry = Buffer.alloc(16)
  entry.writeUInt8(size >= 256 ? 0 : size, 0); entry.writeUInt8(size >= 256 ? 0 : size, 1); entry.writeUInt8(0, 2); entry.writeUInt8(0, 3)
  entry.writeUInt16LE(1, 4); entry.writeUInt16LE(32, 6); entry.writeUInt32LE(png.length, 8); entry.writeUInt32LE(22, 12)
  return Buffer.concat([header, entry, png])
}
// Proje ikonunu (sunucudaki PNG/JPG) indirip .ico yapar; yoksa oyunun exe simgesi kullanılır.
async function shortcutIcon(payload) {
  const iconsDir = path.join(libDir(), 'icons'); await fsp.mkdir(iconsDir, { recursive: true })
  const ico = path.join(iconsDir, `${payload.projectId}.ico`)
  if (payload.iconUrl) {
    try {
      const response = await net.fetch(payload.iconUrl, { headers: await authHeaders().catch(() => ({})) })
      if (response.ok) {
        const image = nativeImage.createFromBuffer(Buffer.from(await response.arrayBuffer()))
        if (!image.isEmpty()) { await fsp.writeFile(ico, pngToIco(image.resize({ width: 256, height: 256, quality: 'best' }).toPNG(), 256)); return { icon: ico, iconIndex: 0 } }
      }
    } catch (error) { logLine(`kısayol ikonu indirilemedi: ${error.message}`) }
  }
  const info = library.projects[payload.projectId]
  if (info) { const exe = path.join(projectDir(payload.projectName, info.folder), info.executable.replace(/\//g, path.sep)); if (fs.existsSync(exe)) return { icon: exe, iconIndex: 0 } }
  return {}
}
async function createShortcut(payload) {
  try {
    const link = path.join(app.getPath('desktop'), `${projectFolderName(payload.projectName)}.lnk`)
    const icon = await shortcutIcon(payload)
    shell.writeShortcutLink(link, { target: process.execPath, args: `--p2-play=${payload.projectId}`, description: `${payload.projectName} — P2`, ...icon })
    return { ok: true }
  } catch (error) { return { ok: false, error: error.message } }
}

module.exports = { registerLibraryIpc, __internals: { hashFolder, planInstall } }
