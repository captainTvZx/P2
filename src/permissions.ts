import { useCallback, useEffect, useState } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { useRealtimeEvents } from './realtime'

// Sunucudaki katalogla aynı; arayüz bunu yalnızca ne göstereceğine karar vermek
// için kullanır, asıl karar her zaman sunucuda verilir.
export type PermissionId=string
export type PermissionInfo={id:PermissionId;group:string;label:string;hint:string;channel:boolean}
export type MyRole={id:string;handle:string;name:string;color:string|null;everyone:boolean;permissions:PermissionId[]}
export type MyPermissions={
  owner:boolean
  permissions:PermissionId[]
  roles:MyRole[]
  allow:PermissionId[]
  deny:PermissionId[]
  channels:Record<string,PermissionId[]>
}

const empty:MyPermissions={owner:false,permissions:[],roles:[],allow:[],deny:[],channels:{}}

// Oturumdaki kişinin etkin yetkileri. Rol, kullanıcı grubu, kişisel yetki ya da
// kanal izni değiştiğinde sunucu haber verir ve liste anında tazelenir; kişinin
// uygulamayı yeniden başlatmasına gerek kalmaz.
export function useMyPermissions(token:string,organizationId:string){
  const[state,setState]=useState<MyPermissions>(empty)
  const[version,setVersion]=useState(0)
  const load=useCallback(()=>{
    if(!token||!organizationId)return
    p2Fetch(`${API_BASE}/organizations/${organizationId}/permissions/me`,{headers:{Authorization:`Bearer ${token}`}})
      .then(response=>response.ok?response.json():empty)
      .then((data:MyPermissions)=>setState({...empty,...data}))
      .catch(()=>{})
  },[token,organizationId])
  useEffect(load,[load])
  const refresh=useCallback(()=>{load();setVersion(value=>value+1)},[load])
  useRealtimeEvents(token,organizationId,{
    'permissions:changed':refresh,
    'organization:role':refresh,
    'usergroup:changed':refresh,
    'usergroup:removed':refresh,
    'channel:permissions':refresh,
  })
  return {
    ...state,
    version,   // kanal listesi gibi yetkiye bağlı verileri yeniden çekmek için
    can:(permission:PermissionId)=>state.permissions.includes(permission),
    canIn:(channelId:string|null|undefined,permission:PermissionId)=>{
      if(!channelId)return state.permissions.includes(permission)
      const list=state.channels[channelId]
      return list?list.includes(permission):state.permissions.includes(permission)
    },
  }
}
export type PermissionApi=ReturnType<typeof useMyPermissions>
