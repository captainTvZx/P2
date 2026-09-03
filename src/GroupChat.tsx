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
import { anchorFrom, ComposerSuggestions, MessageMenuPopup, ReactionBar, ReactionPicker, useReactionPicker, type MentionSuggestion, type MessageMenu } from './ChannelContent'
import { groupReactionRows } from './reactions'
import type { ChatGroup, GroupMessage } from './groupTypes'

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

function GroupRow({message,own,latest,me,onProfile,onQuote,onEdit,onDelete,onReact}:{message:GroupMessage;own:boolean;latest:boolean;me:string;onProfile:()=>void;onQuote:()=>void;onEdit:()=>void;onDelete:()=>void;onReact:(emoji:string)=>void}){
  const picker=useReactionPicker()
  const[menu,setMenu]=useState<MessageMenu|null>(null)
  if(message.kind==='SYSTEM')return <p className="group-system">{message.content}</p>
  const reactions=groupReactionRows(message.reactions,me)
  return <article className={latest?'message slack-message direct-message latest':'message slack-message direct-message'}
    onContextMenu={event=>{event.preventDefault();setMenu({x:event.clientX,y:event.clientY,up:event.clientY})}}>
    <span className={`message-avatar clickable ${avatarHue(message.author.id)}`} role="button" tabIndex={0} title={`${message.author.displayName} profilini aç`} onClick={onProfile} onKeyDown={event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();onProfile()}}}>{message.author.displayName[0].toUpperCase()}</span>
    <div className="slack-message-content"><div className="message-body">
      {message.quotedMessage&&<div className="discord-quote"><b>{message.quotedMessage.author.displayName}</b><span>{message.quotedMessage.content}</span></div>}
      <header><b className="author-name" title={`${message.author.displayName} profilini aç`} onClick={onProfile}>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleString('tr-TR')}{message.editedAt&&' · düzenlendi'}</small></header>
      <MessageRichText content={message.content}/>
      <ReactionBar reactions={reactions} onToggle={onReact} onAddAt={box=>picker.openAt(box)}/>
    </div></div>
    <div className="message-actions">
      <button type="button" title="Tepki ver" aria-label="Tepki ver" onClick={event=>picker.openAt(event.currentTarget.getBoundingClientRect())}>☺</button>
      <button type="button" title="Mesajı alıntıla" aria-label="Mesajı alıntıla" onClick={onQuote}>❝</button>
      <button type="button" title="Diğer işlemler" aria-label="Diğer işlemler" onClick={event=>setMenu(anchorFrom(event.currentTarget.getBoundingClientRect(),true))}>⋯</button>
    </div>
    {picker.menu&&<ReactionPicker menu={picker.menu} onClose={picker.close} onSelect={emoji=>{onReact(emoji);picker.close()}}/>}
    {menu&&<MessageMenuPopup menu={menu} onClose={()=>setMenu(null)} className="message-context-menu">
      <button onClick={()=>{picker.openMenu(menu);setMenu(null)}}><span>☺</span>Tepki ver</button>
      <button onClick={()=>{onQuote();setMenu(null)}}><span>❝</span>Alıntıla</button>
      {own&&<><hr/>
        <button onClick={()=>{onEdit();setMenu(null)}}><span>✎</span>Düzenle</button>
        <button className="danger" onClick={()=>{onDelete();setMenu(null)}}><span>🗑</span>Sil</button>
      </>}
    </MessageMenuPopup>}
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
  const[quoted,setQuoted]=useState<GroupMessage|null>(null),[editing,setEditing]=useState<GroupMessage|null>(null)
  const[suggestionsClosed,setSuggestionsClosed]=useState(false)
  const[suggestionIndex,setSuggestionIndex]=useState(0)
  const[split,setSplit]=useState(75)
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
          <button type="button" className={adding?'tool active':'tool'} title="Kişi ekle" aria-label="Kişi ekle" onClick={()=>setAdding(value=>!value)}><CallIcon name="personAdd"/></button>
          {adding&&<><div className="group-add-layer" onMouseDown={()=>setAdding(false)}/><div className="group-add-menu">
            <p>Gruba ekle</p>
            {missing.map(person=><button type="button" key={person.id} onClick={()=>{onAddMembers([person.id]);setAdding(false)}}>
              <span className={`person-avatar ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>{person.displayName}
            </button>)}
            {!missing.length&&<small>Eklenecek başka kimse yok.</small>}
          </div></>}
        </span>
        <button type="button" className={inGroupCall?'tool call-active':'tool'} title={inGroupCall?'Grup aramasındasın':'Grubu ara'} aria-label="Grup araması" onClick={()=>void call.joinGroup(group.id)} disabled={inGroupCall}><ComposerIcon name="call"/></button>
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
            onReact={emoji=>void react(message.id,emoji)}/>)}
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
            <button type="button" title="Birini etiketle" onClick={()=>input.current?.insert(`@${group.members.find(member=>member.id!==auth.user.id)?.username||''} `)}><ComposerIcon name="mention"/></button>
          </span><button className="composer-send" type="submit" disabled={sending||!draft.trim()}><ComposerIcon name="send"/></button></div>
        </form>
      </div>
    </div>
  </section>
}
