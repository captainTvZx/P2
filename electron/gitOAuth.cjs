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
const LOOPBACK_HOST = '127.0.0.1'
const LOOPBACK_PATH = '/'

function pkce() {
  const verifier = base64url(randomBytes(48))
  return { verifier, challenge: base64url(createHash('sha256').update(verifier).digest()) }
}

const SCOPES = { gitlab: 'api read_user', gitea: 'read:user write:repository read:repository', github: 'repo read:user' }

// Sağlayıcıların yerleşik istemci kayıtları. Gitea, git istemcileri hiçbir şey
// tanımlamadan giriş yapabilsin diye bunları kutudan çıkarır; GitHub ve GitLab
// tarafında da yaygın git istemcilerinin herkese açık (gizli anahtarsız, PKCE)
// kimlikleri var. Hangisinin kabul edildiğini sunucuya sorup buluyoruz — böylece
// kullanıcı "önce uygulama tanımla" ekranıyla hiç karşılaşmıyor.
const BUILTIN = {
  gitea: ['a4792ccc-144e-407e-86c9-5e7d8d9c3269', 'e90ee53c-94e2-48ac-9358-a874fb9e0662', 'd57cb8c4-630c-4168-8324-ec79935e18d4'],
  gitlab: ['10bfbbf46e5b760b55ce772a262d7a0205eacc417816eb84d37d0fb02c89bb97'],
  github: ['b895675a4e2cf54d5c6c', '0120e057bd645470c1ed'],
}

// Sunucu istemciyi tanımıyorsa bu hatalarla yanıt verir; kodun sahte olmasından
// şikâyet ediyorsa (invalid_grant) istemci kaydı vardır demektir.
const UNKNOWN_CLIENT = /invalid_client|unauthorized_client|client is not found|client_id|404|not found/i

async function clientAccepted({ kind, host, clientId }) {
  const { token } = endpoints(kind, trimHost(host))
  try {
    await postForm(token, {
      grant_type: 'authorization_code', client_id: clientId, code: 'p2-yoklama',
      redirect_uri: `http://${LOOPBACK_HOST}${LOOPBACK_PATH}`, code_verifier: base64url(randomBytes(32)),
    })
    return true
  } catch (error) { return !UNKNOWN_CLIENT.test(String(error?.message || '')) }
}

// Bu sunucunun kabul ettiği ilk yerleşik kimliği bulur; hiçbiri yoksa boş döner.
async function discoverClientId({ kind, host }) {
  for (const clientId of BUILTIN[kind] || []) if (await clientAccepted({ kind, host, clientId })) return clientId
  return ''
}

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
      const url = new URL(request.url, `http://${LOOPBACK_HOST}`)
      if (url.pathname !== LOOPBACK_PATH) { response.statusCode = 404; response.end('yok'); return }
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
    http.listen(0, LOOPBACK_HOST, () => { port = http.address().port })
  })

  // Sunucunun portu belli olana kadar bekle, sonra tarayıcıyı aç.
  for (let tick = 0; tick < 100 && !port; tick++) await new Promise(resolve => setTimeout(resolve, 10))
  if (!port) throw new Error('Yerel dinleyici başlatılamadı.')
  // Gitea'nin yerlesik masaustu istemcileri kok loopback URI'sine kayitlidir.
  // Public istemcilerde port serbesttir ancak yol ayni kalmalidir.
  const redirectUri = `http://${LOOPBACK_HOST}:${port}${LOOPBACK_PATH}`
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

module.exports = { loginWithCode, startDeviceLogin, pollDeviceLogin, refresh, discoverClientId, clientAccepted, SCOPES, BUILTIN, endpoints, pkce, postForm }
