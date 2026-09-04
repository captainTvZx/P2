import { FormEvent, lazy, Suspense, useEffect, useRef, useState } from 'react'
import { API_BASE } from './apiBase'
import { CallIcon } from './CallIcons'
import { useCall } from './CallProvider'
import { ComposerIcon } from './ComposerIcons'
import EmojiSuggestions, { applyEmojiSuggestion, useEmojiSuggestions } from './EmojiSuggestions'
import { avatarHue, userInitials } from './initials'
import MessageRichText from './MessageRichText'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import StreamStage, { type StageItem } from './StreamStage'
import { ResizeHandle, useResizable } from './useResizable'
import { ComposerSuggestions, ReplyComposer, type MentionSuggestion } from './ChannelContent'
import { MessageTools, ReactionBar, useMessageTools } from './messageMenu'
import { groupReactionRows } from './reactions'
import { subscribeGroupReactions, subscribeGroupReplies } from './groupBus'
import { useEscape } from './useEscape'
import { useOutsideClose } from './useOutsideClose'

const EmojiPanel=lazy(()=>import('./EmojiPanel'))
import type { ChatGroup, GroupMessage } from './groupTypes'

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

function GroupRow({message,own,latest,me,onProfile,onQuote,onEdit,onDelete,onReact,onThread}:{message:GroupMessage;own:boolean;latest:boolean;me:string;onProfile:()=>void;onQuote:()=>void;onEdit:()=>void;onDelete:()=>void;onReact:(emoji:string)=>void;onThread:()=>void}){
  const tools=useMessageTools()
  if(message.kind==='SYSTEM')return <p className="group-system">{message.content}</p>
  return <article className={latest?'message slack-message direct-message latest':'message slack-message direct-message'}
    onContextMenu={event=>{event.preventDefault();tools.openAtPoint(event.clientX,event.clientY)}}>
    <span className={`message-avatar clickable ${avatarHue(message.author.id)}`} role="button" tabIndex={0} title={`${message.author.displayName} profilini aç`} onClick={onProfile} onKeyDown={event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();onProfile()}}}>{message.author.displayName[0].toUpperCase()}</span>
    <div className="slack-message-content"><div className="message-body">
      {message.quotedMessage&&<div className="discord-quote"><b>{message.quotedMessage.author.displayName}</b><span>{message.quotedMessage.content}</span></div>}
      <header><b className="author-name" title={`${message.author.displayName} profilini aç`} onClick={onProfile}>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleString('tr-TR')}{message.editedAt&&' · düzenlendi'}</small></header>
      <MessageRichText content={message.content}/>
      <ReactionBar reactions={groupReactionRows(message.reactions,me)} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/>
      {Boolean(message._count?.replies)&&<button type="button" className="thread-summary" onClick={onThread}>
        <b>{message._count?.replies} yanıt</b><small>Thread'i görüntüle</small>
      </button>}
    </div></div>
    <MessageTools tools={tools} actions={{own,content:message.content,editedAt:message.editedAt,onReact,onQuote,onEdit,onDelete,onThread}}/>
  </article>
}

// Thread yanıtları da ana mesajlarla aynı araçları kullanır (tepki, alıntı, düzenle, sil).
function ThreadReplyRow({message,own,me,root,onReact,onQuote,onEdit,onDelete}:{message:GroupMessage;own:boolean;me:string;root?:boolean;onReact:(emoji:string)=>void;onQuote:()=>void;onEdit:()=>void;onDelete:()=>void}){
  const tools=useMessageTools()
  return <article className={root?'message slack-message thread-root':'message slack-message'} onContextMenu={event=>{event.preventDefault();tools.openAtPoint(event.clientX,event.clientY)}}>
    <span className={`message-avatar ${avatarHue(message.author.id)}`}>{message.author.displayName[0].toUpperCase()}</span>
    <div className="slack-message-content"><div className="message-body">
      <header><b className="author-name">{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleTimeString('tr-TR',{hour:'2-digit',minute:'2-digit'})}</small>{message.editedAt&&<i className="edited">düzenlendi</i>}</header>
      <MessageRichText content={message.content}/>
      <ReactionBar reactions={groupReactionRows(message.reactions,me)} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/>
    </div></div>
    <MessageTools tools={tools} actions={{own,content:message.content,editedAt:message.editedAt,onReact,onQuote,onEdit,onDelete}}/>
  </article>
}

