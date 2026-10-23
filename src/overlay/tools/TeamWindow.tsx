import { useEffect } from 'react'
import Avatar from '../../ui/Avatar'
import { CallIcon } from '../../CallIcons'
import { useTeam, type TeamRow } from '../../hooks/useTeam'
import { overlayCallAction, useOverlayVoice } from '../useOverlayVoice'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import type { OverlayToolProps } from '../overlayTypes'

// Ekip penceresi (overlay): oyunun projesindeki üyeler, durum noktası ve etkinlik satırı.
// Üye listesi GET /members?projectId= ile çekilir; durum/etkinlik aynalanan presence olaylarından gelir.
function statusLabel(status?:string){return status==='ONLINE'?'Çevrimiçi':status==='AWAY'?'Uzakta':status==='BUSY'?'Meşgul':'Çevrimdışı'}
function dot(status?:string):'online'|'away'|'busy'|'offline'{return status==='ONLINE'?'online':status==='AWAY'?'away':status==='BUSY'?'busy':'offline'}

function Row({row,inCall,pinned}:{row:TeamRow;inCall:boolean;pinned:boolean}){
  const{person}=row
  const line=row.withMe?'Seninle sesli odada':row.text||statusLabel(person.status)
  return <div className="p2-row" style={{alignItems:'center',gap:10,cursor:'default',transform:'none'}}>
    <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="sm" status={dot(person.status)}/>
    <div style={{flex:1,minWidth:0}}>
      <b style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',color:'var(--p2-text-strong)'}}>{person.displayName}</b>
      <small style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',color:row.playingHere?'var(--p2-success-text)':row.withMe?'var(--p2-text-accent)':'var(--p2-text-3)'}}>{line}</small>
    </div>
    {!pinned&&<div className="overlay-team-actions">
      <button type="button" className="p2-btn sm secondary" title="Özel mesaj gönder" onClick={()=>window.p2overlay?.openInMain({kind:'person',id:person.id})}>Mesaj</button>
      {!row.withMe&&<button type="button" className="p2-btn sm secondary" title={inCall?'Bu kişiyi sesli odaya davet et':'Sesli arama başlat'} onClick={()=>overlayCallAction(inCall?{action:'invite',userId:person.id}:{action:'startDirect',userId:person.id})}><CallIcon name={inCall?'personAdd':'mic'}/> {inCall?'Davet et':'Ara'}</button>}
      <button type="button" className="p2-btn sm ghost" title="Profili ana pencerede aç" onClick={()=>window.p2overlay?.openInMain({kind:'profile',id:person.id})}>Profil</button>
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
  useEffect(()=>{props.setTitle?.(`Ekip${team.people.length?` · ${team.people.length}`:''}`)},[team.people.length])
  // Overlay görünür olunca sunucudan etkinlik + oda anlık görüntüsü yeniden istenir (üyeler de tazelenir).
  useEffect(()=>{if(!props.visible||!auth.ready)return;team.requestSnapshot();void team.refresh()},[props.visible,auth.ready,projectId])

  if(!auth.ready)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  const online=team.sorted.filter(row=>row.rank<5),offline=team.sorted.filter(row=>row.rank>=5)
  if(props.pinned&&props.minimized)return <div style={{display:'flex',gap:4,flexWrap:'wrap',padding:4}} aria-label="Ekip">
    {online.map(row=><Avatar key={row.person.id} id={row.person.id} name={row.person.displayName} url={row.person.avatarUrl} size="xs" status={dot(row.person.status)}/>)}
  </div>
  return <div className="overlay-team" style={{display:'flex',flexDirection:'column',gap:4,minHeight:0,height:'100%',overflow:'auto'}}>
    {!team.people.length&&<div className="p2-empty"><b>Ekip listesi boş</b><small>Bu projede senden başka üye görünmüyor.</small></div>}
    {online.length>0&&<small style={{color:'var(--p2-text-3)',padding:'4px 8px'}}>Çevrimiçi — {online.length}</small>}
    {online.map(row=><Row key={row.person.id} row={row} inCall={voice.active} pinned={props.pinned}/>)}
    {offline.length>0&&<small style={{color:'var(--p2-text-3)',padding:'8px 8px 4px'}}>Çevrimdışı — {offline.length}</small>}
    {offline.map(row=><Row key={row.person.id} row={row} inCall={voice.active} pinned={props.pinned}/>)}
  </div>
}
