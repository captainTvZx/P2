// Dilim 18b uçtan uca testi: arkadaşlar, Ana sayfa ve sohbet kapsamları — İSTEMCİ arayüzü (docs/arkadaslar.md).
// Rust istemcisi `P2_AUTOPILOT_HOME` kancasıyla (masaüstüne girdi göndermeden) sürülür:
//   proje kenar çubuğu (SOHBETLER: o projenin birebir sohbetleri, okunmamış rozetleri) → proje sohbeti
//   (Ana sayfaya geçmeden) → proje sohbetinden arama (karşı istemcide gelen arama kartı) → sohbet satırı
//   menüsü ("Listeden kaldır") → Ana sayfa: Arkadaşlar sekmeleri, arkadaş ekle → Ana sayfa sohbeti
//   (Yıldızlı bölümü + Ana sayfa grubu listede) → listeden kaldır / yeni mesajla geri gelme → kapsamlı
//   yıldız → "Yeni mesaj" penceresi (tek kişi: sohbet; birkaç kişi: Ana sayfa grubu) → grup adı →
//   Ana sayfa sohbetinden arama (arama paneli, katılımcı kutucukları) → kişi menüsü, profil kartı.
// Görüntüler koyu ve açık temada alınır (C:/p2-pg/slice18/out). Pencereler ekran dışında açılır
// (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE).
// Önkoşul: test PostgreSQL (:55432, göçler uygulanmış) + cargo build --release. Sunucu: P2_API verilmezse
// test kendi Rust sunucusunu (:4130, P2_TEST_PORT) başlatır.   node tools/slice18b-test.mjs
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET, io, sql } from './lib.mjs'

const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice18'
rmSync(BASE, { recursive: true, force: true })
mkdirSync(BASE + '/out', { recursive: true })
const sleep = ms => new Promise(r => setTimeout(r, ms))
let pass = 0, fail = 0
const check = (name, ok, detail = '') => { ok ? pass++ : fail++; console.log(`  ${ok ? '✔' : '✘'} ${name}${detail ? ' — ' + detail : ''}`) }
async function call(method, path, body, token) {
  const r = await fetch(API + path, { method, headers: { 'content-type': 'application/json', ...(token ? { authorization: 'Bearer ' + token } : {}) }, body: body ? JSON.stringify(body) : undefined })
  return { status: r.status, body: await r.json().catch(() => null) }
}
const tag = randomBytes(3).toString('hex')
const register = async (name, displayName, extra = {}) => (await call('POST', '/auth/register', { email: `${name}_${tag}@p2.test`, username: `${name}_${tag}`, displayName, password: 'Parola123', passwordConfirm: 'Parola123', ...extra })).body
const befriend = async (a, b) => { const r = (await call('POST', '/friends/requests', { userId: b.user.id }, a.accessToken)).body; return (await call('POST', `/friends/requests/${r.requestId}/accept`, null, b.accessToken)).status }
// `project` verilirse o projenin sohbeti, verilmezse Ana sayfa sohbeti.
const say = async (from, to, org, texts, project) => {
  const c = (await call('POST', `/organizations/${org}/direct`, { userId: to.user.id, ...(project ? { projectId: project } : {}) }, from.accessToken)).body
  for (const content of texts) await call('POST', `/organizations/${org}/direct/${c.id}/messages`, { content }, from.accessToken)
  return c
}

