// Dilim 17 uçtan uca testi: Keşfet — herkese açık projeler ve haberleri.
//   [S] Sunucu: liste (yalnız açık projeler; üye/çevrimiçi sayısı), arama, kategori, sıralama, önizleme
//       (kanal önizlemesi, üyeler), kodsuz katılma, özel proje listelenmez/katılınmaz, yasaklı katılamaz,
//       görünürlük aç/kapat (yalnız "Projeyi düzenle" yetkisiyle), haberler (yalnız açık projelerden).
//   [1] İstemci: projesi olmayan kişi başlangıç ekranından Keşfet'e girer → arar → ayrıntı → katıl → çalışma alanı.
//   [2] İstemci: yasaklı kişi katılamaz (PROJECT_BANNED); özel proje istemci listesinde yok.
//   [3] İstemci: proje yönetim paneli › Ayarlar'dan görünürlük kapatılır/açılır (sunucudan doğrulanır).
//   [4] İstemci: başlangıç ekranında "herkese açık" proje oluşturma (yetkili kişi) ve yetkisiz kişide soluk kart.
//   [4b] Projesi kalmayan kişi başlangıç ekranındaki "Arkadaşlar ve mesajlar" ile Ana sayfaya geçer (ray görünür).
//   [5] Görünümler (koyu + açık): başlangıç, Keşfet (tanıtım alanı + ızgara), ayrıntı çekmecesi, haber okuma,
//       proje oluşturma penceresi (görünürlük), proje ayarları (görünürlük), dar/geniş pencere.
// İstemci otomatik pilot kancalarıyla sürülür (masaüstüne girdi gönderilmez); pencereler ekran dışında
// (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE). Görüntüler: C:/p2-pg/slice17/out.
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış) + cargo build --release. Sunucu: P2_API verilmezse
// test kendi Rust sunucusunu (:4130) başlatır.   node tools/slice17-test.mjs [--server-only]
import { spawn, execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET, io } from './lib.mjs'
import { seedDiscover } from './discover-seed.mjs'

const SERVER_ONLY = process.argv.includes('--server-only')
const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice17'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
// İstemci kendi kopyasından çalışır: test sürerken `cargo build` asıl exe'nin üzerine yazabilsin.
const CLIENT = BASE + '/p2-client.exe'
if (!SERVER_ONLY) copyFileSync(BIN + 'p2-client.exe', CLIENT)
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName: `${name.toUpperCase()} ${tag}`, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body
const q = s => encodeURIComponent(s)

console.log(`hazırlık: etiket ${tag}`)
// Proje açmak sunucu yetkisidir (`project.create`): kurulum sahibi p2test, testin proje sahibi O'ya bu yetkiyi
// verir (p2test'in kendi proje rayı değişmesin diye projeleri p2test açmaz).
const ADMIN = (await call('POST', '/auth/login', { identifier: 'p2test', password: 'Parola123' })).body
check('kurulum sahibi girişi', !!ADMIN?.accessToken)
const O = await register('ko')
check('yetkisiz kişi proje açamaz (403)', (await call('POST', '/projects', { name: `Yetkisiz ${tag}`, slug: `yetkisiz-${tag}` }, O.accessToken)).status === 403)
check('O\'ya project.create verildi', (await call('PUT', `/server/users/${O.user.id}/permissions`, { permissions: ['project.create'] }, ADMIN.accessToken)).status === 200)
const U = await register('ku'), V = await register('kv'), W = await register('kw'), X = await register('kx'), Y = await register('ky')
const PUB = `Açık Liman ${tag}`, PRIV = `Gizli Koy ${tag}`
const pub = (await call('POST', '/projects', { name: PUB, slug: `acik-liman-${tag}`, description: 'Keşfet testi: herkese açık topluluk', visibility: 'PUBLIC', category: 'GAME', tagline: `Dalga ${tag} sürüyor` }, O.accessToken)).body
const priv = (await call('POST', '/projects', { name: PRIV, slug: `gizli-koy-${tag}`, description: 'Keşfet testi: özel topluluk' }, O.accessToken)).body
const cleanup = async () => { for (const p of [pub, priv]) if (p?.id) await call('PATCH', `/projects/${p.id}/admin`, { visibility: 'PRIVATE' }, O.accessToken) }
const org = (await call('GET', '/organizations', null, O.accessToken)).body[0]

