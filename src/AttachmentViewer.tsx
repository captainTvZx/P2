import { PointerEvent as ReactPointerEvent, WheelEvent, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import * as pdfjs from 'pdfjs-dist'
import pdfWorker from 'pdfjs-dist/build/pdf.worker.mjs?url'
import { attachmentUrl, formatSize, type UploadedFile } from './attachments'

pdfjs.GlobalWorkerOptions.workerSrc=pdfWorker

// Masaüstünde indirme, ana süreçteki kaydetme penceresinden geçer: kullanıcı
// dosyayı nereye koyacağını görür ve hata olursa öğrenir. Tarayıcıda blob yolu
// kullanılır.
export async function downloadAttachment(file:UploadedFile,saveAs=false){
  const url=attachmentUrl(file.url)
  try{
    if(window.p2?.files){
      const done=await window.p2.files.saveAs({url,name:file.name})
      if(done===false&&!saveAs)return   // kullanıcı vazgeçti
      if(done)return
      throw new Error('Dosya sunucudan alınamadı.')
    }
    const response=await fetch(url)
    if(!response.ok)throw new Error(`Sunucu ${response.status} döndü.`)
    const blob=URL.createObjectURL(await response.blob()),anchor=document.createElement('a')
    anchor.href=blob;anchor.download=file.name;document.body.appendChild(anchor);anchor.click();anchor.remove()
    setTimeout(()=>URL.revokeObjectURL(blob),1000)
  }catch(failure:any){
    window.dispatchEvent(new CustomEvent('p2:toast',{detail:`“${file.name}” indirilemedi. ${failure?.message||''}`.trim()}))
  }
}

function PdfCanvas({document,page,scale,rotation}:{document:any;page:number;scale:number;rotation:number}){const canvas=useRef<HTMLCanvasElement>(null);useEffect(()=>{let cancelled=false,task:any;(async()=>{const pdfPage=await document.getPage(page);if(cancelled)return;const viewport=pdfPage.getViewport({scale,rotation}),element=canvas.current;if(!element)return;const ratio=window.devicePixelRatio||1;element.width=Math.floor(viewport.width*ratio);element.height=Math.floor(viewport.height*ratio);element.style.width=`${viewport.width}px`;element.style.height=`${viewport.height}px`;const context=element.getContext('2d');if(!context)return;task=pdfPage.render({canvasContext:context,viewport,transform:ratio===1?undefined:[ratio,0,0,ratio,0,0]});await task.promise})().catch(()=>{});return()=>{cancelled=true;task?.cancel?.()}},[document,page,scale,rotation]);return <article className="pdf-page"><canvas ref={canvas}/><small>{page}</small></article>}
export function PdfThumbnail({file}:{file:UploadedFile}){const canvas=useRef<HTMLCanvasElement>(null),[error,setError]=useState(false);useEffect(()=>{let active=true,loading:any,render:any;(async()=>{const response=await fetch(file.url);if(!response.ok)throw new Error();loading=pdfjs.getDocument({data:await response.arrayBuffer()});const document=await loading.promise,page=await document.getPage(1),viewport=page.getViewport({scale:.55}),element=canvas.current;if(!active||!element)return;element.width=viewport.width;element.height=viewport.height;const context=element.getContext('2d');if(!context)return;render=page.render({canvasContext:context,viewport});await render.promise})().catch(()=>active&&setError(true));return()=>{active=false;render?.cancel?.();loading?.destroy?.()}},[file.url]);return error?<span className="pdf-thumb-error">PDF</span>:<canvas ref={canvas}/>}
function PdfViewer({file}:{file:UploadedFile}){const[document,setDocument]=useState<any>(null),[scale,setScale]=useState(1),[rotation,setRotation]=useState(0),[error,setError]=useState('');useEffect(()=>{let active=true,task:any;(async()=>{const response=await fetch(file.url);if(!response.ok)throw new Error();task=pdfjs.getDocument({data:await response.arrayBuffer()});const loaded=await task.promise;if(active)setDocument(loaded)})().catch(()=>active&&setError('PDF açılamadı.'));return()=>{active=false;task?.destroy?.()}},[file.url]);if(error)return <p className="attachment-viewer-state">{error}</p>;if(!document)return <p className="attachment-viewer-state">PDF hazırlanıyor…</p>;return <><div className="pdf-toolbar"><b>{document.numPages} sayfa</b><i/><button onClick={()=>setScale(value=>Math.max(.5,value-.15))}>−</button><span>{Math.round(scale*100)}%</span><button onClick={()=>setScale(value=>Math.min(2.5,value+.15))}>+</button><button onClick={()=>setRotation(value=>(value+90)%360)}>↻</button></div><div className="pdf-stage">{Array.from({length:document.numPages},(_,index)=><PdfCanvas key={index} document={document} page={index+1} scale={scale} rotation={rotation}/>)}</div></>}

function ImageViewer({file}:{file:UploadedFile}){const[zoom,setZoom]=useState(1),[position,setPosition]=useState({x:0,y:0}),drag=useRef<{x:number;y:number;left:number;top:number}|null>(null);function wheel(event:WheelEvent){event.preventDefault();setZoom(value=>Math.min(8,Math.max(.2,value*(event.deltaY<0?1.12:.89))))}function down(event:ReactPointerEvent){event.currentTarget.setPointerCapture(event.pointerId);drag.current={x:event.clientX,y:event.clientY,left:position.x,top:position.y}}function move(event:ReactPointerEvent){if(!drag.current)return;setPosition({x:drag.current.left+event.clientX-drag.current.x,y:drag.current.top+event.clientY-drag.current.y})}function up(event:ReactPointerEvent){event.currentTarget.releasePointerCapture(event.pointerId);drag.current=null}return <div className="attachment-image-stage" onWheel={wheel} onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={()=>{drag.current=null}} onDoubleClick={()=>{setZoom(1);setPosition({x:0,y:0})}}><img src={file.url} alt={file.name} draggable={false} style={{transform:`translate(${position.x}px,${position.y}px) scale(${zoom})`}}/><output>{Math.round(zoom*100)}%</output></div>}
function TextViewer({file}:{file:UploadedFile}){const[text,setText]=useState(''),[error,setError]=useState('');useEffect(()=>{let active=true;fetch(file.url).then(response=>response.text()).then(value=>active&&setText(value)).catch(()=>active&&setError('Metin açılamadı.'));return()=>{active=false}},[file.url]);return error?<p className="attachment-viewer-state">{error}</p>:<pre className="attachment-text-full">{text||'Metin yükleniyor…'}</pre>}
export default function AttachmentViewer({file,onClose}:{file:UploadedFile;onClose:()=>void}){useEffect(()=>{const close=(event:KeyboardEvent)=>{if(event.key==='Escape')onClose()};window.addEventListener('keydown',close);return()=>window.removeEventListener('keydown',close)},[onClose]);return createPortal(<div className="attachment-viewer-layer" onMouseDown={onClose}><section className="attachment-viewer" onMouseDown={event=>event.stopPropagation()}><header><div><b>{file.name}</b><small>{formatSize(file.size)}</small></div><span><button onClick={()=>void downloadAttachment(file)}>İndir</button><button className="viewer-close" onClick={onClose} aria-label="Kapat" title="Kapat"><i/><i/></button></span></header><main>{file.type==='application/pdf'?<PdfViewer file={file}/>:file.type.startsWith('image/')?<ImageViewer file={file}/>:<TextViewer file={file}/>}</main></section></div>,document.body)}
