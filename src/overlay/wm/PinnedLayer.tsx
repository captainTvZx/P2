import { useMemo } from 'react'
import type { OverlayToolProps } from '../overlayTypes'
import { pinnedWindows, useLayout, type WinState } from '../layoutStore'
import { TOOLS } from '../tools/registry'
import { IconPin } from '../icons'

// Panel kapalıyken yalnız sabitli + açık pencereler çizilir: pinOpacity ile, girdi almadan, scrim/araç çubuğu olmadan.
// Animasyon yok (her kare oyunun üstüne biner; gereksiz paint üretilmez).
const noop = () => {}

function GhostWindow({ win, projectId }: { win: WinState; projectId: string }) {
  const tool = TOOLS[win.id]
  const Component = tool.component
  const Icon = tool.icon
  const props: OverlayToolProps = useMemo(() => ({ projectId, visible: false, pinned: true, minimized: win.minimized, focus: noop, requestClose: noop }), [projectId, win.minimized])
  return <section className={`ow ghost${win.minimized ? ' minimized' : ''}`} aria-hidden="true" data-window={win.id}
    style={{ left: win.x, top: win.y, width: win.w, height: win.minimized ? undefined : win.h, opacity: win.pinOpacity, zIndex: 10 + win.z }}>
    <header className="ow-head"><span className="ow-head-icon"><Icon size={15} /></span><b className="ow-title">{tool.title}</b><span className="ow-ghost-pin"><IconPin size={13} /></span></header>
    {win.minimized ? <div className="ow-mini"><Component {...props} /></div> : <div className="ow-body"><Component {...props} /></div>}
  </section>
}

export default function PinnedLayer({ projectId }: { projectId: string }) {
  const layout = useLayout()
  const wins = pinnedWindows(layout)
  if (!wins.length) return null
  return <div className="ow-pinned-layer">{wins.map(win => <GhostWindow key={win.id} win={win} projectId={projectId} />)}</div>
}