console.log(`hazırlık: etiket ${tag}`)
const A = await register('ada', 'Ada Yıldız')
try { sql(`insert into "ServerPermission" ("userId", "permission") values ($1, 'project.create') on conflict do nothing`, [A.user.id]) } catch {}
const P = (await call('POST', '/projects', { name: `Gece Atölyesi ${tag}`, slug: `gece-atolyesi-${tag}`, description: 'Arkadaşlar testi' }, A.accessToken)).body
const org = (await call('GET', '/organizations', null, A.accessToken)).body[0]
const invite = (await call('POST', `/projects/${P.id}/admin/invites`, { maxUses: 20 }, A.accessToken)).body
check('A projesini açtı', !!P?.id && !!invite?.code)
const F1 = await register('deniz', 'Deniz Aksoy', { inviteCode: invite.code }) // arkadaş + aynı projede
const F2 = await register('mert', 'Mert Kaya') // arkadaş, ortak proje yok
const F3 = await register('elif', 'Elif Demir') // arkadaş, çevrimdışı
const R1 = await register('baran', 'Baran Çelik') // A'ya istek gönderdi
const O1 = await register('selin', 'Selin Arslan') // A istek gönderdi
const X1 = await register('kaan', 'Kaan Öztürk') // A engelledi
const Z1 = await register('zeynep', 'Zeynep Şahin', { inviteCode: invite.code }) // projede, arkadaş değil
const friendsOk = [await befriend(A, F1), await befriend(A, F2), await befriend(A, F3)]
check('A\'nın 3 arkadaşı var', friendsOk.every(s => s === 201), friendsOk.join())
const r1 = (await call('POST', '/friends/requests', { userId: A.user.id }, R1.accessToken)).body
await call('POST', '/friends/requests', { userId: O1.user.id }, A.accessToken)
await call('POST', `/blocks/${X1.user.id}`, null, A.accessToken)
const direct = async () => (await call('GET', `/organizations/${org.id}/direct`, null, A.accessToken)).body
const readAll = async () => { for (const c of await direct()) await call('POST', `/organizations/${org.id}/direct/${c.id}/read`, null, A.accessToken) }
await say(A, F3, org.id, ['Yarın bakarız'])
await say(F3, A, org.id, ['Tamam, haber ver'])
await readAll()
// Okunmamışlar: Ana sayfa (F1: 2, F2: 1) ve AYNI kişiyle (F1) proje sohbeti (3) + Z1 proje sohbeti (1).
const unreadSeed = async () => {
  await readAll()
  await say(F1, A, org.id, ['Selam! Yeni sahneyi gördün mü?', 'Akşam beraber bakalım mı'])
  await say(F2, A, org.id, ['Oyun gecesi cuma mı?'])
  await say(F1, A, org.id, ['Derleme kırıldı mı?', 'Panoya not düştüm', 'Bakabilir misin'], P.id)
  await say(Z1, A, org.id, ['Görev bende'], P.id)
}
await unreadSeed()
// Kapsamlı yıldızlar: F2 Ana sayfada, Z1 yalnız projede yıldızlı.
const star = (target, scope, starred = true) => call('PUT', `/organizations/${org.id}/preferences`, { kind: 'PERSON', targetId: target.user.id, starred, scope }, A.accessToken)
await star(F2, 'home')
await star(Z1, P.id)
// Ana sayfa grubu (projeye bağlı değil): A + F1 + F2 + F3.
const seedGroup = (await call('POST', `/organizations/${org.id}/groups`, { name: 'Hafta Sonu', userIds: [F1.user.id, F2.user.id, F3.user.id] }, A.accessToken)).body
await call('POST', `/organizations/${org.id}/groups/${seedGroup.id}/messages`, { content: 'Cumartesi buluşalım mı?' }, F2.accessToken)
check('hazırlık: Ana sayfa grubu (4 üye, projesiz)', seedGroup?.home === true && seedGroup.members.length === 4 && seedGroup.projectId === null)
const before = (await call('GET', '/friends', null, A.accessToken)).body
check('hazırlık: 3 arkadaş, 1 gelen, 1 giden, 1 engelli', before.friends.length === 3 && before.incoming.length === 1 && before.outgoing.length === 1 && before.blocked.length === 1)
const seeded = await direct()
const of = (list, user, project) => list.find(c => c.user.id === user.user.id && c.projectId === project)
check('hazırlık: aynı kişiyle (Deniz) Ana sayfa sohbeti 2, proje sohbeti 3 okunmamış', of(seeded, F1, null)?.unread === 2 && of(seeded, F1, P.id)?.unread === 3, `${of(seeded, F1, null)?.unread} / ${of(seeded, F1, P.id)?.unread}`)
// F2 çevrimiçi görünsün ve aramaları yanıtlasın (soket); F1 gerçek istemciyle bağlanır.
const f2Events = []
const sockF2 = io(API + '/stream', { auth: { token: F2.accessToken }, transports: ['websocket'] })
sockF2.on('call:incoming', p => { f2Events.push(p); setTimeout(() => sockF2.emit('call:accept', { callId: p.call.id }), 2500) })
await sleep(800)

