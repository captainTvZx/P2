const { app, ipcMain, safeStorage } = require('electron')
const fs = require('node:fs')
const path = require('node:path')
const { loginWithCode, startDeviceLogin, pollDeviceLogin, refresh, discoverClientId, BUILTIN } = require('./gitOAuth.cjs')

// Uzak sunucu hesapları (GitHub, GitLab, Gitea). Giriş tarayıcıda yapılır; dönen
// jetonlar safeStorage ile şifreli olarak diskte durur, arayüze gönderilmez ve
// depo yapılandırmasına yazılmaz. Ağ komutlarında yalnızca o isteğe özel bir
// Authorization başlığı olarak geçer: oturum kalıcı, jeton gizli kalır.
const storePath = () => path.join(app.getPath('userData'), 'git-connections.bin')
const appsPath = () => path.join(app.getPath('userData'), 'git-oauth-apps.bin')

function readEncrypted(file) {
  try { return JSON.parse(safeStorage.decryptString(fs.readFileSync(file))) } catch { return [] }
}
function writeEncrypted(file, rows) {
  try { fs.writeFileSync(file, safeStorage.encryptString(JSON.stringify(rows))) } catch {}
}
const readConnections = () => readEncrypted(storePath())
const writeConnections = (rows) => writeEncrypted(storePath(), rows)
const readApps = () => readEncrypted(appsPath())
const writeApps = (rows) => writeEncrypted(appsPath(), rows)

const trimHost = (value) => String(value || '').trim().replace(/\/+$/, '')
const hostOf = (kind, host) => kind === 'github' ? 'https://github.com' : trimHost(host)
const appId = (kind, host) => `${kind}:${hostOf(kind, host)}`
// Arayüz yalnızca kimin bağlı olduğunu görür; jeton hiçbir zaman dışarı çıkmaz.
const publicShape = (row) => ({ id: row.id, kind: row.kind, host: row.host, login: row.login, name: row.name, avatar: row.avatar, auth: row.auth })
const publicApp = (row) => ({ id: row.id, kind: row.kind, host: row.host, clientId: row.clientId, builtin: Boolean(row.builtin) })

const apiRoot = (kind, host) => kind === 'github' ? 'https://api.github.com' : `${hostOf(kind, host)}/api/${kind === 'gitlab' ? 'v4' : 'v1'}`

function authHeaders(connection) {
  if (connection.auth === 'oauth') return { Authorization: `Bearer ${connection.token}` }
  if (connection.kind === 'gitlab') return { 'PRIVATE-TOKEN': connection.token }
  return { Authorization: `token ${connection.token}` }
}

// Git ağ komutlarında kullanılan başlık: jeton uzak adrese gömülmez.
function basicHeader(connection) {
  const user = connection.kind === 'gitlab' ? 'oauth2' : connection.kind === 'github' ? 'x-access-token' : (connection.login || 'token')
  return `Authorization: Basic ${Buffer.from(`${user}:${connection.token}`).toString('base64')}`
}

async function call(connection, pathname, params = {}) {
  const url = new URL(apiRoot(connection.kind, connection.host) + pathname)
  for (const [key, value] of Object.entries(params)) url.searchParams.set(key, String(value))
  const response = await fetch(url, { headers: { Accept: 'application/json', ...authHeaders(connection) } })
  if (!response.ok) {
    const text = await response.text().catch(() => '')
    throw new Error(response.status === 401 ? 'Oturum kabul edilmedi. Yeniden giriş yapman gerekebilir.' : `${response.status} ${text.slice(0, 160)}`)
  }
  return response.json()
}

async function identify(connection) {
  const me = await call(connection, '/user')
  // Gitea görünen adı full_name alanında verir; GitHub name alanında.
  return { login: me.login || me.username, name: me.name || me.full_name || me.login || me.username, avatar: me.avatar_url || '' }
}

// Her sağlayıcının farklı alan adlarını tek bir depo şekline indiriyoruz.
function normalize(kind, host, row) {
  if (kind === 'gitlab') return {
    id: String(row.id), name: row.path, fullName: row.path_with_namespace, description: row.description || '',
    private: row.visibility !== 'public', url: row.http_url_to_repo, web: row.web_url,
    defaultBranch: row.default_branch || '', updatedAt: row.last_activity_at || '', host,
  }
  return {
    id: String(row.id), name: row.name, fullName: row.full_name || `${row.owner?.login || row.owner?.username || ''}/${row.name}`,
    description: row.description || '', private: Boolean(row.private), url: row.clone_url || row.html_url,
    web: row.html_url, defaultBranch: row.default_branch || '', updatedAt: row.updated_at || row.pushed_at || '', host,
  }
}

async function listRepos(connection) {
  const rows = connection.kind === 'gitlab'
    ? await call(connection, '/projects', { membership: true, per_page: 100, order_by: 'last_activity_at', simple: true })
    : connection.kind === 'github'
    ? await call(connection, '/user/repos', { per_page: 100, sort: 'updated', affiliation: 'owner,collaborator,organization_member' })
    : await call(connection, '/user/repos', { limit: 100 })
  const list = Array.isArray(rows) ? rows : (rows.data || [])
  return list.map(row => normalize(connection.kind, connection.host, row))
}

