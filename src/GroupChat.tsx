import { FormEvent, useEffect, useRef, useState } from 'react'
import { API_BASE } from './apiBase'
import { CallIcon } from './CallIcons'
import { useCall } from './CallProvider'
import { ComposerIcon } from './ComposerIcons'
import EmojiSuggestions, { applyEmojiSuggestion, useEmojiSuggestions } from './EmojiSuggestions'
import { avatarHue, userInitials } from './initials'
import MessageRichText from './MessageRichText'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import StreamStage, { type StageItem } from './StreamStage'
import type { ChatGroup, GroupMessage } from './groupTypes'

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

function GroupRow({message,latest,onProfile}:{message:GroupMessage;latest:boolean;onProfile:()=>void}){
  if(message.kind==='SYSTEM')return <p className="group-system">{message.content}</p>
  return <article className={latest?'message slack-message direct-message latest':'message slack-message direct-message'}>
    <span className={`message-avatar clickable ${avatarHue(message.author.id)}`} role="button" tabIndex={0} title={`${message.author.displayName} profilini aç`} onClick={onProfile} onKeyDown={event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();onProfile()}}}>{message.author.displayName[0].toUpperCase()}</span>
    <div className="slack-message-content"><div className="message-body">
      <p className="message-meta"><b>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleTimeString('tr-TR',{hour:'2-digit',minute:'2-digit'})}</small>{message.editedAt&&<i className="edited">düzenlendi</i>}</p>
      <MessageRichText content={message.content}/>
    </div></div>
  </article>
}

