import { useCallback, useEffect, useRef } from 'react'
import { OVERLAY_MODE } from './settings'
import type { OverlayNotificationSettings, OverlayToast, OverlayToastActionDetail, OverlayToolId } from './overlay/overlayTypes'
import { getOverlaySessions, useOverlaySessions, type OverlaySessionRef } from './overlayMirror'

// Oyun içi bildirim yönlendiricisi. `routeNotification` saftır (overlayNotify.spec.ts karar tablosu):
// bir olay + bağlam → overlay kartı (ya da null) + masaüstü bildiriminin bastırılıp bastırılmayacağı.
// Kural: oyun için overlay hazırsa kart overlay'e gider, OS bildirimi ve masaüstü toast'ı bastırılır,
// okunmamış sayaçları yine artar. Overlay yoksa (ya da ilgili anahtar kapalıysa) mevcut davranış sürer.

type Person={id:string;displayName:string;avatarUrl?:string|null}
export type NotifyInput=
  |{type:'dm';from:Person;content:string}
  |{type:'channel-message';projectId:string;channelId:string;channelName?:string;author:Person;content:string;mentioned:boolean;rule:string}
  |{type:'group-message';groupId:string;groupName?:string;author:Person;content:string}
  |{type:'play';user:Person;projectId:string;projectName:string}
  |{type:'play-stop';user:Person;projectId?:string;projectName:string}
  |{type:'call-incoming';callId:string;from:Person;kind:'direct'|'channel'|'group';silent:boolean}
  |{type:'call-ended';callId:string}
  |{type:'task-assigned';activityId:string;taskId:string;boardId:string|null;projectId:string|null;preview:string;actor:Person}
  |{type:'release';phase:'published'|'live';projectId:string;projectName?:string;version:string}
  |{type:'download-done';projectId:string;projectName?:string}

export type NotifyContext={
  overlayReady:boolean                       // en az bir oyun overlay'i kare çiziyor
  me:string
  settings:{overlayNotifications:OverlayNotificationSettings;playToasts:boolean}
  now?:number                                // kart kimliği için (testlerde sabit)
}
export type OpenTarget={window:OverlayToolId;target?:unknown}
export type NotifyPayload={open?:OpenTarget;call?:{callId:string}}
export type NotifyDecision={overlay:OverlayToast|null;suppressDesktop:boolean;dismiss?:string}

