import { useSyncExternalStore } from 'react'
import { EMPTY_VOICE_STATE, type OverlayCallAction, type OverlayVoiceParticipant, type OverlayVoiceState } from './overlayTypes'
import { overlaySend } from '../overlaySession'
import type { CallParticipant, CallState, IncomingCall } from '../CallProvider'

// ---- Ana pencere tarafı: CallApi -> OverlayVoiceState (saf; overlayVoice.spec.ts) ----
// CallOverlay.tsx bu dönüşümü 150 ms kısarak `gameOverlay.push('voice')` ile iter.
export type VoiceSource={
  me:string
  call:CallState|null
  incoming:IncomingCall|null
  muted:boolean
  deafened:boolean
  micReady:boolean
  connectedAt:number|null
  ringing:boolean
  speaking:Record<string,boolean>
  levels:Record<string,number>
  userVolumes:Record<string,number>
  locallyMuted:Set<string>|string[]
  voiceRooms:Record<string,CallParticipant[]>
}
type Person={id:string;displayName:string;avatarUrl?:string|null}

export function voiceLabel(call:CallState|null,me:string,people:Person[]=[]):string{
  if(!call)return ''
  if(call.kind==='channel')return 'Sesli oda'
  if(call.kind==='group')return 'Grup araması'
  const counterpartId=call.caller===me?call.callee:call.caller
  const participant=call.participants.find(item=>item.userId===counterpartId&&item.userId!==me)||call.participants.find(item=>item.userId!==me)
  if(participant)return participant.displayName
  const invited=call.invited.find(id=>id!==me)
  return people.find(person=>person.id===(counterpartId||invited))?.displayName||'Sesli arama'
}

export function toOverlayVoiceState(source:VoiceSource,people:Person[]=[]):OverlayVoiceState{
  const locallyMuted=source.locallyMuted instanceof Set?source.locallyMuted:new Set(source.locallyMuted)
  const call=source.call
  const participant=(item:CallParticipant,pending=false):OverlayVoiceParticipant=>{
    const person=people.find(row=>row.id===item.userId)
    return{
      userId:item.userId,displayName:item.displayName||person?.displayName||'Katılımcı',avatarUrl:item.avatarUrl??person?.avatarUrl??null,
      muted:!!item.muted,deafened:!!item.deafened,screen:!!item.screen,video:!!item.video,
      speaking:!pending&&!item.muted&&!!source.speaking[item.userId],
      level:pending||item.muted?0:Math.round((source.levels[item.userId]||0)*100)/100,
      locallyMuted:locallyMuted.has(item.userId),
      volume:source.userVolumes?.[item.userId]??100,
      ...(pending?{pending:true}:{}),
    }
  }
  const participants:OverlayVoiceParticipant[]=call?[
    ...call.participants.map(item=>participant(item)),
    ...call.invited.filter(id=>id!==source.me&&!call.participants.some(item=>item.userId===id)).map(id=>participant({userId:id,displayName:people.find(person=>person.id===id)?.displayName||'Katılımcı',muted:false,deafened:false,video:false,screen:false},true)),
  ]:[]
  const voiceRooms:OverlayVoiceState['voiceRooms']={}
  for(const[channelId,members]of Object.entries(source.voiceRooms||{})){
    if(!members?.length)continue
    voiceRooms[channelId]=members.map(item=>({userId:item.userId,displayName:item.displayName,avatarUrl:item.avatarUrl??people.find(person=>person.id===item.userId)?.avatarUrl??null}))
  }
  return{
    active:!!call,
    kind:call?.kind||null,
    callId:call?.id||null,
    channelId:call?.channelId||null,
    label:voiceLabel(call,source.me,people),
    ringing:!!call&&source.ringing,
    connectedAt:call?source.connectedAt:null,
    muted:source.muted,
    deafened:source.deafened,
    micReady:!!call&&source.micReady,
    me:source.me,
    participants,
    incoming:source.incoming?{callId:source.incoming.call.id,from:{userId:source.incoming.from.userId,displayName:source.incoming.from.displayName},kind:source.incoming.call.kind,silent:!!source.incoming.silent}:null,
    voiceRooms,
  }
}

// ---- Overlay tarafı ----
let current:OverlayVoiceState=EMPTY_VOICE_STATE
const listeners=new Set<()=>void>()
let started=false
function start(){
  if(started)return
  started=true
  window.p2overlay?.onPush(push=>{
    if(push.topic!=='voice')return
    const next=push.payload as Partial<OverlayVoiceState>|undefined
    current={...EMPTY_VOICE_STATE,...next,participants:Array.isArray(next?.participants)?next!.participants:[],voiceRooms:next?.voiceRooms&&typeof next.voiceRooms==='object'?next.voiceRooms:{}}
    listeners.forEach(fn=>fn())
  })
}
function subscribe(fn:()=>void){listeners.add(fn);return()=>{listeners.delete(fn)}}

// Ana pencereden aktarılan ses durumu; ilk push gelene kadar boş durum.
export function useOverlayVoice():OverlayVoiceState{
  start()
  return useSyncExternalStore(subscribe,()=>current,()=>current)
}
// Düğmeler ana pencereye tipli komut gönderir; WebRTC overlay'de hiç açılmaz.
export function overlayCallAction(action:OverlayCallAction){overlaySend('call',action)}
