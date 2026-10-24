import { useEffect, useState } from 'react'
import Avatar from '../../ui/Avatar'
import { CallIcon } from '../../CallIcons'
import { useTeam, type TeamRow } from '../../hooks/useTeam'
import { overlayCallAction, useOverlayVoice } from '../useOverlayVoice'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import { openWindow } from '../layoutStore'
import { setOpenTarget } from '../openTargets'
import { IconChat } from '../icons'
import type { OverlayToolProps } from '../overlayTypes'

// Ekip penceresi (overlay): oyunun projesindeki üyeler, durum noktası ve etkinlik satırı.
// Üye listesi GET /members?projectId= ile çekilir; durum/etkinlik aynalanan presence olaylarından gelir.
// Satır eylemleri simge düğmeleridir ve OYUNDAN ÇIKMADAN çalışır: Mesaj → overlay Sohbet penceresinde özel mesaj,
// Ara/Davet → sesli arama, Profil → satırın altında açılan kart (kullanıcı adı, unvan, rol, durum, etkinlik).
function statusLabel(status?:string){return status==='ONLINE'?'Çevrimiçi':status==='AWAY'?'Uzakta':status==='BUSY'?'Meşgul':'Çevrimdışı'}
function dot(status?:string):'online'|'away'|'busy'|'offline'{return status==='ONLINE'?'online':status==='AWAY'?'away':status==='BUSY'?'busy':'offline'}
function roleLabel(role?:string){return role==='OWNER'?'Kurucu':role==='ADMIN'?'Yönetici':role?'Üye':''}
function ProfileIcon(){return <svg viewBox="0 0 24 24" width={15} height={15} fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="12" cy="8.5" r="3.6"/><path d="M5 19.5a7 7 0 0 1 14 0"/></svg>}

function openChatWith(userId:string){
  setOpenTarget('chat',{kind:'person',id:userId})
  openWindow('chat')
}

function Row({row,me,inCall,ghost,open,onToggle}:{row:TeamRow;me:string;inCall:boolean;ghost:boolean;open:boolean;onToggle:()=>void}){
  const{person}=row
  const self=person.id===me
  const line=row.withMe?'Seninle sesli odada':row.text||statusLabel(person.status)
  return <div className={`overlay-team-item${open?' open':''}`}>
    <div className="p2-row overlay-team-row" style={{alignItems:'center',gap:10,cursor:'default',transform:'none'}}>
      <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="sm" status={dot(person.status)}/>
      <div style={{flex:1,minWidth:0}}>
        <b style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',color:'var(--p2-text-strong)'}}>{person.displayName}{self?' (sen)':''}</b>
        <small style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',color:row.playingHere?'var(--p2-success-text)':row.withMe?'var(--p2-text-accent)':'var(--p2-text-3)'}}>{line}</small>
      </div>
      {!ghost&&<div className="overlay-team-actions">
        {!self&&<button type="button" className="p2-btn sm icon ghost" title="Özel mesaj (overlay'de açılır)" aria-label={`${person.displayName}: özel mesaj`} onClick={()=>openChatWith(person.id)}><IconChat size={16}/></button>}
        {!self&&!row.withMe&&<button type="button" className="p2-btn sm icon ghost" title={inCall?'Sesli odaya davet et':'Sesli arama başlat'} aria-label={`${person.displayName}: ${inCall?'davet et':'ara'}`} onClick={()=>overlayCallAction(inCall?{action:'invite',userId:person.id}:{action:'startDirect',userId:person.id})}><CallIcon name={inCall?'personAdd':'phone'}/></button>}
        <button type="button" className={`p2-btn sm icon ghost${open?' on':''}`} title={open?'Profili kapat':'Profil'} aria-label={`${person.displayName}: profil`} aria-expanded={open} onClick={onToggle}><ProfileIcon/></button>
      </div>}
    </div>
    {open&&!ghost&&<div className="overlay-team-profile p2-card" aria-label={`${person.displayName} profili`}>
      <div className="overlay-team-profile-head">
        <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="md" status={dot(person.status)}/>
        <div style={{flex:1,minWidth:0}}>
          <b>{person.displayName}</b>
          {person.username&&<small>@{person.username}</small>}
        </div>
      </div>
      <dl className="overlay-team-profile-meta">
        {person.title&&<div><dt>Unvan</dt><dd>{person.title}</dd></div>}
        {person.role&&<div><dt>Rol</dt><dd>{roleLabel(person.role)}</dd></div>}
        <div><dt>Durum</dt><dd>{row.withMe?'Seninle sesli odada':statusLabel(person.status)}</dd></div>
        {row.text&&<div><dt>Şu an</dt><dd>{row.text}</dd></div>}
      </dl>
      <div className="overlay-team-profile-actions">
        {!self&&<button type="button" className="p2-btn sm secondary" onClick={()=>openChatWith(person.id)}><IconChat size={14}/>Mesaj</button>}
        {!self&&!row.withMe&&<button type="button" className="p2-btn sm secondary" onClick={()=>overlayCallAction(inCall?{action:'invite',userId:person.id}:{action:'startDirect',userId:person.id})}><CallIcon name={inCall?'personAdd':'phone'}/>{inCall?'Davet et':'Ara'}</button>}
        <button type="button" className="p2-btn sm ghost" title="Tam profili P2 penceresinde aç" onClick={()=>window.p2overlay?.openInMain({kind:'profile',id:person.id})}>Uygulamada aç</button>
      </div>
    </div>}
  </div>
}

