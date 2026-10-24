import { useEffect, useMemo, useState } from 'react'
import Avatar from '../../ui/Avatar'
import { CallIcon } from '../../CallIcons'
import { overlayCallAction, useOverlayVoice } from '../useOverlayVoice'
import { overlaySessionInfo, useOverlayAuth, useOverlaySnapshot } from '../../overlaySession'
import type { OverlayToolProps, OverlayVoiceParticipant, OverlayVoiceState } from '../overlayTypes'
import './voice.css'

// Sesli oda penceresi (overlay). WebRTC ana pencerede kalır: burada yalnız 150 ms'de bir gelen zengin durum çizilir,
// düğmeler tipli `call` komutları gönderir. Görünüm: aramada katılımcı kutucukları (konuşan halkası, seviye çubuğu,
// kişi sesi); aramada değilken projenin sesli kanalları kart olarak (içindekiler avatarlarla, boşlar soluk) + "Katıl".
// Altta Discord tarzı denetim çubuğu. Panel kapalıyken (hayalet) düğme çizilmez.
type Person={id:string;displayName:string;avatarUrl?:string|null}
type Room={channelId:string;name:string;members:{userId:string;displayName:string;avatarUrl?:string|null}[]}

function duration(since:number|null){
  if(!since)return '00:00'
  const total=Math.max(0,Math.floor((Date.now()-since)/1000))
  return `${String(Math.floor(total/60)).padStart(2,'0')}:${String(total%60).padStart(2,'0')}`
}

// Sabitli + küçültülmüş hali: Discord tarzı konuşan göstergesi — yalnız o an konuşanlar.
function VoicePill({voice}:{voice:OverlayVoiceState}){
  const speaking=voice.participants.filter(participant=>participant.speaking)
  if(!voice.active||!speaking.length)return null
  return <div className="ov-voice-pill" aria-label="Konuşanlar">
    {speaking.map(participant=><span key={participant.userId}>
      <Avatar id={participant.userId} name={participant.displayName} url={participant.avatarUrl} size="sm" ring/>
      <b>{participant.displayName}</b>
    </span>)}
  </div>
}

// Katılımcı kutucuğu: büyük avatar, konuşurken vurgu halkası, altta ad + durum simgeleri + seviye çubuğu.
function Tile({participant,me,ghost,open,onToggle}:{participant:OverlayVoiceParticipant;me:string;ghost:boolean;open:boolean;onToggle():void}){
  const self=participant.userId===me
  const title=participant.deafened?'Mikrofonu ve sesi kapalı':participant.muted?'Mikrofonu kapalı':participant.speaking?'Konuşuyor':''
  return <div className={`ov-voice-tile${participant.speaking?' speaking':''}${participant.pending?' pending':''}${open?' open':''}`}>
    <button type="button" className="ov-voice-tile-main" disabled={ghost||self||participant.pending} title={self?'Sen':'Kişi sesi ve sessize alma'} onClick={onToggle}>
      <span className="ov-voice-avatar"><Avatar id={participant.userId} name={participant.displayName} url={participant.avatarUrl} size="lg"/>
        {participant.screen&&<i className="ov-voice-flag screen" title="Ekran paylaşıyor"><CallIcon name="screen"/></i>}
        {participant.video&&<i className="ov-voice-flag video" title="Kamera açık"><CallIcon name="camera"/></i>}
      </span>
      <span className="ov-voice-name"><b>{participant.displayName}{self?' (sen)':''}</b>{participant.pending&&<small>çalıyor…</small>}</span>
      <span className="ov-voice-state" title={title}>
        {participant.muted&&<CallIcon name="micOff"/>}
        {participant.deafened&&<CallIcon name="headphonesOff"/>}
        {participant.locallyMuted&&<CallIcon name="volumeOff"/>}
      </span>
      <i className="ov-voice-level" style={{['--level' as string]:`${Math.round(Math.min(1,participant.level)*100)}%`}}/>
    </button>
    {open&&!ghost&&!self&&<div className="ov-voice-tile-tools">
      <label><CallIcon name="volume"/><input type="range" className="p2-slider" min={0} max={200} value={participant.volume} aria-label={`${participant.displayName} ses seviyesi`} style={{['--value' as string]:`${participant.volume/2}%`}} onChange={event=>overlayCallAction({action:'setUserVolume',userId:participant.userId,volume:Number(event.target.value)})}/><small>%{participant.volume}</small></label>
      <button type="button" className={`p2-btn sm${participant.locallyMuted?' danger':' secondary'}`} onClick={()=>overlayCallAction({action:'toggleLocalMute',userId:participant.userId})}><CallIcon name={participant.locallyMuted?'volumeOff':'volume'}/>{participant.locallyMuted?'Sesini aç':'Senin için sustur'}</button>
    </div>}
  </div>
}

