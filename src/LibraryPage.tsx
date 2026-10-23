import { useCallback, useEffect, useRef, useState } from 'react'
import ContextLayer from './ui/ContextLayer'
import type { Organization } from './App'
type Auth={user:{id:string;username:string;displayName:string;avatarUrl?:string|null};accessToken:string}
import type { PermissionApi } from './permissions'
import type { ProfileMember } from './ProfilePanel'
import MessageRichText from './MessageRichText'
import NotesEditor from './NotesEditor'
import ConfirmDialog from './ConfirmDialog'
import LibraryUploader, { Chevron } from './LibraryUploader'
import ReleaseDiffPanel from './ReleaseDiff'
import ProjectSettingsDialog, { imageSrc, type ProjectSummary } from './ProjectSettingsDialog'
import { channelLabel, formatBytes, formatDuration, formatEta, formatSpeed, phaseLabel, releaseRequest, useLibraryStore, useLibraryUploads, type HubAchievement, type HubData, type HubPost, type ReleaseDiff, type ReleaseList, type ReleaseView } from './library'
import { avatarHue, userInitials } from './initials'
import { useRealtimeEvents } from './realtime'
import { getSettings } from './settings'
import { useEscape } from './useEscape'
import ProjectNotesPanel, { NOTE_CONFLICT_TEXT } from './ProjectNotesPanel'
import ProjectScreenshotsPanel from './ProjectScreenshotsPanel'
import { pushToast } from './Toasts'

type Project = { id: string; name: string; description?: string | null; defaultChannelId?: string | null; coverUrl?: string | null; iconUrl?: string | null }
type Activity = { kind: string; projectId?: string; projectName: string; detail?: string; percent?: number; since?: number } | null
type MenuItem = { label: string; onClick?: () => void; danger?: boolean; disabled?: boolean; checked?: boolean; hint?: string; children?: MenuItem[] } | 'sep'

