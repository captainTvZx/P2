// Kuruluştan çıkarılan kişi, kalan üyelerin listesinden anında düşüyor mu?
// (Eskiden yalnızca çıkarılan kişiye haber gidiyordu; diğerleri uygulamayı
// yeniden başlatana kadar kişiyi listede görüyordu.)
import fs from 'node:fs'
import { randomUUID } from 'node:crypto'
import jwt from 'jsonwebtoken'
import pg from 'pg'
import { io } from 'socket.io-client'

const api = process.env.P2_API || 'http://127.0.0.1:4000'
const env = fs.readFileSync('.env', 'utf8')
const pick = key => env.match(new RegExp(`${key}="?([^"\r\n]+)`))[1]
const db = new pg.Client({ connectionString: pick('DATABASE_URL') })
await db.connect()

const organizationId = (await db.query('select id from "Organization" limit 1')).rows[0].id
const owner = (await db.query('select "userId" from "OrganizationMember" where "organizationId"=$1 and role=\'OWNER\' limit 1', [organizationId])).rows[0].userId
const token = jwt.sign({ sub: owner, username: 'kick-test' }, pick('JWT_ACCESS_SECRET'), { expiresIn: '10m' })

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

// Geçici üye: testin sonunda silinir.
const userId = randomUUID()
const handle = `e2e-kick-${Date.now()}`
await db.query(
  'insert into "User" ("id","email","username","displayName","passwordHash","createdAt","updatedAt") values ($1,$2,$3,$4,$5,now(),now())',
  [userId, `${handle}@ornek.test`, handle, 'E2E Çıkarılan', 'x'],
)
// OrganizationMember'ın birincil anahtarı (organizationId,userId) çiftidir.
await db.query(
  'insert into "OrganizationMember" ("organizationId","userId","role","joinedAt") values ($1,$2,$3,now())',
  [organizationId, userId, 'MEMBER'],
)

const socket = io(`${api}/stream`, { auth: { token }, transports: ['websocket'] })
const connected = await new Promise(resolve => {
  socket.on('connect', () => resolve(true))
  socket.on('connect_error', () => resolve(false))
  setTimeout(() => resolve(false), 8000)
})
check('kalan üye bağlandı', connected)
socket.emit('subscribe', { organizationId })
await new Promise(r => setTimeout(r, 500))

const duyuru = new Promise(resolve => {
  socket.on('organization:member:removed', payload => resolve(payload))
  setTimeout(() => resolve(null), 10000)
})

const response = await fetch(`${api}/organizations/${organizationId}/admin/members/${userId}`, {
  method: 'DELETE', headers: { Authorization: `Bearer ${token}` },
})
check('üye çıkarıldı', response.ok, String(response.status))

const payload = await duyuru
check('kalan üyelere duyuru gitti', Boolean(payload), payload ? '' : 'duyuru yok (10 sn)')
check('duyuru doğru kişiyi işaret ediyor', payload?.userId === userId && payload?.organizationId === organizationId)

const kalan = await db.query('select count(*)::int as n from "OrganizationMember" where "userId"=$1', [userId])
check('üyelik kaydı silindi', kalan.rows[0].n === 0)

socket.close()
await db.query('delete from "User" where id=$1', [userId])
await db.end()
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