// Discord tarzı denetim çubuğu: aramada olsun olmasın görünür. Mikrofon/kulaklık ana penceredeki CallProvider
// durumunu değiştirir (bir sonraki aramaya da uygulanır); kamera/yayın yalnız aramada anlamlıdır.
function ControlBar({voice,me,invitePicker,onInvite}:{voice:OverlayVoiceState;me:string;invitePicker:boolean;onInvite:()=>void}){
  const mine=voice.participants.find(p=>p.userId===me)
  const screenOn=!!mine?.screen,cameraOn=!!mine?.video
  const Btn=({on,danger,icon,label,title,onClick,disabled}:{on?:boolean;danger?:boolean;icon:Parameters<typeof CallIcon>[0]['name'];label:string;title:string;onClick:()=>void;disabled?:boolean})=>
    <button type="button" className={`overlay-voice-ctl${on?' on':''}${danger?' danger':''}`} title={title} aria-label={title} aria-pressed={on} disabled={disabled} onClick={onClick}><CallIcon name={icon}/><span>{label}</span></button>
  return <div className="overlay-voice-bar" role="toolbar" aria-label="Ses denetimleri">
    <Btn on={!voice.muted} icon={voice.muted?'micOff':'mic'} label={voice.muted?'Sessiz':'Mikrofon'} title={voice.muted?'Mikrofonu aç':'Mikrofonu kapat'} onClick={()=>overlayCallAction({action:'mute'})}/>
    <Btn on={!voice.deafened} icon={voice.deafened?'headphonesOff':'headphones'} label={voice.deafened?'Sağır':'Kulaklık'} title={voice.deafened?'Sesi aç':'Sağırlaştır (hiçbir şeyi duyma)'} onClick={()=>overlayCallAction({action:'deafen'})}/>
    <Btn on={cameraOn} icon={cameraOn?'camera':'cameraOff'} label="Kamera" title={voice.active?(cameraOn?'Kamerayı kapat':'Kamerayı aç'):'Kamera için önce bir aramaya katıl'} disabled={!voice.active} onClick={()=>overlayCallAction({action:'toggleCamera'})}/>
    <Btn on={screenOn} icon={screenOn?'screen':'screenOff'} label="Yayın" title={voice.active?(screenOn?'Ekran yayınını durdur':'Ekran yayını başlat (kaynak seçimi ana pencerede açılır)'):'Yayın için önce bir aramaya katıl'} disabled={!voice.active} onClick={()=>overlayCallAction({action:'toggleScreen'})}/>
    <Btn on={invitePicker} icon="personAdd" label={voice.active?'Davet':'Ara'} title={voice.active?'Odaya kişi davet et':'Ekipten birini ara'} onClick={onInvite}/>
    {voice.active&&<Btn danger icon="hangup" label="Ayrıl" title="Aramadan ayrıl" onClick={()=>overlayCallAction({action:'leave'})}/>}
  </div>
}

