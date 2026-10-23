import { useEffect, useState } from 'react'
import { useSettings } from '../settings'
import { bridge, MOCK, mockPerf } from './overlayBridge'
import type { OverlayPerf } from './overlayTypes'
import { usePing } from './usePing'

// FPS + ping göstergesi (sol üst). Host 1 Hz gönderir; burada da 900 ms altı güncellemeler düşürülür: her yeniden
// çizim tüm overlay yüzeyinin paint'i demek. Yalnız settings.overlayHud açıkken çizilir.
export default function Hud({ shifted }: { shifted: boolean }) {
  const settings = useSettings()
  const enabled = settings.overlayHud
  const [perf, setPerf] = useState<OverlayPerf | null>(null)
  // Aramada değilken sunucu /health gidiş-dönüşü gösterilir (host yalnız arama RTT'sini bilir).
  const healthPing = usePing(enabled && !MOCK)
  useEffect(() => {
    if (!enabled) { setPerf(null); return }
    let last = 0
    const off = bridge.onPerf(next => { const now = Date.now(); if (now - last < 900) return; last = now; setPerf(next) })
    let mockTimer = 0
    if (MOCK) {
      let tick = 0
      mockTimer = window.setInterval(() => { tick++; mockPerf({ fps: 138 + ((tick * 7) % 9), ping: 31 + ((tick * 3) % 6) }) }, 1000)
      mockPerf({ fps: 142, ping: 33 })
    }
    return () => { off(); if (mockTimer) window.clearInterval(mockTimer) }
  }, [enabled])
  if (!enabled) return null
  const ping = perf && perf.ping != null && Number.isFinite(perf.ping) ? Math.round(perf.ping) : healthPing
  const fps = perf && Number.isFinite(perf.fps) ? Math.round(perf.fps) : null
  return <div className={`ow-hud${shifted ? ' shifted' : ''}`} aria-label="Performans">
    <span className={fps != null && fps < 30 ? 'low' : ''}>{fps ?? '—'}<small>FPS</small></span>
    <i />
    <span className={ping != null && ping > 150 ? 'low' : ''}>{ping ?? '—'}<small>ms</small></span>
  </div>
}
