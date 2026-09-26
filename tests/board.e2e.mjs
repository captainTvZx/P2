// Pano uçları: her proje kendi panosunu taşır. Sprint aç/başlat/sil, görev
// oluştur/taşı/düzenle/sil ve projeler arası yalıtım denenir.
// Adres: P2_API (varsayılan http://127.0.0.1:4000).
import fs from 'node:fs'
import jwt from 'jsonwebtoken'
import pg from 'pg'

const env = fs.readFileSync('.env', 'utf8')
const pick = key => env.match(new RegExp(`${key}="?([^"\r\n]+)`))[1]
const api = process.env.P2_API || 'http://127.0.0.1:4000'
const db = new pg.Client({ connectionString: pick('DATABASE_URL') })
await db.connect()
const org = (await db.query('select id from "Organization" limit 1')).rows[0].id
const owner = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and role=\'OWNER\' limit 1', [org])).rows[0].userId
const other = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and "userId"<>$2 limit 1', [org, owner])).rows[0]?.userId || owner
const token = jwt.sign({ sub: owner, username: 'e2e' }, pick('JWT_ACCESS_SECRET'), { expiresIn: '15m' })

// İki geçici proje: panoların birbirine karışmadığı burada görülür.
const projects = []
for (const suffix of ['a', 'b']) {
  const id = `e2e-board-${suffix}-${Date.now()}`
  await db.query(
    'insert into "Project" ("id","name","slug","ownerId","organizationId","createdAt","updatedAt") values ($1,$2,$3,$4,$5,now(),now())',
    [id, `E2E Pano ${suffix.toUpperCase()}`, id, owner, org],
  )
  projects.push(id)
}
const [projectA, projectB] = projects
const boardUrl = project => `${api}/organizations/${org}/projects/${project}/board`

const call = async (project, path, init) => {
  const response = await fetch(boardUrl(project) + path, { ...init, headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` } })
  const body = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(`${init?.method || 'GET'} ${path} → ${response.status} ${JSON.stringify(body)}`)
  return body
}
let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

try {
  const empty = await call(projectA, '')
  check('proje panosu boş açılıyor', Array.isArray(empty.tasks) && !empty.tasks.length && !empty.sprints.length)

  const sprint = await call(projectA, '/sprints', { method: 'POST', body: JSON.stringify({ name: 'E2E Sprint', goal: 'panoyu doğrula', endsAt: new Date(Date.now() + 6e8).toISOString() }) })
  check('sprint oluştu', sprint.id && sprint.status === 'PLANNED' && sprint.projectId === projectA, sprint.name)
  const started = await call(projectA, `/sprints/${sprint.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'ACTIVE' }) })
  check('sprint başladı', started.status === 'ACTIVE' && Boolean(started.startsAt))

  // Takvimden seçilen aralık: başlangıç ve bitiş birlikte kaydedilir.
  const aralik = { startsAt: new Date('2026-10-05T00:00:00.000Z').toISOString(), endsAt: new Date('2026-10-18T23:59:59.000Z').toISOString() }
  const dated = await call(projectA, `/sprints/${sprint.id}`, { method: 'PATCH', body: JSON.stringify({ name: 'E2E Sprint (2. tur)', goal: 'tarih aralığı', ...aralik }) })
  check('sprint tarih aralığı ve adı güncelleniyor', dated.name.includes('2. tur') && dated.startsAt === aralik.startsAt && new Date(dated.endsAt).toISOString().slice(0, 10) === '2026-10-18', `${dated.startsAt} → ${dated.endsAt}`)

  const first = await call(projectA, '/tasks', { method: 'POST', body: JSON.stringify({ title: 'İlk görev', description: 'açıklama', sprintId: sprint.id, assigneeId: other, priority: 'HIGH', points: 3, dueAt: new Date(Date.now() + 864e5).toISOString() }) })
  const second = await call(projectA, '/tasks', { method: 'POST', body: JSON.stringify({ title: 'İkinci görev', sprintId: sprint.id, status: 'TODO' }) })
  check('görev numaraları projede #1\'den başlıyor', first.number === 1 && second.number === 2, `#${first.number}, #${second.number}`)
  check('atama, puan ve proje kaydedildi', first.assigneeId === other && first.points === 3 && first.projectId === projectA)
  check('yeni kart en üste giriyor', second.position < first.position, `${second.position} < ${first.position}`)

  // İkinci proje: kendi panosu, kendi numaraları.
  const foreign = await call(projectB, '/tasks', { method: 'POST', body: JSON.stringify({ title: 'Diğer projenin görevi' }) })
  check('ikinci proje de #1\'den başlıyor', foreign.number === 1, `#${foreign.number}`)
  const boardA = await call(projectA, '')
  const boardB = await call(projectB, '')
  check('panolar birbirine karışmıyor', boardA.tasks.length === 2 && boardB.tasks.length === 1 && !boardA.tasks.some(task => task.id === foreign.id))
  check('sprint de projesine ait', boardA.sprints.length === 1 && boardB.sprints.length === 0)

  let crossed = 0
  try { await call(projectB, `/tasks/${first.id}`, { method: 'PATCH', body: JSON.stringify({ title: 'çapraz' }) }) } catch (error) { crossed = Number(/→ (\d+)/.exec(error.message)?.[1] || 0) }
  check('başka projenin görevi düzenlenemiyor', crossed === 404, String(crossed))

  const moved = await call(projectA, `/tasks/${first.id}/move`, { method: 'PATCH', body: JSON.stringify({ status: 'IN_PROGRESS', order: [first.id] }) })
  check('sütun değişti', moved.tasks.find(task => task.id === first.id).status === 'IN_PROGRESS')

  const edited = await call(projectA, `/tasks/${second.id}`, { method: 'PATCH', body: JSON.stringify({ title: 'İkinci görev (düzenli)', assigneeId: '', sprintId: '', status: 'REVIEW' }) })
  check('düzenleme ve backlog’a alma', edited.title.endsWith('(düzenli)') && edited.sprintId === null && edited.assigneeId === null && edited.status === 'REVIEW')

  await call(projectA, `/sprints/${sprint.id}`, { method: 'DELETE' })
  const afterSprint = await call(projectA, '')
  check('sprint silinince görevler backlog’a düşüyor', !afterSprint.sprints.length && afterSprint.tasks.every(task => task.sprintId === null))

  const invalid = await fetch(`${boardUrl(projectA)}/tasks/${first.id}/move`, { method: 'PATCH', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ status: 'NOPE' }) })
  check('geçersiz sütun reddediliyor', invalid.status === 400, String(invalid.status))

  const missing = await fetch(`${api}/organizations/${org}/projects/yok-boyle-proje/board`, { headers: { Authorization: `Bearer ${token}` } })
  check('olmayan proje 404', missing.status === 404, String(missing.status))

  const anon = await fetch(boardUrl(projectA))
  check('jetonsuz erişim reddediliyor', anon.status === 401, String(anon.status))
} finally {
  // Geçici projeler silinince görev ve sprint'leri de gider (ON DELETE CASCADE).
  await db.query('delete from "Project" where id = any($1)', [projects])
  const leftovers = await db.query('select count(*)::int as n from "Task" where "projectId" = any($1)', [projects])
  check('proje silinince görevleri de siliniyor', leftovers.rows[0].n === 0, String(leftovers.rows[0].n))
  await db.end()
}
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
