import { useSyncExternalStore } from 'react'
import type { OverlayToastCorner, OverlayToolId } from './overlayTypes'
import { REFERENCE_VIEWPORT, TOOLBAR_W, TOOL_META, type ToolBounds } from './tools/toolMeta'

// Overlay pencere düzeni: proje başına localStorage'da saklanır (p2:overlay:layout:<projectId>), 300 ms gecikmeli yazılır.
// Tek kaynak: `layout`; bileşenler useSyncExternalStore ile okur. Sürükleme/boyutlandırma sırasında depoya yazılmaz
// (OverlayWindow transform ile taşır), pointerup'ta tek `moveWindow`/`resizeWindow` çağrısı olur.
//
// Pencere kimliği: araç kimliği ('tasks') ya da araç örneği ('tasks:<boardId>' — bir pano ayrı pencerede). Örnek
// pencereler aynı aracın bileşenini `instance` prop'uyla çizer; araç çubuğu yalnız temel kimlikleri gösterir.
//
// Görünüm alanı değişince (ekran dışı pencere 1280×720 ile açılır, oyun çözünürlüğü sonra gelir; DPI/çözünürlük
// değişimi) pencereler HER ZAMAN kullanıcının son ayarladığı `base` dikdörtgeninden ölçeklenir: ara ölçeklerde
// en küçük boyut kıskacı uygulanmaz, böylece 1920→1280→1920 gidiş-dönüşü pencereyi büyütmez/kaydırmaz.

export type WinId = string
export type WinRect = ToolBounds
export type WinBase = { vw: number; vh: number; x: number; y: number; w: number; h: number }
export type WinState = {
  id: WinId
  x: number; y: number; w: number; h: number
  open: boolean
  minimized: boolean
  pinned: boolean
  pinOpacity: number          // .2–1
  z: number
  restore?: WinRect | null    // büyütmeden önceki dikdörtgen (büyütülmüşse dolu)
  base?: WinBase              // kullanıcının ayarladığı dikdörtgen + o anki görünüm alanı (ölçekleme kaynağı)
}
export type Layout = {
  version: 1
  viewport: { w: number; h: number }
  focus: WinId | null
  windows: Partial<Record<WinId, WinState>>
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

// 'tasks:abc' → 'tasks'; bilinmeyen kimlikler için null.
export function baseTool(id: WinId): OverlayToolId | null {
  const tool = String(id).split(':')[0] as OverlayToolId
  return tool in TOOL_META ? tool : null
}
export function instanceOf(id: WinId): string | undefined { const index = String(id).indexOf(':'); return index > 0 ? String(id).slice(index + 1) : undefined }
export function instanceId(tool: OverlayToolId, instance: string): WinId { return `${tool}:${instance}` }

function emptyLayout(w: number, h: number): Layout { return { version: 1, viewport: { w, h }, focus: null, windows: {} } }
const storageKey = (id: string) => `p2:overlay:layout:${id}`
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value))

function clampRect(rect: WinRect, id: WinId, viewport = layout.viewport): WinRect {
  const min = TOOL_META[baseTool(id) || 'notes'].minSize
  const w = clamp(Math.round(rect.w), Math.min(min.w, viewport.w - TOOLBAR_W), Math.max(min.w, viewport.w - TOOLBAR_W))
  const h = clamp(Math.round(rect.h), Math.min(min.h, viewport.h), Math.max(min.h, viewport.h))
  const x = clamp(Math.round(rect.x), TOOLBAR_W, Math.max(TOOLBAR_W, viewport.w - EDGE_RIGHT))
  const y = clamp(Math.round(rect.y), 0, Math.max(0, viewport.h - EDGE_BOTTOM))
  return { x, y, w, h }
}
// Ara ölçeklerde en küçük boyut uygulanmaz (kayıpsız gidiş-dönüş); yalnız başlık çubuğu erişilebilir kalır.
function clampLoose(rect: WinRect, viewport: { w: number; h: number }): WinRect {
  return {
    x: clamp(Math.round(rect.x), TOOLBAR_W, Math.max(TOOLBAR_W, viewport.w - EDGE_RIGHT)),
    y: clamp(Math.round(rect.y), 0, Math.max(0, viewport.h - EDGE_BOTTOM)),
    w: Math.max(1, Math.round(rect.w)), h: Math.max(1, Math.round(rect.h)),
  }
}
function scaleRect(rect: WinRect, from: { w: number; h: number }, to: { w: number; h: number }): WinRect {
  const sx = to.w / from.w, sy = to.h / from.h
  return { x: rect.x * sx, y: rect.y * sy, w: rect.w * sx, h: rect.h * sy }
}
const baseOf = (rect: WinRect, viewport = layout.viewport): WinBase => ({ vw: viewport.w, vh: viewport.h, x: rect.x, y: rect.y, w: rect.w, h: rect.h })
function fromBase(base: WinBase, to: { w: number; h: number }): WinRect {
  if (base.vw === to.w && base.vh === to.h) return { x: base.x, y: base.y, w: base.w, h: base.h }
  return clampLoose(scaleRect(base, { w: base.vw, h: base.vh }, to), to)
}
function defaultWindow(id: WinId): WinState {
  const tool = baseTool(id) || 'notes'
  const rect = clampRect(scaleRect(TOOL_META[tool].defaultBounds, REFERENCE_VIEWPORT, layout.viewport), id)
  return { id, ...rect, open: false, minimized: false, pinned: false, pinOpacity: defaultPinOpacity, z: 0, restore: null, base: baseOf(rect) }
}
function maxZ() { let z = 0; for (const win of Object.values(layout.windows)) if (win && win.z > z) z = win.z; return z }

