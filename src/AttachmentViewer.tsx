import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import * as pdfjs from 'pdfjs-dist'
import pdfWorker from 'pdfjs-dist/build/pdf.worker.mjs?url'
import { formatSize, type UploadedFile } from './attachments'

pdfjs.GlobalWorkerOptions.workerSrc=pdfWorker

export async function downloadAttachment(file:UploadedFile){
  const response=await fetch(file.url)
  if(!response.ok)throw new Error('Dosya indirilemedi.')
  const url=URL.createObjectURL(await response.blob()),anchor=document.createElement('a')
  anchor.href=url;anchor.download=file.name;document.body.appendChild(anchor);anchor.click();anchor.remove()
  setTimeout(()=>URL.revokeObjectURL(url),1000)
}

function PdfCanvas({document,page,scale,rotation}:{document:any;page:number;scale:number;rotation:number}){
  const canvas=useRef<HTMLCanvasElement>(null)
  useEffect(()=>{let cancelled=false,task:any;(async()=>{const pdfPage=await document.getPage(page);if(cancelled)return;const viewport=pdfPage.getViewport({scale,rotation}),element=canvas.current;if(!element)return;const ratio=window.devicePixelRatio||1;element.width=Math.floor(viewport.width*ratio);element.height=Math.floor(viewport.height*ratio);element.style.width=`${viewport.width}px`;element.style.height=`${viewport.height}px`;const context=element.getContext('2d');if(!context)return;task=pdfPage.render({canvasContext:context,viewport,transform:ratio===1?undefined:[ratio,0,0,ratio,0,0]});await task.promise})().catch(()=>{});return()=>{cancelled=true;task?.cancel?.()}},[document,page,scale,rotation])
  return <canvas ref={canvas}/>
}

function PdfViewer({file}:{file:UploadedFile}){
  const[document,setDocument]=useState<any>(null),[page,setPage]=useState(1),[scale,setScale]=useState(1),[rotation,setRotation]=useState(0),[error,setError]=useState('')
  useEffect(()=>{let active=true,task:any;(async()=>{const bytes=await fetch(file.url).then(response=>response.arrayBuffer());task=pdfjs.getDocument({data:bytes});const loaded=await task.promise;if(active)setDocument(loaded)})().catch(()=>active&&setError('PDF açılamadı.'));return()=>{active=false;task?.destroy?.()}},[file.url])
  if(error)return <p className="attachment-viewer-state">{error}</p>
  if(!document)return <p className="attachment-viewer-state">PDF hazırlanıyor…</p>
  return <><div className="pdf-toolbar"><button onClick={()=>setPage(value=>Math.max(1,value-1))} disabled={page===1}>‹</button><label><input value={page} onChange={event=>setPage(Math.min(document.numPages,Math.max(1,Number(event.target.value)||1)))}/> / {document.numPages}</label><button onClick={()=>setPage(value=>Math.min(document.numPages,value+1))} disabled={page===document.numPages}>›</button><i/><button onClick={()=>setScale(value=>Math.max(.5,value-.15))}>−</button><span>{Math.round(scale*100)}%</span><button onClick={()=>setScale(value=>Math.min(2.5,value+.15))}>+</button><button onClick={()=>setRotation(value=>(value+90)%360)}>↻</button></div><div className="pdf-stage"><PdfCanvas document={document} page={page} scale={scale} rotation={rotation}/></div></>
}

function TextViewer({file}:{file:UploadedFile}){
  const[text,setText]=useState(''),[error,setError]=useState('')
  useEffect(()=>{let active=true;fetch(file.url).then(response=>response.text()).then(value=>active&&setText(value)).catch(()=>active&&setError('Metin açılamadı.'));return()=>{active=false}},[file.url])
  return error?<p className="attachment-viewer-state">{error}</p>:<pre className="attachment-text-full">{text||'Metin yükleniyor…'}</pre>
}

export default function AttachmentViewer({file,onClose}:{file:UploadedFile;onClose:()=>void}){
  useEffect(()=>{const close=(event:KeyboardEvent)=>{if(event.key==='Escape')onClose()};window.addEventListener('keydown',close);return()=>window.removeEventListener('keydown',close)},[onClose])
  return createPortal(<div className="attachment-viewer-layer" onMouseDown={onClose}><section className="attachment-viewer" onMouseDown={event=>event.stopPropagation()}><header><div><b>{file.name}</b><small>{formatSize(file.size)}</small></div><span><button onClick={()=>void downloadAttachment(file)}>İndir</button><button className="viewer-close" onClick={onClose} aria-label="Kapat">×</button></span></header><main>{file.type==='application/pdf'?<PdfViewer file={file}/>:file.type.startsWith('image/')?<img className="attachment-image-full" src={file.url} alt={file.name}/>:<TextViewer file={file}/>}</main></section></div>,document.body)
}
