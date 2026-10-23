import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import Avatar from './ui/Avatar'
import ConfirmDialog from './ConfirmDialog'
import { attachmentMarker, attachmentUrl, formatSize, type UploadedFile } from './attachments'
import { request } from './api/client'
import { screenshotFile, type ProjectScreenshot } from './api/screenshots'
import { useScreenshots, type ScreenshotsApi } from './hooks/useScreenshots'
import { noteTimeAgo } from './ProjectNotesPanel'
import { pushToast } from './Toasts'
import { useEscape } from './useEscape'
import ChannelIcon from './ChannelIcons'
import './project-screenshots.css'

// Proje ekran görüntüsü galerisi: Kütüphane "Ekran görüntüleri" sekmesi (geniş) ve overlay
// "Ekran görüntüsü" penceresi (dar, compact) aynı ızgara/büyütme/paylaşım bileşenlerini kullanır.
// Paylaşım: kanala (TEXT/ANNOUNCEMENT) ya da kişiye mesaj = başlık + ek işareti (attachmentMarker).

export type ShareTarget={kind:'channel';id:string;name:string}|{kind:'person';id:string;name:string}
type ChannelRow={id:string;name:string;type:string}
type Category={id:string;name:string;channels:ChannelRow[]}
type Person={id:string;displayName:string;username?:string;avatarUrl?:string|null}
export type Notify=(text:string,tone:'ok'|'error'|'info')=>void

export const canEditScreenshot=(shot:Pick<ProjectScreenshot,'userId'>,me:string,canManage:boolean)=>shot.userId===me||canManage
export const screenshotTimeAgo=noteTimeAgo
export const screenshotDims=(shot:Pick<ProjectScreenshot,'width'|'height'>)=>`${shot.width}×${shot.height}`

// Paylaşım mesajı: başlık (varsa) + ek işareti; kanal ya da kişi (DM) hedefine gönderir.
export function shareContent(file:UploadedFile,caption:string){
  return [caption.trim(),attachmentMarker(file)].filter(Boolean).join('\n')
}
export async function sendShareMessage(token:string,organizationId:string,projectId:string,target:ShareTarget,content:string){
  const org=encodeURIComponent(organizationId)
  if(target.kind==='channel'){
    return request<{id:string}>(`/organizations/${org}/projects/${encodeURIComponent(projectId)}/channels/${encodeURIComponent(target.id)}/messages`,token,{method:'POST',body:JSON.stringify({content})})
  }
  const conversation=await request<{id:string}>(`/organizations/${org}/direct`,token,{method:'POST',body:JSON.stringify({userId:target.id})})
  return request<{id:string}>(`/organizations/${org}/direct/${encodeURIComponent(conversation.id)}/messages`,token,{method:'POST',body:JSON.stringify({content})})
}
// Projenin yazılabilir metin kanalları (TEXT/ANNOUNCEMENT), kategori sırasıyla.
export async function loadShareChannels(token:string,organizationId:string,projectId:string){
  const categories=await request<Category[]>(`/organizations/${encodeURIComponent(organizationId)}/projects/${encodeURIComponent(projectId)}/channels`,token)
  return categories.flatMap(category=>(category.channels||[]).filter(channel=>channel.type==='TEXT'||channel.type==='ANNOUNCEMENT').map(channel=>({id:channel.id,name:channel.name,type:channel.type,category:category.name})))
}
export async function loadSharePeople(token:string,organizationId:string,projectId:string,me:string){
  const people=await request<Person[]>(`/organizations/${encodeURIComponent(organizationId)}/members?projectId=${encodeURIComponent(projectId)}`,token)
  return people.filter(person=>person.id!==me)
}

const ShareIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7"/><path d="M12 15V4M7.5 8.5 12 4l4.5 4.5"/></svg>
const TrashIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M5 7h14M10 7V4h4v3M8 7l1 13h6l1-13"/></svg>

