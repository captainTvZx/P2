import { useSyncExternalStore } from 'react'
import { bridge, MOCK, mockPerf } from './overlayBridge'
import type { OverlayPerf } from './overlayTypes'

// Performans örnekleri: host 1 Hz `overlay:perf` yollar (yalnız panel açıkken ya da HUD sabitliyken). Örnekler pencere
// bileşenlerinden bağımsız burada tutulur; böylece Performans penceresi kapatılıp açılınca geçmiş (FPS çubukları,
// ortalama/en düşük) silinmez. 900 ms altı güncellemeler düşürülür: her yeniden çizim tüm overlay yüzeyinin paint'idir.
export const PERF_HISTORY = 60
export type PerfSnapshot = { perf: OverlayPerf | null; history: number[]; at: number }

let snapshot: PerfSnapshot = { perf: null, history: [], at: 0 }
const listeners = new Set<() => void>()
let started = false
let mockTimer = 0

function emit() { listeners.forEach(fn => fn()) }
export function recordPerf(next: OverlayPerf) {
  const now = Date.now()
  if (now - snapshot.at < 900) return
  const fps = Number.isFinite(next.fps) ? Math.max(0, Math.round(next.fps)) : 0
  snapshot = { perf: next, history: [...snapshot.history.slice(-(PERF_HISTORY - 1)), fps], at: now }
  emit()
}
export function startPerfStore() {
  if (started) return
  started = true
  bridge.onPerf(recordPerf)
}
// Showroom / tarayıcı: sahte örnekler (yalnız MOCK).
export function startMockPerf() {
  if (!MOCK || mockTimer) return
  let tick = 0
  const sample = () => { tick++; mockPerf({ fps: 138 + ((tick * 7) % 9), ping: 31 + ((tick * 3) % 6), cpu: 23 + (tick % 5), ram: { used: 11800, total: 32768 }, gpu: { util: 61 + (tick % 7), temp: 66, memUsed: 5200, memTotal: 12288 }, loss: null, game: { cpu: 14 + (tick % 4), ramMb: 2300 } }) }
  sample()
  mockTimer = window.setInterval(sample, 1000)
}
export function stopMockPerf() { if (mockTimer) { window.clearInterval(mockTimer); mockTimer = 0 } }
const get = () => snapshot
export function usePerf(): PerfSnapshot {
  startPerfStore()
  return useSyncExternalStore(fn => { listeners.add(fn); return () => { listeners.delete(fn) } }, get, get)
}
export const perfAverage = (history: number[]) => history.length ? Math.round(history.reduce((sum, value) => sum + value, 0) / history.length) : null
export const perfMin = (history: number[]) => history.length ? Math.min(...history) : null