// ============================================================ S) Sunucu
console.log('\n[S] Sunucu uçları')
check('açık proje oluşturuldu (visibility/category/tagline)', pub?.visibility === 'PUBLIC' && pub.category === 'GAME' && pub.tagline === `Dalga ${tag} sürüyor`, JSON.stringify({ v: pub?.visibility, c: pub?.category }))
check('görünürlük verilmezse proje özel', priv?.visibility === 'PRIVATE' && priv.category === null)
check('geçersiz görünürlük 400', (await call('POST', '/projects', { name: `Hata ${tag}`, slug: `hata-${tag}`, visibility: 'HERKES' }, O.accessToken)).status === 400)
check('geçersiz kategori 400', (await call('POST', '/projects', { name: `Hata ${tag}`, slug: `hata-${tag}`, category: 'OYUN' }, O.accessToken)).status === 400)
check('oturumsuz liste 401', (await call('GET', '/discover/projects')).status === 401)

const list = (await call('GET', '/discover/projects', null, U.accessToken)).body
const mine = list.projects.find(p => p.id === pub.id)
check('liste: açık proje var', !!mine, `${list.projects.length} proje`)
check('liste: özel proje yok', !list.projects.some(p => p.id === priv.id))
check('liste: yalnız PUBLIC', list.projects.every(p => p.visibility === 'PUBLIC'))
check('kart alanları (ad, açıklama, tanıtım, kategori, sayılar, joined=false)', mine?.name === PUB && mine.description && mine.tagline && mine.category === 'GAME' && mine.memberCount === 1 && mine.onlineCount === 0 && mine.joined === false, JSON.stringify(mine))
check('kategori sayıları ve toplam', list.categories.GAME >= 1 && list.total === Object.values(list.categories).reduce((a, b) => a + b, 0), JSON.stringify(list.categories))
const found = (await call('GET', `/discover/projects?q=${q('liman ' + tag)}`, null, U.accessToken)).body
check('arama: ada göre (büyük/küçük harf duyarsız)', found.projects.length === 1 && found.projects[0].id === pub.id)
check('arama: tanıtım satırına göre', (await call('GET', `/discover/projects?q=${q('dalga ' + tag)}`, null, U.accessToken)).body.projects.some(p => p.id === pub.id))
check('arama: özel projenin adı sonuç vermez', (await call('GET', `/discover/projects?q=${q('Gizli Koy ' + tag)}`, null, U.accessToken)).body.projects.length === 0)
check('arama: joker karakter düz metin (%)', (await call('GET', `/discover/projects?q=${q('%' + tag)}`, null, U.accessToken)).body.projects.length === 0)
check('kategori: GAME içerir', (await call('GET', '/discover/projects?category=GAME', null, U.accessToken)).body.projects.some(p => p.id === pub.id))
check('kategori: MUSIC içermez', !(await call('GET', '/discover/projects?category=MUSIC', null, U.accessToken)).body.projects.some(p => p.id === pub.id))
check('sıralama: new → en yeni başta', (await call('GET', '/discover/projects?sort=new', null, U.accessToken)).body.projects[0]?.id === pub.id)

const preview = await call('GET', `/discover/projects/${pub.id}`, null, U.accessToken)
check('önizleme: kanallar, üyeler, sahip', preview.status === 200 && preview.body.channels.length === 5 && preview.body.channels.some(c => c.name === 'genel' && c.type === 'TEXT') && preview.body.members.length === 1 && preview.body.owner?.id === O.user.id && preview.body.banned === false, `${preview.body?.channels?.length} kanal`)
check('önizleme: e-posta sızmaz', !JSON.stringify(preview.body).includes('@p2.test'))
check('önizleme: özel proje 404', (await call('GET', `/discover/projects/${priv.id}`, null, U.accessToken)).status === 404)

const joined = await call('POST', `/discover/projects/${pub.id}/join`, null, U.accessToken)
check('kodsuz katılma (açık proje)', joined.status === 201 && joined.body.joined === true && joined.body.projectId === pub.id && joined.body.organization?.id === org.id, JSON.stringify(joined.body).slice(0, 120))
check('ikinci katılma no-op', (await call('POST', `/discover/projects/${pub.id}/join`, null, U.accessToken)).body.joined === false)
check('katılan kişi projeyi görür (/projects)', (await call('GET', '/projects', null, U.accessToken)).body.some(p => p.id === pub.id && p.role === 'MEMBER'))
const after = (await call('GET', `/discover/projects?q=${q(tag)}`, null, U.accessToken)).body.projects.find(p => p.id === pub.id)
check('liste: üye sayısı 2, joined=true', after?.memberCount === 2 && after.joined === true)
check('özel projeye kodsuz katılma 404', (await call('POST', `/discover/projects/${priv.id}/join`, null, U.accessToken)).status === 404)
check('özel projeye hâlâ davet koduyla girilir', await (async () => {
  const inv = (await call('POST', `/projects/${priv.id}/admin/invites`, {}, O.accessToken)).body
  return (await call('POST', '/projects/join', { code: inv.code }, W.accessToken)).body?.joined === true
})())

