import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { attachmentMarker, formatSize, type UploadedFile } from '../../attachments'
import { request } from '../../api/client'
import { screenshotFile, uploadScreenshot, type ProjectScreenshot } from '../../api/screenshots'
import { BOARD_COLUMNS, STATUS_LABEL, createTask, listBoards, type BoardInfo, type Task, type TaskPriority, type TaskStatus } from '../../api/tasks'
import { BUG_SEVERITIES, buildBugChannelMessage, buildBugDescription, pickReportChannel, type ReportChannel } from '../../bugReport'
import { useScreenshots, type ScreenshotsApi } from '../../hooks/useScreenshots'
import { useMyPermissions } from '../../permissions'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import { ScreenshotGrid, ScreenshotLightbox, ScreenshotShareDialog, canEditScreenshot, sendShareMessage, type Notify } from '../../ProjectScreenshotsPanel'
import { useEscape } from '../../useEscape'
import { IconCapture } from '../icons'
import { clearOpenTarget, useOpenTarget } from '../openTargets'
import { overlay } from '../overlayBridge'
import { markUploaded, patchShot, useRecentShots, type RecentShot } from '../screenshotStore'
import { pushOverlayToast } from '../toastStore'
import type { OverlayToolProps } from '../overlayTypes'
import './capture.css'

// Ekran görüntüsü penceresi (overlay): çek (F12 / düğme), son çekilenler (yerel PNG + bellekteki JPEG),
// proje galerisi (sunucu), paylaş (kanal/kişi), hata bildir (görev + isteğe bağlı kanal mesajı).
// Toast'taki "Paylaş" pencereyi `{shotId}` hedefiyle açar: çekim yerel depodan bulunur, paylaşım penceresi açılır.
// Yükleme iki adımlı: POST /uploads (JPEG q90) → POST .../screenshots (galeri satırı); bir çekim yalnız bir kez yüklenir.

type OpenCaptureTarget={shotId?:number;path?:string;width?:number;height?:number;projectId?:string}
type LibraryInfo={releaseId?:string;version?:string;channel?:string}|null
type BugSubject={kind:'local';shot:RecentShot}|{kind:'gallery';shot:ProjectScreenshot}|null
type Category={id:string;name:string;channels:ReportChannel[]}

const failText=(problem:unknown,fallback:string)=>(problem as {message?:string})?.message||fallback
const baseName=(path:string)=>path.split(/[\\/]/).pop()||'ekran-goruntusu.png'
const jpegName=(shot:RecentShot)=>baseName(shot.path).replace(/\.png$/i,'')+'.jpg'
const dims=(shot:{width:number;height:number})=>`${shot.width}×${shot.height}`
let toastSeq=0
const notify:Notify=(text,tone)=>pushOverlayToast({id:`capture-${++toastSeq}`,kind:tone==='error'?'error':tone==='ok'?'success':'info',title:text,duration:3500})

function FolderIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>}
function BugIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M8 9a4 4 0 0 1 8 0v5a4 4 0 0 1-8 0z"/><path d="M12 14v5M8 11H4M20 11h-4M8 16l-3 2M16 16l3 2M9 6 7 4M15 6l2-2"/></svg>}
function ShareIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7"/><path d="M12 15V4M7.5 8.5 12 4l4.5 4.5"/></svg>}
function OpenIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/></svg>}

// ---- Son çekilen kart ----
function RecentCard({shot,canUpload,onUpload,onShare,onBug}:{shot:RecentShot;canUpload:boolean;onUpload():void;onShare():void;onBug():void}){
  const openFolder=()=>overlay.toastAction({toastId:'capture',tag:`shot:${shot.id}`,actionId:'open-folder',payload:{path:shot.path}})
  return <figure className="oc-shot">
    <div className="oc-shot-thumb">{shot.jpegDataUrl?<img src={shot.jpegDataUrl} alt="" draggable={false}/>:<span className="oc-shot-nodata">Önizleme yok</span>}</div>
    <figcaption>
      <span className="oc-shot-text"><b title={shot.path}>{baseName(shot.path)}</b><small>{dims(shot)}{shot.fallback?' · yedek yol':''}</small></span>
      {shot.upload?<span className="p2-badge success">Yüklendi</span>:shot.uploading?<span className="p2-badge muted">Yükleniyor…</span>:canUpload&&shot.jpegDataUrl?<button type="button" className="p2-btn sm ghost" onClick={onUpload}>Galeriye yükle</button>:null}
    </figcaption>
    <div className="oc-shot-actions">
      <button type="button" className="p2-btn sm icon ghost" title="Paylaş" aria-label="Paylaş" disabled={!shot.jpegDataUrl&&!shot.upload} onClick={onShare}><ShareIcon/></button>
      <button type="button" className="p2-btn sm icon ghost" title="Hata bildir" aria-label="Hata bildir" onClick={onBug}><BugIcon/></button>
      <button type="button" className="p2-btn sm icon ghost" title="Klasörü aç" aria-label="Klasörü aç" onClick={openFolder}><FolderIcon/></button>
    </div>
  </figure>
}

