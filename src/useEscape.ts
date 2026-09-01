import { useEffect } from 'react'

// Açık olan katman Esc'i kendisi yutar (yakalama fazı + stopImmediatePropagation),
// böylece tek Esc yalnızca en üstteki şeyi kapatır.
export function useEscape(active:boolean,handler:()=>void){
  useEffect(()=>{
    if(!active)return
    const escape=(e:KeyboardEvent)=>{
      if(e.key!=='Escape')return
      e.preventDefault();e.stopPropagation();e.stopImmediatePropagation()
      handler()
    }
    document.addEventListener('keydown',escape,true)
    return()=>document.removeEventListener('keydown',escape,true)
  })
}
