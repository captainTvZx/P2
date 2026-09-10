import { RefObject, useEffect, useRef } from 'react'

// Sohbet akışını alta yapıştırır. Görsel, video ve PDF önizlemeleri mesaj çizildikten
// sonra yüklendiği için akışın yüksekliği sonradan büyüyor; yalnızca mesaj sayısına
// bakan tek seferlik bir kaydırma bu yüzden ortada kalıyordu. Burada medya yüklendikçe
// ve yeni düğüm eklendikçe, kullanıcı hâlâ altta duruyorsa yeniden aşağı iniyoruz.
export function useStickyScroll(feed:RefObject<HTMLElement|null>,keys:unknown[]){
  const stuck=useRef(true)
  useEffect(()=>{
    const node=feed.current
    if(!node)return
    const toBottom=()=>{node.scrollTop=node.scrollHeight}
    const follow=()=>{if(stuck.current)requestAnimationFrame(toBottom)}
    const onScroll=()=>{stuck.current=node.scrollHeight-node.scrollTop-node.clientHeight<120}
    node.addEventListener('scroll',onScroll,{passive:true})
    // load ve loadedmetadata baloncuk yapmaz; yakalama evresinde dinlenmeleri gerekir.
    node.addEventListener('load',follow,true)
    node.addEventListener('loadedmetadata',follow,true)
    const sizes=new ResizeObserver(follow)
    for(const child of Array.from(node.children))sizes.observe(child)
    const added=new MutationObserver(records=>{
      for(const record of records)for(const item of Array.from(record.addedNodes))if(item instanceof Element)sizes.observe(item)
      follow()
    })
    added.observe(node,{childList:true})
    return()=>{
      node.removeEventListener('scroll',onScroll)
      node.removeEventListener('load',follow,true)
      node.removeEventListener('loadedmetadata',follow,true)
      sizes.disconnect()
      added.disconnect()
    }
  },[feed])
  // Yeni mesajda ve sohbet değişiminde her durumda en alta in. Kod blokları,
  // görseller ve videolar birkaç kare boyunca yüksekliği büyütmeye devam ettiği
  // için tek bir kaydırma yetmiyor; kısa bir süre alta yapışık kalınıyor.
  useEffect(()=>{
    stuck.current=true
    const node=feed.current
    if(!node)return
    let frame=0
    const until=performance.now()+900
    const settle=()=>{
      if(!stuck.current)return
      node.scrollTop=node.scrollHeight
      if(performance.now()<until)frame=requestAnimationFrame(settle)
    }
    frame=requestAnimationFrame(settle)
    return()=>cancelAnimationFrame(frame)
  },keys)
}
