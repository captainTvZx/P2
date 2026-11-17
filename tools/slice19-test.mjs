// Dilim 19 uçtan uca testi: kullanıcının bildirdiği beş düzeltme (docs/proje-bazli-yapi.md "Arayüz düzeltmeleri").
//   [1] Keşfet her zaman sol rayla açılır: hiç projesi olmayan kişi başlangıç ekranındaki "Projelere göz at"
//       ile girince çalışma alanı projesiz açılır (ray: Ana sayfa, ＋, pusula, profil), başlık "Keşfet".
//   [2] Bildirimler sağ üstteki tek karttan (toasts.rs): "… projesine katıldın" başarı bildirimi
//       (`bildirim[ok]`), yetki reddi uyarı (`bildirim[uyarı]`); eski kırmızı kutu yok.
//   [3] Raydaki proje rozeti: proje açılıp okununca kaybolur, okunmayan kanalın sayısı kalır; thread yanıtı
//       kanal rozetine sayılmaz (sunucu `/projects/unread` + `/channels`).
//   [4] Sağ tık / "•••" menüleri: yönetim satırları yalnız yetkisi olana görünür; "Projeden ayrıl" hep durur.
//   [5] Proje içinde "＋" → "Yeni mesaj" penceresi (yalnız proje üyeleri): tek kişi → proje sohbeti, birkaç
//       kişi → proje grubu (ChatGroup.projectId); grup kenar çubuğunda üye avatarlarıyla, sağ tık menüsü,
//       üyeler ve ad; `group.create` / `direct.send` yetkisi yoksa "＋" görünmez, sunucu 403 döner.
// Rust istemcisi otomatik pilot kancalarıyla (masaüstüne girdi göndermeden) sürülür; pencereler ekran
// dışında açılır (P2_TEST_WINDOW_POS), ses aygıtı açılmaz (P2_NO_AUDIO_DEVICE). Görüntüler koyu ve açık
// temada: C:/p2-pg/slice19/out.
// Önkoşul: test PostgreSQL (:55432) + cargo build --release. Sunucu: P2_API verilmezse test kendi Rust
// sunucusunu (:4130, P2_TEST_PORT) başlatır.   node tools/slice19-test.mjs [--server-only]
import { spawn, execFileSync } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { randomBytes } from 'node:crypto'
import { testServer, TARGET, sql } from './lib.mjs'

const { api: API, stop } = await testServer()
const BIN = TARGET + '/'
const BASE = 'C:/p2-pg/slice19'
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
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const u = x => x.user.username

// ---------------------------------------------------------------- hazırlık
console.log(`hazırlık: etiket ${tag}`)
const O = await register('sahip', 'Sahip Kaptan')
try { sql(`insert into "ServerPermission" ("userId", "permission") values ($1, 'project.create') on conflict do nothing`, [O.user.id]) } catch {}
const ATOLYE = `Atölye ${tag}`, LIMAN = `Liman ${tag}`, KISITLI = `Kısıtlı ${tag}`
// Ada sırası ray sırasıdır (oluşturma sırası): Atölye (ilk seçilen), Liman (herkese açık), Kısıtlı.
const P1 = (await call('POST', '/projects', { name: ATOLYE, slug: `atolye-${tag}`, description: 'Düzeltme testi' }, O.accessToken)).body
const P2 = (await call('POST', '/projects', { name: LIMAN, slug: `liman-${tag}`, description: 'Herkese açık topluluk', visibility: 'PUBLIC', category: 'COMMUNITY', tagline: `Dalga ${tag}` }, O.accessToken)).body
const P3 = (await call('POST', '/projects', { name: KISITLI, slug: `kisitli-${tag}`, description: 'Üyelerin yetkisi kısık' }, O.accessToken)).body
check('üç proje açıldı (biri herkese açık)', !!P1?.id && P2?.visibility === 'PUBLIC' && !!P3?.id)
const org = (await call('GET', '/organizations', null, O.accessToken)).body[0]
// Test projesi Keşfet'te (ortak test veritabanında) kalmasın: bitince özel yapılır.
const cleanup = () => call('PATCH', `/projects/${P2.id}/admin`, { visibility: 'PRIVATE' }, O.accessToken)
const invite = async p => (await call('POST', `/projects/${p.id}/admin/invites`, { maxUses: 20 }, O.accessToken)).body.code
const [c1, c2, c3] = [await invite(P1), await invite(P2), await invite(P3)]
const M = await register('uye', 'Mina Üye', { inviteCode: c1 }) // düz üye: üç projede de
const N = await register('nil', 'Nil Kara', { inviteCode: c1 }) // Atölye + Kısıtlı üyesi
const K = await register('kerem', 'Kerem Ay', { inviteCode: c1 }) // Atölye üyesi
const V = await register('yeni', 'Yeni Gelen') // hiçbir projede değil
for (const code of [c2, c3]) await call('POST', `/invites/${code}/join`, null, M.accessToken)
await call('POST', `/invites/${c3}/join`, null, N.accessToken)
const mine = (await call('GET', '/projects', null, M.accessToken)).body
check('üye üç projede, yeni gelen hiçbirinde', mine.length === 3 && (await call('GET', '/projects', null, V.accessToken)).body.length === 0, String(mine.length))

