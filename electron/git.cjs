const { execFile } = require('node:child_process')
const { dialog, ipcMain, shell } = require('electron')
const path = require('node:path')
const fs = require('node:fs/promises')
const { authArgs, registerProviderIpc, ensureConnection } = require('./gitProviders.cjs')
const { createCloneProgressParser } = require('./gitProgress.cjs')

// Git istemcisi ana süreçte çalışır: depo kullanıcının kendi diskinde durur ve
// komutlar onun git kurulumuyla, onun kimlik bilgileriyle koşar. Böylece GitHub,
// Gitea ve GitLab dahil her uzak sunucu ek yapılandırma olmadan çalışır.
const MAX_BUFFER = 24 * 1024 * 1024

function run(cwd, args, { timeout = 120000, input, onStderr } = {}) {
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
    if (input !== undefined) { child.stdin.write(input); child.stdin.end() }
  })
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
  const args = ['log', `--max-count=${limit}`, `--pretty=format:${format}`]
  if (branch) args.push(branch)
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
      const [state, branchList, remoteList, history] = await Promise.all([
        status(root), branches(root), remotes(root), log(root, { limit: 60 }),
      ])
      // LFS üç ek süreç çalıştırır ve çoğu kullanıcı bu sekmeyi hiç açmaz.
      // İlk boyamayı bekletmemesi için yalnızca LFS sekmesi istendiğinde yüklenir.
      return { ok: true, root, name: path.basename(root), status: state, branches: branchList, remotes: remoteList, log: history, lfs: { installed: false, patterns: [], files: [] } }
    } catch (error) { return fail(error) }
  })

  ipcMain.handle('git:status', async (_event, { cwd }) => {
    try { return { ok: true, status: await status(cwd) } } catch (error) { return fail(error) }
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

  ipcMain.handle('git:commit', async (_event, { cwd, message, amend }) => {
    const args = ['commit', '-F', '-']
    if (amend) args.push('--amend')
    const result = await run(cwd, args, { input: message })
    return result.ok ? { ok: true, output: result.stdout.trim() } : { ok: false, error: (result.stderr || result.stdout).trim() }
  })

  ipcMain.handle('git:sync', async (_event, { cwd, action, remote = 'origin', branch, setUpstream }) => {
    const url = (await run(cwd, ['remote', 'get-url', remote])).stdout.trim()
    const auth = url ? await authArgs(url) : []
    const args = action === 'push'
      ? ['push', ...(setUpstream ? ['--set-upstream'] : []), remote, ...(branch ? [branch] : [])]
      : action === 'pull' ? ['pull', '--ff-only', remote, ...(branch ? [branch] : [])]
      : ['fetch', '--prune', remote]
    const result = await run(cwd, [...auth, ...args], { timeout: 300000 })
    const text = (result.stderr || result.stdout).trim()
    if (result.ok) return { ok: true, output: text || 'Tamamlandı.' }
    // Kimlik hatası masaüstünde en sık karşılaşılan durum; ayrıca işaretliyoruz.
    const needsAuth = /Authentication failed|could not read Username|Permission denied|403/i.test(text)
    return { ok: false, error: text || 'İşlem başarısız.', auth: needsAuth }
  })

  ipcMain.handle('git:branch', async (_event, { cwd, action, name, from, force }) => {
    const args = action === 'create' ? ['switch', '-c', name, ...(from ? [from] : [])]
      : action === 'checkout' ? ['switch', ...(name.includes('/') ? ['--detach'] : []), name]
      : action === 'delete' ? ['branch', force ? '-D' : '-d', name]
      : action === 'merge' ? ['merge', '--no-edit', name]
      : ['branch']
    const result = await run(cwd, args)
    return result.ok ? { ok: true, output: (result.stdout || result.stderr).trim() } : { ok: false, error: (result.stderr || result.stdout).trim() }
  })

  ipcMain.handle('git:stash', async (_event, { cwd, action, index = 0 }) => {
    const args = action === 'push' ? ['stash', 'push', '--include-untracked']
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

module.exports = { registerGitIpc }
