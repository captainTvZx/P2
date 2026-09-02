import { useEffect, useRef, useState } from 'react'
import type { ViewerQuality } from './screenShare'

// Yayın kalitesi seçimi. Tarayıcının kendi <select> kutusu uygulamanın diline uymadığı için
// menü de diğer açılır menülerle aynı stilde çizilir.
const LEVELS:{value:ViewerQuality;label:string}[]=[
  {value:'auto',label:'Otomatik'},{value:'1080p',label:'1080p'},{value:'720p',label:'720p'},{value:'480p',label:'480p'},{value:'360p',label:'360p'},
]

export default function QualityMenu({value,onChange}:{value:ViewerQuality;onChange:(level:ViewerQuality)=>void}){
  const[open,setOpen]=useState(false)
  const root=useRef<HTMLDivElement>(null)
  useEffect(()=>{
    if(!open)return
    const close=(event:MouseEvent)=>{if(!root.current?.contains(event.target as Node))setOpen(false)}
    const escape=(event:KeyboardEvent)=>{if(event.key==='Escape')setOpen(false)}
    document.addEventListener('mousedown',close)
    document.addEventListener('keydown',escape)
    return()=>{document.removeEventListener('mousedown',close);document.removeEventListener('keydown',escape)}
  },[open])
  const active=LEVELS.find(item=>item.value===value)||LEVELS[0]
  return <div className="quality-menu" ref={root} onClick={event=>event.stopPropagation()}>
    <button type="button" className={open?'quality-trigger open':'quality-trigger'} title="Yayını hangi kalitede alacağını seç" onClick={()=>setOpen(current=>!current)}>
      {active.label}<i/>
    </button>
    {open&&<div className="quality-options">
      {LEVELS.map(item=><button type="button" key={item.value} className={item.value===value?'active':undefined} onClick={()=>{onChange(item.value);setOpen(false)}}>{item.label}</button>)}
    </div>}
  </div>
}
