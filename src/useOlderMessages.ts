import { RefObject, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'

export const OLDER_PAGE=50

// Sohbet akışında "daha eski mesajlar" sayfalaması. Kullanıcı en üste yaklaşınca
// (scrollTop<40) sunucudan `before=<görünen en eski createdAt>&limit=50` ile bir sayfa
// daha ister; sayfa geldikten sonra kaydırma konumu korunur (önceki scrollHeight farkı
// kadar aşağı kaydırılır). İstenenden az satır dönünce hasMore=false olur ve durur.
// Sohbet (key) değişince süren istek yok sayılır; aynı anda tek istek çalışır.
export function useOlderMessages<T extends {createdAt:string}>({feed,key,oldest,hasMore,fetchPage,onPage}:{
  feed:RefObject<HTMLElement|null>
  key:string|null
  oldest:T|undefined
  hasMore:boolean
  fetchPage:(before:string,limit:number)=>Promise<T[]>
  onPage:(rows:T[],hasMore:boolean)=>void
}){
  const[loading,setLoading]=useState(false)
  const busy=useRef(false)
  const keyRef=useRef(key)
  const anchor=useRef<{height:number;top:number}|null>(null)
  useEffect(()=>{keyRef.current=key;busy.current=false;anchor.current=null;setLoading(false)},[key])

  const loadOlder=useCallback(async()=>{
    if(busy.current||!hasMore||!oldest||!key)return
    const node=feed.current,requestKey=key
    busy.current=true;setLoading(true)
    try{
      const rows=await fetchPage(oldest.createdAt,OLDER_PAGE)
      if(keyRef.current!==requestKey)return
      if(node)anchor.current={height:node.scrollHeight,top:node.scrollTop}
      onPage(rows,rows.length>=OLDER_PAGE)
    }catch{/* ağ hatası: hasMore değişmez, sonraki kaydırmada yeniden denenir */}
    finally{if(keyRef.current===requestKey){busy.current=false;setLoading(false)}}
  },[feed,key,oldest,hasMore,fetchPage,onPage])

  // Başa eklenen satırlar çizildikten sonra görünen mesajlar yerinde kalsın.
  useLayoutEffect(()=>{
    const node=feed.current,saved=anchor.current
    if(!node||!saved)return
    anchor.current=null
    node.scrollTop=node.scrollHeight-saved.height+saved.top
  })

  // Akış ekranı doldurmuyorsa kaydırma olmaz; eski sayfa varsa kendiliğinden doldur.
  useEffect(()=>{
    const node=feed.current
    if(node&&hasMore&&node.scrollHeight<=node.clientHeight+1)void loadOlder()
  },[hasMore,oldest,loadOlder])

  const onScroll=useCallback((event:{currentTarget:HTMLElement})=>{if(event.currentTarget.scrollTop<40)void loadOlder()},[loadOlder])
  return{loading,onScroll,loadOlder}
}
