const { execFile, spawn } = require('node:child_process')
const { dialog, ipcMain, shell } = require('electron')
const path = require('node:path')
const fs = require('node:fs/promises')
const { authArgs, registerProviderIpc, ensureConnection } = require('./gitProviders.cjs')
const { createCloneProgressParser } = require('./gitProgress.cjs')

// Git istemcisi ana süreçte çalışır: depo kullanıcının kendi diskinde durur ve
// komutlar onun git kurulumuyla, onun kimlik bilgileriyle koşar. Böylece GitHub,
// Gitea ve GitLab dahil her uzak sunucu ek yapılandırma olmadan çalışır.
const MAX_BUFFER = 24 * 1024 * 1024

function run(cwd, args, { timeout = 120000, input, onStderr, onChild } = {}) {
  return new Promise(resolve => {
    const child = execFile('git', args, {
      cwd,
      timeout,
      maxBuffer: MAX_BUFFER,
      windowsHide: true,
      // Parola soran bir istem masaüstünde pencereyi kilitler; git'i sessiz tutuyoruz.
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0', LC_ALL: 'C.UTF-8' },
    }, (error, stdout, stderr) => {
      resolve({
        ok: !error,
        code: error?.code ?? 0,
        stdout: String(stdout || ''),
        stderr: String(stderr || error?.message || ''),
      })
    })
    if (onStderr) child.stderr?.on('data', chunk => { try { onStderr(String(chunk)) } catch {} })
    if (onChild) onChild(child)
    if (input !== undefined) { child.stdin.write(input); child.stdin.end() }
  })
}

// Süren ağ/checkout işlemleri: arayüz ilerlemeyi izler ve × ile iptal edebilir.
const runningOperations = new Map()
function trackOperation(event, requestId, kind) {
  const send = (payload) => { if (requestId && !event.sender.isDestroyed()) event.sender.send('git:operation-progress', { requestId, kind, ...payload }) }
  return {
    onStderr: createCloneProgressParser(progress => send(progress)),
    onChild: child => { if (requestId) runningOperations.set(requestId, child) },
    done: () => { if (requestId) runningOperations.delete(requestId) },
  }
}
// İlerleme satırlarını ayıklanmış, kullanıcıya gösterilecek çıktı.
const cleanOutput = (result) => (result.stderr || result.stdout).replace(/\r/g, '\n').split('\n').filter(line => line && !/\d+%/.test(line)).join('\n').trim()

