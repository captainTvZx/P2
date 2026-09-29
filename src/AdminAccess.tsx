import { useCallback, useEffect, useMemo, useState } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import ChannelIcon from './ChannelIcons'
import { avatarHue, userInitials } from './initials'

type Auth={accessToken:string}
type AccessMember={id:string;username:string;displayName:string;avatarUrl:string|null;owner:boolean}
type AccessChannel={id:string;name:string;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';category:string;visibleUserIds:string[]}
type AccessProject={id:string;name:string;ownerId:string;memberIds:string[];channels:AccessChannel[]}
type AccessData={members:AccessMember[];projects:AccessProject[]}

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await p2Fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

export default function AdminAccess({auth,organizationId}:{auth:Auth;organizationId:string}){
  const[data,setData]=useState<AccessData|null>(null)
  const[selectedId,setSelectedId]=useState('')
  const[busyKey,setBusyKey]=useState('')
  const[error,setError]=useState('')
  const base=`/organizations/${organizationId}/admin/access`
  const load=useCallback(async()=>{
    setError('')
    try{
      const next=await api<AccessData>(base,auth.accessToken)
      setData(next)
      setSelectedId(current=>next.members.some(member=>member.id===current)?current:(next.members.find(member=>!member.owner)?.id||next.members[0]?.id||''))
    }catch(problem:any){setError(problem?.message||'Erişim bilgileri yüklenemedi.')}
  },[auth.accessToken,base])
  useEffect(()=>{void load()},[load])
  const selected=useMemo(()=>data?.members.find(member=>member.id===selectedId)||null,[data,selectedId])

  async function toggleProject(project:AccessProject,visible:boolean){
    if(!selected)return
    const key=`project:${project.id}`;setBusyKey(key);setError('')
    try{
      await api(`${base}/projects/${project.id}/members/${selected.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({visible})})
      await load()
    }catch(problem:any){setError(problem?.message||'Proje erişimi değiştirilemedi.')}
    finally{setBusyKey('')}
  }
  async function toggleChannel(channel:AccessChannel,visible:boolean){
    if(!selected)return
    const key=`channel:${channel.id}`;setBusyKey(key);setError('')
    try{
      await api(`${base}/channels/${channel.id}/members/${selected.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({visible})})
      await load()
    }catch(problem:any){setError(problem?.message||'Kanal erişimi değiştirilemedi.')}
    finally{setBusyKey('')}
  }

  if(!data)return <p className="settings-hint">Erişim listesi yükleniyor…</p>
  return <div className="admin-access">
    {error&&<p className="settings-warn">{error}</p>}
    <section className="settings-block admin-access-people">
      <h4>Kişi seç</h4>
      <p className="settings-hint">Bir kişinin görebileceği projeleri ve o projelerdeki kanalları belirle.</p>
      <div className="admin-access-member-list">
        {data.members.map(member=><button type="button" key={member.id} className={member.id===selectedId?'active':undefined} onClick={()=>setSelectedId(member.id)}>
          <i className={`admin-avatar small ${avatarHue(member.id)}`}>{member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}</i>
          <span><b>{member.displayName}</b><small>@{member.username}{member.owner?' · Kuruluş sahibi':''}</small></span>
        </button>)}
      </div>
    </section>
    <section className="settings-block admin-access-projects">
      <h4>{selected?.displayName||'Kişi'} için görünür içerikler</h4>
      {selected?.owner&&<p className="settings-hint">Kuruluş sahibi güvenlik gereği bütün projeleri ve kanalları görür.</p>}
      <div className="admin-access-project-list">
        {data.projects.map(project=>{
          const projectVisible=Boolean(selected&&(selected.owner||project.ownerId===selected.id||project.memberIds.includes(selected.id)))
          const protectedAccess=Boolean(selected&&(selected.owner||project.ownerId===selected.id))
          return <article key={project.id} className={projectVisible?'enabled':''}>
            <header>
              <span><b>{project.name}</b><small>{project.channels.length} kanal</small></span>
              <label className="admin-access-switch">
                <input type="checkbox" checked={projectVisible} disabled={protectedAccess||Boolean(busyKey)} onChange={event=>void toggleProject(project,event.target.checked)}/>
                <i/><em>{projectVisible?'Projeyi görebilir':'Proje gizli'}</em>
              </label>
            </header>
            <div className="admin-access-channels">
              {project.channels.map(channel=>{
                const visible=Boolean(selected&&(selected.owner||channel.visibleUserIds.includes(selected.id)))
                return <label key={channel.id} className={!projectVisible?'disabled':undefined}>
                  <span className="admin-channel-icon"><ChannelIcon type={channel.type}/></span>
                  <span><b>{channel.name}</b><small>{channel.category}</small></span>
                  <input type="checkbox" checked={projectVisible&&visible} disabled={!projectVisible||Boolean(selected?.owner)||Boolean(busyKey)} onChange={event=>void toggleChannel(channel,event.target.checked)}/>
                </label>
              })}
              {!project.channels.length&&<small className="settings-hint">Bu projede kanal yok.</small>}
            </div>
          </article>
        })}
      </div>
    </section>
  </div>
}
