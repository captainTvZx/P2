import { useEffect, useState } from 'react'
import Avatar from '../../ui/Avatar'
import MessageRichText from '../../MessageRichText'
import { imageSrc } from '../../ProjectSettingsDialog'
import { channelLabel, formatDuration } from '../../library'
import type { HubPost } from '../../library'
import { useGameOverview } from '../../hooks/useGameOverview'
import type { TeamPerson } from '../../hooks/useTeam'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import { bridge } from '../overlayBridge'
import type { OverlayToolProps } from '../overlayTypes'
import './overview.css'

// Oyun özeti penceresi (overlay): kurulu sürüm + kanal, "Güncelleme var", oynama süreleri, kim oynuyor,
// canlı sürümün notları ve vitrindeki son gönderiler. Veri: useGameOverview (REST + kütüphane IPC + presence).
// Sabitliyken kompakt kart (sürüm, süreler, oynayan avatarları); sabitli + küçültülmüş tek satır.

function timeAgo(value:string|number){
  const diff=(Date.now()-new Date(value).getTime())/1000
  if(diff<60)return 'az önce'
  if(diff<3600)return `${Math.max(1,Math.floor(diff/60))} dk önce`
  if(diff<86400)return `${Math.floor(diff/3600)} sa önce`
  if(diff<172800)return 'dün'
  return new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'short'})
}
// Uzun gönderi: kısaltılır, "devamı" ana penceredeki vitrine götürür.
const isLong=(content:string)=>content.length>220||content.split('\n').length>4

function Players({people,size='xs',max=6}:{people:TeamPerson[];size?:'xs'|'sm';max?:number}){
  const shown=people.slice(0,max)
  return <div className="ov-avatars" aria-hidden="true">
    {shown.map(person=><Avatar key={person.id} id={person.id} name={person.displayName} url={person.avatarUrl} size={size}/>)}
    {people.length>max&&<span className="ov-chip">+{people.length-max}</span>}
  </div>
}

function Post({post,projectId}:{post:HubPost;projectId:string}){
  const long=isLong(post.content)
  return <article className="ov-post">
    <Avatar id={post.author.id} name={post.author.displayName} url={post.author.avatarUrl} size="sm"/>
    <div className="ov-post-main">
      <div className="ov-post-head">
        <b>{post.author.displayName}</b>
        {post.kind==='RELEASE'&&<span className="ov-chip live"><i/>Sürüm</span>}
        <time dateTime={post.createdAt} title={new Date(post.createdAt).toLocaleString('tr-TR')}>{timeAgo(post.createdAt)}</time>
      </div>
      <div className={`ov-post-body${long?' clamped':''}`}><MessageRichText content={post.content}/></div>
      {long&&<button type="button" className="p2-btn sm ghost ov-more" onClick={()=>bridge.openInMain({kind:'overview',projectId})}>devamı</button>}
    </div>
  </article>
}

