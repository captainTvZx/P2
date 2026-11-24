// Microsoft Store ekran görüntüleri: örnek verili bir test sunucusunda (gerçek kullanıcı verisi yok)
// istemciyi ekran dışında açar (P2_TEST_WINDOW_POS) ve p2-wincap (PrintWindow) ile PNG alır.
// Masaüstüne girdi gönderilmez, ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE).
// Çıktı: C:/p2-pg/store-shots/out/{tr,en}-NN-*.png
// Önkoşul: test PostgreSQL (:55432) + cargo build --release -p p2-server -p p2-client -p p2-wincap -p p2-d3dtest
// + overlay (packaging/build-overlay.mjs → native/overlay/dist).
//   node tools/store-screenshots.mjs                         (hepsi)
//   P2_SHOT_ONLY=overlay node tools/store-screenshots.mjs    (adı eşleşenler)
import { spawn, execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { startServer, TARGET, sql, sleep } from './lib.mjs'

const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/store-shots'
rmSync(BASE, { recursive: true, force: true })
for (const d of ['out', 'server', 'uploads', 'releases', 'games', 'build', 'shots']) mkdirSync(`${BASE}/${d}`, { recursive: true })
process.env.P2_UPLOAD_DIR = BASE + '/uploads'
process.env.P2_RELEASE_DIR = BASE + '/releases'
const { base: API, stop } = await startServer(Number(process.env.P2_TEST_PORT) || 4133, { cwd: BASE + '/server' })
console.log('sunucu: ' + API)
const SIZE = process.env.P2_SHOT_SIZE || '1440,900'
const ONLY = process.env.P2_SHOT_ONLY ? new RegExp(process.env.P2_SHOT_ONLY) : null
const wanted = label => !ONLY || ONLY.test(label)

async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, displayName, extra = {}) => {
  const r = await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })
  if (!r.body?.accessToken) throw new Error(`kayıt olmadı (${name}): ${r.status} ${JSON.stringify(r.body)}`)
  return r.body
}
const grant = u => { try { sql(`insert into "ServerPermission" ("userId", "permission") values ($1, 'project.create') on conflict do nothing`, [u.user.id]) } catch {} }

// ---------------------------------------------------------------- örnek veri
const owner = await register('deniz', 'Deniz Kaya')
grant(owner)
const org = (await call('GET', '/organizations', null, owner.accessToken)).body[0]

async function project(by, body) {
  const p = (await call('POST', '/projects', { ...body, slug: `${body.slug}-${tag}` }, by.accessToken)).body
  if (!p?.id) throw new Error('proje açılamadı: ' + body.name + ' ' + JSON.stringify(p))
  return p
}
const TR = await project(owner, { name: 'Kuzey Stüdyosu', slug: 'kuzey', description: 'Bağımsız oyun stüdyosu' })
const EN = await project(owner, { name: 'North Studio', slug: 'north', description: 'Indie game studio' })
const invite = async (p, by = owner) => (await call('POST', `/projects/${p.id}/admin/invites`, { maxUses: 50 }, by.accessToken)).body.code
const [ctr, cen] = [await invite(TR), await invite(EN)]
const people = {
  ece: await register('ece', 'Ece Yılmaz', { inviteCode: ctr }),
  mert: await register('mert', 'Mert Aydın', { inviteCode: ctr }),
  selin: await register('selin', 'Selin Arslan', { inviteCode: ctr }),
  can: await register('can', 'Can Demir', { inviteCode: ctr }),
}
for (const p of Object.values(people)) await call('POST', `/invites/${cen}/join`, null, p.accessToken)
const U = who => (who === 'deniz' ? owner : people[who])
const root = p => `/organizations/${org.id}/projects/${p.id}`

