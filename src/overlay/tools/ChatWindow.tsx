import { lazy, Suspense, useEffect, useMemo } from 'react'
import { useOverlayAuth } from '../../overlaySession'
import { useOpenTarget } from '../openTargets'
import { pushOverlayToast } from '../toastStore'
import type { OverlayToolProps } from '../overlayTypes'
import type { OverlayChatMode } from '../../Workspace'

// Sohbet penceresi: ana penceredeki GERÇEK Workspace (kanal kenar çubuğu + ChannelContent /
// DirectMessages / GroupChat, thread, tepkiler, düzenle/sil, arama, görüntüleyici…) overlay
// modunda çizilir. Veri aynalanan soket + kiralanan jetonla gelir; WebRTC OverlayCallProvider ile
// ana pencereye yönlendirilir. Workspace ağır olduğu için tembel yüklenir.
const Workspace=lazy(()=>import('../../Workspace'))
type ChatTarget=NonNullable<OverlayChatMode['target']>

export default function ChatWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const organization=auth.organization as {id:string}|null
  const open=useOpenTarget<ChatTarget>('chat')
  const target=useMemo<ChatTarget|null>(()=>open?.target&&typeof open.target==='object'&&'kind' in open.target?{...open.target}:null,[open?.at])
  // Sohbet bileşenlerinin uygulama içi kısa bildirimleri (ör. indirme hatası) overlay kartına çevrilir.
  useEffect(()=>{
    const listener=(event:Event)=>{
      const detail=(event as CustomEvent).detail
      if(!detail)return
      if(typeof detail==='string'){pushOverlayToast({id:`app-${Date.now()}`,kind:'info',title:detail});return}
      if(typeof detail==='object'){
        const item=detail as {text?:string;title?:string;tone?:string;kind?:string}
        if(item.kind==='play')return
        pushOverlayToast({id:`app-${Date.now()}`,kind:item.tone==='error'?'error':item.tone==='ok'?'success':'info',title:item.title||item.text||'P2',text:item.title?item.text:undefined,duration:5000})
      }
    }
    window.addEventListener('p2:toast',listener)
    return()=>window.removeEventListener('p2:toast',listener)
  },[])
  if(!auth.ready||!auth.accessToken||!auth.user||!organization)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  if(props.pinned&&props.minimized)return null
  return <div className="overlay-chat">
    <Suspense fallback={<div className="p2-empty"><small>Sohbet yükleniyor…</small></div>}>
      <Workspace auth={{user:auth.user as never,accessToken:auth.accessToken}} organization={organization as never} logout={()=>{}} overlay={{projectId:props.projectId,target}}/>
    </Suspense>
  </div>
}