function sanitize(raw: unknown, viewport: { w: number; h: number }): Layout | null {
  if (!raw || typeof raw !== 'object') return null
  const data = raw as Partial<Layout>
  if (data.version !== 1 || !data.windows || typeof data.windows !== 'object') return null
  const from = data.viewport && data.viewport.w > 0 && data.viewport.h > 0 ? data.viewport : viewport
  const windows: Layout['windows'] = {}
  for (const id of Object.keys(data.windows)) {
    const tool = baseTool(id)
    const win = (data.windows as Record<string, any>)[id]
    if (!tool || !win || typeof win !== 'object') continue
    const savedBase = win.base && typeof win.base === 'object' && +win.base.vw > 0 && +win.base.vh > 0
      ? { vw: +win.base.vw, vh: +win.base.vh, x: +win.base.x || 0, y: +win.base.y || 0, w: +win.base.w || 0, h: +win.base.h || 0 }
      : { vw: from.w, vh: from.h, x: +win.x || 0, y: +win.y || 0, w: +win.w || 0, h: +win.h || 0 }
    const base: WinBase = savedBase.w > 0 && savedBase.h > 0 ? savedBase : baseOf(TOOL_META[tool].defaultBounds, REFERENCE_VIEWPORT)
    const rect = fromBase(base, viewport)
    windows[id] = {
      id, ...rect, base,
      open: !!win.open, minimized: !!win.minimized, pinned: !!win.pinned && TOOL_META[tool].pinnable,
      pinOpacity: clamp(+win.pinOpacity || defaultPinOpacity, PIN_OPACITY_MIN, PIN_OPACITY_MAX),
      z: Math.max(0, +win.z || 0),
      restore: win.restore && typeof win.restore === 'object' ? clampLoose(scaleRect(win.restore, from, viewport), viewport) : null,
    }
  }
  const focus = data.focus && windows[data.focus] ? data.focus : null
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
function patch(id: WinId, change: Partial<WinState>) {
  const win = layout.windows[id] || defaultWindow(id)
  set({ ...layout, windows: { ...layout.windows, [id]: { ...win, ...change } } })
}
// Kullanıcı dikdörtgeni değiştirdi: yeni base = şimdiki görünüm alanı.
function placed(id: WinId, rect: WinRect): Partial<WinState> { const next = clampRect(rect, id); return { ...next, base: baseOf(next), restore: null } }

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
export function useWindow(id: WinId) { return useSyncExternalStore(subscribeLayout, () => layout.windows[id], () => layout.windows[id]) }
export function openWindows(state: Layout = layout): WinState[] {
  return (Object.values(state.windows) as WinState[]).filter(win => win && win.open).sort((a, b) => a.z - b.z)
}
export function pinnedWindows(state: Layout = layout): WinState[] { return openWindows(state).filter(win => win.pinned) }
// Açık pencereler çizim sırası: araç çubuğu sırası, sonra örnek pencereler (kararlı anahtar sırası).
export function openWindowIds(state: Layout = layout, order: readonly OverlayToolId[]): WinId[] {
  const ids = Object.keys(state.windows).filter(id => state.windows[id]?.open)
  const rank = (id: WinId) => { const index = order.indexOf(baseTool(id) as OverlayToolId); return (index < 0 ? 99 : index) * 2 + (instanceOf(id) ? 1 : 0) }
  return ids.sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))
}