// ---- Paylaşım penceresi ----
export type ScreenshotShareDialogProps={
  token:string
  organizationId:string
  projectId:string
  me:string
  file:UploadedFile|null          // null: önce yükleme gerekir (overlay yerel çekim) → ensureFile
  ensureFile?:()=>Promise<UploadedFile>
  defaultCaption?:string
  allowPeople?:boolean
  onClose():void
  onSent?(target:ShareTarget):void
  notify?:Notify
}
export function ScreenshotShareDialog(props:ScreenshotShareDialogProps){
  const{token,organizationId,projectId,me}=props
  const[caption,setCaption]=useState(props.defaultCaption||'')
  const[channels,setChannels]=useState<{id:string;name:string;type:string;category:string}[]>([])
  const[people,setPeople]=useState<Person[]>([])
  const[target,setTarget]=useState<ShareTarget|null>(null)
  const[filter,setFilter]=useState('')
  const[loading,setLoading]=useState(true)
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  const notify=props.notify||((text,tone)=>pushToast({text,tone}))
  useEscape(true,props.onClose)

  useEffect(()=>{
    let active=true
    setLoading(true)
    Promise.all([loadShareChannels(token,organizationId,projectId),props.allowPeople?loadSharePeople(token,organizationId,projectId,me):Promise.resolve([] as Person[])])
      .then(([list,members])=>{if(!active)return;setChannels(list);setPeople(members);if(!target&&list.length)setTarget({kind:'channel',id:list[0].id,name:list[0].name})})
      .catch(problem=>{if(active)setError((problem as {message?:string})?.message||'Kanallar yüklenemedi.')})
      .finally(()=>{if(active)setLoading(false)})
    return()=>{active=false}
  // eslint-disable-next-line react-hooks/exhaustive-deps
  },[token,organizationId,projectId,me,props.allowPeople])

  const normalized=filter.trim().toLocaleLowerCase('tr-TR')
  const visibleChannels=channels.filter(channel=>!normalized||channel.name.toLocaleLowerCase('tr-TR').includes(normalized))
  const visiblePeople=people.filter(person=>!normalized||person.displayName.toLocaleLowerCase('tr-TR').includes(normalized)||(person.username||'').toLocaleLowerCase('tr-TR').includes(normalized))

  async function submit(event:FormEvent){
    event.preventDefault()
    if(!target||busy)return
    setBusy(true);setError('')
    try{
      const file=props.file||(props.ensureFile?await props.ensureFile():null)
      if(!file)throw new Error('Görüntü yüklenemedi.')
      await sendShareMessage(token,organizationId,projectId,target,shareContent(file,caption))
      notify(target.kind==='channel'?`#${target.name} kanalına gönderildi`:`${target.name} kişisine gönderildi`,'ok')
      props.onSent?.(target)
      props.onClose()
    }catch(problem){setError((problem as {message?:string})?.message||'Gönderilemedi.')}
    finally{setBusy(false)}
  }

  return <div className="p2-modal-layer ps-share-layer" onMouseDown={props.onClose}>
    <form className="p2-modal sm ps-share" onMouseDown={event=>event.stopPropagation()} onSubmit={submit} aria-label="Ekran görüntüsünü paylaş">
      <div className="p2-modal-head"><div><b>Paylaş</b><small>{props.allowPeople?'Bir kanala ya da kişiye mesaj olarak gönderilir.':'Bir kanala mesaj olarak gönderilir.'}</small></div></div>
      <div className="p2-modal-body ps-share-body">
        <label className="p2-field"><span>Başlık</span><input className="p2-input" value={caption} maxLength={500} placeholder="İsteğe bağlı açıklama" onChange={event=>setCaption(event.target.value)} autoFocus/></label>
        <label className="p2-field"><span>Hedef</span><input className="p2-input" value={filter} placeholder={props.allowPeople?'Kanal ya da kişi ara…':'Kanal ara…'} onChange={event=>setFilter(event.target.value)}/></label>
        <div className="ps-share-list" role="listbox" aria-label="Hedef">
          {loading&&<small className="ps-muted">Yükleniyor…</small>}
          {!loading&&visibleChannels.map(channel=><button type="button" key={`c:${channel.id}`} role="option" aria-selected={target?.kind==='channel'&&target.id===channel.id} className={`ps-share-item${target?.kind==='channel'&&target.id===channel.id?' active':''}`} onClick={()=>setTarget({kind:'channel',id:channel.id,name:channel.name})}>
            <i className="ps-share-hash" aria-hidden="true"><ChannelIcon type={channel.type as 'TEXT'|'ANNOUNCEMENT'}/></i><b>{channel.name}</b><small>{channel.category}</small>
          </button>)}
          {!loading&&props.allowPeople&&visiblePeople.map(person=><button type="button" key={`p:${person.id}`} role="option" aria-selected={target?.kind==='person'&&target.id===person.id} className={`ps-share-item${target?.kind==='person'&&target.id===person.id?' active':''}`} onClick={()=>setTarget({kind:'person',id:person.id,name:person.displayName})}>
            <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="xs"/><b>{person.displayName}</b>{person.username&&<small>@{person.username}</small>}
          </button>)}
          {!loading&&!visibleChannels.length&&!visiblePeople.length&&<small className="ps-muted">Eşleşen hedef yok.</small>}
        </div>
        {error&&<p className="ps-error" role="alert">{error}</p>}
      </div>
      <div className="p2-modal-foot">
        <button type="button" className="p2-btn ghost" onClick={props.onClose}>Vazgeç</button>
        <button type="submit" className="p2-btn primary" disabled={!target||busy}>{busy?'Gönderiliyor…':'Gönder'}</button>
      </div>
    </form>
  </div>
}

