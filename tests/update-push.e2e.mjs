// Yeni sürüm yayınlandığı anda, o sırada bağlı olan istemcilere duyuru gidiyor mu?
// Çalışan bir P2 sunucusuna bağlanır (P2_API, varsayılan http://127.0.0.1:4000)
// ve o sunucunun izlediği yayın klasörüne (P2_UPDATE_DIR) sahte bir sürüm koyar.
import fs from 'node:fs'
import path from 'node:path'
import jwt from 'jsonwebtoken'
import pg from 'pg'
import { io } from 'socket.io-client'

const api = process.env.P2_API || 'http://127.0.0.1:4000'
const dir = process.env.P2_UPDATE_DIR
if (!dir) { console.error('P2_UPDATE_DIR verilmeli: sunucunun izlediği yayın klasörü.'); process.exit(1) }

const env = fs.readFileSync('.env', 'utf8')
const pick = key => env.match(new RegExp(`${key}="?([^"\r\n]+)`))[1]
const db = new pg.Client({ connectionString: pick('DATABASE_URL') })
await db.connect()
const owner = (await db.query('select "userId" from "OrganizationMember" limit 1')).rows[0].userId
const organizationId = (await db.query('select id from "Organization" limit 1')).rows[0].id
await db.end()
const token = jwt.sign({ sub: owner, username: 'update-push' }, pick('JWT_ACCESS_SECRET'), { expiresIn: '10m' })

let failed = 0
const check = (label, ok, detail = '') => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}${detail ? ' · ' + detail : ''}`); if (!ok) failed++ }

const socket = io(`${api}/stream`, { auth: { token }, transports: ['websocket'] })
const connected = await new Promise(resolve => {
  socket.on('connect', () => resolve(true))
  socket.on('connect_error', () => resolve(false))
  setTimeout(() => resolve(false), 8000)
})
check('istemci bağlandı', connected)

socket.emit('subscribe', { organizationId })
const duyuru = new Promise(resolve => {
  socket.on('update:published', payload => resolve(payload))
  setTimeout(() => resolve(null), 40000)
})

// Yayın: publish-update.mjs'in yaptığı gibi latest.yml'i tazeliyoruz.
fs.mkdirSync(dir, { recursive: true })
const version = `9.9.${Math.floor(Math.random() * 900) + 10}`
fs.writeFileSync(path.join(dir, 'latest.yml'), `version: ${version}\nfiles:\n  - url: P2-Setup-${version}.exe\n    sha512: test\n    size: 1\npath: P2-Setup-${version}.exe\n`)
console.log(`yayınlandı: ${version} → ${dir}`)

const payload = await duyuru
check('yayın anında duyuru geldi', Boolean(payload), payload ? `sürüm ${payload.version}` : 'duyuru yok (40 sn)')
check('duyurudaki sürüm doğru', payload?.version === version, payload?.version === version ? String(version) : `${payload?.version} ≠ ${version}`)

socket.close()
console.log(`\n${failed ? failed + ' başarısız' : 'hepsi geçti'}`)
process.exitCode = failed ? 1 : 0
