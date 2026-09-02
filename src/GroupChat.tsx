import { FormEvent, useEffect, useRef, useState } from 'react'
import { API_BASE } from './apiBase'
import { ComposerIcon } from './ComposerIcons'
import { avatarHue, userInitials } from './initials'
import MessageRichText from './MessageRichText'
import { useCall } from './CallProvider'
import { CallIcon } from './CallIcons'
import type { ChatGroup, GroupMessage } from './groupTypes'

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

// Grup sohbeti: kişi listesindeki bireysel sohbetin çok kişili hali.
export default function GroupChat({auth,organizationId,group,messages,onSend,onRead,onAddMembers,people}:{
  auth:{user:{id:string};accessToken:string}
  organizationId:string
  group:ChatGroup
  messages:GroupMessage[]
  onSend:(message:GroupMessage)=>void
  onRead:()=>void
  onAddMembers:(userIds:string[])=>void
  people:{id:string;displayName:string;username:string}[]
}){
  const call=useCall()
  const feed=useRef<HTMLDivElement>(null)
  const[draft,setDraft]=useState('')
  const[sending,setSending]=useState(false)
  const[error,setError]=useState('')
  const[adding,setAdding]=useState(false)

  useEffect(()=>{const node=feed.current;if(node)node.scrollTop=node.scrollHeight},[messages.length,group.id])
  useEffect(()=>{
    if(!group.unread)return
    void api(`/organizations/${organizationId}/groups/${group.id}/read`,auth.accessToken,{method:'POST'}).then(onRead).catch(()=>{})
  },[group.id,group.unread,organizationId,auth.accessToken])

  async function submit(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    const content=draft.trim()
    if(!content||sending)return
    setSending(true);setError('')
    try{
      const message=await api<GroupMessage>(`/organizations/${organizationId}/groups/${group.id}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content})})
      onSend(message)
      setDraft('')
    }catch(problem:any){setError(problem?.message||'Mesaj gönderilemedi.')}
    finally{setSending(false)}
  }

  const missing=people.filter(person=>!group.members.some(member=>member.id===person.id))

  return <section className="channel-content group-content">
    <header>
      <span className="group-avatar large">{userInitials(group.name)}</span>
      <div>
        <h2>{group.name}</h2>
        <p>{group.members.length} kişi · {group.members.map(member=>member.displayName).join(', ')}</p>
      </div>
      <nav className="channel-tools">
        <button type="button" className="tool" title="Kişi ekle" aria-label="Kişi ekle" onClick={()=>setAdding(value=>!value)}><CallIcon name="personAdd"/></button>
        <button type="button" className="tool" title="Gruptakileri ara" aria-label="Ara" onClick={()=>group.members.filter(member=>member.id!==auth.user.id).forEach(member=>call.invite(member.id))} disabled={!call.call}><ComposerIcon name="call"/></button>
      </nav>
    </header>

    {adding&&<div className="group-add-bar">
      {missing.length?missing.map(person=><button type="button" key={person.id} onClick={()=>{onAddMembers([person.id]);setAdding(false)}}>
        <span className={`person-avatar ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>{person.displayName}
      </button>):<small>Eklenecek başka kimse yok.</small>}
    </div>}

    <div className="chat-layout">
      <div className="channel-chat-main">
        <div className="message-feed" ref={feed}>
          {!messages.length&&<div className="channel-welcome">
            <span className="group-avatar huge">{userInitials(group.name)}</span>
            <h1>{group.name}</h1>
            <p>Bu grubun başlangıcı. {group.members.length} kişi burada yazışabilir.</p>
          </div>}
          {messages.map(message=><article className="message slack-message" key={message.id}>
            <span className={`person-avatar ${avatarHue(message.author.id)}`}>{userInitials(message.author.displayName)}</span>
            <div className="slack-message-content">
              <p className="message-meta"><b>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleTimeString('tr-TR',{hour:'2-digit',minute:'2-digit'})}</small></p>
              <div className="message-body"><MessageRichText content={message.content}/></div>
            </div>
          </article>)}
        </div>
        <form className="message-composer slack-composer" onSubmit={submit}>
          {error&&<p className="composer-error">{error}</p>}
          <div className="composer-input-row">
            <input value={draft} onChange={event=>setDraft(event.target.value.slice(0,4000))} placeholder={`${group.name} grubuna mesaj gönder`} disabled={sending}/>
            <button className="composer-send" type="submit" disabled={sending||!draft.trim()}><ComposerIcon name="send"/></button>
          </div>
        </form>
      </div>
    </div>
  </section>
}