// ---- Büyütme (lightbox) ----
export type ScreenshotLightboxProps={
  shot:ProjectScreenshot
  canEdit:boolean
  onClose():void
  onCaption?(caption:string):Promise<boolean>|void
  onDelete?():void
  onShare?():void
}
export function ScreenshotLightbox(props:ScreenshotLightboxProps){
  const{shot}=props
  const[caption,setCaption]=useState(shot.caption||'')
  const[saving,setSaving]=useState(false)
  useEffect(()=>setCaption(shot.caption||''),[shot.id,shot.caption])
  useEscape(true,props.onClose)
  const dirty=caption.trim()!==(shot.caption||'')
  async function save(){
    if(!dirty||!props.onCaption)return
    setSaving(true)
    try{await props.onCaption(caption.trim())}finally{setSaving(false)}
  }
  return <div className="p2-modal-layer dim ps-lightbox-layer" onMouseDown={props.onClose}>
    <section className="ps-lightbox" onMouseDown={event=>event.stopPropagation()} aria-label="Ekran görüntüsü">
      <img src={attachmentUrl(shot.uploadPath)} alt={shot.caption||'Ekran görüntüsü'} draggable={false}/>
      <footer>
        <div className="ps-lightbox-meta">
          <Avatar id={shot.user.id} name={shot.user.displayName} url={shot.user.avatarUrl} size="xs"/>
          <b>{shot.user.displayName}</b>
          <small>{screenshotTimeAgo(shot.takenAt)} · {screenshotDims(shot)}{shot.size?` · ${formatSize(shot.size)}`:''}</small>
        </div>
        {props.canEdit?<div className="ps-lightbox-caption">
          <input className="p2-input" value={caption} maxLength={500} placeholder="Başlık ekle…" aria-label="Başlık" onChange={event=>setCaption(event.target.value)} onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();void save()}}}/>
          {dirty&&<button type="button" className="p2-btn sm primary" disabled={saving} onClick={()=>void save()}>{saving?'Kaydediliyor…':'Kaydet'}</button>}
        </div>:shot.caption?<p className="ps-lightbox-text">{shot.caption}</p>:null}
        <div className="ps-lightbox-actions">
          {props.onShare&&<button type="button" className="p2-btn sm" onClick={props.onShare}><ShareIcon/>Paylaş</button>}
          {props.canEdit&&props.onDelete&&<button type="button" className="p2-btn sm danger" onClick={props.onDelete}><TrashIcon/>Sil</button>}
          <button type="button" className="p2-btn sm ghost" onClick={props.onClose}>Kapat</button>
        </div>
      </footer>
    </section>
  </div>
}

