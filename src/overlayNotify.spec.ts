import { expect, test } from 'vitest'

// apiBase/realtime penceresiz çalışmaz; en küçük sahte pencere yeterli (router saf).
;(globalThis as any).window = {
  location: { search: '', origin: 'http://yerel' },
  localStorage: { getItem: () => null, setItem() {} },
  addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true },
}

const { routeNotification, planToastAction } = await import('./overlayNotify')
type Ctx = Parameters<typeof routeNotification>[1]

const all = { messages: true, calls: true, play: true, tasks: true, releases: true, downloads: true }
const ctx = (patch: Partial<Ctx> = {}, notifications: Partial<typeof all> = {}): Ctx => ({
  overlayReady: true, me: 'me', now: 1, settings: { overlayNotifications: { ...all, ...notifications }, playToasts: true }, ...patch,
})
const ayse = { id: 'u1', displayName: 'Ayşe', avatarUrl: null }

test('overlay hazır değilse hiçbir şey yönlendirilmez, masaüstü davranışı sürer', () => {
  const decision = routeNotification({ type: 'dm', from: ayse, content: 'selam' }, ctx({ overlayReady: false }))
  expect(decision).toEqual({ overlay: null, suppressDesktop: false })
})

test('DM: mesaj kartı + "Aç" → sohbet penceresi, masaüstü bildirimi bastırılır', () => {
  const decision = routeNotification({ type: 'dm', from: ayse, content: '**selam** [bak](https://x.y)' }, ctx())
  expect(decision.suppressDesktop).toBe(true)
  expect(decision.overlay).toMatchObject({ id: 'dm:u1:1', tag: 'dm:u1', kind: 'message', title: 'Ayşe', text: 'selam bak', avatar: { userId: 'u1', name: 'Ayşe' } })
  expect(decision.overlay?.actions).toEqual([{ id: 'open-chat', label: 'Aç', primary: true }])
  expect(decision.overlay?.payload).toEqual({ open: { window: 'chat', target: { kind: 'person', id: 'u1' } } })
})

test('kendi mesajım ve "mesajlar" anahtarı kapalıyken DM geçer', () => {
  expect(routeNotification({ type: 'dm', from: { ...ayse, id: 'me' }, content: 'x' }, ctx()).overlay).toBeNull()
  const off = routeNotification({ type: 'dm', from: ayse, content: 'x' }, ctx({}, { messages: false }))
  expect(off).toEqual({ overlay: null, suppressDesktop: false })
})

test('kanal: susturulmuş kanal hiç; MENTIONS yalnız bahsetmede; ALL her mesajda', () => {
  const base = { type: 'channel-message' as const, projectId: 'p', channelId: 'c', channelName: 'genel', author: ayse, content: 'hey @me' }
  expect(routeNotification({ ...base, mentioned: true, rule: 'MUTED' }, ctx()).overlay).toBeNull()
  expect(routeNotification({ ...base, mentioned: false, rule: 'MENTIONS' }, ctx()).overlay).toBeNull()
  const mention = routeNotification({ ...base, mentioned: true, rule: 'MENTIONS' }, ctx())
  expect(mention.overlay).toMatchObject({ kind: 'mention', title: 'Ayşe · #genel', tag: 'channel:c' })
  expect(mention.overlay?.payload).toEqual({ open: { window: 'chat', target: { kind: 'channel', id: 'c', projectId: 'p' } } })
  expect(routeNotification({ ...base, mentioned: false, rule: 'ALL' }, ctx()).overlay).toMatchObject({ kind: 'message' })
})

test('grup mesajı', () => {
  const decision = routeNotification({ type: 'group-message', groupId: 'g', groupName: 'Sanat', author: ayse, content: 'merhaba' }, ctx())
  expect(decision.overlay).toMatchObject({ kind: 'message', title: 'Ayşe · Sanat', tag: 'group:g', payload: { open: { window: 'chat', target: { kind: 'group', id: 'g' } } } })
})

test('oyuna girdi/çıktı: playToasts ve "play" anahtarına bağlı; çıkışta eylem yok', () => {
  const play = routeNotification({ type: 'play', user: ayse, projectId: 'p', projectName: 'Aurora' }, ctx())
  expect(play.overlay).toMatchObject({ kind: 'play', text: 'Aurora projesini çalıştırdı', tag: 'play:u1', actions: [{ id: 'open-overview', label: 'Özet' }] })
  expect(routeNotification({ type: 'play', user: ayse, projectId: 'p', projectName: 'Aurora' }, ctx({ settings: { overlayNotifications: all, playToasts: false } })).overlay).toBeNull()
  expect(routeNotification({ type: 'play', user: ayse, projectId: 'p', projectName: 'Aurora' }, ctx({}, { play: false })).overlay).toBeNull()
  const stop = routeNotification({ type: 'play-stop', user: ayse, projectName: 'Aurora' }, ctx())
  expect(stop.overlay).toMatchObject({ kind: 'play-stop', text: 'Aurora oyunundan çıktı', duration: 4000 })
  expect(stop.overlay?.actions).toBeUndefined()
})

