import { useSyncExternalStore } from 'react'
import type { OverlayDrawState, OverlayEvent, OverlayHotkeyEvent, OverlayOpenInApp, OverlayPerf, OverlayPush, OverlayScreenshot, OverlaySendTopic, OverlaySessionInfo, OverlayToast, OverlayToastActionDetail, P2OverlayBridge } from './overlayTypes'

// window.p2overlay üzerinde tipli sarmalayıcı. Köprü yoksa (tarayıcıda ?overlay=1 ya da ?ui-demo=…) veya
// ?mock=1 verilmişse sahte köprü devreye girer: panel görünür, oturum "Demo", gönderenler konsola yazar.
// Tüm sahte olaylar `mockToast()` / `mockVisible()` / `mockPerf()` ile tetiklenebilir (showroom bunları kullanır).

type Handler<T> = (value: T) => void
function channel<T>() {
  const set = new Set<Handler<T>>()
  return {
    on(handler: Handler<T>) { set.add(handler); return () => { set.delete(handler) } },
    emit(value: T) { set.forEach(fn => fn(value)) },
  }
}

const query = (() => { try { return new URLSearchParams(window.location.search) } catch { return new URLSearchParams() } })()
const native: P2OverlayBridge | undefined = (window as any).p2overlay
export const MOCK_QUERY = query.get('mock') === '1'
export const MOCK = !native || MOCK_QUERY

const mockVisibleCh = channel<boolean>()
const mockToastCh = channel<OverlayToast>()
const mockDismissCh = channel<string>()
const mockPerfCh = channel<OverlayPerf>()
const mockFocusCh = channel<boolean>()
const mockEventCh = channel<OverlayEvent>()
const mockPushCh = channel<OverlayPush>()
const mockHotkeyCh = channel<OverlayHotkeyEvent>()
const mockShotCh = channel<OverlayScreenshot>()
let mockVisibleState = true
let mockToastSeq = 0
const mockSession: OverlaySessionInfo = { pid: 0, projectId: query.get('project') || 'demo', projectName: query.get('projectName') || 'Demo' }

const mockBridge: P2OverlayBridge = {
  session: () => mockSession,
  onVisible: handler => { handler(mockVisibleState); return mockVisibleCh.on(handler) },
  setVisible: visible => { mockVisibleState = visible; mockVisibleCh.emit(visible) },
  drawState: state => { if (import.meta.env.DEV) console.debug('[overlay mock] drawState', state) },
  onFocus: mockFocusCh.on,
  onPerf: mockPerfCh.on,
  onHotkey: mockHotkeyCh.on,
  onScreenshot: mockShotCh.on,
  onEvent: mockEventCh.on,
  onToast: mockToastCh.on,
  onToastDismiss: mockDismissCh.on,
  toastAction: detail => console.debug('[overlay mock] toastAction', detail),
  screenshot: async () => {
    mockToast({ kind: 'screenshot', title: 'Ekran görüntüsü alındı', text: 'Resimler\\P2\\Demo\\2026-10-23_18-04-12.png', actions: [{ id: 'share', label: 'Paylaş', primary: true }, { id: 'folder', label: 'Klasörü aç' }] })
    return { ok: true }
  },
  setZoom: () => {},
  openInMain: detail => console.debug('[overlay mock] openInMain', detail),
  send: (topic, payload) => console.debug('[overlay mock] send', topic, payload),
  onPush: mockPushCh.on,
  onCallState: () => () => {},
  callAction: action => console.debug('[overlay mock] callAction', action),
}

export const bridge: P2OverlayBridge = MOCK ? mockBridge : native!

// ---- Sahte tetikleyiciler (yalnız MOCK; gerçek köprüde hiçbir şey yapmaz) ----
export function mockToast(toast: Omit<OverlayToast, 'id'> & { id?: OverlayToast['id'] }) {
  if (!MOCK) return
  mockToastCh.emit({ ...toast, id: toast.id ?? `mock-${++mockToastSeq}` })
}
export function mockDismiss(tag: string) { if (MOCK) mockDismissCh.emit(tag) }
export function mockVisible(visible: boolean) { if (MOCK) mockBridge.setVisible(visible) }
export function mockPerf(perf: OverlayPerf) { if (MOCK) mockPerfCh.emit(perf) }
export function mockFocus(focused: boolean) { if (MOCK) mockFocusCh.emit(focused) }
export function mockEvent(event: OverlayEvent) { if (MOCK) mockEventCh.emit(event) }
export function mockPush(push: OverlayPush) { if (MOCK) mockPushCh.emit(push) }

// Tarayıcı demosunda Shift+2 paneli açıp kapatır (oyunda bunu DLL yapar).
if (MOCK && typeof window !== 'undefined') {
  window.addEventListener('keydown', event => {
    if (event.shiftKey && event.code === 'Digit2' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLTextAreaElement)) { event.preventDefault(); mockBridge.setVisible(!mockVisibleState) }
  })
}

// ---- Görünürlük / odak depoları ----
let visibleState = MOCK ? mockVisibleState : false
let focusedState = true
const visibleListeners = new Set<() => void>()
const focusListeners = new Set<() => void>()
let wired = false
function wire() {
  if (wired) return
  wired = true
  bridge.onVisible(value => { if (value === visibleState) return; visibleState = value; visibleListeners.forEach(fn => fn()) })
  bridge.onFocus(value => { focusedState = value; focusListeners.forEach(fn => fn()) })
}
export function getVisible() { wire(); return visibleState }
export function useOverlayVisible() {
  wire()
  return useSyncExternalStore(fn => { visibleListeners.add(fn); return () => { visibleListeners.delete(fn) } }, () => visibleState, () => visibleState)
}
export function useOverlayFocus() {
  wire()
  return useSyncExternalStore(fn => { focusListeners.add(fn); return () => { focusListeners.delete(fn) } }, () => focusedState, () => focusedState)
}
export function getSession(): OverlaySessionInfo {
  return bridge.session() || mockSession
}

// Kısa yardımcılar (çağıranların her yerde null denetimi yapmaması için)
export const overlay = {
  setVisible: (visible: boolean) => bridge.setVisible(visible),
  drawState: (state: OverlayDrawState) => bridge.drawState(state),
  toastAction: (detail: OverlayToastActionDetail) => bridge.toastAction(detail),
  screenshot: () => bridge.screenshot(),
  openInMain: (detail: OverlayOpenInApp) => bridge.openInMain(detail),
  send: (topic: OverlaySendTopic, payload: unknown) => bridge.send(topic, payload),
}

// Tarayıcı/önizleme betikleri için (yalnız sahte köprü): window.__p2overlayMock.toast({...}) vb.
if (MOCK && typeof window !== 'undefined') (window as any).__p2overlayMock = { toast: mockToast, dismiss: mockDismiss, visible: mockVisible, perf: mockPerf, focus: mockFocus, event: mockEvent, push: mockPush }
