import { useEffect, useRef, useState } from 'react'
import { emitRealtime, useRealtimeEvents } from './realtime'

export type TypingPayload={userId:string;displayName:string;channelId:string|null;conversationId:string|null;typing:boolean}
type Scope={organizationId?:string;channelId?:string;conversationId?:string}

const REPEAT=2500   // yazmaya devam ederken bilgiyi kaç ms'de bir tazeleyelim
const STOP=4000     // tuşa basılmayınca kaç ms sonra "yazmıyor" sayalım
const EXPIRE=6000   // karşı taraftan haber gelmezse kaç ms sonra listeden düşsün

// Yazarken karşı tarafa haber verir; boşta kalınca kendiliğinden durur.
export function useTypingBroadcast(scope:Scope){
  const last=useRef(0),stopTimer=useRef<number|undefined>(undefined),current=useRef(scope)
  current.current=scope
  function send(typing:boolean){
    const{organizationId,channelId,conversationId}=current.current
    if(!conversationId&&!(organizationId&&channelId))return
    emitRealtime('typing',{organizationId,channelId,conversationId,typing})
  }
  useEffect(()=>()=>{window.clearTimeout(stopTimer.current);send(false)},[scope.channelId,scope.conversationId])
  return{
    typing(){
      const now=Date.now()
      if(now-last.current>REPEAT){last.current=now;send(true)}
      window.clearTimeout(stopTimer.current)
      stopTimer.current=window.setTimeout(()=>{last.current=0;send(false)},STOP)
    },
    stop(){window.clearTimeout(stopTimer.current);last.current=0;send(false)},
  }
}

// Şu an yazan kişilerin adları (kendin hariç).
export function useTypists(token:string,organizationId:string,selfId:string,match:(payload:TypingPayload)=>boolean){
  const[people,setPeople]=useState<Record<string,{name:string;at:number}>>({})
  const filter=useRef(match)
  filter.current=match
  useRealtimeEvents(token,organizationId,{
    typing:(payload:TypingPayload)=>{
      if(payload.userId===selfId||!filter.current(payload))return
      setPeople(current=>{
        if(!payload.typing){if(!current[payload.userId])return current;const next={...current};delete next[payload.userId];return next}
        return{...current,[payload.userId]:{name:payload.displayName,at:Date.now()}}
      })
    },
  })
  useEffect(()=>{
    const timer=window.setInterval(()=>setPeople(current=>{
      const now=Date.now()
      const next=Object.fromEntries(Object.entries(current).filter(([,value])=>now-value.at<EXPIRE))
      return Object.keys(next).length===Object.keys(current).length?current:next
    }),1200)
    return()=>window.clearInterval(timer)
  },[])
  return Object.entries(people).map(([userId,item])=>({userId,name:item.name}))
}

export function typingText(names:string[]){
  if(!names.length)return ''
  if(names.length<=3)return `${names.join(', ')} yazıyor…`
  return `${names.slice(0,2).join(', ')} ve ${names.length-2} kişi daha yazıyor…`
}