export default function OverviewWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const session=overlaySessionInfo()
  const projectId=props.projectId||session?.projectId||auth.selectedProjectId||''
  const me=String((auth.user as {id?:string}|null)?.id||'')
  const organizationId=String((auth.organization as {id?:string}|null)?.id||'')
  const data=useGameOverview({token:auth.accessToken,organizationId,projectId,me,enabled:auth.ready&&(props.visible||props.pinned)})
  const[notesOpen,setNotesOpen]=useState(false)
  useEffect(()=>{props.setTitle?.('Oyun özeti')},[])

  if(!auth.ready||!organizationId||!projectId)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  const{project,installed,live,updateAvailable,playtime,posts,playing,running}=data
  const name=project?.name||session?.projectName||'Proje'
  const icon=imageSrc(project?.iconUrl||project?.coverUrl)
  const openLibrary=()=>bridge.openInMain({kind:'library',projectId})
  const openOverview=()=>bridge.openInMain({kind:'overview',projectId})

  const head=<div className="ov-head">
    <div className="ov-icon">{icon?<img src={icon} alt="" draggable={false}/>:name.slice(0,1).toLocaleUpperCase('tr-TR')}</div>
    <div className="ov-head-main">
      <div className="ov-title" title={name}>{name}</div>
      <div className="ov-meta">
        {installed?<><code>v{installed.version}</code><span className="ov-chip">{channelLabel(installed.channel)}</span></>:<span>Kurulu değil</span>}
        {running&&<span className="ov-chip live"><i/>Çalışıyor</span>}
      </div>
    </div>
  </div>

  if(props.pinned&&props.minimized)return <div className="ov-pill" aria-label="Oyun özeti">
    <b>{name}</b>
    {installed&&<code>v{installed.version}</code>}
    <span className="ov-muted">{formatDuration((installed?.playSeconds||0)||playtime.mine)}</span>
    {playing.length>0&&<Players people={playing} max={4}/>}
  </div>

  if(!props.visible)return <div className="ov-pinned" aria-label="Oyun özeti">
    {head}
    {updateAvailable&&live&&<div className="ov-update"><span>Güncelleme var · v{live.version}</span></div>}
    <div className="ov-stats">
      <div className="ov-stat"><small>Oynama süren</small><b>{formatDuration(playtime.mine)}</b></div>
      <div className="ov-stat"><small>Oynayan</small><b>{playing.length}</b></div>
    </div>
    {playing.length>0&&<div className="ov-players"><Players people={playing} size="sm"/><span className="ov-players-names">{playing.map(person=>person.displayName).join(', ')}</span></div>}
  </div>

  return <div className="ov" aria-label="Oyun özeti">
    {head}
    {updateAvailable&&live&&<div className="ov-update" role="status">
      <span>Güncelleme var · v{live.version}</span>
      <button type="button" className="p2-btn sm secondary" onClick={openLibrary}>Kütüphanede aç</button>
    </div>}
    {data.error&&<div className="ov-empty" role="alert">{data.error}</div>}

    <div className="ov-stats">
      <div className="ov-stat" title="Bu projede toplam oynama süren"><small>Oynama süren</small><b>{formatDuration(playtime.mine)}</b></div>
      <div className="ov-stat" title="Ekibin toplam oynama süresi"><small>Ekip toplamı</small><b>{formatDuration(playtime.team)}</b></div>
      <div className="ov-stat" title="Şu an bu projeyi oynayan üye sayısı"><small>Oynayan</small><b>{playing.length}</b></div>
    </div>

    <section className="ov-section">
      <div className="ov-section-head"><h4>Kim oynuyor</h4>{playing.length>0&&<em>{playing.length}</em>}</div>
      {playing.length?<div className="ov-players"><Players people={playing} size="sm"/><span className="ov-players-names">{playing.map(person=>person.displayName).join(', ')}</span></div>
        :<div className="ov-empty">Şu an ekipten kimse oynamıyor.</div>}
    </section>

    <section className="ov-section">
      <div className="ov-section-head">
        <h4>Sürüm notları</h4>
        {live&&<em>v{live.version} · {channelLabel(live.channel)}</em>}
        {live?.notes&&<button type="button" className="p2-btn sm ghost" aria-expanded={notesOpen} onClick={()=>setNotesOpen(open=>!open)}>{notesOpen?'Gizle':'Göster'}</button>}
      </div>
      {!live&&<div className="ov-empty">{data.loading?'Yükleniyor…':'Yayınlanmış sürüm yok.'}</div>}
      {live&&!live.notes&&<div className="ov-empty">Bu sürüm için not yazılmamış.</div>}
      {live?.notes&&notesOpen&&<div className="ov-notes"><MessageRichText content={live.notes}/></div>}
    </section>

    <section className="ov-section">
      <div className="ov-section-head">
        <h4>Son gönderiler</h4>
        <button type="button" className="p2-btn sm ghost" onClick={openOverview}>Tümü</button>
      </div>
      {!posts.length&&<div className="ov-empty">{data.loading?'Yükleniyor…':'Henüz gönderi yok.'}</div>}
      {posts.map(post=><Post key={post.id} post={post} projectId={projectId}/>)}
    </section>
  </div>
}