// Tüm geçmiş tek seferde belleğe alınmaz: git log akıtılır, satırlar parça
// parça arayüze gönderilir. Aynı depo için yeni istek eskisini durdurur.
const historyStreams = new Map()
function streamHistory(event, { cwd, requestId, ref }) {
  const previous = historyStreams.get(cwd)
  if (previous) { try { previous.kill() } catch {} historyStreams.delete(cwd) }
  const format = ['%H', '%h', '%an', '%ae', '%at', '%s', '%P', '%D'].join('%x1f')
  const args = ['log', '--topo-order', `--pretty=format:${format}`, ref || '--all']
  const child = spawn('git', args, { cwd, windowsHide: true, env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0', LC_ALL: 'C.UTF-8' } })
  historyStreams.set(cwd, child)
  const send = (payload) => { if (!event.sender.isDestroyed()) event.sender.send('git:history-chunk', { requestId, ...payload }) }
  let tail = '', rows = [], total = 0, timer = null
  const parse = line => {
    const [hash, short, author, email, at, subject, parents, refs] = line.split('\x1f')
    return { hash, short, author, email, at: Number(at) * 1000, subject, parents: parents ? parents.split(' ') : [], refs: refs ? refs.split(', ').filter(Boolean) : [] }
  }
  const flush = (done) => {
    if (timer) { clearTimeout(timer); timer = null }
    if (rows.length || done) { send({ rows, done, total }); rows = [] }
  }
  child.stdout.setEncoding('utf8')
  child.stdout.on('data', chunk => {
    const lines = (tail + chunk).split('\n'); tail = lines.pop() || ''
    for (const line of lines) if (line) { rows.push(parse(line)); total++ }
    // İlk parçalar küçük olsun ki ekran hemen dolsun; sonra daha seyrek gönderilir.
    if (rows.length >= (total < 4000 ? 500 : 4000)) flush(false)
    else if (!timer) timer = setTimeout(() => flush(false), 120)
  })
  child.stderr.setEncoding('utf8')
  let errorText = ''
  child.stderr.on('data', chunk => { errorText = (errorText + chunk).slice(-2000) })
  child.on('close', code => {
    if (tail) { rows.push(parse(tail)); total++; tail = '' }
    if (historyStreams.get(cwd) === child) historyStreams.delete(cwd)
    if (code && !child.killed) { rows = []; send({ rows: [], done: true, total, error: errorText.trim() || `git log ${code} koduyla bitti.` }); return }
    flush(true)
  })
  child.on('error', error => send({ rows: [], done: true, total, error: String(error?.message || error) }))
}

async function ensureRepo(cwd) {
  const top = await run(cwd, ['rev-parse', '--show-toplevel'])
  if (!top.ok) throw new Error('Bu klasör bir git deposu değil.')
  return top.stdout.trim()
}

// porcelain=v2 -z çıktısını satır satır çözer. Yeniden adlandırmalarda yol iki
// NUL alanına yayıldığı için akışı elle ilerletiyoruz.
function parseStatus(raw) {
  const fields = raw.split('\0')
  const result = { branch: '', upstream: '', ahead: 0, behind: 0, oid: '', files: [] }
  for (let index = 0; index < fields.length; index++) {
    const line = fields[index]
    if (!line) continue
    if (line.startsWith('# branch.head ')) { result.branch = line.slice(14); continue }
    if (line.startsWith('# branch.upstream ')) { result.upstream = line.slice(18); continue }
    if (line.startsWith('# branch.oid ')) { result.oid = line.slice(13); continue }
    if (line.startsWith('# branch.ab ')) {
      const match = /\+(\d+) -(\d+)/.exec(line)
      if (match) { result.ahead = Number(match[1]); result.behind = Number(match[2]) }
      continue
    }
    if (line.startsWith('# ')) continue
    const kind = line[0]
    if (kind === '1' || kind === '2') {
      const parts = line.split(' ')
      const xy = parts[1]
      const filePath = kind === '1' ? parts.slice(8).join(' ') : parts.slice(9).join(' ').split('\t')[0]
      const from = kind === '2' ? fields[++index] : null
      result.files.push({ path: filePath, from, staged: xy[0] !== '.', unstaged: xy[1] !== '.', index: xy[0], work: xy[1], untracked: false, conflicted: false })
      continue
    }
    if (kind === 'u') {
      const parts = line.split(' ')
      result.files.push({ path: parts.slice(10).join(' '), from: null, staged: false, unstaged: true, index: 'U', work: 'U', untracked: false, conflicted: true })
      continue
    }
    if (kind === '?') result.files.push({ path: line.slice(2), from: null, staged: false, unstaged: true, index: '?', work: '?', untracked: true, conflicted: false })
  }
  return result
}

async function status(cwd) {
  const [raw, stash] = await Promise.all([
    run(cwd, ['status', '--porcelain=v2', '--branch', '--untracked-files=all', '-z']),
    run(cwd, ['stash', 'list']),
  ])
  if (!raw.ok) throw new Error(raw.stderr.trim() || 'Durum okunamadı.')
  const parsed = parseStatus(raw.stdout)
  parsed.stashes = stash.ok ? stash.stdout.split('\n').filter(Boolean).length : 0
  return parsed
}

async function log(cwd, { limit = 60, branch } = {}) {
  const format = ['%H', '%h', '%an', '%ae', '%at', '%s', '%P', '%D'].join('%x1f')
  const args = ['log', '--topo-order', `--max-count=${limit}`, `--pretty=format:${format}`]
  if (branch) args.push(branch)
  else args.push('--all')
  const result = await run(cwd, args)
  if (!result.ok) return []
  return result.stdout.split('\n').filter(Boolean).map(line => {
    const [hash, short, author, email, at, subject, parents, refs] = line.split('\x1f')
    return { hash, short, author, email, at: Number(at) * 1000, subject, parents: parents ? parents.split(' ') : [], refs: refs ? refs.split(', ').filter(Boolean) : [] }
  })
}

async function branches(cwd) {
  const format = ['%(refname:short)', '%(upstream:short)', '%(objectname:short)', '%(committerdate:unix)', '%(HEAD)', '%(contents:subject)'].join('%1f')
  const [local, remote] = await Promise.all([
    run(cwd, ['for-each-ref', '--sort=-committerdate', `--format=${format}`, 'refs/heads']),
    run(cwd, ['for-each-ref', '--sort=-committerdate', `--format=${format}`, 'refs/remotes']),
  ])
  const parse = (raw, kind) => raw.split('\n').filter(Boolean).map(line => {
    const [name, upstream, short, at, head, subject] = line.split('\x1f')
    return { name, upstream, short, at: Number(at) * 1000, current: head === '*', subject, kind }
  })
  return {
    local: local.ok ? parse(local.stdout, 'local') : [],
    remote: remote.ok ? parse(remote.stdout, 'remote').filter(item => !item.name.endsWith('/HEAD')) : [],
  }
}

async function remotes(cwd) {
  const result = await run(cwd, ['remote', '-v'])
  if (!result.ok) return []
  const seen = new Map()
  for (const line of result.stdout.split('\n').filter(Boolean)) {
    const [name, rest] = line.split('\t')
    const url = (rest || '').split(' ')[0]
    if (!seen.has(name)) seen.set(name, { name, url })
  }
  return [...seen.values()]
}

async function lastFetchedAt(cwd) {
  const location = await run(cwd, ['rev-parse', '--git-path', 'FETCH_HEAD'])
  if (!location.ok) return 0
  const target = path.isAbsolute(location.stdout.trim()) ? location.stdout.trim() : path.resolve(cwd, location.stdout.trim())
  try { return (await fs.stat(target)).mtimeMs } catch { return 0 }
}

// Uzak adresi arayüze taşımadan yalnızca sağlayıcıyı tanımaya yetecek alan adını verir.
// HTTPS yanında git@host:sahip/depo.git biçimindeki SSH adreslerini de destekler.
function remoteHost(value) {
  try { return new URL(value).host.toLowerCase() } catch {
    return (/^(?:[^@\s]+@)?([^:\s]+):/.exec(String(value || '').trim())?.[1] || '').toLowerCase()
  }
}

// Değişiklik önizlemesi: izlenmeyen dosyalar için de birleşik fark üretilir.
async function diff(cwd, filePath, staged, untracked) {
  const args = untracked
    ? ['diff', '--no-index', '--no-color', '--', process.platform === 'win32' ? 'NUL' : '/dev/null', filePath]
    : ['diff', '--no-color', ...(staged ? ['--cached'] : []), '--', filePath]
  const result = await run(cwd, args)
  // --no-index farkı bulunca 1 ile çıkar; çıktı yine geçerlidir.
  return result.stdout || (result.ok ? '' : result.stderr)
}

async function commitDiff(cwd, hash, filePath) {
  const result = await run(cwd, ['show', '--no-color', '--format=', hash, ...(filePath ? ['--', filePath] : [])])
  return result.stdout || result.stderr
}

async function commitFiles(cwd, hash) {
  const [tree, stats] = await Promise.all([
    run(cwd, ['diff-tree', '--root', '--no-commit-id', '--name-status', '-r', hash]),
    run(cwd, ['show', '--numstat', '--format=', hash]),
  ])
  if (!tree.ok) throw new Error(tree.stderr.trim() || 'Commit dosyaları okunamadı.')
  const files = tree.stdout.split('\n').filter(Boolean).map(line => {
    const fields = line.split('\t')
    const state = (fields.shift() || 'M')[0]
    return { state, path: fields[fields.length - 1] || '', from: fields.length > 1 ? fields[0] : null }
  }).filter(file => file.path)
  let additions = 0, deletions = 0
  if (stats.ok) for (const line of stats.stdout.split('\n').filter(Boolean)) {
    const [added, deleted] = line.split('\t')
    if (/^\d+$/.test(added)) additions += Number(added)
    if (/^\d+$/.test(deleted)) deletions += Number(deleted)
  }
  return { files, additions, deletions }
}

async function commitTree(cwd, hash) {
  const result = await run(cwd, ['ls-tree', '-r', '--name-only', '-z', hash])
  if (!result.ok) throw new Error(result.stderr.trim() || 'Commit dosya ağacı okunamadı.')
  return result.stdout.split('\0').filter(Boolean)
}

async function appendIgnored(cwd, patterns) {
  const root = await ensureRepo(cwd)
  const target = path.join(root, '.gitignore')
  let current = ''
  try { current = await fs.readFile(target, 'utf8') } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }
  const lines = current.replace(/\r\n/g, '\n').split('\n').filter(Boolean)
  const known = new Set(lines)
  for (const value of patterns || []) {
    const pattern = String(value || '').replace(/\\/g, '/').trim()
    if (pattern && !known.has(pattern)) { known.add(pattern); lines.push(pattern) }
  }
  await fs.writeFile(target, lines.length ? `${lines.join('\n')}\n` : '', 'utf8')
}

async function repositorySettings(cwd) {
  const root = await ensureRepo(cwd)
  const [remote, userName, userEmail, lfs] = await Promise.all([
    run(root, ['remote', 'get-url', 'origin']),
    run(root, ['config', '--local', '--get', 'user.name']),
    run(root, ['config', '--local', '--get', 'user.email']),
    lfsInfo(root),
  ])
  let ignored = ''
  try { ignored = await fs.readFile(path.join(root, '.gitignore'), 'utf8') } catch (error) {
    if (error?.code !== 'ENOENT') throw error
  }
  return {
    root,
    remoteUrl: remote.ok ? remote.stdout.trim() : '',
    ignored,
    userName: userName.ok ? userName.stdout.trim() : '',
    userEmail: userEmail.ok ? userEmail.stdout.trim() : '',
    lfs,
  }
}

async function saveRepositorySettings(cwd, values) {
  const root = await ensureRepo(cwd)
  const remoteUrl = String(values.remoteUrl || '').trim()
  if (remoteUrl) {
    const existing = await run(root, ['remote', 'get-url', 'origin'])
    const changed = await run(root, ['remote', existing.ok ? 'set-url' : 'add', 'origin', remoteUrl])
    if (!changed.ok) throw new Error(changed.stderr.trim() || 'Remote kaydedilemedi.')
  }
  const ignored = String(values.ignored || '').replace(/\r\n/g, '\n')
  await fs.writeFile(path.join(root, '.gitignore'), ignored && !ignored.endsWith('\n') ? `${ignored}\n` : ignored, 'utf8')
  for (const [key, value] of [['user.name', values.userName], ['user.email', values.userEmail]]) {
    const text = String(value || '').trim()
    const result = text
      ? await run(root, ['config', '--local', key, text])
      : await run(root, ['config', '--local', '--unset-all', key])
    if (!result.ok && text) throw new Error(result.stderr.trim() || `${key} kaydedilemedi.`)
  }
  return repositorySettings(root)
}

async function lfsInfo(cwd) {
  const version = await run(cwd, ['lfs', 'version'])
  if (!version.ok) return { installed: false, patterns: [], files: [] }
  const tracked = await run(cwd, ['lfs', 'track'])
  const patterns = tracked.stdout.split('\n').map(line => line.trim())
    .filter(line => line && !line.startsWith('Listing') && !line.startsWith('Tracking'))
    .map(line => line.split(' ')[0])
  const files = await run(cwd, ['lfs', 'ls-files', '-n'])
  return {
    installed: true,
    version: version.stdout.trim(),
    patterns,
    files: files.ok ? files.stdout.split('\n').filter(Boolean).slice(0, 500) : [],
  }
}

const fail = (error) => ({ ok: false, error: String(error?.message || error) })

// Klonlama iki uçtan da çağrılıyor: klasör seçimi, kimlik başlığı ve hedef adı
// tek yerde kalsın diye ayrı bir yardımcı.
async function cloneRepo({ url, parent, name }, onProgress = () => {}) {
  try {
    let target = parent
    if (!target) {
      const picked = await dialog.showOpenDialog({ properties: ['openDirectory', 'createDirectory'], title: 'Klonlanacak klasörü seç' })
      if (picked.canceled || !picked.filePaths[0]) return null
      target = picked.filePaths[0]
    }
    const folder = name?.trim() || (url.split('/').pop() || 'repo').replace(/\.git$/, '')
    const destination = path.join(target, folder)
    const result = await run(target, [...(await authArgs(url)), 'clone', '--progress', url, folder], {
      timeout: 600000,
      onStderr: createCloneProgressParser(onProgress),
    })
    if (!result.ok) return { ok: false, error: result.stderr.trim() || 'Klonlanamadı.' }
    return { ok: true, path: destination, name: folder, output: result.stderr.trim() }
  } catch (error) { return fail(error) }
}

function registerGitIpc() {
  registerProviderIpc()
  // Depo seçme ve klonlama
  ipcMain.handle('git:pick-folder', async () => {
    const picked = await dialog.showOpenDialog({ properties: ['openDirectory'], title: 'Depo klasörünü seç' })
    if (picked.canceled || !picked.filePaths[0]) return null
    const folder = picked.filePaths[0]
    const top = await run(folder, ['rev-parse', '--show-toplevel'])
    if (!top.ok) return { ok: false, error: 'Seçilen klasör bir git deposu değil.' }
    const root = top.stdout.trim()
    const remoteList = await remotes(root)
    const origin = remoteList.find(item => item.name === 'origin') || remoteList[0]
    return { ok: true, path: root, name: path.basename(root), remoteHost: remoteHost(origin?.url) }
  })

  ipcMain.handle('git:repo-source', async (_event, { cwd }) => {
    try {
      const root = await ensureRepo(cwd)
      const remoteList = await remotes(root)
      const origin = remoteList.find(item => item.name === 'origin') || remoteList[0]
      return { ok: true, remoteHost: remoteHost(origin?.url) }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:clone', async (event, payload) => {
    const requestId = String(payload?.requestId || '')
    return cloneRepo(payload, progress => {
      if (requestId && !event.sender.isDestroyed()) event.sender.send('git:clone-progress', { requestId, ...progress })
    })
  })

  // Yapıştırılan adresle klonlama: gerekiyorsa önce tarayıcıda giriş açılır,
  // onaydan sonra klonlama kendiliğinden sürer. Kullanıcı hiçbir şey tanımlamaz.
  ipcMain.handle('git:clone-url', async (_event, { url, parent, name }) => {
    try {
      const address = String(url || '').trim()
      if (!/^https?:\/\//i.test(address)) return { ok: false, error: 'Depo adresi http:// ya da https:// ile başlamalı.' }
      const session = await ensureConnection(address)
      // Giriş başarısızsa yine de deneriz: açık depolar kimlik istemez.
      const cloned = await cloneRepo({ url: address, parent, name })
      if (cloned && cloned.ok === false && !session.ok) return { ...cloned, error: session.error || cloned.error, needsApp: session.needsApp }
      return cloned ? { ...cloned, connection: session.ok ? session.connection : undefined } : null
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:info', async (_event, { cwd }) => {
    try {
      const root = await ensureRepo(cwd)
      const [state, branchList, remoteList, history, fetchedAt] = await Promise.all([
        status(root), branches(root), remotes(root), log(root, { limit: 60 }), lastFetchedAt(root),
      ])
      // LFS üç ek süreç çalıştırır ve çoğu kullanıcı bu sekmeyi hiç açmaz.
      // İlk boyamayı bekletmemesi için yalnızca LFS sekmesi istendiğinde yüklenir.
      return { ok: true, root, name: path.basename(root), status: state, branches: branchList, remotes: remoteList, log: history, lastFetchedAt: fetchedAt, lfs: { installed: false, patterns: [], files: [] } }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:status', async (_event, { cwd }) => {
    try { return { ok: true, status: await status(cwd) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:history-stream', async (event, payload) => { try { streamHistory(event, payload); return { ok: true } } catch (error) { return fail(error) } })
  ipcMain.handle('git:history-cancel', async (_event, { cwd }) => { const child = historyStreams.get(cwd); if (child) { try { child.kill() } catch {} historyStreams.delete(cwd) } return { ok: true } })
  ipcMain.handle('git:operation-cancel', async (_event, { requestId }) => { const child = runningOperations.get(requestId); if (child) { try { child.kill() } catch {} runningOperations.delete(requestId) } return { ok: true } })

  ipcMain.handle('git:history', async (_event, { cwd, limit = 500, ref }) => {
    try { return { ok: true, log: await log(cwd, { limit: Math.min(1000, Math.max(60, Number(limit) || 500)), branch: ref || undefined }) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:diff', async (_event, { cwd, path: filePath, staged, untracked }) => {
    try { return { ok: true, diff: await diff(cwd, filePath, staged, untracked) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:commit-diff', async (_event, { cwd, hash, path: filePath }) => {
    try { return { ok: true, diff: await commitDiff(cwd, hash, filePath) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:commit-files', async (_event, { cwd, hash }) => {
    try { return { ok: true, ...(await commitFiles(cwd, hash)) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:commit-tree', async (_event, { cwd, hash }) => {
    try { return { ok: true, files: await commitTree(cwd, hash) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:repository-settings', async (_event, { cwd }) => {
    try { return { ok: true, ...(await repositorySettings(cwd)) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:save-repository-settings', async (_event, payload) => {
    try { return { ok: true, ...(await saveRepositorySettings(payload.cwd, payload)) } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:stage', async (_event, { cwd, paths }) => {
    const result = await run(cwd, ['add', '--', ...paths])
    return result.ok ? { ok: true } : { ok: false, error: result.stderr.trim() }
  })

  ipcMain.handle('git:unstage', async (_event, { cwd, paths }) => {
    const result = await run(cwd, ['restore', '--staged', '--', ...paths])
    if (result.ok) return { ok: true }
    // Henüz commit yokken HEAD çözülemez; ilk dosyalar indeksten böyle çıkarılır.
    const head = await run(cwd, ['rev-parse', '--verify', 'HEAD'])
    if (!head.ok) {
      const removed = await run(cwd, ['rm', '--cached', '-r', '--', ...paths])
      return removed.ok ? { ok: true } : { ok: false, error: removed.stderr.trim() }
    }
    return { ok: false, error: result.stderr.trim() }
  })

  // Değişikliği geri alma: izlenen dosya son haline döner, izlenmeyen silinir.
  ipcMain.handle('git:discard', async (_event, { cwd, paths, untracked }) => {
    if (untracked?.length) {
      for (const item of untracked) { try { await fs.rm(path.join(cwd, item), { recursive: true, force: true }) } catch {} }
    }
    if (!paths?.length) return { ok: true }
    const result = await run(cwd, ['restore', '--staged', '--worktree', '--', ...paths])
    if (result.ok) return { ok: true }
    const head = await run(cwd, ['rev-parse', '--verify', 'HEAD'])
    if (!head.ok) {
      const removed = await run(cwd, ['rm', '--cached', '-r', '--', ...paths])
      return removed.ok ? { ok: true } : { ok: false, error: removed.stderr.trim() }
    }
    return { ok: false, error: result.stderr.trim() }
  })

  ipcMain.handle('git:ignore', async (_event, { cwd, patterns }) => {
    try { await appendIgnored(cwd, patterns); return { ok: true } } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:open-path', async (_event, { cwd, path: filePath }) => {
    try {
      const root = await ensureRepo(cwd)
      const error = await shell.openPath(path.join(root, filePath || ''))
      return error ? { ok: false, error } : { ok: true }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:save-patch', async (_event, { cwd, paths, staged }) => {
    try {
      const root = await ensureRepo(cwd)
      const result = await run(root, ['diff', '--no-color', ...(staged ? ['--cached'] : []), '--', ...(paths || [])])
      if (!result.ok) return { ok: false, error: result.stderr.trim() || 'Patch oluşturulamadı.' }
      const chosen = await dialog.showSaveDialog({
        title: 'Patch olarak kaydet',
        defaultPath: path.join(root, `changes-${Date.now()}.patch`),
        filters: [{ name: 'Git patch', extensions: ['patch', 'diff'] }],
      })
      if (chosen.canceled || !chosen.filePath) return { ok: false, canceled: true }
      await fs.writeFile(chosen.filePath, result.stdout, 'utf8')
      return { ok: true, path: chosen.filePath }
    } catch (error) { return fail(error) }
  })

  // Geçmişte bir commit'e sağ tık: checkout (ayrık HEAD), cherry-pick, revert,
  // reset, patch olarak kaydet, yerel değişikliklerle karşılaştır.
  ipcMain.handle('git:commit-op', async (event, { cwd, action, hash, mode, requestId }) => {
    try {
      const root = await ensureRepo(cwd)
      if (!/^[0-9a-f]{7,40}$/i.test(String(hash || ''))) return { ok: false, error: 'Geçersiz commit.' }
      if (action === 'patch') {
        const result = await run(root, ['format-patch', '-1', hash, '--stdout'])
        if (!result.ok) return { ok: false, error: result.stderr.trim() || 'Patch oluşturulamadı.' }
        const chosen = await dialog.showSaveDialog({ title: 'Patch olarak kaydet', defaultPath: path.join(root, `${hash.slice(0, 10)}.patch`), filters: [{ name: 'Git patch', extensions: ['patch', 'diff'] }] })
        if (chosen.canceled || !chosen.filePath) return { ok: false, canceled: true }
        await fs.writeFile(chosen.filePath, result.stdout, 'utf8')
        return { ok: true, path: chosen.filePath }
      }
      if (action === 'compare') {
        // Çalışma ağacı ile o commit arasındaki fark: dosya listesi + birleşik diff.
        const [names, diff] = await Promise.all([run(root, ['diff', '--name-status', hash]), run(root, ['diff', '--no-color', hash])])
        if (!names.ok) return { ok: false, error: names.stderr.trim() || 'Karşılaştırılamadı.' }
        const files = names.stdout.split('\n').filter(Boolean).map(line => { const [state, ...rest] = line.split('\t'); return { path: rest.pop() || '', from: rest[0] || null, state: state[0] } })
        return { ok: true, files, diff: diff.stdout }
      }
      const args = action === 'checkout' ? ['switch', '--detach', '--progress', hash]
        : action === 'cherry-pick' ? ['cherry-pick', hash]
        : action === 'revert' ? ['revert', '--no-edit', hash]
        : action === 'reset' ? ['reset', `--${['soft', 'mixed', 'hard'].includes(mode) ? mode : 'mixed'}`, hash]
        : null
      if (!args) return { ok: false, error: 'Bilinmeyen commit işlemi.' }
      const tracked = trackOperation(event, requestId, action)
      const result = await run(root, args, { timeout: 600000, onStderr: tracked.onStderr, onChild: tracked.onChild })
      tracked.done()
      const text = cleanOutput(result)
      return result.ok ? { ok: true, output: text || 'Tamamlandı.' } : { ok: false, error: text || 'İşlem başarısız.' }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:commit', async (_event, { cwd, message, amend }) => {
    const args = ['commit', '-F', '-']
    if (amend) args.push('--amend')
    const result = await run(cwd, args, { input: message })
    return result.ok ? { ok: true, output: result.stdout.trim() } : { ok: false, error: (result.stderr || result.stdout).trim() }
  })

  ipcMain.handle('git:sync', async (event, { cwd, action, remote = 'origin', branch, setUpstream, requestId }) => {
    const url = (await run(cwd, ['remote', 'get-url', remote])).stdout.trim()
    const auth = url ? await authArgs(url) : []
    const args = action === 'push'
      ? ['push', '--progress', ...(setUpstream ? ['--set-upstream'] : []), remote, ...(branch ? [branch] : [])]
      : action === 'pull' ? ['pull', '--ff-only', '--progress', remote, ...(branch ? [branch] : [])]
      : ['fetch', '--prune', '--progress', remote]
    const tracked = trackOperation(event, requestId, action)
    const result = await run(cwd, [...auth, ...args], { timeout: 300000, onStderr: tracked.onStderr, onChild: tracked.onChild })
    tracked.done()
    const text = cleanOutput(result)
    if (result.ok) return { ok: true, output: text || 'Tamamlandı.' }
    if (result.code === null || /SIGTERM/i.test(String(result.stderr))) return { ok: false, error: 'İşlem iptal edildi.', canceled: true }
    // Kimlik hatası masaüstünde en sık karşılaşılan durum; ayrıca işaretliyoruz.
    const needsAuth = /Authentication failed|could not read Username|Permission denied|403/i.test(text)
    return { ok: false, error: text || 'İşlem başarısız.', auth: needsAuth }
  })

  ipcMain.handle('git:branch', async (event, { cwd, action, name, from, force, requestId }) => {
    const args = action === 'create' ? ['switch', '--progress', '-c', name, ...(from ? [from] : [])]
      : action === 'checkout' ? ['switch', '--progress', name]
      : action === 'delete' ? ['branch', force ? '-D' : '-d', name]
      : action === 'merge' ? ['merge', '--no-edit', name]
      : ['branch']
    const tracked = trackOperation(event, requestId, 'checkout')
    const result = await run(cwd, args, { timeout: 600000, onStderr: tracked.onStderr, onChild: tracked.onChild })
    tracked.done()
    const text = cleanOutput(result)
    if (!result.ok && result.code === null) return { ok: false, error: 'İşlem iptal edildi.', canceled: true }
    return result.ok ? { ok: true, output: text } : { ok: false, error: text }
  })

  // Fork'taki "New Worktree": seçilen klasöre ayrı bir çalışma ağacı açılır;
  // dal adı verilmezse git klasör adıyla HEAD'den yeni dal türetir.
  ipcMain.handle('git:worktree', async (_event, { cwd, action, path: target, branch }) => {
    try {
      const root = await ensureRepo(cwd)
      if (action === 'list') {
        const result = await run(root, ['worktree', 'list', '--porcelain'])
        const items = result.stdout.split(/\n\n+/).filter(Boolean).map(block => {
          const lines = block.split('\n'); const get = key => (lines.find(line => line.startsWith(key + ' ')) || '').slice(key.length + 1)
          return { path: get('worktree'), head: get('HEAD'), branch: get('branch').replace('refs/heads/', ''), bare: lines.includes('bare'), detached: lines.includes('detached') }
        })
        return { ok: true, items }
      }
      if (action === 'add') {
        let folder = target
        if (!folder) {
          const picked = await dialog.showOpenDialog({ properties: ['openDirectory', 'createDirectory'], title: 'Yeni çalışma ağacı klasörü' })
          if (picked.canceled || !picked.filePaths[0]) return null
          folder = picked.filePaths[0]
        }
        const result = await run(root, ['worktree', 'add', ...(branch ? ['-b', branch] : []), folder], { timeout: 600000 })
        const text = (result.stderr || result.stdout).trim()
        return result.ok ? { ok: true, path: folder, output: text } : { ok: false, error: text }
      }
      return { ok: false, error: 'Bilinmeyen çalışma ağacı işlemi.' }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:branch-op', async (_event, { cwd, action, ref, name, remote = 'origin', branch }) => {
    try {
      const root = await ensureRepo(cwd)
      let args
      if (action === 'fast-forward') {
        const current = (await run(root, ['branch', '--show-current'])).stdout.trim()
        args = current === name ? ['merge', '--ff-only', ref] : ['branch', '-f', name, ref]
      }
      else if (action === 'merge') args = ['merge', '--no-edit', ref]
      else if (action === 'rebase') args = ['rebase', ref]
      else if (action === 'rename') args = ['branch', '-m', ref, name]
      else if (action === 'track') args = ['branch', '--set-upstream-to', ref, name]
      else if (action === 'tag') args = ['tag', name, ref]
      else if (action === 'delete-remote') args = ['push', remote, '--delete', branch]
      else if (action === 'pull') args = ['pull', '--no-rebase', remote, branch]
      else if (action === 'push') args = ['push', remote, ref]
      else return { ok: false, error: 'Bilinmeyen dal işlemi.' }
      const url = ['pull', 'push', 'delete-remote'].includes(action) ? (await run(root, ['remote', 'get-url', remote])).stdout.trim() : ''
      const auth = url ? await authArgs(url) : []
      const result = await run(root, [...auth, ...args], { timeout: 300000 })
      const text = (result.stderr || result.stdout).trim()
      return result.ok ? { ok: true, output: text || 'Tamamlandı.' } : { ok: false, error: text || 'İşlem başarısız.' }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:stash', async (_event, { cwd, action, index = 0, paths }) => {
    const args = action === 'push' ? ['stash', 'push', '--include-untracked', ...(paths?.length ? ['--', ...paths] : [])]
      : action === 'pop' ? ['stash', 'pop', `stash@{${index}}`]
      : ['stash', 'list']
    const result = await run(cwd, args)
    return result.ok ? { ok: true, output: (result.stdout || result.stderr).trim() } : { ok: false, error: (result.stderr || result.stdout).trim() }
  })

  ipcMain.handle('git:lfs', async (_event, { cwd, action, pattern }) => {
    if (action === 'status') return { ok: true, output: '', lfs: await lfsInfo(cwd) }
    const args = action === 'install' ? ['lfs', 'install']
      : action === 'track' ? ['lfs', 'track', pattern]
      : action === 'untrack' ? ['lfs', 'untrack', pattern]
      : ['lfs', 'status']
    const result = await run(cwd, args)
    if (!result.ok) return { ok: false, error: (result.stderr || result.stdout).trim() }
    return { ok: true, output: (result.stdout || result.stderr).trim(), lfs: await lfsInfo(cwd) }
  })

  ipcMain.handle('git:reveal', async (_event, { cwd, path: filePath }) => {
    try { shell.showItemInFolder(path.join(cwd, filePath || '')); return { ok: true } } catch (error) { return fail(error) }
  })
}

module.exports = { registerGitIpc, __internals: { log, parseStatus, status, branches } }
