import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createScreenshot, listScreenshots, removeScreenshot, updateScreenshot, type CreateScreenshotInput, type ProjectScreenshot, type ScreenshotAddedEvent, type ScreenshotDeletedEvent, type ScreenshotUpdatedEvent } from '../api/screenshots'
import { useRealtimeEvents } from '../realtime'

// Proje ekran görüntüsü galerisi: sayfalı liste (en yeni önce) + canlı `screenshot:added|updated|deleted`.
// Ana pencerede (Kütüphane sekmesi) ve overlay penceresinde (aynalanan olaylar) aynı hook çalışır.

export const SCREENSHOT_PAGE=60

export type UseScreenshotsInput={
  token:string
  organizationId:string
  projectId:string|null
  enabled?:boolean
  onError?:(message:string)=>void
}

export function sortScreenshots(list:ProjectScreenshot[]){
  return [...list].sort((a,b)=>Date.parse(b.takenAt)-Date.parse(a.takenAt)||Date.parse(b.createdAt)-Date.parse(a.createdAt))
}
export function mergeScreenshot(list:ProjectScreenshot[],shot:ProjectScreenshot){
  return sortScreenshots(list.some(item=>item.id===shot.id)?list.map(item=>item.id===shot.id?shot:item):[...list,shot])
}

export function useScreenshots(input:UseScreenshotsInput){
  const{token,organizationId,projectId}=input
  const enabled=input.enabled!==false&&Boolean(token&&organizationId&&projectId)
  const[items,setItems]=useState<ProjectScreenshot[]>([])
  const[loading,setLoading]=useState(true)
  const[loadingMore,setLoadingMore]=useState(false)
  const[hasMore,setHasMore]=useState(false)
  const[error,setError]=useState('')
  const itemsRef=useRef(items);itemsRef.current=items
  const onError=useRef(input.onError);onError.current=input.onError
  const fail=(problem:unknown,fallback:string)=>{const message=(problem as {message?:string})?.message||fallback;setError(message);onError.current?.(message)}

  const reload=useCallback(async()=>{
    if(!enabled||!projectId)return
    try{
      const list=await listScreenshots(token,organizationId,projectId,{limit:SCREENSHOT_PAGE})
      setItems(sortScreenshots(list));setHasMore(list.length>=SCREENSHOT_PAGE);setError('')
    }catch(problem){fail(problem,'Ekran görüntüleri yüklenemedi.')}
    finally{setLoading(false)}
  },[enabled,token,organizationId,projectId])

  const scopeKey=`${organizationId}|${projectId}`
  useEffect(()=>{setItems([]);setLoading(true);setHasMore(false)},[scopeKey])
  useEffect(()=>{void reload()},[reload])

  // Daha eski sayfa: görünen en eski çekimin takenAt değeri imleçtir.
  const loadMore=useCallback(async()=>{
    if(!enabled||!projectId||loadingMore)return
    const oldest=itemsRef.current[itemsRef.current.length-1]
    if(!oldest)return
    setLoadingMore(true)
    try{
      const list=await listScreenshots(token,organizationId,projectId,{before:oldest.takenAt,limit:SCREENSHOT_PAGE})
      setItems(current=>sortScreenshots([...current,...list.filter(item=>!current.some(existing=>existing.id===item.id))]))
      setHasMore(list.length>=SCREENSHOT_PAGE)
    }catch(problem){fail(problem,'Daha eski görüntüler yüklenemedi.')}
    finally{setLoadingMore(false)}
  },[enabled,token,organizationId,projectId,loadingMore])

  const add=useCallback(async(init:CreateScreenshotInput)=>{
    if(!enabled||!projectId)return null
    try{
      const shot=await createScreenshot(token,organizationId,projectId,init)
      setItems(current=>mergeScreenshot(current,shot));setError('')
      return shot
    }catch(problem){fail(problem,'Görüntü galeriye eklenemedi.');return null}
  },[enabled,token,organizationId,projectId])

  const setCaption=useCallback(async(id:string,caption:string)=>{
    if(!enabled||!projectId)return false
    const before=itemsRef.current
    setItems(current=>current.map(item=>item.id===id?{...item,caption:caption.trim()||null}:item))
    try{const shot=await updateScreenshot(token,organizationId,projectId,id,{caption});setItems(current=>mergeScreenshot(current,shot));return true}
    catch(problem){setItems(before);fail(problem,'Başlık kaydedilemedi.');return false}
  },[enabled,token,organizationId,projectId])

  const remove=useCallback(async(id:string)=>{
    if(!enabled||!projectId)return false
    const before=itemsRef.current
    setItems(current=>current.filter(item=>item.id!==id))
    try{await removeScreenshot(token,organizationId,projectId,id);return true}
    catch(problem){setItems(before);fail(problem,'Görüntü silinemedi.');return false}
  },[enabled,token,organizationId,projectId])

  useRealtimeEvents(enabled?token:'',organizationId,{
    'screenshot:added':(event:ScreenshotAddedEvent)=>{if(event?.projectId===projectId&&event.screenshot)setItems(current=>mergeScreenshot(current,event.screenshot))},
    'screenshot:updated':(event:ScreenshotUpdatedEvent)=>{if(event?.projectId===projectId&&event.screenshot)setItems(current=>current.some(item=>item.id===event.screenshot.id)?mergeScreenshot(current,event.screenshot):current)},
    'screenshot:deleted':(event:ScreenshotDeletedEvent)=>{if(event?.projectId===projectId)setItems(current=>current.filter(item=>item.id!==event.id))},
  })

  return useMemo(()=>({items,loading,loadingMore,hasMore,error,reload,loadMore,add,setCaption,remove}),[items,loading,loadingMore,hasMore,error,reload,loadMore,add,setCaption,remove])
}
export type ScreenshotsApi=ReturnType<typeof useScreenshots>
