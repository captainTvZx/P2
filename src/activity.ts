import { emitRealtime } from './realtime'
import { OVERLAY_MODE, getSettings } from './settings'

// Zengin etkinlik: ne yaptığımı ekibe bildiririm. "Etkinliğimi paylaş" kapalıysa
// hiçbir şey gönderilmez (ve bir kez temizlenir).
export type ActivityKind = 'playing' | 'downloading' | 'uploading' | 'working'
export type ActivityState = { kind: ActivityKind; projectId: string; projectName: string; detail?: string; percent?: number } | null

let currentKey = ''
let cleared = false

export function setActivity(state: ActivityState) {
  if (OVERLAY_MODE) return // oyun içi pencere 'çalışıyor' diye ana pencerenin 'oynuyor' durumunu ezmesin
  if (!getSettings().shareActivity) {
    if (!cleared) { cleared = true; emitRealtime('activity:set', { kind: null }) }
    currentKey = ''
    return
  }
  cleared = false
  if (!state) {
    if (currentKey) { currentKey = ''; emitRealtime('activity:set', { kind: null }) }
    return
  }
  // Yüzde dışındaki değişimler kimliğe girer; yalnız yüzde değişince de gönderilir
  // ama throttle çağıran tarafta yapılır.
  const key = `${state.kind}|${state.projectId}|${state.detail || ''}|${state.percent ?? ''}`
  if (key === currentKey) return
  currentKey = key
  emitRealtime('activity:set', { kind: state.kind, projectId: state.projectId, projectName: state.projectName, detail: state.detail, percent: state.percent })
}

// Paylaşım ayarı kapatılınca sunucudan da düşür.
export function syncActivityShare() {
  if (!getSettings().shareActivity) setActivity(null)
}

// Ekip üyesinin etkinliğini kısa, Discord tarzı metne çevirir.
export function activityText(activity: { kind: string; projectName: string; detail?: string; percent?: number } | null | undefined) {
  if (!activity) return ''
  const name = activity.projectName
  if (activity.kind === 'playing') return `${name} oynuyor`
  if (activity.kind === 'downloading') return `${name} indiriyor${activity.percent != null ? ` · %${activity.percent}` : ''}`
  if (activity.kind === 'uploading') return `${name} için sürüm yüklüyor`
  if (activity.kind === 'working') return activity.detail ? `${name} · ${activity.detail}` : `${name} üzerinde`
  return ''
}
