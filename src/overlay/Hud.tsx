import { useEffect } from 'react'
import { useSettings } from '../settings'
import { MOCK } from './overlayBridge'
import { startMockPerf, stopMockPerf, usePerf } from './perfStore'
import { usePingStats } from './usePing'

// FPS + ping göstergesi; yeri ayardan (sol üst / üst orta / sağ üst). "Ayrıntılı" açıkken oyunun CPU'su, RAM, GPU ve
// paket kaybı da yazar. Host 1 Hz gönderir; perfStore 900 ms altını düşürür (her yeniden çizim tüm overlay yüzeyinin
// paint'i demek). Yalnız settings.overlayHud açıkken çizilir.
export default function Hud({ shifted }: { shifted: boolean }) {
  const settings = useSettings()
  const enabled = settings.overlayHud
  const { perf } = usePerf()
  // Aramada değilken sunucu /health gidiş-dönüşü gösterilir (host yalnız arama RTT'sini bilir).
  const net = usePingStats(enabled && !MOCK)
  useEffect(() => { if (!enabled || !MOCK) return; startMockPerf(); return stopMockPerf }, [enabled])
  if (!enabled) return null
  const ping = perf && perf.ping != null && Number.isFinite(perf.ping) ? Math.round(perf.ping) : net.ping
  const fps = perf && Number.isFinite(perf.fps) ? Math.round(perf.fps) : null
  const loss = perf?.loss != null ? Math.round(perf.loss * 10) / 10 : net.loss
  const gameCpu = perf?.game?.cpu ?? perf?.cpu ?? null
  const gameRam = perf?.game?.ramMb ?? null
  const position = settings.overlayHudPosition || 'top-left'
  const details = settings.overlayHudDetails
  return <div className={`ow-hud pos-${position}${shifted && position === 'top-left' ? ' shifted' : ''}`} aria-label="Performans">
    <span className={fps != null && fps < 30 ? 'low' : ''}>{fps ?? '—'}<small>FPS</small></span>
    <i />
    <span className={ping != null && ping > 150 ? 'low' : ''}>{ping ?? '—'}<small>ms</small></span>
    {details && <>
      <i />
      <span className={gameCpu != null && gameCpu > 90 ? 'low' : ''} title="Oyun sürecinin CPU kullanımı">{gameCpu ?? '—'}<small>CPU</small></span>
      {gameRam != null && <span title="Oyun sürecinin belleği">{gameRam >= 1024 ? `${Math.round(gameRam / 1024 * 10) / 10}G` : `${gameRam}M`}<small>RAM</small></span>}
      {perf?.gpu && <span className={perf.gpu.temp > 85 ? 'low' : ''} title={`GPU ${perf.gpu.temp}°C`}>{perf.gpu.util}<small>GPU</small></span>}
      <span className={loss != null && loss > 5 ? 'low' : ''} title={perf?.loss != null ? 'Sesli arama paket kaybı' : 'P2 sunucusuna erişim kaybı'}>{loss ?? '—'}<small>%kayıp</small></span>
    </>}
  </div>
}