export default function TeamWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const voice=useOverlayVoice()
  const projectId=props.projectId||overlaySessionInfo()?.projectId||auth.selectedProjectId
  const me=String((auth.user as {id?:string}|null)?.id||'')
  const organizationId=String((auth.organization as {id?:string}|null)?.id||'')
  const team=useTeam({token:auth.accessToken,organizationId,projectId,me,withMe:voice.active?voice.participants.filter(participant=>!participant.pending).map(participant=>participant.userId):[]})
  const[openId,setOpenId]=useState('')
  const ghost=!props.visible
  useEffect(()=>{props.setTitle?.(`Ekip${team.people.length?` · ${team.people.length}`:''}`)},[team.people.length])
  // Overlay görünür olunca sunucudan etkinlik + oda anlık görüntüsü yeniden istenir (üyeler de tazelenir).
  useEffect(()=>{if(!props.visible||!auth.ready)return;team.requestSnapshot();void team.refresh()},[props.visible,auth.ready,projectId])
  useEffect(()=>{if(ghost)setOpenId('')},[ghost])

  if(!auth.ready)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  const online=team.sorted.filter(row=>row.rank<5),offline=team.sorted.filter(row=>row.rank>=5)
  if(props.pinned&&props.minimized)return <div style={{display:'flex',gap:4,flexWrap:'wrap',padding:4}} aria-label="Ekip">
    {online.map(row=><Avatar key={row.person.id} id={row.person.id} name={row.person.displayName} url={row.person.avatarUrl} size="xs" status={dot(row.person.status)}/>)}
  </div>
  const row=(item:TeamRow)=><Row key={item.person.id} row={item} me={me} inCall={voice.active} ghost={ghost} open={openId===item.person.id} onToggle={()=>setOpenId(current=>current===item.person.id?'':item.person.id)}/>
  return <div className="overlay-team" style={{display:'flex',flexDirection:'column',gap:4,minHeight:0,height:'100%',overflow:'auto'}}>
    {!team.people.length&&<div className="p2-empty"><b>Ekip listesi boş</b><small>Bu projede senden başka üye görünmüyor.</small></div>}
    {online.length>0&&<small style={{color:'var(--p2-text-3)',padding:'4px 8px'}}>Çevrimiçi — {online.length}</small>}
    {online.map(row)}
    {offline.length>0&&<small style={{color:'var(--p2-text-3)',padding:'8px 8px 4px'}}>Çevrimdışı — {offline.length}</small>}
    {offline.map(row)}
  </div>
}