// Çevrimiçi sayısı: bağlı üye sayılır.
const socket = io(API + '/stream', { auth: { token: U.accessToken }, transports: ['websocket'] })
await new Promise(done => { socket.on('connect', done); setTimeout(done, 3000) })
socket.emit('presence:join', { organizationId: org.id })
await sleep(700)
const live = (await call('GET', `/discover/projects?q=${q(tag)}`, null, V.accessToken)).body.projects.find(p => p.id === pub.id)
check('çevrimiçi sayısı bağlı üyeyi sayar', live?.onlineCount === 1, `onlineCount=${live?.onlineCount}`)
check('sıralama: active kabul edilir', (await call('GET', '/discover/projects?sort=active', null, V.accessToken)).status === 200)
socket.close()

// Yasak: yasaklı kişi açık projeye de katılamaz (V önce katılır, sonra yasaklanır: üyeliği de düşer).
check('V katıldı', (await call('POST', `/discover/projects/${pub.id}/join`, null, V.accessToken)).body?.joined === true)
const banV = await call('PUT', `/projects/${pub.id}/admin/bans/${V.user.id}`, { reason: 'keşfet testi' }, O.accessToken)
check('V yasaklandı', banV.status === 200, `${banV.status} ${JSON.stringify(banV.body)?.slice(0, 80)}`)
check('yasaklanan kişinin üyeliği düştü', !(await call('GET', '/projects', null, V.accessToken)).body.some(p => p.id === pub.id))
const bannedJoin = await call('POST', `/discover/projects/${pub.id}/join`, null, V.accessToken)
check('yasaklı kişi katılamaz (403 PROJECT_BANNED)', bannedJoin.status === 403 && bannedJoin.body?.code === 'PROJECT_BANNED', `${bannedJoin.status} ${bannedJoin.body?.code}`)
check('önizleme yasaklıya banned=true', (await call('GET', `/discover/projects/${pub.id}`, null, V.accessToken)).body?.banned === true)

// Haberler: yalnız açık projelerden.
const cover = (await call('GET', `/discover/projects/${pub.id}`, null, U.accessToken)).body.coverUrl
const post1 = await call('POST', `/organizations/${org.id}/projects/${pub.id}/hub/posts`, { title: `Liman haberi ${tag}`, content: 'Yeni iskele açıldı.\n\nAyrıntılar #genel kanalında.' }, O.accessToken)
const post2 = await call('POST', `/organizations/${org.id}/projects/${pub.id}/hub/posts`, { content: `# Başlıksız gönderi ${tag}\nİlk satır başlık olur.` }, O.accessToken)
await call('POST', `/organizations/${org.id}/projects/${priv.id}/hub/posts`, { title: `Gizli haber ${tag}`, content: 'Bunu yalnız üyeler görür.' }, O.accessToken)
check('gönderi başlıkla yazıldı', post1.status === 201 && post1.body.title === `Liman haberi ${tag}` && post2.status === 201)
const news = (await call('GET', '/discover/news?limit=60', null, W.accessToken)).body
const n1 = news.find(n => n.id === post1.body.id), n2 = news.find(n => n.id === post2.body.id)
check('haberler: açık projenin gönderisi (başlık, özet, proje)', n1?.title === `Liman haberi ${tag}` && n1.excerpt.startsWith('Yeni iskele açıldı.') && n1.project.name === PUB && n1.coverUrl === (cover ?? null), JSON.stringify(n1)?.slice(0, 160))
check('haberler: başlıksız gönderide ilk satır başlık', n2?.title === `Başlıksız gönderi ${tag}` && n2.excerpt === 'İlk satır başlık olur.', `${n2?.title} / ${n2?.excerpt}`)
check('haberler: özel projenin gönderisi yok', !news.some(n => n.projectId === priv.id))
check('haberler: en yeni başta', news[0]?.id === post2.body.id)
check('önizleme: projenin son haberleri', (await call('GET', `/discover/projects/${pub.id}`, null, W.accessToken)).body.news.length === 2)

