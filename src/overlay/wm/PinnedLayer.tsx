import { useMemo } from 'react'
import type { OverlayToolProps } from '../overlayTypes'
import { baseTool, instanceOf, pinnedWindows, useLayout, type WinState } from '../layoutStore'
import { TOOLS } from '../tools/registry'
import { TOOL_META } from '../tools/toolMeta'
import { IconPin } from '../icons'

// Panel kapalıyken yalnız sabitli + açık pencereler çizilir: pinOpacity ile, girdi almadan, scrim/araç çubuğu olmadan.
// Animasyon yok (her kare oyunun üstüne biner; gereksiz paint üretilmez). İçerik, OverlayWindow'daki gibi .ow-frame
// içinde kırpılır; yoksa başlık arka planı yuvarlak köşelerden taşar.
const noop = () => {}

function GhostWindow({ win, projectId }: { win: WinState; projectId: string }) {
  const tool = TOOLS[baseTool(win.id) || 'notes']
  const Component = tool.component
  const Icon = tool.icon
  const instance = instanceOf(win.id)
  const props: OverlayToolProps = useMemo(() => ({ projectId, visible: false, pinned: true, minimized: win.minimized, instance, focus: noop, requestClose: noop }), [projectId, win.minimized, instance])
  return <section className={`ow ghost${win.minimized ? ' minimized' : ''}`} aria-hidden="true" data-window={win.id}
    style={{ left: win.x, top: win.y, width: win.w, height: win.minimized || TOOL_META[tool.id].ghostCompact ? undefined : win.h, maxHeight: win.h, opacity: win.pinOpacity, zIndex: 10 + win.z }}>
    <div className="ow-frame">
      <header className="ow-head"><span className="ow-head-icon"><Icon size={15} /></span><b className="ow-title">{tool.title}</b><span className="ow-ghost-pin"><IconPin size={13} /></span></header>
      {win.minimized ? <div className="ow-mini"><Component {...props} /></div> : <div className="ow-body"><Component {...props} /></div>}
    </div>
  </section>
}

export default function PinnedLayer({ projectId }: { projectId: string }) {
  const layout = useLayout()
  const wins = pinnedWindows(layout)
  if (!wins.length) return null
  return <div className="ow-pinned-layer">{wins.map(win => <GhostWindow key={win.id} win={win} projectId={projectId} />)}</div>
}
