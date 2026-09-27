import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'

// Kısa bildirimler. Her yerden `window.dispatchEvent(new CustomEvent('p2:toast',
// {detail:'metin'}))` ile çağrılır; indirme gibi sessizce başarısız olabilen
// işlerin kullanıcıya görünmesi için.
export default function Toasts(){
  const[items,setItems]=useState<{id:number;text:string;tone:'ok'|'error'}[]>([])
  useEffect(()=>{
    let last=0
    const show=(event:Event)=>{
      // detail ya düz metin ya da {text, tone} olur.
      const detail=(event as CustomEvent).detail
      const text=String((typeof detail==='object'&&detail?detail.text:detail)||'').trim()
      const tone:'ok'|'error'=typeof detail==='object'&&detail?.tone==='ok'?'ok':'error'
      if(!text)return
      const id=++last
      setItems(list=>[...list.slice(-2),{id,text,tone}])
      window.setTimeout(()=>setItems(list=>list.filter(item=>item.id!==id)),7000)
    }
    window.addEventListener('p2:toast',show)
    return()=>window.removeEventListener('p2:toast',show)
  },[])
  if(!items.length)return null
  // Gövdeye taşınır: uygulama ızgarasının ".app-content > *" kuralı buna da
  // height:100% verip bildirimi ekran boyu yapıyordu.
  return createPortal(<div className="app-toasts">
    {items.map(item=><div key={item.id} className={`app-toast ${item.tone}`} role="status">
      <span>{item.text}</span>
      <button type="button" aria-label="Kapat" onClick={()=>setItems(list=>list.filter(entry=>entry.id!==item.id))}>×</button>
    </div>)}
  </div>,document.body)
}
