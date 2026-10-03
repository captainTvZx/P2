const { execFile } = require('node:child_process')
const fs = require('node:fs')
const fsp = require('node:fs/promises')
const os = require('node:os')
const path = require('node:path')
const { Client } = require('pg')

function run(file, args, options = {}) {
  return new Promise((resolve, reject) => {
    execFile(file, args, { windowsHide: true, maxBuffer: 16 * 1024 * 1024, ...options }, (error, stdout, stderr) => {
      if (error) {
        error.message = `${error.message}${stderr ? `\n${String(stderr).trim()}` : ''}`
        reject(error)
      } else resolve({ stdout: String(stdout || ''), stderr: String(stderr || '') })
    })
  })
}

// PostgreSQL her zaman Program Files altına kurulmaz; kurulum yolunu asıl
// bilen kayıt defteri. Başka bir sürücüye kurulmuş PostgreSQL'de yedek alma ve
// yükleme bu olmadan "pg_restore bulunamadı" diye düşüyordu.
async function windowsInstallRoots() {
  const key = 'HKLM\\SOFTWARE\\PostgreSQL\\Installations'
  const roots = []
  for (const view of ['/reg:64', '/reg:32']) {
    try {
      const { stdout } = await run('reg.exe', ['query', key, '/s', '/v', 'Base Directory', view])
      for (const line of stdout.split(/\r?\n/)) {
        const found = /^\s*Base Directory\s+REG_SZ\s+(.+?)\s*$/i.exec(line)
        if (found && !roots.includes(found[1])) roots.push(found[1])
      }
    } catch {}
  }
  // Birden çok sürüm kuruluysa en yenisi önce denensin.
  return roots.sort((a, b) => b.localeCompare(a, undefined, { numeric: true }))
}

async function postgresTool(name) {
  const executable = process.platform === 'win32' ? `${name}.exe` : name
  const candidates = []
  if (process.env.P2_PG_BIN) candidates.push(path.join(process.env.P2_PG_BIN, executable))
  if (process.platform === 'win32') {
    for (const base of await windowsInstallRoots()) candidates.push(path.join(base, 'bin', executable))
    for (const root of [process.env.ProgramFiles, process.env['ProgramFiles(x86)']].filter(Boolean)) {
      const base = path.join(root, 'PostgreSQL')
      try {
        const versions = (await fsp.readdir(base, { withFileTypes: true })).filter(item => item.isDirectory()).map(item => item.name).sort((a, b) => b.localeCompare(a, undefined, { numeric: true }))
        for (const version of versions) candidates.push(path.join(base, version, 'bin', executable))
      } catch {}
    }
  }
  candidates.push(executable)
  for (const candidate of candidates) {
    try { await run(candidate, ['--version']); return candidate } catch {}
  }
  throw new Error(`${name} bulunamadı. PostgreSQL istemci araçlarını kur veya P2_PG_BIN ile bin klasörünü belirt.`)
}

function cleanDatabaseUrl(value) {
  const url = new URL(String(value || ''))
  url.searchParams.delete('schema')
  return url.toString()
}