async function say(p, lines) {
  const ch = (await call('GET', `${root(p)}/channels`, null, owner.accessToken)).body.flatMap(c => c.channels).find(c => c.kind !== 'VOICE' && c.type !== 'VOICE')
  for (const [who, text] of lines) { await call('POST', `${root(p)}/channels/${ch.id}/messages`, { content: text }, U(who).accessToken); await sleep(100) }
}
await say(TR, [
  ['ece', 'Günaydın ekip! Yeni bölümün ışıklandırması bitti, akşam derlemeye alıyorum 🌅'],
  ['mert', 'Harika! Ses efektlerini de bugün yüklerim, overlay\'den deneyebiliriz.'],
  ['selin', 'Sprint planlaması için panoya üç yeni görev ekledim, bakabilir misiniz?'],
  ['can', 'Envanter hatasını çözdüm, inceleme bekliyor. Deniz, bakarsan sevinirim.'],
  ['deniz', 'Bakıyorum 👍 Saat 15:00\'te sesli kanalda kısa bir toplantı yapalım mı?'],
  ['ece', 'Olur, ekranımı paylaşıp yeni haritayı gösteririm.'],
  ['mert', 'Ben de katılırım. Yeni sürüm kütüphaneye düşünce herkes güncellesin 🎮'],
])
await say(EN, [
  ['ece', 'Morning team! Lighting for the new level is done, it goes into tonight\'s build 🌅'],
  ['mert', 'Great! I\'ll upload the sound effects today so we can try them in the overlay.'],
  ['selin', 'I added three new tasks to the board for sprint planning, can you take a look?'],
  ['can', 'Fixed the inventory bug, it\'s waiting for review. Deniz, could you check it?'],
  ['deniz', 'On it 👍 Quick meeting in the voice channel at 3 PM?'],
  ['ece', 'Sure, I\'ll share my screen and walk you through the new map.'],
  ['mert', 'Count me in. Everyone please update once the new build hits the library 🎮'],
])

// Özel mesaj: Deniz ↔ Ece (her dilin projesinde ayrı sohbet).
const DM = {
  tr: [['ece', 'Selam! Yeni karakter tasarımlarını belgeler bölümüne yükledim.'], ['deniz', 'Süper, hemen bakıyorum. Renk paleti çok iyi olmuş 🎨'], ['ece', 'Teşekkürler! Akşam test derlemesinde birlikte deneyelim mi?'], ['deniz', 'Olur, overlay açıkken notları oradan paylaşırım.']],
  en: [['ece', 'Hey! I uploaded the new character designs to Documents.'], ['deniz', 'Awesome, taking a look now. The color palette looks great 🎨'], ['ece', 'Thanks! Want to try them together in tonight\'s test build?'], ['deniz', 'Sure, I\'ll share notes from the overlay while we play.']],
}
for (const [lang, p] of [['tr', TR], ['en', EN]]) {
  const dm = (await call('POST', `/organizations/${org.id}/direct`, { userId: people.ece.user.id, projectId: p.id }, owner.accessToken)).body
  if (!dm?.id) { console.log('  (özel sohbet açılamadı: ' + JSON.stringify(dm) + ')'); continue }
  for (const [who, text] of DM[lang]) { await call('POST', `/organizations/${org.id}/direct/${dm.id}/messages`, { content: text }, U(who).accessToken); await sleep(100) }
}

// Belgeler: sayfa belgesi (HTML gövde).
const DOC = {
  tr: { title: 'Oyun Tasarım Belgesi', body: '<h1>Kuzey: Oyun Tasarım Belgesi</h1><p>Kuzey, donmuş bir takımadada geçen, keşif odaklı bir macera oyunudur. Oyuncu, kaybolan deniz fenerlerini yeniden yakarak adalar arasında yol açar.</p><h2>Temel döngü</h2><p>• Keşfet: yeni adalara ulaş, kaynak topla.</p><p>• İnşa et: fenerleri onar, köprüler kur.</p><p>• Hayatta kal: fırtına ve soğukla başa çık.</p><h2>Kış Güncellemesi hedefleri</h2><p>1. Yeni bölüm: Buzul Mağaraları</p><p>2. Yeniden tasarlanan ana menü</p><p>3. Bulut kaydı desteği</p><p><b>Teslim:</b> Sprint 12 sonu. Sorular için #genel kanalına yazın.</p>' },
  en: { title: 'Game Design Document', body: '<h1>North: Game Design Document</h1><p>North is an exploration-focused adventure set on a frozen archipelago. Players relight lost lighthouses to open paths between the islands.</p><h2>Core loop</h2><p>• Explore: reach new islands and gather resources.</p><p>• Build: repair lighthouses and raise bridges.</p><p>• Survive: weather storms and the cold.</p><h2>Winter Update goals</h2><p>1. New level: Glacier Caves</p><p>2. Redesigned main menu</p><p>3. Cloud save support</p><p><b>Due:</b> end of Sprint 12. Questions go to the #general channel.</p>' },
}
for (const [lang, p] of [['tr', TR], ['en', EN]]) {
  const d = (await call('POST', `${root(p)}/documents`, { title: DOC[lang].title, kind: 'PAGE' }, owner.accessToken)).body
  if (d?.id) await call('PATCH', `${root(p)}/documents/${d.id}`, { content: { version: 1, body: DOC[lang].body } }, owner.accessToken)
  else console.log('  (belge açılamadı: ' + JSON.stringify(d) + ')')
}