// Grup sohbeti: bireysel sohbetin aynısı — aynı yazma alanı, aynı yayın bölmesi.
export default function GroupChat({auth,organizationId,group,messages,onSend,onRead,onAddMembers,people,onProfile}:{
  auth:{user:{id:string};accessToken:string}
  organizationId:string
  group:ChatGroup
  messages:GroupMessage[]
  onSend:(message:GroupMessage)=>void
  onRead:()=>void
  onAddMembers:(userIds:string[])=>void
  people:{id:string;displayName:string;username:string}[]
  onProfile:(userId:string)=>void
}){
  const call=useCall()
  const feed=useRef<HTMLDivElement>(null)
  const form=useRef<HTMLFormElement>(null)
  const input=useRef<RichHandle>(null)
  const[draft,setDraft]=useState('')
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const[sending,setSending]=useState(false)
  const[error,setError]=useState('')
  const[adding,setAdding]=useState(false)
  const[suggestionsClosed,setSuggestionsClosed]=useState(false)
  const[suggestionIndex,setSuggestionIndex]=useState(0)
  const[split,setSplit]=useState(75)
  const emojiSuggestions=useEmojiSuggestions(draft),suggestionItems=suggestionsClosed?[]:emojiSuggestions
  const activeSuggestion=suggestionItems.length?Math.min(suggestionIndex,suggestionItems.length-1):-1

  useEffect(()=>{const node=feed.current;if(node)node.scrollTop=node.scrollHeight},[messages.length,group.id])
  useEffect(()=>{
    if(!group.unread)return
    void api(`/organizations/${organizationId}/groups/${group.id}/read`,auth.accessToken,{method:'POST'}).then(onRead).catch(()=>{})
  },[group.id,group.unread,organizationId,auth.accessToken])

  // Gruptaki yayınlar: izlemeye aldıklarım solda, kendi yayınım da sahnede.
  const sharing=group.members.filter(member=>member.id!==auth.user.id&&call.call?.participants.some(item=>item.userId===member.id&&(item.screen||item.video)))
  const stage:StageItem[]=[]
  for(const member of sharing){
    const remote=call.remote[member.id]
    if(call.watching.includes(member.id)&&remote)stage.push({userId:member.id,stream:remote.stream,label:`${member.displayName} · ekran`})
  }
  if(call.localVideo&&(call.screenOn||call.cameraOn))stage.push({userId:auth.user.id,stream:call.localVideo,label:call.screenOn?'Sen · ekran':'Sen',self:true})
  const splitView=stage.length>0

  function dragSplit(event:React.PointerEvent<HTMLDivElement>){
    event.preventDefault()
    const shell=event.currentTarget.closest('.group-content') as HTMLElement|null
    if(!shell)return
    const move=(pointer:PointerEvent)=>{
      const box=shell.getBoundingClientRect()
      setSplit(Math.min(78,Math.max(35,((pointer.clientX-box.left)/box.width)*100)))
    }
    const up=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',up)}
    window.addEventListener('pointermove',move);window.addEventListener('pointerup',up)
  }

  async function submit(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    const raw=draft.trim()
    if(!raw||sending)return
    setSending(true);setError('')
    try{
      const{expandEmojiShortcodes}=await import('./emojiShortcodes')
      const message=await api<GroupMessage>(`/organizations/${organizationId}/groups/${group.id}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content:expandEmojiShortcodes(raw)})})
      onSend(message)
      setDraft('')
    }catch(problem:any){setError(problem?.message||'Mesaj gönderilemedi.')}
    finally{setSending(false)}
  }

  const missing=people.filter(person=>!group.members.some(member=>member.id===person.id))

  return <section className={splitView?'channel-content direct-content group-content watching':'channel-content direct-content group-content'} style={splitView?{['--stream-split' as any]:`${split}%`}:undefined}>
    <header>
      <span className="group-avatar large">{userInitials(group.name)}</span>
      <div>
        <h2>{group.name}</h2>
        <p>{group.members.length} kişi · {group.members.map(member=>member.displayName).join(', ')}</p>
      </div>
      <nav className="channel-tools">
        {sharing.map(member=><button type="button" key={member.id} className={call.watching.includes(member.id)?'tool watch-tool active':'tool watch-tool'} title={`${member.displayName} yayınını izle`} onClick={()=>call.watch(member.id)}>
          <CallIcon name="screen"/><span>{call.watching.includes(member.id)?'Yayını kapat':`${member.displayName.split(' ')[0]} yayında`}</span>
        </button>)}
        <button type="button" className="tool" title="Kişi ekle" aria-label="Kişi ekle" onClick={()=>setAdding(value=>!value)}><CallIcon name="personAdd"/></button>
        <button type="button" className="tool" title="Gruptakileri aramaya çağır" aria-label="Ara" onClick={()=>group.members.filter(member=>member.id!==auth.user.id).forEach(member=>call.invite(member.id))} disabled={!call.call}><ComposerIcon name="call"/></button>
      </nav>
    </header>

    {adding&&<div className="group-add-bar">
      {missing.length?missing.map(person=><button type="button" key={person.id} onClick={()=>{onAddMembers([person.id]);setAdding(false)}}>
        <span className={`person-avatar ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>{person.displayName}
      </button>):<small>Eklenecek başka kimse yok.</small>}
    </div>}

    {splitView&&<div className="direct-stream-pane">
      <StreamStage items={stage} onClose={userId=>call.watch(userId)}/>
      <div className="direct-stream-divider" onPointerDown={dragSplit} title="Genişliği ayarla"/>
    </div>}

    <div className="chat-layout">
      <div className="channel-chat-main">
        <div className="message-feed" ref={feed}>
          {!messages.length&&<div className="channel-welcome">
            <span className="group-avatar huge">{userInitials(group.name)}</span>
            <h1>{group.name}</h1>
            <p>Bu grubun başlangıcı. {group.members.length} kişi burada yazışabilir.</p>
          </div>}
          {messages.map((message,index)=><GroupRow key={message.id} message={message} latest={index===messages.length-1} onProfile={()=>onProfile(message.author.id)}/>)}
        </div>

        <form ref={form} className="message-composer slack-composer" onSubmit={submit}>
          {error&&<p className="composer-error">{error}</p>}
          <EmojiSuggestions items={suggestionItems} onPick={item=>setDraft(current=>applyEmojiSuggestion(current,item))} activeIndex={activeSuggestion} onHover={setSuggestionIndex}/>
          <div className="composer-input-row">
            <button type="button" title="Ekle" aria-label="Ekle"><ComposerIcon name="plus"/></button>
            <RichInput ref={input} value={draft} onChange={value=>{setDraft(value.slice(0,4000));setSuggestionsClosed(false);setSuggestionIndex(0)}} onKeyDown={event=>{
              if(!suggestionItems.length)return
              if(event.key==='ArrowDown'){event.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionItems.length-1)+1)%suggestionItems.length)}
              else if(event.key==='ArrowUp'){event.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionItems.length-1)-1+suggestionItems.length)%suggestionItems.length)}
              else if(event.key==='Enter'||event.key==='Tab'){event.preventDefault();const item=suggestionItems[activeSuggestion];if(item)setDraft(current=>applyEmojiSuggestion(current,item));setSuggestionIndex(0)}
              else if(event.key==='Escape'){event.preventDefault();event.stopPropagation();setSuggestionsClosed(true)}
            }} onEnter={()=>form.current?.requestSubmit()} onStateChange={setFormat} placeholder={`${group.name} grubuna mesaj gönder`} disabled={sending}/>
          </div>
          <div className="composer-toolbar"><span>
            <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={event=>event.preventDefault()} onClick={()=>input.current?.format('bold')}><ComposerIcon name="bold"/></button>
            <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={event=>event.preventDefault()} onClick={()=>input.current?.format('italic')}><ComposerIcon name="italic"/></button>
            <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={event=>event.preventDefault()} onClick={()=>input.current?.openLink()}><ComposerIcon name="link"/></button><i/>
            <button type="button" title="Birini etiketle" onClick={()=>input.current?.insert(`@${group.members.find(member=>member.id!==auth.user.id)?.username||''} `)}><ComposerIcon name="mention"/></button>
          </span><button className="composer-send" type="submit" disabled={sending||!draft.trim()}><ComposerIcon name="send"/></button></div>
        </form>
      </div>
    </div>
  </section>
}