function archiveTool() { return process.platform === 'win32' ? 'tar.exe' : 'tar' }
function stamp() { return new Date().toISOString().replace(/[:.]/g, '-').replace('T', '_').replace('Z', '') }
function identifier(value) { return `"${String(value).replace(/"/g, '""')}"` }
function portableValue(value) {
  if (typeof value === 'bigint') return { $p2: 'bigint', value: String(value) }
  if (Buffer.isBuffer(value)) return { $p2: 'buffer', value: value.toString('base64') }
  if (Array.isArray(value)) return value.map(portableValue)
  if (value && typeof value === 'object' && !(value instanceof Date)) return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, portableValue(item)]))
  return value
}
function restoredValue(value) {
  if (Array.isArray(value)) return value.map(restoredValue)
  if (value && typeof value === 'object') {
    if (value.$p2 === 'bigint') return String(value.value)
    if (value.$p2 === 'buffer') return Buffer.from(String(value.value), 'base64')
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, restoredValue(item)]))
  }
  return value
}
async function createPortableDump(databaseUrl, destination) {
  const db = new Client({ connectionString: databaseUrl })
  await db.connect()
  try {
    const names = (await db.query("SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename")).rows.map(row => row.tablename)
    const tables = []
    for (const name of names) {
      const rows = (await db.query(`SELECT * FROM ${identifier(name)}`)).rows.map(portableValue)
      tables.push({ name, rows })
    }
    await fsp.writeFile(destination, JSON.stringify({ format: 'p2-postgres-json-v1', tables }), { mode: 0o600 })
  } finally { await db.end() }
}
async function restorePortableDump(databaseUrl, source) {
  const dump = JSON.parse(await fsp.readFile(source, 'utf8'))
  if (dump?.format !== 'p2-postgres-json-v1' || !Array.isArray(dump.tables)) throw new Error('Taşınabilir veritabanı yedeği geçersiz.')
  const db = new Client({ connectionString: databaseUrl })
  await db.connect()
  try {
    await db.query('BEGIN')
    const existing = new Set((await db.query("SELECT tablename FROM pg_tables WHERE schemaname = 'public'")).rows.map(row => row.tablename))
    const tables = dump.tables.filter(table => existing.has(table.name) && Array.isArray(table.rows))
    if (tables.length) await db.query(`TRUNCATE ${tables.map(table => identifier(table.name)).join(', ')} RESTART IDENTITY CASCADE`)
    const relations = (await db.query("SELECT child.relname AS child, parent.relname AS parent FROM pg_constraint fk JOIN pg_class child ON child.oid = fk.conrelid JOIN pg_class parent ON parent.oid = fk.confrelid JOIN pg_namespace ns ON ns.oid = child.relnamespace WHERE fk.contype = 'f' AND ns.nspname = 'public'")).rows
    const pending = new Map(tables.map(table => [table.name, table]))
    const ordered = []
    while (pending.size) {
      const ready = [...pending.values()].filter(table => !relations.some(link => link.child === table.name && pending.has(link.parent) && link.parent !== table.name))
      if (!ready.length) { ordered.push(...pending.values()); break }
      for (const table of ready) { ordered.push(table); pending.delete(table.name) }
    }
    for (const table of ordered) {
      for (const raw of table.rows) {
        const row = restoredValue(raw)
        const columns = Object.keys(row)
        if (!columns.length) continue
        const placeholders = columns.map((_, index) => `$${index + 1}`).join(', ')
        await db.query(`INSERT INTO ${identifier(table.name)} (${columns.map(identifier).join(', ')}) VALUES (${placeholders})`, columns.map(column => row[column]))
      }
    }
    await db.query('COMMIT')
  } catch (error) { await db.query('ROLLBACK').catch(() => {}); throw error } finally { await db.end() }
}

async function createBackup({ databaseUrl, dataDir, destination, version = 'unknown' }) {
  if (!databaseUrl) throw new Error('PostgreSQL yapılandırması bulunamadı.')
  const target = path.resolve(destination)
  await fsp.mkdir(path.dirname(target), { recursive: true })
  const work = await fsp.mkdtemp(path.join(os.tmpdir(), 'p2-backup-'))
  try {
    const dump = path.join(work, 'database.dump')
    let databaseFormat = 'postgres-custom'
    let pgDump = null
    try { pgDump = await postgresTool('pg_dump') } catch {}
    if (pgDump) await run(pgDump, ['--dbname', cleanDatabaseUrl(databaseUrl), '--format=custom', '--no-owner', '--no-acl', '--file', dump])
    else { databaseFormat = 'p2-postgres-json-v1'; await createPortableDump(databaseUrl, dump) }
    const uploads = path.join(dataDir, 'uploads')
    const manifest = {
      format: 1,
      product: 'P2 Server',
      version,
      createdAt: new Date().toISOString(),
      databaseFormat,
      includes: { database: true, uploads: fs.existsSync(uploads) },
    }
    await fsp.writeFile(path.join(work, 'manifest.json'), JSON.stringify(manifest, null, 2), { mode: 0o600 })
    const args = ['-czf', target, '-C', work, 'manifest.json', 'database.dump']
    if (manifest.includes.uploads) args.push('-C', dataDir, 'uploads')
    await run(archiveTool(), args)
    const info = await fsp.stat(target)
    return { ok: true, path: target, size: info.size, manifest }
  } finally {
    await fsp.rm(work, { recursive: true, force: true }).catch(() => {})
  }
}

