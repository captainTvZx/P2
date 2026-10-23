import { useEffect, useRef, useSyncExternalStore } from 'react'
import { OVERLAY_MODE } from './settings'
import { refreshClientSession } from './apiBase'
import { showCallPanel } from './callBus'
import { emitRealtime, overlayMirror } from './realtime'
import type { OverlayAuthPush, OverlayCallAction, OverlayOpenInApp, OverlaySend, OverlaySnapshot } from './overlay/overlayTypes'

// Ana pencere tarafı: oyun içi overlay oturumlarına kimlik (kiralanan jeton), anlık görüntü
// (ekip, etkinlikler, odalar, okunmamışlar) iter ve overlay'den gelen `overlay:send`
// mesajlarını (emit / call / open-in-app / auth-request / toast-action) işler.

// ---- Hazır oturumlar (pid başına; yalnız kare çizen pencere "hazır" olur) ----
export type OverlaySessionRef={pid:number;projectId:string}
let sessions:OverlaySessionRef[]=[]
const listeners=new Set<()=>void>()
function setSessions(next:OverlaySessionRef[]){
  sessions=next
  overlayMirror.enabled=next.length>0
  listeners.forEach(fn=>fn())
}
export function getOverlaySessions(){return sessions}
export function isOverlayReady(projectId?:string){return projectId?sessions.some(session=>session.projectId===projectId):sessions.length>0}
export function useOverlaySessions(){
  return useSyncExternalStore(fn=>{listeners.add(fn);return()=>{listeners.delete(fn)}},()=>sessions,()=>sessions)
}
let readyBound=false
function bindReady(onReady:(info:OverlaySessionRef)=>void){
  if(readyBound)return
  readyBound=true
  window.p2?.gameOverlay?.onReady(info=>{
    const rest=sessions.filter(session=>session.pid!==info.pid)
    if(info.ready){
      setSessions([...rest,{pid:info.pid,projectId:info.projectId}])
      onReady({pid:info.pid,projectId:info.projectId})
      // CallOverlay ses durumunu, Workspace anlık görüntüyü yeniden iter.
      window.dispatchEvent(new CustomEvent('p2:overlay-ready',{detail:info}))
    }else setSessions(rest)
  })
}
const readyHandlers=new Set<(info:OverlaySessionRef)=>void>()

type CallApiLike={
  toggleMute:()=>void;toggleDeafen:()=>void;leave:()=>void;accept:()=>Promise<void>|void;decline:()=>void
  joinChannel:(channelId:string,ring?:boolean)=>Promise<void>|void;joinGroup?:(groupId:string,ring?:boolean)=>Promise<void>|void;toggleScreen?:()=>Promise<void>|void;toggleCamera?:()=>Promise<void>|void;invite:(userId:string)=>void;startDirect:(userId:string)=>Promise<void>|void
  setUserVolume:(userId:string,volume:number)=>void;toggleLocalMute:(userId:string)=>void
}
export type OverlayOpeners={
  person?:(id:string)=>void
  channel?:(id:string,projectId?:string)=>void
  group?:(id:string)=>void
  task?:(taskId:string,boardId?:string,projectId?:string)=>void
  profile?:(id:string)=>void
  settings?:(section?:string)=>void
  library?:(projectId?:string)=>void
  overview?:(projectId?:string)=>void
  url?:(url:string)=>void
}
export type OverlayMirrorInput={
  auth:{user:object;accessToken:string}
  organization:object|null
  callApi:CallApiLike
  people:object[]
  activities:Record<string,{kind:string;projectName:string;detail?:string;percent?:number}|null|undefined>
  voiceRooms:Record<string,{userId:string;displayName:string;avatarUrl?:string|null}[]>
  projects:object[]
  selectedProjectId:string|null
  unread:Record<string,number>
  channelUnread:Record<string,number>
  prefs:Record<string,{starred:boolean;notify:string;hidden?:boolean}>
  channels?:{id:string;name:string;type:string;projectId:string}[]
  openers:OverlayOpeners
}
const SNAPSHOT_THROTTLE_MS=500

