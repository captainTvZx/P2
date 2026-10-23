import { useSyncExternalStore } from 'react'
import type { OverlayToastCorner, OverlayToolId } from './overlayTypes'
import { REFERENCE_VIEWPORT, TOOLBAR_W, TOOL_META, type ToolBounds } from './tools/toolMeta'

// Overlay pencere düzeni: proje başına localStorage'da saklanır (p2:overlay:layout:<projectId>), 300 ms gecikmeli yazılır.
// Tek kaynak: `layout`; bileşenler useSyncExternalStore ile okur. Sürükleme/boyutlandırma sırasında depoya yazılmaz
// (OverlayWindow transform ile taşır), pointerup'ta tek `moveWindow`/`resizeWindow` çağrısı olur.

export type WinRect = ToolBounds
export type WinState = {
  id: OverlayToolId
  x: number; y: number; w: number; h: number
  open: boolean
  minimized: boolean
  pinned: boolean
  pinOpacity: number          // .2–1
  z: number
  restore?: WinRect | null    // büyütmeden önceki dikdörtgen (büyütülmüşse dolu)
}
export type Layout = {
  version: 1
  viewport: { w: number; h: number }
  focus: OverlayToolId | null
  windows: Partial<Record<OverlayToolId, WinState>>
  toastCorner?: OverlayToastCorner
}

export const PIN_OPACITY_MIN = 0.2
export const PIN_OPACITY_MAX = 1
const EDGE_RIGHT = 48   // sağda kalması gereken başlık payı
const EDGE_BOTTOM = 32  // altta kalması gereken başlık payı
const PERSIST_MS = 300

let projectId = 'demo'
let layout: Layout = emptyLayout(REFERENCE_VIEWPORT.w, REFERENCE_VIEWPORT.h)
let defaultPinOpacity = 0.92
const listeners = new Set<() => void>()
let persistTimer = 0

function emptyLayout(w: number, h: number): Layout { return { version: 1, viewport: { w, h }, focus: null, windows: {} } }
const storageKey = (id: string) => `p2:overlay:layout:${id}`
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value))

function clampRect(rect: WinRect, id: OverlayToolId, viewport = layout.viewport): WinRect {
  const min = TOOL_META[id].minSize
  const w = clamp(Math.round(rect.w), Math.min(min.w, viewport.w - TOOLBAR_W), Math.max(min.w, viewport.w - TOOLBAR_W))
  const h = clamp(Math.round(rect.h), Math.min(min.h, viewport.h), Math.max(min.h, viewport.h))
  const x = clamp(Math.round(rect.x), TOOLBAR_W, Math.max(TOOLBAR_W, viewport.w - EDGE_RIGHT))
  const y = clamp(Math.round(rect.y), 0, Math.max(0, viewport.h - EDGE_BOTTOM))
  return { x, y, w, h }
}
function scaleRect(rect: WinRect, from: { w: number; h: number }, to: { w: number; h: number }): WinRect {
  const sx = to.w / from.w, sy = to.h / from.h
  return { x: rect.x * sx, y: rect.y * sy, w: rect.w * sx, h: rect.h * sy }
}
function defaultWindow(id: OverlayToolId): WinState {
  const rect = clampRect(scaleRect(TOOL_META[id].defaultBounds, REFERENCE_VIEWPORT, layout.viewport), id)
  return { id, ...rect, open: false, minimized: false, pinned: false, pinOpacity: defaultPinOpacity, z: 0, restore: null }
}
function maxZ() { let z = 0; for (const win of Object.values(layout.windows)) if (win && win.z > z) z = win.z; return z }