// Keşfet: herkese açık örnek projeler (Ece'nin). Dil başına ayrı set; çekimde yalnız o dilinki açık.
grant(people.ece)
const PUBLIC = {
  tr: [
    { name: 'Gece Yarısı Speedrun', slug: 'speedrun', category: 'GAME', tagline: 'Haftalık speedrun yarışmaları ve rota paylaşımı', description: 'Speedrun topluluğu' },
    { name: 'Piksel Atölyesi', slug: 'piksel', category: 'DEV', tagline: 'Piksel sanatı ve bağımsız oyun geliştiricileri buluşması', description: 'Geliştirici topluluğu' },
    { name: 'Retro Oyun Kulübü', slug: 'retro', category: 'COMMUNITY', tagline: 'Eski konsollar, emülatörler ve nostalji sohbetleri', description: 'Retro oyun sevenler' },
    { name: 'Ses Tasarımı Okulu', slug: 'ses', category: 'EDUCATION', tagline: 'Oyunlar için ses efekti ve müzik dersleri', description: 'Eğitim' },
  ],
  en: [
    { name: 'Midnight Speedrun', slug: 'speedrun-en', category: 'GAME', tagline: 'Weekly speedrun races and route sharing', description: 'Speedrun community' },
    { name: 'Pixel Workshop', slug: 'pixel', category: 'DEV', tagline: 'Pixel art and indie game developers hangout', description: 'Developer community' },
    { name: 'Retro Game Club', slug: 'retro-en', category: 'COMMUNITY', tagline: 'Old consoles, emulators and nostalgia talk', description: 'Retro fans' },
    { name: 'Sound Design School', slug: 'sound', category: 'EDUCATION', tagline: 'Sound effects and music lessons for games', description: 'Education' },
  ],
}
const publicProjects = { tr: [], en: [] }
for (const lang of ['tr', 'en']) for (const p of PUBLIC[lang]) publicProjects[lang].push(await project(people.ece, { ...p, visibility: 'PUBLIC' }))
const showPublic = async lang => {
  for (const l of ['tr', 'en']) for (const p of publicProjects[l]) await call('PATCH', `/projects/${p.id}/admin`, { visibility: l === lang ? 'PUBLIC' : 'PRIVATE' }, people.ece.accessToken)
}

// Görev panosu (P2_AUTOPILOT_BOARD_DEMO: sunucuya yazılmaz, yalnız görünüm).
function board(tr) {
  const ids = { d: owner.user.id, e: people.ece.user.id, m: people.mert.user.id, s: people.selin.user.id, c: people.can.user.id }
  const t = (n, [title, labels], status, priority, kind, points, who) => ({ id: `t${n}`, number: n, title, status, priority, kind, labels, points, position: n, assigneeId: ids[who], createdById: ids.d, sprintId: 's1', boardId: '', createdAt: new Date().toISOString() })
  const L = tr
    ? [['Ana menü yeniden tasarımı', 'ui'], ['Envanter sıralama hatası', 'oynanış'], ['Yağmur ses efektleri', 'ses'], ['Kaydetme sistemi bulut desteği', 'altyapı'], ['Yeni bölüm ışıklandırması', 'grafik'], ['Ayarlar ekranı erişilebilirlik', 'ui'], ['Boss savaşı animasyonları', 'animasyon'], ['Yükleme süresini azalt', 'performans']]
    : [['Main menu redesign', 'ui'], ['Inventory sorting bug', 'gameplay'], ['Rain sound effects', 'audio'], ['Cloud save support', 'backend'], ['New level lighting', 'graphics'], ['Settings accessibility', 'ui'], ['Boss fight animations', 'animation'], ['Reduce loading time', 'performance']]
  return JSON.stringify({
    sprints: [{ id: 's1', name: tr ? 'Sprint 12 — Kış Güncellemesi' : 'Sprint 12 — Winter Update', goal: tr ? 'Yeni bölüm ve menü' : 'New level and menu', status: 'ACTIVE' }],
    tasks: [
      t(1, L[0], 'TODO', 'HIGH', 'FEATURE', 5, 's'), t(2, L[1], 'REVIEW', 'URGENT', 'BUG', 3, 'c'), t(3, L[2], 'IN_PROGRESS', 'MEDIUM', 'TASK', 2, 'm'),
      t(4, L[3], 'TODO', 'MEDIUM', 'FEATURE', 8, 'd'), t(5, L[4], 'DONE', 'HIGH', 'TASK', 5, 'e'), t(6, L[5], 'IN_PROGRESS', 'LOW', 'TASK', 3, 's'),
      t(7, L[6], 'TODO', 'HIGH', 'FEATURE', 5, 'e'), t(8, L[7], 'DONE', 'MEDIUM', 'TASK', 3, 'c'),
    ],
  })
}