// ---- Eylemler ----
export function openWindow(id: WinId) {
  if (!baseTool(id)) return
  const win = layout.windows[id] || defaultWindow(id)
  set({ ...layout, focus: id, windows: { ...layout.windows, [id]: { ...win, open: true, minimized: false, z: maxZ() + 1 } } })
}
// Araç örneği (ör. bir pano ayrı pencerede): ilk açılışta ana pencerenin yanına kaydırılarak yerleşir.
export function openInstance(tool: OverlayToolId, instance: string) {
  const id = instanceId(tool, instance)
  if (!layout.windows[id]) {
    const parent = layout.windows[tool]
    const fallback = defaultWindow(id)
    const rect = parent ? clampRect({ x: parent.x + 36, y: parent.y + 36, w: parent.w, h: parent.h }, id) : fallback
    const win: WinState = { ...fallback, ...rect, base: baseOf(rect) }
    set({ ...layout, windows: { ...layout.windows, [id]: win } })
  }
  openWindow(id)
}
export function closeWindow(id: WinId) {
  const win = layout.windows[id]
  if (!win || !win.open) return
  const restored = win.restore ? { ...win.restore, base: baseOf(win.restore) } : {}
  const windows = { ...layout.windows, [id]: { ...win, open: false, minimized: false, restore: null, ...restored } }
  const focus = layout.focus === id ? (openWindows({ ...layout, windows }).at(-1)?.id ?? null) : layout.focus
  set({ ...layout, focus, windows })
}
// Örnek pencere kapanınca kaydı da silinir (pano silinmiş olabilir); temel pencereler yerlerini korur.
export function forgetWindow(id: WinId) {
  if (!instanceOf(id) || !layout.windows[id]) return
  const windows = { ...layout.windows }; delete windows[id]
  const focus = layout.focus === id ? (openWindows({ ...layout, windows }).at(-1)?.id ?? null) : layout.focus
  set({ ...layout, focus, windows })
}
export function toggleWindow(id: WinId) {
  const win = layout.windows[id]
  if (!win || !win.open) return openWindow(id)
  if (win.minimized || layout.focus !== id) return openWindow(id)  // küçültülmüş/arkada → öne getir
  closeWindow(id)
}
export function focusWindow(id: WinId) {
  const win = layout.windows[id]
  if (!win || !win.open) return
  if (layout.focus === id && win.z === maxZ()) return
  set({ ...layout, focus: id, windows: { ...layout.windows, [id]: { ...win, z: maxZ() + 1 } } })
}
export function moveWindow(id: WinId, x: number, y: number) {
  const win = layout.windows[id]; if (!win) return
  patch(id, placed(id, { x, y, w: win.w, h: win.h }))
}
export function resizeWindow(id: WinId, w: number, h: number, x = layout.windows[id]?.x ?? TOOLBAR_W, y = layout.windows[id]?.y ?? 0) {
  if (!layout.windows[id]) return
  patch(id, placed(id, { x, y, w, h }))
}
export function setPinned(id: WinId, pinned: boolean) {
  const tool = baseTool(id)
  if (!tool || (pinned && !TOOL_META[tool].pinnable)) return
  patch(id, { pinned })
}
export function setPinOpacity(id: WinId, opacity: number) { patch(id, { pinOpacity: clamp(Math.round(opacity * 100) / 100, PIN_OPACITY_MIN, PIN_OPACITY_MAX) }) }
export function minimizeWindow(id: WinId, minimized = true) {
  const win = layout.windows[id]; if (!win || !win.open) return
  const focus = minimized && layout.focus === id ? (openWindows().filter(other => other.id !== id && !other.minimized).at(-1)?.id ?? null) : (minimized ? layout.focus : id)
  set({ ...layout, focus, windows: { ...layout.windows, [id]: { ...win, minimized, z: minimized ? win.z : maxZ() + 1 } } })
}
export function maximizeWindow(id: WinId) {
  const win = layout.windows[id]; if (!win || !win.open) return
  if (win.restore) { patch(id, { ...placed(id, win.restore), minimized: false }); return }
  const { w, h } = layout.viewport
  patch(id, { restore: { x: win.x, y: win.y, w: win.w, h: win.h }, x: TOOLBAR_W, y: 0, w: w - TOOLBAR_W, h, minimized: false })
}
export function isMaximized(win: WinState | undefined) { return !!win?.restore }
export function setToastCorner(corner: OverlayToastCorner | undefined) { set({ ...layout, toastCorner: corner }) }
export function resetLayout() {
  const { w, h } = layout.viewport
  set(emptyLayout(w, h))
}
// Görünüm alanı değişince (çözünürlük/DPI): pencereler `base` dikdörtgeninden ölçeklenir (kayıpsız), büyütülmüşler tam ekran kalır.
export function applyViewport(w: number, h: number) {
  w = Math.max(320, Math.round(w)); h = Math.max(240, Math.round(h))
  if (w === layout.viewport.w && h === layout.viewport.h) return
  const from = layout.viewport, to = { w, h }
  const windows: Layout['windows'] = {}
  for (const win of Object.values(layout.windows) as WinState[]) {
    if (!win) continue
    const maximized = !!win.restore
    const base = win.base || baseOf(win, from)
    const rect = maximized ? { x: TOOLBAR_W, y: 0, w: w - TOOLBAR_W, h } : fromBase(base, to)
    windows[win.id] = { ...win, ...rect, base, restore: win.restore ? clampLoose(scaleRect(win.restore, from, to), to) : null }
  }
  set({ ...layout, viewport: to, windows })
}
export function flushLayout() { if (persistTimer) persistNow() }