// ---- Izgara ----
export type ScreenshotGridProps={
  api:ScreenshotsApi
  me:string
  canManage:boolean
  compact?:boolean
  onOpen(shot:ProjectScreenshot):void
  onShare?(shot:ProjectScreenshot):void
  emptyText?:string
}
export function ScreenshotGrid({api,me,canManage,compact,onOpen,onShare,emptyText}:ScreenshotGridProps){
  const{items,loading,hasMore,loadingMore,loadMore}=api
  const sentinel=useRef<HTMLDivElement>(null)
  // Sona yaklaşınca daha eski sayfa.
  useEffect(()=>{
    const node=sentinel.current
    if(!node||!hasMore||typeof IntersectionObserver==='undefined')return
    const observer=new IntersectionObserver(entries=>{if(entries.some(entry=>entry.isIntersecting))void loadMore()})
    observer.observe(node)
    return()=>observer.disconnect()
  },[hasMore,loadMore,items.length])
  if(loading)return <div className="p2-empty ps-empty"><small>Ekran görüntüleri yükleniyor…</small></div>
  if(!items.length)return <div className="p2-empty ps-empty"><b>Henüz ekran görüntüsü yok</b><small>{emptyText||'Oyun içinde F12 ile çek, "Galeriye yükle" ile ekibe göster.'}</small></div>
  return <div className={`ps-grid${compact?' compact':''}`}>
    {items.map(shot=><figure key={shot.id} className="ps-card">
      <button type="button" className="ps-card-open" onClick={()=>onOpen(shot)} title={shot.caption||'Büyüt'} aria-label={shot.caption?`Büyüt: ${shot.caption}`:'Büyüt'}>
        <img src={attachmentUrl(shot.uploadPath)} alt={shot.caption||''} loading="lazy" decoding="async" draggable={false}/>
      </button>
      <figcaption>
        <Avatar id={shot.user.id} name={shot.user.displayName} url={shot.user.avatarUrl} size="xs"/>
        <span className="ps-card-text"><b>{shot.caption||screenshotDims(shot)}</b><small>{shot.user.id===me?'Sen':shot.user.displayName} · {screenshotTimeAgo(shot.takenAt)}</small></span>
        {onShare&&<button type="button" className="p2-btn sm icon ghost" title="Paylaş" aria-label="Paylaş" onClick={()=>onShare(shot)}><ShareIcon/></button>}
        {canEditScreenshot(shot,me,canManage)&&<i className="ps-card-own" aria-hidden="true"/>}
      </figcaption>
    </figure>)}
    {hasMore&&<div ref={sentinel} className="ps-more">{loadingMore?<small className="ps-muted">Yükleniyor…</small>:<button type="button" className="p2-btn sm ghost" onClick={()=>void loadMore()}>Daha eski</button>}</div>}
  </div>
}

// ---- Ana pencere paneli (Kütüphane sekmesi) ----
export type ProjectScreenshotsPanelProps={
  token:string
  organizationId:string
  projectId:string
  me:string
  canManage:boolean
  notify?:Notify
}
export default function ProjectScreenshotsPanel(props:ProjectScreenshotsPanelProps){
  const{token,organizationId,projectId,me,canManage}=props
  const notify=useMemo<Notify>(()=>props.notify||((text,tone)=>pushToast({text,tone})),[props.notify])
  const api=useScreenshots({token,organizationId,projectId,onError:message=>notify(message,'error')})
  const[openId,setOpenId]=useState<string|null>(null)
  const[share,setShare]=useState<ProjectScreenshot|null>(null)
  const[confirm,setConfirm]=useState<ProjectScreenshot|null>(null)
  const open=useMemo(()=>api.items.find(item=>item.id===openId)||null,[api.items,openId])
  useEffect(()=>{if(openId&&!open)setOpenId(null)},[openId,open])

  return <div className="ps">
    <div className="ps-toolbar">
      <span className="ps-muted">{api.items.length?`${api.items.length} görüntü${api.hasMore?'+':''}`:''}</span>
      <span className="spacer"/>
      <button type="button" className="p2-btn sm ghost" onClick={()=>void api.reload()} title="Yenile">Yenile</button>
    </div>
    <ScreenshotGrid api={api} me={me} canManage={canManage} onOpen={shot=>setOpenId(shot.id)} onShare={setShare}/>
    {open&&<ScreenshotLightbox shot={open} canEdit={canEditScreenshot(open,me,canManage)} onClose={()=>setOpenId(null)}
      onCaption={caption=>api.setCaption(open.id,caption)} onDelete={()=>setConfirm(open)} onShare={()=>setShare(open)}/>}
    {share&&<ScreenshotShareDialog token={token} organizationId={organizationId} projectId={projectId} me={me} file={screenshotFile(share)} defaultCaption={share.caption||''} allowPeople={false} notify={notify} onClose={()=>setShare(null)}/>}
    {confirm&&<ConfirmDialog title="Ekran görüntüsünü sil" body="Galeriden kaldırılır; paylaşılmış mesajlardaki kopyalar kalır." confirmLabel="Sil" danger
      onCancel={()=>setConfirm(null)} onConfirm={()=>{const target=confirm;setConfirm(null);setOpenId(null);void api.remove(target.id).then(ok=>{if(ok)notify('Ekran görüntüsü silindi','info')})}}/>}
  </div>
}