function timeAgo(value: string | number) {
  const diff = (Date.now() - new Date(value).getTime()) / 1000
  if (diff < 60) return 'az önce'
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} dk önce`
  if (diff < 86400) return `${Math.floor(diff / 3600)} sa önce`
  if (diff < 172800) return 'dün'
  return new Date(value).toLocaleDateString('tr-TR', { day: 'numeric', month: 'long', year: 'numeric' })
}
const ACHIEVEMENT_ICONS = ['🏆', '🥇', '🎯', '🔥', '⭐', '💎', '🚀', '🧪', '🐛', '🎮', '👑', '🛠️', '🧭', '⚡', '🎉', '🧠']

// Kategori rozeti: aynı ad her yerde aynı rengi alır (avatar ton paletinden).
function ChannelBadge({ name }: { name: string }) {
  return <span className={`library-chip-badge ${avatarHue('channel:' + name)}`}><i />{channelLabel(name)}</span>
}
function Avatar({ user, size }: { user: { id: string; displayName: string; avatarUrl?: string | null }; size?: 'lg' }) {
  return <span className={`library-avatar${size ? ' ' + size : ''} ${avatarHue(user.id)}`}>{user.avatarUrl ? <img src={user.avatarUrl} alt="" /> : userInitials(user.displayName)}</span>
}
const Gear = () => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3h0a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8v0a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z" /></svg>

// Sağ tık menüsü (alt menü destekli): proje rayındaki menüyle aynı görsel dil.
// Alt menü (Sürümler) tıklamayla açılır, ekrana sabit konumlanır: sağda yer varsa sağa,
// yoksa sola; uzun listeler kayar. Üst menünün overflow'una takılmaz.
function MenuList({ items, onClose }: { items: MenuItem[]; onClose: () => void }) {
  const [open, setOpen] = useState<{ index: number; left: number; top: number } | null>(null)
  function toggle(index: number, count: number, e: React.MouseEvent<HTMLButtonElement>) {
    if (open?.index === index) { setOpen(null); return }
    const box = e.currentTarget.getBoundingClientRect()
    const width = 236, height = Math.min(window.innerHeight * 0.6, count * 34 + 12)
    const left = box.right + width + 8 <= window.innerWidth ? box.right - 2 : Math.max(4, box.left - width + 2)
    const top = Math.max(4, Math.min(box.top - 6, window.innerHeight - height - 8))
    setOpen({ index, left, top })
  }
  return <>{items.map((item, index) => {
    if (item === 'sep') return <i key={index} className="library-menu-sep" />
    if (item.children) return <div key={index} className="library-menu-parent">
      <button type="button" className={`plain${open?.index === index ? ' open' : ''}`} disabled={item.disabled} onClick={e => toggle(index, item.children!.length, e)} aria-haspopup="menu" aria-expanded={open?.index === index}>{item.label}<em>›</em></button>
      {open?.index === index && <div className="channel-context-menu library-menu library-submenu" style={{ left: open.left, top: open.top }} onMouseDown={e => e.stopPropagation()}><MenuList items={item.children} onClose={onClose} /></div>}
    </div>
    return <button key={index} type="button" className={item.danger ? '' : 'plain'} disabled={item.disabled} onClick={() => { item.onClick?.(); onClose() }}>{item.checked !== undefined && <i className={`library-menu-check${item.checked ? ' on' : ''}`} />}{item.label}{item.hint && <small>{item.hint}</small>}</button>
  })}</>
}
function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  const left = Math.max(4, Math.min(x, window.innerWidth - 250))
  const top = Math.max(4, Math.min(y, window.innerHeight - 12 - items.length * 34))
  return <ContextLayer onMouseDown={onClose} onContextMenu={e => { e.preventDefault(); onClose() }}>
    <div className="channel-context-menu library-menu" style={{ left, top }} onMouseDown={e => e.stopPropagation()}><MenuList items={items} onClose={onClose} /></div>
  </ContextLayer>
}

// Steam benzeri kütüphane: solda tüm projeler (arama + sürüklenebilir genişlik), üstte kapak ve
// yeşil ÇALIŞTIR şeridi, ortada ekip aktivitesi, sağda oynayanlar / başarımlar / incelemeler / görevler.
// Araçlar ve sürüm seçimi sağ tık (ya da çark) menüsünde; sürüm yönetimi yalnız yetkililerde.
export default function LibraryPage({ auth, organization, project: incoming, projects, people, activities, perms, onSelectProject, onOpenSettings, onOpenBoard }: {
  auth: Auth; organization: Organization; project: Project; projects: Project[]; people: ProfileMember[]; activities: Record<string, Activity>; perms: PermissionApi
  onSelectProject: (project: Project) => void; onOpenSettings?: () => void; onOpenBoard?: () => void
}) {
  const store = useLibraryStore()
  const [data, setData] = useState<ReleaseList | null>(null)
  const [hub, setHub] = useState<HubData | null>(null)
  const [channel, setChannel] = useState<string>(() => store.projects[incoming.id]?.channel || '')
  const [uploading, setUploading] = useState(false)
  const [manage, setManage] = useState(false)
  const [editing, setEditing] = useState<ReleaseView | null>(null)
  const [projectSettingsFor, setProjectSettingsFor] = useState<Project | null>(null)
  const [override, setOverride] = useState<Partial<ProjectSummary>>({})
  const [confirm, setConfirm] = useState<{ title: string; body: string; label: string; danger?: boolean; run: () => void } | null>(null)
  const [showInstalls, setShowInstalls] = useState<ReleaseView | null>(null)
  const [menu, setMenu] = useState<{ x: number; y: number; target: Project } | null>(null)
  const [search, setSearch] = useState('')
  const [error, setError] = useState('')
  const [postDraft, setPostDraft] = useState('')
  const [postBusy, setPostBusy] = useState(false)
  const [editingPost, setEditingPost] = useState<HubPost | null>(null)
  const [achievementDialog, setAchievementDialog] = useState(false)
  const [middleTab, setMiddleTab] = useState<'activity' | 'notes' | 'screenshots'>('activity')
  const { uploads, refresh: refreshUploads } = useLibraryUploads()
  // Kaydedilen kapak/ikon, Workspace'in gerçek zamanlı yenilemesi gelene kadar yerelden gösterilir.
  const project: Project = { ...incoming, ...override }
  const root = `/organizations/${organization.id}/projects/${project.id}/releases`
  const hubRoot = `/organizations/${organization.id}/projects/${project.id}/hub`
  const installed = store.projects[project.id]
  const job = store.jobs[project.id]
  const isRunning = store.running.includes(project.id)
  const canPublish = perms.can('release.publish')
  const canEditProject = perms.can('channel.manage')
  const cover = imageSrc(project.coverUrl)
  useEscape(manage && !uploading && !confirm && !showInstalls && !editing, () => setManage(false))
  useEffect(() => setOverride({}), [incoming.coverUrl, incoming.iconUrl])

  // Sol panel genişliği sürüklenerek ayarlanır ve hatırlanır.
  const [side, setSide] = useState(() => { try { return Math.min(440, Math.max(190, Number(localStorage.getItem('p2.library.side')) || 250)) } catch { return 250 } })
  const sideRef = useRef(side); sideRef.current = side
  useEffect(() => { try { localStorage.setItem('p2.library.side', String(side)) } catch {} }, [side])
  function startResize(e: React.MouseEvent) {
    e.preventDefault()
    const startX = e.clientX, start = sideRef.current
    const move = (ev: MouseEvent) => setSide(Math.min(440, Math.max(190, start + ev.clientX - startX)))
    const up = () => { window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); document.body.style.cursor = '' }
    document.body.style.cursor = 'col-resize'
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up)
  }

  const load = useCallback(() => { releaseRequest<ReleaseList>(root, auth.accessToken).then(setData).catch(() => setData(null)) }, [root, auth.accessToken])
  const loadHub = useCallback(() => { releaseRequest<HubData>(hubRoot, auth.accessToken).then(setHub).catch(() => setHub(null)) }, [hubRoot, auth.accessToken])
  useEffect(load, [load])
  useEffect(loadHub, [loadHub])
  const mine = (p: { projectId: string }) => p.projectId === project.id
  useRealtimeEvents(auth.accessToken, organization.id, {
    'release:published': (p: { projectId: string }) => { if (mine(p)) load() },
    'release:live': (p: { projectId: string }) => { if (mine(p)) load() },
    'release:deleted': (p: { projectId: string }) => { if (mine(p)) load() },
    'release:updated': (p: { projectId: string }) => { if (mine(p)) load() },
    'hub:changed': (p: { projectId: string }) => { if (mine(p)) loadHub() },
  })
  // Kendi oturumum kapanınca süre hemen görünsün.
  useEffect(() => window.p2?.library?.onEvent(event => { if (event.type === 'played' && event.projectId === project.id) loadHub() }), [project.id, loadHub])

  const channels = data?.channels || []
  const activeChannel = channels.includes(channel) ? channel : (installed?.channel && channels.includes(installed.channel) ? installed.channel : channels[0] || '')
  const live = activeChannel ? data?.latest[activeChannel] || null : null
  const canUse = Boolean(window.p2?.library)
  const published = (data?.releases || []).filter(r => r.status === 'PUBLISHED').sort((a, b) => new Date(b.publishedAt || b.createdAt).getTime() - new Date(a.publishedAt || a.createdAt).getTime())
  const drafts = (data?.releases || []).filter(r => r.status === 'DRAFT')
  const myUploads = uploads.filter(u => u.projectId === project.id)
  const query = search.trim().toLocaleLowerCase('tr-TR')
  const listed = projects.filter(p => !query || p.name.toLocaleLowerCase('tr-TR').includes(query))
  const playing = people.filter(person => activities[person.id]?.kind === 'playing' && activities[person.id]?.projectId === project.id)
  const updatable = Boolean(installed && live && live.id !== installed.releaseId && !installed.pinned)

  async function chooseRoot() {
    const result = await window.p2?.library?.chooseRoot()
    if (result && !result.ok && result.error) setError(result.error)
  }
  // pinned: elle seçilen eski/başka sürüm canlıya otomatik geçmez; canlı sürüm seçilince sabit kalkar.
  async function install(release: ReleaseView, pinned = false) {
    setError('')
    if (!store.root) { await chooseRoot(); if (!store.root) return }
    setChannel(release.channel); void window.p2?.library?.setChannel({ projectId: project.id, channel: release.channel })
    const result = await window.p2!.library!.install({ organizationId: organization.id, projectId: project.id, projectName: project.name, releaseId: release.id, channel: release.channel, pinned })
    if (!result.ok && result.error) setError(result.error)
  }
  async function launch(target: Project = project) {
    // Overlay: hem kullanıcının ayarı hem kurulu sürümün bayrağı açık olmalı.
    const installedRelease = (data?.releases || []).find(r => r.id === store.projects[target.id]?.releaseId)
    const overlay = getSettings().libraryOverlay && installedRelease?.overlay !== false
    const result = await window.p2!.library!.launch({ organizationId: organization.id, projectId: target.id, projectName: target.name, overlay, iconUrl: imageSrc(target.iconUrl) || undefined })
    if (!result.ok && result.error) setError(result.error)
  }
  async function stop(target: Project = project) {
    const result = await window.p2!.library!.stop({ projectId: target.id })
    if (!result.ok && result.error) setError(result.error)
  }
  async function publish(releaseId: string) { await releaseRequest(`${root}/${releaseId}/publish`, auth.accessToken, { method: 'POST' }); load() }
  async function makeLive(releaseId: string) { await releaseRequest(`${root}/${releaseId}/live`, auth.accessToken, { method: 'POST' }); load() }
  async function remove(release: ReleaseView) { await releaseRequest(`${root}/${release.id}`, auth.accessToken, { method: 'DELETE' }); load() }
  const loadDiff = (release: ReleaseView) => () => releaseRequest<ReleaseDiff>(`${root}/${release.id}/diff`, auth.accessToken)

  // ---- vitrin işlemleri ----
  async function submitPost() {
    if (!postDraft.trim() || postBusy) return
    setPostBusy(true); setError('')
    try { await releaseRequest(`${hubRoot}/posts`, auth.accessToken, { method: 'POST', body: JSON.stringify({ content: postDraft }) }); setPostDraft(''); loadHub() }
    catch (problem: any) { setError(problem?.message || 'Gönderi paylaşılamadı.') }
    finally { setPostBusy(false) }
  }
  async function savePost(post: HubPost, content: string) {
    try { await releaseRequest(`${hubRoot}/posts/${post.id}`, auth.accessToken, { method: 'PATCH', body: JSON.stringify({ content }) }); setEditingPost(null); loadHub() }
    catch (problem: any) { setError(problem?.message || 'Kaydedilemedi.') }
  }
  async function deletePost(post: HubPost) { await releaseRequest(`${hubRoot}/posts/${post.id}`, auth.accessToken, { method: 'DELETE' }).catch(() => {}); loadHub() }
  async function toggleUnlock(item: HubAchievement) { await releaseRequest(`${hubRoot}/achievements/${item.id}/unlock`, auth.accessToken, { method: 'POST', body: JSON.stringify({ unlocked: !item.unlockedByMe }) }).catch(() => {}); loadHub() }
  async function deleteAchievement(item: HubAchievement) { await releaseRequest(`${hubRoot}/achievements/${item.id}`, auth.accessToken, { method: 'DELETE' }).catch(() => {}); loadHub() }
  async function saveReview(recommended: boolean, body: string) { await releaseRequest(`${hubRoot}/reviews`, auth.accessToken, { method: 'PUT', body: JSON.stringify({ recommended, body }) }); loadHub() }
  async function deleteReview() { await releaseRequest(`${hubRoot}/reviews`, auth.accessToken, { method: 'DELETE' }).catch(() => {}); loadHub() }

  // Sağ tık menüsü (listedeki proje, kapak, çark ya da ÇALIŞTIR yanındaki ok). Araçlar ve sürümler burada.
  function menuFor(target: Project): MenuItem[] {
    const inst = store.projects[target.id]
    const running = store.running.includes(target.id)
    const busy = Boolean(store.jobs[target.id]?.active)
    const isCurrent = target.id === project.id
    const items: MenuItem[] = []
    if (!isCurrent) items.push({ label: 'Sayfasını aç', onClick: () => onSelectProject(target) })
    if (canUse && inst) items.push(running ? { label: 'Durdur', onClick: () => void stop(target) } : { label: 'Çalıştır', disabled: busy, onClick: () => void launch(target) })
    if (canUse && isCurrent && !inst && live) items.push({ label: `Yükle · ${formatBytes(live.totalSize)}`, disabled: busy, onClick: () => void install(live) })
    if (canUse && isCurrent && updatable) items.push({ label: `Güncelle · ${live!.version}`, disabled: busy, onClick: () => void install(live!) })
    if (canUse && isCurrent && inst?.pinned && live && live.id !== inst.releaseId) items.push({ label: `Canlı sürüme dön · ${live.version}`, disabled: busy, onClick: () => void install(live) })
    if (canUse && isCurrent && published.length) {
      // Sürümler: kategoriye göre; canlı olan işaretli, kurulu olan tikli. Eski sürüme buradan dönülür.
      const groups = [...new Set(published.map(r => r.channel))]
      const children: MenuItem[] = []
      for (const name of groups) {
        if (groups.length > 1) children.push({ label: channelLabel(name), disabled: true })
        for (const release of published.filter(r => r.channel === name)) {
          const isLive = data?.latest[name]?.id === release.id
          children.push({ label: release.version, checked: inst?.releaseId === release.id, hint: isLive ? 'canlı' : undefined, disabled: busy || running, onClick: () => { if (inst?.releaseId !== release.id) setConfirm({ title: isLive ? 'Canlı sürüme geç' : 'Bu sürüme geç', body: isLive ? `${release.version} kurulacak; sonraki güncellemeler otomatik gelir.` : `${release.version} kurulacak ve bu sürümde sabit kalınır; canlı sürüm değişse de otomatik güncellenmez.`, label: 'Kur', run: () => void install(release, !isLive) }) } })
        }
      }
      items.push({ label: 'Sürümler', children })
    }
    if (canUse && inst) items.push('sep',
      { label: 'Klasörü aç', onClick: () => void window.p2?.library?.openFolder({ projectId: target.id, projectName: target.name }) },
      { label: 'Dosyaları doğrula ve onar', disabled: busy || running, onClick: () => setConfirm({ title: 'Dosyaları doğrula', body: 'Kurulu dosyalar denetlenip bozuk parçalar yeniden indirilecek.', label: 'Doğrula', run: () => void window.p2?.library?.verify({ organizationId: organization.id, projectId: target.id, projectName: target.name }) }) },
      { label: 'Masaüstü kısayolu oluştur', onClick: () => void window.p2?.library?.createShortcut({ projectId: target.id, projectName: target.name, iconUrl: imageSrc(target.iconUrl) || undefined }) })
    if ((canPublish && isCurrent) || canEditProject) items.push('sep')
    if (canPublish && isCurrent) items.push({ label: 'Sürüm yönetimi…', onClick: () => setManage(true) })
    if (canEditProject) items.push({ label: 'Proje ayarları…', onClick: () => setProjectSettingsFor(target) })
    if (canUse && inst) items.push('sep', { label: 'Kaldır', danger: true, disabled: busy || running, onClick: () => setConfirm({ title: 'Oyunu kaldır', body: `${target.name} diskten silinecek.`, label: 'Kaldır', danger: true, run: () => void window.p2?.library?.uninstall({ organizationId: organization.id, projectId: target.id, projectName: target.name }) }) })
    return items
  }
  const openMenu = (target: Project) => (e: React.MouseEvent) => { e.preventDefault(); e.stopPropagation(); setMenu({ x: e.clientX, y: e.clientY, target }) }
  const openMenuBelow = (target: Project) => (e: React.MouseEvent) => { e.preventDefault(); e.stopPropagation(); const box = e.currentTarget.getBoundingClientRect(); setMenu({ x: box.left, y: box.bottom + 6, target }) }

  // Eylem çubuğundaki büyük düğme (Steam yeşili; ok ile menü).
  function primary() {
    if (!canUse) return <p className="library-hint">Kütüphane yalnız P2 masaüstü uygulamasında kullanılabilir.</p>
    if (job?.phase === 'failed') return <div className="library-action-row"><button className="library-secondary" onClick={() => live && install(live)}>Yeniden dene</button><span className="library-error-inline">{job.error}</span></div>
    if (!channels.length) return <p className="library-hint">Henüz yayınlanmış sürüm yok.</p>
    const split = (label: string, tone: 'install' | 'play' | 'stop', onClick: () => void, disabled?: boolean) => <div className={`library-play ${tone}`}>
      <button type="button" className="library-play-main" onClick={onClick} disabled={disabled}>
        {tone === 'stop' ? <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" /></svg> : tone === 'install' ? <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5M5 20h14" /></svg> : <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12-7.5z" /></svg>}
        <span>{label}</span>
      </button>
      <button type="button" className="library-play-caret" onClick={openMenuBelow(project)} aria-label="Diğer işlemler" aria-haspopup="menu"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg></button>
    </div>
    if (job?.active) return split(phaseLabel[job.phase] || 'İndiriliyor', 'install', () => {}, true)
    if (!installed) return live ? split('Yükle', 'install', () => void install(live)) : <p className="library-hint">Bu kategoride yayınlanmış sürüm yok.</p>
    if (isRunning) return split('Durdur', 'stop', () => void stop())
    return <div className="library-action-row">
      {split('Çalıştır', 'play', () => void launch())}
      {updatable && <button className="library-secondary" onClick={() => void install(live!)}>Güncelle · {live!.version}</button>}
    </div>
  }

  const pct = job?.bytesTotal ? Math.min(100, Math.round((job.bytesDone / job.bytesTotal) * 100)) : job?.total ? Math.round((job.done / job.total) * 100) : 0
  const playSeconds = Math.max(hub?.playtime.mine || 0, installed?.playSeconds || 0)
  const lastPlayed = installed?.lastPlayedAt || (hub?.playtime.lastPlayedAt ? new Date(hub.playtime.lastPlayedAt).getTime() : 0)
  const positive = hub ? hub.reviews.filter(r => r.recommended).length : 0
  const myReview = hub?.reviews.find(r => r.userId === auth.user.id) || null

  return <div className="library-page" style={{ gridTemplateColumns: `${side}px minmax(0, 1fr)` }}>
    <aside className="library-side">
      <header className="library-side-head">
        <b>Kütüphane</b>
        {onOpenSettings && <button type="button" className="library-gear" onClick={onOpenSettings} aria-label="Kütüphane ayarları" title="Kütüphane ayarları: konum, bant genişliği, otomatik indirme, gizlilik"><Gear /></button>}
      </header>
      <label className="library-search"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" /></svg><input value={search} onChange={e => setSearch(e.target.value)} placeholder="Ara" aria-label="Projelerde ara" /></label>
      <div className="library-games">
        {listed.map(p => { const inst = store.projects[p.id]; const running = store.running.includes(p.id); const j = store.jobs[p.id]; const busy = j?.active; const rowPct = j?.bytesTotal ? Math.round((j.bytesDone / j.bytesTotal) * 100) : 0; return <button key={p.id} type="button" className={`library-game${p.id === project.id ? ' active' : ''}${inst ? ' installed' : ''}${running ? ' running' : ''}`} onClick={() => { if (p.id !== project.id) onSelectProject(p) }} onContextMenu={openMenu(p)} title={p.name}>
          <span className="library-game-icon">{p.iconUrl ? <img src={imageSrc(p.iconUrl)} alt="" /> : userInitials(p.name)}</span>
          <span className="library-game-name">{p.name}</span>
          {busy ? <em className="library-game-pct" title={phaseLabel[j.phase]}>{j.phase === 'downloading' || j.phase === 'preparing' ? `%${rowPct}` : j.phase === 'installing' ? 'kuruluyor' : '…'}</em> : running ? <i className="library-game-state running" title="Çalışıyor" /> : inst ? <i className="library-game-state" title={`Kurulu · ${inst.version}`} /> : null}
        </button> })}
        {!listed.length && <p className="library-hint library-games-empty">{query ? 'Eşleşen proje yok.' : 'Henüz proje yok.'}</p>}
      </div>
      <div className="library-resizer" onMouseDown={startResize} role="separator" aria-orientation="vertical" aria-label="Panel genişliği" />
    </aside>

    <main className="library-main">
      <div className="library-content">
        {/* Kapak yalnız üstte; alta doğru üç kademede tema rengine iner. */}
        <div className={`library-hero${cover ? '' : ' no-cover'}`} style={cover ? { backgroundImage: `url(${cover})` } : undefined} onContextMenu={openMenu(project)}>
          <div className="library-hero-fade" aria-hidden="true" />
          {!cover && <div className="library-cover-fallback" aria-hidden="true">{userInitials(project.name)}</div>}
          <div className="library-hero-body">
            <span className="library-hero-icon">{project.iconUrl ? <img src={imageSrc(project.iconUrl)} alt="" /> : userInitials(project.name)}</span>
            <div><h1>{project.name}</h1>{playing.length > 0 && <p className="library-hero-sub"><i />{playing.length === 1 ? `${playing[0].displayName} şu an oynuyor` : `${playing.length} kişi şu an oynuyor`}</p>}</div>
          </div>
        </div>

        <div className={`library-actionbar${job?.active ? ' busy' : ''}`}>
          {job?.active ? <DownloadPanel job={job} pct={pct} projectId={project.id} /> : <>
            <div className="library-actionbar-primary">{primary()}</div>
            <div className="library-stats">
              <div className="library-stat"><small>Oynama süresi</small><b>{formatDuration(playSeconds)}</b></div>
              <div className="library-stat"><small>Son çalıştırma</small><b>{lastPlayed ? timeAgo(lastPlayed) : '—'}</b></div>
              <div className="library-stat"><small>Gerekli alan</small><b>{live ? formatBytes(live.totalSize) : '—'}</b></div>
              <div className="library-stat"><small>Kurulu sürüm</small><b>{installed ? installed.version : '—'}{installed?.pinned && <em title="Elle seçildi; otomatik güncellenmez"> · sabit</em>}</b></div>
              <div className="library-stat"><small>Son güncelleme</small><b>{live?.publishedAt ? timeAgo(live.liveAt || live.publishedAt) : '—'}</b></div>
              <div className="library-stat"><small>Kategori</small><b>{activeChannel ? <ChannelBadge name={activeChannel} /> : '—'}</b></div>
            </div>
          </>}
          <div className="library-actionbar-tools">
            {canEditProject && <button type="button" className="library-icon-btn" onClick={() => setProjectSettingsFor(project)} title="Proje ayarları: kapak, ikon" aria-label="Proje ayarları"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 013 3L7 19l-4 1 1-4z" /></svg></button>}
            <button type="button" className="library-icon-btn" onClick={openMenuBelow(project)} title="Diğer işlemler ve sürümler" aria-label="Diğer işlemler" aria-haspopup="menu"><Gear /></button>
          </div>
        </div>

        {!store.root && canUse && <div className="library-root-notice"><b>Önce kütüphane konumunu seç</b><p>Oyunlar seçtiğin diskte <code>P2ProjectLibrary</code> klasörüne kurulur. Sonradan Ayarlar &gt; Kütüphane'den değiştirebilirsin.</p><button className="library-secondary" onClick={chooseRoot}>Konum seç</button></div>}
        {error && <div className="library-error">{error}<button type="button" onClick={() => setError('')} aria-label="Kapat">×</button></div>}

        <div className="library-columns">
          <div className="library-col-main">
            <section className={`library-panel library-section library-activity${middleTab === 'notes' ? ' library-notes-tab' : middleTab === 'screenshots' ? ' library-screenshots-tab' : ''}`}>
              <div className="library-tabs-head">
                <h3>{middleTab === 'notes' ? 'Notlar' : middleTab === 'screenshots' ? 'Ekran görüntüleri' : 'Aktivite'}</h3>
                <div className="p2-tabs segment" role="tablist" aria-label="Aktivite, notlar veya ekran görüntüleri">
                  <button type="button" role="tab" className={`p2-tab${middleTab === 'activity' ? ' active' : ''}`} aria-selected={middleTab === 'activity'} onClick={() => setMiddleTab('activity')}>Aktivite</button>
                  <button type="button" role="tab" className={`p2-tab${middleTab === 'notes' ? ' active' : ''}`} aria-selected={middleTab === 'notes'} onClick={() => setMiddleTab('notes')}>Notlar</button>
                  <button type="button" role="tab" className={`p2-tab${middleTab === 'screenshots' ? ' active' : ''}`} aria-selected={middleTab === 'screenshots'} onClick={() => setMiddleTab('screenshots')}>Ekran görüntüleri</button>
                </div>
              </div>
              {middleTab === 'screenshots' ? <ProjectScreenshotsPanel token={auth.accessToken} organizationId={organization.id} projectId={project.id} me={auth.user.id} canManage={Boolean(hub?.permissions.manage)} /> : middleTab === 'notes' ? <ProjectNotesPanel token={auth.accessToken} organizationId={organization.id} projectId={project.id} me={auth.user.id} canManage={Boolean(hub?.permissions.manage)} onConflict={() => pushToast({ text: NOTE_CONFLICT_TEXT, tone: 'info' })} /> : <>
              {hub?.permissions.post && !editingPost && <div className="library-post-composer"><Avatar user={auth.user} /><NotesEditor token={auth.accessToken} value={postDraft} onChange={setPostDraft} placeholder={`${project.name} için ekibe bir şey yaz…`} onSubmit={() => void submitPost()} submitLabel="Paylaş" busy={postBusy} compact /></div>}
              {!hub && <p className="library-hint">Aktivite yükleniyor…</p>}
              {hub && !hub.posts.length && <p className="library-hint library-activity-empty">{hub.permissions.post ? 'Henüz aktivite yok. İlk gönderiyi sen yaz: ne üzerinde çalışıyorsunuz, ne bitti, ne geliyor?' : 'Henüz aktivite yok.'}</p>}
              {hub?.posts.map(post => <article key={post.id} className="library-post">
                <Avatar user={post.author} />
                <div className="library-post-body">
                  <header><b>{post.author.displayName}</b>{post.kind === 'RELEASE' && <span className="library-badge live">Sürüm</span>}<small>{timeAgo(post.createdAt)}{post.updatedAt !== post.createdAt && ' · düzenlendi'}</small>
                    {(post.authorId === auth.user.id || hub.permissions.manage) && editingPost?.id !== post.id && <span className="library-post-tools"><button type="button" onClick={() => setEditingPost(post)}>Düzenle</button><button type="button" className="danger" onClick={() => setConfirm({ title: 'Gönderiyi sil', body: 'Bu aktivite silinecek.', label: 'Sil', danger: true, run: () => void deletePost(post) })}>Sil</button></span>}
                  </header>
                  {editingPost?.id === post.id ? <PostEditor token={auth.accessToken} post={post} onCancel={() => setEditingPost(null)} onSave={content => void savePost(post, content)} /> : <div className="library-post-content"><MessageRichText content={post.content} /></div>}
                </div>
              </article>)}
              </>}
            </section>
          </div>

          <div className="library-col-side">
            <section className="library-panel library-section">
              <h3>Oynayan arkadaşlar{playing.length > 0 && <em className="library-count">{playing.length}</em>}</h3>
              {!playing.length && <p className="library-hint">Şu an kimse çalıştırmıyor.</p>}
              {playing.length > 0 && <div className="library-players">{playing.map(person => <div key={person.id} className="library-player"><Avatar user={person} /><div><b>{person.displayName}</b><small>{activities[person.id]?.since ? `${formatDuration(Math.max(60, (Date.now() - activities[person.id]!.since!) / 1000))} oynuyor` : 'oynuyor'}</small></div><i className="library-player-dot" /></div>)}</div>}
            </section>

            <section className="library-panel library-section">
              <h3>Başarımlar{hub && hub.achievements.length > 0 && <em className="library-count">{hub.achievements.filter(a => a.unlockedByMe).length}/{hub.achievements.length}</em>}{hub?.permissions.manage && <button type="button" className="library-h3-action" onClick={() => setAchievementDialog(true)}>+ Ekle</button>}</h3>
              {hub && !hub.achievements.length && <p className="library-hint">{hub.permissions.manage ? 'Henüz başarım yok. Ekibe hedef koy: "İlk build", "100 görev", "Sıfır çökme"…' : 'Henüz başarım tanımlanmadı.'}</p>}
              {hub && hub.achievements.length > 0 && <div className="library-achievements">{hub.achievements.map(item => <button type="button" key={item.id} className={`library-achievement${item.unlockedByMe ? ' unlocked' : ''}`} onClick={() => void toggleUnlock(item)} onContextMenu={hub.permissions.manage ? e => { e.preventDefault(); setConfirm({ title: 'Başarımı sil', body: `"${item.title}" silinecek.`, label: 'Sil', danger: true, run: () => void deleteAchievement(item) }) } : undefined} title={`${item.title}${item.description ? ` — ${item.description}` : ''}\n${item.unlocks.length ? item.unlocks.map(u => u.user.displayName).join(', ') + ' kazandı' : 'Henüz kimse kazanmadı'}\n${item.unlockedByMe ? 'Tıkla: kazanımı geri al' : 'Tıkla: kazandım'}`}>
                <span className="library-achievement-icon">{item.icon}</span>
                <span className="library-achievement-text"><b>{item.title}</b><small>{item.unlocks.length ? `${item.unlocks.length} kişi kazandı` : 'Kimse kazanmadı'}</small></span>
                {item.unlockedByMe && <i className="library-achievement-check">✓</i>}
              </button>)}</div>}
            </section>

            <section className="library-panel library-section">
              <h3>İncelemeler{hub && hub.reviews.length > 0 && <em className={`library-count ${positive / hub.reviews.length >= .7 ? 'good' : positive / hub.reviews.length >= .4 ? 'mixed' : 'bad'}`}>%{Math.round((positive / hub.reviews.length) * 100)} olumlu</em>}</h3>
              {hub && <ReviewForm key={myReview?.id || 'new'} mine={myReview} onSave={saveReview} onDelete={deleteReview} />}
              {hub && hub.reviews.filter(r => r.userId !== auth.user.id).map(review => <div key={review.id} className="library-review"><Avatar user={review.user} /><div><header><b>{review.user.displayName}</b><i className={review.recommended ? 'up' : 'down'}>{review.recommended ? '👍 Tavsiye ediyor' : '👎 Tavsiye etmiyor'}</i><small>{timeAgo(review.updatedAt)}</small></header>{review.body && <p>{review.body}</p>}</div></div>)}
              {hub && !hub.reviews.length && <p className="library-hint">Henüz inceleme yok. İlk yorumu sen yaz.</p>}
            </section>

            <ProjectTasks root={`/organizations/${organization.id}/projects/${project.id}/boards`} token={auth.accessToken} onOpenBoard={onOpenBoard} />
          </div>
        </div>
      </div>
    </main>

    {menu && <ContextMenu x={menu.x} y={menu.y} items={menuFor(menu.target)} onClose={() => setMenu(null)} />}

    {manage && canPublish && <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) setManage(false) }}>
      <section className="library-manage-modal">
        <header><div><p className="eyebrow">SÜRÜM YÖNETİMİ</p><h2>{project.name}</h2></div><button type="button" onClick={() => setManage(false)} aria-label="Kapat">×</button></header>
        <div className="library-manage-body">
          <button type="button" className="library-new-release" onClick={() => setUploading(true)}>
            <span className="library-new-release-ico"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg></span>
            <span><b>Yeni sürüm yükle</b><small>Build klasörünü seç, kategori ve sürüm numarası ver, taslak olarak yükle; hazır olunca yayınla ve canlıya al.</small></span>
          </button>

          {myUploads.length > 0 && <section className="library-section"><h3>Devam eden yüklemeler</h3>
            {myUploads.map(u => <div key={u.releaseId} className="library-release-card draft">
              <div className="library-release-head">
                <div className="library-release-title"><b>{u.version}</b> <ChannelBadge name={u.channel} /><small>{u.total - u.pending}/{u.total} parça yüklendi</small></div>
                <div className="library-card-actions"><button onClick={() => { void window.p2?.library?.uploadResume({ requestId: crypto.randomUUID(), releaseId: u.releaseId }); refreshUploads() }}>Devam et</button><button className="library-danger-text" onClick={() => { void window.p2?.library?.uploadCancel({ requestId: u.releaseId, releaseId: u.releaseId, organizationId: organization.id, projectId: project.id, discard: true }); refreshUploads() }}>Vazgeç</button></div>
              </div>
            </div>)}
          </section>}

          <section className="library-section"><h3>Taslaklar</h3>
            {!drafts.length && <p className="library-hint">Taslak yok. Yüklenen sürümler yayınlanana kadar burada bekler.</p>}
            {drafts.map(release => <ReleaseCard key={release.id} release={release} draft loadDiff={loadDiff(release)} onEdit={() => setEditing(release)} onPublish={() => setConfirm({ title: 'Sürümü yayınla', body: `${release.version} (${channelLabel(release.channel)}) ekibin "Sürümler" menüsünde görünür. Kategorinin canlı sürümü yoksa canlıya da alınır.`, label: 'Yayınla', run: () => void publish(release.id) })} onDelete={() => setConfirm({ title: 'Taslağı sil', body: `${release.version} silinecek.`, label: 'Sil', danger: true, run: () => void remove(release) })} onInstalls={() => setShowInstalls(release)} />)}
          </section>

          <section className="library-section"><h3>Yayınlanmış sürümler</h3>
            <p className="library-hint library-manage-hint">Canlı sürüm herkese gider; kurulu oyunlar ona güncellenir. Eski bir sürümü canlıya alarak geri dönebilirsin.</p>
            {!published.length && <p className="library-hint">Henüz yayın yok.</p>}
            {published.map(release => <ReleaseCard key={release.id} release={release} live={data?.latest[release.channel]?.id === release.id} installedId={installed?.releaseId} loadDiff={loadDiff(release)} onLive={() => setConfirm({ title: 'Canlıya al', body: `${release.version} (${channelLabel(release.channel)}) canlı sürüm olacak; bu kategoriyi kurmuş herkes bu sürüme güncellenir.`, label: 'Canlıya al', run: () => void makeLive(release.id) })} onEdit={() => setEditing(release)} onDelete={() => setConfirm({ title: 'Sürümü sil', body: `${release.version} silinecek.`, label: 'Sil', danger: true, run: () => void remove(release) })} onInstalls={() => setShowInstalls(release)} />)}
          </section>
        </div>
      </section>
    </div>}

    {uploading && data && <LibraryUploader auth={auth} organization={organization} project={project} latest={data.latest} channels={channels} defaultChannel={activeChannel} onClose={() => { setUploading(false); refreshUploads() }} onDone={async (releaseId, pub) => { if (pub) await publish(releaseId); load() }} />}
    {editing && <EditReleaseDialog root={root} token={auth.accessToken} release={editing} channels={channels} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); load(); refreshUploads() }} />}
    {achievementDialog && <AchievementDialog root={hubRoot} token={auth.accessToken} onClose={() => setAchievementDialog(false)} onSaved={() => { setAchievementDialog(false); loadHub() }} />}
    {projectSettingsFor && <ProjectSettingsDialog token={auth.accessToken} organizationId={organization.id} project={projectSettingsFor} onClose={() => setProjectSettingsFor(null)} onSaved={saved => { if (saved.id === project.id) setOverride({ coverUrl: saved.coverUrl, iconUrl: saved.iconUrl }); setProjectSettingsFor(null) }} />}
    {confirm && <ConfirmDialog title={confirm.title} body={confirm.body} confirmLabel={confirm.label} danger={confirm.danger} onCancel={() => setConfirm(null)} onConfirm={() => { confirm.run(); setConfirm(null) }} />}
    {showInstalls && <InstallsDialog root={root} release={showInstalls} token={auth.accessToken} onClose={() => setShowInstalls(null)} />}
  </div>
}

// ---- İndirme paneli: yüzde, indirme/disk hızı, kalan süre, duraklat/devam/iptal ----
function DownloadPanel({ job, pct, projectId }: { job: LibraryJob; pct: number; projectId: string }) {
  const downloading = job.phase === 'downloading'
  const label = job.paused ? 'Duraklatıldı' : phaseLabel[job.phase] || 'İndiriliyor'
  const writePct = job.phase === 'installing' && job.bytesToWrite ? Math.min(100, Math.round(((job.bytesWritten || 0) / job.bytesToWrite) * 100)) : null
  const shown = writePct ?? pct
  return <div className={`library-download${job.paused ? ' paused' : ''}`}>
    <div className="library-download-head">
      <div className="library-download-title"><b>{label}</b><small>{downloading ? `${formatBytes(job.bytesDone)} / ${formatBytes(job.bytesTotal)}` : job.phase === 'installing' ? `${formatBytes(job.bytesWritten || 0)} / ${formatBytes(job.bytesToWrite || 0)} yazıldı` : ''}</small></div>
      <div className="library-download-pct">%{shown}</div>
    </div>
    <div className="library-progress-bar big"><i style={{ width: `${shown}%` }} /><s /></div>
    <div className="library-download-foot">
      <div className="library-download-meters">
        <span className="library-meter"><small>İndirme</small><b>{downloading && !job.paused ? formatSpeed(job.speed || 0) : '—'}</b></span>
        <span className="library-meter"><small>Disk yazma</small><b>{(job.diskSpeed || 0) > 0 ? formatSpeed(job.diskSpeed || 0) : '—'}</b></span>
        <span className="library-meter"><small>Kalan</small><b>{downloading && !job.paused && job.eta ? formatEta(job.eta) : '—'}</b></span>
        <span className="library-meter"><small>Parça</small><b>{job.total ? `${job.done}/${job.total}` : '—'}</b></span>
      </div>
      <div className="library-download-actions">
        {downloading && (job.paused
          ? <button type="button" className="library-secondary accent" onClick={() => window.p2?.library?.resume({ projectId })}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l12-7.5z" /></svg>Devam et</button>
          : <button type="button" className="library-secondary" onClick={() => window.p2?.library?.pause({ projectId })}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>Duraklat</button>)}
        <button type="button" className="library-secondary danger" onClick={() => window.p2?.library?.cancel({ projectId })}>İptal</button>
      </div>
    </div>
  </div>
}

function PostEditor({ token, post, onCancel, onSave }: { token: string; post: HubPost; onCancel: () => void; onSave: (content: string) => void }) {
  const [value, setValue] = useState(post.content)
  return <div className="library-post-edit"><NotesEditor token={token} value={value} onChange={setValue} placeholder="Gönderiyi düzenle" onSubmit={() => { if (value.trim()) onSave(value) }} submitLabel="Kaydet" compact autoFocus /><button type="button" className="library-link" onClick={onCancel}>Vazgeç</button></div>
}

// Kendi incelemem: tavsiye ederim / etmem + kısa metin.
function ReviewForm({ mine, onSave, onDelete }: { mine: { recommended: boolean; body: string | null } | null; onSave: (recommended: boolean, body: string) => Promise<void>; onDelete: () => Promise<void> }) {
  const [open, setOpen] = useState(!mine)
  const [recommended, setRecommended] = useState(mine?.recommended ?? true)
  const [body, setBody] = useState(mine?.body || '')
  const [busy, setBusy] = useState(false)
  if (mine && !open) return <div className="library-review mine"><div><header><b>Senin incelemen</b><i className={mine.recommended ? 'up' : 'down'}>{mine.recommended ? '👍 Tavsiye ediyorsun' : '👎 Tavsiye etmiyorsun'}</i></header>{mine.body && <p>{mine.body}</p>}<span className="library-post-tools"><button type="button" onClick={() => setOpen(true)}>Düzenle</button><button type="button" className="danger" onClick={() => void onDelete()}>Sil</button></span></div></div>
  return <form className="library-review-form" onSubmit={async e => { e.preventDefault(); setBusy(true); try { await onSave(recommended, body); setOpen(false) } finally { setBusy(false) } }}>
    <div className="library-review-choice" role="radiogroup" aria-label="Tavsiye">
      <button type="button" role="radio" aria-checked={recommended} className={recommended ? 'active up' : ''} onClick={() => setRecommended(true)}>👍 Tavsiye ederim</button>
      <button type="button" role="radio" aria-checked={!recommended} className={!recommended ? 'active down' : ''} onClick={() => setRecommended(false)}>👎 Etmem</button>
    </div>
    <textarea value={body} onChange={e => setBody(e.target.value.slice(0, 2000))} rows={3} placeholder="Neyi beğendin, ne eksik? (isteğe bağlı)" />
    <footer>{mine && <button type="button" onClick={() => setOpen(false)}>Vazgeç</button>}<button type="submit" className="library-secondary accent" disabled={busy}>{busy ? 'Kaydediliyor…' : mine ? 'Güncelle' : 'İncelemeyi yayınla'}</button></footer>
  </form>
}

function AchievementDialog({ root, token, onClose, onSaved }: { root: string; token: string; onClose: () => void; onSaved: () => void }) {
  useEscape(true, onClose)
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [icon, setIcon] = useState('🏆')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  async function save() {
    if (!title.trim()) return
    setBusy(true); setError('')
    try { await releaseRequest(`${root}/achievements`, token, { method: 'POST', body: JSON.stringify({ title: title.trim(), description: description.trim(), icon }) }); onSaved() }
    catch (problem: any) { setError(problem?.message || 'Kaydedilemedi.'); setBusy(false) }
  }
  return <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
    <section className="library-upload-modal library-achievement-modal">
      <header><div><p className="eyebrow">YENİ BAŞARIM</p><h2>Ekibe bir hedef koy</h2></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="library-upload-body">
        <div className="library-field"><span>Simge</span><div className="library-icon-choices">{ACHIEVEMENT_ICONS.map(item => <button type="button" key={item} className={icon === item ? 'active' : ''} onClick={() => setIcon(item)}>{item}</button>)}<input value={icon} onChange={e => setIcon(e.target.value.slice(0, 8))} aria-label="Özel emoji" /></div></div>
        <label className="library-field"><span>Ad</span><input value={title} onChange={e => setTitle(e.target.value)} maxLength={60} placeholder="ör. İlk build'i yükledi" autoFocus /></label>
        <label className="library-field"><span>Açıklama (isteğe bağlı)</span><input value={description} onChange={e => setDescription(e.target.value)} maxLength={240} placeholder="Nasıl kazanılır?" /></label>
        <p className="library-hint">Üyeler başarıma tıklayarak "kazandım" der; sağ tıkla silinir.</p>
        {error && <div className="library-error">{error}</div>}
        <footer><button type="button" onClick={onClose}>Vazgeç</button><button type="button" className="library-primary" disabled={!title.trim() || busy} onClick={save}>{busy ? 'Kaydediliyor…' : 'Ekle'}</button></footer>
      </div>
    </section>
  </div>
}

// ---- Görevler: panolardaki açık işler (sağ sütun) ----
type TaskRow = { id: string; number: number; title: string; status: string; priority: string; dueAt: string | null; boardId: string }
const STATUS_LABEL: Record<string, string> = { BACKLOG: 'Birikim', TODO: 'Yapılacak', IN_PROGRESS: 'Sürüyor', REVIEW: 'İncelemede', DONE: 'Bitti' }
const STATUS_ORDER = ['IN_PROGRESS', 'REVIEW', 'TODO', 'BACKLOG']
const PRIORITY_LABEL: Record<string, string> = { URGENT: 'Acil', HIGH: 'Yüksek' }
const PRIORITY_ORDER = ['URGENT', 'HIGH', 'MEDIUM', 'LOW']

function ProjectTasks({ root, token, onOpenBoard }: { root: string; token: string; onOpenBoard?: () => void }) {
  const [state, setState] = useState<{ loading: boolean; boards: Array<{ id: string; name: string; tasks: TaskRow[] }> }>({ loading: true, boards: [] })
  useEffect(() => {
    let alive = true
    void (async () => {
      try {
        const boards = await releaseRequest<Array<{ id: string; name: string }>>(root, token)
        const full = await Promise.all(boards.map(async board => { const content = await releaseRequest<{ tasks: TaskRow[] }>(`${root}/${board.id}`, token); return { id: board.id, name: board.name, tasks: content.tasks } }))
        if (alive) setState({ loading: false, boards: full })
      } catch { if (alive) setState({ loading: false, boards: [] }) }
    })()
    return () => { alive = false }
  }, [root, token])
  const all = state.boards.flatMap(board => board.tasks.map(task => ({ ...task, boardName: board.name })))
  const open = all.filter(task => task.status !== 'DONE').sort((a, b) => (STATUS_ORDER.indexOf(a.status) - STATUS_ORDER.indexOf(b.status)) || (PRIORITY_ORDER.indexOf(a.priority) - PRIORITY_ORDER.indexOf(b.priority)) || a.number - b.number)
  const done = all.length - open.length
  const shown = open.slice(0, 8)
  const multiBoard = state.boards.length > 1
  return <section className="library-panel library-section"><h3>Görevler{onOpenBoard && <button type="button" className="library-h3-action" onClick={onOpenBoard}>Panoyu aç</button>}</h3>
    {state.loading && <p className="library-hint">Görevler yükleniyor…</p>}
    {!state.loading && !all.length && <p className="library-hint">Bu projede henüz görev yok.</p>}
    {!state.loading && all.length > 0 && !open.length && <p className="library-hint">Açık görev yok; {done} görev tamamlandı.</p>}
    {shown.length > 0 && <div className="library-tasks">
      {shown.map(task => <button type="button" key={task.id} className="library-task" onClick={onOpenBoard} title={multiBoard ? task.boardName : undefined}>
        <em>#{task.number}</em>
        <b>{task.title}</b>
        <span className="library-task-meta">
          {PRIORITY_LABEL[task.priority] && <i className={`library-task-chip ${task.priority}`}>{PRIORITY_LABEL[task.priority]}</i>}
          <i className={`library-task-chip ${task.status}`}>{STATUS_LABEL[task.status] || task.status}</i>
        </span>
      </button>)}
    </div>}
    {!state.loading && all.length > 0 && <div className="library-tasks-foot"><span>{open.length} açık · {done} tamamlandı</span>{open.length > shown.length && onOpenBoard && <button type="button" onClick={onOpenBoard}>+{open.length - shown.length} daha</button>}</div>}
  </section>
}

function ReleaseCard({ release, draft, live, installedId, loadDiff, onLive, onPublish, onDelete, onEdit, onInstalls }: {
  release: ReleaseView; draft?: boolean; live?: boolean; installedId?: string; loadDiff: () => Promise<ReleaseDiff>
  onLive?: () => void; onPublish?: () => void; onDelete?: () => void; onEdit?: () => void; onInstalls?: () => void
}) {
  const [open, setOpen] = useState(false)
  const [diff, setDiff] = useState<{ loading: boolean; data: ReleaseDiff | null } | null>(null)
  const isInstalled = installedId === release.id
  function toggleDiff() {
    if (diff) { setDiff(null); return }
    setDiff({ loading: true, data: null })
    loadDiff().then(data => setDiff({ loading: false, data })).catch(() => setDiff({ loading: false, data: null }))
  }
  return <div className={`library-release-card${draft ? ' draft' : ''}${live ? ' live' : ''}`}>
    <div className="library-release-head">
      <div className="library-release-title">
        <b>{release.version}</b>
        <ChannelBadge name={release.channel} />
        {draft && <span className="library-badge draft">Taslak</span>}
        {live && <span className="library-badge live">Canlı</span>}
        {isInstalled && <span className="library-badge installed">Kurulu</span>}
        <small>{formatBytes(release.totalSize)} · {release.fileCount} dosya · {release.publishedAt ? new Date(release.publishedAt).toLocaleDateString('tr-TR') : 'yayınlanmadı'}</small>
      </div>
      <div className="library-card-actions">
        {onInstalls && <button onClick={onInstalls} title="Kimde kurulu">{release.installCount} kurulu</button>}
        <button onClick={toggleDiff} className={diff ? 'active' : ''} aria-expanded={Boolean(diff)}>Değişiklikler</button>
        {onEdit && <button onClick={onEdit}>Düzenle</button>}
        {onLive && !live && <button className="library-primary" onClick={onLive}>Canlıya al</button>}
        {onPublish && <button className="library-primary" onClick={onPublish}>Yayınla</button>}
        {onDelete && <button className="library-danger-text" onClick={onDelete}>Sil</button>}
      </div>
    </div>
    <div className="library-release-divider" />
    <div className="library-release-author"><Avatar user={release.createdBy} />{release.createdBy.displayName}{release.publishedAt && <small> · {timeAgo(release.publishedAt)}</small>}</div>
    {release.notes && <div className={`library-notes${open ? '' : ' clamped'}`}><MessageRichText content={release.notes} />{!open && release.notes.length > 180 && <button type="button" className="library-more" onClick={() => setOpen(true)}>daha fazla</button>}</div>}
    {diff && <ReleaseDiffPanel diff={diff.data} loading={diff.loading} />}
  </div>
}

// Taslakta kategori ve sürüm dâhil her şey; yayınlanmışta notlar/argümanlar/overlay. Yalnız yetkili açabilir.
function EditReleaseDialog({ root, token, release, channels, onClose, onSaved }: { root: string; token: string; release: ReleaseView; channels: string[]; onClose: () => void; onSaved: () => void }) {
  useEscape(true, onClose)
  const isDraft = release.status === 'DRAFT'
  const [channel, setChannel] = useState(release.channel)
  const [version, setVersion] = useState(release.version)
  const [notes, setNotes] = useState(release.notes || '')
  const [launchArgs, setLaunchArgs] = useState(release.launchArgs || '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const cleanChannel = channel.trim().replace(/\s+/g, ' ')
  const canSave = Boolean(cleanChannel && version.trim()) && !busy

  async function save() {
    setBusy(true); setError('')
    // Overlay sürüm bazında her zaman açık; kullanıcı Ayarlar > Kütüphane'den kapatır.
    const body: Record<string, unknown> = { notes, launchArgs, overlay: true }
    if (isDraft) { body.channel = cleanChannel; body.version = version.trim() }
    try { await releaseRequest(`${root}/${release.id}`, token, { method: 'PATCH', body: JSON.stringify(body) }); onSaved() }
    catch (problem: any) { setError(problem?.message || 'Kaydedilemedi.'); setBusy(false) }
  }

  return <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
    <section className="library-upload-modal">
      <header><div><p className="eyebrow">{isDraft ? 'TASLAĞI DÜZENLE' : 'SÜRÜMÜ DÜZENLE'}</p><h2>{release.version} · {channelLabel(release.channel)}</h2></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="library-upload-body">
        <div className="library-field-row">
          <label className="library-field"><span>Kategori</span>
            <span className="library-select"><input list="library-edit-channel-options" value={channel} onChange={e => setChannel(e.target.value)} maxLength={24} disabled={!isDraft} /><Chevron /></span>
            <datalist id="library-edit-channel-options">{channels.map(c => <option key={c} value={c} />)}</datalist>
          </label>
          <label className="library-field"><span>Sürüm</span><input value={version} onChange={e => setVersion(e.target.value)} maxLength={40} disabled={!isDraft} /></label>
        </div>
        {isDraft && channels.length > 0 && <div className="library-chip-row">{channels.map(c => <button type="button" key={c} className={`library-chip ${avatarHue('channel:' + c)}${cleanChannel === c ? ' active' : ''}`} onClick={() => setChannel(c)}><i />{channelLabel(c)}</button>)}</div>}
        {!isDraft && <p className="library-hint">Yayınlanmış sürümün kategorisi ve numarası değiştirilemez; notlar ve başlatma argümanları güncellenebilir.</p>}
        <label className="library-field"><span>Başlatma argümanları</span><input value={launchArgs} onChange={e => setLaunchArgs(e.target.value)} placeholder="-windowed -log" maxLength={500} /></label>
        <div className="library-field"><span>Sürüm notları</span><NotesEditor token={token} value={notes} onChange={setNotes} placeholder="Bu sürümde neler değişti? Görsel ekleyebilir, kalın/italik yazabilirsin." /></div>
        {error && <div className="library-error">{error}</div>}
        <footer><button type="button" onClick={onClose}>Vazgeç</button><button type="button" className="library-primary" disabled={!canSave} onClick={save}>{busy ? 'Kaydediliyor…' : 'Kaydet'}</button></footer>
      </div>
    </section>
  </div>
}

function InstallsDialog({ root, release, token, onClose }: { root: string; release: ReleaseView; token: string; onClose: () => void }) {
  useEscape(true, onClose)
  const [rows, setRows] = useState<Array<{ deviceId: string; releaseId: string | null; version: string; status: string; playSeconds?: number; user: { id: string; displayName: string; username: string; avatarUrl: string | null }; lastLaunchedAt: string | null }>>([])
  useEffect(() => { releaseRequest<{ installs: typeof rows }>(`${root}/installs`, token).then(data => setRows(data.installs)).catch(() => {}) }, [root, token])
  return <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
    <section className="library-installs-modal">
      <header><div><p className="eyebrow">KURULUMLAR</p><h2>{release.version} kimlerde</h2></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="library-installs-list">
        {!rows.length && <p className="library-hint">Henüz kimse kurmadı.</p>}
        {rows.map(row => <div key={row.user.id + row.deviceId} className="library-install-row">
          <Avatar user={row.user} />
          <div><b>{row.user.displayName}</b><small>{row.version}{row.releaseId === release.id ? '' : ' (başka sürüm)'} · {row.status === 'INSTALLED' ? 'kurulu' : row.status === 'UPDATING' ? 'güncelleniyor' : row.status.toLowerCase()}{row.playSeconds ? ` · ${formatDuration(row.playSeconds)} oynadı` : ''}</small></div>
        </div>)}
      </div>
    </section>
  </div>
}