const PASS:NotifyDecision={overlay:null,suppressDesktop:false}
function preview(content:string){return content.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,'$1').replace(/\*\*|\*|`/g,'').replace(/\s+/g,' ').trim().slice(0,140)||'Yeni mesaj'}
function avatarOf(person:Person):OverlayToast['avatar']{return{userId:person.id,name:person.displayName,url:person.avatarUrl??null}}
function card(ctx:NotifyContext,tag:string,toast:Omit<OverlayToast,'id'|'tag'>):NotifyDecision{
  return{overlay:{id:`${tag}:${ctx.now??Date.now()}`,tag,...toast},suppressDesktop:true}
}

export function routeNotification(input:NotifyInput,ctx:NotifyContext):NotifyDecision{
  if(!ctx.overlayReady)return PASS
  const on=ctx.settings.overlayNotifications
  switch(input.type){
    case 'dm':{
      if(!on.messages||input.from.id===ctx.me)return PASS
      const payload:NotifyPayload={open:{window:'chat',target:{kind:'person',id:input.from.id}}}
      return card(ctx,`dm:${input.from.id}`,{kind:'message',title:input.from.displayName,text:preview(input.content),avatar:avatarOf(input.from),actions:[{id:'open-chat',label:'Aç',primary:true}],payload})
    }
    case 'channel-message':{
      if(!on.messages||input.author.id===ctx.me)return PASS
      // Kanal tercihi: susturulmuşsa hiç; "Tümü" değilse yalnız bahsetmelerde (ana pencereyle aynı).
      if(input.rule==='MUTED')return PASS
      if(!input.mentioned&&input.rule!=='ALL')return PASS
      const payload:NotifyPayload={open:{window:'chat',target:{kind:'channel',id:input.channelId,projectId:input.projectId}}}
      return card(ctx,`channel:${input.channelId}`,{kind:input.mentioned?'mention':'message',title:`${input.author.displayName}${input.channelName?` · #${input.channelName}`:''}`,text:preview(input.content),avatar:avatarOf(input.author),actions:[{id:'open-chat',label:'Aç',primary:true}],payload})
    }
    case 'group-message':{
      if(!on.messages||input.author.id===ctx.me)return PASS
      const payload:NotifyPayload={open:{window:'chat',target:{kind:'group',id:input.groupId}}}
      return card(ctx,`group:${input.groupId}`,{kind:'message',title:`${input.author.displayName}${input.groupName?` · ${input.groupName}`:''}`,text:preview(input.content),avatar:avatarOf(input.author),actions:[{id:'open-chat',label:'Aç',primary:true}],payload})
    }
    case 'play':{
      if(!on.play||!ctx.settings.playToasts||input.user.id===ctx.me)return PASS
      const payload:NotifyPayload={open:{window:'summary',target:{projectId:input.projectId}}}
      return card(ctx,`play:${input.user.id}`,{kind:'play',title:input.user.displayName,text:`${input.projectName} projesini çalıştırdı`,avatar:avatarOf(input.user),actions:[{id:'open-overview',label:'Özet'}],payload,duration:5000})
    }
    case 'play-stop':{
      if(!on.play||!ctx.settings.playToasts||input.user.id===ctx.me)return PASS
      return card(ctx,`play:${input.user.id}`,{kind:'play-stop',title:input.user.displayName,text:`${input.projectName} oyunundan çıktı`,avatar:avatarOf(input.user),duration:4000})
    }
    case 'call-incoming':{
      if(!on.calls||input.silent||input.from.id===ctx.me)return PASS
      const payload:NotifyPayload={call:{callId:input.callId}}
      return card(ctx,`call:${input.callId}`,{kind:'call',title:input.from.displayName,text:input.kind==='channel'?'Seni sesli odaya çağırıyor':input.kind==='group'?'Grup araması':'Seni arıyor',avatar:avatarOf(input.from),sticky:true,actions:[{id:'accept',label:'Kabul et',primary:true},{id:'decline',label:'Reddet'},{id:'silence',label:'Sustur'}],payload})
    }
    case 'call-ended':
      return{overlay:null,suppressDesktop:false,dismiss:`call:${input.callId}`}
    case 'task-assigned':{
      if(!on.tasks||input.actor.id===ctx.me)return PASS
      const payload:NotifyPayload={open:{window:'tasks',target:{taskId:input.taskId,boardId:input.boardId,projectId:input.projectId}}}
      return card(ctx,`task:${input.taskId}`,{kind:'task',title:`${input.actor.displayName} sana bir görev atadı`,text:preview(input.preview),avatar:avatarOf(input.actor),actions:[{id:'open-task',label:'Görevi aç',primary:true}],payload})
    }
    case 'release':{
      if(!on.releases)return PASS
      const payload:NotifyPayload={open:{window:'summary',target:{projectId:input.projectId}}}
      return card(ctx,`release:${input.projectId}`,{kind:'release',title:`${input.projectName||'Kütüphane'} · ${input.version}`,text:input.phase==='live'?'Sürüm canlıya alındı.':'Yeni sürüm yayınlandı.',actions:[{id:'open-overview',label:'Özet'}],payload})
    }
    case 'download-done':{
      if(!on.downloads)return PASS
      const payload:NotifyPayload={open:{window:'library',target:{projectId:input.projectId}}}
      return card(ctx,`download:${input.projectId}`,{kind:'download',title:input.projectName||'Kütüphane',text:'İndirme tamamlandı.',actions:[{id:'open-library',label:'Kütüphane'}],payload})
    }
  }
  return PASS
}

// Toast düğmesi → yapılacak iş (saf; hook uygular).
export type ToastActionPlan={kind:'open';open:OpenTarget}|{kind:'call';action:'accept'|'decline'|'silence';callId:string}|{kind:'none'}
export function planToastAction(detail:OverlayToastActionDetail):ToastActionPlan{
  const payload=(detail.payload||{}) as NotifyPayload
  if(detail.actionId.startsWith('open-')&&payload.open)return{kind:'open',open:payload.open}
  if((detail.actionId==='accept'||detail.actionId==='decline'||detail.actionId==='silence')&&payload.call)return{kind:'call',action:detail.actionId,callId:payload.call.callId}
  return{kind:'none'}
}

// ---- Workspace kancası ----
type Activities=Record<string,{kind:string;projectId?:string;projectName:string}|null|undefined>
export type OverlayNotificationsInput={
  me:string
  settings:NotifyContext['settings']
  people:Person[]
  activities:Activities
  callApi:{incoming:{call:{id:string;kind:'direct'|'channel'|'group'};from:{userId:string;displayName:string};silent?:boolean}|null;accept:()=>Promise<void>|void;decline:()=>void}
  projectName?:(projectId:string)=>string|undefined
}

// Workspace'teki mevcut olay işleyicileri `notify(...)` çağırır; true dönerse kart overlay'e
// gitmiştir ve masaüstü bildirimi/toast'ı atlanır. Gelen arama, oyundan çıkış (presence
// geçişi) ve indirme bitişi burada kendiliğinden izlenir.
export function useOverlayNotifications(input:OverlayNotificationsInput){
  const sessions=useOverlaySessions()
  const latest=useRef(input)
  latest.current=input
  const ready=sessions.length>0
  const ctx=useCallback(():NotifyContext=>({overlayReady:getOverlaySessions().length>0,me:latest.current.me,settings:latest.current.settings}),[])

  const notify=useCallback((event:NotifyInput):boolean=>{
    const decision=routeNotification(event,ctx())
    const bridge=OVERLAY_MODE?undefined:window.p2?.gameOverlay
    if(!bridge)return false
    if(decision.dismiss)bridge.event({type:'toast-dismiss',tag:decision.dismiss})
    if(decision.overlay)bridge.toast('all',decision.overlay)
    return decision.suppressDesktop
  },[ctx])

  // Gelen arama: yapışkan kart; arama bitince/yanıtlanınca kart düşer.
  const incoming=input.callApi.incoming
  const incomingId=incoming?.call.id||null
  useEffect(()=>{
    if(!incomingId||!incoming)return
    notify({type:'call-incoming',callId:incomingId,from:{id:incoming.from.userId,displayName:incoming.from.displayName,avatarUrl:latest.current.people.find(person=>person.id===incoming.from.userId)?.avatarUrl},kind:incoming.call.kind,silent:!!incoming.silent})
    return()=>{notify({type:'call-ended',callId:incomingId})}
  },[incomingId,notify])

  // Oyundan çıkış: presence:activity geçişi playing → başka/yok.
  const previous=useRef<Activities>({})
  useEffect(()=>{
    const before=previous.current
    previous.current=input.activities
    if(!ready)return
    for(const[userId,was]of Object.entries(before)){
      if(was?.kind!=='playing'||userId===latest.current.me)continue
      const now=input.activities[userId]
      if(now?.kind==='playing'&&now.projectId===was.projectId)continue
      const person=latest.current.people.find(item=>item.id===userId)
      if(!person)continue
      notify({type:'play-stop',user:person,projectId:was.projectId,projectName:was.projectName})
    }
  },[input.activities,ready,notify])

  // İndirme bitti (ana süreç kütüphane olayı).
  useEffect(()=>window.p2?.library?.onEvent(event=>{
    if(event.type!=='job'||event.phase!=='done')return
    notify({type:'download-done',projectId:event.projectId,projectName:latest.current.projectName?.(event.projectId)})
  }),[notify])

  // Toast düğmeleri: aç → ilgili overlay oturumunu görünür yap + pencereyi aç; arama → CallApi.
  useEffect(()=>{
    const handled=new Map<string,number>()
    const apply=(detail:OverlayToastActionDetail&{pid?:number})=>{
      const key=`${detail.toastId}:${detail.actionId}`,now=Date.now()
      if((handled.get(key)||0)>now-1000)return
      handled.set(key,now)
      const plan=planToastAction(detail)
      const bridge=OVERLAY_MODE?undefined:window.p2?.gameOverlay
      if(plan.kind==='open'){
        const targets:OverlaySessionRef[]=detail.pid?getOverlaySessions().filter(session=>session.pid===detail.pid):getOverlaySessions()
        targets.forEach(session=>bridge?.setVisible(session.pid,true))
        bridge?.push('open',plan.open)
      }else if(plan.kind==='call'){
        const api=latest.current.callApi
        if(api.incoming?.call.id!==plan.callId)return
        if(plan.action==='accept')void api.accept()
        else if(plan.action==='decline')api.decline()
      }
    }
    const unsubscribe=window.p2?.gameOverlay?.onToastAction(apply)
    const viaSend=(event:Event)=>apply((event as CustomEvent).detail)
    window.addEventListener('p2:overlay-toast-action',viaSend)
    return()=>{unsubscribe?.();window.removeEventListener('p2:overlay-toast-action',viaSend)}
  },[])

  return{notify,overlayReady:ready}
}
