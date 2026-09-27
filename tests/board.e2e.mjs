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
const boardsUrl = project => `${api}/organizations/${org}/projects/${project}/boards`

// call(proje, yol) panolar kökünden; callBoard(pano, yol) tek panonun içinden.
const send = async (url, init) => {
  const response = await fetch(url, { ...init, headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` } })
  const body = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(`${init?.method || 'GET'} ${url} → ${response.status} ${JSON.stringify(body)}`)
  return body
}
const call = (project, path, init) => send(boardsUrl(project) + path, init)
let boardA = '', boardB = ''
const onA = (path, init) => call(projectA, `/${boardA}${path}`, init)
let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

try {
  // Projede hiç pano yokken ilk istek "Genel" panosunu açar.
  const created = await call(projectA, '')
  boardA = created[0]?.id || ''
  check('ilk istekte Genel panosu açılıyor', created.length === 1 && created[0].name === 'Genel' && created[0].projectId === projectA, created[0]?.name)
  boardB = (await call(projectB, ''))[0]?.id || ''

  const empty = await onA('')
  check('pano boş açılıyor', Array.isArray(empty.tasks) && !empty.tasks.length && !empty.sprints.length && empty.board.id === boardA)

  // Aynı projede ikinci takım panosu.
  const team = await call(projectA, '', { method: 'POST', body: JSON.stringify({ name: 'Sanat takımı' }) })
  check('projeye ikinci pano eklendi', team.name === 'Sanat takımı' && team.id !== boardA, team.name)
  const renamed = await call(projectA, `/${team.id}`, { method: 'PATCH', body: JSON.stringify({ name: 'Sanat' }) })
  check('pano yeniden adlandırılıyor', renamed.name === 'Sanat')

  const sprint = await onA('/sprints', { method: 'POST', body: JSON.stringify({ name: 'E2E Sprint', goal: 'panoyu doğrula', endsAt: new Date(Date.now() + 6e8).toISOString() }) })
  check('sprint oluştu', sprint.id && sprint.status === 'PLANNED' && sprint.projectId === projectA, sprint.name)
  const started = await onA(`/sprints/${sprint.id}`, { method: 'PATCH', body: JSON.stringify({ status: 'ACTIVE' }) })
  check('sprint başladı', started.status === 'ACTIVE' && Boolean(started.startsAt))

  // Takvimden seçilen aralık: başlangıç ve bitiş birlikte kaydedilir.
  const aralik = { startsAt: new Date('2026-10-05T00:00:00.000Z').toISOString(), endsAt: new Date('2026-10-18T23:59:59.000Z').toISOString() }
  const dated = await onA(`/sprints/${sprint.id}`, { method: 'PATCH', body: JSON.stringify({ name: 'E2E Sprint (2. tur)', goal: 'tarih aralığı', ...aralik }) })
  check('sprint tarih aralığı ve adı güncelleniyor', dated.name.includes('2. tur') && dated.startsAt === aralik.startsAt && new Date(dated.endsAt).toISOString().slice(0, 10) === '2026-10-18', `${dated.startsAt} → ${dated.endsAt}`)

  const first = await onA('/tasks', { method: 'POST', body: JSON.stringify({ title: 'İlk görev', description: 'açıklama', sprintId: sprint.id, assigneeId: other, priority: 'HIGH', points: 3, dueAt: new Date(Date.now() + 864e5).toISOString() }) })
  const second = await onA('/tasks', { method: 'POST', body: JSON.stringify({ title: 'İkinci görev', sprintId: sprint.id, status: 'TODO' }) })
  check('görev numaraları projede #1\'den başlıyor', first.number === 1 && second.number === 2, `#${first.number}, #${second.number}`)
  check('atama, puan ve proje kaydedildi', first.assigneeId === other && first.points === 3 && first.projectId === projectA)
  check('yeni kart en üste giriyor', second.position < first.position, `${second.position} < ${first.position}`)

  // İkinci proje: kendi panosu, kendi numaraları.
  const foreign = await call(projectB, `/${boardB}/tasks`, { method: 'POST', body: JSON.stringify({ title: 'Diğer projenin görevi' }) })
  check('ikinci proje de #1\'den başlıyor', foreign.number === 1, `#${foreign.number}`)

  // Aynı projenin ikinci panosu: ayrı kart listesi, ortak numara dizisi.
  const teamTask = await call(projectA, `/${team.id}/tasks`, { method: 'POST', body: JSON.stringify({ title: 'Sanat panosunun görevi' }) })
  check('ikinci panoda numara proje dizisinden devam ediyor', teamTask.number === 3, `#${teamTask.number}`)

  const contentA = await onA('')
  const contentTeam = await call(projectA, `/${team.id}`)
  const contentB = await call(projectB, `/${boardB}`)
  check('aynı projedeki panolar ayrı çalışıyor', contentA.tasks.length === 2 && contentTeam.tasks.length === 1 && !contentA.tasks.some(task => task.id === teamTask.id))
  check('projeler birbirine karışmıyor', contentB.tasks.length === 1 && !contentB.tasks.some(task => task.id === first.id))
  check('sprint de panosuna ait', contentA.sprints.length === 1 && contentTeam.sprints.length === 0)

  let crossed = 0
  try { await call(projectA, `/${team.id}/tasks/${first.id}`, { method: 'PATCH', body: JSON.stringify({ title: 'çapraz' }) }) } catch (error) { crossed = Number(/→ (\d+)/.exec(error.message)?.[1] || 0) }
  check('başka panonun görevi düzenlenemiyor', crossed === 404, String(crossed))

  let wrongSprint = 0
  try { await call(projectA, `/${team.id}/tasks`, { method: 'POST', body: JSON.stringify({ title: 'yanlış sprint', sprintId: sprint.id }) }) } catch (error) { wrongSprint = Number(/→ (\d+)/.exec(error.message)?.[1] || 0) }
  check('görev başka panonun sprint\'ine bağlanamıyor', wrongSprint === 400, String(wrongSprint))

  // Görev altındaki konuşma
  const yorum = await onA(`/tasks/${first.id}/comments`, { method: 'POST', body: JSON.stringify({ content: 'Bu görevi bugün bitiriyorum.' }) })
  check('yorum eklendi', yorum.id && yorum.taskId === first.id && yorum.content.includes('bugün'), yorum.content)
  const yorumlar = await onA(`/tasks/${first.id}/comments`)
  check('yorumlar listeleniyor', yorumlar.length === 1 && yorumlar[0].id === yorum.id, String(yorumlar.length))
  const duzeltilmis = await onA(`/tasks/${first.id}/comments/${yorum.id}`, { method: 'PATCH', body: JSON.stringify({ content: 'Yarına kaldı.' }) })
  check('yorum düzenlendi', duzeltilmis.content === 'Yarına kaldı.')
  let yabanci = 0
  try { await call(projectA, `/${team.id}/tasks/${first.id}/comments`, { method: 'POST', body: JSON.stringify({ content: 'çapraz' }) }) } catch (error) { yabanci = Number(/→ (\d+)/.exec(error.message)?.[1] || 0) }
  check('başka panodan yorum yazılamıyor', yabanci === 404, String(yabanci))
  await onA(`/tasks/${first.id}/comments/${yorum.id}`, { method: 'DELETE' })
  check('yorum silindi', (await onA(`/tasks/${first.id}/comments`)).length === 0)

  const moved = await onA(`/tasks/${first.id}/move`, { method: 'PATCH', body: JSON.stringify({ status: 'IN_PROGRESS', order: [first.id] }) })
  check('sütun değişti', moved.tasks.find(task => task.id === first.id).status === 'IN_PROGRESS')

  const edited = await onA(`/tasks/${second.id}`, { method: 'PATCH', body: JSON.stringify({ title: 'İkinci görev (düzenli)', assigneeId: '', sprintId: '', status: 'REVIEW' }) })
  check('düzenleme ve backlog’a alma', edited.title.endsWith('(düzenli)') && edited.sprintId === null && edited.assigneeId === null && edited.status === 'REVIEW')

  await onA(`/sprints/${sprint.id}`, { method: 'DELETE' })
  const afterSprint = await onA('')
  check('sprint silinince görevler backlog’a düşüyor', !afterSprint.sprints.length && afterSprint.tasks.every(task => task.sprintId === null))

  const invalid = await fetch(`${boardsUrl(projectA)}/${boardA}/tasks/${first.id}/move`, { method: 'PATCH', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ status: 'NOPE' }) })
  check('geçersiz sütun reddediliyor', invalid.status === 400, String(invalid.status))

  const missing = await fetch(`${api}/organizations/${org}/projects/yok-boyle-proje/boards`, { headers: { Authorization: `Bearer ${token}` } })
  check('olmayan proje 404', missing.status === 404, String(missing.status))

  const strayBoard = await fetch(`${boardsUrl(projectB)}/${boardA}`, { headers: { Authorization: `Bearer ${token}` } })
  check('başka projenin panosu açılamıyor', strayBoard.status === 404, String(strayBoard.status))

  // Panolar silinebilir ama projenin sonuncusu kalır.
  await call(projectA, `/${team.id}`, { method: 'DELETE' })
  const kalan = await call(projectA, '')
  check('pano silindi, görevleri de gitti', kalan.length === 1 && kalan[0].id === boardA)
  const sonPano = await fetch(`${boardsUrl(projectA)}/${boardA}`, { method: 'DELETE', headers: { Authorization: `Bearer ${token}` } })
  check('son pano silinemiyor', sonPano.status === 400, String(sonPano.status))

  const anon = await fetch(boardsUrl(projectA))
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
