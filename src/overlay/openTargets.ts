import { useSyncExternalStore } from 'react'
import type { OverlayToolId } from './overlayTypes'

// Ana pencereden "şu pencereyi şu hedefle aç" isteği (OverlayOpenPayload.target): pencere açılır, hedef burada bekler.
// Araç bileşeni `useOpenTarget(id)` ile okur, işledikten sonra `clearOpenTarget(id)` çağırır.
const targets = new Map<OverlayToolId, { target: unknown; at: number }>()
const listeners = new Set<() => void>()
let snapshot: Partial<Record<OverlayToolId, { target: unknown; at: number }>> = {}

function emit() { snapshot = Object.fromEntries(targets); listeners.forEach(fn => fn()) }
export function setOpenTarget(id: OverlayToolId, target: unknown) { targets.set(id, { target, at: Date.now() }); emit() }
export function clearOpenTarget(id: OverlayToolId) { if (targets.delete(id)) emit() }
export function getOpenTarget<T = unknown>(id: OverlayToolId): T | undefined { return targets.get(id)?.target as T | undefined }
export function useOpenTarget<T = unknown>(id: OverlayToolId): { target: T; at: number } | undefined {
  return useSyncExternalStore(fn => { listeners.add(fn); return () => { listeners.delete(fn) } }, () => snapshot[id] as { target: T; at: number } | undefined, () => undefined)
}
