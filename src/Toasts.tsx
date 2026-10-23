import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import Avatar from './ui/Avatar'
import Logo from './ui/Logo'

// Kısa bildirimler (masaüstü). Oyun içi overlay ile aynı kart: lacivert cam, P2 logosu, sağdan elastik
// giriş, süre çubuğu. Her yerden `pushToast(...)` ya da `window.dispatchEvent(new CustomEvent('p2:toast',
// {detail}))` ile çağrılır. detail: düz metin | {text, tone?, title?} | Steam tarzı kişi kartı
// {kind:'play', user, title, text, onClick}. Düz metnin varsayılan tonu "error"dur (sessizce başarısız
// olabilen işler kullanıcıya görünsün diye).
export type ToastDetail = string | { text: string; tone?: 'ok' | 'error' | 'info'; title?: string } | { kind: 'play'; user: { id: string; displayName: string; avatarUrl?: string | null }; title: string; text: string; onClick?: () => void }
type Tone = 'ok' | 'error' | 'info' | 'play'
type Item = { id: number; tone: Tone; text: string; title?: string; user?: { id: string; displayName: string; avatarUrl?: string | null }; onClick?: () => void; leaving?: boolean }

const KICKER: Record<Tone, string> = { ok: 'Tamam', error: 'Hata', info: 'P2', play: 'Oyunda' }
const OUT_MS = 360

export function pushToast(detail: ToastDetail) { window.dispatchEvent(new CustomEvent('p2:toast', { detail })) }

export default function Toasts() {
  const [items, setItems] = useState<Item[]>([])
  useEffect(() => {
    let last = 0
    const timers = new Map<number, number[]>()
    const remove = (id: number) => { setItems(list => list.filter(entry => entry.id !== id)); timers.get(id)?.forEach(clearTimeout); timers.delete(id) }
    const dismiss = (id: number) => {
      setItems(list => list.map(entry => entry.id === id ? { ...entry, leaving: true } : entry))
      timers.set(id, [...(timers.get(id) || []), window.setTimeout(() => remove(id), OUT_MS)])
    }
    const show = (event: Event) => {
      const detail = (event as CustomEvent).detail as ToastDetail
      let item: Omit<Item, 'id'> | null = null
      if (typeof detail === 'object' && detail && 'kind' in detail && detail.kind === 'play') item = { tone: 'play', text: detail.text, title: detail.title, user: detail.user, onClick: detail.onClick }
      else {
        const source = typeof detail === 'object' && detail ? (detail as { text: string; tone?: Tone; title?: string }) : { text: String(detail ?? '') }
        const text = String(source.text || '').trim()
        if (!text) return
        item = { tone: source.tone === 'ok' || source.tone === 'info' ? source.tone : 'error', text, title: source.title }
      }
      const id = ++last
      setItems(list => [...list.slice(-2), { id, ...item! }])
      const life = item.tone === 'play' ? 9000 : 7000
      timers.set(id, [window.setTimeout(() => dismiss(id), life - OUT_MS)])
    }
    const close = (event: Event) => dismiss((event as CustomEvent).detail as number)
    window.addEventListener('p2:toast', show)
    window.addEventListener('p2:toast-close', close)
    return () => { window.removeEventListener('p2:toast', show); window.removeEventListener('p2:toast-close', close); timers.forEach(list => list.forEach(clearTimeout)) }
  }, [])
  if (!items.length) return null
  const closeOne = (id: number) => window.dispatchEvent(new CustomEvent('p2:toast-close', { detail: id }))
  // Gövdeye taşınır: uygulama ızgarasının ".app-content > *" kuralı buna da height:100% verirdi.
  return createPortal(<div className="p2-toasts" aria-live="polite">
    {items.map(item => <div key={item.id} role="status" className={`p2-toast ${item.tone}${item.leaving ? ' leaving' : ''}${item.onClick ? ' clickable' : ''}`} onClick={item.onClick ? () => { item.onClick?.(); closeOne(item.id) } : undefined}>
      {item.tone === 'play' && item.user ? <Avatar id={item.user.id} name={item.user.displayName} url={item.user.avatarUrl} size="lg" /> : <Logo size="md" />}
      <div className="p2-toast-body">
        <span className="p2-kicker">{KICKER[item.tone]}</span>
        {item.title ? <><b>{item.title}</b><small>{item.text}</small></> : <b title={item.text} style={{ whiteSpace: 'normal' }}>{item.text}</b>}
      </div>
      <button type="button" className="p2-btn icon ghost sm p2-toast-close" aria-label="Kapat" onClick={event => { event.stopPropagation(); closeOne(item.id) }}>×</button>
      <i className="p2-toast-bar" style={{ animationDuration: `${(item.tone === 'play' ? 9000 : 7000) - OUT_MS}ms` }} />
    </div>)}
  </div>, document.body)
}