// Kısıtlı projede herkes rolünden kanal yönetimi, grup kurma ve bireysel mesaj alınır.
const roles = (await call('GET', `/projects/${P3.id}/roles`, null, O.accessToken)).body
const everyone = roles.find(r => r.everyone)
const trimmed = (everyone?.permissions || []).filter(p => !['channel.manage', 'group.create', 'direct.send'].includes(p))
const setRole = await call('PATCH', `/projects/${P3.id}/roles/${everyone?.id}/permissions`, { permissions: trimmed }, O.accessToken)
const mPerms = async p => (await call('GET', `/projects/${p.id}/permissions/me`, null, M.accessToken)).body
const kisitli = await mPerms(P3)
check('kısıtlı projede üyenin channel.manage / group.create / direct.send yetkisi yok', setRole.status === 200 && !['channel.manage', 'group.create', 'direct.send'].some(p => kisitli.permissions.includes(p)), JSON.stringify(kisitli.permissions))
const atolye = await mPerms(P1)
check('düz üyede yönetim yetkisi yok (admin.panel, organization.manage, member.*)', !atolye.owner && !['admin.panel', 'organization.manage', 'member.invite', 'member.kick', 'member.roles'].some(p => atolye.permissions.includes(p)))

// ---------------------------------------------------------------- [3] sunucu: kanal okunmamışları
console.log('\n[3] Ray rozeti (sunucu): okununca sıfırlanır, thread yanıtı sayılmaz')
const channelsOf = async (p, who) => (await call('GET', `/organizations/${org.id}/projects/${p.id}/channels`, null, who.accessToken)).body.flatMap(c => c.channels)
const chL = await channelsOf(P2, O)
const [firstL, genelL] = [chL.find(c => c.type !== 'VOICE'), chL.find(c => c.name === 'genel')]
const post = async (p, ch, content, who = O) => (await call('POST', `/organizations/${org.id}/projects/${p.id}/channels/${ch.id}/messages`, { content }, who.accessToken)).body
const unreadOf = async who => (await call('GET', `/organizations/${org.id}/projects/unread`, null, who.accessToken)).body
const readCh = (p, ch, who) => call('POST', `/organizations/${org.id}/projects/${p.id}/channels/${ch.id}/read`, null, who.accessToken)
// Temiz başlangıç: üyenin bütün kanalları okunmuş.
for (const p of [P1, P2, P3]) for (const ch of await channelsOf(p, M)) await readCh(p, ch, M)
check('başlangıçta okunmamış yok', Object.keys(await unreadOf(M)).length === 0, JSON.stringify(await unreadOf(M)))
const m1 = await post(P2, genelL, 'Liman açıldı, hoş geldiniz')
check('yeni kanal mesajı: proje rozeti 1', (await unreadOf(M))[P2.id] === 1, JSON.stringify(await unreadOf(M)))
const reply = await call('POST', `/organizations/${org.id}/projects/${P2.id}/channels/${genelL.id}/messages/${m1.id}/replies`, { content: 'Thread yanıtı' }, O.accessToken)
check('thread yanıtı kanal rozetine sayılmaz (hâlâ 1)', reply.status === 201 && (await unreadOf(M))[P2.id] === 1, `${reply.status} ${JSON.stringify(await unreadOf(M))}`)
check('kenar çubuğu sayısı da 1 (kanal listesi)', (await channelsOf(P2, M)).find(c => c.id === genelL.id)?.unread === 1)
await readCh(P2, genelL, M)
check('kanal okununca rozet kalmaz', (await unreadOf(M))[P2.id] === undefined, JSON.stringify(await unreadOf(M)))
await call('POST', `/organizations/${org.id}/projects/${P2.id}/channels/${genelL.id}/messages/${m1.id}/replies`, { content: 'Okuduktan sonra gelen yanıt' }, O.accessToken)
check('okuduktan sonra gelen thread yanıtı hayalet rozet üretmez', (await unreadOf(M))[P2.id] === undefined && (await channelsOf(P2, M)).find(c => c.id === genelL.id)?.unread === 0, JSON.stringify(await unreadOf(M)))
// Ayrılınan (gizlenen) kanal sayılmaz.
await post(P2, genelL, 'Gizlenen kanala mesaj')
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'CHANNEL', targetId: genelL.id, hidden: true }, M.accessToken)
check('ayrıldığım kanalın okunmamışı rozete sayılmaz', (await unreadOf(M))[P2.id] === undefined, JSON.stringify(await unreadOf(M)))
await call('PUT', `/organizations/${org.id}/preferences`, { kind: 'CHANNEL', targetId: genelL.id, hidden: false }, M.accessToken)
await readCh(P2, genelL, M)

