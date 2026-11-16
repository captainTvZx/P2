// Proje bazlı yapı göçünün doğrulaması (docs/proje-bazli-yapi.md, "Doğrulama").
//
// Bir veritabanı kopyası üzerinde, verilen sunucu ikilisini kısa süre açıp her kullanıcı için
// "hangi projeleri görüyor" ve "hangi kanalda hangi yetkileri var" anlık görüntüsünü alır.
// Kişisel veri yazılmaz: yalnız kimlikler (uuid) ve yetki adları.
//
//   node tools/project-scope-verify.mjs snapshot <exe> <DATABASE_URL> <çıktı.json>
//   node tools/project-scope-verify.mjs compare <önce.json> <sonra.json>
//
// Erişim belirteçleri test sırrıyla (C:/p2-pg/test-env.json JWT_ACCESS_SECRET) üretilir; sunucu da
// aynı sırla açılır. Göç öncesi eski ikili, sonrası yeni ikiliyle çalıştırılır.
import { spawn } from 'node:child_process'
import { createHmac } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { sql, testEnv, sleep } from './lib.mjs'

const b64 = v => Buffer.from(typeof v === 'string' ? v : JSON.stringify(v)).toString('base64url')
function token(secret, sub, username) {
  const now = Math.floor(Date.now() / 1000)
  const head = b64({ alg: 'HS256', typ: 'JWT' })
  const body = b64({ sub, username, iat: now, exp: now + 3600 })
  return `${head}.${body}.${createHmac('sha256', secret).update(`${head}.${body}`).digest('base64url')}`
}

async function snapshot(exe, url, out) {
  const env = testEnv()
  const port = 4177
  const child = spawn(exe, [], { env: { ...process.env, ...env, DATABASE_URL: url, PORT: String(port), HOST: '127.0.0.1', RUST_LOG: 'error' }, stdio: 'ignore' })
  const base = `http://127.0.0.1:${port}`
  try {
    for (let i = 0; i < 100; i++) { try { if ((await fetch(base + '/health')).ok) break } catch {} await sleep(100) }
    const users = sql(`select u.id, u.username, m."organizationId" as org from "User" u join "OrganizationMember" m on m."userId" = u.id order by u.id`, [], url)
    const result = {}
    for (const u of users) {
      const auth = { authorization: `Bearer ${token(env.JWT_ACCESS_SECRET, u.id, u.username)}` }
      const get = async path => { const r = await fetch(base + path, { headers: auth }); return r.ok ? r.json() : { status: r.status } }
      const projects = await get(`/organizations/${u.org}/projects`)
      const perms = await get(`/organizations/${u.org}/permissions/me`)
      const channels = {}
      for (const [id, list] of Object.entries(perms.channels || {})) channels[id] = [...list].sort()
      result[u.id] = { projects: Array.isArray(projects) ? projects.map(p => p.id).sort() : projects, channels }
    }
    writeFileSync(out, JSON.stringify(result, null, 1))
    console.log(`${Object.keys(result).length} kullanıcı → ${out}`)
  } finally { child.kill() }
}

function compare(beforeFile, afterFile) {
  const before = JSON.parse(readFileSync(beforeFile, 'utf8'))
  const after = JSON.parse(readFileSync(afterFile, 'utf8'))
  let same = 0, projectDiff = 0, lostChannel = 0, lostPerm = 0, gainedChannel = 0, gainedPerm = 0
  for (const [user, b] of Object.entries(before)) {
    const a = after[user] || { projects: [], channels: {} }
    if (JSON.stringify(b.projects) !== JSON.stringify(a.projects)) { projectDiff++; console.log(`PROJE FARKI ${user.slice(0, 8)}…: ${JSON.stringify(b.projects)} → ${JSON.stringify(a.projects)}`) }
    else same++
    for (const [ch, list] of Object.entries(b.channels)) {
      const now = a.channels[ch]
      if (!now) { lostChannel++; continue }
      const lost = list.filter(p => !now.includes(p) && !RETIRED.includes(p))
      const gained = now.filter(p => !list.includes(p))
      if (lost.length) { lostPerm += lost.length; console.log(`YETKİ KAYBI ${user.slice(0, 8)}… kanal ${ch.slice(0, 8)}…: ${lost.join(',')}`) }
      if (gained.length) gainedPerm += gained.length
    }
    for (const ch of Object.keys(a.channels)) if (!b.channels[ch]) gainedChannel++
  }
  console.log(JSON.stringify({ users: Object.keys(before).length, sameProjects: same, projectDiff, lostChannel, lostPerm, gainedChannel, gainedPerm }))
  if (projectDiff || lostChannel || lostPerm) process.exit(1)
}

// Katalogdan bilerek kalkan yetkiler: proje açmak artık herkese açık (yetki gerekmez).
const RETIRED = ['project.create']

const [cmd, ...args] = process.argv.slice(2)
if (cmd === 'snapshot') await snapshot(...args)
else if (cmd === 'compare') compare(...args)
else { console.error('kullanım: snapshot <exe> <url> <out> | compare <önce> <sonra>'); process.exit(2) }
