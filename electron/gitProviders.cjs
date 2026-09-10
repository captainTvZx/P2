const { app, ipcMain, safeStorage } = require('electron')
const fs = require('node:fs')
const path = require('node:path')

// Uzak sunucu bağlantıları (GitHub, GitLab, Gitea). Jetonlar diskte safeStorage
// ile şifreli durur; hiçbir zaman depo yapılandırmasına ya da uzak adrese yazılmaz.
// Ağ komutlarında yalnızca o isteğe özel bir Authorization başlığı olarak geçer.
const storePath = () => path.join(app.getPath('userData'), 'git-connections.bin')

function readConnections() {
  try { return JSON.parse(safeStorage.decryptString(fs.readFileSync(storePath()))) } catch { return [] }
}
function writeConnections(rows) {
  try { fs.writeFileSync(storePath(), safeStorage.encryptString(JSON.stringify(rows))) } catch {}
}
// Arayüze jeton gönderilmez; yalnızca kimin bağlı olduğu görünür.
const publicShape = (row) => ({ id: row.id, kind: row.kind, host: row.host, login: row.login, name: row.name, avatar: row.avatar })

const trimHost = (value) => String(value || '').trim().replace(/\/+$/, '')
const apiRoot = (kind, host) => kind === 'github' ? 'https://api.github.com' : `${trimHost(host)}/api/${kind === 'gitlab' ? 'v4' : 'v1'}`

function authHeaders(connection) {
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
    throw new Error(response.status === 401 ? 'Jeton kabul edilmedi. Yetkileri ve süresini kontrol et.' : `${response.status} ${text.slice(0, 160)}`)
  }
  return response.json()
}

async function identify(connection) {
  if (connection.kind === 'gitlab') {
    const me = await call(connection, '/user')
    return { login: me.username, name: me.name || me.username, avatar: me.avatar_url || '' }
  }
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

function findConnectionForUrl(url) {
  try {
    const host = new URL(url).host
    return readConnections().find(row => {
      if (row.kind === 'github') return host === 'github.com'
      try { return new URL(row.host).host === host } catch { return false }
    }) || null
  } catch { return null }
}

// git komutlarına eklenen kimlik doğrulama seçenekleri.
function authArgs(url) {
  const connection = findConnectionForUrl(url)
  return connection ? ['-c', `http.extraHeader=${basicHeader(connection)}`] : []
}

function registerProviderIpc() {
  ipcMain.handle('git:connections', async () => readConnections().map(publicShape))

  ipcMain.handle('git:connect', async (_event, { kind, host, token }) => {
    try {
      const draft = { kind, host: kind === 'github' ? 'https://github.com' : trimHost(host), token: String(token || '').trim() }
      if (!draft.token) return { ok: false, error: 'Erişim jetonu gerekli.' }
      if (kind !== 'github' && !/^https?:\/\//.test(draft.host)) return { ok: false, error: 'Sunucu adresi http:// ya da https:// ile başlamalı.' }
      const who = await identify(draft)
      const rows = readConnections().filter(row => !(row.kind === draft.kind && row.host === draft.host))
      const saved = { id: `${draft.kind}:${draft.host}`, ...draft, ...who }
      writeConnections([saved, ...rows])
      return { ok: true, connection: publicShape(saved) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })

  ipcMain.handle('git:disconnect', async (_event, { id }) => {
    writeConnections(readConnections().filter(row => row.id !== id))
    return { ok: true }
  })

  ipcMain.handle('git:remote-repos', async (_event, { id }) => {
    try {
      const connection = readConnections().find(row => row.id === id)
      if (!connection) return { ok: false, error: 'Bağlantı bulunamadı.' }
      return { ok: true, repos: await listRepos(connection) }
    } catch (error) { return { ok: false, error: String(error?.message || error) } }
  })
}

// Saf yardımcılar testten de çağrılabilsin diye dışa veriliyor.
module.exports = { registerProviderIpc, authArgs, findConnectionForUrl, __internals: { normalize, basicHeader, apiRoot, identify, listRepos } }