// ---------------------------------------------------------------- istemci
function start(label, user, lang, theme, projectName, env = {}, profile = label) {
  const c = { lines: [], proc: null }
  const dir = `${BASE}/p-${profile}`
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: lang, theme }))
  c.proc = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user.user.username, P2_AUTOPILOT_PASS: 'Parola123',
      P2_AUTOPILOT_PROJECT: projectName, P2_TEST_WINDOW_POS: '-12000,-12000', P2_TEST_WINDOW_SIZE: SIZE, P2_D3DTEST_SIZE: SIZE.replace(',', 'x'),
      P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1',
      P2_LIBRARY_ROOT: BASE + '/games', P2_SHOT_DIR: BASE + '/shots', ...env,
    },
  })
  c.proc.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) c.lines.push(l) })
  c.find = re => c.lines.find(l => re.test(l))
  c.wait = async (re, ms = 40000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = c.find(re); if (hit) return hit; await sleep(150) } return null }
  c.finish = async () => { try { c.proc.kill() } catch {} await sleep(900) }
  return c
}
const capture = (args, name) => {
  try { execFileSync(BIN + 'p2-wincap.exe', [...args, `${BASE}/out/${name}.png`]); console.log(`  ✔ ${name}.png`); return true } catch (e) { console.log(`  ✘ ${name}: ` + String(e.stderr || e.message).trim().slice(0, 200)); return false }
}

async function shoot(label, { lang, theme = 'dark', project, env = {}, settle = 3500, user = owner, ready = /olay: gerçek zamanlı bağlandı/, before, target, profile }) {
  if (!wanted(label)) return
  console.log(`${label}…`)
  const helpers = before ? await before() : []
  const c = start(label, user, lang, theme, project, env, profile || label)
  if (!(await c.wait(ready, 90000))) console.log(`  (beklenen iz görülmedi: ${ready}; yine de çekiliyor)`)
  await sleep(settle)
  capture(target ? await target(c) : ['--client', String(c.proc.pid)], label)
  if (process.env.P2_SHOT_DEBUG) console.log(c.lines.filter(l => /overlay|kütüphane|başlatma|oyun|enjekte|hata/i.test(l)).slice(-40).join('\n'))
  await c.finish()
  for (const h of helpers) await h.finish()
}

