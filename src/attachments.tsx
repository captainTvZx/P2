import { ChangeEvent, useRef, useState } from 'react'
import { API_BASE } from './apiBase'
import { ComposerIcon } from './ComposerIcons'

export type UploadedFile={name:string;type:string;size:number;url:string}
const marker=(file:UploadedFile)=>`[[p2-file|${encodeURIComponent(file.name)}|${encodeURIComponent(file.type)}|${file.size}|${file.url}]]`
const fileMarker=/^\[\[p2-file\|([^|]+)\|([^|]+)\|(\d+)\|(https?:\/\/[^\]]+)\]\]$/
export function attachmentsFromContent(content:string){return content.split('\n').flatMap(line=>{const match=fileMarker.exec(line.trim());return match?[{name:decodeURIComponent(match[1]),type:decodeURIComponent(match[2]),size:Number(match[3]),url:match[4]} satisfies UploadedFile]:[]})}
const readData=(file:File)=>new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(reader.error);reader.readAsDataURL(file)})

export function useAttachments(token:string){
  const input=useRef<HTMLInputElement>(null)
  const[files,setFiles]=useState<UploadedFile[]>([])
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  async function add(items:File[]|FileList){
    const selected=Array.from(items)
    if(!selected.length)return
    setBusy(true);setError('')
    try{
      const uploaded:UploadedFile[]=[]
      for(const file of selected){
        const response=await fetch(API_BASE+'/uploads',{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`},body:JSON.stringify({name:file.name,type:file.type||'application/octet-stream',data:await readData(file)})})
        const data=await response.json().catch(()=>({}))
        if(!response.ok)throw new Error(data.message||`${file.name} yüklenemedi.`)
        uploaded.push({...data,url:API_BASE+data.url})
      }
      setFiles(current=>[...current,...uploaded])
    }catch(problem:any){setError(problem?.message||'Dosya yüklenemedi.')}
    finally{setBusy(false)}
  }
  function appendTo(content:string){return [content.trim(),...files.map(marker)].filter(Boolean).join('\n')}
  function clear(){setFiles([]);setError('')}
  return {files,busy,error,input,add,appendTo,clear,remove:(index:number)=>setFiles(current=>current.filter((_,i)=>i!==index))}
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
      {state.files.map((file,index)=><div key={file.url}><span>{file.type.startsWith('image/')?'▧':file.type.startsWith('video/')?'▶':file.type==='application/pdf'?'PDF':'▤'}</span><b>{file.name}</b><small>{formatSize(file.size)}</small><button type="button" onClick={()=>state.remove(index)}>×</button></div>)}
      {state.busy&&<p>Dosya yükleniyor…</p>}{state.error&&<p className="error">{state.error}</p>}
    </div>:null
}

export function formatSize(size:number){if(size<1024)return `${size} B`;if(size<1024*1024)return `${(size/1024).toFixed(1)} KB`;return `${(size/1024/1024).toFixed(1)} MB`}
