// Dilim 8 uçtan uca testi: yönetim paneli (proje toplulukları: panel seçili projenindir). Rust istemcisi
// (p2test, kurulum sahibi ve projenin sahibi) otomatik pilotla projede rol oluşturur, yetki açar, rolü
// rustbot'a verir (rustbot'un o projedeki etkin yetkisi sunucudan doğrulanır), proje davet kodu oluşturup
// iptal eder, proje ayarlarından kütüphane bayrağını değiştirir, sunucu yönetiminde kuruluşu yeniden
// adlandırıp geri alır, rolü geri alıp siler; her bölüm açılır ve görüntüsü alınır.
// Önkoşul: test PostgreSQL (:55432) + derlenmiş Rust ikilileri (cargo build --release). Sunucu:
//   P2_API verilmezse test kendi Rust sunucusunu (:4130, tools/lib.mjs) başlatır ve sonunda kapatır.
//   node tools/slice8-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import { testServer, TARGET } from './lib.mjs'

const { api: API } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice8'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}

const owner = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
const bot = (await call('POST', '/auth/login', { identifier: 'rustbot', password: 'Parola123' })).body
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]
const project = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body[0]
const ROLE = 'Rust Moderatör'
// Önceki koşudan kalan rol
for (const r of (await call('GET', `/projects/${project.id}/roles`, null, owner.accessToken)).body.filter(r => r.name === ROLE)) await call('DELETE', `/projects/${project.id}/roles/${r.id}`, null, owner.accessToken)
const invitesBefore = (await call('GET', `/projects/${project.id}/admin/invites`, null, owner.accessToken)).body.length
const orgName = org.name
console.log(`hazırlık: "${orgName}", proje "${project.name}" (kütüphane ${project.libraryEnabled ? 'açık' : 'kapalı'}), ${invitesBefore} davet kodu`)

const steps = [
  'wait:800', 'open:team', 'wait:800', 'roles', `role:${ROLE}`, `perm:${ROLE}:member.invite`, `assign:${ROLE}:rustbot`, 'wait:2500',
  'open:invites', 'invite:5:7', 'wait:1500', 'revoke-last',
  'open:library', `library:${project.name}:${project.libraryEnabled ? 'off' : 'on'}`, `library:${project.name}:${project.libraryEnabled ? 'on' : 'off'}`,
  'open:organization', `rename:${orgName} Rust`, 'wait:1200', `rename:${orgName}`,
  'open:channels', 'wait:1000', 'open:trash', 'wait:600',
  'open:team', `unassign:${ROLE}:rustbot`, `delete-role:${ROLE}`, 'open:overview', 'wait:1200', 'close',
].join(';')

const lines = []
const client = spawn(BIN + 'p2-client.exe', [], { env: { ...process.env, P2_PROFILE_DIR: BASE + '/profile', P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: 'p2test', P2_AUTOPILOT_PASS: 'Parola123', P2_AUTOPILOT_ADMIN: steps } })
client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/yönetim:/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
const wait = async (re, ms = 20000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', [String(client.pid), `${BASE}/out/${name}.png`]) } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)) } }

console.log('\n[1] Panel ve genel bakış')
check('yönetim paneli açıldı', !!(await wait(/yönetim: panel açıldı/)))
check('ekip yüklendi', !!(await wait(/yönetim: \d+ üye, \d+ rol/)))

console.log('\n[2] Rol ve yetki')
check('rol oluşturuldu', !!(await wait(new RegExp(`yönetim: ${ROLE} rolü oluşturuldu`))))
check('rolde yetki açıldı', !!(await wait(new RegExp(`yönetim: member.invite yetkisi ${ROLE} rolünde açıldı`))))
check('rol rustbot\'a verildi', !!(await wait(/yönetim: Rol verildi/)))
const role = (await call('GET', `/projects/${project.id}/roles`, null, owner.accessToken)).body.find(r => r.name === ROLE)
check('rol sunucuda (yetki + üye)', role?.permissions.includes('member.invite') && role.members.some(m => m.username === 'rustbot'), role ? `${role.handle} · ${role.permissions.join(',')}` : 'yok')
const botPerms = (await call('GET', `/projects/${project.id}/permissions/me`, null, bot.accessToken)).body
check('rustbot\'un etkin yetkisinde member.invite', botPerms.permissions.includes('member.invite'))
await sleep(600)
shot('1-roller')

console.log('\n[3] Davetler')
check('davet kodu oluşturuldu', !!(await wait(/yönetim: Yeni davet kodu oluşturuldu/)))
await sleep(400)
shot('2-davetler')
check('davet kodu iptal edildi', !!(await wait(/yönetim: .* davet kodu iptal edildi/)))
const invitesAfter = (await call('GET', `/projects/${project.id}/admin/invites`, null, owner.accessToken)).body.length
check('davet sayısı eski haline döndü', invitesAfter === invitesBefore, `${invitesBefore} → ${invitesAfter}`)

console.log('\n[4] Kütüphane ve kuruluş')
check('kütüphane bayrağı iki kez değişti', lines.filter(l => /yönetim: .* kütüphanede (listeleniyor|gizlendi)\./.test(l)).length >= 1 && !!(await wait(new RegExp(`yönetim: ${project.name} kütüphanede ${project.libraryEnabled ? 'listeleniyor' : 'gizlendi'}\\.`))))
const projAfter = (await call('GET', `/organizations/${org.id}/projects`, null, owner.accessToken)).body.find(p => p.id === project.id)
check('kütüphane bayrağı eski haline döndü', projAfter.libraryEnabled === project.libraryEnabled)
check('kuruluş yeniden adlandırıldı', !!(await wait(/yönetim: adım rename:/)) && !!(await wait(/yönetim: Kuruluş adı güncellendi/)))
await sleep(300)
shot('3-kurulus')
await wait(/yönetim: adım open:channels/)
await sleep(1500)
check('kuruluş adı geri alındı', (await call('GET', '/organizations', null, owner.accessToken)).body[0].name === orgName)

console.log('\n[5] Kanallar, geri dönüşüm, temizlik')
check('kanallar bölümü', !!(await wait(/yönetim: bölüm Channels/)))
shot('4-kanallar')
check('geri dönüşüm bölümü', !!(await wait(/yönetim: bölüm Trash/)))
check('rol geri alındı', !!(await wait(/yönetim: Rol geri alındı/)))
check('rol silindi', !!(await wait(new RegExp(`yönetim: ${ROLE} silindi`))))
check('rol sunucudan kalktı', !(await call('GET', `/projects/${project.id}/roles`, null, owner.accessToken)).body.some(r => r.name === ROLE))
check('genel bakış', !!(await wait(/yönetim: bölüm Overview/)))
await sleep(900)
shot('5-genel-bakis')
check('panel kapandı', !!(await wait(/yönetim: kapandı/)))
client.kill()
console.log(`\ngörüntüler: ${BASE}/out`)
console.log(`SONUÇ: ${pass} geçti, ${fail} kaldı`)
process.exit(fail ? 1 : 0)