// Süresi dolmak üzere olan jeton sessizce yenilenir; kullanıcı tekrar giriş yapmaz.
async function freshen(connection) {
  if (connection.auth !== 'oauth' || !connection.expiresAt || !connection.refreshToken) return connection
  if (connection.expiresAt - Date.now() > 60000) return connection
  try {
    const renewed = await refresh({ kind: connection.kind, host: connection.host, clientId: connection.clientId, refreshToken: connection.refreshToken })
    if (!renewed?.accessToken) return connection
    const updated = { ...connection, token: renewed.accessToken, refreshToken: renewed.refreshToken, expiresAt: renewed.expiresAt }
    writeConnections(readConnections().map(row => row.id === updated.id ? updated : row))
    return updated
  } catch { return connection }
}

async function connectionFor(id) {
  const row = readConnections().find(item => item.id === id)
  return row ? freshen(row) : null
}

async function findConnectionForUrl(url) {
  try {
    const host = new URL(url).host
    const row = readConnections().find(item => {
      if (item.kind === 'github') return host === 'github.com'
      try { return new URL(item.host).host === host } catch { return false }
    })
    return row ? freshen(row) : null
  } catch { return null }
}

// git komutlarına eklenen kimlik doğrulama seçenekleri.
async function authArgs(url) {
  const connection = await findConnectionForUrl(url)
  return connection ? ['-c', `http.extraHeader=${basicHeader(connection)}`] : []
}

// Giriş için kullanılacak istemci kimliği: kullanıcı kendi uygulamasını
// tanımladıysa o, yoksa sunucunun yerleşik kaydı. Bulunan kimlik saklanır ki
// sonraki girişlerde sunucu yeniden yoklanmasın.
async function clientIdFor(kind, host) {
  const id = appId(kind, host)
  const known = readApps().find(row => row.id === id)
  if (known?.clientId) return known.clientId
  const found = await discoverClientId({ kind, host: hostOf(kind, host) })
  if (found) writeApps([{ id, kind, host: hostOf(kind, host), clientId: found, builtin: true }, ...readApps().filter(row => row.id !== id)])
  return found
}

// Adresten hangi sunucu yazılımı olduğunu anlar: sürüm uçları birbirinden ayrı.
async function detectKind(host) {
  const server = trimHost(host)
  try { if (/(^|\.)github\.com$/i.test(new URL(server).host)) return 'github' } catch { return '' }
  for (const [kind, probe] of [['gitea', `${server}/api/v1/version`], ['gitlab', `${server}/api/v4/version`]]) {
    try {
      const response = await fetch(probe, { headers: { Accept: 'application/json' } })
      // GitLab sürümü kimlik ister (401); Gitea herkese açık yanıtlar.
      if (response.ok || response.status === 401) return kind
    } catch {}
  }
  return ''
}

// Bir depo adresi için oturum garanti eder: bağlı değilsek tarayıcıda giriş açar.
async function ensureConnection(url) {
  const existing = await findConnectionForUrl(url)
  if (existing) return { ok: true, connection: publicShape(existing), already: true }
  let origin = ''
  try { origin = new URL(url).origin } catch { return { ok: false, error: 'Depo adresi anlaşılmadı.' } }
  const kind = await detectKind(origin)
  if (!kind) return { ok: false, error: 'Bu adreste GitHub, GitLab ya da Gitea bulunamadı.' }
  const clientId = await clientIdFor(kind, origin)
  if (!clientId) return { ok: false, error: 'Sunucu hazır bir giriş kaydı sunmuyor; kendi uygulama kimliğini tanımlaman gerekiyor.', needsApp: true, kind, host: origin }
  const granted = await loginWithCode({ kind, host: hostOf(kind, origin), clientId })
  const draft = { kind, host: origin, auth: 'oauth', clientId, token: granted.accessToken, refreshToken: granted.refreshToken, expiresAt: granted.expiresAt }
  const who = await identify(draft)
  return { ok: true, connection: publicShape(saveConnection({ ...draft, ...who })) }
}

function saveConnection(draft) {
  const saved = { ...draft, host: hostOf(draft.kind, draft.host), id: appId(draft.kind, draft.host) }
  writeConnections([saved, ...readConnections().filter(row => row.id !== saved.id)])
  return saved
}

const pendingDevice = new Map()

