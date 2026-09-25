// Pano uçları: sprint aç/başlat, görev oluştur, taşı, düzenle, sil.
import fs from 'node:fs'
import jwt from 'jsonwebtoken'
import pg from 'pg'

const env = fs.readFileSync('.env', 'utf8')
const pick = key => env.match(new RegExp(`${key}="?([^"\r\n]+)`))[1]
const db = new pg.Client({ connectionString: pick('DATABASE_URL') })
await db.connect()
const org = (await db.query('select id from "Organization" limit 1')).rows[0].id
const owner = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and role=\'OWNER\' limit 1', [org])).rows[0].userId
const other = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and "userId"<>$2 limit 1', [org, owner])).rows[0]?.userId || owner
const token = jwt.sign({ sub: owner, username: 'e2e' }, pick('JWT_ACCESS_SECRET'), { expiresIn: '15m' })

const base = `http://127.0.0.1:4000/organizations/${org}/board`
const call = async (path, init) => {
  const response = await fetch(base + path, { ...init, headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` } })
  const body = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(`${init?.method || 'GET'} ${path} → ${response.status} ${JSON.stringify(body)}`)
  return body
}
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) process.exitCode = 1 }

const empty = await call('')
check('pano boş açılıyor', Array.isArray(empty.tasks) && Array.isArray(empty.sprints))

const sprint = await call('/sprints', { method: 'POST', body: JSON.stringify({ name: 'E2E Sprint', goal: 'panoyu doğrula', endsAt: new Date(Date.now() + 6e8).toISOString() }) })
check('sprint oluştu', sprint.id && sprint.status === 'PLANNED', sprint.name)
const started = await call(`/sprints/${sprint.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'ACTIVE' }) })
check('sprint başladı', started.status === 'ACTIVE' && started.startsAt, String(started.startsAt))

const first = await call('/tasks', { method: 'POST', body: JSON.stringify({ title: 'İlk görev', description: 'açıklama', sprintId: sprint.id, assigneeId: other, priority: 'HIGH', points: 3, dueAt: new Date(Date.now() + 864e5).toISOString() }) })
const second = await call('/tasks', { method: 'POST', body: JSON.stringify({ title: 'İkinci görev', sprintId: sprint.id, status: 'TODO' }) })
check('görevler oluştu', first.number > 0 && second.number === first.number + 1, `#${first.number}, #${second.number}`)
check('atama ve puan kaydedildi', first.assigneeId === other && first.points === 3 && first.priority === 'HIGH')
check('yeni kart en üste giriyor', second.position < first.position, `${second.position} < ${first.position}`)

const moved = await call(`/tasks/${first.id}/move`, { method: 'PATCH', body: JSON.stringify({ status: 'IN_PROGRESS', order: [first.id] }) })
check('sütun değişti', moved.tasks.find(task => task.id === first.id).status === 'IN_PROGRESS')

const edited = await call(`/tasks/${second.id}`, { method: 'PATCH', body: JSON.stringify({ title: 'İkinci görev (düzenli)', assigneeId: '', sprintId: '', status: 'REVIEW' }) })
check('düzenleme ve backlog’a alma', edited.title.endsWith('(düzenli)') && edited.sprintId === null && edited.assigneeId === null && edited.status === 'REVIEW')

const full = await call('')
check('liste iki görev döndürüyor', full.tasks.length === 2 && full.sprints.some(item => item.id === sprint.id))

await call(`/sprints/${sprint.id}`, { method: 'DELETE' })
const afterSprint = await call('')
check('sprint silinince görevler backlog’a düşüyor', !afterSprint.sprints.some(item => item.id === sprint.id) && afterSprint.tasks.every(task => task.sprintId === null))

const invalid = await fetch(`${base}/tasks/${first.id}/move`, { method: 'PATCH', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ status: 'NOPE' }) })
check('geçersiz sütun reddediliyor', invalid.status === 400, String(invalid.status))

const anon = await fetch(base)
check('jetonsuz erişim reddediliyor', anon.status === 401, String(anon.status))

for (const id of [first.id, second.id]) await call(`/tasks/${id}`, { method: 'DELETE' })
const cleaned = await call('')
check('temizlendi', cleaned.tasks.length === 0 && cleaned.sprints.length === 0)
await db.end()
