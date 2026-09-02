import { useEffect, useRef } from 'react'
import { io, Socket } from 'socket.io-client'
import { SOCKET_BASE } from './apiBase'

const API=SOCKET_BASE
export type RealtimeHandlers={[event:string]:(payload:any)=>void}

// Tüm pencere için tek bir bağlantı: aynı token ve kuruluş için yeniden kullanılır.
let socket:Socket|null=null,socketKey=''
function ensureSocket(token:string,organizationId:string){
  const key=`${token}|${organizationId}`
  if(socket&&socketKey===key)return socket
  socket?.disconnect()
  socketKey=key
  const next=io(`${API}/stream`,{auth:{token},transports:['websocket','polling'],reconnectionDelay:700,reconnectionDelayMax:4000})
  const join=()=>{next.emit('subscribe',{organizationId})}
  next.on('connect',join)
  socket=next
  return next
}
export function closeRealtime(){socket?.disconnect();socket=null;socketKey=''}
export function emitRealtime(event:string,payload?:unknown){socket?.emit(event,payload)}

// Handler'lar her render'da tazelenir, dinleyiciler yeniden bağlanmaz; böylece
// olay işleyicileri güncel state'i görür ama bağlantı hep aynı kalır.
export function useRealtimeEvents(token:string,organizationId:string,handlers:RealtimeHandlers){
  const latest=useRef(handlers)
  latest.current=handlers
  const events=Object.keys(handlers).sort().join(',')
  useEffect(()=>{
    if(!token||!organizationId)return
    const active=ensureSocket(token,organizationId)
    const bound=events.split(',').filter(Boolean).map(event=>{
      const listener=(payload:any)=>latest.current[event]?.(payload)
      active.on(event,listener)
      return [event,listener] as const
    })
    return()=>{bound.forEach(([event,listener])=>active.off(event,listener))}
  },[token,organizationId,events])
}