async function validateArchive(file) {
  const info = await fsp.stat(file)
  if (!info.isFile() || info.size < 64 || info.size > 250 * 1024 * 1024 * 1024) throw new Error('Yedek dosyası geçersiz veya desteklenen boyut sınırını aşıyor.')
  const listed = await run(archiveTool(), ['-tzf', file])
  const entries = listed.stdout.split(/\r?\n/).map(value => value.trim()).filter(Boolean)
  if (!entries.length || entries.length > 100000) throw new Error('Yedek içeriği geçersiz veya çok fazla dosya içeriyor.')
  for (const entry of entries) {
    const normalized = entry.replace(/\\/g, '/')
    if (normalized.startsWith('/') || /^[A-Za-z]:/.test(normalized) || normalized.split('/').includes('..')) throw new Error('Yedek güvenli olmayan bir dosya yolu içeriyor.')
    if (!(normalized === 'manifest.json' || normalized === 'database.dump' || normalized === 'uploads' || normalized.startsWith('uploads/'))) throw new Error(`Yedekte beklenmeyen içerik var: ${normalized}`)
  }
  const verbose = await run(archiveTool(), ['-tvzf', file])
  if (verbose.stdout.split(/\r?\n/).some(line => /^[lh]/i.test(line.trim()))) throw new Error('Sembolik bağlantı içeren yedekler güvenlik nedeniyle yüklenemez.')
  if (!entries.includes('manifest.json') || !entries.includes('database.dump')) throw new Error('Yedekte manifest veya veritabanı dökümü eksik.')
  return entries
}

async function restoreBackup({ databaseUrl, dataDir, source, stop, start }) {
  if (!databaseUrl) throw new Error('Önce hedef PostgreSQL bağlantısını yapılandır.')
  const file = path.resolve(source)
  await validateArchive(file)
  const work = await fsp.mkdtemp(path.join(os.tmpdir(), 'p2-restore-'))
  let stopped = false
  try {
    await run(archiveTool(), ['-xzf', file, '-C', work])
    const manifest = JSON.parse(await fsp.readFile(path.join(work, 'manifest.json'), 'utf8'))
    if (manifest?.format !== 1 || manifest?.product !== 'P2 Server') throw new Error('Bu dosya desteklenen bir P2 Server yedeği değil.')
    if (stop) {
      const result = await stop()
      if (result && result.ok === false) throw new Error(result.message || 'Sunucu durdurulamadı.')
      stopped = true
    }
    if (manifest.databaseFormat === 'p2-postgres-json-v1') {
      await restorePortableDump(databaseUrl, path.join(work, 'database.dump'))
    } else {
      const pgRestore = await postgresTool('pg_restore')
      await run(pgRestore, ['--dbname', cleanDatabaseUrl(databaseUrl), '--clean', '--if-exists', '--no-owner', '--no-acl', '--exit-on-error', path.join(work, 'database.dump')])
    }
    const restoredUploads = path.join(work, 'uploads')
    if (fs.existsSync(restoredUploads)) {
      await fsp.mkdir(dataDir, { recursive: true })
      const incoming = path.join(dataDir, `.uploads-restored-${stamp()}`)
      await fsp.cp(restoredUploads, incoming, { recursive: true, force: true })
      const current = path.join(dataDir, 'uploads')
      if (fs.existsSync(current)) await fsp.rename(current, path.join(dataDir, `uploads-before-restore-${stamp()}`))
      await fsp.rename(incoming, current)
    }
    return { ok: true, manifest }
  } finally {
    await fsp.rm(work, { recursive: true, force: true }).catch(() => {})
    if (stopped && start) await start().catch(() => {})
  }
}

module.exports = { createBackup, restoreBackup, validateArchive, cleanDatabaseUrl }
