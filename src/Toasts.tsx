import { useEffect, useState } from 'react'

// Kısa bildirimler. Her yerden `window.dispatchEvent(new CustomEvent('p2:toast',
// {detail:'metin'}))` ile çağrılır; indirme gibi sessizce başarısız olabilen
// işlerin kullanıcıya görünmesi için.
export default function Toasts(){
  const[items,setItems]=useState<{id:number;text:string}[]>([])
  useEffect(()=>{
    let last=0
    const show=(event:Event)=>{
      const text=String((event as CustomEvent).detail||'').trim()
      if(!text)return
      const id=++last
      setItems(list=>[...list.slice(-2),{id,text}])
      window.setTimeout(()=>setItems(list=>list.filter(item=>item.id!==id)),7000)
    }
    window.addEventListener('p2:toast',show)
    return()=>window.removeEventListener('p2:toast',show)
  },[])
  if(!items.length)return null
  return <div className="app-toasts">
    {items.map(item=><div key={item.id} className="app-toast" role="status">
      <span>{item.text}</span>
      <button type="button" aria-label="Kapat" onClick={()=>setItems(list=>list.filter(entry=>entry.id!==item.id))}>×</button>
    </div>)}
  </div>
}
