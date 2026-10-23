import { useEffect, useRef, useSyncExternalStore } from 'react'
import { applyLeasedAuth } from './apiBase'
import type { OverlayAuthPush, OverlayPush, OverlayPushTopic, OverlaySendTopic, OverlaySnapshot } from './overlay/overlayTypes'

// Overlay penceresinin oturumu: kendi girişi/soketi yok. Ana pencere `auth` (kullanıcı +
// kiralanan erişim jetonu) ve `snapshot` (ekip, etkinlikler, odalar, okunmamışlar) iter;
// buradaki hook'lar bu push'ları React'e taşır. Açılışta `auth-request` gönderilir.

export type OverlaySessionState={
  ready:boolean                                  // ilk auth geldi mi
  user:Record<string,unknown>|null
  accessToken:string
  organization:Record<string,unknown>|null
  projects:Record<string,unknown>[]
  selectedProjectId:string|null
  snapshot:OverlaySnapshot|null
}
const EMPTY:OverlaySessionState={ready:false,user:null,accessToken:'',organization:null,projects:[],selectedProjectId:null,snapshot:null}
let state=EMPTY
const listeners=new Set<()=>void>()
function set(next:OverlaySessionState){state=next;listeners.forEach(fn=>fn())}

let started=false
function start(){
  if(started)return
  started=true
  const bridge=window.p2overlay
  if(!bridge)return
  bridge.onPush((push:OverlayPush)=>{
    if(push.topic==='auth'){
      const auth=push.payload as OverlayAuthPush
      if(!auth?.accessToken)return
      applyLeasedAuth({accessToken:auth.accessToken,user:auth.user})
      set({...state,ready:true,user:auth.user||null,accessToken:auth.accessToken,organization:auth.organization||null,projects:auth.projects||[],selectedProjectId:auth.selectedProjectId??null})
    }else if(push.topic==='snapshot'){
      const snapshot=push.payload as OverlaySnapshot
      set({...state,snapshot,organization:snapshot.organization||state.organization,projects:snapshot.projects||state.projects,selectedProjectId:snapshot.selectedProjectId??state.selectedProjectId})
    }
  })
  bridge.send('auth-request',{reason:'mount'})
}

export function overlaySend(topic:OverlaySendTopic,payload:unknown){window.p2overlay?.send(topic,payload)}
export function overlaySessionInfo(){try{return window.p2overlay?.session()||null}catch{return null}}

function subscribe(fn:()=>void){listeners.add(fn);return()=>{listeners.delete(fn)}}
export function useOverlayAuth(){
  start()
  return useSyncExternalStore(subscribe,()=>state,()=>state)
}
export function useOverlaySnapshot():OverlaySnapshot|null{
  start()
  return useSyncExternalStore(subscribe,()=>state.snapshot,()=>state.snapshot)
}

// Belirli bir push konusunu dinler; işleyici her render'da tazelenir, dinleyici sabit kalır.
export function useOverlayPush<T=unknown>(topic:OverlayPushTopic,handler:(payload:T)=>void){
  const latest=useRef(handler)
  latest.current=handler
  useEffect(()=>{
    start()
    return window.p2overlay?.onPush(push=>{if(push.topic===topic)latest.current(push.payload as T)})
  },[topic])
}
