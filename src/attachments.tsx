import { ChangeEvent, DragEvent, useRef, useState } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { ComposerIcon } from './ComposerIcons'

export type UploadedFile={name:string;type:string;size:number;url:string}
export type PreviewKind='image'|'svg'|'audio'|'video'|'pdf'|'text'|'docx'|'sheet'|'slides'|'archive'|'epub'|'font'|'binary'
const extensionOf=(name:string)=>(/\.([^.]+)$/.exec(name)?.[1]||'').toLocaleLowerCase('en-US')
export function attachmentPreviewKind(file:Pick<UploadedFile,'name'|'type'>):PreviewKind{
  const ext=extensionOf(file.name),type=(file.type||'').toLowerCase()
  if(type==='image/svg+xml'||ext==='svg')return'svg'
  if(type.startsWith('image/'))return'image'
  if(type.startsWith('audio/')||['mp3','wav','ogg','oga','opus','m4a','aac','flac'].includes(ext))return'audio'
  if(type.startsWith('video/')||['mp4','m4v','webm','ogv','mov'].includes(ext))return'video'
  if(type==='application/pdf'||ext==='pdf')return'pdf'
  if(type.includes('wordprocessingml')||ext==='docx'||ext==='odt')return'docx'
  if(type.includes('spreadsheet')||type.includes('ms-excel')||['xlsx','xls','ods'].includes(ext))return'sheet'
  if(type.includes('presentation')||['pptx','odp'].includes(ext))return'slides'
  if(type==='application/epub+zip'||ext==='epub')return'epub'
  if(type.startsWith('font/')||['ttf','otf','woff','woff2'].includes(ext))return'font'
  if(/(?:zip|rar|7z|tar|gzip|compressed)/.test(type)||['zip','rar','7z','tar','gz','tgz','bz2'].includes(ext))return'archive'
  if(type.startsWith('text/')||['txt','md','log','csv','tsv','json','xml','yaml','yml','js','jsx','ts','tsx','py','rb','rs','go','java','kt','cs','cpp','c','h','hpp','php','swift','sql','sh','ps1','bat','ini','toml','conf','css','scss','html','vue','svelte'].includes(ext))return'text'
  return'binary'
}
export function attachmentTitle(file:Pick<UploadedFile,'name'>){return file.name.replace(/\.[^.]+$/,'').trim()||file.name}
// Mesaja yalnızca "/uploads/<id>" yolu yazılır. Eskiden yükleyenin kendi API
// adresi (çoğunlukla http://127.0.0.1:4000) gömülüyordu; o mesajı açan başka
// biri kendi makinesine bakıp dosyayı bulamıyordu.
export function attachmentPath(url:string){
  try{
    // Yalnızca "/uploads/<id>" kısmı yazılır: tarayıcı modunda API "/api" altında
    // sunulduğu için tam yol yazılsaydı okunurken "/api/api/uploads/…" olurdu.
    const path=new URL(url,API_BASE).pathname
    const at=path.indexOf('/uploads/')
    return at>=0?path.slice(at):path
  }catch{ return url }
}
// Okurken herkes kendi bağlandığı sunucudan ister; eski mesajlardaki mutlak
// adresler de böylece çalışır hâle gelir.
export function attachmentUrl(value:string){
  if(!value)return value
  if(value.startsWith('/'))return API_BASE+value
  try{
    const parsed=new URL(value)
    const at=parsed.pathname.indexOf('/uploads/')
    return at>=0?API_BASE+parsed.pathname.slice(at):value
  }catch{ return value }
}
// Ek işareti dışarıya da açık: ekran görüntüsü paylaşımı/hata bildirimi aynı biçimi üretir.
export const attachmentMarker=(file:UploadedFile)=>`[[p2-file|${encodeURIComponent(file.name)}|${encodeURIComponent(file.type)}|${file.size}|${attachmentPath(file.url)}]]`
// Yol hem "/uploads/…" (yeni) hem de eski mesajlardaki tam adres olabilir.
const fileMarker=/^\[\[p2-file\|([^|]+)\|([^|]+)\|(\d+)\|((?:https?:\/\/|\/)[^\]]+)\]\]$/
export function attachmentsFromContent(content:string){return content.split('\n').flatMap(line=>{const match=fileMarker.exec(line.trim());return match?[{name:decodeURIComponent(match[1]),type:decodeURIComponent(match[2]),size:Number(match[3]),url:attachmentUrl(match[4])} satisfies UploadedFile]:[]})}
// Açıklama/mesaj metninden ek satırlarını çıkarır: editörde ham marker görünmesin.
export function withoutAttachments(content:string){
  const satirlar=content.split(/\r?\n/).filter(line=>!attachmentsFromContent(line).length)
  return satirlar.join('\n').trim()
}
const readData=(file:File)=>new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(reader.error);reader.readAsDataURL(file)})