// ---- Hata bildir ----
type BugReportFormProps={
  token:string;organizationId:string;projectId:string;projectName:string;me:string
  subject:BugSubject;library:LibraryInfo
  canCreate:boolean;canPost:boolean
  ensureFile(shot:RecentShot):Promise<UploadedFile>
  onClose():void
}
function BugReportForm(props:BugReportFormProps){
  const{token,organizationId,projectId}=props
  const[title,setTitle]=useState(''),[description,setDescription]=useState('')
  const[priority,setPriority]=useState<TaskPriority>('MEDIUM'),[status,setStatus]=useState<TaskStatus>('TODO')
  const[boards,setBoards]=useState<BoardInfo[]>([]),[boardId,setBoardId]=useState('')
  const[assignMe,setAssignMe]=useState(true),[writeChannel,setWriteChannel]=useState(false)
  const[channel,setChannel]=useState<ReportChannel|null>(null)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  const[done,setDone]=useState<Task|null>(null)
  useEscape(true,props.onClose)

  useEffect(()=>{
    let active=true
    listBoards(token,organizationId,projectId).then(list=>{if(!active)return;setBoards(list);setBoardId(current=>current||list[0]?.id||'')}).catch(problem=>{if(active)setError(failText(problem,'Panolar yüklenemedi.'))})
    request<Category[]>(`/organizations/${encodeURIComponent(organizationId)}/projects/${encodeURIComponent(projectId)}/channels`,token).then(categories=>{if(active)setChannel(pickReportChannel(categories))}).catch(()=>{})
    return()=>{active=false}
  },[token,organizationId,projectId])

  const valid=title.trim().length>=2&&Boolean(boardId)&&props.canCreate
  async function submit(event:FormEvent){
    event.preventDefault()
    if(!valid||busy)return
    setBusy(true);setError('')
    try{
      const file=props.subject?.kind==='local'?await props.ensureFile(props.subject.shot):props.subject?.kind==='gallery'?screenshotFile(props.subject.shot):null
      const attachment=file?attachmentMarker(file):null
      const task=await createTask(token,organizationId,projectId,boardId,{
        title:title.trim(),status,priority,assigneeId:assignMe?props.me:undefined,
        description:buildBugDescription({projectName:props.projectName,version:props.library?.version,channel:props.library?.channel,releaseId:props.library?.releaseId,clientVersion:window.p2?.version,description,attachment}),
      })
      if(writeChannel&&channel&&props.canPost){
        const severityLabel=BUG_SEVERITIES.find(item=>item.value===priority)?.label||priority
        try{await sendShareMessage(token,organizationId,projectId,{kind:'channel',id:channel.id,name:channel.name},buildBugChannelMessage({title,severityLabel,taskNumber:task.number,attachment}))}
        catch(problem){notify(failText(problem,'Kanala yazılamadı.'),'error')}
      }
      pushOverlayToast({id:`bug-${task.id}`,kind:'task',title:'Hata bildirildi',text:`#${task.number} ${task.title}`,duration:5000})
      setDone(task)
    }catch(problem){setError(failText(problem,'Hata bildirilemedi.'))}
    finally{setBusy(false)}
  }

  const subjectLabel=props.subject?.kind==='local'?baseName(props.subject.shot.path):props.subject?.kind==='gallery'?(props.subject.shot.caption||dims(props.subject.shot)):''
  return <div className="p2-modal-layer oc-modal-layer" onMouseDown={props.onClose}>
    <form className="p2-modal oc-bug" onMouseDown={event=>event.stopPropagation()} onSubmit={submit} aria-label="Hata bildir">
      <div className="p2-modal-head"><div><b>Hata bildir</b><small>Görev olarak panoya düşer; ekran görüntüsü eklenir.</small></div></div>
      {done?<div className="p2-modal-body oc-bug-done">
        <b>#{done.number} {done.title}</b>
        <small>{STATUS_LABEL[done.status]||done.status} sütununa eklendi{writeChannel&&channel?` · #${channel.name} kanalına yazıldı`:''}.</small>
        <div className="oc-bug-done-actions">
          <button type="button" className="p2-btn sm primary" onClick={()=>{overlay.openInMain({kind:'task',id:done.id,boardId:done.boardId,projectId:done.projectId});props.onClose()}}><OpenIcon/>Uygulamada aç</button>
          <button type="button" className="p2-btn sm ghost" onClick={props.onClose}>Kapat</button>
        </div>
      </div>:<>
      <div className="p2-modal-body oc-bug-body">
        {!props.canCreate&&<p className="oc-error" role="alert">Görev oluşturma yetkin yok.</p>}
        <label className="p2-field"><span>Başlık</span><input className="p2-input" value={title} maxLength={160} placeholder="Ne oldu? (kısa)" autoFocus onChange={event=>setTitle(event.target.value)}/></label>
        <label className="p2-field"><span>Açıklama</span><textarea className="p2-textarea" rows={4} value={description} maxLength={8000} placeholder="Adımlar, beklenen ve olan davranış…" onChange={event=>setDescription(event.target.value)}/></label>
        <div className="oc-bug-row">
          <label className="p2-field"><span>Önem</span><select className="p2-select" value={priority} onChange={event=>setPriority(event.target.value as TaskPriority)}>{BUG_SEVERITIES.map(item=><option key={item.value} value={item.value}>{item.label}</option>)}</select></label>
          <label className="p2-field"><span>Sütun</span><select className="p2-select" value={status} onChange={event=>setStatus(event.target.value as TaskStatus)}>{BOARD_COLUMNS.map(item=><option key={item} value={item}>{STATUS_LABEL[item]}</option>)}</select></label>
          {boards.length>1&&<label className="p2-field"><span>Pano</span><select className="p2-select" value={boardId} onChange={event=>setBoardId(event.target.value)}>{boards.map(board=><option key={board.id} value={board.id}>{board.name}</option>)}</select></label>}
        </div>
        <div className="oc-bug-toggles">
          <span className="oc-toggle" onClick={()=>setAssignMe(value=>!value)}><button type="button" className="p2-toggle" role="switch" aria-checked={assignMe} aria-label="Bana ata" onClick={event=>{event.stopPropagation();setAssignMe(value=>!value)}}><i/></button>Bana ata</span>
          <span className={`oc-toggle${!channel||!props.canPost?' disabled':''}`} title={!channel?'Projede metin kanalı yok':!props.canPost?'Kanala yazma yetkin yok':`#${channel.name} kanalına da yazılır`} onClick={()=>{if(channel&&props.canPost)setWriteChannel(value=>!value)}}><button type="button" className="p2-toggle" role="switch" aria-checked={writeChannel} disabled={!channel||!props.canPost} aria-label="Kanala da yaz" onClick={event=>{event.stopPropagation();setWriteChannel(value=>!value)}}><i/></button>Kanala da yaz{channel&&<small>#{channel.name}</small>}</span>
        </div>
        <p className="oc-bug-attach">{subjectLabel?<><span className="p2-badge soft">Ek</span>{subjectLabel}</>:<small className="oc-muted">Ekran görüntüsü eklenmedi.</small>}</p>
        {error&&<p className="oc-error" role="alert">{error}</p>}
      </div>
      <div className="p2-modal-foot">
        <button type="button" className="p2-btn ghost" onClick={props.onClose}>Vazgeç</button>
        <button type="submit" className="p2-btn primary" disabled={!valid||busy}>{busy?'Bildiriliyor…':'Bildir'}</button>
      </div>
      </>}
    </form>
  </div>
}

// ---- Sabitli (salt-okunur) ----
function PinnedStrip({shots}:{shots:RecentShot[]}){
  if(!shots.length)return <div className="oc-pinned p2-empty"><small>Henüz çekim yok. F12 ile ekran görüntüsü al.</small></div>
  return <div className="oc-pinned" aria-label="Son çekilenler">
    {shots.slice(0,3).map(shot=><div key={shot.id} className="oc-pinned-shot" title={baseName(shot.path)}>{shot.jpegDataUrl?<img src={shot.jpegDataUrl} alt="" draggable={false}/>:<IconCapture size={16}/>}</div>)}
  </div>
}

export default function CaptureWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const session=overlaySessionInfo()
  const projectId=props.projectId||session?.projectId||auth.selectedProjectId||''
  const me=String((auth.user as {id?:string}|null)?.id||'')
  const organizationId=String((auth.organization as {id?:string}|null)?.id||'')
  const token=auth.accessToken
  const projectName=useMemo(()=>{
    const found=(auth.projects as {id?:string;name?:string}[]).find(project=>project?.id===projectId)
    return String(found?.name||session?.projectName||'')
  },[auth.projects,projectId,session?.projectName])
  const perms=useMyPermissions(auth.ready?token:'',organizationId)
  const canManage=perms.can('channel.manage')
  const recentAll=useRecentShots()
  const recent=useMemo(()=>recentAll.filter(shot=>!shot.projectId||shot.projectId===projectId),[recentAll,projectId])
  const api:ScreenshotsApi=useScreenshots({token,organizationId,projectId:projectId||null,enabled:auth.ready&&(props.visible||props.pinned),onError:message=>notify(message,'error')})
  const open=useOpenTarget<OpenCaptureTarget>('capture')

  const[taking,setTaking]=useState(false)
  const[library,setLibrary]=useState<LibraryInfo>(null)
  const[lightboxId,setLightboxId]=useState<string|null>(null)
  const[share,setShare]=useState<{file:UploadedFile|null;local?:RecentShot;caption:string}|null>(null)
  const[bug,setBug]=useState<{subject:BugSubject}|null>(null)
  const[confirmId,setConfirmId]=useState<string|null>(null)
  const handledAt=useRef(0)

  useEffect(()=>{props.setTitle?.(`Ekran görüntüsü${api.items.length?` · ${api.items.length}`:''}`)},[api.items.length])
  // Kurulu sürüm (hata bildirimi ve galeri satırı için): Kütüphane durumu.
  useEffect(()=>{
    let active=true
    try{window.p2?.library?.state().then(state=>{if(!active)return;const installed=state?.projects?.[projectId];setLibrary(installed?{releaseId:installed.releaseId,version:installed.version,channel:installed.channel}:null)}).catch(()=>{})}catch{}
    return()=>{active=false}
  },[projectId])

  // Bir yerel çekimi bir kez yükler (JPEG → /uploads → galeri satırı) ve sohbet eki olarak döner.
  const ensureFile=useCallback(async(shot:RecentShot):Promise<UploadedFile>=>{
    const latest=recentAll.find(item=>item.id===shot.id)||shot
    if(latest.upload)return {name:jpegName(latest),type:'image/jpeg',size:latest.upload.size,url:latest.upload.uploadPath}
    if(!latest.jpegDataUrl)throw {message:'Bu çekimin yükleme verisi bellekte yok; klasörden paylaş.'}
    patchShot(shot.id,{uploading:true})
    try{
      const uploaded=await uploadScreenshot(token,{jpegDataUrl:latest.jpegDataUrl,name:jpegName(latest)})
      const row=await api.add({uploadPath:uploaded.uploadPath,width:latest.width,height:latest.height,size:uploaded.size,releaseId:library?.releaseId,takenAt:new Date(latest.at).toISOString()})
      markUploaded(shot.id,{uploadPath:uploaded.uploadPath,size:uploaded.size,screenshotId:row?.id||null})
      return {name:uploaded.name,type:'image/jpeg',size:uploaded.size,url:uploaded.uploadPath}
    }catch(problem){patchShot(shot.id,{uploading:false});throw problem}
  },[recentAll,token,api,library?.releaseId])

  const uploadShot=useCallback((shot:RecentShot)=>{ensureFile(shot).then(()=>notify('Galeriye yüklendi','ok')).catch(problem=>notify(failText(problem,'Yüklenemedi.'),'error'))},[ensureFile])
  const shareLocal=useCallback((shot:RecentShot)=>setShare({file:shot.upload?{name:jpegName(shot),type:'image/jpeg',size:shot.upload.size,url:shot.upload.uploadPath}:null,local:shot,caption:''}),[])
  const shareGallery=useCallback((shot:ProjectScreenshot)=>setShare({file:screenshotFile(shot),caption:shot.caption||''}),[])

  // Toast "Paylaş" → {shotId}: çekim bellekteyse paylaşım penceresi; değilse klasör ipucu.
  useEffect(()=>{
    if(!open||open.at===handledAt.current)return
    handledAt.current=open.at
    const target=open.target
    const shot=typeof target?.shotId==='number'?recentAll.find(item=>item.id===target.shotId):undefined
    if(shot)shareLocal(shot)
    else notify('Çekim bellekte bulunamadı; dosya klasörde duruyor.','error')
    clearOpenTarget('capture')
  },[open,recentAll,shareLocal])

  async function take(){
    if(taking)return
    setTaking(true)
    try{const result=await overlay.screenshot();if(!result?.ok&&result?.error)notify(result.error,'error')}
    catch(problem){notify(failText(problem,'Ekran görüntüsü alınamadı.'),'error')}
    finally{setTaking(false)}
  }

  if(props.pinned&&props.minimized)return null
  if(props.pinned)return <PinnedStrip shots={recent}/>
  if(!auth.ready||!organizationId||!projectId)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>

  const lightbox=api.items.find(item=>item.id===lightboxId)||null
  const confirmShot=api.items.find(item=>item.id===confirmId)||null
  const canShareChannel=perms.can('message.send'),canShareDm=perms.can('direct.send')
  const canUpload=Boolean(token) // galeri yüklemesi proje erişimiyle yeter; ek yetkisi yalnız sohbete gönderirken aranır

  return <div className="oc">
    <header className="oc-head">
      <button type="button" className="p2-btn sm primary" disabled={taking} onClick={()=>void take()}><IconCapture size={16}/>{taking?'Çekiliyor…':'Ekran görüntüsü al'}</button>
      <span className="oc-grow"/>
      <button type="button" className="p2-btn sm" onClick={()=>setBug({subject:null})} title="Görev olarak hata bildir"><BugIcon/>Hata bildir</button>
    </header>
    <div className="oc-body">
      <section className="oc-section">
        <h4>Son çekilenler<small>{recent.length?` · ${recent.length}`:''}</small></h4>
        {recent.length?<div className="oc-strip">{recent.map(shot=><RecentCard key={shot.id} shot={shot} canUpload={canUpload}
          onUpload={()=>uploadShot(shot)} onShare={()=>shareLocal(shot)} onBug={()=>setBug({subject:{kind:'local',shot}})}/>)}</div>
          :<p className="oc-muted">Bu oturumda henüz çekim yok. F12 ya da üstteki düğme ile çek; PNG {'Resimler\\P2'} klasörüne kaydedilir.</p>}
      </section>
      <section className="oc-section oc-gallery">
        <h4>Galeri<small>{api.items.length?` · ${api.items.length}${api.hasMore?'+':''}`:''}</small></h4>
        <ScreenshotGrid api={api} me={me} canManage={canManage} compact onOpen={shot=>setLightboxId(shot.id)} onShare={canShareChannel||canShareDm?shareGallery:undefined} emptyText={'Çekimlerini "Galeriye yükle" ile ekibe göster.'}/>
      </section>
    </div>
    {lightbox&&<ScreenshotLightbox shot={lightbox} canEdit={canEditScreenshot(lightbox,me,canManage)} onClose={()=>setLightboxId(null)}
      onCaption={caption=>api.setCaption(lightbox.id,caption)} onDelete={()=>setConfirmId(lightbox.id)} onShare={canShareChannel||canShareDm?()=>shareGallery(lightbox):undefined}/>}
    {share&&<ScreenshotShareDialog token={token} organizationId={organizationId} projectId={projectId} me={me} file={share.file} defaultCaption={share.caption} allowPeople={canShareDm} notify={notify}
      ensureFile={share.local?()=>ensureFile(share.local!):undefined} onClose={()=>setShare(null)}/>}
    {bug&&<BugReportForm token={token} organizationId={organizationId} projectId={projectId} projectName={projectName} me={me} subject={bug.subject} library={library}
      canCreate={perms.can('task.create')} canPost={canShareChannel} ensureFile={ensureFile} onClose={()=>setBug(null)}/>}
    {confirmShot&&<div className="p2-modal-layer oc-modal-layer" onMouseDown={()=>setConfirmId(null)}>
      <div className="p2-modal sm" onMouseDown={event=>event.stopPropagation()} role="alertdialog" aria-label="Ekran görüntüsünü sil">
        <div className="p2-modal-head"><div><b>Ekran görüntüsünü sil</b><small>Galeriden kaldırılır; paylaşılmış mesajlardaki kopyalar kalır.</small></div></div>
        <div className="p2-modal-body"><small className="oc-muted">{confirmShot.caption||dims(confirmShot)}{confirmShot.size?` · ${formatSize(confirmShot.size)}`:''}</small></div>
        <div className="p2-modal-foot">
          <button type="button" className="p2-btn ghost" onClick={()=>setConfirmId(null)}>Vazgeç</button>
          <button type="button" className="p2-btn danger" onClick={()=>{const id=confirmShot.id;setConfirmId(null);setLightboxId(null);void api.remove(id).then(ok=>{if(ok)notify('Ekran görüntüsü silindi','info')})}}>Sil</button>
        </div>
      </div>
    </div>}
  </div>
}
