import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { request } from '../api/client'
import { useLibraryStore, type HubData, type HubPost, type ReleaseView } from '../library'
import { useOverlayAuth } from '../overlaySession'
import { useRealtimeEvents } from '../realtime'
import { useTeam, type TeamPerson } from './useTeam'

// Oyun özeti (overlay "Oyun özeti" penceresi): kurulu sürüm (kütüphane IPC) + canlı sürüm (REST) +
// vitrin (oynama süresi, son gönderiler) + kim oynuyor (ekip presence). Canlı: `hub:changed`,
// `release:published`/`release:live` (ana pencereden aynalanır) ve kütüphane `played`/`running` olayları.

export type OverviewProject={id:string;name:string;coverUrl?:string|null;iconUrl?:string|null}
export type OverviewPlaytime={mine:number;team:number;players:number;lastPlayedAt:string|null}
export type UseGameOverviewInput={
  token:string
  organizationId:string
  projectId:string
  me:string
  projects?:OverviewProject[]      // verilirse overlay oturumundaki liste yerine kullanılır (ana pencere)
  enabled?:boolean                 // false: istek atılmaz (pencere kapalı/küçük)
}
export type GameOverview={
  project:OverviewProject|null
  installed:LibraryInstalled|null
  running:boolean
  live:ReleaseView|null
  updateAvailable:boolean
  playtime:OverviewPlaytime
  posts:HubPost[]
  playing:TeamPerson[]
  loading:boolean
  error:string
  refresh():Promise<void>
}

export const POST_LIMIT=5
const EMPTY_PLAYTIME:OverviewPlaytime={mine:0,team:0,players:0,lastPlayedAt:null}

// Güncelleme var mı: canlı sürüm kurulu olandan farklı ve kullanıcı sürümü elle sabitlememiş.
export function hasUpdate(installed:{releaseId:string;pinned:boolean;channel:string}|null|undefined,live:{id:string;channel:string}|null|undefined){
  if(!installed||!live)return false
  if(installed.channel&&live.channel&&installed.channel!==live.channel)return false
  return live.id!==installed.releaseId&&!installed.pinned
}

export function useGameOverview(input:UseGameOverviewInput):GameOverview{
  const{token,organizationId,projectId,me}=input
  const enabled=input.enabled!==false&&Boolean(token&&organizationId&&projectId)
  const auth=useOverlayAuth()
  const store=useLibraryStore()
  const installed=store.projects[projectId]||null
  const running=store.running.includes(projectId)
  const channel=installed?.channel||''
  const projects=(input.projects||(auth.projects as unknown as OverviewProject[]))||[]
  const project=useMemo(()=>projects.find(item=>item.id===projectId)||null,[projects,projectId])

  const[hub,setHub]=useState<HubData|null>(null)
  const[live,setLive]=useState<ReleaseView|null>(null)
  const[loading,setLoading]=useState(false)
  const[error,setError]=useState('')
  const base=`/organizations/${organizationId}/projects/${projectId}`
  // Projeden projeye geçişte eski yanıt yeni projeye yazılmasın (her istek türü kendi sayacını tutar).
  const hubSeq=useRef(0),liveSeq=useRef(0)

  const loadHub=useCallback(async()=>{
    if(!enabled)return
    const id=++hubSeq.current
    try{const data=await request<HubData>(`${base}/hub`,token);if(id===hubSeq.current)setHub(data)}
    catch(err){if(id===hubSeq.current)setError((err as {message?:string})?.message||'Vitrin yüklenemedi.')}
  },[enabled,base,token])
  const loadLive=useCallback(async()=>{
    if(!enabled)return
    const id=++liveSeq.current
    try{
      const data=await request<{release:ReleaseView|null}>(`${base}/releases/latest${channel?`?channel=${encodeURIComponent(channel)}`:''}`,token)
      if(id===liveSeq.current)setLive(data.release)
    }catch{if(id===liveSeq.current)setLive(null)}
  },[enabled,base,token,channel])
  const refresh=useCallback(async()=>{
    if(!enabled)return
    setLoading(true);setError('')
    await Promise.all([loadHub(),loadLive()])
    setLoading(false)
  },[enabled,loadHub,loadLive])

  useEffect(()=>{setHub(null);setLive(null);setError('')},[projectId])
  useEffect(()=>{void refresh()},[refresh])

  const mine=(payload:{projectId?:string})=>payload?.projectId===projectId
  useRealtimeEvents(enabled?token:'',organizationId,{
    'hub:changed':(payload:{projectId:string})=>{if(mine(payload))void loadHub()},
    'release:published':(payload:{projectId:string})=>{if(mine(payload))void loadLive()},
    'release:live':(payload:{projectId:string})=>{if(mine(payload))void loadLive()},
    'release:updated':(payload:{projectId:string})=>{if(mine(payload))void loadLive()},
    'release:deleted':(payload:{projectId:string})=>{if(mine(payload))void loadLive()},
  })
  // Kendi oyun oturumum kapanınca süre hemen görünsün; oyun başlayınca/bitince de tazele.
  useEffect(()=>{
    if(!enabled)return
    return window.p2?.library?.onEvent(event=>{
      if((event.type==='played'||event.type==='running')&&event.projectId===projectId)void loadHub()
    })
  },[enabled,projectId,loadHub])

  const team=useTeam({token:enabled?token:'',organizationId,projectId,me})
  const playing=useMemo(()=>team.playingProject(projectId),[team.playingProject,projectId])

  const posts=useMemo(()=>(hub?.posts||[]).slice(0,POST_LIMIT),[hub])
  const playtime=hub?.playtime||EMPTY_PLAYTIME
  return{project,installed,running,live,updateAvailable:hasUpdate(installed,live),playtime,posts,playing,loading:loading&&!hub,error,refresh}
}
