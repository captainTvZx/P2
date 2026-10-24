import { useEffect, useLayoutEffect, useState } from 'react'
import { startSelectShim } from './overlay/selectShim'
import './p2-theme.css'
import './motion.css'
import './ui.css'
import './overlay.css'
import './overlay/overlay-wm.css'
import Logo from './ui/Logo'
import { getSettings, useSettings } from './settings'
import { bridge, getSession, MOCK_QUERY, useOverlayFocus, useOverlayVisible } from './overlay/overlayBridge'
import { applyViewport, baseTool, flushLayout, initLayout, openWindow, openWindowIds, pinnedWindows, useLayout } from './overlay/layoutStore'
import { setOpenTarget } from './overlay/openTargets'
import { startToastStore, useOverlayToasts } from './overlay/toastStore'
import { TOOLS, TOOL_ORDER } from './overlay/tools/registry'
import type { OverlayOpenPayload } from './overlay/overlayTypes'
import OverlayWindow from './overlay/wm/OverlayWindow'
import Toolbar from './overlay/wm/Toolbar'
import PinnedLayer from './overlay/wm/PinnedLayer'
import OverlayToasts from './overlay/Toasts'
import Hud from './overlay/Hud'
import { formatHotkeyKeys } from './hotkeys'

// Oyun içi overlay kabuğu (Shift+2). Ekran dışı çizilir, enjekte DLL oyunun üstüne bindirir.
// Panel açık: scrim + sol araç çubuğu + açık pencereler (+ bildirimler + HUD).
// Panel kapalı: yalnız sabitli pencereler (girdisiz, saydam) + bildirimler + HUD; geri kalan her şey şeffaftır.
// Çizim maskesinin sahibi bu bileşendir: bildirim/sabitli/HUD sayısı değişince host'a anında `drawState` gider.
const isEditable = (el: Element | null) => !!el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el as HTMLElement).isContentEditable)

export default function OverlayApp({ demo = false }: { demo?: boolean }) {
  const [session] = useState(() => {
    const info = getSession()
    initLayout(info.projectId, { w: window.innerWidth, h: window.innerHeight }, { defaultPinOpacity: getSettings().overlayOpacity })
    startToastStore()
    return info
  })
  const visible = useOverlayVisible()
  const focused = useOverlayFocus()
  const settings = useSettings()
  const layout = useLayout()
  const toasts = useOverlayToasts()
  const openIds = openWindowIds(layout, TOOL_ORDER)
  const pinnedCount = pinnedWindows(layout).length
  const hud = settings.overlayHud

  // Uygulamanın kök arka planı (styles.css :root) opak; overlay penceresi tamamen şeffaf olmalı.
  useLayoutEffect(() => {
    if (demo) return
    document.documentElement.classList.add('overlay-mode')
    startSelectShim() // OSR: yerel <select> listesi çizilmez; DOM içi liste
    return () => document.documentElement.classList.remove('overlay-mode')
  }, [demo])
  useEffect(() => { bridge.setZoom(Math.max(0.5, Math.min(2, settings.overlayScale || 1))) }, [settings.overlayScale])
  useEffect(() => { document.documentElement.classList.toggle('reduce-motion', !!settings.overlayReduceMotion) }, [settings.overlayReduceMotion])
  useEffect(() => {
    const onResize = () => applyViewport(window.innerWidth, window.innerHeight)
    onResize()
    window.addEventListener('resize', onResize)
    window.addEventListener('beforeunload', flushLayout)
    return () => { window.removeEventListener('resize', onResize); window.removeEventListener('beforeunload', flushLayout); flushLayout() }
  }, [])
  // Odak kaybı (oyun pencereye geri döndü): tutulan girdi odağı bırakılır.
  useEffect(() => { if (!focused) (document.activeElement as HTMLElement | null)?.blur?.() }, [focused])
  // Panel kapanınca düzen hemen yazılır.
  useEffect(() => { if (!visible) flushLayout() }, [visible])
  // Çizim maskesi: host yalnız gerekli olduğunda kare çizer.
  useEffect(() => { bridge.drawState({ toasts: toasts.length, pinned: pinnedCount > 0, hud }) }, [toasts.length, pinnedCount, hud])
  // Ana pencereden "pencere aç" istekleri (overlay:event {type:'open'} ya da push topic 'open').
  useEffect(() => {
    const handle = (payload: OverlayOpenPayload | undefined) => {
      if (!payload || !payload.window || !baseTool(String(payload.window))) return
      if (payload.target !== undefined) setOpenTarget(String(payload.window), payload.target)
      openWindow(String(payload.window))
      bridge.setVisible(true)
    }
    const offEvent = bridge.onEvent(event => { if (event && event.type === 'open') handle(event as unknown as OverlayOpenPayload) })
    const offPush = bridge.onPush(push => { if (push && push.topic === 'open') handle(push.payload as OverlayOpenPayload) })
    return () => { offEvent(); offPush() }
  }, [])
  // Esc zinciri: pencere içindeyken OverlayWindow işler (girdi blur → pencere kapat); hiçbir pencere odaklı değilse panel kapanır.
  useEffect(() => {
    if (!visible) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return
      if ((event.target as HTMLElement | null)?.closest?.('.ow')) return
      if (isEditable(document.activeElement)) { (document.activeElement as HTMLElement).blur(); return }
      bridge.setVisible(false)
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [visible])

  const stage = demo || MOCK_QUERY ? <div className="ow-demo-stage" aria-hidden="true"><b>OYUN</b><small>{visible ? 'Esc paneli kapatır' : 'Shift+2 paneli açar'}</small></div> : null
  const panelKeys = formatHotkeyKeys(settings.overlayHotkeys?.panel || '')
  if (!visible) return <>{stage}<div className="overlay-hidden">
    <PinnedLayer projectId={session.projectId} />
    <OverlayToasts interactive={false} />
    <Hud shifted={false} />
  </div></>
  return <>{stage}<div className={`overlay-root${demo ? ' ow-demo' : ''}`}>
    <div className="overlay-scrim" />
    <Toolbar projectName={session.projectName} />
    {openIds.map(id => <OverlayWindow key={id} win={layout.windows[id]!} tool={TOOLS[baseTool(id)!]} focused={layout.focus === id} projectId={session.projectId} />)}
    <OverlayToasts interactive />
    <Hud shifted />
  </div></>
}
