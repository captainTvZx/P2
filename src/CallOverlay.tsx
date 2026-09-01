import { useEffect, useMemo, useRef, useState } from 'react'
import { useCall, type CallDisplayParticipant, type CallParticipant } from './CallProvider'
import { setCallSummary, showCallPanel, useCallSummary } from './callBus'
import { CallIcon } from './CallIcons'
import { avatarHue, userInitials } from './initials'
import { applyOutputDevice, useSettings } from './settings'
import { useEscape } from './useEscape'

// Zil sesi: bildirim sesiyle aynı yaklaşım, tekrarlı çalar.
function useRingtone(active:boolean,volume:number){
  useEffect(()=>{
    if(!active)return
    let context:AudioContext|null=null
    let timer=0
    try{
      context=new AudioContext()
      const ring=()=>{
        if(!context)return
        const now=context.currentTime
        const master=context.createGain()
        master.gain.setValueAtTime(0.0001,now)
        master.gain.exponentialRampToValueAtTime(Math.max(0.015,0.085*(volume/100)),now+0.02)
        master.gain.exponentialRampToValueAtTime(0.0001,now+1.1)
        master.connect(context.destination)
        for(const[frequency,start]of[[660,0],[880,0.22],[660,0.44]] as Array<[number,number]>){
          const tone=context.createOscillator(),gain=context.createGain()
          tone.type='sine'
          tone.frequency.setValueAtTime(frequency,now+start)
          gain.gain.setValueAtTime(0.0001,now+start)
          gain.gain.exponentialRampToValueAtTime(0.8,now+start+0.03)
          gain.gain.exponentialRampToValueAtTime(0.0001,now+start+0.2)
          tone.connect(gain);gain.connect(master)
          tone.start(now+start);tone.stop(now+start+0.24)
        }
      }
      ring()
      timer=window.setInterval(ring,2400)
    }catch{}
    return()=>{window.clearInterval(timer);void context?.close().catch(()=>{})}
  },[active,volume])
}

// Arayan taraf için modern bir bekleme sesi: yükselen iki yumuşak nota,
// klasik telefon zili yerine kısa ve rahatsız etmeyen bir işaret.
function useRingback(active:boolean,volume:number){
  useEffect(()=>{
    if(!active)return
    let context:AudioContext|null=null
    let timer=0
    try{
      context=new AudioContext()
      const level=Math.max(0.01,0.045*(volume/100))
      const note=(frequency:number,at:number,length:number)=>{
        if(!context)return
        const tone=context.createOscillator(),gain=context.createGain()
        tone.type='triangle'
        tone.frequency.setValueAtTime(frequency,at)
        gain.gain.setValueAtTime(0.0001,at)
        gain.gain.exponentialRampToValueAtTime(level,at+0.05)
        gain.gain.exponentialRampToValueAtTime(0.0001,at+length)
        tone.connect(gain);gain.connect(context.destination)
        tone.start(at);tone.stop(at+length+0.05)
      }
      const pulse=()=>{
        if(!context)return
        const now=context.currentTime
        note(523.25,now,0.34)        // C5
        note(783.99,now+0.19,0.42)   // G5
      }
      pulse()
      timer=window.setInterval(pulse,3200)
    }catch{}
    return()=>{window.clearInterval(timer);void context?.close().catch(()=>{})}
  },[active,volume])
}

function RemoteAudio({userId,stream,deafened}:{userId:string;stream:MediaStream;deafened:boolean}){
  const settings=useSettings()
  const element=useRef<HTMLAudioElement>(null)
  useEffect(()=>{
    const node=element.current
    if(!node)return
    node.srcObject=stream
    applyOutputDevice(node,settings)
    node.volume=settings.outputVolume/100
    void node.play().catch(()=>{})
  },[stream,settings.outputDeviceId,settings.outputVolume])
  useEffect(()=>{if(element.current)element.current.muted=deafened},[deafened])
  return <audio ref={element} autoPlay data-user={userId}/>
}

function VideoTile({stream,label,muted,speaking,self}:{stream:MediaStream;label:string;muted?:boolean;speaking?:boolean;self?:boolean}){
  const element=useRef<HTMLVideoElement>(null)
  useEffect(()=>{const node=element.current;if(!node)return;node.srcObject=stream;void node.play().catch(()=>{})},[stream])
  return <div className={speaking?'call-tile speaking':'call-tile'}>
    <video ref={element} autoPlay playsInline muted={self} className={self?'mirrored':undefined}/>
    <footer><b>{label}</b>{muted&&<i title="Susturulmuş">🔇</i>}</footer>
  </div>
}