// Görünürlük: yalnız "Projeyi düzenle" yetkisiyle; özel yapılınca listeden düşer.
check('üye görünürlüğü değiştiremez (403)', (await call('PATCH', `/projects/${pub.id}/admin`, { visibility: 'PRIVATE' }, U.accessToken)).status === 403)
check('geçersiz görünürlük 400 (ayarlar)', (await call('PATCH', `/projects/${pub.id}/admin`, { visibility: 'ACIK' }, O.accessToken)).status === 400)
const closed = await call('PATCH', `/projects/${pub.id}/admin`, { visibility: 'PRIVATE' }, O.accessToken)
check('sahip projeyi özel yaptı', closed.status === 200 && closed.body.visibility === 'PRIVATE')
check('özel yapılan proje listeden düştü', !(await call('GET', `/discover/projects?q=${q(tag)}`, null, W.accessToken)).body.projects.length)
check('özel yapılan projeye kodsuz katılma 404', (await call('POST', `/discover/projects/${pub.id}/join`, null, W.accessToken)).status === 404)
check('özel yapılan projenin haberleri gizlendi', !(await call('GET', '/discover/news?limit=60', null, W.accessToken)).body.some(n => n.projectId === pub.id))
check('var olan üye üyeliğini korur', (await call('GET', '/projects', null, U.accessToken)).body.some(p => p.id === pub.id))
const reopened = await call('PATCH', `/projects/${pub.id}/admin`, { visibility: 'PUBLIC', category: 'COMMUNITY', tagline: '' }, O.accessToken)
check('yeniden açıldı; kategori değişti, tanıtım kaldırıldı', reopened.body.visibility === 'PUBLIC' && reopened.body.category === 'COMMUNITY' && reopened.body.tagline === null)
check('kategori kaldırılabilir (boş dizgi)', (await call('PATCH', `/projects/${pub.id}/admin`, { category: '' }, O.accessToken)).body.category === null)
await call('PATCH', `/projects/${pub.id}/admin`, { category: 'GAME', tagline: `Dalga ${tag} sürüyor` }, O.accessToken)
check('yeniden listelendi', (await call('GET', `/discover/projects?q=${q(tag)}`, null, W.accessToken)).body.projects.some(p => p.id === pub.id))

if (SERVER_ONLY) {
  await cleanup()
  console.log(`\nSonuç: ${pass} geçti, ${fail} kaldı (yalnız sunucu)`)
  stop()
  process.exit(fail ? 1 : 0)
}

