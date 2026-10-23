import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useSyncExternalStore } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { OVERLAY_MODE, getSettings } from './settings'
import { useRealtimeEvents } from './realtime'
import { playMessageSound, showMessageNotification } from './notifications'
import type { NotifyInput } from './overlayNotify'
type ReleaseNotifyInput = Extract<NotifyInput, { type: 'release' }>

// Kategori (kanal) kullanıcı tanımlı serbest bir etikettir: "alpha", "beta", "release"…
// Her kategorinin en çok bir "canlı" sürümü vardır: herkese giden, otomatik güncellenen sürüm.
export type ReleaseView = {
  id: string; projectId: string; channel: string; version: string; notes: string | null; status: 'DRAFT' | 'PUBLISHED' | 'ARCHIVED'
  executable: string; launchArgs: string | null; platform: string; overlay: boolean
  totalSize: number; fileCount: number; chunkCount: number; installCount: number
  createdById: string; createdAt: string; updatedAt: string; publishedAt: string | null; liveAt: string | null
  createdBy: { id: string; username: string; displayName: string; avatarUrl: string | null }
}
export type ReleaseList = { releases: ReleaseView[]; latest: Record<string, ReleaseView | null>; channels: string[]; permissions: { publish: boolean; download: boolean } }
export type ReleaseDiff = {
  against: { id: string; version: string; channel: string; publishedAt: string | null } | null
  added: number; removed: number; changed: number; unchanged: number
  addedBytes: number; removedBytes: number; changedBytes: number; downloadBytes: number
  files: { added: Array<{ path: string; size: number }>; removed: Array<{ path: string; size: number }>; changed: Array<{ path: string; size: number; previousSize: number }> }
  truncated: boolean
}

// ---- vitrin (hub): aktivite, başarım, inceleme, oynama süresi ----
export type HubUser = { id: string; username: string; displayName: string; avatarUrl: string | null }
export type HubPost = { id: string; projectId: string; authorId: string; kind: 'POST' | 'RELEASE'; releaseId: string | null; content: string; createdAt: string; updatedAt: string; author: HubUser }
export type HubAchievement = { id: string; title: string; description: string | null; icon: string; createdAt: string; unlockedByMe: boolean; unlocks: Array<{ userId: string; unlockedAt: string; user: HubUser }> }
export type HubReview = { id: string; userId: string; recommended: boolean; body: string | null; createdAt: string; updatedAt: string; user: HubUser }
export type HubData = {
  posts: HubPost[]; achievements: HubAchievement[]; reviews: HubReview[]
  playtime: { mine: number; lastPlayedAt: string | null; team: number; players: number }
  permissions: { post: boolean; manage: boolean; review: boolean }
}