// Grup sohbeti: bireysel sohbetin aynısı — aynı yazma alanı, aynı yayın bölmesi.
export default function GroupChat({auth,organizationId,group,messages,onSend,onRead,onAddMembers,onRemoveMember,people,onProfile}:{
  auth:{user:{id:string};accessToken:string}
  organizationId:string
  group:ChatGroup
  messages:GroupMessage[]
  onSend:(message:GroupMessage)=>void
  onRead:()=>void
  onAddMembers:(userIds:string[])=>void
  onRemoveMember:(userId:string)=>void
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
  const[adding,setAdding]=useState(false),[emojiOpen,setEmojiOpen]=useState(false)
  const[quoted,setQuoted]=useState<GroupMessage|null>(null),[editing,setEditing]=useState<GroupMessage|null>(null)
  const[thread,setThread]=useState<GroupMessage|null>(null),[replies,setReplies]=useState<GroupMessage[]>([]),[replying,setReplying]=useState(false)
  const[suggestionsClosed,setSuggestionsClosed]=useState(false)
  const[suggestionIndex,setSuggestionIndex]=useState(0)
  const[split,setSplit]=useState(75)
  const threadPanel=useResizable('p2:group-thread-width',360,280,660,-1)
  useEscape(emojiOpen,()=>setEmojiOpen(false))
  useOutsideClose(Boolean(thread),()=>setThread(null),'.thread-summary')
  const emojiSuggestions=useEmojiSuggestions(draft),suggestionItems=suggestionsClosed?[]:emojiSuggestions
  // Etiketleme yalnızca bu gruptaki kişileri önerir.
  const mentionMatch=draft.match(/@([a-zA-Z0-9_.-]*)$/)
  const mentionNeedle=(mentionMatch?.[1]||'').toLocaleLowerCase('tr-TR')
  const mentionItems:MentionSuggestion[]=suggestionsClosed||!mentionMatch?[]:group.members
    .filter(member=>member.username.toLocaleLowerCase('tr-TR').startsWith(mentionNeedle))
    .map(member=>({label:`@${member.username}`,description:member.displayName,value:member.username,avatar:member.displayName[0].toLocaleUpperCase('tr-TR')})).slice(0,8)
  const activeMention=mentionItems.length?Math.min(suggestionIndex,mentionItems.length-1):-1
  function pickMention(item:MentionSuggestion){setDraft(current=>current.replace(/@([a-zA-Z0-9_.-]*)$/,`@${item.value} `));setSuggestionIndex(0)}
  const activeSuggestion=suggestionItems.length?Math.min(suggestionIndex,suggestionItems.length-1):-1

  useEffect(()=>{const node=feed.current;if(node)node.scrollTop=node.scrollHeight},[messages.length,group.id])
  useEffect(()=>{
    if(!group.unread)return
    void api(`/organizations/${organizationId}/groups/${group.id}/read`,auth.accessToken,{method:'POST'}).then(onRead).catch(()=>{})
  },[group.id,group.unread,organizationId,auth.accessToken])

  // Gruptaki yayınlar: izlemeye aldıklarım solda, kendi yayınım da sahnede.
  const sharing=(call.call?.participants||[]).filter(item=>item.userId!==auth.user.id&&(item.screen||item.video))
  const stage:StageItem[]=[]
  for(const item of sharing){
    const remote=call.remote[item.userId]
    if(call.watching.includes(item.userId)&&remote)stage.push({userId:item.userId,stream:remote.stream,label:`${item.displayName}${item.screen?' · ekran':''}`})
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
      const content=expandEmojiShortcodes(raw)
      if(editing){
        const updated=await api<GroupMessage>(`/organizations/${organizationId}/groups/${group.id}/messages/${editing.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({content})})
        onSend(updated);setEditing(null)
      }else{
        const message=await api<GroupMessage>(`/organizations/${organizationId}/groups/${group.id}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content,quotedMessageId:quoted?.id})})
        onSend(message)
      }
      setQuoted(null)
      setDraft('')
    }catch(problem:any){setError(problem?.message||'Mesaj gönderilemedi.')}
    finally{setSending(false)}
  }

  async function react(messageId:string,emoji:string){
    try{await api(`/organizations/${organizationId}/groups/${group.id}/messages/${messageId}/reactions`,auth.accessToken,{method:'POST',body:JSON.stringify({emoji})})}catch{}
  }
  async function removeMessage(messageId:string){
    try{await api(`/organizations/${organizationId}/groups/${group.id}/messages/${messageId}`,auth.accessToken,{method:'DELETE'})}catch{}
  }
  // Thread paneli: seçilen mesajın yanıtları.
  useEffect(()=>{
    if(!thread){setReplies([]);return}
    setReplies([])
    void api<GroupMessage[]>(`/organizations/${organizationId}/groups/${group.id}/messages/${thread.id}/replies`,auth.accessToken).then(setReplies).catch(()=>{})
  },[thread?.id,group.id,organizationId,auth.accessToken])
  useEffect(()=>subscribeGroupReactions(group.id,payload=>{
    setReplies(current=>current.map(item=>item.id===payload.messageId?{...item,reactions:payload.rows}:item))
  }),[group.id])
  useEffect(()=>{
    const off=subscribeGroupReplies(group.id,payload=>{
      if(thread?.id===payload.parentId)setReplies(current=>current.some(item=>item.id===payload.message.id)?current:[...current,payload.message])
    })
    return off
  },[group.id,thread?.id])
  async function sendReply(raw:string){
    const content=raw.trim()
    if(!thread||!content||replying)return
    setReplying(true)
    try{
      const message=await api<GroupMessage>(`/organizations/${organizationId}/groups/${group.id}/messages/${thread.id}/replies`,auth.accessToken,{method:'POST',body:JSON.stringify({content})})
      setReplies(current=>current.some(item=>item.id===message.id)?current:[...current,message])
    }catch{}
    finally{setReplying(false)}
  }
  const owner=group.createdById===auth.user.id
  const inGroupCall=call.call?.kind==='group'&&call.call.groupId===group.id
  const missing=people.filter(person=>!group.members.some(member=>member.id===person.id))

  return <section className={splitView?'channel-content direct-content group-content watching':'channel-content direct-content group-content'} style={splitView?{['--stream-split' as any]:`${split}%`}:undefined}>
    <header>
      <span className="group-avatar large">{userInitials(group.name)}</span>
      <div>
        <h2>{group.name}</h2>
        <p>{group.members.length} kişi · {group.members.map(member=>member.displayName).join(', ')}</p>
      </div>
      <nav className="channel-tools">
        {sharing.map(item=><button type="button" key={item.userId} className={call.watching.includes(item.userId)?'tool watch-tool active':'tool watch-tool'} title={`${item.displayName} yayınını izle`} onClick={()=>call.watch(item.userId)}>
          <CallIcon name="screen"/><span>{call.watching.includes(item.userId)?'Yayını kapat':`${item.displayName.split(' ')[0]} yayında`}</span>
        </button>)}
        <span className="group-add-anchor">
          <button type="button" className={adding?'tool active':'tool'} title="Üyeler" aria-label="Üyeler" onClick={()=>setAdding(value=>!value)}><CallIcon name="personAdd"/></button>
          {adding&&<><div className="group-add-layer" onMouseDown={()=>setAdding(false)}/><div className="group-add-menu">
            <p>Üyeler — {group.members.length}</p>
            {group.members.map(member=><span className="group-member-row" key={member.id}>
              <span className={`person-avatar ${avatarHue(member.id)}`}>{userInitials(member.displayName)}</span>
              <b>{member.displayName}</b>
              {member.id===auth.user.id?<small>sen</small>:owner?<button type="button" className="group-kick" title={`${member.displayName} kişisini gruptan çıkar`} onClick={()=>onRemoveMember(member.id)}>×</button>:null}
            </span>)}
            <p>Gruba ekle</p>
            {missing.map(person=><button type="button" key={person.id} className="group-add-row" onClick={()=>{onAddMembers([person.id]);setAdding(false)}}>
              <span className={`person-avatar ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>{person.displayName}
            </button>)}
            {!missing.length&&<small>Eklenecek başka kimse yok.</small>}
          </div></>}
        </span>
        <button type="button" className={inGroupCall?'tool call-active':'tool'} title={inGroupCall?'Grup aramasındasın':group.members.length<2?'Aramak için gruba en az bir kişi daha ekle':'Grubu ara'} aria-label="Grup araması" onClick={()=>void call.joinGroup(group.id)} disabled={inGroupCall||group.members.length<2}><ComposerIcon name="call"/></button>
      </nav>
    </header>


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
          {messages.map((message,index)=><GroupRow key={message.id} message={message} own={message.author.id===auth.user.id} latest={index===messages.length-1} me={auth.user.id}
            onProfile={()=>onProfile(message.author.id)}
            onQuote={()=>{setQuoted(message);setEditing(null)}}
            onEdit={()=>{setEditing(message);setQuoted(null);setDraft(message.content)}}
            onDelete={()=>void removeMessage(message.id)}
            onReact={emoji=>void react(message.id,emoji)}
            onThread={()=>setThread(message)}/>)}
        </div>

        <form ref={form} className="message-composer slack-composer" onSubmit={submit}>
          {error&&<p className="composer-error">{error}</p>}
          {quoted&&!editing&&<div className="composer-reference"><span><b>{quoted.author.displayName}</b> adlı kişiye yanıt veriliyor<small>{quoted.content}</small></span><button type="button" onClick={()=>setQuoted(null)}>×</button></div>}
          {editing&&<div className="composer-reference editing"><span><b>Mesaj düzenleniyor</b><small>{editing.content}</small></span><button type="button" onClick={()=>{setEditing(null);setDraft('')}}>×</button></div>}
          <ComposerSuggestions mentions={mentionItems} commands={[]} onMention={pickMention} onCommand={()=>{}} activeIndex={activeMention} onHover={setSuggestionIndex}/>
          <EmojiSuggestions items={suggestionItems} onPick={item=>setDraft(current=>applyEmojiSuggestion(current,item))} activeIndex={activeSuggestion} onHover={setSuggestionIndex}/>
          <div className="composer-input-row">
            <button type="button" title="Ekle" aria-label="Ekle"><ComposerIcon name="plus"/></button>
            <RichInput ref={input} value={draft} onChange={value=>{setDraft(value.slice(0,4000));setSuggestionsClosed(false);setSuggestionIndex(0)}} onKeyDown={event=>{
              if(mentionItems.length){
                if(event.key==='ArrowDown'){event.preventDefault();setSuggestionIndex(current=>(Math.min(current,mentionItems.length-1)+1)%mentionItems.length);return}
                if(event.key==='ArrowUp'){event.preventDefault();setSuggestionIndex(current=>(Math.min(current,mentionItems.length-1)-1+mentionItems.length)%mentionItems.length);return}
                if(event.key==='Enter'||event.key==='Tab'){event.preventDefault();const item=mentionItems[activeMention];if(item)pickMention(item);return}
                if(event.key==='Escape'){event.preventDefault();event.stopPropagation();setSuggestionsClosed(true);return}
              }
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
            <button type="button" className={emojiOpen?'active':undefined} title="Emoji seç" onClick={()=>setEmojiOpen(open=>!open)}><ComposerIcon name="emoji"/></button>
            <button type="button" title="Birini etiketle" onClick={()=>input.current?.insert(`@${group.members.find(member=>member.id!==auth.user.id)?.username||''} `)}><ComposerIcon name="mention"/></button>
          </span><button className="composer-send" type="submit" disabled={sending||!draft.trim()}><ComposerIcon name="send"/></button></div>
          {emojiOpen&&<div className="emoji-picker-popover"><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel onSelect={emoji=>input.current?.insert(emoji)}/></Suspense></div>}
        </form>
      </div>

      {thread&&<aside className="thread-panel group-thread" style={{width:threadPanel.width}}>
        <ResizeHandle edge="left" onMouseDown={threadPanel.start} label="Thread genişliği"/>
        <header><div><b>Thread</b><small>{group.name}</small></div><button type="button" onClick={()=>setThread(null)} aria-label="Kapat">×</button></header>
        <ThreadReplyRow message={thread} own={thread.author.id===auth.user.id} me={auth.user.id} root
          onReact={emoji=>void react(thread.id,emoji)}
          onQuote={()=>{setQuoted(thread);setEditing(null)}}
          onEdit={()=>{setEditing(thread);setDraft(thread.content)}}
          onDelete={()=>{void removeMessage(thread.id);setThread(null)}}/>
        <div className="thread-divider"><span>{replies.length?`${replies.length} yanıt`:'Henüz yanıt yok'}</span></div>
        <div className="forum-replies">
          {replies.map(item=><ThreadReplyRow key={item.id} message={item} own={item.author.id===auth.user.id} me={auth.user.id}
            onReact={emoji=>void react(item.id,emoji)}
            onQuote={()=>{setQuoted(item);setEditing(null)}}
            onEdit={()=>{setEditing(item);setDraft(item.content)}}
            onDelete={()=>void removeMessage(item.id)}/>)}
        </div>
        <ReplyComposer sending={replying} placeholder="Thread'e yanıt yaz…" onSubmit={content=>sendReply(content)} editing={null} onCancelEdit={()=>{}} quoted={null} onCancelQuote={()=>{}} mentions={group.members}/>
      </aside>}
    </div>
  </section>
}
