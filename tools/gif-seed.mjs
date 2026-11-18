// Keşfet ve profil fotoğrafları için hareketli GIF örnekleri (yalnız yerel test veritabanı, API üzerinden).
// GIF'ler `apps/p2-client/examples/gif_ornekleri.rs` ile yordamsal üretilir (dış görsel yok); tekrar
// çalıştırılabilir: her çalıştırmada dosyalar yeniden yüklenip bağlanır.
//
//   node tools/gif-seed.mjs [--regen] [--out <klasör>]
//     P2_API      hedef sunucu (varsayılan http://127.0.0.1:4010 — kullanıcının yerel test sunucusu)
//     --regen     GIF'leri yeniden üret (yoksa klasörde eksik olduğunda üretilir)
//     --out       GIF klasörü (varsayılan: %TEMP%/p2-gif-ornekleri)
//
// Önkoşul: tools/discover-seed.mjs çalışmış olmalı (projelerin sahibi `kesif00`, üyeler `kesif01…`).
// Test hesaplarının parolası `Parola123`.
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { spawnSync } from 'node:child_process'
import { REPO } from './lib.mjs'

const API = process.env.P2_API || 'http://127.0.0.1:4010'
const args = process.argv.slice(2)
const OUT = args.includes('--out') ? args[args.indexOf('--out') + 1] : join(tmpdir(), 'p2-gif-ornekleri')

// Proje (kesif00'ın) → ikon / afiş GIF'i. Listede olmayan projeler PNG görselleriyle kalır.
const PROJECTS = {
  'kesif-synthwave': { icon: 'ikon-altigen-neon', cover: 'afis-synthwave' },
  'kesif-northwind': { icon: 'ikon-halkalar', cover: 'afis-daglar' },
  'kesif-acik-akademi': { icon: 'ikon-halkalar-yesil', cover: 'afis-aurora' },
  'kesif-piksel-kahvesi': { icon: 'ikon-lav', cover: 'afis-piksel-sehir' },
  'kesif-shader-gunlukleri': { cover: 'afis-plazma' },
  'kesif-orbit-jam': { icon: 'ikon-yorunge', cover: 'afis-dalgalar' },
  'kesif-rust-atolyesi': { icon: 'ikon-altigen' },
}
// Kullanıcı → profil fotoğrafı GIF'i.
const USERS = {
  p2test: 'avatar-p2',
  rustbot: 'avatar-rb',
  rustbot2: 'avatar-r2',
  kesif00: 'avatar-k',
  kesif01: 'avatar-yorunge',
  kesif02: 'avatar-lav',
  kesif03: 'avatar-halkalar',
  kesif04: 'avatar-altigen',
}

const needed = new Set([...Object.values(PROJECTS).flatMap(p => Object.values(p)), ...Object.values(USERS)])
if (args.includes('--regen') || [...needed].some(n => !existsSync(join(OUT, n + '.gif')))) {
  console.log(`GIF'ler üretiliyor → ${OUT}`)
  const r = spawnSync('cargo', ['run', '--release', '-q', '-p', 'p2-client', '--example', 'gif_ornekleri', '--', OUT], { cwd: REPO, stdio: 'inherit', env: { CARGO_TARGET_DIR: 'C:/p2-rust-target', ...process.env } })
  if (r.status !== 0) throw new Error('GIF üretici başarısız')
}

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
async function login(username) {
  const auth = (await call('POST', '/auth/login', { identifier: username, password: 'Parola123' })).body
  if (!auth?.accessToken) throw new Error(`${username} girişi başarısız: ${JSON.stringify(auth)}`)
  return auth
}
async function upload(name, token) {
  const data = readFileSync(join(OUT, name + '.gif'))
  const r = await call('POST', '/uploads', { name: name + '.gif', type: 'image/gif', data: 'data:image/gif;base64,' + data.toString('base64') }, token)
  if (!r.body?.url) throw new Error(`${name} yüklenemedi: ${JSON.stringify(r.body)}`)
  return r.body.url.slice(r.body.url.indexOf('/uploads/'))
}

const health = await fetch(API + '/health').then(r => r.json()).catch(() => null)
if (!health) throw new Error(`${API} yanıt vermiyor (P2_API ile başka sunucu verilebilir)`)
console.log(`Sunucu: ${API} (${health.version})`)

// Projeler: kesif00 sahibi, PATCH /projects/:id/admin { iconUrl, coverUrl }.
const owner = await login('kesif00')
const mine = ((await call('GET', '/projects', null, owner.accessToken)).body || []).filter(p => p.ownerId === owner.user.id)
for (const [slug, gifs] of Object.entries(PROJECTS)) {
  const project = mine.find(p => p.slug === slug)
  if (!project) { console.log(`  ! ${slug} yok (önce tools/discover-seed.mjs)`); continue }
  const patch = {}
  if (gifs.icon) patch.iconUrl = await upload(gifs.icon, owner.accessToken)
  if (gifs.cover) patch.coverUrl = await upload(gifs.cover, owner.accessToken)
  const r = await call('PATCH', `/projects/${project.id}/admin`, patch, owner.accessToken)
  if (r.status !== 200) throw new Error(`${slug} güncellenemedi: ${JSON.stringify(r.body)}`)
  console.log(`  proje ${project.name}: ${Object.values(gifs).join(', ')}`)
}

// Profil fotoğrafları: her kullanıcı kendi adına PATCH /auth/profile { avatarUrl }.
for (const [username, gif] of Object.entries(USERS)) {
  let auth
  try { auth = await login(username) } catch (e) { console.log(`  ! ${e.message}`); continue }
  const avatarUrl = await upload(gif, auth.accessToken)
  const r = await call('PATCH', '/auth/profile', { avatarUrl }, auth.accessToken)
  if (r.status !== 200) throw new Error(`${username} profili güncellenemedi: ${JSON.stringify(r.body)}`)
  console.log(`  kullanıcı ${username}: ${gif}`)
}
console.log('Bitti.')