function registerProviderIpc() {
  ipcMain.handle('git:connections', async () => readConnections().map(publicShape))
  ipcMain.handle('git:oauth-apps', async () => readApps().map(publicApp))

  // Sağlayıcıda bir kez tanımlanan uygulama kimliği; sonrasında hep "giriş yap".
  ipcMain.handle('git:save-app', async (_event, { kind, host, clientId }) => {
    const id = appId(kind, host)
    const row = { id, kind, host: hostOf(kind, host), clientId: String(clientId || '').trim() }
    if (!row.clientId) return { ok: false, error: 'Uygulama kimliği gerekli.' }
    writeApps([row, ...readApps().filter(item => item.id !== id)])
    return { ok: true, app: publicApp(row) }
  })

  // GitLab / Gitea: tarayıcıda onay, dönüşte jeton.
  ipcMain.handle('git:login', async (_event, { kind, host }) => {
    try {
      const clientId = await clientIdFor(kind, host)
      if (!clientId) return { ok: false, error: 'Bu sunucu hazır bir giriş kaydı sunmuyor; kendi uygulama kimliğini tanımlayabilirsin.', needsApp: true }
      const granted = await loginWithCode({ kind, host: hostOf(kind, host), clientId })
      const draft = { kind, host, auth: 'oauth', clientId, token: granted.accessToken, refreshToken: granted.refreshToken, expiresAt: granted.expiresAt }
      const who = await identify(draft)
      return { ok: true, connection: publicShape(saveConnection({ ...draft, ...who })) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  // GitHub: cihaz akışı. Kod gösterilir, onay beklenirken sorgulanır.
  ipcMain.handle('git:device-start', async (_event, { kind, host }) => {
    try {
      // Cihaz akışında yoklama ayrı bir uç değil: adayları sırayla deneyip
      // hangisi kod veriyorsa onunla devam ediyoruz.
      const registration = readApps().find(row => row.id === appId(kind, host))
      const candidates = registration?.clientId ? [registration.clientId] : (BUILTIN[kind] || [])
      let started = null, clientId = '', last = ''
      for (const candidate of candidates) {
        try { started = await startDeviceLogin({ clientId: candidate, host: hostOf(kind, host) }); clientId = candidate; break }
        catch (error) { last = String(error?.message || error) }
      }
      if (!started) return { ok: false, error: last || 'Giriş başlatılamadı.', needsApp: true }
      if (!registration) writeApps([{ id: appId(kind, host), kind, host: hostOf(kind, host), clientId, builtin: true }, ...readApps().filter(row => row.id !== appId(kind, host))])
      const ticket = `${kind}:${Date.now()}`
      pendingDevice.set(ticket, { kind, host: hostOf(kind, host), clientId, deviceCode: started.deviceCode })
      return { ok: true, ticket, userCode: started.userCode, verificationUri: started.verificationUri, interval: started.interval, expiresIn: started.expiresIn }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  ipcMain.handle('git:device-poll', async (_event, { ticket }) => {
    const pending = pendingDevice.get(ticket)
    if (!pending) return { ok: false, error: 'Giriş isteği bulunamadı.' }
    const result = await pollDeviceLogin({ clientId: pending.clientId, deviceCode: pending.deviceCode, host: pending.host })
    if (result.status !== 'ok') return { ok: true, status: result.status, error: result.error }
    pendingDevice.delete(ticket)
    try {
      const draft = { kind: pending.kind, host: pending.host, auth: 'oauth', clientId: pending.clientId, token: result.accessToken, refreshToken: result.refreshToken, expiresAt: result.expiresAt }
      const who = await identify(draft)
      return { ok: true, status: 'ok', connection: publicShape(saveConnection({ ...draft, ...who })) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  // Uygulama tanımlanamayan kurulumlar için elle jeton hâlâ mümkün.
  ipcMain.handle('git:connect-token', async (_event, { kind, host, token }) => {
    try {
      const draft = { kind, host: hostOf(kind, host), auth: 'pat', token: String(token || '').trim() }
      if (!draft.token) return { ok: false, error: 'Erişim jetonu gerekli.' }
      if (kind !== 'github' && !/^https?:\/\//.test(draft.host)) return { ok: false, error: 'Sunucu adresi http:// ya da https:// ile başlamalı.' }
      const who = await identify(draft)
      return { ok: true, connection: publicShape(saveConnection({ ...draft, ...who })) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  ipcMain.handle('git:disconnect', async (_event, { id }) => {
    writeConnections(readConnections().filter(row => row.id !== id))
    return { ok: true }
  })

  // Yapıştırılan adres için sunucuyu tanır ve gerekiyorsa girişi başlatır.
  ipcMain.handle('git:detect', async (_event, { url }) => {
    try {
      const origin = new URL(url).origin
      return { ok: true, host: origin, kind: await detectKind(origin), connected: Boolean(await findConnectionForUrl(url)) }
    } catch { return { ok: false, error: 'Depo adresi anlaşılmadı.' } }
  })

  ipcMain.handle('git:ensure-auth', async (_event, { url }) => {
    try { return await ensureConnection(url) } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  ipcMain.handle('git:remote-repos', async (_event, { id }) => {
    try {
      const connection = await connectionFor(id)
      if (!connection) return { ok: false, error: 'Bağlantı bulunamadı.' }
      return { ok: true, repos: await listRepos(connection) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })
}

// Saf yardımcılar testten de çağrılabilsin diye dışa veriliyor.
module.exports = { registerProviderIpc, authArgs, findConnectionForUrl, ensureConnection, detectKind, __internals: { normalize, basicHeader, apiRoot, identify, listRepos, authHeaders, publicShape, publicApp, freshen, saveConnection, readConnections, clientIdFor } }