test('gelen arama: yapışkan kart, Kabul et / Reddet / Sustur; sessiz arama ve kapalı anahtar geçer; bitince kart düşer', () => {
  const incoming = routeNotification({ type: 'call-incoming', callId: 'c1', from: ayse, kind: 'direct', silent: false }, ctx())
  expect(incoming.overlay).toMatchObject({ kind: 'call', sticky: true, tag: 'call:c1', text: 'Seni arıyor', payload: { call: { callId: 'c1' } } })
  expect(incoming.overlay?.actions?.map(action => action.id)).toEqual(['accept', 'decline', 'silence'])
  expect(routeNotification({ type: 'call-incoming', callId: 'c1', from: ayse, kind: 'channel', silent: false }, ctx()).overlay?.text).toBe('Seni sesli odaya çağırıyor')
  expect(routeNotification({ type: 'call-incoming', callId: 'c1', from: ayse, kind: 'direct', silent: true }, ctx()).overlay).toBeNull()
  expect(routeNotification({ type: 'call-incoming', callId: 'c1', from: ayse, kind: 'direct', silent: false }, ctx({}, { calls: false })).overlay).toBeNull()
  expect(routeNotification({ type: 'call-ended', callId: 'c1' }, ctx())).toEqual({ overlay: null, suppressDesktop: false, dismiss: 'call:c1' })
})

test('görev atandı: "Görevi aç" → görev penceresi; aktör bensem geçer', () => {
  const input = { type: 'task-assigned' as const, activityId: 'a', taskId: 't', boardId: 'b', projectId: 'p', preview: 'Menü ekranı', actor: ayse }
  const decision = routeNotification(input, ctx())
  expect(decision.overlay).toMatchObject({ kind: 'task', title: 'Ayşe sana bir görev atadı', text: 'Menü ekranı', tag: 'task:t', payload: { open: { window: 'tasks', target: { taskId: 't', boardId: 'b', projectId: 'p' } } } })
  expect(routeNotification({ ...input, actor: { ...ayse, id: 'me' } }, ctx()).overlay).toBeNull()
  expect(routeNotification(input, ctx({}, { tasks: false })).overlay).toBeNull()
})

test('sürüm yayınlandı / canlı ve indirme bitti', () => {
  expect(routeNotification({ type: 'release', phase: 'published', projectId: 'p', projectName: 'Aurora', version: '1.2.0' }, ctx()).overlay).toMatchObject({ kind: 'release', title: 'Aurora · 1.2.0', text: 'Yeni sürüm yayınlandı.' })
  expect(routeNotification({ type: 'release', phase: 'live', projectId: 'p', version: '1.2.0' }, ctx()).overlay).toMatchObject({ title: 'Kütüphane · 1.2.0', text: 'Sürüm canlıya alındı.' })
  expect(routeNotification({ type: 'release', phase: 'live', projectId: 'p', version: '1' }, ctx({}, { releases: false })).overlay).toBeNull()
  const download = routeNotification({ type: 'download-done', projectId: 'p', projectName: 'Aurora' }, ctx())
  expect(download.overlay).toMatchObject({ kind: 'download', title: 'Aurora', text: 'İndirme tamamlandı.', payload: { open: { window: 'library', target: { projectId: 'p' } } } })
  expect(routeNotification({ type: 'download-done', projectId: 'p' }, ctx({}, { downloads: false })).overlay).toBeNull()
})

test('toast düğmesi planı: open-* → pencere aç, accept/decline → arama, bilinmeyen → hiç', () => {
  expect(planToastAction({ toastId: 1, actionId: 'open-chat', payload: { open: { window: 'chat', target: { kind: 'person', id: 'u1' } } } })).toEqual({ kind: 'open', open: { window: 'chat', target: { kind: 'person', id: 'u1' } } })
  expect(planToastAction({ toastId: 1, actionId: 'accept', payload: { call: { callId: 'c1' } } })).toEqual({ kind: 'call', action: 'accept', callId: 'c1' })
  expect(planToastAction({ toastId: 1, actionId: 'dismiss' })).toEqual({ kind: 'none' })
})