// ---------------------------------------------------------------- [5] sunucu: proje grubu ve yetkisi
console.log('\n[5] Proje grubu (sunucu): üyeler projeden, yetki group.create')
const g1 = await call('POST', `/organizations/${org.id}/groups`, { projectId: P1.id, name: 'Sunucu Grubu', userIds: [N.user.id, K.user.id, V.user.id] }, M.accessToken)
check('proje grubu kuruldu (projectId dolu, home değil)', g1.status === 201 && g1.body.projectId === P1.id && g1.body.home === false, `${g1.status}`)
check('yalnız proje üyeleri eklendi (projede olmayan kişi alınmaz)', g1.body?.members?.length === 3 && !g1.body.members.some(m => m.id === V.user.id), String(g1.body?.members?.length))
check('proje grupları listesinde, Ana sayfa gruplarında değil', (await call('GET', `/organizations/${org.id}/groups?projectId=${P1.id}`, null, M.accessToken)).body.some(g => g.id === g1.body.id) && !(await call('GET', `/organizations/${org.id}/groups?home=1`, null, M.accessToken)).body.some(g => g.id === g1.body.id))
const g2 = await call('POST', `/organizations/${org.id}/groups`, { projectId: P3.id, userIds: [N.user.id, O.user.id] }, M.accessToken)
check('group.create yetkisi yoksa 403', g2.status === 403, `${g2.status} ${g2.body?.code}`)
const d2 = await call('POST', `/organizations/${org.id}/direct`, { userId: N.user.id, projectId: P3.id }, M.accessToken)
check('direct.send yetkisi yoksa proje sohbeti 403', d2.status === 403, `${d2.status} ${d2.body?.code}`)
await call('POST', `/organizations/${org.id}/groups/${g1.body.id}/leave`, null, M.accessToken)

if (process.argv.includes('--server-only')) {
  await cleanup()
  stop()
  console.log(`\n${pass} geçti, ${fail} kaldı (yalnız sunucu)`)
  process.exit(fail ? 1 : 0)
}