function RoomCard({room,ghost,current}:{room:Room;ghost:boolean;current:boolean}){
  const live=room.members.length>0
  return <section className={`ov-voice-room${live?' live':''}${current?' current':''}`} aria-label={room.name}>
    <span className="ov-voice-room-icon"><CallIcon name="volume"/></span>
    <div className="ov-voice-room-main">
      <b>{room.name}</b>
      {live?<div className="ov-voice-room-members">
        <span className="ov-voice-stack">{room.members.slice(0,5).map(member=><Avatar key={member.userId} id={member.userId} name={member.displayName} url={member.avatarUrl} size="xs"/>)}</span>
        <small>{room.members.map(member=>member.displayName).join(', ')}</small>
      </div>:<small className="ov-voice-room-empty">Boş · ilk sen gir</small>}
    </div>
    {!ghost&&!current&&<button type="button" className={`p2-btn sm ${live?'primary':'secondary'}`} onClick={()=>overlayCallAction({action:'joinChannel',channelId:room.channelId})}>Katıl</button>}
    {current&&<span className="p2-badge success">Buradasın</span>}
  </section>
}

export default function VoiceWindow(props:OverlayToolProps){
  const voice=useOverlayVoice()
  const auth=useOverlayAuth()
  const snapshot=useOverlaySnapshot()
  const me=voice.me||String((auth.user as {id?:string}|null)?.id||'')
  const people=useMemo(()=>((snapshot?.people||[]) as Person[]).filter(person=>person.id!==me),[snapshot?.people,me])
  const[invitePicker,setInvitePicker]=useState(false)
  const[openId,setOpenId]=useState('')
  const[,setTick]=useState(0)
  useEffect(()=>{props.setTitle?.(voice.active?(voice.label||'Sesli oda'):'Sesli oda')},[voice.active,voice.label])
  useEffect(()=>{if(!voice.active||!props.visible)return;const timer=window.setInterval(()=>setTick(value=>value+1),1000);return()=>window.clearInterval(timer)},[voice.active,props.visible])
  useEffect(()=>{if(!voice.active){setInvitePicker(false);setOpenId('')}},[voice.active])

  if(props.pinned&&props.minimized)return <VoicePill voice={voice}/>
  // Panel kapalıyken (sabitli hayalet): Discord tarzı kompakt liste — yalnız avatar + ad, konuşan vurgulu. Tam arayüz panel açıkken.
  const ghost=!props.visible
  if(ghost){
    if(!voice.active)return <div className="ov-voice-compact empty"><CallIcon name="volume"/><span>Sesli odada değilsin</span></div>
    return <div className="ov-voice-compact" aria-label={voice.label||'Sesli oda'}>
      {voice.participants.map(participant=><div key={participant.userId} className={`ov-voice-compact-row${participant.speaking?' speaking':''}${participant.pending?' pending':''}`}>
        <Avatar id={participant.userId} name={participant.displayName} url={participant.avatarUrl} size="sm" ring={participant.speaking}/>
        <b>{participant.displayName}</b>
        {(participant.muted||participant.deafened)&&<span className="ov-voice-compact-state"><CallIcon name={participant.deafened?'headphonesOff':'micOff'}/></span>}
      </div>)}
    </div>
  }

  const invitable=people.filter(person=>!voice.participants.some(participant=>participant.userId===person.id))
  // Katılınabilecek odalar: projenin TÜM sesli kanalları (boş olanlar dahil) + içinde biri olan ama kanal listesinde
  // bulunmayan odalar (başka projenin kanalı seçiliyse yalnız doluları biliriz).
  const projectId=props.projectId||overlaySessionInfo()?.projectId||''
  const voiceChannels=(snapshot?.channels||[]).filter(channel=>channel.type==='VOICE'&&(!projectId||!channel.projectId||channel.projectId===projectId))
  const rooms:Room[]=voiceChannels.map(channel=>({channelId:channel.id,name:channel.name,members:voice.voiceRooms[channel.id]||[]}))
  for(const[channelId,members]of Object.entries(voice.voiceRooms))if(!rooms.some(room=>room.channelId===channelId))rooms.push({channelId,name:snapshot?.channels?.find(channel=>channel.id===channelId)?.name||'Sesli oda',members})
  rooms.sort((a,b)=>(b.members.length>0?1:0)-(a.members.length>0?1:0)||a.name.localeCompare(b.name,'tr'))
  const others=voice.participants.filter(participant=>participant.userId!==me)

  return <div className={`overlay-voice${ghost?' ghost':''}`}>
    {voice.incoming&&<section className="ov-voice-incoming" aria-live="polite">
      <Avatar id={voice.incoming.from.userId} name={voice.incoming.from.displayName} url={people.find(person=>person.id===voice.incoming!.from.userId)?.avatarUrl} size="md" ring/>
      <div className="ov-voice-incoming-text"><b>{voice.incoming.from.displayName}</b><small>{voice.incoming.kind==='channel'?'Seni sesli odaya çağırıyor':voice.incoming.kind==='group'?'Grup araması':'Seni arıyor'}</small></div>
      {!ghost&&<><button type="button" className="p2-btn sm danger" onClick={()=>overlayCallAction({action:'decline'})}>Reddet</button>
      <button type="button" className="p2-btn sm primary" onClick={()=>overlayCallAction({action:'accept'})}>Kabul et</button></>}
    </section>}

    {voice.active?<>
      <header className="ov-voice-head">
        <span className={`ov-voice-dot${voice.ringing?' ringing':''}`}/>
        <div className="ov-voice-head-main">
          <b>{voice.label||'Sesli oda'}</b>
          <small>{voice.ringing?`Çalıyor… ${duration(voice.connectedAt)}`:`${duration(voice.connectedAt)} · ${voice.participants.filter(participant=>!participant.pending).length} kişi`}{!voice.micReady&&' · mikrofon açılamadı'}</small>
        </div>
        {!ghost&&<button type="button" className="p2-btn sm icon ghost" title="Ana pencerede aç" aria-label="Aramayı ana pencerede aç" onClick={()=>overlayCallAction({action:'open'})}><CallIcon name="expand"/></button>}
      </header>
      <div className="ov-voice-grid" aria-label="Katılımcılar">
        {voice.participants.map(participant=><Tile key={participant.userId} participant={participant} me={me} ghost={ghost} open={openId===participant.userId} onToggle={()=>setOpenId(current=>current===participant.userId?'':participant.userId)}/>)}
      </div>
      {!others.length&&<small className="ov-voice-alone">Odada tek kişisin — "Davet" ile ekipten birini çağır.</small>}
      {invitePicker&&!ghost&&<div className="ov-voice-picker">
        <small>Odaya ekle</small>
        {invitable.map(person=><button type="button" key={person.id} className="ov-voice-person" onClick={()=>{overlayCallAction({action:'invite',userId:person.id});setInvitePicker(false)}}>
          <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="xs"/><span>{person.displayName}</span>
        </button>)}
        {!invitable.length&&<small className="ov-voice-alone">Eklenecek başka kimse yok.</small>}
      </div>}
    </>:<>
      <div className="ov-voice-rooms">
        <span className="p2-kicker">Sesli kanallar{rooms.length?` · ${rooms.length}`:''}</span>
        {rooms.map(room=><RoomCard key={room.channelId} room={room} ghost={ghost} current={false}/>)}
        {!rooms.length&&<div className="p2-empty"><b>Bu projede sesli kanal yok</b><small>Ana pencereden bir sesli kanal açabilir ya da ekipten birini arayabilirsin.</small></div>}
      </div>
      {invitePicker&&!ghost&&<div className="ov-voice-picker">
        <small>Ara</small>
        {people.map(person=><button type="button" key={person.id} className="ov-voice-person" onClick={()=>{overlayCallAction({action:'startDirect',userId:person.id});setInvitePicker(false)}}>
          <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="xs"/><span>{person.displayName}</span>
        </button>)}
        {!people.length&&<small className="ov-voice-alone">Ekip listesi henüz gelmedi.</small>}
      </div>}
    </>}
    {!ghost&&<ControlBar voice={voice} me={me} invitePicker={invitePicker} onInvite={()=>setInvitePicker(value=>!value)}/>}
  </div>
}
