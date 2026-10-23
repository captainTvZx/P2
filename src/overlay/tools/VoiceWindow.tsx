import { useEffect, useMemo, useState } from 'react'
import Avatar from '../../ui/Avatar'
import { CallIcon } from '../../CallIcons'
import { overlayCallAction, useOverlayVoice } from '../useOverlayVoice'
import { useOverlayAuth, useOverlaySnapshot } from '../../overlaySession'
import type { OverlayToolProps, OverlayVoiceParticipant, OverlayVoiceState } from '../overlayTypes'

// Sesli oda penceresi (overlay). WebRTC ana pencerede kalır: burada yalnız 150 ms'de bir
// gelen zengin durum çizilir, düğmeler tipli `call` komutları gönderir.
type Person={id:string;displayName:string;avatarUrl?:string|null}

function duration(since:number|null){
  if(!since)return '00:00'
  const total=Math.max(0,Math.floor((Date.now()-since)/1000))
  return `${String(Math.floor(total/60)).padStart(2,'0')}:${String(total%60).padStart(2,'0')}`
}

// Sabitli + küçültülmüş hali: Discord tarzı konuşan göstergesi — yalnız o an konuşanlar.
function VoicePill({voice}:{voice:OverlayVoiceState}){
  const speaking=voice.participants.filter(participant=>participant.speaking)
  if(!voice.active||!speaking.length)return null
  return <div className="overlay-voice-pill" style={{display:'flex',alignItems:'center',gap:6,padding:'4px 8px',borderRadius:'var(--p2-r-pill)',background:'var(--p2-bg-2)'}} aria-label="Konuşanlar">
    {speaking.map(participant=><span key={participant.userId} style={{display:'flex',alignItems:'center',gap:6}}>
      <Avatar id={participant.userId} name={participant.displayName} url={participant.avatarUrl} size="sm" ring/>
      <b style={{fontSize:'var(--p2-fs-sm)',color:'var(--p2-text-strong)'}}>{participant.displayName}</b>
    </span>)}
  </div>
}

