import { useEffect, useRef } from 'react'

// Sağdan açılan paneller (profil, görev, etkinlik, thread) dışarı tıklanınca
// kapanır. Portal ile gövdeye çizilen katmanlar — menüler, emoji seçici,
// pencereler, bildirimler — panelin "dışı" sayılmaz; yoksa panelden açılan bir
// menüye tıklamak paneli de kapatırdı.
const DIS_SAYILMAZ = '.message-menu-layer,.message-context-menu,.reaction-picker-popup,.share-layer,.settings-layer,.avatar-crop-layer,.task-center-layer,.app-toasts,.p2-modal-layer,.p2-menu-layer,.p2-toasts,.attachment-viewer-layer,.action-modal-layer,.channel-context-layer,.emoji-picker-popover,.task-emoji-popover,.rail-profile-layer,.incoming-layer,.call-dock,.group-add-menu,.search-toggle,.rail-action.activity,.board-menu-toggle'

export function useDismissOutside<T extends HTMLElement>(onClose:()=>void,enabled=true){
  const ref=useRef<T|null>(null)
  useEffect(()=>{
    if(!enabled)return
    const disari=(event:PointerEvent)=>{
      const hedef=event.target as HTMLElement|null
      const panel=ref.current
      if(!hedef||!panel||panel.contains(hedef))return
      if(hedef.closest(DIS_SAYILMAZ))return
      onClose()
    }
    // Yakalama fazı: alttaki bileşen olayı yutsa da kapanma çalışır.
    document.addEventListener('pointerdown',disari,true)
    return()=>document.removeEventListener('pointerdown',disari,true)
  },[enabled,onClose])
  return ref
}
