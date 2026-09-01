import { FormEvent, lazy, Suspense, useEffect, useRef, useState } from 'react'
import { ComposerIcon } from './ComposerIcons'
import EmojiSuggestions, { applyEmojiSuggestion, useEmojiSuggestions } from './EmojiSuggestions'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import { avatarHue, userInitials } from './initials'
import MessageRichText from './MessageRichText'
import { MentionContext } from './mentions'
import ProfilePanel, { presenceClass, statusLabel, type ProfileMember, type ProfileUser } from './ProfilePanel'
import { useRealtimeEvents } from './realtime'
import { useFreshMessage } from './useFreshMessage'
import { typingText, useTypingBroadcast, useTypists } from './useTyping'

type Auth={user:{id:string;username:string;displayName:string};accessToken:string}
type Organization={id:string}
type Conversation={id:string;user:ProfileMember;unread:number}
type DirectMsg={id:string;content:string;kind?:string;createdAt:string;editedAt?:string|null;quotedMessage?:{id:string;content:string;author:{displayName:string}}|null;author:{id:string;username:string;displayName:string}}

const EmojiPanel=lazy(()=>import('./EmojiPanel'))
const API=window.p2?.apiUrl||'http://127.0.0.1:4000'
async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{const response=await fetch(API+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}});const data=await response.json().catch(()=>({}));if(!response.ok)throw data;return data}
function upsert(items:DirectMsg[],message:DirectMsg){const index=items.findIndex(item=>item.id===message.id);if(index<0)return[...items,message];const next=[...items];next[index]={...next[index],...message};return next}

function callSummary(message:DirectMsg,own:boolean,other:string){
  const seconds=Number(message.content)||0
  const length=`${String(Math.floor(seconds/60)).padStart(2,'0')}:${String(seconds%60).padStart(2,'0')}`
  if(message.kind==='CALL_ENDED')return{missed:false,text:`Sesli arama · ${length}`,tone:'done'}
  if(message.kind==='CALL_MISSED')return{missed:true,text:own?`${other} aramayı yanıtlamadı`:'Cevapsız arama',tone:'missed'}
  return{missed:false,text:own?`${other} kişisini arıyorsun…`:'Seni arıyor…',tone:'ringing'}
}
function CallRow({message,own,other}:{message:DirectMsg;own:boolean;other:string}){
  const info=callSummary(message,own,other)
  return <article className={`call-log ${info.tone}`}>
    <span className={info.missed?'missed-icon':undefined}><ComposerIcon name="call"/></span>
    <b>{info.text}</b>
    <small>{new Date(message.createdAt).toLocaleTimeString('tr-TR',{hour:'2-digit',minute:'2-digit'})}</small>
  </article>
}
function DirectRow({message,own,latest,onQuote,onEdit,onDelete,onProfile}:{message:DirectMsg;own:boolean;latest:boolean;onQuote:()=>void;onEdit:()=>void;onDelete:()=>void;onProfile:()=>void}){
  const[confirming,setConfirming]=useState(false)
  useEffect(()=>{if(!confirming)return;const timer=setTimeout(()=>setConfirming(false),4000);return()=>clearTimeout(timer)},[confirming])
  return <article className={latest?'message slack-message direct-message latest':'message slack-message direct-message'}>
    <span className={`message-avatar clickable ${avatarHue(message.author.id)}`} role="button" tabIndex={0} title={`${message.author.displayName} profilini aç`} onClick={onProfile} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();onProfile()}}}>{message.author.displayName[0].toUpperCase()}</span>
    <div className="slack-message-content"><div className="message-body">
      {message.quotedMessage&&<div className="discord-quote"><b>{message.quotedMessage.author.displayName}</b><span>{message.quotedMessage.content}</span></div>}
      <header><b className="author-name" title={`${message.author.displayName} profilini aç`} onClick={onProfile}>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleString('tr-TR')}{message.editedAt&&' · düzenlendi'}</small></header>
      <MessageRichText content={message.content}/>
    </div></div>
    <div className="message-actions">
      <button type="button" title="Mesajı alıntıla" aria-label="Mesajı alıntıla" onClick={onQuote}>❝</button>
      {own&&<><button type="button" title="Mesajı düzenle" aria-label="Mesajı düzenle" onClick={onEdit}>✎</button>
      <button type="button" className={confirming?'danger':undefined} title={confirming?'Silmek için tekrar tıkla':'Mesajı sil'} aria-label="Mesajı sil" onClick={()=>{if(confirming){onDelete();setConfirming(false)}else setConfirming(true)}}>{confirming?'?':'⌫'}</button></>}
    </div>
  </article>
}

