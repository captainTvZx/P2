import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react'
import type { OverlayToolProps } from '../overlayTypes'
import { closeWindow, focusWindow, maximizeWindow, minimizeWindow, moveWindow, openWindows, resizeWindow, setPinOpacity, setPinned, PIN_OPACITY_MAX, PIN_OPACITY_MIN, type WinState } from '../layoutStore'
import { TOOLBAR_W } from '../tools/toolMeta'
import type { ToolDef } from '../tools/registry'
import { IconClose, IconMaximize, IconMinimize, IconPin, IconRestore } from '../icons'
import { useDrag } from './useDrag'
import { bridge } from '../overlayBridge'

// Tek araç penceresi: başlık sürükleme (pointer capture, translate3d; pointerup'ta tek depo yazımı), sabitle (saydamlık
// popover'ı), küçült/büyüt/kapat, dört kenar + dört köşe tutamacı (Windows gibi). Açılış/kapanış animasyonu yalnız geçişte oynar.
// Esc: odaklı girdi → blur; değilse pencere kapanır; hiçbir pencere odaklı değilse OverlayApp paneli gizler.
const EDGE_RIGHT = 48, EDGE_BOTTOM = 32, CLOSE_MS = 220
// Yapışma: kenarlara ve diğer pencerelere SNAP_PX içinde yaklaşınca hizalanır; sol/sağ kenara sürüklenince yarım ekran,
// üst kenara sürüklenince tam ekran (büyütme) bölgesi önizlenir ve bırakınca uygulanır.
const SNAP_PX = 12, ZONE_PX = 10
type SnapZone = { x: number; y: number; w: number; h: number } | null
function snapPosition(id: string, nx: number, ny: number, w: number, h: number): { x: number; y: number } {
  const vw = window.innerWidth, vh = window.innerHeight
  let x = nx, y = ny
  const edgesX = [TOOLBAR_W, vw - w], edgesY = [0, vh - h]
  for (const other of openWindows()) {
    if (other.id === id || other.minimized) continue
    edgesX.push(other.x, other.x + other.w, other.x - w, other.x + other.w - w)
    edgesY.push(other.y, other.y + other.h, other.y - h, other.y + other.h - h)
  }
  for (const edge of edgesX) if (Math.abs(x - edge) <= SNAP_PX) { x = edge; break }
  for (const edge of edgesY) if (Math.abs(y - edge) <= SNAP_PX) { y = edge; break }
  return { x, y }
}
function snapZone(pointerX: number, pointerY: number): SnapZone {
  const vw = window.innerWidth, vh = window.innerHeight
  if (pointerY <= ZONE_PX) return { x: TOOLBAR_W, y: 0, w: vw - TOOLBAR_W, h: vh }
  if (pointerX <= TOOLBAR_W + ZONE_PX) return { x: TOOLBAR_W, y: 0, w: Math.round((vw - TOOLBAR_W) / 2), h: vh }
  if (pointerX >= vw - ZONE_PX) { const half = Math.round((vw - TOOLBAR_W) / 2); return { x: vw - half, y: 0, w: half, h: vh } }
  return null
}
let snapPreviewEl: HTMLDivElement | null = null
function showSnapPreview(zone: SnapZone) {
  if (!zone) { snapPreviewEl?.remove(); snapPreviewEl = null; return }
  if (!snapPreviewEl) { snapPreviewEl = document.createElement('div'); snapPreviewEl.className = 'ow-snap-preview'; document.body.appendChild(snapPreviewEl) }
  Object.assign(snapPreviewEl.style, { left: zone.x + 'px', top: zone.y + 'px', width: zone.w + 'px', height: zone.h + 'px' })
}
type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'
const RESIZE_EDGES: ResizeEdge[] = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw']
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value))
const isEditable = (el: Element | null) => !!el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || (el as HTMLElement).isContentEditable)
export const motionReduced = () => document.documentElement.classList.contains('reduce-motion') || (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false)

type Props = { win: WinState; tool: ToolDef; focused: boolean; projectId: string; animate?: boolean }

