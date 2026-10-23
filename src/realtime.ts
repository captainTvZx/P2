import { useEffect, useRef } from 'react'
import { io, Socket } from 'socket.io-client'
import { API_BASE, OVERLAY_MODE, SOCKET_BASE, p2Fetch } from './apiBase'

const API=SOCKET_BASE
export type RealtimeHandlers={[event:string]:(payload:any)=>void}

// ---- Oyun içi overlay aynası ----
// Soket yalnız ana pencerede açılır. Bir overlay oturumu hazır olduğunda (overlayMirror.ts
// bayrağı kaldırır) her sunucu olayı `window.p2.gameOverlay.push('event')` ile overlay
// pencerelerine aynalanır. Overlay penceresinde ise aynı hook'lar bu aynadan beslenir;
// böylece useTyping, useMyPermissions, mesaj hook'ları değişmeden çalışır.
export const overlayMirror={enabled:false}
// Hacimli ya da yalnız ana pencereyi ilgilendiren olaylar aynalanmaz.
const MIRROR_SKIP=new Set(['call:signal','document:cursor'])
function mirror(event:string,payload:unknown){
  if(!overlayMirror.enabled||MIRROR_SKIP.has(event))return
  try{window.p2?.gameOverlay?.push('event',{event,payload})}catch{}
}

// Overlay tarafı: aynalanan olaylar tek köprü dinleyicisinden olay adına göre dağıtılır.
type Listener=(payload:any)=>void
const overlayListeners=new Map<string,Set<Listener>>()
let overlayBridgeStarted=false
function startOverlayBridge(){
  if(overlayBridgeStarted)return
  overlayBridgeStarted=true
  window.p2overlay?.onPush(push=>{
    if(push?.topic!=='event')return
    const message=push.payload as {event?:string;payload?:unknown}|undefined
    if(!message?.event)return
    overlayListeners.get(message.event)?.forEach(listener=>{try{listener(message.payload)}catch(problem){console.error('[overlay] olay işleyici hatası',message.event,problem)}})
  })
}

// Tüm pencere için tek bir bağlantı: aynı token ve kuruluş için yeniden kullanılır.
let socket:Socket|null=null,socketKey=''
function ensureSocket(token:string,organizationId:string){
  // Overlay penceresi ikinci bir soket açmaz; bu yol geliştirme sırasında yanlış kullanımı yakalar.
  if(OVERLAY_MODE)throw new Error('Overlay penceresi kendi soketini açmaz; olaylar ana pencereden aynalanır.')
  const key=`${token}|${organizationId}`
  if(socket&&socketKey===key)return socket
  socket?.disconnect()
  socketKey=key
  const next=io(`${API}/stream`,{auth:{token},transports:['websocket','polling'],reconnectionDelay:700,reconnectionDelayMax:4000})
  const join=()=>{next.emit('subscribe',{organizationId})}
  next.on('connect',join)
  next.onAny(mirror)
  // Sunucuya yeni sürüm yayınlandığında, o an bağlı olan herkes anında öğrenir.
  next.on('update:published',(payload:{version?:string})=>{
    window.dispatchEvent(new CustomEvent('p2:update-published',{detail:payload}))
  })
  next.on('disconnect',reason=>{
    if(reason!=='io server disconnect')return
    // Sunucu süresi dolmuş bir socket jetonunu reddettiyse merkezi API katmanı
    // refresh yapar; App yeni jetonla bu socket'i yeniden oluşturur.
    void p2Fetch(`${API_BASE}/organizations`,{headers:{Authorization:`Bearer ${token}`}}).catch(()=>{})
  })
  socket=next
  return next
}
export function closeRealtime(){socket?.disconnect();socket=null;socketKey=''}
export function emitRealtime(event:string,payload?:unknown){
  if(OVERLAY_MODE){window.p2overlay?.send('emit',{event,payload});return}
  socket?.emit(event,payload)
}

// Handler'lar her render'da tazelenir, dinleyiciler yeniden bağlanmaz; böylece
// olay işleyicileri güncel state'i görür ama bağlantı hep aynı kalır.
export function useRealtimeEvents(token:string,organizationId:string,handlers:RealtimeHandlers){
  const latest=useRef(handlers)
  latest.current=handlers
  const events=Object.keys(handlers).sort().join(',')
  useEffect(()=>{
    if(!token||!organizationId)return
    const names=events.split(',').filter(Boolean)
    if(OVERLAY_MODE){
      startOverlayBridge()
      const bound=names.map(event=>{
        const listener:Listener=payload=>latest.current[event]?.(payload)
        let set=overlayListeners.get(event)
        if(!set){set=new Set();overlayListeners.set(event,set)}
        set.add(listener)
        return[event,listener] as const
      })
      return()=>{bound.forEach(([event,listener])=>overlayListeners.get(event)?.delete(listener))}
    }
    const active=ensureSocket(token,organizationId)
    const bound=names.map(event=>{
      const listener=(payload:any)=>latest.current[event]?.(payload)
      active.on(event,listener)
      return [event,listener] as const
    })
    return()=>{bound.forEach(([event,listener])=>active.off(event,listener))}
  },[token,organizationId,events])
}
