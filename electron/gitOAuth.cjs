const { shell } = require('electron')
const { createHash, randomBytes } = require('node:crypto')
const { createServer } = require('node:http')

// "GitHub ile giriş yap" akışları. İki yöntem var:
//  · GitLab ve Gitea: yetkilendirme kodu + PKCE. Tarayıcı açılır, onaydan sonra
//    127.0.0.1'deki geçici sunucumuza döner. Gizli anahtar gerekmez.
//  · GitHub: cihaz akışı (device flow). Tarayıcıda kısa bir kod onaylanır;
//    masaüstü uygulamasında gizli anahtar tutmak zorunda kalmayız.
const base64url = (buffer) => buffer.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
const trimHost = (value) => String(value || '').trim().replace(/\/+$/, '')

function pkce() {
  const verifier = base64url(randomBytes(48))
  return { verifier, challenge: base64url(createHash('sha256').update(verifier).digest()) }
}

const SCOPES = { gitlab: 'api read_user', gitea: 'read:user write:repository read:repository', github: 'repo read:user' }

function endpoints(kind, host) {
  if (kind === 'gitlab') return { authorize: `${host}/oauth/authorize`, token: `${host}/oauth/token` }
  return { authorize: `${host}/login/oauth/authorize`, token: `${host}/login/oauth/access_token` }
}

async function postForm(url, body) {
  const response = await fetch(url, {
    method: 'POST',
    headers: { Accept: 'application/json', 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams(body).toString(),
  })
  const text = await response.text()
  let data = {}
  try { data = JSON.parse(text) } catch { data = Object.fromEntries(new URLSearchParams(text)) }
  if (!response.ok || data.error) throw new Error(data.error_description || data.error || `${response.status} ${text.slice(0, 160)}`)
  return data
}

// GitLab / Gitea: tarayıcıda onay, 127.0.0.1'e dönüş, PKCE ile jeton değişimi.
async function loginWithCode({ kind, host, clientId }) {
  const server = trimHost(host)
  const { authorize, token } = endpoints(kind, server)
  const { verifier, challenge } = pkce()
  const state = base64url(randomBytes(16))

  let port = 0
  const waiting = new Promise((resolve, reject) => {
    const http = createServer((request, response) => {
      const url = new URL(request.url, 'http://127.0.0.1')
      if (url.pathname !== '/callback') { response.statusCode = 404; response.end('yok'); return }
      const code = url.searchParams.get('code')
      const error = url.searchParams.get('error_description') || url.searchParams.get('error')
      response.setHeader('content-type', 'text/html; charset=utf-8')
      response.end(`<!doctype html><meta charset="utf-8"><title>P2</title>
        <body style="margin:0;display:grid;place-items:center;height:100vh;background:#14171c;color:#dfe2e6;font:15px system-ui">
        <div style="text-align:center"><div style="font-size:32px">${error ? '✕' : '✓'}</div>
        <p>${error ? 'Giriş tamamlanamadı.' : 'Giriş tamamlandı, P2’ye dönebilirsin.'}</p></div>`)
      http.close(); clearTimeout(timer)
      if (error) reject(new Error(error))
      else if (!code || url.searchParams.get('state') !== state) reject(new Error('Yanıt doğrulanamadı.'))
      else resolve(code)
    })
    const timer = setTimeout(() => { http.close(); reject(new Error('Giriş zaman aşımına uğradı.')) }, 300000)
    http.on('error', reject)
    http.listen(0, '127.0.0.1', () => { port = http.address().port })
  })

  // Sunucunun portu belli olana kadar bekle, sonra tarayıcıyı aç.
  for (let tick = 0; tick < 100 && !port; tick++) await new Promise(resolve => setTimeout(resolve, 10))
  if (!port) throw new Error('Yerel dinleyici başlatılamadı.')
  const redirectUri = `http://127.0.0.1:${port}/callback`
  const url = new URL(authorize)
  url.search = new URLSearchParams({
    client_id: clientId, redirect_uri: redirectUri, response_type: 'code', state,
    scope: SCOPES[kind], code_challenge: challenge, code_challenge_method: 'S256',
  }).toString()
  await shell.openExternal(url.toString())

  const code = await waiting
  const granted = await postForm(token, {
    grant_type: 'authorization_code', client_id: clientId, code,
    redirect_uri: redirectUri, code_verifier: verifier,
  })
  return {
    accessToken: granted.access_token,
    refreshToken: granted.refresh_token || '',
    expiresAt: granted.expires_in ? Date.now() + Number(granted.expires_in) * 1000 : 0,
    redirectUri,
  }
}

// GitHub: cihaz akışı. Kullanıcıya kısa bir kod verilir, tarayıcıda onaylanır.
// host yalnızca kendi sunucusunu kullananlar (ve testler) için; varsayılan github.com.
async function startDeviceLogin({ clientId, host }) {
  const server = trimHost(host) || 'https://github.com'
  const data = await postForm(`${server}/login/device/code`, { client_id: clientId, scope: SCOPES.github })
  return {
    deviceCode: data.device_code,
    userCode: data.user_code,
    verificationUri: data.verification_uri,
    interval: Math.max(Number(data.interval) || 5, 5),
    expiresIn: Number(data.expires_in) || 900,
  }
}

async function pollDeviceLogin({ clientId, deviceCode, host }) {
  const server = trimHost(host) || 'https://github.com'
  try {
    const granted = await postForm(`${server}/login/oauth/access_token`, {
      client_id: clientId, device_code: deviceCode,
      grant_type: 'urn:ietf:params:oauth:grant-type:device_code',
    })
    return { status: 'ok', accessToken: granted.access_token, refreshToken: granted.refresh_token || '', expiresAt: granted.expires_in ? Date.now() + Number(granted.expires_in) * 1000 : 0 }
  } catch (error) {
    const message = String(error?.message || '')
    if (/authorization_pending|pending/i.test(message)) return { status: 'pending' }
    if (/slow_down/i.test(message)) return { status: 'slow' }
    if (/expired_token|expired/i.test(message)) return { status: 'expired' }
    if (/access_denied|denied/i.test(message)) return { status: 'denied' }
    return { status: 'error', error: message }
  }
}

// Süresi dolan jetonları sessizce yeniler; kullanıcı tekrar giriş yapmaz.
async function refresh({ kind, host, clientId, refreshToken }) {
  if (!refreshToken) return null
  const { token } = endpoints(kind, trimHost(kind === 'github' ? 'https://github.com' : host))
  const granted = await postForm(token, { grant_type: 'refresh_token', refresh_token: refreshToken, client_id: clientId })
  return {
    accessToken: granted.access_token,
    refreshToken: granted.refresh_token || refreshToken,
    expiresAt: granted.expires_in ? Date.now() + Number(granted.expires_in) * 1000 : 0,
  }
}

module.exports = { loginWithCode, startDeviceLogin, pollDeviceLogin, refresh, SCOPES, endpoints, pkce, postForm }
