// Geri dönüşüm: silinen proje listeleniyor mu, kalıcı silinince proje kodu
// yeniden kullanılabiliyor mu? (Silinen proje kodu kayıtlı kaldığı için aynı
// adla yeni proje açılamıyordu.)
import fs from 'node:fs'
import { randomUUID } from 'node:crypto'
import jwt from 'jsonwebtoken'
import pg from 'pg'

const api = process.env.P2_API || 'http://127.0.0.1:4000'
const env = fs.readFileSync('.env', 'utf8')
const pick = key => env.match(new RegExp(`${key}="?([^"\r\n]+)`))[1]
const db = new pg.Client({ connectionString: pick('DATABASE_URL') })
await db.connect()
const organizationId = (await db.query('select id from "Organization" limit 1')).rows[0].id
const owner = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and role=\'OWNER\' limit 1', [organizationId])).rows[0].userId
const token = jwt.sign({ sub: owner, username: 'purge-test' }, pick('JWT_ACCESS_SECRET'), { expiresIn: '10m' })

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }
const call = async (path, init) => {
  const response = await fetch(api + path, { ...init, headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` } })
  const body = await response.json().catch(() => ({}))
  return { status: response.status, body }
}

const slug = `e2e-purge-${Date.now()}`
const projectId = randomUUID()
await db.query(
  'insert into "Project" ("id","name","slug","ownerId","organizationId","createdAt","updatedAt") values ($1,$2,$3,$4,$5,now(),now())',
  [projectId, 'E2E Silinecek', slug, owner, organizationId],
)

try {
  // Henüz silinmemiş: kalıcı silme reddedilmeli.
  const erken = await call(`/organizations/${organizationId}/admin/projects/${projectId}/purge`, { method: 'DELETE' })
  check('silinmemiş proje kalıcı silinemiyor', erken.status === 409, String(erken.status))

  // Projeyi çöpe at (uygulamanın yaptığı gibi soft delete).
  await db.query('update "Project" set "deletedAt"=now(), "purgeAt"=now() + interval \'30 days\' where id=$1', [projectId])

  const liste = await call(`/organizations/${organizationId}/admin/projects/trash`)
  const kayit = (liste.body || []).find(item => item.id === projectId)
  check('silinen proje geri dönüşümde listeleniyor', Boolean(kayit), kayit ? `${kayit.name} (${kayit.slug})` : String(liste.status))
  check('liste silme ve temizlik tarihini veriyor', Boolean(kayit?.deletedAt && kayit?.purgeAt))

  // Aynı kodla yeni proje: kalıcı silmeden önce çakışmalı.
  const cakisma = await call(`/organizations/${organizationId}/projects`, { method: 'POST', body: JSON.stringify({ name: 'E2E Yeniden', slug }) })
  check('kalıcı silmeden aynı kod kullanılamıyor', cakisma.status >= 400, String(cakisma.status))

  const silme = await call(`/organizations/${organizationId}/admin/projects/${projectId}/purge`, { method: 'DELETE' })
  check('kalıcı silme başarılı', silme.status === 200 && silme.body?.ok === true, String(silme.status))
  const kalan = await db.query('select count(*)::int as n from "Project" where id=$1', [projectId])
  check('kayıt veritabanından gitti', kalan.rows[0].n === 0)

  const yeniden = await call(`/organizations/${organizationId}/projects`, { method: 'POST', body: JSON.stringify({ name: 'E2E Yeniden', slug }) })
  check('aynı kodla yeni proje açılabiliyor', yeniden.status === 200 || yeniden.status === 201, String(yeniden.status))
  if (yeniden.body?.id) await db.query('delete from "Project" where id=$1', [yeniden.body.id])

  const yetkisiz = await fetch(`${api}/organizations/${organizationId}/admin/projects/trash`)
  check('jetonsuz erişim reddediliyor', yetkisiz.status === 401, String(yetkisiz.status))
} finally {
  await db.query('delete from "Project" where id=$1 or slug=$2', [projectId, slug])
  await db.end()
}
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
