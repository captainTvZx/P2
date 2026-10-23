import Avatar from '../ui/Avatar'
import Logo from '../ui/Logo'
import { useSettings } from '../settings'
import type { OverlayToastKind } from './overlayTypes'
import { dismissToast, runToastAction, toastLifeMs, useOverlayToasts, TOAST_OUT_MS, type ToastItem } from './toastStore'

// Oyun içi bildirimler: masaüstü kartıyla (.p2-toast) aynı görünüm; köşe ayardan, en çok 3 kart + kuyruk,
// yapışkan kartlar (arama) eylem düğmeleriyle. Düğmeler yalnız panel açıkken tıklanabilir (oyun girdiyi DLL'e
// panel kapalıyken vermez), kapalıyken kart salt gösterimdir.
const KICKER: Record<OverlayToastKind, string> = {
  info: 'P2', success: 'P2', error: 'P2', message: 'Mesaj', mention: 'Mesaj', call: 'Arama', screenshot: 'Ekran görüntüsü',
  play: 'Oyunda', 'play-stop': 'Oyunda', task: 'Görev', release: 'Sürüm', download: 'İndirme',
}
const tone = (kind?: OverlayToastKind) => kind === 'success' ? 'ok' : kind === 'error' ? 'error' : kind === 'call' ? 'info call' : 'info'

function ToastCard({ item, interactive }: { item: ToastItem; interactive: boolean }) {
  const kind = item.kind || 'info'
  const life = toastLifeMs(item)
  return <div role="status" className={`p2-toast ${tone(item.kind)}${item.leaving ? ' leaving' : ''}${item.actions?.length ? ' has-actions' : ''}`} data-kind={kind} data-tag={item.tag}>
    {item.avatar ? <Avatar id={item.avatar.userId} name={item.avatar.name} url={item.avatar.url} size="lg" /> : <Logo size="md" />}
    <div className="p2-toast-body">
      <span className="p2-kicker">{KICKER[kind] || 'P2'}</span>
      <b title={item.title}>{item.title}</b>
      {(item.text || item.keys?.length) && <small>{item.text}{item.keys?.length ? <span className="p2-keys">{item.keys.map((key, index) => <span key={index}>{index > 0 && <i>+</i>}<kbd>{key}</kbd></span>)}</span> : null}</small>}
      {item.actions?.length ? <div className="ow-toast-actions">
        {item.actions.map(action => <button key={action.id} type="button" className={`p2-btn sm${action.primary ? ' primary' : ' secondary'}`} disabled={!interactive} onClick={() => runToastAction(item, action.id)}>{action.label}</button>)}
      </div> : null}
    </div>
    {interactive ? <button type="button" className="p2-btn icon ghost sm p2-toast-close" aria-label="Kapat" onClick={() => dismissToast(item.id)}>×</button> : <span />}
    {!item.sticky && <i className="p2-toast-bar" style={{ animationDuration: `${life - TOAST_OUT_MS}ms` }} />}
  </div>
}

export default function OverlayToasts({ interactive }: { interactive: boolean }) {
  const items = useOverlayToasts()
  const settings = useSettings()
  const corner = settings.overlayToastCorner || 'top-right'
  if (!items.length) return null
  return <div className={`p2-toasts ow-toasts corner-${corner}${interactive ? ' interactive' : ''}${settings.overlayHud ? ' with-hud' : ''}`} aria-live="polite">
    {items.map(item => <ToastCard key={item.tag ? 'tag:' + item.tag : item.seq} item={item} interactive={interactive} />)}
  </div>
}
