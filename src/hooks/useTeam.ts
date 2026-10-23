import { useCallback, useEffect, useMemo, useState } from 'react'
import { request } from '../api/client'
import { activityText } from '../activity'
import { emitRealtime, useRealtimeEvents } from '../realtime'

// Ekip listesi: hem ana pencerede (Workspace zaten people/activities tutar; buraya verilir)
// hem overlay penceresinde (kendisi çeker, aynalanan presence olaylarıyla günceller) kullanılır.
export type TeamPerson={id:string;displayName:string;username?:string;avatarUrl?:string|null;status:string;role?:string;title?:string|null}
export type TeamActivity={kind:string;projectId?:string;projectName:string;detail?:string;percent?:number;since?:number}|null
export type TeamRow={person:TeamPerson;activity:TeamActivity;text:string;rank:number;withMe:boolean;playingHere:boolean}

export type UseTeamInput={
  token:string
  organizationId:string
  projectId:string|null
  me:string
  people?:TeamPerson[]                          // verilirse çekilmez (ana pencere)
  activities?:Record<string,TeamActivity>       // verilirse dinlenmez (ana pencere)
  withMe?:Iterable<string>                      // benimle aynı sesli odada olanlar
}

// Sıralama: sesli odada benimle → bu projeyi oynuyor → başka oyun → ONLINE → AWAY/BUSY → OFFLINE
export function teamRank(row:{status:string;activity:TeamActivity;withMe:boolean;projectId:string|null}){
  if(row.withMe)return 0
  if(row.activity?.kind==='playing'&&row.projectId&&row.activity.projectId===row.projectId)return 1
  if(row.activity?.kind==='playing')return 2
  if(row.status==='ONLINE')return 3
  if(row.status==='AWAY'||row.status==='BUSY')return 4
  return 5
}
export function sortTeam(people:TeamPerson[],activities:Record<string,TeamActivity>,projectId:string|null,withMe:Set<string>):TeamRow[]{
  return people.map(person=>{
    const activity=activities[person.id]||null
    const isWithMe=withMe.has(person.id)
    const rank=teamRank({status:person.status,activity,withMe:isWithMe,projectId})
    return{person,activity,text:activityText(activity),rank,withMe:isWithMe,playingHere:rank===1||(isWithMe&&activity?.kind==='playing'&&activity.projectId===projectId)}
  }).sort((a,b)=>a.rank-b.rank||a.person.displayName.localeCompare(b.person.displayName,'tr'))
}

export function useTeam(input:UseTeamInput){
  const{token,organizationId,projectId,me}=input
  const external=Boolean(input.people)
  const[fetched,setFetched]=useState<TeamPerson[]>([])
  const[liveActivities,setLiveActivities]=useState<Record<string,TeamActivity>>({})
  const refresh=useCallback(async()=>{
    if(external||!token||!organizationId||!projectId)return
    try{
      const items=await request<TeamPerson[]>(`/organizations/${organizationId}/members?projectId=${encodeURIComponent(projectId)}`,token)
      setFetched(items.filter(item=>item.id!==me))
    }catch{}
  },[external,token,organizationId,projectId,me])
  useEffect(()=>{void refresh()},[refresh])
  // Sunucudan o anki etkinlik + sesli oda anlık görüntüsünü yeniden ister (overlay görünür olunca).
  const requestSnapshot=useCallback(()=>{if(organizationId)emitRealtime('presence:snapshot',{organizationId})},[organizationId])
  useRealtimeEvents(external?'':token,organizationId,{
    'presence:update':(payload:{userId:string;status:string;activity?:TeamActivity})=>{
      setFetched(items=>items.map(item=>item.id===payload.userId?{...item,status:payload.status}:item))
      setLiveActivities(current=>({...current,[payload.userId]:payload.activity??null}))
    },
    'presence:activity':(payload:{userId:string;activity:TeamActivity})=>setLiveActivities(current=>({...current,[payload.userId]:payload.activity??null})),
    'organization:member:joined':()=>{void refresh()},
    'organization:member:removed':(payload:{userId:string})=>setFetched(items=>items.filter(item=>item.id!==payload.userId)),
  })
  const people=input.people||fetched
  const activities=input.activities||liveActivities
  const withMeKey=input.withMe?[...input.withMe].sort().join(','):''
  const sorted=useMemo(()=>sortTeam(people,activities,projectId,new Set(withMeKey?withMeKey.split(','):[])),[people,activities,projectId,withMeKey])
  const playingProject=useCallback((id:string)=>people.filter(person=>activities[person.id]?.kind==='playing'&&activities[person.id]?.projectId===id),[people,activities])
  return{people,activities,sorted,playingProject,refresh,requestSnapshot}
}
