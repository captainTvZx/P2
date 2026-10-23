import { useEffect } from 'react'

// Panelin (thread gibi) dışında bir yere tıklanınca kapatır. Menü, emoji seçici ve
// pencere katmanları hariç tutulur; yoksa onlara tıklamak da paneli kapatırdı.
const SAFE='.thread-panel,.message-menu-layer,.emoji-picker-popover,.share-layer,.settings-layer,.incoming-layer,.call-dock,.channel-context-layer,.group-add-menu,.p2-modal-layer,.p2-menu-layer,.p2-toasts'

export function useOutsideClose(active:boolean,close:()=>void,extra=''){
  useEffect(()=>{
    if(!active)return
    const selector=extra?`${SAFE},${extra}`:SAFE
    const handler=(event:MouseEvent)=>{
      const target=event.target as HTMLElement|null
      if(target?.closest(selector))return
      close()
    }
    document.addEventListener('mousedown',handler)
    return()=>document.removeEventListener('mousedown',handler)
  },[active,close,extra])
}
