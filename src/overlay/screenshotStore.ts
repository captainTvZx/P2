import { useSyncExternalStore } from 'react'
import { bridge } from './overlayBridge'
import type { OverlayScreenshot } from './overlayTypes'

// Son çekilen ekran görüntüleri (yalnız bu overlay penceresinin belleği). Host her çekimde
// `onScreenshot(shot)` ile PNG yolunu ve yükleme için JPEG (q90) veri adresini verir; toast'taki
// "Paylaş" eylemi pencereyi `{shotId}` hedefiyle açınca görüntü buradan bulunur. En çok MAX_RECENT
// kayıt tutulur (JPEG'ler bellekte; 4K'da ~3–6 MB/kayıt).

export type RecentShotUpload={uploadPath:string;size:number;screenshotId:string|null}
export type RecentShot=OverlayScreenshot&{at:number;upload?:RecentShotUpload;uploading?:boolean}
export const MAX_RECENT=12

let shots:RecentShot[]=[]
const listeners=new Set<()=>void>()
let started=false
function emit(){listeners.forEach(fn=>fn())}

export function recordShot(shot:OverlayScreenshot){
  if(!shot||typeof shot.id!=='number'||!shot.path)return
  const existing=shots.find(item=>item.id===shot.id)
  const next:RecentShot={...existing,...shot,at:existing?.at||Date.now()}
  shots=[next,...shots.filter(item=>item.id!==shot.id)].slice(0,MAX_RECENT)
  emit()
}
export function patchShot(id:number,patch:Partial<RecentShot>){
  if(!shots.some(item=>item.id===id))return
  shots=shots.map(item=>item.id===id?{...item,...patch}:item)
  emit()
}
export function markUploaded(id:number,upload:RecentShotUpload){patchShot(id,{upload,uploading:false})}
export function getRecentShot(id:number){return shots.find(item=>item.id===id)||null}
export function getRecentShots(){return shots}
export function clearRecentShots(){shots=[];emit()}

export function startScreenshotStore(){
  if(started)return
  started=true
  bridge.onScreenshot(shot=>recordShot(shot))
}
export function useRecentShots(){
  startScreenshotStore()
  return useSyncExternalStore(fn=>{listeners.add(fn);return()=>{listeners.delete(fn)}},getRecentShots,getRecentShots)
}
// Pencere açılmadan önce çekilen görüntüler de yakalansın: modül yüklenir yüklenmez dinlemeye başlar
// (registry araç bileşenlerini baştan içe aktardığından bu, overlay açılışında olur).
if(typeof window!=='undefined')startScreenshotStore()