/** İstemci süreci: izleme satırları, bekleme ve görüntü. */
function start(user, env, theme, label = 'A') {
  const c = { lines: [], cursor: 0, proc: null }
  const dir = `${BASE}/p-${user}-${theme}`
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: 'tr', theme }))
  c.proc = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123',
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  c.proc.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { c.lines.push(l); if (/arkadaşlar:|ana sayfa:|dm:|ekran:|ray:|arama:|yeni mesaj:|grup:|sohbetler:|tercih:/.test(l)) console.log(`    [${label}] ` + l.slice(0, 170)) } })
  c.next = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = c.cursor; i < c.lines.length; i++) if (re.test(c.lines[i])) { c.cursor = i + 1; return c.lines[i] } await sleep(100) } return null }
  c.wait = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = c.lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
  /** `re` satırı `from` dizininden sonra geldi mi (tekrarlayan olaylar için). */
  c.after = async (from, re, ms = 20000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = c.lines.slice(from).find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
  c.shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(c.proc.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
  c.step = s => c.next(new RegExp(`ana sayfa: otomatik pilot «${s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}»`))
  c.finish = async () => { try { c.proc.kill() } catch {} await sleep(800) }
  return c
}

const u = x => x.user.username
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
async function tour(theme, mutate) {
  console.log(`\n[${theme}] sohbet kapsamları ve Ana sayfa turu (A)`)
  const group = `Gece Ekibi ${theme === 'dark' ? 'K' : 'A'}`
  const steps = [
    'wait:ray',
    // --- proje içinde: proje sohbeti, proje sohbetinden arama, sohbet satırı menüsü
    `pdm:${u(F1)}`, `call:${u(F1)}`, 'wait:arama', 'collapse', 'hangup', `dmmenu:${u(Z1)}`, 'wait:menu',
    // --- Ana sayfa: arkadaşlar
    'online', 'all', 'pending', 'blocked', 'add', `request:${u(Z1)}`,
    // --- Ana sayfa sohbeti, listeden kaldır, yıldız
    `dm:${u(F1)}`, `dmmenu:${u(F3)}`, `hide:${u(F3)}`, 'wait:gizli', `star:${u(F1)}`,
    // --- yeni mesaj: grup ve tek kişi
    'new', `pick:${u(F1)},${u(F2)}|${group}`, 'go', 'members', `rename:${group} ✦`, 'wait:ad', 'new', `pick:${u(F3)}`, 'go',
    // --- Ana sayfa sohbetinden arama (F2 soketle yanıtlar)
    `dm:${u(F2)}`, `call:${u(F2)}`, 'wait:arama', 'collapse', 'expand', 'hangup',
    // --- kişi menüsü, profil, istek kabul, projeye dönüş
    `menu:${u(F2)}`, 'wait:menu', `profile:${u(F3)}`, 'wait:profil', ...(mutate ? [`accept:${u(R1)}`, 'pending'] : []), 'project:0', `pdm:${u(Z1)}`,
  ]
  // Karşı taraf (Deniz) gerçek istemciyle bağlı: gelen arama kartı onun penceresinde görünür.
  const b = start(u(F1), {}, theme, 'B')
  check('karşı istemci (Deniz) açıldı', !!(await b.wait(/ekran: çalışma alanı/)))
  const a = start(u(A), { P2_AUTOPILOT_HOME: steps.join(';'), P2_AUTOPILOT_HOME_DELAY: '4000' }, theme)
  check('çalışma alanı açıldı', !!(await a.wait(/ekran: çalışma alanı/)))
  check('arkadaş listesi yüklendi', !!(await a.wait(/arkadaşlar: \d+ arkadaş, \d+ gelen, \d+ giden, 1 engelli/)))
  check('sohbetler kapsamlarıyla yüklendi (Ana sayfa + proje)', !!(await a.wait(/sohbetler: [1-9]\d* ana sayfa, [1-9]\d* proje/)))
  await a.step('wait:ray'); await sleep(900)
  check('proje kenar çubuğu: SOHBETLER + okunmamış rozetleri, ray: Ana sayfa baloncukları', a.shot(`${theme}-01-proje-kenar-cubugu-sohbetler`))

  // Proje sohbeti: Ana sayfaya geçmeden, projenin içinde açılır.
  await a.step(`pdm:${u(F1)}`)
  check('proje kişisine tıklamak PROJE sohbetini açar', !!(await a.wait(new RegExp(`dm: @${esc(u(F1))} açıldı \\(proje\\)`))) && !a.lines.some(l => /ana sayfa: açıldı/.test(l)))
  await sleep(2000); a.shot(`${theme}-02-proje-sohbeti`)
  let list = await direct()
  check('proje sohbeti okundu, Ana sayfa sohbeti (aynı kişi) okunmamış kaldı', of(list, F1, P.id)?.unread === 0 && of(list, F1, null)?.unread === 2, `${of(list, F1, P.id)?.unread} / ${of(list, F1, null)?.unread}`)

  // Proje sohbetinden arama: arama paneli açılır; karşı tarafta gelen arama kartı.
  await a.step(`call:${u(F1)}`)
  check('proje sohbetinden arama: istek proje kapsamında', !!(await a.wait(/arama: bireysel arama isteği \(proje\)/)))
  check('arayan aramaya katıldı', !!(await a.wait(/arama: katıldın \(direct, 1 kişi\)/)))
  check('arama görünümü kendiliğinden açıldı (katılımcı kutucukları)', !!(await a.wait(/arama: arama görünümü açıldı \(direct\)/)))
  check('karşı istemcide gelen arama', !!(await b.wait(/arama: gelen arama Ada Yıldız/)))
  await sleep(1500)
  a.shot(`${theme}-03-proje-sohbetinden-arama-gorunumu`)
  b.shot(`${theme}-04-karsi-tarafta-gelen-arama-karti`)
  await a.step('wait:arama')
  await a.step('collapse'); await sleep(1200)
  check('arama görünümü panele küçültüldü', !!(await a.wait(/arama: panel küçültüldü/)) && a.shot(`${theme}-03b-proje-sohbetinden-arama-paneli`))
  await a.step('hangup')
  check('arama kapatıldı', !!(await a.wait(/arama: ayrıldın/)))
  await sleep(700)
  const projMsgs = (await call('GET', `/organizations/${org.id}/direct/${of(await direct(), F1, P.id).id}/messages`, null, A.accessToken)).body
  check('arama kaydı proje sohbetine düştü', projMsgs.some(m => (m.kind || '').startsWith('CALL_')))
  await a.step(`dmmenu:${u(Z1)}`); await sleep(1200)
  check('proje sohbet satırı menüsü ("Listeden kaldır")', a.shot(`${theme}-05-proje-sohbet-menusu-listeden-kaldir`))

  // Ana sayfa.
  check('Ana sayfa açıldı', !!(await a.step('online')) && !!(await a.wait(/ana sayfa: açıldı/)))
  check('Ana sayfa grupları yüklendi', !!(await a.wait(/ana sayfa: [1-9]\d* grup/)))
  await sleep(1500); a.shot(`${theme}-06-ana-sayfa-yildizli-ve-grup`)
  await a.step('all'); await sleep(1300); a.shot(`${theme}-07-arkadaslar-tumu`)
  await a.step('pending'); await sleep(1300); a.shot(`${theme}-08-arkadaslar-bekleyen`)
  await a.step('blocked'); await sleep(1300); a.shot(`${theme}-09-arkadaslar-engellenen`)
  await a.step('add'); await sleep(1300); a.shot(`${theme}-10-arkadas-ekle`)
  await a.step(`request:${u(Z1)}`)
  if (mutate) {
    check('formdan istek gönderildi', !!(await a.wait(/arkadaşlar: istek PENDING → Zeynep Şahin/)))
    const out = (await call('GET', '/friends', null, A.accessToken)).body.outgoing
    check('sunucuda giden istek (Zeynep)', out.some(r => r.user.id === Z1.user.id))
  } else {
    check('aynı istek tekrar: hata gösterilir', !!(await a.wait(/arkadaşlar: istek hatası FRIEND_REQUEST_EXISTS/)))
  }
  await sleep(1300); a.shot(`${theme}-11-arkadas-ekle-sonuc`)

  // Ana sayfa sohbeti (aynı kişiyle proje sohbetinden ayrı).
  await a.step(`dm:${u(F1)}`)
  check('Ana sayfa sohbeti açıldı', !!(await a.wait(new RegExp(`dm: @${esc(u(F1))} açıldı \\(ana sayfa\\)`))))
  await sleep(2000); a.shot(`${theme}-12-ana-sayfa-sohbeti`)
  // Listeden kaldır.
  await a.step(`dmmenu:${u(F3)}`); await sleep(1200)
  check('Ana sayfa sohbet satırı menüsü ("Listeden kaldır")', a.shot(`${theme}-13-ana-sayfa-sohbet-menusu-listeden-kaldir`))
  const mark = a.lines.length
  await a.step(`hide:${u(F3)}`)
  check('sohbet listeden kaldırıldı', !!(await a.after(mark, new RegExp(`dm: @${esc(u(F3))} listeden kaldırıldı`))))
  await sleep(900)
  list = await direct()
  const hiddenId = of(list, F3, null)?.id
  check('sunucuda gizli; mesajlar duruyor', of(list, F3, null)?.hidden === true && (await call('GET', `/organizations/${org.id}/direct/${hiddenId}/messages`, null, A.accessToken)).body.length >= 2)
  a.shot(`${theme}-14-listeden-kaldirildi`)
  await say(F3, A, org.id, ['Buradayım, döndüm'])
  check('yeni mesajla sohbet listeye döndü', !!(await a.after(mark, new RegExp(`dm: @${esc(u(F3))} listeye döndü`))) && of(await direct(), F3, null)?.hidden === false)
  // Kapsamlı yıldız: Ana sayfada yıldızlanır, projeye sızmaz.
  await a.step('wait:gizli')
  await a.step(`star:${u(F1)}`)
  check('Ana sayfada yıldızlandı (kapsam: home)', !!(await a.after(mark, new RegExp(`tercih: PERSON:home:${esc(F1.user.id)} (yıldızlandı|yıldızı kaldırıldı)`))))
  await sleep(600)
  const prefs = (await call('GET', `/organizations/${org.id}/preferences`, null, A.accessToken)).body
  const starredIn = (user, scope) => prefs.some(x => x.kind === 'PERSON' && x.targetId === user.user.id && x.scope === scope && x.starred)
  check('yıldızlar kapsamlı: Deniz yalnız Ana sayfada, Zeynep yalnız projede', starredIn(F1, 'home') && !starredIn(F1, P.id) && starredIn(Z1, P.id) && !starredIn(Z1, 'home'), JSON.stringify(prefs.filter(x => x.kind === 'PERSON').map(x => [x.scope.slice(0, 6), x.starred])))
  await sleep(700); a.shot(`${theme}-15-ana-sayfa-yildizli-bolumu`)

  // Yeni mesaj penceresi: birkaç kişi → Ana sayfa grubu.
  await a.step('new')
  check('"Yeni mesaj" penceresi açıldı (＋ artık arkadaş ekle değil)', !!(await a.after(mark, /yeni mesaj: pencere açıldı/)) && !!(await a.after(mark, /yeni mesaj: \d+ kişi listelendi/)))
  await sleep(1200); a.shot(`${theme}-16-yeni-mesaj-penceresi`)
  await a.step(`pick:${u(F1)},${u(F2)}|${group}`)
  check('iki kişi seçildi', !!(await a.after(mark, /yeni mesaj: 2 kişi seçildi/)))
  await sleep(1200); a.shot(`${theme}-17-yeni-mesaj-secim-cipleri`)
  await a.step('go')
  check('Ana sayfa grubu kuruldu', !!(await a.after(mark, new RegExp(`yeni mesaj: grup «${esc(group)}» kuruldu \\(3 üye\\)`))))
  const homeGroups = (await call('GET', `/organizations/${org.id}/groups?home=1`, null, A.accessToken)).body
  const made = homeGroups.find(g => g.name === group)
  check('sunucuda projesiz grup (home)', !!made && made.home === true && made.projectId === null && made.members.length === 3)
  await sleep(1800); a.shot(`${theme}-18-ana-sayfa-grubu-sohbeti`)
  await a.step('members'); await sleep(1300); a.shot(`${theme}-19-grup-uyeleri-ve-adi`)
  await a.step(`rename:${group} ✦`)
  check('grup yeniden adlandırıldı', !!(await a.after(mark, new RegExp(`grup: ad «${esc(group)} ✦»`))) && (await call('GET', `/organizations/${org.id}/groups?home=1`, null, A.accessToken)).body.some(g => g.id === made?.id && g.name === `${group} ✦`))
  // Tek kişi → Ana sayfa birebir sohbeti.
  await a.step('wait:ad')
  await a.step('new'); await a.step(`pick:${u(F3)}`); await a.step('go')
  check('tek kişi seçilince Ana sayfa sohbeti açılır', !!(await a.after(mark, new RegExp(`yeni mesaj: @${esc(u(F3))} ile sohbet`))) && !!(await a.after(mark, new RegExp(`dm: @${esc(u(F3))} açıldı \\(ana sayfa\\)`))))
  await sleep(1500); a.shot(`${theme}-20-yeni-mesajdan-birebir-sohbet`)

  // Ana sayfa sohbetinden arama: arama paneli açılır, karşı taraf yanıtlayınca katılımcı görünür.
  await a.step(`dm:${u(F2)}`)
  const callMark = a.lines.length
  f2Events.length = 0
  await a.step(`call:${u(F2)}`)
  check('Ana sayfa sohbetinden arama: istek Ana sayfa kapsamında', !!(await a.after(callMark, /arama: bireysel arama isteği \(ana sayfa\)/)))
  check('arayan aramaya katıldı', !!(await a.after(callMark, /arama: katıldın \(direct, 1 kişi\)/)))
  check('arama görünümü kendiliğinden açıldı', !!(await a.after(callMark, /arama: arama görünümü açıldı \(direct\)/)))
  await sleep(700)
  check('karşı tarafa çağrı ulaştı (projectId yok)', f2Events.length === 1 && f2Events[0].call.projectId === null, JSON.stringify(f2Events[0]?.call?.projectId))
  a.shot(`${theme}-21-ana-sayfa-aramasi-caliyor`)
  check('karşı taraf yanıtladı: arama 2 kişi', !!(await a.after(callMark, /olay: arama durumu direct \(2 kişi\)/)))
  await sleep(1200); a.shot(`${theme}-22-ana-sayfa-aramasi-baglandi`)
  await a.step('wait:arama')
  await a.step('collapse')
  check('arama paneline küçültüldü (sustur / sesi kapat / kapat)', !!(await a.after(callMark, /arama: panel küçültüldü/)))
  await sleep(1500); a.shot(`${theme}-23-ana-sayfa-aramasi-panel`)
  await a.step('expand')
  check('arama görünümü yeniden büyütüldü', !!(await a.after(callMark, /arama: panel büyütüldü/)))
  await a.step('hangup')
  check('arama kapatıldı', !!(await a.after(callMark, /arama: ayrıldın/)))
  await sleep(600)
  const homeMsgs = (await call('GET', `/organizations/${org.id}/direct/${of(await direct(), F2, null).id}/messages`, null, A.accessToken)).body
  check('arama kaydı Ana sayfa sohbetine düştü', homeMsgs.some(m => (m.kind || '').startsWith('CALL_')))

  await a.step(`menu:${u(F2)}`); await sleep(1200); a.shot(`${theme}-24-kisi-menusu`)
  await a.step(`profile:${u(F3)}`); await sleep(1500); a.shot(`${theme}-25-profil-karti`)
  if (mutate) {
    await a.step(`accept:${u(R1)}`)
    await sleep(1500)
    const now = (await call('GET', '/friends', null, A.accessToken)).body
    check('gelen istek arayüzden kabul edildi', now.friends.some(f => f.id === R1.user.id) && now.incoming.length === 0)
    await a.step('pending'); await sleep(1200); a.shot(`${theme}-26-bekleyen-kabul-sonrasi`)
  }
  const homeEntries = a.lines.filter(l => /ana sayfa: açıldı/.test(l)).length
  check('projeye dönüldü', !!(await a.step('project:0')))
  await sleep(2000); a.shot(`${theme}-27-projeye-donus`)
  const back = a.lines.length
  await a.step(`pdm:${u(Z1)}`)
  check('proje kişisine mesaj: sohbet projede açılır (Ana sayfaya geçilmez)', !!(await a.after(back, new RegExp(`dm: @${esc(u(Z1))} açıldı \\(proje\\)`))) && a.lines.filter(l => /ana sayfa: açıldı/.test(l)).length === homeEntries)
  await sleep(1800); a.shot(`${theme}-28-proje-kisisi-proje-sohbeti`)
  await a.finish()
  await b.finish()
}

await tour('dark', true)
await unreadSeed()
// Açık tema turu aynı adımları yineler: gizlenen sohbet ve yıldız ilk turdaki gibi başlasın.
await star(F1, 'home', false)
await tour('light', false)

sockF2.close()
stop()
console.log(`\n${pass} geçti, ${fail} kaldı — görüntüler: ${BASE}/out`)
process.exit(fail ? 1 : 0)
