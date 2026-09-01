import { useEffect, useState } from 'react'

// Yeni gelen mesaj kısa süre vurgulanır; kullanıcı pencereye bakınca vurgu söner.
// Kendi gönderdiğin mesaj hiç vurgulanmaz.
export function useFreshMessage(id:string|null,mine:boolean){
  const[fresh,setFresh]=useState<string|null>(null)
  useEffect(()=>{
    if(!id||mine){setFresh(null);return}
    setFresh(id)
    let timer:number|undefined
    const arm=()=>{window.clearTimeout(timer);timer=window.setTimeout(()=>setFresh(current=>current===id?null:current),2800)}
    if(document.hasFocus()&&!document.hidden)arm()
    const focused=()=>arm()
    window.addEventListener('focus',focused)
    return()=>{window.clearTimeout(timer);window.removeEventListener('focus',focused)}
  },[id,mine])
  return fresh
}