function MemberRow({participant,me,pinned}:{participant:OverlayVoiceParticipant;me:string;pinned:boolean}){
  const self=participant.userId===me
  const stateTitle=participant.deafened?'Mikrofonu ve sesi kapalı':participant.muted?'Mikrofonu kapalı':''
  return <div className={`overlay-voice-member${participant.speaking?' speaking':''}${participant.pending?' pending':''}`}>
    <Avatar id={participant.userId} name={participant.displayName} url={participant.avatarUrl} size="sm" ring={participant.speaking}/>
    <div style={{flex:1,minWidth:0}}>
      <div style={{display:'flex',alignItems:'center',gap:6}}>
        <b style={{overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>{participant.displayName}{self?' (sen)':''}{participant.pending?' · çalıyor':''}</b>
        {stateTitle&&<i title={stateTitle} style={{display:'inline-flex',color:'var(--p2-text-3)'}}><CallIcon name="micOff"/>{participant.deafened&&<CallIcon name="headphonesOff"/>}</i>}
        {participant.screen&&<span className="p2-badge soft">Ekran</span>}
        {participant.locallyMuted&&<span className="p2-badge muted">Senin için sessiz</span>}
      </div>
      <div className="p2-progress thin" style={{marginTop:4,['--value' as string]:`${Math.round(Math.min(1,participant.level)*100)}%`}}><i/></div>
      {!self&&!pinned&&!participant.pending&&<div style={{display:'flex',alignItems:'center',gap:8,marginTop:6}}>
        <input type="range" className="p2-slider" min={0} max={200} value={participant.volume} aria-label={`${participant.displayName} ses seviyesi`} style={{['--value' as string]:`${participant.volume/2}%`}} onChange={event=>overlayCallAction({action:'setUserVolume',userId:participant.userId,volume:Number(event.target.value)})}/>
        <button type="button" className={`p2-btn sm icon${participant.locallyMuted?' danger':' ghost'}`} title={participant.locallyMuted?'Sesini aç':'Senin için sustur'} aria-label={participant.locallyMuted?'Sesini aç':'Senin için sustur'} onClick={()=>overlayCallAction({action:'toggleLocalMute',userId:participant.userId})}><CallIcon name={participant.locallyMuted?'headphonesOff':'headphones'}/></button>
      </div>}
    </div>
  </div>
}


// Discord tarzı denetim çubuğu: aramada olsun olmasın görünür. Mikrofon/kulaklık ana penceredeki
// CallProvider durumunu değiştirir (bir sonraki aramaya da uygulanır); kamera/yayın yalnız aramada anlamlıdır.
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

export default function VoiceWindow(props:OverlayToolProps){
  const voice=useOverlayVoice()
  const auth=useOverlayAuth()
  const snapshot=useOverlaySnapshot()
  const me=voice.me||String((auth.user as {id?:string}|null)?.id||'')
  const people=useMemo(()=>((snapshot?.people||[]) as Person[]).filter(person=>person.id!==me),[snapshot?.people,me])
  const[invitePicker,setInvitePicker]=useState(false)
  const[,setTick]=useState(0)
  useEffect(()=>{props.setTitle?.(voice.active?(voice.label||'Sesli oda'):'Sesli oda')},[voice.active,voice.label])
  useEffect(()=>{if(!voice.active||!props.visible)return;const timer=window.setInterval(()=>setTick(value=>value+1),1000);return()=>window.clearInterval(timer)},[voice.active,props.visible])
  useEffect(()=>{if(!voice.active)setInvitePicker(false)},[voice.active])

  if(props.pinned&&props.minimized)return <VoicePill voice={voice}/>

  const invitable=people.filter(person=>!voice.participants.some(participant=>participant.userId===person.id))
  const rooms=Object.entries(voice.voiceRooms)

  return <div className="overlay-voice" style={{display:'flex',flexDirection:'column',gap:10,minHeight:0,height:'100%'}}>
    {voice.incoming&&<section className="p2-card" style={{display:'flex',alignItems:'center',gap:10,padding:10}} aria-live="polite">
      <Avatar id={voice.incoming.from.userId} name={voice.incoming.from.displayName} url={people.find(person=>person.id===voice.incoming!.from.userId)?.avatarUrl} size="md" ring/>
      <div style={{flex:1,minWidth:0}}><b>{voice.incoming.from.displayName}</b><br/><small style={{color:'var(--p2-text-3)'}}>{voice.incoming.kind==='channel'?'Seni sesli odaya çağırıyor':voice.incoming.kind==='group'?'Grup araması':'Seni arıyor'}</small></div>
      <button type="button" className="p2-btn sm danger" onClick={()=>overlayCallAction({action:'decline'})}>Reddet</button>
      <button type="button" className="p2-btn sm primary" onClick={()=>overlayCallAction({action:'accept'})}>Kabul et</button>
    </section>}

    {voice.active?<>
      <header className="overlay-voice-label" style={{display:'flex',alignItems:'center',gap:8}}>
        <span className={`presence ${voice.ringing?'away':'online'}`} style={{position:'static',width:9,height:9,border:0}}/>
        <div style={{flex:1,minWidth:0}}>
          <b>{voice.label||'Sesli oda'}</b>
          <small style={{display:'block',color:'var(--p2-text-3)'}}>{voice.ringing?`Çalıyor… ${duration(voice.connectedAt)}`:`${duration(voice.connectedAt)} · ${voice.participants.filter(participant=>!participant.pending).length} kişi`}{!voice.micReady&&' · mikrofon açılamadı'}</small>
        </div>
      </header>
      <div className="overlay-voice-members" style={{flex:1,minHeight:0,overflow:'auto'}}>
        {voice.participants.map(participant=><MemberRow key={participant.userId} participant={participant} me={me} pinned={props.pinned}/>)}
        {voice.participants.filter(participant=>participant.userId!==me).length===0&&<small className="p2-empty" style={{padding:'12px 0'}}>Odada tek kişisin</small>}
      </div>

      {invitePicker&&!props.pinned&&<div className="p2-card" style={{padding:8,maxHeight:180,overflow:'auto'}}>
        <small style={{color:'var(--p2-text-3)'}}>Odaya ekle</small>
        {invitable.map(person=><button type="button" key={person.id} className="p2-row" style={{width:'100%'}} onClick={()=>{overlayCallAction({action:'invite',userId:person.id});setInvitePicker(false)}}>
          <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="xs"/><span>{person.displayName}</span>
        </button>)}
        {!invitable.length&&<small className="p2-empty" style={{padding:'10px 0'}}>Eklenecek başka kimse yok.</small>}
      </div>}
    </>:<>
      <div style={{flex:1,minHeight:0,overflow:'auto',display:'flex',flexDirection:'column',gap:8}}>
        {rooms.map(([channelId,members])=><section key={channelId} className="p2-card" style={{padding:10,display:'flex',alignItems:'center',gap:10}}>
          <div style={{flex:1,minWidth:0}}>
            <b>{snapshot?.channels?.find(channel=>channel.id===channelId)?.name||'Sesli oda'}</b>
            <div style={{display:'flex',alignItems:'center',gap:4,marginTop:6,flexWrap:'wrap'}}>
              {members.map(member=><Avatar key={member.userId} id={member.userId} name={member.displayName} url={member.avatarUrl} size="xs"/>)}
              <small style={{color:'var(--p2-text-3)'}}>{members.map(member=>member.displayName).join(', ')}</small>
            </div>
          </div>
          {!props.pinned&&<button type="button" className="p2-btn sm primary" onClick={()=>overlayCallAction({action:'joinChannel',channelId})}>Katıl</button>}
        </section>)}
        {!rooms.length&&<div className="p2-empty"><b>Açık sesli oda yok</b><small>Ekipten birini arayabilir ya da ana pencereden bir sesli kanala katılabilirsin.</small></div>}
      </div>

      {invitePicker&&!props.pinned&&<div className="p2-card" style={{padding:8,maxHeight:180,overflow:'auto'}}>
        {people.map(person=><button type="button" key={person.id} className="p2-row" style={{width:'100%'}} onClick={()=>{overlayCallAction({action:'startDirect',userId:person.id});setInvitePicker(false)}}>
          <Avatar id={person.id} name={person.displayName} url={person.avatarUrl} size="xs"/><span>{person.displayName}</span>
        </button>)}
        {!people.length&&<small className="p2-empty" style={{padding:'10px 0'}}>Ekip listesi henüz gelmedi.</small>}
      </div>}
    </>}
    {!props.pinned&&<ControlBar voice={voice} me={me} invitePicker={invitePicker} onInvite={()=>setInvitePicker(value=>!value)}/>}
  </div>
}