// ---------------------------------------------------------------- istemci
function start(user, env, theme, label, lang = 'tr') {
  const c = { lines: [], cursor: 0, proc: null }
  const dir = `${BASE}/p-${label}-${theme}-${lang}`
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(dir + '/settings.json', JSON.stringify({ mode: 'client', apiUrl: API, language: lang, theme }))
  c.proc = spawn(BIN + 'p2-client.exe', [], {
    env: {
      ...process.env, P2_PROFILE_DIR: dir, P2_AUTOPILOT_SERVER: API, P2_AUTOPILOT_USER: user, P2_AUTOPILOT_PASS: 'Parola123',
      P2_TEST_WINDOW_POS: '-12000,-12000', P2_NO_AUDIO_DEVICE: '1', P2_NO_TRAY: '1', P2_SILENT_NOTIFY: '1', P2_SPLASH: '0', P2_NO_ELECTRON_IMPORT: '1', ...env,
    },
  })
  c.proc.stdout.on('data', d => { for (const l of String(d).split(/\r?\n/).filter(Boolean)) { c.lines.push(l); if (/keşfet:|başlangıç:|ana sayfa: otomatik|ekran:|ray:|yeni mesaj:|grup:|bildirim\[|başlık:|proje menüsü|katıl:/.test(l)) console.log(`    [${label}] ` + l.slice(0, 220)) } })
  c.wait = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = c.lines.find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
  c.after = async (from, re, ms = 25000) => { const end = Date.now() + ms; while (Date.now() < end) { const hit = c.lines.slice(from).find(l => re.test(l)); if (hit) return hit; await sleep(120) } return null }
  c.next = async (re, ms = 30000) => { const end = Date.now() + ms; while (Date.now() < end) { for (let i = c.cursor; i < c.lines.length; i++) if (re.test(c.lines[i])) { c.cursor = i + 1; return c.lines[i] } await sleep(100) } return null }
  c.step = s => c.next(new RegExp(`ana sayfa: otomatik pilot «${esc(s)}»`))
  c.shot = name => { try { execFileSync(BIN + 'p2-wincap.exe', ['--client', String(c.proc.pid), `${BASE}/out/${name}.png`]); return true } catch (e) { console.log('    görüntü alınamadı: ' + String(e.stderr || e.message).trim().slice(0, 160)); return false } }
  c.finish = async () => { try { c.proc.kill() } catch {} await sleep(800) }
  return c
}

// [1] + [2]: projesi olmayan kişi başlangıç ekranından Keşfet'e girer; ray görünür, başlık "Keşfet";
// katılınca sağ üstte başarı bildirimi.
async function discoverTour(theme, user, join, lang = 'tr') {
  console.log(`\n[1] Başlangıç → Keşfet rayla açılır (${theme}, ${lang}${join ? ', katıl' : ''})`)
  const steps = [`search:${tag}`, `open:${LIMAN}`, ...(join ? ['join'] : [])]
  const c = start(u(user), { P2_AUTOPILOT_DISCOVER: steps.join(';'), P2_AUTOPILOT_DISCOVER_DELAY: '3500' }, theme, `kesif-${lang}`, lang)
  check('başlangıç ekranı (hiç proje yok)', !!(await c.wait(/ekran: kuruluş/)))
  check('"Projelere göz at" çalışma alanını projesiz açar', !!(await c.wait(/başlangıç: keşfet çalışma alanında açılıyor/)) && !!(await c.wait(/ekran: çalışma alanı/)))
  check('ray projesiz kuruldu (Ana sayfa, ＋, pusula, profil)', !!(await c.wait(/ray: 0 proje/)))
  check('Keşfet rayın yanında açıldı', !!(await c.wait(/keşfet: çalışma alanında açıldı/)))
  check(`başlık "${lang === 'en' ? 'Discover' : 'Keşfet'}" ("Kuruluş" değil)`, !!(await c.wait(new RegExp(`başlık: ${lang === 'en' ? 'Discover' : 'Keşfet'}$`))) && !c.lines.some(l => /başlık: Kuruluş/.test(l)), c.lines.filter(l => /başlık:/.test(l)).map(l => l.split('başlık: ')[1]).join(' → '))
  if (lang === 'en') check('İngilizce arayüzde Türkçe başlık yok ("Giriş", "Kuruluş")', !c.lines.some(l => /başlık: (Giriş|Kuruluş|Kurulum|Proje)$/.test(l)), c.lines.filter(l => /başlık:/.test(l)).map(l => l.split('başlık: ')[1]).join(' → '))
  await c.wait(/keşfet: \d+ haber/); await sleep(2500)
  check('görüntü: Keşfet + sol ray (projesiz)', c.shot(`${theme}-01-kesfet-rayla-projesiz${lang === 'en' ? '-en' : ''}`))
  if (join) {
    check('ayrıntı çekmecesi', !!(await c.wait(new RegExp(`keşfet: ayrıntı ${esc(LIMAN)}`))))
    check('katıldı', !!(await c.wait(new RegExp(`keşfet: katıldın ${esc(LIMAN)}`))))
    check('"… projesine katıldın" BAŞARI bildirimi (sağ üst)', !!(await c.wait(new RegExp(`bildirim\\[ok\\]: ${esc(LIMAN)} projesine katıldın`))), c.lines.filter(l => /bildirim\[/.test(l)).join(' | '))
    check('kırmızı / hata bildirimi yok', !c.lines.some(l => /bildirim\[hata\]/.test(l)))
    await sleep(900)
    check('görüntü: katılma bildirimi', c.shot(`${theme}-02-katildin-bildirimi`))
    check('proje raya eklendi ve seçildi', !!(await c.wait(new RegExp(`başlık: ${esc(LIMAN)}$`))))
  }
  await c.finish()
}

// [3] [4] [5]: düz üye. Ray rozeti, menüler, proje içinde yeni sohbet / grup.
async function memberTour(theme) {
  console.log(`\n[3][4][5] Üye turu (${theme})`)
  // Rozet verisi: Liman'ın başlangıç kanalına (açılınca okunur) 1, "genel" kanalına (okunmaz) 2 mesaj.
  for (const p of [P1, P2, P3]) for (const ch of await channelsOf(p, M)) await readCh(p, ch, M)
  await post(P2, firstL, `Duyuru ${theme}`)
  await post(P2, genelL, `Genel 1 ${theme}`)
  await post(P2, genelL, `Genel 2 ${theme}`)
  check('hazırlık: Liman rozeti 3', (await unreadOf(M))[P2.id] === 3, JSON.stringify(await unreadOf(M)))
  const group = `Gece Vardiyası ${theme === 'dark' ? 'K' : 'A'}`
  const steps = [
    'wait:ray',
    // [4] menüler: seçili olmayan proje, seçili proje, başlık menüsü
    `projmenu:${LIMAN}`, `projmenu:${ATOLYE}`, 'headmenu:1', 'wait:menu',
    // [5] proje içinde yeni mesaj: grup
    'pnew', `pick:${u(N)},${u(K)}|${group}`, 'go', 'wait:grup', `pgroupmenu:${group}`, 'members', `rename:${group} ✦`, 'wait:ad',
    // [5] tek kişi → proje sohbeti
    'pnew', `pick:${u(N)}`, 'go',
    // [3] rozet: Liman'a gir (başlangıç kanalı okunur), Atölye'ye dön
    `goto:${LIMAN}`, 'wait:okuma', `goto:${ATOLYE}`, 'wait:rozet',
    // [5] yetkisiz proje: ＋ yok
    `goto:${KISITLI}`, 'headmenu:1', 'pnew', 'wait:son',
  ]
  const c = start(u(M), { P2_AUTOPILOT_HOME: steps.join(';'), P2_AUTOPILOT_HOME_DELAY: '3800', P2_AUTOPILOT_PROJECT: ATOLYE }, theme, 'uye')
  check('çalışma alanı açıldı', !!(await c.wait(/ekran: çalışma alanı/)))
  check('ray: 3 proje, yetkiler alındı', !!(await c.wait(/ray: 3 proje/)) && !!(await c.wait(/ray: 3 projenin yetkileri alındı/)))
  check('ray rozeti: Liman 3', !!(await c.wait(new RegExp(`ray: rozetler \\[.*${esc(LIMAN)}=3`))), c.lines.filter(l => /ray: rozetler/.test(l)).pop())
  await c.step('wait:ray'); await sleep(1200)
  check('görüntü: proje kenar çubuğu (SOHBETLER ＋, GRUPLAR ＋) ve ray rozeti', c.shot(`${theme}-03-proje-kenar-cubugu-arti-ve-rozet`))

  // [4]
  let mark = c.lines.length
  await c.step(`projmenu:${LIMAN}`)
  const menuL = await c.after(mark, new RegExp(`ray: proje menüsü «${esc(LIMAN)}» \\[`))
  check('sağ tık (seçili olmayan proje): üyeye yalnız "Projeden ayrıl"', /\[leave\]/.test(menuL || ''), menuL?.split('ray: ')[1])
  await sleep(1500); check('görüntü: üyenin sağ tık menüsü', c.shot(`${theme}-04-uye-sag-tik-menusu`))
  mark = c.lines.length
  await c.step(`projmenu:${ATOLYE}`)
  const menuA = await c.after(mark, new RegExp(`ray: proje menüsü «${esc(ATOLYE)}» \\[`))
  check('sağ tık (seçili proje): "Proje ayarları / Yönetim", "Proje ayarları…", "Projeyi sil" YOK', /\[leave\]/.test(menuA || ''), menuA?.split('ray: ')[1])
  mark = c.lines.length
  await c.step('headmenu:1')
  const head = await c.after(mark, /proje menüsü: \[/)
  check('"•••" menüsü: yönetim ve davet satırı yok; kanal işlemleri (channel.manage) ve ayrıl var', /\[category, channel, browse, leave\]/.test(head || ''), head?.split('proje menüsü: ')[1])
  await sleep(1500); check('görüntü: üyenin proje menüsü', c.shot(`${theme}-05-uye-proje-menusu`))
  await c.step('wait:menu')

  // [5] grup
  mark = c.lines.length
  await c.step('pnew')
  check('proje içinde "＋": "Yeni mesaj" penceresi yalnız proje üyeleriyle', !!(await c.after(mark, new RegExp(`yeni mesaj: proje penceresi açıldı \\(${esc(ATOLYE)} · 3 üye\\)`))), c.lines.slice(mark).find(l => /yeni mesaj:/.test(l)))
  await sleep(1300); check('görüntü: projeye bağlı "Yeni mesaj" penceresi', c.shot(`${theme}-06-projede-yeni-mesaj-penceresi`))
  await c.step(`pick:${u(N)},${u(K)}|${group}`)
  check('iki kişi seçildi', !!(await c.after(mark, /yeni mesaj: 2 kişi seçildi/)))
  await sleep(1300); c.shot(`${theme}-07-projede-yeni-mesaj-secim`)
  await c.step('go')
  check('proje grubu kuruldu', !!(await c.after(mark, new RegExp(`yeni mesaj: proje grubu «${esc(group)}» kuruldu \\(3 üye\\)`))))
  check('başarı bildirimi: grup kuruldu', !!(await c.after(mark, new RegExp(`bildirim\\[ok\\]: “${esc(group)}” grubu kuruldu`))))
  const made = (await call('GET', `/organizations/${org.id}/groups?projectId=${P1.id}`, null, M.accessToken)).body.find(g => g.name === group)
  check('sunucuda proje grubu (projectId = Atölye, 3 üye, Ana sayfa grubu değil)', !!made && made.projectId === P1.id && made.home === false && made.members.length === 3, JSON.stringify(made && { p: made.projectId === P1.id, home: made.home, n: made.members.length }))
  check('Ana sayfa gruplarına sızmadı', !(await call('GET', `/organizations/${org.id}/groups?home=1`, null, M.accessToken)).body.some(g => g.name === group))
  await sleep(1800); check('görüntü: proje grubu kenar çubuğunda (üye avatarları) ve sohbeti', c.shot(`${theme}-08-proje-grubu-kenar-cubugunda`))
  await c.step('wait:grup')
  await c.step(`pgroupmenu:${group}`)
  check('proje grubu sağ tık menüsü', !!(await c.after(mark, new RegExp(`grup: proje grubu menüsü «${esc(group)}»`))))
  await sleep(1300); c.shot(`${theme}-09-proje-grubu-sag-tik-menusu`)
  await c.step('members'); await sleep(1500); check('görüntü: grup üyeleri ve adı', c.shot(`${theme}-10-proje-grubu-uyeler-ve-ad`))
  await c.step(`rename:${group} ✦`)
  check('proje grubu yeniden adlandırıldı', !!(await c.after(mark, new RegExp(`grup: ad «${esc(group)} ✦»`))) && (await call('GET', `/organizations/${org.id}/groups?projectId=${P1.id}`, null, M.accessToken)).body.some(g => g.id === made?.id && g.name === `${group} ✦`))
  await c.step('wait:ad')
  // [5] tek kişi
  mark = c.lines.length
  await c.step('pnew'); await c.step(`pick:${u(N)}`); await c.step('go')
  check('tek kişi: PROJE sohbeti açılır (Ana sayfaya geçilmez)', !!(await c.after(mark, new RegExp(`yeni mesaj: @${esc(u(N))} ile proje sohbeti`))) && !!(await c.after(mark, new RegExp(`dm: @${esc(u(N))} açıldı \\(proje\\)`))) && !c.lines.some(l => /ana sayfa: açıldı/.test(l)))
  await sleep(1500); c.shot(`${theme}-11-projede-yeni-mesajdan-proje-sohbeti`)

  // [3] rozet
  mark = c.lines.length
  await c.step(`goto:${LIMAN}`)
  check('Liman açıldı', !!(await c.after(mark, new RegExp(`başlık: ${esc(LIMAN)}$`))))
  await c.step('wait:okuma')
  mark = c.lines.length
  await c.step(`goto:${ATOLYE}`)
  const left = await c.after(mark, new RegExp(`ray: ${esc(LIMAN)} projesinden çıkıldı, kanal okunmamışı (\\d+)`))
  check('projeden çıkarken rozet eldeki kanal listesinden yazılır: okunan kanal düştü, okunmayan (genel: 2) kaldı', /okunmamışı 2$/.test(left || ''), left?.split('ray: ')[1])
  const fresh = await c.after(mark, new RegExp(`ray: rozetler \\[.*${esc(LIMAN)}=\\d+`))
  check('sunucudan tazelenen rozet de 2 (eskiden 3 kalıyordu)', new RegExp(`${esc(LIMAN)}=2[,\\]]`).test(fresh || ''), fresh?.split('ray: ')[1])
  check('sunucu toplamı 2', (await unreadOf(M))[P2.id] === 2, JSON.stringify(await unreadOf(M)))
  await c.step('wait:rozet'); await sleep(600)
  check('görüntü: ray rozeti 2', c.shot(`${theme}-12-ray-rozeti-okunduktan-sonra`))
  // Kalan kanal da okununca rozet tamamen kalkar (istemci bir sonraki tazelemede 0 görür).
  await readCh(P2, genelL, M)

  // [5] yetkisiz proje
  mark = c.lines.length
  await c.step(`goto:${KISITLI}`)
  const cleared = await c.after(mark, new RegExp(`ray: rozetler \\[.*${esc(LIMAN)}=\\d+`))
  check('her şey okununca rozet kalmaz (0)', new RegExp(`${esc(LIMAN)}=0[,\\]]`).test(cleared || ''), cleared?.split('ray: ')[1])
  mark = c.lines.length
  await c.step('headmenu:1')
  const head3 = await c.after(mark, /proje menüsü: \[/)
  check('kısıtlı projede "•••" menüsü: yalnız göz at + ayrıl (soluk satır yok)', /\[browse, leave\]/.test(head3 || ''), head3?.split('proje menüsü: ')[1])
  await sleep(1500); check('görüntü: yetkisiz üyenin proje menüsü ve ＋ olmayan kenar çubuğu', c.shot(`${theme}-13-yetkisiz-proje-menusu`))
  mark = c.lines.length
  await c.step('pnew')
  check('yetkisiz projede yeni mesaj: uyarı bildirimi (sağ üst), pencere açılmaz', !!(await c.after(mark, /bildirim\[uyarı\]: Bu projede sohbet başlatma yetkin yok/)) && !c.lines.slice(mark).some(l => /yeni mesaj: proje penceresi açıldı/.test(l)))
  await sleep(900); check('görüntü: uyarı bildirimi', c.shot(`${theme}-14-uyari-bildirimi`))
  await c.finish()
}

// [4]: proje sahibi bütün satırları görür.
async function ownerTour(theme) {
  console.log(`\n[4] Sahip turu (${theme})`)
  const steps = ['wait:ray', `projmenu:${LIMAN}`, `projmenu:${ATOLYE}`, 'headmenu:1', 'wait:son']
  const c = start(u(O), { P2_AUTOPILOT_HOME: steps.join(';'), P2_AUTOPILOT_HOME_DELAY: '3500', P2_AUTOPILOT_PROJECT: ATOLYE }, theme, 'sahip')
  check('çalışma alanı açıldı', !!(await c.wait(/ekran: çalışma alanı/)) && !!(await c.wait(/ray: 3 projenin yetkileri alındı/)))
  let mark = c.lines.length
  await c.step(`projmenu:${LIMAN}`)
  const m1 = await c.after(mark, new RegExp(`ray: proje menüsü «${esc(LIMAN)}» \\[`))
  check('sahip (seçili olmayan proje): yönetim + sil + ayrıl ("Proje ayarları…" yalnız seçili projede)', /\[admin, delete, leave\]/.test(m1 || ''), m1?.split('ray: ')[1])
  mark = c.lines.length
  await c.step(`projmenu:${ATOLYE}`)
  const m2 = await c.after(mark, new RegExp(`ray: proje menüsü «${esc(ATOLYE)}» \\[`))
  check('sahip (seçili proje): yönetim + ayarlar + sil + ayrıl', /\[admin, settings, delete, leave\]/.test(m2 || ''), m2?.split('ray: ')[1])
  await sleep(1500); check('görüntü: sahibin sağ tık menüsü', c.shot(`${theme}-15-sahip-sag-tik-menusu`))
  mark = c.lines.length
  await c.step('headmenu:1')
  const head = await c.after(mark, /proje menüsü: \[/)
  check('sahip "•••" menüsü: hepsi', /\[category, channel, browse, invite, admin, leave\]/.test(head || ''), head?.split('proje menüsü: ')[1])
  await sleep(1500); c.shot(`${theme}-16-sahip-proje-menusu`)
  await c.finish()
}

// Koyu tema: yeni gelen Keşfet'ten katılır. Açık tema: ikinci bir yeni gelen yalnız bakar; İngilizce başlık.
await discoverTour('dark', V, true)
const V2 = await register('yeni2', 'İkinci Gelen')
await discoverTour('light', V2, false)
await discoverTour('dark', V2, false, 'en')
await memberTour('dark')
await memberTour('light')
await ownerTour('dark')

await cleanup()
stop()
console.log(`\n${pass} geçti, ${fail} kaldı — görüntüler: ${BASE}/out`)
process.exit(fail ? 1 : 0)