function AvatarTile({participant,speaking,pending,onOpen}:{participant:CallParticipant;speaking?:boolean;pending?:boolean;onOpen?:()=>void}){
  return <div className={`call-tile avatar${speaking?' speaking':''}${pending?' pending':''}`} role="button" title={`${participant.displayName} profilini aç`} onClick={onOpen}>
    <span className={`call-avatar ${avatarHue(participant.userId)}`}>{userInitials(participant.displayName)}</span>
    <footer><b>{participant.displayName}{pending?' · çalıyor':''}</b>{participant.muted&&<CallIcon name="micOff"/>}</footer>
  </div>
}

function duration(startedAt:number|null){
  if(!startedAt)return '00:00'
  const total=Math.max(0,Math.floor((Date.now()-startedAt)/1000))
  const minutes=String(Math.floor(total/60)).padStart(2,'0')
  const seconds=String(total%60).padStart(2,'0')
  return `${minutes}:${seconds}`
}

export default function CallOverlay({me,people,onOpenPerson}:{me:string;people:Array<{id:string;displayName:string}>;onOpenPerson?:(userId:string)=>void}){
  const call=useCall()
  const settings=useSettings()
  const bus=useCallSummary()
  const[stage,setStage]=useState(false)
  const[invitePicker,setInvitePicker]=useState(false)
  const[, setTick]=useState(0)
  useRingtone(Boolean(call.incoming),settings.ringtoneVolume)
  useRingback(call.ringing&&call.call?.kind==='direct',settings.ringtoneVolume)
  useEscape(stage,()=>setStage(false))
  useEffect(()=>{if(!call.call)return;const timer=window.setInterval(()=>setTick(value=>value+1),1000);return()=>window.clearInterval(timer)},[call.call])
  // Gelen arama 45 saniye yanıtlanmazsa kendiliğinden reddedilir.
  useEffect(()=>{if(!call.incoming)return;const timer=window.setTimeout(()=>call.decline(),45000);return()=>window.clearTimeout(timer)},[call.incoming])
  useEffect(()=>{if(!call.call)setStage(false)},[call.call])
  useEffect(()=>{if(bus.expandRequest&&call.call)setStage(true)},[bus.expandRequest])
  useEffect(()=>{
    const summary=call.call
      ?{active:true,label:call.call.kind==='channel'?'Sesli oda':'Sesli arama',participants:call.call.participants.length,muted:call.muted,startedAt:call.startedAt,connectedAt:call.connectedAt,ringing:call.ringing}
      :{active:false,label:'',participants:0,muted:false,startedAt:null,connectedAt:null,ringing:false}
    setCallSummary(summary)
    // Masaüstü balonu (pencere küçültülünce görünür) aynı özeti kullanır.
    window.p2?.call?.update(summary)
  },[call.call,call.muted,call.startedAt])
  // Masaüstü balonundaki düğmeler ana penceredeki aramayı yönetir.
  useEffect(()=>window.p2?.call?.onWidgetAction(action=>{
    if(action==='mute')call.toggleMute()
    else if(action==='leave')call.leave()
    else if(action==='open')showCallPanel()
  }),[call])

  const others=useMemo(()=>call.call?call.call.participants.filter(participant=>participant.userId!==me):[],[call.call,me])
  // Panelde kendi avatarım değil, konuştuğum/aradığım kişiler görünür.
  const roster=useMemo<CallDisplayParticipant[]>(()=>{
    if(!call.call)return[]
    const waiting=call.call.invited.filter(userId=>userId!==me).map(userId=>({userId,displayName:people.find(person=>person.id===userId)?.displayName||'Katılımcı',muted:false,video:false,screen:false,pending:true}))
    if(call.call.kind==='direct'){
      const counterpartId=call.call.caller===me?call.call.callee:call.call.caller
      const participant=others.find(item=>item.userId===counterpartId)||others[0]
      if(participant)return[participant]
      const pending=waiting.find(item=>item.userId===counterpartId)||waiting[0]
      return pending?[pending]:[]
    }
    return[...others,...waiting]
  },[call.call,others,people,me])
  const headline=call.call?.kind==='channel'?'Sesli oda':roster[0]?.displayName||'Sesli arama'
  const videoTiles=useMemo(()=>Object.entries(call.remote).filter(([,media])=>media.hasVideo),[call.remote])
  const invitable=people.filter(person=>!call.call?.participants.some(participant=>participant.userId===person.id)&&!call.call?.invited.includes(person.id))

  return <>
    {Object.entries(call.remote).map(([userId,media])=><RemoteAudio key={userId} userId={userId} stream={media.stream} deafened={call.deafened}/>)}

    {call.incoming&&<div className="incoming-layer">
      <section className="incoming-call">
        <span className={`call-avatar large ${avatarHue(call.incoming.from.userId)}`}>{userInitials(call.incoming.from.displayName)}</span>
        <h3>{call.incoming.from.displayName}</h3>
        <p>{call.incoming.call.kind==='channel'?'Kanal araması':'Seni arıyor'}</p>
        <div className="incoming-actions">
          <button type="button" className="decline" onClick={call.decline}>Reddet</button>
          <button type="button" className="accept" onClick={()=>void call.accept()} disabled={call.joining}>{call.joining?'Bağlanıyor…':'Kabul et'}</button>
        </div>
      </section>
    </div>}

    {call.call&&<section className={stage?'call-dock expanded':'call-dock'}>
      <header onClick={()=>setStage(value=>!value)} role="button" title={stage?'Küçült':'Görüntü ve ekranı büyüt'}>
        <span className="call-dot"/>
        <div><b>{headline}</b><small>{call.ringing?<em className="ringing-text">Çalıyor… {duration(call.startedAt)}</em>:<>{duration(call.connectedAt)} · {call.call.participants.length} kişi</>}</small></div>
      </header>

      {stage&&<div className="call-stage">
        {call.localVideo&&<VideoTile stream={call.localVideo} label={call.screenOn?'Sen · ekran':'Sen'} self speaking={call.speaking[me]}/>}
        {videoTiles.map(([userId,media])=><VideoTile key={userId} stream={media.stream} label={call.call?.participants.find(participant=>participant.userId===userId)?.displayName||'Katılımcı'} muted={call.call?.participants.find(participant=>participant.userId===userId)?.muted} speaking={call.speaking[userId]}/>)}
        {roster.filter(participant=>!call.remote[participant.userId]?.hasVideo).map(participant=><AvatarTile key={participant.userId} participant={participant} speaking={call.speaking[participant.userId]} pending={participant.pending} onOpen={()=>onOpenPerson?.(participant.userId)}/>)}
        {!roster.length&&<p className="call-waiting">{call.ringing?`Çalıyor… ${duration(call.startedAt)}`:'Karşı tarafın katılması bekleniyor…'}</p>}
      </div>}

      {!stage&&<div className="call-people">
        {roster.map(participant=><button type="button" key={participant.userId} className={`call-chip ${avatarHue(participant.userId)}${call.speaking[participant.userId]?' speaking':''}${participant.pending?' pending':''}`} title={`${participant.displayName}${participant.pending?' · bekleniyor':''} — profili aç`} onClick={()=>onOpenPerson?.(participant.userId)}>{userInitials(participant.displayName)}</button>)}
        {!roster.length&&<small className="call-alone">Odada tek kişisin</small>}
      </div>}

      <div className="call-controls">
        <button type="button" className={call.muted?'danger-on':undefined} onClick={call.toggleMute} title={call.muted?'Mikrofonu aç':'Sustur'} aria-label="Sustur"><CallIcon name={call.muted?'micOff':'mic'}/></button>
        <button type="button" className={call.deafened?'danger-on':undefined} onClick={call.toggleDeafen} title={call.deafened?'Sesi aç':'Sesi kapat'} aria-label="Kulaklık"><CallIcon name={call.deafened?'headphonesOff':'headphones'}/></button>
        <button type="button" className={call.cameraOn?'active':undefined} onClick={()=>void call.toggleCamera()} title={call.cameraOn?'Kamerayı kapat':'Kamerayı aç'} aria-label="Kamera"><CallIcon name={call.cameraOn?'camera':'cameraOff'}/></button>
        <button type="button" className={call.screenOn?'active':undefined} onClick={()=>void call.toggleScreen()} title={call.screenOn?'Paylaşımı durdur':'Ekran paylaş'} aria-label="Ekran paylaş"><CallIcon name={call.screenOn?'screen':'screenOff'}/></button>
        <button type="button" className={invitePicker?'active':undefined} onClick={()=>setInvitePicker(value=>!value)} title="Kişi ekle" aria-label="Kişi ekle"><CallIcon name="personAdd"/></button>
        <button type="button" className="hangup" onClick={call.leave} title="Aramadan ayrıl" aria-label="Ayrıl"><CallIcon name="hangup"/></button>
      </div>

      {invitePicker&&<div className="call-invite">
        <p>Aramaya ekle</p>
        <div>{invitable.map(person=><button type="button" key={person.id} onClick={()=>{call.invite(person.id);setInvitePicker(false)}}>
          <span className={`call-chip ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>{person.displayName}
        </button>)}
        {!invitable.length&&<small>Eklenecek başka kimse yok.</small>}</div>
      </div>}

      {call.error&&<p className="call-error">{call.error}</p>}
    </section>}
  </>
}
