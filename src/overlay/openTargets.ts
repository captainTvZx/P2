import { useSyncExternalStore } from 'react'
import type { WinId } from './layoutStore'

// Ana pencereden "şu pencereyi şu hedefle aç" isteği (OverlayOpenPayload.target): pencere açılır, hedef burada bekler.
// Araç bileşeni `useOpenTarget(id)` ile okur, işledikten sonra `clearOpenTarget(id)` çağırır. Anahtar pencere kimliğidir
// ('tasks' ya da 'tasks:<boardId>' gibi örnek kimlikleri de olabilir).
const targets = new Map<WinId, { target: unknown; at: number }>()
const listeners = new Set<() => void>()
let snapshot: Partial<Record<WinId, { target: unknown; at: number }>> = {}

function emit() { snapshot = Object.fromEntries(targets); listeners.forEach(fn => fn()) }
export function setOpenTarget(id: WinId, target: unknown) { targets.set(id, { target, at: Date.now() }); emit() }
export function clearOpenTarget(id: WinId) { if (targets.delete(id)) emit() }
export function getOpenTarget<T = unknown>(id: WinId): T | undefined { return targets.get(id)?.target as T | undefined }
export function useOpenTarget<T = unknown>(id: WinId): { target: T; at: number } | undefined {
  return useSyncExternalStore(fn => { listeners.add(fn); return () => { listeners.delete(fn) } }, () => snapshot[id] as { target: T; at: number } | undefined, () => undefined)
}