function buildAuth(input:OverlayMirrorInput,accessToken=input.auth.accessToken):OverlayAuthPush{
  return{user:input.auth.user as Record<string,unknown>,accessToken,organization:(input.organization||null) as OverlayAuthPush['organization'],projects:input.projects as Record<string,unknown>[],selectedProjectId:input.selectedProjectId}
}
function buildSnapshot(input:OverlayMirrorInput):OverlaySnapshot{
  const activities:OverlaySnapshot['activities']={}
  for(const[userId,activity]of Object.entries(input.activities))activities[userId]=activity?{kind:activity.kind,projectName:activity.projectName,detail:activity.detail,percent:activity.percent,...(activity as {projectId?:string}).projectId?{projectId:(activity as {projectId?:string}).projectId}:{}}:null
  const voiceRooms:OverlaySnapshot['voiceRooms']={}
  for(const[channelId,members]of Object.entries(input.voiceRooms||{}))if(members?.length)voiceRooms[channelId]=members.map(member=>({userId:member.userId,displayName:member.displayName,avatarUrl:member.avatarUrl??null}))
  return{organization:(input.organization||null) as OverlaySnapshot['organization'],projects:input.projects as Record<string,unknown>[],selectedProjectId:input.selectedProjectId,activities,voiceRooms,unread:input.unread,channelUnread:input.channelUnread,people:input.people as Record<string,unknown>[],prefs:input.prefs,channels:input.channels||[]}
}

export function useOverlayMirror(input:OverlayMirrorInput){
  const latest=useRef(input)
  latest.current=input
  const bridge=OVERLAY_MODE?undefined:window.p2?.gameOverlay // oyun içi pencere kendisi ayna değildir

  // Hazır oturum takibi (modül tekili; Workspace yeniden kurulsa da liste kalır).
  useEffect(()=>{
    const onReady=()=>{
      const push=window.p2?.gameOverlay?.push
      if(!push)return
      push('auth',buildAuth(latest.current))
      push('snapshot',buildSnapshot(latest.current))
    }
    readyHandlers.add(onReady)
    bindReady(info=>readyHandlers.forEach(fn=>fn(info)))
    // Hook bağlanmadan önce hazır olmuş oturum varsa (örn. kuruluş değişimi) hemen besle.
    if(sessions.length)onReady()
    return()=>{readyHandlers.delete(onReady)}
  },[])

  // Kimlik: jeton değişince ve merkezi yenilemede (p2:auth-refreshed) yeniden itilir.
  useEffect(()=>{
    if(!bridge||!sessions.length)return
    bridge.push('auth',buildAuth(latest.current))
  },[bridge,input.auth.accessToken,input.selectedProjectId])
  useEffect(()=>{
    const refreshed=(event:Event)=>{
      const token=String((event as CustomEvent).detail?.accessToken||'')
      if(!sessions.length||!token)return
      window.p2?.gameOverlay?.push('auth',buildAuth(latest.current,token))
    }
    window.addEventListener('p2:auth-refreshed',refreshed)
    return()=>window.removeEventListener('p2:auth-refreshed',refreshed)
  },[])

  // Anlık görüntü: 500 ms kısılmış (sondan tetiklemeli), yalnız hazır oturum varken.
  const snapshotTimer=useRef(0)
  useEffect(()=>{
    if(!bridge||!sessions.length)return
    if(snapshotTimer.current)return
    snapshotTimer.current=window.setTimeout(()=>{
      snapshotTimer.current=0
      if(!sessions.length)return
      window.p2?.gameOverlay?.push('snapshot',buildSnapshot(latest.current))
    },SNAPSHOT_THROTTLE_MS)
  },[bridge,input.organization,input.projects,input.selectedProjectId,input.activities,input.voiceRooms,input.unread,input.channelUnread,input.people,input.prefs,input.channels])
  useEffect(()=>()=>{if(snapshotTimer.current)window.clearTimeout(snapshotTimer.current)},[])

  // Overlay'den gelen mesajlar.
  useEffect(()=>{
    const api=window.p2?.gameOverlay
    if(!api)return
    return api.onSend((message:OverlaySend)=>{
      const current=latest.current
      try{
        switch(message.topic){
          case 'auth-request':{
            const reason=String((message.payload as {reason?:string}|undefined)?.reason||'')
            if(reason==='expired'){
              // Overlay 401 aldı: ana pencerenin ortak kuyruğundan tek yenileme, sonra yeni jeton itilir.
              void refreshClientSession().then(auth=>api.push('auth',buildAuth(latest.current,auth.accessToken))).catch(()=>api.push('auth',buildAuth(latest.current)))
            }else{
              api.push('auth',buildAuth(current))
              api.push('snapshot',buildSnapshot(current))
            }
            break
          }
          case 'emit':{
            const{event,payload}=(message.payload||{}) as {event?:string;payload?:unknown}
            if(event)emitRealtime(event,payload)
            break
          }
          case 'call':
            applyCallAction(current.callApi,message.payload as OverlayCallAction)
            break
          case 'open-in-app':
            applyOpenInApp(current.openers,message.payload as OverlayOpenInApp)
            break
          case 'toast-action':
            window.dispatchEvent(new CustomEvent('p2:overlay-toast-action',{detail:{...(message.payload as object),pid:message.pid}}))
            break
        }
      }catch(problem){console.error('[overlay] mesaj işlenemedi',message.topic,problem)}
    })
  },[])
}

