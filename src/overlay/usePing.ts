import { useEffect, useState } from 'react'
import { API_BASE } from '../apiBase'

// Aramada değilken HUD'daki "ms" değeri: P2 sunucusuna GET /health gidiş-dönüş süresi (5 sn'de bir).
// Aramadayken host, ana pencerenin WebRTC RTT'sini gönderir; bu hook yalnız yedektir.
export function usePing(enabled:boolean):number|null{
  const[ping,setPing]=useState<number|null>(null)
  useEffect(()=>{
    if(!enabled){setPing(null);return}
    let alive=true
    const probe=async()=>{
      const started=performance.now()
      try{
        const controller=new AbortController()
        const timer=window.setTimeout(()=>controller.abort(),4000)
        await fetch(`${API_BASE}/health`,{cache:'no-store',signal:controller.signal})
        window.clearTimeout(timer)
        if(alive)setPing(Math.round(performance.now()-started))
      }catch{if(alive)setPing(null)}
    }
    void probe()
    const timer=window.setInterval(()=>{void probe()},5000)
    return()=>{alive=false;window.clearInterval(timer)}
  },[enabled])
  return ping
}