export async function releaseRequest<T>(path: string, token: string, init?: RequestInit): Promise<T> {
  const response = await p2Fetch(API_BASE + path, { ...init, headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` } })
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw data
  return data as T
}

// ---- kütüphane durumu (ana süreçten) ----
const empty: LibraryState = { root: '', deviceId: '', projects: {}, jobs: {}, running: [] }
let current: LibraryState = empty
const listeners = new Set<() => void>()
function set(next: LibraryState) { current = next; listeners.forEach(fn => fn()) }
function patchJob(job: LibraryJob) {
  const jobs = { ...current.jobs }
  if (job.active === false && (job.phase === 'done' || job.phase === 'canceled')) delete jobs[job.projectId]
  else jobs[job.projectId] = job
  set({ ...current, jobs })
}

let started = false
function startBridge() {
  if (started || !window.p2?.library) return
  started = true
  void window.p2.library.state().then(set).catch(() => {})
  window.p2.library.onEvent(event => {
    if (event.type === 'job') patchJob(event)
    else if (event.type === 'running') set({ ...current, running: event.running ? [...new Set([...current.running, event.projectId])] : current.running.filter(id => id !== event.projectId) })
    else if (event.type === 'state' || event.type === 'auth-required' || event.type === 'played') void window.p2!.library!.state().then(set).catch(() => {})
  })
}

export function useLibraryStore() {
  useEffect(startBridge, [])
  return useSyncExternalStore(fn => { listeners.add(fn); return () => listeners.delete(fn) }, () => current, () => current)
}

// Ana sürece kimlik ve hız sınırı aktar; her auth değişiminde tazelenir.
export function useLibraryAuth(auth: { accessToken: string; refreshToken?: string }) {
  useEffect(() => {
    if (!window.p2?.library || OVERLAY_MODE) return // overlay penceresi kiralık jetonla kütüphaneyi yeniden yapılandırmaz
    void window.p2.library.configure({ apiBase: API_BASE, accessToken: auth.accessToken, refreshToken: auth.refreshToken || '' })
    void window.p2.library.setLimit({ mbps: getSettings().libraryBandwidthMbps })
  }, [auth.accessToken, auth.refreshToken])
}

// Canlıya alınan sürümleri izleyip kurulu oyunları (Steam gibi) otomatik günceller.
// Elle seçilmiş (sabitlenmiş) sürümler dokunulmaz; "Sürümler" menüsünden canlıya dönülür.
// onRelease: oyun içi overlay bildirim yönlendiricisi (true dönerse masaüstü bildirimi atlanır).
export function useLibraryAutoUpdate(auth: { accessToken: string }, organizationId: string, projects: { id: string; name: string }[], onRelease?: (input: ReleaseNotifyInput) => boolean) {
  const releaseRef = useRef(onRelease); releaseRef.current = onRelease
  const store = useLibraryStore()
  const projectName = useMemo(() => Object.fromEntries(projects.map(p => [p.id, p.name])), [projects])
  const queue = useRef<Array<{ projectId: string; releaseId: string; channel: string }>>([])
  const pumping = useRef(false)
  const pump = useCallback(() => {
    if (pumping.current || !window.p2?.library) return
    const active = Object.values(current.jobs).some(job => job.active)
    if (active) return
    const next = queue.current.shift()
    if (!next) return
    pumping.current = true
    const name = projectName[next.projectId] || 'Proje'
    void window.p2.library.install({ organizationId, projectId: next.projectId, projectName: name, releaseId: next.releaseId, channel: next.channel, pinned: false }).finally(() => { pumping.current = false; setTimeout(pump, 500) })
  }, [organizationId, projectName])
  const consider = useCallback((projectId: string, release: ReleaseView | null) => {
    if (!release || !window.p2?.library || OVERLAY_MODE) return // otomatik indirme yalnız ana pencereden
    const installed = current.projects[projectId]
    if (!installed || installed.pinned || installed.releaseId === release.id) return
    if (installed.channel !== release.channel) return
    if (queue.current.some(item => item.projectId === projectId)) return
    if (getSettings().libraryAutoDownload) { queue.current.push({ projectId, releaseId: release.id, channel: release.channel }); pump() }
  }, [pump])
  // Açılışta kurulu projeler için canlı sürümü denetle.
  useEffect(() => {
    let alive = true
    void (async () => {
      for (const projectId of Object.keys(current.projects)) {
        const info = current.projects[projectId]
        try { const data = await releaseRequest<{ release: ReleaseView | null }>(`/organizations/${organizationId}/projects/${projectId}/releases/latest?channel=${encodeURIComponent(info.channel)}`, auth.accessToken); if (alive) consider(projectId, data.release) } catch {}
      }
    })()
    return () => { alive = false }
  }, [organizationId, auth.accessToken, store.root])
  useRealtimeEvents(auth.accessToken, organizationId, {
    'release:published': (payload: { projectId: string; projectName?: string; release: ReleaseView }) => {
      playMessageSound()
      if (releaseRef.current?.({ type: 'release', phase: 'published', projectId: payload.projectId, projectName: payload.projectName, version: payload.release.version })) return
      showMessageNotification(`${payload.projectName || 'Kütüphane'} · ${payload.release.version}`, 'Yeni sürüm yayınlandı.', {})
    },
    'release:live': (payload: { projectId: string; projectName?: string; release: ReleaseView }) => {
      releaseRef.current?.({ type: 'release', phase: 'live', projectId: payload.projectId, projectName: payload.projectName, version: payload.release.version })
      consider(payload.projectId, payload.release)
    },
  })
  return store
}

// ---- yardımcılar ----
export function formatBytes(bytes: number) {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)))
  return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`
}
export function formatSpeed(bytesPerSecond: number) {
  return `${formatBytes(Math.max(0, bytesPerSecond))}/s`
}
// "1 sa 20 dk", "35 dk", "2 dk"; sıfır için "—".
export function formatDuration(seconds: number) {
  if (!seconds || seconds < 60) return seconds ? `${Math.max(1, Math.round(seconds))} sn` : '—'
  const hours = Math.floor(seconds / 3600), minutes = Math.round((seconds % 3600) / 60)
  if (hours >= 100) return `${hours} sa`
  if (hours) return minutes ? `${hours} sa ${minutes} dk` : `${hours} sa`
  return `${minutes} dk`
}
// Kalan süre: "~3 dk", "~45 sn".
export function formatEta(seconds: number) {
  if (!seconds || !Number.isFinite(seconds)) return ''
  if (seconds < 60) return `~${Math.max(1, Math.round(seconds))} sn`
  if (seconds < 3600) return `~${Math.round(seconds / 60)} dk`
  return `~${Math.floor(seconds / 3600)} sa ${Math.round((seconds % 3600) / 60)} dk`
}
export const phaseLabel: Record<string, string> = {
  preparing: 'Hazırlanıyor', downloading: 'İndiriliyor', 'waiting-for-exit': 'Oyun kapanınca güncellenecek',
  installing: 'Kuruluyor', verifying: 'Doğrulanıyor', paused: 'Duraklatıldı', done: 'Tamam', failed: 'Hata', canceled: 'İptal edildi',
}
// Kategori adını görüntülemek için ilk harf büyük ("alpha" → "Alpha").
export function channelLabel(name: string) {
  return name ? name.charAt(0).toLocaleUpperCase('tr-TR') + name.slice(1) : ''
}

// Bir sonraki semver yama önerisi (yükleyiciye kolaylık).
export function nextVersion(latest: string | undefined) {
  if (!latest) return '1.0.0'
  const match = /^(\d+)\.(\d+)\.(\d+)(.*)$/.exec(latest)
  if (!match) return latest
  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}`
}

export function useLibraryUploads() {
  const [uploads, setUploads] = useState<Array<{ releaseId: string; projectId: string; projectName: string; channel: string; version: string; pending: number; total: number }>>([])
  const refresh = useCallback(() => { if (window.p2?.library) void window.p2.library.uploads().then(setUploads).catch(() => {}) }, [])
  useEffect(refresh, [refresh])
  return { uploads, refresh }
}