export function useAttachments(token:string,initial:UploadedFile[]=[]){
  const input=useRef<HTMLInputElement>(null)
  // Düzenlemede mevcut ekler kuyruğa yüklü gelir; kullanıcı silebilir ya da
  // yenisini ekleyebilir.
  const[files,setFiles]=useState<UploadedFile[]>(initial)
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  async function add(items:File[]|FileList){
    const selected=Array.from(items)
    if(!selected.length)return
    setBusy(true);setError('')
    try{
      const uploaded:UploadedFile[]=[]
      for(const file of selected){
        const response=await p2Fetch(API_BASE+'/uploads',{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`},body:JSON.stringify({name:file.name,type:file.type||'application/octet-stream',data:await readData(file)})})
        const data=await response.json().catch(()=>({}))
        if(!response.ok)throw new Error(data.message||`${file.name} yüklenemedi.`)
        uploaded.push({...data,url:API_BASE+data.url})
      }
      setFiles(current=>[...current,...uploaded])
    }catch(problem:any){setError(problem?.message||'Dosya yüklenemedi.')}
    finally{setBusy(false)}
  }
  function appendTo(content:string){return [content.trim(),...files.map(attachmentMarker)].filter(Boolean).join('\n')}
  function clear(){setFiles([]);setError('')}
  return {files,busy,error,input,add,appendTo,clear,remove:(index:number)=>setFiles(current=>current.filter((_,i)=>i!==index))}
}

// Sohbetin tamamı bırakma alanıdır: dosya yalnızca yazma kutusuna değil,
// mesajların üstüne de bırakılabilir.
const carriesFiles=(event:DragEvent)=>Array.from(event.dataTransfer?.types||[]).includes('Files')
export function useFileDrop(onFiles:(files:File[])=>void,enabled=true){
  const[dragging,setDragging]=useState(false)
  const depth=useRef(0)
  const reset=()=>{depth.current=0;setDragging(false)}
  const handlers=enabled?{
    onDragEnter:(event:DragEvent)=>{if(!carriesFiles(event))return;event.preventDefault();depth.current++;setDragging(true)},
    onDragOver:(event:DragEvent)=>{if(!carriesFiles(event))return;event.preventDefault();event.dataTransfer.dropEffect='copy'},
    onDragLeave:(event:DragEvent)=>{if(!carriesFiles(event))return;depth.current=Math.max(0,depth.current-1);if(!depth.current)setDragging(false)},
    onDrop:(event:DragEvent)=>{
      // Yazma kutusu kendi bırakmasını zaten işlediyse (defaultPrevented)
      // ikinci kez eklemeyiz; yoksa dosya iki kopya olarak yükleniyordu.
      if(!carriesFiles(event)||event.defaultPrevented){reset();return}
      event.preventDefault();reset()
      const files=Array.from(event.dataTransfer.files)
      if(files.length)onFiles(files)
    },
  }:{}
  return {dragging,handlers}
}

export function DropZone({active,label}:{active:boolean;label:string}){
  if(!active)return null
  return <div className="drop-zone" aria-hidden="true"><div><span>⇪</span><b>Bırak ve gönder</b><small>{label}</small></div></div>
}

export function AttachmentPicker({state}:{state:ReturnType<typeof useAttachments>}){
  function chosen(event:ChangeEvent<HTMLInputElement>){if(event.target.files)void state.add(event.target.files);event.target.value=''}
  return <>
    <button type="button" title="Dosya ekle" aria-label="Dosya ekle" onClick={()=>state.input.current?.click()}><ComposerIcon name="plus"/></button>
    <input ref={state.input} type="file" multiple hidden onChange={chosen}/>
  </>
}
export function AttachmentQueue({state}:{state:ReturnType<typeof useAttachments>}){
  return (state.files.length>0||state.busy||state.error)?<div className="composer-attachments">
      {state.files.map((file,index)=>{const kind=attachmentPreviewKind(file);return <div key={file.url} className={`queue-item ${kind}`}>
        {kind==='image'
          ? <span className="queue-thumb"><img src={attachmentUrl(file.url)} alt=""/></span>
          : kind==='video'
            ? <span className="queue-thumb video"><video src={attachmentUrl(file.url)} muted preload="metadata"/><i>▶</i></span>
            : <span className="queue-thumb kind">{kind==='pdf'?'PDF':kind==='audio'?'SES':(file.name.split('.').pop()||'DSY').slice(0,4).toUpperCase()}</span>}
        <b>{file.name}</b><small>{formatSize(file.size)}</small>
        <button type="button" title="Kaldır" aria-label={`${file.name} ekini kaldır`} onClick={()=>state.remove(index)}>×</button>
      </div>})}
      {state.busy&&<p>Dosya yükleniyor…</p>}{state.error&&<p className="error">{state.error}</p>}
    </div>:null
}

export function formatSize(size:number){if(size<1024)return `${size} B`;if(size<1024*1024)return `${(size/1024).toFixed(1)} KB`;return `${(size/1024/1024).toFixed(1)} MB`}
