import { useSyncExternalStore } from 'react'
import { bridge } from './overlayBridge'
import type { OverlayToast } from './overlayTypes'

// Oyun içi bildirim kuyruğu: en çok MAX_VISIBLE kart ekranda, fazlası sırada bekler. `tag` aynı olan yeni kart
// eskisinin yerini alır (süre sıfırlanır), host `onToastDismiss(tag)` ile kapatır (arama bitti vb.). `sticky` kartların
// süresi yoktur. Kapatma animasyonu için kart önce `leaving` olur, OUT_MS sonra listeden düşer.

export type ToastItem = OverlayToast & { leaving?: boolean; seq: number }
export const MAX_VISIBLE = 3
export const TOAST_OUT_MS = 360
const DEFAULT_MS = 6500

export function toastLifeMs(toast: OverlayToast) { return Math.max(2000, Math.min(20000, toast.duration || DEFAULT_MS)) }

let visible: ToastItem[] = []
let queue: ToastItem[] = []
let seq = 0
const timers = new Map<ToastItem['id'], number>()
const listeners = new Set<() => void>()
let started = false

function emit() { listeners.forEach(fn => fn()) }
function clearTimer(id: ToastItem['id']) { const timer = timers.get(id); if (timer) window.clearTimeout(timer); timers.delete(id) }
function armTimer(item: ToastItem) {
  clearTimer(item.id)
  if (item.sticky) return
  timers.set(item.id, window.setTimeout(() => dismissToast(item.id), toastLifeMs(item) - TOAST_OUT_MS))
}
function promote() {
  while (visible.filter(item => !item.leaving).length < MAX_VISIBLE && queue.length) {
    const next = queue.shift()!
    visible = [...visible, next]
    armTimer(next)
  }
}

export function pushOverlayToast(toast: OverlayToast) {
  const item: ToastItem = { ...toast, seq: ++seq, leaving: false }
  if (toast.tag) {
    // Aynı tag'li çıkmakta olan kart varsa hemen düşür (React anahtarı tag olduğundan iki kart aynı anda olmaz).
    visible.filter(entry => entry.tag === toast.tag && entry.leaving).forEach(entry => clearTimer(entry.id))
    visible = visible.filter(entry => !(entry.tag === toast.tag && entry.leaving))
    const index = visible.findIndex(entry => entry.tag === toast.tag)
    if (index >= 0) {
      clearTimer(visible[index].id)
      visible = visible.map((entry, i) => i === index ? item : entry)
      armTimer(item); emit(); return
    }
    queue = queue.filter(entry => entry.tag !== toast.tag)
  }
  if (visible.some(entry => entry.id === toast.id)) { clearTimer(toast.id); visible = visible.map(entry => entry.id === toast.id ? item : entry); armTimer(item); emit(); return }
  queue.push(item)
  promote(); emit()
}
export function dismissToast(id: ToastItem['id']) {
  clearTimer(id)
  const item = visible.find(entry => entry.id === id)
  if (!item) { queue = queue.filter(entry => entry.id !== id); emit(); return }
  if (item.leaving) return
  visible = visible.map(entry => entry.id === id ? { ...entry, leaving: true } : entry)
  timers.set(id, window.setTimeout(() => { timers.delete(id); visible = visible.filter(entry => entry.id !== id); promote(); emit() }, TOAST_OUT_MS))
  emit()
}
export function dismissToastTag(tag: string) {
  queue = queue.filter(entry => entry.tag !== tag)
  visible.filter(entry => entry.tag === tag).forEach(entry => dismissToast(entry.id))
  emit()
}
export function runToastAction(item: ToastItem, actionId: string) {
  bridge.toastAction({ toastId: item.id, tag: item.tag, actionId, payload: item.payload })
  dismissToast(item.id)
}
export function clearOverlayToasts() { timers.forEach(timer => window.clearTimeout(timer)); timers.clear(); visible = []; queue = []; emit() }

export function startToastStore() {
  if (started) return
  started = true
  bridge.onToast(toast => { if (toast && toast.title) pushOverlayToast(toast) })
  bridge.onToastDismiss(tag => { if (tag) dismissToastTag(tag) })
}
const getVisible = () => visible
export function useOverlayToasts() {
  startToastStore()
  return useSyncExternalStore(fn => { listeners.add(fn); return () => { listeners.delete(fn) } }, getVisible, getVisible)
}
export function getOverlayToastCount() { return visible.length }