function sanitize(raw: unknown, viewport: { w: number; h: number }): Layout | null {
  if (!raw || typeof raw !== 'object') return null
  const data = raw as Partial<Layout>
  if (data.version !== 1 || !data.windows || typeof data.windows !== 'object') return null
  const from = data.viewport && data.viewport.w > 0 && data.viewport.h > 0 ? data.viewport : viewport
  const windows: Layout['windows'] = {}
  for (const id of Object.keys(TOOL_META) as OverlayToolId[]) {
    const win = (data.windows as any)[id]
    if (!win || typeof win !== 'object') continue
    const rect = clampRect(scaleRect({ x: +win.x || 0, y: +win.y || 0, w: +win.w || 0, h: +win.h || 0 }, from, viewport), id, viewport)
    windows[id] = {
      id, ...rect,
      open: !!win.open, minimized: !!win.minimized, pinned: !!win.pinned && TOOL_META[id].pinnable,
      pinOpacity: clamp(+win.pinOpacity || defaultPinOpacity, PIN_OPACITY_MIN, PIN_OPACITY_MAX),
      z: Math.max(0, +win.z || 0),
      restore: win.restore && typeof win.restore === 'object' ? clampRect(scaleRect(win.restore, from, viewport), id, viewport) : null,
    }
  }
  const focus = data.focus && windows[data.focus as OverlayToolId] ? data.focus as OverlayToolId : null
  return { version: 1, viewport: { ...viewport }, focus, windows, toastCorner: data.toastCorner }
}

function emit() { listeners.forEach(fn => fn()) }
function set(next: Layout) { layout = next; emit(); schedulePersist() }
function schedulePersist() {
  window.clearTimeout(persistTimer)
  persistTimer = window.setTimeout(persistNow, PERSIST_MS)
}
function persistNow() {
  window.clearTimeout(persistTimer); persistTimer = 0
  try { localStorage.setItem(storageKey(projectId), JSON.stringify(layout)) } catch { /* depolama kapalı */ }
}
function patch(id: OverlayToolId, change: Partial<WinState>) {
  const win = layout.windows[id] || defaultWindow(id)
  set({ ...layout, windows: { ...layout.windows, [id]: { ...win, ...change } } })
}

// ---- Başlatma ----
export function initLayout(nextProjectId: string, viewport: { w: number; h: number }, options?: { defaultPinOpacity?: number }) {
  if (persistTimer) persistNow()
  projectId = nextProjectId || 'demo'
  if (options?.defaultPinOpacity) defaultPinOpacity = clamp(options.defaultPinOpacity, PIN_OPACITY_MIN, PIN_OPACITY_MAX)
  const safe = { w: Math.max(320, viewport.w || REFERENCE_VIEWPORT.w), h: Math.max(240, viewport.h || REFERENCE_VIEWPORT.h) }
  let loaded: Layout | null = null
  try { loaded = sanitize(JSON.parse(localStorage.getItem(storageKey(projectId)) || 'null'), safe) } catch { loaded = null }
  layout = loaded || emptyLayout(safe.w, safe.h)
  emit()
}
export function getProjectId() { return projectId }

// ---- Okuma ----
export function getLayout() { return layout }
export function subscribeLayout(fn: () => void) { listeners.add(fn); return () => { listeners.delete(fn) } }
export function useLayout() { return useSyncExternalStore(subscribeLayout, getLayout, getLayout) }
export function useWindow(id: OverlayToolId) { return useSyncExternalStore(subscribeLayout, () => layout.windows[id], () => layout.windows[id]) }
export function openWindows(state: Layout = layout): WinState[] {
  return (Object.values(state.windows) as WinState[]).filter(win => win && win.open).sort((a, b) => a.z - b.z)
}
export function pinnedWindows(state: Layout = layout): WinState[] { return openWindows(state).filter(win => win.pinned) }

