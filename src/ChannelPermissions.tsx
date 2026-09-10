import { useEffect, useState } from 'react'
import { API_BASE } from './apiBase'
import { avatarHue, userInitials } from './initials'
import { useEscape } from './useEscape'
import { PermissionRow, type PermState } from './PermissionRow'

type Target={type:'ROLE'|'GROUP'|'USER';id:string;label:string;hint:string;color?:string|null}
type Override={targetType:string;targetId:string;allow:string[];deny:string[]}
type Catalog={id:string;group:string;label:string;hint:string}
type Payload={
  catalog:Catalog[]
  overrides:Override[]
  roles:{id:string;handle:string;name:string;color:string|null;everyone:boolean}[]
  groups:{id:string;handle:string;name:string;color:string|null;memberCount:number}[]
  members:{id:string;username:string;displayName:string;avatarUrl:string|null;owner:boolean}[]
  mine:string[]
  canEdit:boolean
}

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

// Kanala özel izinler. Kurallar sırayla uygulanır: rol → kullanıcı grubu → kişi;
// en dar kural son sözü söyler, tıpkı kanalın kendi kapısındaki gibi.
export default function ChannelPermissionsDialog({auth,organization,project,channel,onClose}:{
  auth:{user:{id:string};accessToken:string}
  organization:{id:string}
  project:{id:string}
  channel:{id:string;name:string}
  onClose:()=>void
}){
  const[data,setData]=useState<Payload|null>(null)
  const[target,setTarget]=useState<Target|null>(null)
  const[tab,setTab]=useState<'ROLE'|'GROUP'|'USER'>('ROLE')
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  const base=`/organizations/${organization.id}/projects/${project.id}/channels/${channel.id}/permissions`
  useEscape(true,onClose)

  useEffect(()=>{api<Payload>(base,auth.accessToken).then(payload=>{
    setData(payload)
    const first=payload.roles[0]
    if(first)setTarget({type:'ROLE',id:first.id,label:first.name,hint:first.everyone?'Kuruluştaki herkes':`@${first.handle}`,color:first.color})
  }).catch(problem=>setError(problem?.message||'Kanal izinleri yüklenemedi.'))},[base,auth.accessToken])

  const current=target?data?.overrides.find(row=>row.targetType===target.type&&row.targetId===target.id):undefined
  const stateOf=(permission:string)=>current?.allow.includes(permission)?'allow':current?.deny.includes(permission)?'deny':'inherit'

  async function set(permission:string,next:PermState){
    if(!data?.canEdit||busy||!target)return
    const allow=(current?.allow||[]).filter(item=>item!==permission)
    const deny=(current?.deny||[]).filter(item=>item!==permission)
    if(next==='allow')allow.push(permission)
    else if(next==='deny')deny.push(permission)
    setBusy(true);setError('')
    try{
      const saved=await api<Override&{removed?:boolean}>(`${base}/${target.type}/${target.id}`,auth.accessToken,{method:'PUT',body:JSON.stringify({allow,deny})})
      setData(previous=>previous&&{
        ...previous,
        overrides:[...previous.overrides.filter(row=>!(row.targetType===target.type&&row.targetId===target.id)),
          ...(saved.removed?[]:[{targetType:target.type,targetId:target.id,allow:saved.allow,deny:saved.deny}])],
      })
    }catch(problem:any){setError(problem?.message||'İzin kaydedilemedi.')}
    finally{setBusy(false)}
  }

  async function clearTarget(){
    if(!data?.canEdit||busy||!target)return
    setBusy(true);setError('')
    try{
      await api(`${base}/${target.type}/${target.id}`,auth.accessToken,{method:'DELETE'})
      setData(previous=>previous&&{...previous,overrides:previous.overrides.filter(row=>!(row.targetType===target.type&&row.targetId===target.id))})
    }catch(problem:any){setError(problem?.message||'İzin sıfırlanamadı.')}
    finally{setBusy(false)}
  }

  const targets:Target[]=tab==='ROLE'
    ?(data?.roles||[]).map(role=>({type:'ROLE' as const,id:role.id,label:role.name,hint:role.everyone?'Kuruluştaki herkes':`@${role.handle}`,color:role.color}))
    :tab==='GROUP'
    ?(data?.groups||[]).map(group=>({type:'GROUP' as const,id:group.id,label:group.name,hint:`@${group.handle} · ${group.memberCount} kişi`,color:group.color}))
    :(data?.members||[]).map(member=>({type:'USER' as const,id:member.id,label:member.displayName,hint:member.owner?'Kuruluş sahibi':`@${member.username}`}))
  const marked=(type:string,id:string)=>data?.overrides.some(row=>row.targetType===type&&row.targetId===id)
  const grouped=[...new Set((data?.catalog||[]).map(item=>item.group))]

  return <div className="settings-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="settings-modal channel-perm-modal" role="dialog" aria-label="Kanal izinleri">
      <nav>
        <header><b>#{channel.name}</b><small>Kanal izinleri</small></header>
        <div className="perm-tabs channel-perm-tabs">
          <button type="button" className={tab==='ROLE'?'active':undefined} onClick={()=>{setTab('ROLE');const first=(data?.roles||[])[0];if(first)setTarget({type:'ROLE',id:first.id,label:first.name,hint:first.everyone?'Kuruluştaki herkes':`@${first.handle}`,color:first.color})}}>Roller</button>
          <button type="button" className={tab==='GROUP'?'active':undefined} onClick={()=>{setTab('GROUP');const first=(data?.groups||[])[0];if(first)setTarget({type:'GROUP',id:first.id,label:first.name,hint:`@${first.handle}`,color:first.color})}}>Gruplar</button>
          <button type="button" className={tab==='USER'?'active':undefined} onClick={()=>{setTab('USER');const first=(data?.members||[])[0];if(first)setTarget({type:'USER',id:first.id,label:first.displayName,hint:`@${first.username}`})}}>Kişiler</button>
        </div>
        <div className="channel-perm-targets">
          {targets.map(item=><button type="button" key={`${item.type}-${item.id}`} className={target?.type===item.type&&target?.id===item.id?'active':undefined} onClick={()=>setTarget(item)}>
            {item.type==='USER'
              ?<span className={`admin-avatar small ${avatarHue(item.id)}`}>{userInitials(item.label)}</span>
              :<span className="channel-perm-glyph" style={item.color?{background:`${item.color}22`,color:item.color}:undefined}>@</span>}
            <span className="admin-member-name"><b>{item.label}</b><small>{item.hint}</small></span>
            {marked(item.type,item.id)&&<i className="channel-perm-dot" title="Bu kanalda özel kuralı var"/>}
          </button>)}
          {!targets.length&&<p className="settings-hint">Liste boş.</p>}
        </div>
      </nav>
      <div className="settings-body">
        <header>
          <h3>{target?.label||'Kanal izinleri'}<small>{target?.hint}</small></h3>
          <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button>
        </header>
        <div className="settings-scroll">
          {error&&<p className="settings-warn">{error}</p>}
          {!data&&!error&&<p className="settings-hint">Yükleniyor…</p>}
          {data&&<>
            <p className="settings-hint">
              Her satırda üç seçenek var: <b>✕ engelle</b> · <b>／ varsayılan</b> · <b>✓ izin ver</b>.
              Kurallar Herkes rolü → diğer roller → gruplar → kişi sırasıyla işlenir; kişiye özel kural her zaman kazanır.
              {!data.canEdit&&<> Bu kanalın izinlerini değiştirme yetkin yok.</>}
            </p>
            <p className="settings-hint">Kuruluş sahibinin yetkileri kanal bazında da kısıtlanamaz.</p>
            {grouped.map(name=><section className="settings-block" key={name}>
              <h4>{name}</h4>
              <div className="perm-list channel-perm-list">
                {data.catalog.filter(item=>item.group===name).map(item=>{
                  const state=stateOf(item.id)
                  return <PermissionRow key={item.id} label={item.label} state={state} disabled={!data.canEdit||busy||!target}
                    hint={state==='allow'?'Bu kanalda izin verildi':state==='deny'?'Bu kanalda engellendi':'Kuruluş genelindeki yetki geçerli'}
                    onChange={next=>void set(item.id,next)}/>
                })}
              </div>
            </section>)}
            {current&&data.canEdit&&<button type="button" className="settings-button" disabled={busy} onClick={()=>void clearTarget()}>Bu hedefin kanal kurallarını sıfırla</button>}
          </>}
        </div>
      </div>
    </section>
  </div>
}
