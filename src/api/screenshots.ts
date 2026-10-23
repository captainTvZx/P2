import { request } from './client'
import type { UploadedFile } from '../attachments'

// Proje ekran görüntüsü galerisi REST sarmalayıcısı (server/src/screenshots.ts ile birebir).
// Yükleme iki adımlı: önce mevcut POST /uploads (base64 JSON) ile JPEG yüklenir, sonra galeri satırı açılır.
export type ScreenshotUser={id:string;username?:string;displayName:string;avatarUrl?:string|null}
export type ProjectScreenshot={
  id:string
  projectId:string
  userId:string
  uploadPath:string          // "/uploads/<id>.jpg"
  width:number
  height:number
  size:number
  caption:string|null
  releaseId:string|null
  takenAt:string
  createdAt:string
  user:ScreenshotUser
}
export type CreateScreenshotInput={uploadPath:string;width:number;height:number;size?:number;caption?:string;releaseId?:string;takenAt?:string}
export type ScreenshotAddedEvent={projectId:string;screenshot:ProjectScreenshot}
export type ScreenshotUpdatedEvent={projectId:string;screenshot:ProjectScreenshot}
export type ScreenshotDeletedEvent={projectId:string;id:string}

// İstemci üst sınırı: 25 MB JPEG (host q90 verir; 4K'da 3–6 MB beklenir).
export const SCREENSHOT_UPLOAD_LIMIT=25*1024*1024
export const screenshotsRoot=(organizationId:string,projectId:string)=>`/organizations/${encodeURIComponent(organizationId)}/projects/${encodeURIComponent(projectId)}/screenshots`

export function listScreenshots(token:string,organizationId:string,projectId:string,options:{before?:string;limit?:number;userId?:string}={}){
  const query=new URLSearchParams()
  if(options.before)query.set('before',options.before)
  if(options.limit)query.set('limit',String(options.limit))
  if(options.userId)query.set('userId',options.userId)
  const suffix=query.toString()
  return request<ProjectScreenshot[]>(`${screenshotsRoot(organizationId,projectId)}${suffix?`?${suffix}`:''}`,token)
}
export function createScreenshot(token:string,organizationId:string,projectId:string,input:CreateScreenshotInput){
  return request<ProjectScreenshot>(screenshotsRoot(organizationId,projectId),token,{method:'POST',body:JSON.stringify(input)})
}
export function updateScreenshot(token:string,organizationId:string,projectId:string,id:string,patch:{caption:string}){
  return request<ProjectScreenshot>(`${screenshotsRoot(organizationId,projectId)}/${encodeURIComponent(id)}`,token,{method:'PATCH',body:JSON.stringify(patch)})
}
export function removeScreenshot(token:string,organizationId:string,projectId:string,id:string){
  return request<{ok:boolean}>(`${screenshotsRoot(organizationId,projectId)}/${encodeURIComponent(id)}`,token,{method:'DELETE'})
}

// base64 veri adresinin ham bayt uzunluğu (yüklemeden önce sınır denetimi için).
export function dataUrlByteSize(dataUrl:string){
  const at=dataUrl.indexOf(',')
  const body=at>=0?dataUrl.slice(at+1):dataUrl
  const padding=body.endsWith('==')?2:body.endsWith('=')?1:0
  return Math.max(0,Math.floor(body.length*3/4)-padding)
}

// JPEG veri adresini mevcut /uploads sözleşmesiyle yükler; galeriye kaydetmek için uploadPath döner.
export async function uploadScreenshot(token:string,input:{jpegDataUrl:string;name:string}):Promise<{uploadPath:string;size:number;type:string;name:string}>{
  if(!/^data:image\/jpeg;base64,/.test(input.jpegDataUrl))throw {code:'SCREENSHOT_DATA',message:'Ekran görüntüsü verisi hazır değil.'}
  if(dataUrlByteSize(input.jpegDataUrl)>SCREENSHOT_UPLOAD_LIMIT)throw {code:'FILE_SIZE',message:'Ekran görüntüsü 25 MB sınırını aşıyor.'}
  const data=await request<{name:string;type:string;size:number;url:string}>('/uploads',token,{method:'POST',body:JSON.stringify({name:input.name,type:'image/jpeg',data:input.jpegDataUrl})})
  return {uploadPath:data.url,size:data.size,type:data.type,name:data.name}
}

// Galeri satırını sohbet eki biçimine çevirir (attachmentMarker / görüntüleyici için).
export function screenshotFile(shot:Pick<ProjectScreenshot,'uploadPath'|'size'|'takenAt'|'caption'>):UploadedFile{
  const stamp=new Date(shot.takenAt)
  const name=Number.isNaN(stamp.getTime())?'ekran-goruntusu.jpg':`${stamp.toISOString().slice(0,19).replace('T','_').replace(/:/g,'-')}.jpg`
  return {name,type:'image/jpeg',size:shot.size||0,url:shot.uploadPath}
}