// ---- Eylemler ----
export function openWindow(id: OverlayToolId) {
  const win = layout.windows[id] || defaultWindow(id)
  set({ ...layout, focus: id, windows: { ...layout.windows, [id]: { ...win, open: true, minimized: false, z: maxZ() + 1 } } })
}
export function closeWindow(id: OverlayToolId) {
  const win = layout.windows[id]
  if (!win || !win.open) return
  const windows = { ...layout.windows, [id]: { ...win, open: false, minimized: false, restore: null, ...(win.restore ? win.restore : {}) } }
  const focus = layout.focus === id ? (openWindows({ ...layout, windows }).at(-1)?.id ?? null) : layout.focus
  set({ ...layout, focus, windows })
}
export function toggleWindow(id: OverlayToolId) {
  const win = layout.windows[id]
  if (!win || !win.open) return openWindow(id)
  if (win.minimized || layout.focus !== id) return openWindow(id)  // küçültülmüş/arkada → öne getir
  closeWindow(id)
}
export function focusWindow(id: OverlayToolId) {
  const win = layout.windows[id]
  if (!win || !win.open) return
  if (layout.focus === id && win.z === maxZ()) return
  set({ ...layout, focus: id, windows: { ...layout.windows, [id]: { ...win, z: maxZ() + 1 } } })
}
export function moveWindow(id: OverlayToolId, x: number, y: number) {
  const win = layout.windows[id]; if (!win) return
  const rect = clampRect({ x, y, w: win.w, h: win.h }, id)
  patch(id, { x: rect.x, y: rect.y, restore: null })
}
export function resizeWindow(id: OverlayToolId, w: number, h: number, x = layout.windows[id]?.x ?? TOOLBAR_W, y = layout.windows[id]?.y ?? 0) {
  if (!layout.windows[id]) return
  patch(id, { ...clampRect({ x, y, w, h }, id), restore: null })
}
export function setPinned(id: OverlayToolId, pinned: boolean) {
  if (pinned && !TOOL_META[id].pinnable) return
  patch(id, { pinned })
}
export function setPinOpacity(id: OverlayToolId, opacity: number) { patch(id, { pinOpacity: clamp(Math.round(opacity * 100) / 100, PIN_OPACITY_MIN, PIN_OPACITY_MAX) }) }
export function minimizeWindow(id: OverlayToolId, minimized = true) {
  const win = layout.windows[id]; if (!win || !win.open) return
  const focus = minimized && layout.focus === id ? (openWindows().filter(other => other.id !== id && !other.minimized).at(-1)?.id ?? null) : (minimized ? layout.focus : id)
  set({ ...layout, focus, windows: { ...layout.windows, [id]: { ...win, minimized, z: minimized ? win.z : maxZ() + 1 } } })
}
export function maximizeWindow(id: OverlayToolId) {
  const win = layout.windows[id]; if (!win || !win.open) return
  if (win.restore) { patch(id, { ...clampRect(win.restore, id), restore: null, minimized: false }); return }
  const { w, h } = layout.viewport
  patch(id, { restore: { x: win.x, y: win.y, w: win.w, h: win.h }, x: TOOLBAR_W, y: 0, w: w - TOOLBAR_W, h, minimized: false })
}
export function isMaximized(win: WinState | undefined) { return !!win?.restore }
export function setToastCorner(corner: OverlayToastCorner | undefined) { set({ ...layout, toastCorner: corner }) }
export function resetLayout() {
  const { w, h } = layout.viewport
  set(emptyLayout(w, h))
}
// Görünüm alanı değişince (çözünürlük/DPI): pencereler oransal ölçeklenir, sonra başlık çubuğu erişilebilir kalacak şekilde kıskaçlanır.
export function applyViewport(w: number, h: number) {
  w = Math.max(320, Math.round(w)); h = Math.max(240, Math.round(h))
  if (w === layout.viewport.w && h === layout.viewport.h) return
  const from = layout.viewport, to = { w, h }
  const windows: Layout['windows'] = {}
  for (const win of Object.values(layout.windows) as WinState[]) {
    if (!win) continue
    const maximized = !!win.restore
    const rect = maximized ? { x: TOOLBAR_W, y: 0, w: w - TOOLBAR_W, h } : clampRect(scaleRect(win, from, to), win.id, to)
    windows[win.id] = { ...win, ...rect, restore: win.restore ? clampRect(scaleRect(win.restore, from, to), win.id, to) : null }
  }
  set({ ...layout, viewport: to, windows })
}
export function flushLayout() { if (persistTimer) persistNow() }