export default function OverlayWindow({ win, tool, focused, projectId, animate = true }: Props) {
  const ref = useRef<HTMLElement>(null)
  const [closing, setClosing] = useState(false)
  const [entering, setEntering] = useState(animate && !motionReduced())
  const [pinMenu, setPinMenu] = useState(false)
  const [customTitle, setCustomTitle] = useState<string | null>(null)
  const maximized = !!win.restore
  const id = win.id

  // ---- Başlık sürükleme ----
  const lastDrop = useRef<{ x: number; y: number; zone: SnapZone } | null>(null)
  const drag = useDrag({
    onStart: event => {
      if (maximized || (event.target as HTMLElement).closest('button, input, select, textarea, a, [contenteditable]')) return false
      ref.current?.classList.add('dragging')
    },
    onMove: (dx, dy, event) => {
      const el = ref.current; if (!el) return
      const zone = snapZone(event.clientX, event.clientY)
      showSnapPreview(zone)
      const raw = { x: clamp(win.x + dx, TOOLBAR_W, window.innerWidth - EDGE_RIGHT), y: clamp(win.y + dy, 0, window.innerHeight - EDGE_BOTTOM) }
      const { x: nx, y: ny } = zone ? raw : snapPosition(id, raw.x, raw.y, win.w, win.h)
      lastDrop.current = { x: nx, y: ny, zone }
      el.style.transform = `translate3d(${Math.round(nx - win.x)}px, ${Math.round(ny - win.y)}px, 0)`
    },
    onEnd: (_dx, _dy, moved) => {
      const el = ref.current
      if (el) { el.style.transform = ''; el.classList.remove('dragging') }
      showSnapPreview(null)
      if (!moved) return
      const drop = lastDrop.current
      if (drop?.zone) resizeWindow(id, drop.zone.w, drop.zone.h, drop.zone.x, drop.zone.y)
      else if (drop) moveWindow(id, drop.x, drop.y)
    },
  })
  // ---- Boyutlandırma tutamaçları (Windows gibi: dört kenar + dört köşe) ----
  // Sol/üst kenardan çekince pencerenin x/y'si de kayar; sağ/alt yalnız boyutu değiştirir.
  const size = useRef({ x: win.x, y: win.y, w: win.w, h: win.h })
  const useResize = (edge: ResizeEdge) => useDrag({
    onStart: () => { if (maximized || win.minimized) return false; ref.current?.classList.add('resizing'); size.current = { x: win.x, y: win.y, w: win.w, h: win.h } },
    onMove: (dx, dy) => {
      const el = ref.current; if (!el) return
      const vw = window.innerWidth, vh = window.innerHeight
      let { x, y, w, h } = { x: win.x, y: win.y, w: win.w, h: win.h }
      if (edge.includes('e')) w = clamp(win.w + dx, tool.minSize.w, vw - win.x)
      if (edge.includes('s')) h = clamp(win.h + dy, tool.minSize.h, vh - win.y)
      if (edge.includes('w')) { const right = win.x + win.w; x = clamp(win.x + dx, TOOLBAR_W, right - tool.minSize.w); w = right - x }
      if (edge.includes('n')) { const bottom = win.y + win.h; y = clamp(win.y + dy, 0, bottom - tool.minSize.h); h = bottom - y }
      size.current = { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) }
      el.style.left = `${size.current.x}px`; el.style.top = `${size.current.y}px`
      el.style.width = `${size.current.w}px`; el.style.height = `${size.current.h}px`
    },
    onEnd: (_dx, _dy, moved) => {
      const el = ref.current
      // Satır içi left/top/width/height silinmez: React yalnız değişen prop'u yazar; değişmeyen eksen silinirse
      // pencere "auto" yüksekliğe/0 konumuna düşerdi. Depo aynı değerleri verir, farklıysa React üzerine yazar.
      if (el) el.classList.remove('resizing')
      if (moved) resizeWindow(id, size.current.w, size.current.h, size.current.x, size.current.y)
    },
  })
  const grips: Record<ResizeEdge, ReturnType<typeof useDrag>> = { n: useResize('n'), s: useResize('s'), e: useResize('e'), w: useResize('w'), ne: useResize('ne'), nw: useResize('nw'), se: useResize('se'), sw: useResize('sw') }

  // ---- Kapanış: animasyon sonra depo ----
  const requestClose = useCallback(() => {
    if (closing) return
    if (!animate || motionReduced()) { closeWindow(id); return }
    setClosing(true)
    window.setTimeout(() => closeWindow(id), CLOSE_MS)
  }, [closing, animate, id])
  useEffect(() => { if (!entering) return; const timer = window.setTimeout(() => setEntering(false), 700); return () => window.clearTimeout(timer) }, [entering])

  // ---- Odak ve klavye ----
  const onPointerDownCapture = () => { if (!focused) focusWindow(id) }
  const onPointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    if (!(event.target as HTMLElement).closest('input, textarea, select, button, a, [contenteditable]')) ref.current?.focus({ preventScroll: true })
  }
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== 'Escape') return
    event.stopPropagation()
    if (pinMenu) { setPinMenu(false); return }
    const active = document.activeElement
    if (isEditable(active) && ref.current?.contains(active)) { (active as HTMLElement).blur(); ref.current?.focus({ preventScroll: true }); return }
    requestClose()
  }
  useEffect(() => {
    if (!pinMenu) return
    const close = (event: PointerEvent) => { if (!(event.target as HTMLElement).closest('.ow-pin-pop, .ow-pin-btn')) setPinMenu(false) }
    document.addEventListener('pointerdown', close, true)
    return () => document.removeEventListener('pointerdown', close, true)
  }, [pinMenu])
  const onPin = () => {
    if (!tool.pinnable) return
    if (!win.pinned) { setPinned(id, true); setPinMenu(true); return }
    setPinMenu(open => !open)
  }

  const focus = useCallback(() => { focusWindow(id); ref.current?.focus({ preventScroll: true }) }, [id])
  const setTitle = useCallback((title: string) => setCustomTitle(title || null), [])
  const toolProps: OverlayToolProps = useMemo(() => ({ projectId, visible: true, pinned: win.pinned, minimized: win.minimized, focus, requestClose, setTitle }), [projectId, win.pinned, win.minimized, focus, requestClose, setTitle])
  const Component = tool.component
  const Icon = tool.icon
  const className = `ow${focused ? ' focused' : ''}${win.pinned ? ' pinned' : ''}${win.minimized ? ' minimized' : ''}${maximized ? ' maximized' : ''}${closing ? ' ow-leave' : entering ? ' ow-enter' : ''}`
  const opacityPct = Math.round(win.pinOpacity * 100)

  return <section ref={ref} className={className} tabIndex={-1} role="dialog" aria-label={customTitle || tool.title} data-window={id}
    style={{ left: win.x, top: win.y, width: win.w, height: win.minimized ? undefined : win.h, zIndex: 10 + win.z }}
    onPointerDownCapture={onPointerDownCapture} onPointerDown={onPointerDown} onKeyDown={onKeyDown}>
    <div className="ow-frame">
    <header className="ow-head" onPointerDown={drag.onPointerDown} onDoubleClick={event => { if (!(event.target as HTMLElement).closest('button')) maximizeWindow(id) }}>
      <span className="ow-head-icon"><Icon size={15} /></span>
      <b className="ow-title">{customTitle || tool.title}</b>
      {win.pinned && <span className="p2-badge soft ow-pin-badge" title="Sabitli: panel kapalıyken de görünür">%{opacityPct}</span>}
      <div className="ow-actions">
        {tool.pinnable && <button type="button" className={`ow-act ow-pin-btn${win.pinned ? ' on' : ''}`} aria-pressed={win.pinned} aria-label={win.pinned ? 'Sabitleme ayarları' : 'Sabitle'} title={win.pinned ? 'Sabitli: panel kapalıyken de görünür' : 'Sabitle (panel kapalıyken de görünsün)'} onClick={onPin}><IconPin size={15} /></button>}
        <button type="button" className="ow-act" aria-label={win.minimized ? 'Geri getir' : 'Küçült'} title={win.minimized ? 'Geri getir' : 'Küçült'} onClick={() => minimizeWindow(id, !win.minimized)}><IconMinimize size={15} /></button>
        <button type="button" className="ow-act" aria-label={maximized ? 'Eski boyut' : 'Büyüt'} title={maximized ? 'Eski boyut' : 'Büyüt'} onClick={() => maximizeWindow(id)}>{maximized ? <IconRestore size={14} /> : <IconMaximize size={14} />}</button>
        <button type="button" className="ow-act ow-close" aria-label="Kapat" title="Kapat (Esc)" onClick={requestClose}><IconClose size={15} /></button>
      </div>
      {pinMenu && <div className="ow-pin-pop p2-card glow" role="dialog" aria-label="Sabitleme ayarları" onPointerDown={event => event.stopPropagation()}>
        <span className="p2-kicker">Sabitli pencere</span>
        <p>Panel kapalıyken bu pencere oyunun üstünde kalır; girdi almaz.</p>
        <label className="ow-pin-range"><span>Saydamlık</span>
          <input type="range" className="p2-slider" min={PIN_OPACITY_MIN * 100} max={PIN_OPACITY_MAX * 100} step={5} value={opacityPct} style={{ '--value': `${((opacityPct - PIN_OPACITY_MIN * 100) / ((PIN_OPACITY_MAX - PIN_OPACITY_MIN) * 100)) * 100}%` } as CSSProperties} onChange={event => setPinOpacity(id, Number(event.target.value) / 100)} aria-label="Sabitli pencere saydamlığı" />
          <kbd className="p2-kbd">%{opacityPct}</kbd>
        </label>
        <div className="ow-pin-pop-actions">
          <button type="button" className="p2-btn ghost sm" onClick={() => { setPinned(id, false); setPinMenu(false) }}>Sabitlemeyi kaldır</button>
          <button type="button" className="p2-btn primary sm" onClick={() => { setPinMenu(false); bridge.setVisible(false) }}>Paneli kapat ve gör</button>
        </div>
      </div>}
    </header>
    {win.minimized
      ? <div className="ow-mini"><Component {...toolProps} /></div>
      : <div className="ow-body"><Component {...toolProps} /></div>}
    </div>
    {!win.minimized && !maximized && RESIZE_EDGES.map(edge => <div key={edge} className={`ow-grip ow-grip-${edge}`} onPointerDown={grips[edge].onPointerDown} />)}
  </section>
}