// Kütüphane: örnek oyun (p2-d3dtest) projeye yüklenir, yayınlanır ve canlıya alınır.
async function prepareLibrary(p, lang) {
  copyFileSync(BIN + 'p2-d3dtest.exe', `${BASE}/build/Game.exe`)
  await call('PATCH', `/projects/${p.id}/admin`, { libraryEnabled: true }, owner.accessToken)
  const up = start(`upload-${lang}`, owner, lang, 'dark', p.name, { P2_AUTOPILOT_PAGE: 'library', P2_AUTOPILOT_UPLOAD: `${BASE}/build|Game.exe|live|1.4.0` })
  const done = await up.wait(/kütüphane: yükleme (tamam|ready)/, 120000)
  await up.finish()
  if (!done) { console.log('  (yükleme tamamlanmadı: ' + up.lines.filter(l => /kütüphane|hata|error/i.test(l)).slice(-3).join(' | ') + ')'); return false }
  const rels = (await call('GET', `${root(p)}/releases`, null, owner.accessToken)).body.releases || []
  const r = rels.find(x => x.version === '1.4.0')
  if (!r) return false
  await call('POST', `${root(p)}/releases/${r.id}/publish`, {}, owner.accessToken)
  await call('POST', `${root(p)}/releases/${r.id}/live`, {}, owner.accessToken)
  return true
}
// Yalnız bu betiğin başlattığı test oyunları (PID ile; aynı adlı başka bir Game.exe'ye dokunulmaz).
const gamePids = new Set()
const killGames = () => { for (const pid of gamePids) { try { execFileSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' }) } catch {} } gamePids.clear()
  // Yedek: yalnız bu betiğin oyun klasöründen çalışan süreçler.
  try { execFileSync('powershell.exe', ['-NoProfile', '-Command', String.raw`Get-Process | Where-Object { $_.Path -like 'C:\p2-pg\store-shots\games\*' } | Stop-Process -Force`], { stdio: 'ignore' }) } catch {}
}

try {
  for (const lang of ['tr', 'en']) {
    const P = lang === 'tr' ? TR : EN
    const voice = 'ekip-odası'
    await shoot(`${lang}-01-sohbet`, { lang, project: P.name })
    await shoot(`${lang}-02-gorev-panosu`, { lang, project: P.name, env: { P2_AUTOPILOT_PAGE: 'board', P2_AUTOPILOT_BOARD_DEMO: board(lang === 'tr') } })
    await shoot(`${lang}-03-belgeler`, { lang, project: P.name, env: { P2_AUTOPILOT_PAGE: 'documents' }, settle: 4500 })
    await shoot(`${lang}-04-sesli-oda`, {
      lang, project: P.name, env: { P2_AUTOPILOT_VOICE: voice }, settle: 6000,
      before: async () => {
        const others = []
        for (const who of ['ece', 'mert']) { const o = start(`${lang}-ses-${who}`, people[who], lang, 'dark', P.name, { P2_AUTOPILOT_VOICE: voice }); await o.wait(/olay: gerçek zamanlı bağlandı/, 60000); others.push(o) }
        await sleep(3000)
        return others
      },
    })
    if (wanted(`${lang}-05-kesfet`)) await showPublic(lang)
    await shoot(`${lang}-05-kesfet`, { lang, project: P.name, env: { P2_AUTOPILOT_DISCOVER: 'wait:500' }, settle: 4500 })
    await shoot(`${lang}-06-ozel-mesaj`, { lang, project: P.name, env: { P2_AUTOPILOT_HOME: `pdm:${people.ece.user.username}`, P2_AUTOPILOT_HOME_DELAY: '2500' }, settle: 5000 })
    if (wanted(`${lang}-07-kutuphane`) || wanted(`${lang}-08-oyun-ici-overlay`)) {
      if (await prepareLibrary(P, lang)) {
        await shoot(`${lang}-07-kutuphane`, { lang, project: P.name, env: { P2_AUTOPILOT_PAGE: 'library', P2_AUTOPILOT_LIBRARY: 'install' }, ready: /kütüphane: iş done/, settle: 2500, profile: `${lang}-oyuncu` })
        await shoot(`${lang}-08-oyun-ici-overlay`, {
          lang, project: P.name, settle: 4000, profile: `${lang}-oyuncu`,
          env: { P2_AUTOPILOT_PAGE: 'library', P2_AUTOPILOT_LIBRARY: 'play', P2_OVERLAY_AUTOPILOT: 'wait:2500;open;wait:900;click:1;wait:900;click:2;wait:120000' },
          ready: /overlay\[\d+\]: .*kök süreç \d+ enjekte edildi/,
          target: async c => { const gamePid = Number(/kök süreç (\d+)/.exec(c.find(/kök süreç \d+ enjekte/) || '')?.[1] || 0); if (gamePid) gamePids.add(gamePid); await c.wait(/adım: click:2/, 30000); await sleep(3000); return ['--client', String(gamePid)] },
        })
        killGames()
      }
    }
    await shoot(`${lang}-09-ayarlar-gorunum`, { lang, project: P.name, env: { P2_AUTOPILOT_SETTINGS: 'appearance' }, settle: 3000 })
    await shoot(`${lang}-10-acik-tema`, { lang, theme: 'light', project: P.name })
  }
} finally {
  killGames()
  stop()
}
console.log(`\nÇıktı: ${BASE}/out`)