// ============================================================ İstemci
let lines = [], client, cursor = 0
function start(user, env, theme = 'dark', profile = 'p', lang = 'tr') {
  lines = []
  cursor = 0
  const dir = `${BASE}/${profile}-${user}-${theme}`
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: lang, theme }))
  client = spawn(CLIENT, [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123',
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  client.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { lines.push(l); if (/keşfet:|başlangıç:|ana sayfa:|katıl:|yönetim: (Proje ayar|.*herkese|.*özel)|ekran:|ray:/.test(l)) console.log('    [istemci] ' + l.slice(0, 200)) } })
}
const next = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = cursor; i < lines.length; i++) if (re.test(lines[i])) { cursor = i + 1; return lines[i] } await sleep(100) } return null }
const wait = async (re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
const shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(client.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
const finish = async () => { try { client.kill() } catch {} await sleep(700) }
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

// Görüntüler gerçek görünsün: afişli örnek projeler + birkaç çevrimiçi üye.
console.log('\nörnek veri (tools/discover-seed.mjs)')
const seeded = await seedDiscover(API, { log: () => {} })
// Örnek projelerin sahibi (kesif00): proje açma yetkisi var, bütün örnek projelerin üyesi.
const DEMO_OWNER = seeded.owner.user.username
const sockets = seeded.users.slice(0, 9).map(u => { const s = io(API + '/stream', { auth: { token: u.token }, transports: ['websocket'] }); s.on('connect', () => s.emit('presence:join', { organizationId: seeded.org.id })); return s })
await sleep(1500)

// ---------------------------------------------------------------- 1) Başlangıç → Keşfet → ara → ayrıntı → katıl (X)
console.log('\n[1] Başlangıç ekranından Keşfet: ara → ayrıntı → katıl (X)')
start(X.user.username, { P2_AUTOPILOT_DISCOVER: `search:${tag};open:${PUB};join`, P2_AUTOPILOT_DISCOVER_DELAY: '1800' })
check('başlangıç ekranı (projesi yok)', !!(await wait(/başlangıç: projeye katıl ya da oluştur/)))
check('keşfet açıldı', !!(await wait(/başlangıç: keşfet açıldı/)))
const first = await wait(/keşfet: \d+ proje \(toplam \d+; q="" /)
check('liste yüklendi (özel proje yok)', !!first && first.includes(PUB) && !first.includes(PRIV), first?.slice(0, 90))
check('haberler yüklendi', !!(await wait(/keşfet: \d+ haber/)))
const searched = await wait(new RegExp(`keşfet: 1 proje \\(toplam \\d+; q="${tag}"`))
check('arama tek sonuca indi', !!searched && searched.includes(PUB))
check('ayrıntı çekmecesi', !!(await wait(new RegExp(`keşfet: ayrıntı ${esc(PUB)} \\(2 üye, 5 kanal\\)`))))
await sleep(900); shot('dark-1-ayrinti-test-projesi')
check('katıldı', !!(await wait(new RegExp(`keşfet: katıldın ${esc(PUB)}`))))
check('çalışma alanı açıldı', !!(await wait(/ekran: çalışma alanı/)))
check('rayda 1 proje', !!(await wait(/ray: 1 proje/)))
check('X sunucuda üye', (await call('GET', '/projects', null, X.accessToken)).body.some(p => p.id === pub.id))
await finish()

// ---------------------------------------------------------------- 2) Yasaklı kişi (V)
console.log('\n[2] Yasaklı kişi katılamaz (V)')
start(V.user.username, { P2_AUTOPILOT_DISCOVER: `search:${tag};open:${PUB};join`, P2_AUTOPILOT_DISCOVER_DELAY: '1800' })
check('ayrıntı: yasaklı', !!(await wait(new RegExp(`keşfet: ayrıntı ${esc(PUB)} \\(\\d+ üye, 5 kanal, yasaklı\\)`))))
check('katılma reddedildi (PROJECT_BANNED)', !!(await wait(/keşfet: hata PROJECT_BANNED/)))
await sleep(900); shot('dark-2-yasakli')
check('V sunucuda üye değil', !(await call('GET', '/projects', null, V.accessToken)).body.some(p => p.id === pub.id))
await finish()

// ---------------------------------------------------------------- 3) Yönetim paneli › Ayarlar: görünürlük (U üye; O sahip)
console.log('\n[3] Proje ayarlarından görünürlük (proje sahibi O)')
// O iki projenin sahibi: panel bu projede açılsın diye proje adıyla seçilir.
start(O.user.username, { P2_AUTOPILOT_PROJECT: PUB, P2_AUTOPILOT_ADMIN: `wait:2500;open:settings;wait:2500;visibility:private;wait:2500;visibility:public;wait:2000;close` })
check('panel açıldı', !!(await wait(new RegExp(`yönetim: panel açıldı \\(${esc(PUB)}\\)`), 40000)))
await next(/yönetim: bölüm Settings/); await sleep(1800); shot('dark-3-ayarlar-gorunurluk')
check('özel yapıldı (arayüz)', !!(await wait(/yönetim: .* artık özel\./)))
check('sunucuda PRIVATE', await (async () => { for (let i = 0; i < 20; i++) { if (!(await call('GET', `/discover/projects?q=${q(tag)}`, null, W.accessToken)).body.projects.length) return true; await sleep(150) } return false })())
check('yeniden açıldı (arayüz)', !!(await wait(/yönetim: .* artık herkese açık\./)))
check('sunucuda PUBLIC', await (async () => { for (let i = 0; i < 20; i++) { if ((await call('GET', `/discover/projects?q=${q(tag)}`, null, W.accessToken)).body.projects.length) return true; await sleep(150) } return false })())
await finish()

// ---------------------------------------------------------------- 4) Başlangıç: yetkisiz kişide soluk kart; yetkili kişi açık proje açar
console.log('\n[4] Proje oluşturma yetkisi ve görünürlük seçimi')
start(Y.user.username, {})
check('yetkisiz kişi: proje açma yetkisi yok', !!(await wait(/başlangıç: proje açma yetkisi yok/)))
await sleep(1500); shot('dark-4-baslangic-yetkisiz')
await finish()
// Y'ye sunucu yetkisi verilir → açık proje oluşturur.
const grant = await call('PUT', `/server/users/${Y.user.id}/permissions`, { permissions: ['project.create'] }, ADMIN.accessToken)
const NEWP = `Yeni Ada ${tag}`
if (grant.status === 200) {
  start(Y.user.username, { P2_AUTOPILOT_ONBOARD: `create:${NEWP}:public:MUSIC` })
  check('yetkili kişi: proje açma yetkisi var', !!(await wait(/başlangıç: proje açma yetkisi var/)))
  check('açık proje oluşturuldu (arayüz)', !!(await wait(new RegExp(`başlangıç: proje açıldı ${esc(NEWP)} \\(herkese açık\\)`))))
  const made = (await call('GET', `/discover/projects?q=${q(NEWP)}`, null, W.accessToken)).body.projects[0]
  check('sunucuda PUBLIC + kategori', made?.name === NEWP && made.category === 'MUSIC', JSON.stringify(made)?.slice(0, 100))
  await finish()
  if (made) await call('PATCH', `/projects/${made.id}/admin`, { visibility: 'PRIVATE' }, Y.accessToken)
} else {
  console.log(`    (sunucu yetkisi verilemedi: ${grant.status} ${JSON.stringify(grant.body)} — adım atlandı)`)
}

// ---------------------------------------------------------------- 4b) Projesi kalmayan kişi Ana sayfaya (arkadaşlar, DM) geçebilir
console.log('\n[4b] Başlangıç ekranından Ana sayfa (projesi kalmayan kişi)')
const H = await register('kh')
await call('POST', `/discover/projects/${pub.id}/join`, null, H.accessToken)
check('H katıldı ve ayrıldı (projesi yok, kurulum kabı var)', (await call('POST', `/projects/${pub.id}/leave`, {}, H.accessToken)).body?.left === true && (await call('GET', '/projects', null, H.accessToken)).body.length === 0)
start(H.user.username, { P2_AUTOPILOT_ONBOARD: 'home' })
check('başlangıç ekranında Ana sayfa girişi', !!(await wait(/başlangıç: ana sayfa girişi \(0 okunmamış\)/)))
check('Ana sayfa projesiz açıldı', !!(await wait(/ana sayfa: açıldı/)) && !!(await wait(/ray: 0 proje/)))
await sleep(2500)
check('başlangıç ekranına geri atılmadı', !lines.some(l => /başlangıç ekranına dönülüyor/.test(l)))
shot('dark-5-ana-sayfa-projesiz')
await finish()

// Test projeleri örnek görüntülere karışmasın.
await cleanup()

// ---------------------------------------------------------------- 5) Görünümler (koyu + açık)
console.log('\n[5] Görünümler (koyu + açık)')
const Z = await register('kz')
for (const theme of ['dark', 'light']) {
  const t = theme
  // Başlangıç ekranı (projesi olmayan, yetkisiz Z: "oluştur" soluk).
  start(Z.user.username, {}, t, 'onb')
  check(`${t}: başlangıç ekranı`, !!(await wait(/başlangıç: \d+ açık proje/)))
  await sleep(1800); shot(`${t}-10-baslangic`)
  await finish()
  // Başlangıç ekranı (yetkili kişi: "oluştur" açık) ve yeni proje formu (görünürlük).
  start(DEMO_OWNER, { P2_AUTOPILOT_SCREEN: 'onboarding' }, t, 'onb-owner')
  check(`${t}: başlangıç ekranı (yetkili)`, !!(await wait(/başlangıç: proje açma yetkisi var/)))
  await sleep(1800); shot(`${t}-11-baslangic-yetkili`)
  await finish()
  start(DEMO_OWNER, { P2_AUTOPILOT_SCREEN: 'onboarding', P2_AUTOPILOT_ONBOARD: 'project-public' }, t, 'onb-form')
  await wait(/başlangıç: proje açma yetkisi var/); await sleep(1500); shot(`${t}-12-baslangic-yeni-proje-gorunurluk`)
  await finish()
  // Keşfet: tanıtım alanı + öne çıkanlar + haber sütunu; kaydırılmış ızgara; ayrıntı; haber okuma.
  start(Z.user.username, { P2_AUTOPILOT_DISCOVER: 'scroll:520;scroll:0;open:Northwind;back;news:0;back;category:DEV;category:all;search:atölye', P2_AUTOPILOT_DISCOVER_DELAY: '3200' }, t, 'disc')
  check(`${t}: keşfet listesi`, !!(await wait(/keşfet: \d+ proje \(toplam \d+; q="" kategori=hepsi/)))
  await sleep(2400); shot(`${t}-20-kesfet`)
  await next(/keşfet: adım scroll:520/); await sleep(1200); shot(`${t}-21-kesfet-izgara`)
  check(`${t}: ayrıntı çekmecesi`, !!(await next(/keşfet: ayrıntı Northwind/)))
  await sleep(1600); shot(`${t}-22-kesfet-ayrinti`)
  check(`${t}: haber okuma`, !!(await next(/keşfet: haber /)))
  await sleep(1300); shot(`${t}-23-kesfet-haber`)
  check(`${t}: kategori süzgeci`, !!(await next(/keşfet: \d+ proje \(toplam \d+; q="" kategori=DEV/)))
  await sleep(1200); shot(`${t}-24-kesfet-kategori`)
  check(`${t}: arama`, !!(await next(/keşfet: 1 proje \(toplam \d+; q="atölye"/)))
  await sleep(1200); shot(`${t}-25-kesfet-arama`)
  await finish()
  // Çalışma alanı: raydaki pusula + Keşfet; "＋" menüsü; proje oluşturma penceresi (görünürlük); ayarlar.
  start(DEMO_OWNER, { P2_AUTOPILOT_DISCOVER: 'scroll:0', P2_AUTOPILOT_DISCOVER_DELAY: '2500' }, t, 'ws')
  check(`${t}: çalışma alanında keşfet`, !!(await wait(/keşfet: çalışma alanında açıldı/, 40000)))
  await wait(/keşfet: \d+ haber/); await sleep(2600); shot(`${t}-30-calisma-alani-kesfet`)
  await finish()
  start(DEMO_OWNER, { P2_AUTOPILOT_RAIL_ADD: '1' }, t, 'rail')
  await wait(/ray: \d+ proje/, 40000); await sleep(2400); shot(`${t}-31-ray-arti-menusu`)
  await finish()
  start(DEMO_OWNER, { P2_AUTOPILOT_MANAGE: 'view:new-project-public' }, t, 'new')
  check(`${t}: proje oluşturma penceresi`, !!(await wait(/yönet: adım view:new-project-public/, 40000)))
  await sleep(1800); shot(`${t}-32-yeni-proje-gorunurluk`)
  await finish()
  start(DEMO_OWNER, { P2_AUTOPILOT_PROJECT: 'Northwind', P2_AUTOPILOT_ADMIN: 'wait:2500;open:settings;wait:6000;close' }, t, 'set')
  check(`${t}: proje ayarları`, !!(await wait(/yönetim: bölüm Settings/, 40000)))
  await sleep(2200); shot(`${t}-33-ayarlar-gorunurluk`)
  await finish()
}
// Pencere boyutları: 900 ve 1920 genişlik (koyu).
for (const [w, h] of [[900, 620], [1920, 1040]]) {
  start(Z.user.username, { P2_AUTOPILOT_DISCOVER: 'scroll:0', P2_TEST_WINDOW_SIZE: `${w},${h}` }, 'dark', `size${w}`)
  await wait(/keşfet: \d+ haber/); await sleep(2800); shot(`dark-40-kesfet-${w}`)
  await finish()
  start(Z.user.username, { P2_TEST_WINDOW_SIZE: `${w},${h}` }, 'dark', `onb${w}`)
  await wait(/başlangıç: \d+ açık proje/); await sleep(1800); shot(`dark-41-baslangic-${w}`)
  await finish()
}
// İngilizce (varsayılan dil).
start(Z.user.username, { P2_AUTOPILOT_DISCOVER: 'scroll:0' }, 'dark', 'en', 'en')
await wait(/keşfet: \d+ haber/); await sleep(2800); shot('dark-50-kesfet-en')
await finish()
start(Z.user.username, {}, 'dark', 'onb-en', 'en')
await wait(/başlangıç: \d+ açık proje/); await sleep(1800); shot('dark-51-baslangic-en')
await finish()

for (const s of sockets) s.close()
console.log(`\nSonuç: ${pass} geçti, ${fail} kaldı — görüntüler ${BASE}/out`)
stop()
process.exit(fail ? 1 : 0)
