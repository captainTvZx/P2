import { useEffect, useState } from 'react'
import { API_BASE } from '../apiBase'

// Aramada değilken HUD'daki "ms" değeri: P2 sunucusuna GET /health gidiş-dönüş süresi (5 sn'de bir).
// Aramadayken host, ana pencerenin WebRTC RTT'sini gönderir; bu hook yalnız yedektir.
// `loss`: son PROBE_WINDOW yoklamadan kaçı zaman aşımına/hataya düştü (yüzde) — oyunun kendi ağ trafiği ölçülemez
// (UDP, oyun sunucusu); bu P2 sunucusuna erişim kaybıdır ve aramada WebRTC paket kaybı onun önüne geçer.
export type PingStats = { ping: number | null; loss: number | null; samples: number }
const PROBE_WINDOW = 24
const PROBE_MS = 5000
const TIMEOUT_MS = 4000

export function usePingStats(enabled: boolean): PingStats {
  const [stats, setStats] = useState<PingStats>({ ping: null, loss: null, samples: 0 })
  useEffect(() => {
    if (!enabled) { setStats({ ping: null, loss: null, samples: 0 }); return }
    let alive = true
    const results: boolean[] = []
    const push = (ok: boolean, ping: number | null) => {
      results.push(ok); if (results.length > PROBE_WINDOW) results.shift()
      const failed = results.filter(value => !value).length
      if (alive) setStats({ ping, loss: results.length >= 3 ? Math.round((failed / results.length) * 1000) / 10 : null, samples: results.length })
    }
    const probe = async () => {
      const started = performance.now()
      const controller = new AbortController()
      const timer = window.setTimeout(() => controller.abort(), TIMEOUT_MS)
      try {
        const response = await fetch(`${API_BASE}/health`, { cache: 'no-store', signal: controller.signal })
        window.clearTimeout(timer)
        push(response.ok, Math.round(performance.now() - started))
      } catch { window.clearTimeout(timer); push(false, null) }
    }
    void probe()
    const timer = window.setInterval(() => { void probe() }, PROBE_MS)
    return () => { alive = false; window.clearInterval(timer) }
  }, [enabled])
  return stats
}
export function usePing(enabled: boolean): number | null { return usePingStats(enabled).ping }