function applyCallAction(api:CallApiLike,action:OverlayCallAction){
  if(!action||typeof action!=='object')return
  switch(action.action){
    case 'mute':api.toggleMute();break
    case 'deafen':api.toggleDeafen();break
    case 'leave':api.leave();break
    case 'accept':void api.accept();break
    case 'decline':api.decline();break
    case 'silence':break   // overlay kartı kendisi kapatır; ana pencerede zil CallOverlay'in yerel durumunda
    case 'open':showCallPanel();break
    case 'toggleScreen':void api.toggleScreen?.();break
    case 'toggleCamera':void api.toggleCamera?.();break
    case 'joinChannel':void api.joinChannel(action.channelId);break
    case 'joinGroup':void api.joinGroup?.(action.groupId);break
    case 'invite':api.invite(action.userId);break
    case 'startDirect':void api.startDirect(action.userId);break
    case 'setUserVolume':api.setUserVolume(action.userId,Math.max(0,Math.min(200,Number(action.volume)||0)));break
    case 'toggleLocalMute':api.toggleLocalMute(action.userId);break
  }
}

function applyOpenInApp(openers:OverlayOpeners,detail:OverlayOpenInApp){
  if(!detail||typeof detail!=='object')return
  const missing=()=>console.info('[overlay] open-in-app için ana pencerede açıcı yok:',detail.kind,detail)
  switch(detail.kind){
    case 'person':detail.id&&openers.person?openers.person(detail.id):missing();break
    case 'channel':detail.id&&openers.channel?openers.channel(detail.id,detail.projectId):missing();break
    case 'group':detail.id&&openers.group?openers.group(detail.id):missing();break
    case 'task':detail.id&&openers.task?openers.task(detail.id,detail.boardId,detail.projectId):missing();break
    case 'profile':detail.id&&openers.profile?openers.profile(detail.id):missing();break
    case 'settings':openers.settings?openers.settings(detail.section):missing();break
    case 'library':openers.library?openers.library(detail.projectId):missing();break
    case 'overview':openers.overview?openers.overview(detail.projectId):missing();break
    case 'url':detail.url&&openers.url?openers.url(detail.url):missing();break
    default:missing()
  }
}