export default function DirectConversation({auth,organization,person,onCall,inCallWith,profileRequest,onProfileHandled,onRead}:{auth:Auth;organization:Organization;person:ProfileMember;onCall:()=>void;inCallWith:boolean;profileRequest:ProfileUser|null;onProfileHandled:()=>void;onRead:(userId:string)=>void}){
  const[conversation,setConversation]=useState<Conversation|null>(null),[messages,setMessages]=useState<DirectMsg[]>([])
  const[draft,setDraft]=useState(''),[editing,setEditing]=useState<DirectMsg|null>(null)
  const[loading,setLoading]=useState(true),[sending,setSending]=useState(false),[error,setError]=useState('')
  const[profileTarget,setProfileTarget]=useState<{user:ProfileUser;member:ProfileMember}|null>(null),[status,setStatus]=useState(person.status),[emojiOpen,setEmojiOpen]=useState(false)
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false}),[quoted,setQuoted]=useState<DirectMsg|null>(null)
  const[suggestionIndex,setSuggestionIndex]=useState(0),[suggestionsClosed,setSuggestionsClosed]=useState(false)
  const emojiSuggestions=useEmojiSuggestions(draft),suggestionItems=suggestionsClosed?[]:emojiSuggestions
  const activeSuggestion=suggestionItems.length?Math.min(suggestionIndex,suggestionItems.length-1):-1
  const base=`/organizations/${organization.id}/direct`
  const feed=useRef<HTMLDivElement>(null),input=useRef<RichHandle>(null),form=useRef<HTMLFormElement>(null)
  const conversationId=conversation?.id||null
  const typists=useTypists(auth.accessToken,organization.id,auth.user.id,payload=>!!conversationId&&payload.conversationId===conversationId)
  const broadcast=useTypingBroadcast({conversationId:conversationId||undefined})
  const newest=messages[messages.length-1]
  const freshMessage=useFreshMessage(newest?.id??null,newest?.author.id===auth.user.id)

  useEffect(()=>{setStatus(person.status)},[person.id,person.status])
  useEffect(()=>{if(!profileRequest)return;setProfileTarget({user:person,member:{...person,status}});onProfileHandled()},[profileRequest,onProfileHandled])
  useEffect(()=>{requestAnimationFrame(()=>{if(feed.current)feed.current.scrollTop=feed.current.scrollHeight})},[messages.length,conversationId])
  useEffect(()=>{
    let live=true
    setLoading(true);setMessages([]);setConversation(null);setEditing(null);setDraft('');setError('');setProfileTarget(null);setEmojiOpen(false);setQuoted(null)
    ;(async()=>{
      try{
        const found=await api<Conversation>(base,auth.accessToken,{method:'POST',body:JSON.stringify({userId:person.id})})
        if(!live)return
        setConversation(found)
        setMessages(await api<DirectMsg[]>(`${base}/${found.id}/messages`,auth.accessToken))
        await api(`${base}/${found.id}/read`,auth.accessToken,{method:'POST'})
        onRead(person.id)
      }catch(x:any){if(live)setError(x.message||'Sohbet açılamadı.')}
      finally{if(live){setLoading(false);requestAnimationFrame(()=>input.current?.focus())}}
    })()
    return()=>{live=false}
  },[auth.accessToken,base,person.id])

  useEffect(()=>{
    const escape=(e:globalThis.KeyboardEvent)=>{
      if(e.key!=='Escape')return
      if(emojiOpen)setEmojiOpen(false)
      else if(profileTarget)setProfileTarget(null)
      else if(editing){setEditing(null);setDraft('')}
      else if(quoted)setQuoted(null)
      else return
      e.preventDefault()
    }
    document.addEventListener('keydown',escape)
    return()=>document.removeEventListener('keydown',escape)
  })
  function wrapDraft(command:'bold'|'italic'){input.current?.format(command)}
  function addLink(){input.current?.openLink()}
  async function submit(e:FormEvent<HTMLFormElement>){
    e.preventDefault()
    broadcast.stop()
    const raw=draft.trim()
    if(!conversation||!raw)return
    setSending(true);setError('')
    try{
      const{expandEmojiShortcodes}=await import('./emojiShortcodes')
      const content=expandEmojiShortcodes(raw)
      if(editing){const updated=await api<DirectMsg>(`${base}/${conversation.id}/messages/${editing.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({content})});setMessages(items=>upsert(items,updated));setEditing(null)}
      else{const message=await api<DirectMsg>(`${base}/${conversation.id}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content,quotedMessageId:quoted?.id})});setMessages(items=>upsert(items,message));setQuoted(null)}
      setDraft('')
    }catch(x:any){setError(x.message||'Mesaj gönderilemedi.')}
    finally{setSending(false)}
  }
  async function removeMessage(message:DirectMsg){
    if(!conversation)return
    try{await api(`${base}/${conversation.id}/messages/${message.id}`,auth.accessToken,{method:'DELETE'});setMessages(items=>items.filter(item=>item.id!==message.id))}
    catch(x:any){setError(x.message||'Mesaj silinemedi.')}
  }

  useRealtimeEvents(auth.accessToken,organization.id,{
    'direct:created':(payload:{conversationId:string;message:DirectMsg})=>{
      if(payload.conversationId!==conversationId)return
      setMessages(items=>upsert(items,payload.message))
      if(payload.message.author.id!==auth.user.id){void api(`${base}/${payload.conversationId}/read`,auth.accessToken,{method:'POST'}).catch(()=>{});onRead(person.id)}
    },
    'direct:updated':(payload:{conversationId:string;message:DirectMsg})=>{if(payload.conversationId===conversationId)setMessages(items=>upsert(items,payload.message))},
    'direct:deleted':(payload:{conversationId:string;messageId:string})=>{if(payload.conversationId===conversationId)setMessages(items=>items.filter(item=>item.id!==payload.messageId))},
    'presence:update':(payload:{userId:string;status:string})=>{if(payload.userId===person.id)setStatus(payload.status)},
  })

  const openProfile=()=>setProfileTarget({user:person,member:{...person,status}})
  const openAuthorProfile=(author:{id:string;username:string;displayName:string})=>setProfileTarget(author.id===person.id?{user:person,member:{...person,status}}:{user:author,member:{id:author.id,username:author.username,displayName:author.displayName,status:'ONLINE',role:''}})
  const openMentioned=(username:string)=>{if(username.toLocaleLowerCase('tr-TR')===person.username.toLocaleLowerCase('tr-TR'))openProfile()}
  return <MentionContext.Provider value={openMentioned}><section className="channel-content direct-content">
    <header>
      <span className={`direct-avatar large clickable ${avatarHue(person.id)}`} role="button" tabIndex={0} title={`${person.displayName} profilini aç`} onClick={openProfile} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();openProfile()}}}>{userInitials(person.displayName)}<i className={presenceClass(status)}/></span>
      <div><h2 className="direct-title" onClick={openProfile}>{person.displayName}</h2><p>@{person.username} · {statusLabel(status)}</p></div>
      <nav className="channel-tools"><button type="button" className={inCallWith?'tool call-active':'tool'} title={inCallWith?'Aramadasınız':`${person.displayName} kişisini ara`} aria-label="Sesli ara" onClick={onCall} disabled={inCallWith}><ComposerIcon name="call"/></button></nav>
    </header>
    <div className="chat-layout">
      <div className="channel-chat-main">
        <div className="message-feed" ref={feed}>
          {loading&&<p className="messages-state">Sohbet açılıyor…</p>}
          {!loading&&!messages.length&&<div className="channel-welcome"><span className={`direct-avatar huge ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span><h1>{person.displayName}</h1><p>Bu, {person.displayName} ile aranızdaki bireysel sohbetin başlangıcı.</p></div>}
          {messages.map((message,index)=>message.kind&&message.kind!=='TEXT'?<CallRow key={message.id} message={message} own={message.author.id===auth.user.id} other={person.displayName}/>:<DirectRow key={message.id} message={message} own={message.author.id===auth.user.id} latest={message.id===freshMessage} onQuote={()=>{setQuoted(message);setEditing(null);input.current?.focus()}} onEdit={()=>{setEditing(message);setDraft(message.content);requestAnimationFrame(()=>input.current?.focus())}} onDelete={()=>removeMessage(message)} onProfile={()=>openAuthorProfile(message.author)}/>)}
        </div>
        <form ref={form} className="message-composer slack-composer" onSubmit={submit}>
          {typists.length>0&&<p className="typing-line"><i/><i/><i/>{typingText(typists.map(item=>item.name))}</p>}
    {quoted&&!editing&&<div className="composer-reference"><span><b>{quoted.author.displayName}</b> adlı kişiye yanıt veriliyor<small>{quoted.content}</small></span><button type="button" onClick={()=>setQuoted(null)}>×</button></div>}
    {editing&&<div className="composer-reference editing"><span><b>Mesaj düzenleniyor</b><small>{editing.content}</small></span><button type="button" onClick={()=>{setEditing(null);setDraft('')}}>×</button></div>}
    <EmojiSuggestions items={suggestionItems} onPick={item=>setDraft(current=>applyEmojiSuggestion(current,item))} activeIndex={activeSuggestion} onHover={setSuggestionIndex}/>
          <div className="composer-input-row"><button type="button" title="Ekle" aria-label="Ekle"><ComposerIcon name="plus"/></button><RichInput ref={input} value={draft} onChange={value=>{setDraft(value.slice(0,4000));setSuggestionsClosed(false);setSuggestionIndex(0);if(value.trim())broadcast.typing();else broadcast.stop()}} onKeyDown={e=>{
      if(!suggestionItems.length)return
      if(e.key==='ArrowDown'){e.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionItems.length-1)+1)%suggestionItems.length)}
      else if(e.key==='ArrowUp'){e.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionItems.length-1)-1+suggestionItems.length)%suggestionItems.length)}
      else if(e.key==='Enter'||e.key==='Tab'){e.preventDefault();const item=suggestionItems[activeSuggestion];if(item)setDraft(current=>applyEmojiSuggestion(current,item));setSuggestionIndex(0)}
      else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();setSuggestionsClosed(true)}
    }} onEnter={()=>form.current?.requestSubmit()} onStateChange={setFormat} placeholder={`${person.displayName} kişisine mesaj gönder`} disabled={loading}/></div>
          <div className="composer-toolbar"><span>
            <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={e=>e.preventDefault()} onClick={()=>wrapDraft('bold')}><ComposerIcon name="bold"/></button>
            <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={e=>e.preventDefault()} onClick={()=>wrapDraft('italic')}><ComposerIcon name="italic"/></button>
            <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={e=>e.preventDefault()} onClick={addLink}><ComposerIcon name="link"/></button><i/>
            <button type="button" className={emojiOpen?"active":""} title="Emoji seç" onClick={()=>setEmojiOpen(open=>!open)}><ComposerIcon name="emoji"/></button>
            <button type="button" title="Birini etiketle" onClick={()=>{input.current?.insert(`@${person.username} `)}}><ComposerIcon name="mention"/></button>
          </span><button className="composer-send" type="submit" disabled={sending||loading}><ComposerIcon name="send"/></button></div>
          {emojiOpen&&<div className="emoji-picker-popover"><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel onSelect={emoji=>input.current?.insert(emoji)}/></Suspense></div>}
        </form>
      </div>
    </div>
    {profileTarget&&<ProfilePanel user={profileTarget.user} member={profileTarget.member} onClose={()=>setProfileTarget(null)}/>}
    {error&&<div className="channel-content-error">{error}</div>}
  </section></MentionContext.Provider>
}
