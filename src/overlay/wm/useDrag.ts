import { useRef, type PointerEvent as ReactPointerEvent } from 'react'

// Pointer capture ile sürükleme: pointerdown'da yakalar, her harekette dx/dy verir, bırakınca onEnd (tek depo yazımı).
// Sürükleme sırasında React state kullanılmaz; çağıran DOM'a doğrudan transform/width yazar.
export type DragHandlers = { onMove(dx: number, dy: number, event: PointerEvent): void; onEnd(dx: number, dy: number, moved: boolean): void; onStart?(event: ReactPointerEvent<HTMLElement>): boolean | void }

export function useDrag(handlers: DragHandlers) {
  const ref = useRef(handlers)
  ref.current = handlers
  const active = useRef<{ id: number; x: number; y: number; moved: boolean; el: HTMLElement } | null>(null)

  const onPointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0 || active.current) return
    if (ref.current.onStart?.(event) === false) return
    const el = event.currentTarget
    const state = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: false, el }
    active.current = state
    try { el.setPointerCapture(event.pointerId) } catch { /* sahte ortam */ }
    const move = (ev: PointerEvent) => {
      if (ev.pointerId !== state.id) return
      const dx = ev.clientX - state.x, dy = ev.clientY - state.y
      if (!state.moved && Math.abs(dx) < 2 && Math.abs(dy) < 2) return
      state.moved = true
      ref.current.onMove(dx, dy, ev)
    }
    const finish = (ev: PointerEvent) => {
      if (ev.pointerId !== state.id) return
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', finish)
      el.removeEventListener('pointercancel', finish)
      try { el.releasePointerCapture(state.id) } catch { /* zaten bırakıldı */ }
      active.current = null
      ref.current.onEnd(ev.clientX - state.x, ev.clientY - state.y, state.moved)
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', finish)
    el.addEventListener('pointercancel', finish)
    event.preventDefault()
  }
  return { onPointerDown, dragging: () => !!active.current }
}
