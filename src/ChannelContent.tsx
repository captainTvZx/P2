import { createContext, FormEvent, lazy, ReactNode, RefObject, Suspense, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState } from 'react'
import ChannelIcon from './ChannelIcons'
import { ComposerIcon } from './ComposerIcons'
import MessageRichText from './MessageRichText'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import { useEscape } from './useEscape'
import { useStickyScroll } from './useStickyScroll'
import { openConversationWindow } from './popout'
import type { PermissionApi } from './permissions'
import ChannelPermissionsDialog from './ChannelPermissions'
import { useOutsideClose } from './useOutsideClose'
import { useDismissOutside } from './useDismiss'
import { useFreshMessage } from './useFreshMessage'
import { typingText, useTypingBroadcast, useTypists } from './useTyping'
import { ResizeHandle, useResizable } from './useResizable'
import EmojiSuggestions, { applyEmojiSuggestion, SuggestionRow, useEmojiSuggestions } from './EmojiSuggestions'
import { avatarHue, userInitials } from './initials'
import { anchorFrom, MessageMenuPopup, MessageTools, ReactionBar, ReactionPicker, ReactionStrip, ThreadLink, useMessageTools, useReactionPicker, type MessageActions, type MessageMenu, type Reaction } from './messageMenu'
import { API_BASE, p2Fetch } from './apiBase'
import type { ViewerQuality } from './screenShare'
import StreamStage, { type StageItem } from './StreamStage'
import { MentionContext } from './mentions'
import ProfilePanel, { type ProfileUser } from './ProfilePanel'
import { useRealtimeEvents } from './realtime'
import { useCall, type CallParticipant } from './CallProvider'
import { CallIcon } from './CallIcons'
import { updateSettings, useSettings } from './settings'
import { AttachmentPicker, AttachmentQueue, DropZone, attachmentTitle, attachmentsFromContent, useAttachments, useFileDrop, withoutAttachments } from './attachments'

const EmojiPanel=lazy(()=>import('./EmojiPanel'))

type Auth={user:{id:string;username:string;displayName:string;email?:string;title?:string|null;avatarUrl?:string|null};accessToken:string}
type Organization={id:string};type Project={id:string}
type Channel={id:string;name:string;topic?:string|null;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';position:number}
type Message={id:string;content:string;createdAt:string;editedAt?:string|null;reactions?:Reaction[];forwardedFromId?:string|null;quotedMessage?:{id:string;content:string;author:{displayName:string}}|null;author:{id:string;username:string;displayName:string};_count?:{replies:number};replies?:Array<{createdAt:string;author:{displayName:string}}>} 
type SearchHit=Message&{parentId?:string|null}
type HistoryItem={id:string;content:string;createdAt:string}
type ChannelChoice={id:string;name:string;type:Channel['type']};type CategoryChoice={channels:ChannelChoice[]}
export type MentionSuggestion={label:string;description:string;value:string;avatar:string}
type UserGroupSummary={id:string;handle:string;name:string;members:{id:string}[]}
type OrganizationMember={id:string;username:string;displayName:string;email?:string;title?:string|null;avatarUrl?:string|null;status:string;role:string}
const API=API_BASE
async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{const response=await p2Fetch(API+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}});const data=await response.json().catch(()=>({}));if(!response.ok)throw data;return data}
const ProfileContext=createContext<((user:ProfileUser)=>void)|null>(null)
const MemberContext=createContext<OrganizationMember[]>([])
function useProfileOpener(){return useContext(ProfileContext)}
function presenceClass2(status?:string){return status==='ONLINE'?'presence online':status==='AWAY'?'presence away':status==='BUSY'?'presence busy':'presence'}
function MembersPopover({members,meId,onSelect}:{members:OrganizationMember[];meId:string;onSelect:(user:ProfileUser)=>void}){
  const[term,setTerm]=useState('')
  const needle=term.trim().toLocaleLowerCase('tr-TR')
  const visible=members.filter(member=>!needle||member.displayName.toLocaleLowerCase('tr-TR').includes(needle)||member.username.toLocaleLowerCase('tr-TR').includes(needle))
  return <div className="members-popover-body">
    <div className="search-field"><ComposerIcon name="search"/><input autoFocus value={term} onChange={e=>setTerm(e.target.value)} placeholder="Kişi ara"/></div>
    <button type="button" className="member-add"><span>＋</span>Kanala kişi ekle<small>yakında</small></button>
    <div className="members-popover-list">
      {visible.map(member=><button type="button" key={member.id} className="member-row" onClick={()=>onSelect(member)}>
        <span className={`member-avatar ${avatarHue(member.id)}`}>{member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}<i className={presenceClass2(member.status)}/></span>
        <div><b>{member.displayName}{member.id===meId?' (sen)':''}</b><small>@{member.username}</small></div>
        {(member.role==='OWNER'||member.role==='ADMIN')&&<u>{member.role==='OWNER'?'Kurucu':'Yönetici'}</u>}
      </button>)}
      {!visible.length&&<p className="messages-state">Eşleşen kişi yok.</p>}
    </div>
  </div>
}
function SearchPanel({channelName,onClose,onSearch,onOpen}:{channelName:string;onClose:()=>void;onSearch:(term:string)=>Promise<SearchHit[]>;onOpen:(hit:SearchHit)=>void}){
  const[term,setTerm]=useState(''),[results,setResults]=useState<SearchHit[]|null>(null),[busy,setBusy]=useState(false)
  const kutu=useDismissOutside<HTMLElement>(onClose)
  useEscape(true,onClose)
  useEffect(()=>{
    const value=term.trim()
    if(value.length<2){setResults(null);return}
    let live=true
    const timer=window.setTimeout(()=>{setBusy(true);onSearch(value).then(items=>{if(live)setResults(items)}).finally(()=>{if(live)setBusy(false)})},280)
    return()=>{live=false;window.clearTimeout(timer)}
  },[term])
  return <aside ref={kutu} className="side-drawer search-panel"><header><b>#{channelName} içinde ara</b><button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button></header>
    <div className="drawer-scroll">
      <div className="search-field"><ComposerIcon name="search"/><input autoFocus value={term} onChange={e=>setTerm(e.target.value)} placeholder="Mesajlarda ara…"/></div>
      {busy&&<p className="messages-state">Aranıyor…</p>}
      {!busy&&results&&!results.length&&<p className="messages-state">Sonuç bulunamadı.</p>}
      {!busy&&results?.map(item=><button type="button" key={item.id} className="search-hit" onClick={()=>onOpen(item)} title="Mesaja git"><header><b>{item.author.displayName}</b><small>{new Date(item.createdAt).toLocaleString('tr-TR')}</small></header><p>{item.content}</p>{item.parentId&&<em>thread yanıtı</em>}</button>)}
      {!results&&!busy&&<p className="messages-state">En az 2 karakter yaz.</p>}
    </div>
  </aside>
}
function upsertMessage(items:Message[],message:Message){const index=items.findIndex(item=>item.id===message.id);if(index<0)return[...items,message];const next=[...items];next[index]={...next[index],...message};return next}
function forumTopicTitle(content:string){const text=withoutAttachments(content).trim();if(text)return text;const file=attachmentsFromContent(content)[0];return file?attachmentTitle(file):'Dosya paylaşımı'}
function summarizeReactions(rows:{emoji:string;userId:string}[],userId:string){const summary=new Map<string,Reaction>();for(const row of rows){const item=summary.get(row.emoji)||{emoji:row.emoji,count:0,reacted:false};item.count+=1;if(row.userId===userId)item.reacted=true;summary.set(row.emoji,item)}return[...summary.values()]}
function Avatar({message}:{message:Message}){const open=useProfileOpener(),members=useContext(MemberContext),member=members.find(item=>item.id===message.author.id);return <span className={`message-avatar ${avatarHue(message.author.id)}${open?' clickable':''}`} role={open?'button':undefined} tabIndex={open?0:undefined} title={open?`${message.author.displayName} profilini aç`:undefined} onClick={e=>{if(!open)return;e.stopPropagation();open({...message.author,...member})}} onKeyDown={e=>{if(open&&(e.key==='Enter'||e.key===' ')){e.preventDefault();e.stopPropagation();open({...message.author,...member})}}}>{member?.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(message.author.displayName)}</span>}
function MessageBody({message}:{message:Message}){const open=useProfileOpener();return <div className="message-body">{message.quotedMessage&&<div className="discord-quote"><b>{message.quotedMessage.author.displayName}</b><span>{message.quotedMessage.content}</span></div>}{message.forwardedFromId&&<small className="forwarded-label">↗ İletildi</small>}<header><b className={open?'author-name':undefined} onClick={()=>open?.(message.author)} title={open?`${message.author.displayName} profilini aç`:undefined}>{message.author.displayName}</b><small>{new Date(message.createdAt).toLocaleString('tr-TR')}{message.editedAt&&' · düzenlendi'}</small></header><MessageRichText content={message.content}/></div>}
const MENU_EDGE_GAP=12

function VoiceRoomStage({name,participants,members,me,levels,joined,onJoin,onProfile}:{name:string;participants:CallParticipant[];members:OrganizationMember[];me:string;levels:Record<string,number>;joined:boolean;onJoin:()=>void;onProfile:(user:ProfileUser)=>void}){
  const call=useCall(),settings=useSettings()
  const[menu,setMenu]=useState<{participant:CallParticipant;x:number;y:number;up:number}|null>(null)
  const[cardsOpen,setCardsOpen]=useState(true)
  if(!participants.length)return <main className="voice-room-stage empty"><div className="voice-room-empty"><span><ChannelIcon type="VOICE"/></span><h1>{name}</h1><p>Şu an sesli sohbette kimse yok</p>{!joined&&<button type="button" className="voice-room-join" onClick={onJoin}>Sesli odaya katıl</button>}</div></main>
  const selected=menu?.participant,isMe=selected?.userId===me
  // Yayını izlemek için kişiye tıklanır; birden fazla yayın aynı anda izlenebilir.
  const live=(participant:CallParticipant)=>Boolean((participant.screen||participant.video)&&call.remote[participant.userId]?.stream)
  const stage:StageItem[]=participants
    .filter(participant=>participant.userId!==me&&call.watching.includes(participant.userId)&&live(participant))
    .map(participant=>({userId:participant.userId,stream:call.remote[participant.userId]!.stream,label:`${participant.displayName}${participant.screen?' · ekran':''}`}))
  if(call.localVideo&&participants.some(participant=>participant.userId===me)&&(call.screenOn||call.cameraOn))
    stage.push({userId:me,stream:call.localVideo,label:call.screenOn?'Sen · ekran':'Sen',self:true})
  const featureStream=stage.length>0
  return <main className={`voice-room-stage${featureStream?' featured':''}${featureStream&&!cardsOpen?' cards-hidden':''}`}>
    {featureStream&&<div className="voice-room-feature"><StreamStage items={stage} onClose={userId=>call.watch(userId)}/></div>}
    {featureStream&&<button type="button" className="cards-toggle" onClick={()=>setCardsOpen(open=>!open)} title={cardsOpen?'Katılımcıları gizle':'Katılımcıları göster'}>
      <i className={cardsOpen?'chevron down':'chevron up'}/>{cardsOpen?'Katılımcıları gizle':`Katılımcılar (${participants.length})`}
    </button>}
    {(!featureStream||cardsOpen)&&<div className="voice-room-grid">{participants.map(participant=>{
    const member=members.find(item=>item.id===participant.userId),level=participant.muted?0:(levels[participant.userId]||0)
    const watchable=participant.userId!==me&&live(participant)
    const watched=call.watching.includes(participant.userId)
    return <div role="button" tabIndex={0} key={participant.userId} className={`voice-room-card ${avatarHue(participant.userId)}${level>.08?' speaking':''}${watched?' watched':''}`} style={{'--voice-level':level} as React.CSSProperties} onClick={()=>{if(watchable){call.watch(watched?null:participant.userId);return}if(member)onProfile(member)}} onKeyDown={event=>{if(event.key==='Enter'&&member)onProfile(member)}} onContextMenu={event=>{event.preventDefault();setMenu({participant,x:event.clientX,y:event.clientY,up:event.clientY})}}>
      <span className="voice-room-avatar">{participant.avatarUrl||member?.avatarUrl?<img src={participant.avatarUrl||member!.avatarUrl!} alt=""/>:userInitials(participant.displayName)}</span>
      <footer><span><b>{participant.displayName}</b>{participant.userId===me&&<small>SEN</small>}{participant.screen&&<small className="live-badge">YAYINDA</small>}</span>{participant.muted&&<CallIcon name="micOff"/>}</footer>
      {watchable&&<button type="button" className="watch-button" onClick={event=>{event.stopPropagation();call.watch(watched?null:participant.userId)}}>{watched?'Yayını kapat':'Yayını izle'}</button>}
    </div>
  })}</div>}
    {!joined&&<button type="button" className="voice-room-join" onClick={onJoin}>Sesli odaya katıl</button>}{menu&&selected&&<MessageMenuPopup menu={menu} onClose={()=>setMenu(null)} className="message-context-menu voice-user-menu"><header><span className={`member-avatar ${avatarHue(selected.userId)}`}>{members.find(item=>item.id===selected.userId)?.avatarUrl?<img src={members.find(item=>item.id===selected.userId)!.avatarUrl!} alt=""/>:userInitials(selected.displayName)}</span><div><b>{selected.displayName}</b><small>{isMe?'Kendi ses ayarların':'Kişisel ses ayarları'}</small></div></header>{!isMe&&live(selected)&&<><button className="watch-entry" onClick={()=>{call.watch(selected.userId);setMenu(null)}}><CallIcon name="screen"/>{call.watching.includes(selected.userId)?'Yayını kapat':'Yayını izle'}</button><hr/></>}{isMe?<><label><span>Mikrofon seviyem</span><b>{settings.inputVolume}%</b><input type="range" min="0" max="100" value={settings.inputVolume} onChange={event=>updateSettings({inputVolume:Number(event.target.value)})}/></label><label><span>Hoparlör seviyem</span><b>{settings.outputVolume}%</b><input type="range" min="0" max="100" value={settings.outputVolume} onChange={event=>updateSettings({outputVolume:Number(event.target.value)})}/></label><hr/><button onClick={()=>{call.toggleMute();setMenu(null)}}><CallIcon name={call.muted?'mic':'micOff'}/>{call.muted?'Mikrofonumu aç':'Mikrofonumu sustur'}</button><button onClick={()=>{call.toggleDeafen();setMenu(null)}}><CallIcon name="headphones"/>{call.deafened?'Sağırlaştırmayı kapat':'Kendimi sağırlaştır'}</button></>:<><label><span>Kullanıcı ses seviyesi</span><b>{call.userVolumes[selected.userId]??100}%</b><input type="range" min="0" max="100" value={call.userVolumes[selected.userId]??100} onChange={event=>call.setUserVolume(selected.userId,Number(event.target.value))}/></label><hr/><button onClick={()=>{call.toggleLocalMute(selected.userId);setMenu(null)}}><CallIcon name={call.locallyMuted.has(selected.userId)?'volume':'volumeOff'}/>{call.locallyMuted.has(selected.userId)?'Susturmayı kaldır':'Kendim için sustur'}</button><button onClick={()=>{call.togglePeerDeafen(selected.userId);setMenu(null)}}><CallIcon name="headphones"/>{call.deafenedFor.has(selected.userId)?'Sesimi yeniden gönder':'Sesimi bu kişiye gönderme'}</button></>}</MessageMenuPopup>}</main>
}
function RootMessage({message,className,onReact,actions}:{message:Message;className:string;onReact:(emoji:string)=>void;actions?:{own:boolean;link?:string;onQuote:()=>void;onEdit:()=>void;onDelete:()=>void;onForward?:()=>void;onHistory?:()=>void}}){
  const tools=useMessageTools()
  return <article className={className} onContextMenu={actions?e=>{e.preventDefault();tools.openAtPoint(e.clientX,e.clientY)}:undefined}>
    <Avatar message={message}/>
    <div className="slack-message-content"><MessageBody message={message}/><ReactionBar always reactions={message.reactions||[]} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/></div>
    <MessageTools tools={tools} actions={{own:actions?.own??false,content:message.content,editedAt:message.editedAt,link:actions?.link,onReact,onQuote:actions?.onQuote??(()=>{}),onEdit:actions?.onEdit??(()=>{}),onDelete:actions?.onDelete??(()=>{}),onForward:actions?.onForward,onHistory:actions?.onHistory}}/>
  </article>
}
function ChannelMessage({message,onThread,onQuote,onEdit,onForward,onDelete,onHistory,onReact,channelId,own,latest,flash}:{message:Message;latest?:boolean;flash?:boolean;onThread:()=>void;onQuote:()=>void;onEdit:()=>void;onForward:()=>void;onDelete:()=>void;onHistory:()=>void;onReact:(emoji:string)=>void;channelId:string;own:boolean}){const count=message._count?.replies||0,last=message.replies?.[0],tools=useMessageTools();return <article id={`msg-${message.id}`} className={`message slack-message${latest?' latest':''}${flash?' flash':''}`} onContextMenu={e=>{e.preventDefault();tools.openAtPoint(e.clientX,e.clientY)}}><Avatar message={message}/><div className="slack-message-content"><MessageBody message={message}/><ReactionBar reactions={message.reactions||[]} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/>{count>0&&<ThreadLink count={count} people={(message.replies||[]).map(item=>item.author)} lastAt={last?.createdAt} onOpen={onThread}/>}</div><MessageTools tools={tools} actions={{own,content:message.content,editedAt:message.editedAt,link:`p2://channels/${channelId}/messages/${message.id}`,onReact,onQuote,onEdit,onDelete,onThread,onForward,onHistory}}/></article>}function DayDivider({date}:{date:string}){const value=new Date(date),today=new Date();const same=value.toDateString()===today.toDateString();return <div className="day-divider"><span>{same?'Bugün':value.toLocaleDateString('tr-TR',{day:'numeric',month:'long',weekday:'long'})}</span></div>}
const commands=[{name:'shrug',description:'Mesajın sonuna ¯\\_(ツ)_/¯ ekler'},{name:'me',description:'Mesajı eylem biçiminde yazar'},{name:'spoiler',description:'Mesajı spoiler olarak işaretler'}]
export function ComposerSuggestions({mentions,commands:commandItems,onMention,onCommand,activeIndex=-1,onHover}:{mentions:MentionSuggestion[];commands:typeof commands;onMention:(item:MentionSuggestion)=>void;onCommand:(name:string)=>void;activeIndex?:number;onHover?:(index:number)=>void}){if(!mentions.length&&!commandItems.length)return null;return <div className="composer-suggestions">{mentions.length>0&&<><header><b>Etiketler</b><small>↑↓ ile seç, Enter ile ekle</small></header>{mentions.map((item,index)=><SuggestionRow key={item.value} active={index===activeIndex} onHover={()=>onHover?.(index)} onMouseDown={e=>{e.preventDefault();onMention(item)}}><span>{item.avatar}</span><div><b>{item.label}</b><small>{item.description}</small></div></SuggestionRow>)}</>}{commandItems.length>0&&<><header><b>Komutlar</b><small>↑↓ ile seç, Enter ile ekle</small></header>{commandItems.map((item,index)=><SuggestionRow key={item.name} active={mentions.length+index===activeIndex} onHover={()=>onHover?.(mentions.length+index)} onMouseDown={e=>{e.preventDefault();onCommand(item.name)}}><span>/</span><div><b>/{item.name}</b><small>{item.description}</small></div></SuggestionRow>)}</>}</div>}
function expandSlashCommand(value:string){if(value.startsWith('/shrug'))return `${value.slice(6).trim()} ¯\\_(ツ)_/¯`.trim();if(value.startsWith('/me '))return `*${value.slice(4).trim()}*`;if(value.startsWith('/spoiler '))return `||${value.slice(9).trim()}||`;return value}

// Kanal ayarları: ad ve konu. Konu, başlıkta varsayılan açıklamanın yerine geçer.
function ChannelSettingsDialog({auth,organization,project,channel,onClose,onSaved}:{auth:Auth;organization:Organization;project:Project;channel:Channel;onClose:()=>void;onSaved:(channel:Channel)=>void}){
  const[name,setName]=useState(channel.name),[topic,setTopic]=useState(channel.topic||''),[busy,setBusy]=useState(false),[error,setError]=useState('')
  useEscape(true,onClose)
  const dirty=name.trim()!==channel.name||topic.trim()!==(channel.topic||'')
  async function save(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    if(!dirty||busy)return
    setBusy(true);setError('')
    try{
      const updated=await api<Channel>(`/organizations/${organization.id}/projects/${project.id}/channels/${channel.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({name:name.trim(),topic:topic.trim()})})
      onSaved(updated)
    }catch(problem:any){setError(problem?.message||'Kanal güncellenemedi.')}
    finally{setBusy(false)}
  }
  return <div className="action-modal-layer" onMouseDown={onClose}><section className="message-action-dialog channel-settings-dialog" onMouseDown={event=>event.stopPropagation()}>
    <header><div><small>KANAL AYARLARI</small><h3>#{channel.name}</h3></div><button type="button" onClick={onClose}>×</button></header>
    <form onSubmit={save}>
      <label><span>Kanal adı</span><input value={name} maxLength={40} autoFocus onChange={event=>setName(event.target.value)}/></label>
      <label><span>Konu</span><input value={topic} maxLength={240} placeholder="Bu kanal ne için kullanılıyor?" onChange={event=>setTopic(event.target.value)}/></label>
      <p className="channel-settings-hint">Konu, kanal başlığında adın altında görünür ve herkes tarafından görülür.</p>
      {error&&<p className="channel-settings-error">{error}</p>}
      <footer><button type="button" onClick={onClose}>Vazgeç</button><button type="submit" className="primary" disabled={!dirty||busy}>{busy?'Kaydediliyor…':'Kaydet'}</button></footer>
    </form>
  </section></div>
}

export default function ChannelContent({auth,organization,project,channel,onDirectMessage,profileRequest,onProfileHandled,preference,onPreference,onCallChannel,activeCallChannelId,voiceRoom,jumpRequest,onJumpHandled,perms,onSplit,onLeave,onChannelUpdated,splitPane,onCloseSplit}:{jumpRequest?:{messageId:string;parentId:string|null}|null;onJumpHandled?:()=>void;auth:Auth;organization:Organization;project:Project;channel:Channel|null;onDirectMessage?:(user:ProfileUser)=>void;profileRequest?:ProfileUser|null;onProfileHandled?:()=>void;preference?:{starred:boolean;notify:string};onPreference?:(patch:{starred?:boolean;notify?:string})=>void;onCallChannel?:(channelId:string,ring?:boolean)=>void;activeCallChannelId?:string|null;voiceRoom?:CallParticipant[];perms?:PermissionApi;onSplit?:(channel:Channel)=>void;onLeave?:(channel:Channel)=>void;onChannelUpdated?:(channel:Channel)=>void;splitPane?:boolean;onCloseSplit?:()=>void}){
  const callApi=useCall()
  const[voiceChatOpen,setVoiceChatOpen]=useState(false)
  // Oda sohbeti diğer paneller gibi kenarından sürüklenir (eskiden yalnızca
  // tarayıcının sol alt köşedeki resize tutamağıyla boyutlanıyordu).
  const voiceChat=useResizable('p2:voice-chat-width',430,300,980,-1)
  const[messages,setMessages]=useState<Message[]>([]),[selectedTopic,setSelectedTopic]=useState<Message|null>(null),[selectedThread,setSelectedThread]=useState<Message|null>(null),[replies,setReplies]=useState<Message[]>([]),[draft,setDraft]=useState(''),[topicTitle,setTopicTitle]=useState(''),[emojiOpen,setEmojiOpen]=useState(false),[quoted,setQuoted]=useState<Message|null>(null),[editing,setEditing]=useState<Message|null>(null),[editingReply,setEditingReply]=useState<Message|null>(null),[quotedReply,setQuotedReply]=useState<Message|null>(null),[forwarding,setForwarding]=useState<Message|null>(null),[deleting,setDeleting]=useState<Message|null>(null),[historyMessage,setHistoryMessage]=useState<Message|null>(null),[profileUser,setProfileUser]=useState<ProfileUser|null>(null),[searchOpen,setSearchOpen]=useState(false),[detailsOpen,setDetailsOpen]=useState(false),[channelSettings,setChannelSettings]=useState(false),[channelPerms,setChannelPerms]=useState(false),[leaveOpen,setLeaveOpen]=useState(false),[flashId,setFlashId]=useState<string|null>(null),[toolMenu,setToolMenu]=useState<{menu:MessageMenu;kind:'notify'|'more'|'members'}|null>(null),[history,setHistory]=useState<HistoryItem[]>([]),[forwardChannels,setForwardChannels]=useState<ChannelChoice[]>([]),[members,setMembers]=useState<OrganizationMember[]>([]),[userGroups,setUserGroups]=useState<UserGroupSummary[]>([]),[loading,setLoading]=useState(false),[sending,setSending]=useState(false),[error,setError]=useState('')
  const canAttach=!!channel&&(perms?perms.canIn(channel.id,'message.attach'):true)
  const composerInput=useRef<RichHandle>(null)
  const attachments=useAttachments(auth.accessToken)
  const topicAttachments=useAttachments(auth.accessToken)
  const forumReplyAttachments=useAttachments(auth.accessToken)
  // Dosya, yazma kutusuna değil sohbetin herhangi bir yerine bırakılabilir.
  const drop=useFileDrop(files=>{if(!canAttach)return;const target=channel?.type==='FORUM'?(selectedTopic?forumReplyAttachments:topicAttachments):attachments;void target.add(files)},canAttach)
  const composerForm=useRef<HTMLFormElement>(null)
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const thread=useResizable('p2:thread-width',360,280,660,-1)
  useOutsideClose(Boolean(selectedThread),()=>{setSelectedThread(null);setReplies([])},'.thread-link')
  useOutsideClose(emojiOpen,()=>setEmojiOpen(false),'.composer-toolbar')
  const typists=useTypists(auth.accessToken,organization.id,auth.user.id,payload=>!!channel&&payload.channelId===channel.id)
  const broadcast=useTypingBroadcast({organizationId:organization.id,channelId:channel?.id})
  const newest=messages[messages.length-1]
  const freshMessage=useFreshMessage(newest?.id??null,newest?.author.id===auth.user.id)
  const messageFeed=useRef<HTMLDivElement>(null)
  const base=channel?`/organizations/${organization.id}/projects/${project.id}/channels/${channel.id}`:''
  const emojiSuggestions=useEmojiSuggestions(draft)
  const mentionMatch=draft.match(/(?:^|\s)@([a-zA-Z0-9_.-]*)$/),mentionNeedle=mentionMatch?.[1].toLocaleLowerCase('tr-TR')
  // @here/@everyone dışındaki grup etiketleri kuruluşun kullanıcı gruplarından gelir.
  const systemMentions:MentionSuggestion[]=[
    {label:'@here',description:'Şu anda çevrimiçi olanlar',value:'here',avatar:'H'},
    {label:'@everyone',description:'Kanaldaki herkes',value:'everyone',avatar:'E'},
    ...userGroups.map(group=>({label:`@${group.handle}`,description:`${group.name} · ${group.members.length} kişi`,value:group.handle,avatar:group.handle.slice(0,2).toLocaleUpperCase('tr-TR')})),
  ]
  const mentionSuggestions=mentionMatch?[...systemMentions,...members.map(member=>({label:`@${member.username}`,description:member.displayName,value:member.username,avatar:userInitials(member.displayName)}))].filter(item=>item.value.toLocaleLowerCase('tr-TR').startsWith(mentionNeedle||'')).slice(0,8):[]
  const commandMatch=draft.match(/(?:^|\s)\/([a-zA-Z]*)$/),commandSuggestions=commandMatch?commands.filter(item=>item.name.startsWith(commandMatch[1].toLowerCase())):[]
  const[suggestionIndex,setSuggestionIndex]=useState(0),[suggestionsClosed,setSuggestionsClosed]=useState(false)
  const emojiItems=suggestionsClosed?[]:emojiSuggestions,mentionItems=suggestionsClosed?[]:mentionSuggestions,commandItems=suggestionsClosed?[]:commandSuggestions
  const suggestionCount=emojiItems.length+mentionItems.length+commandItems.length,activeSuggestion=suggestionCount?Math.min(suggestionIndex,suggestionCount-1):-1
  function pickSuggestion(index:number){
    if(index<0)return
    if(index<emojiItems.length){const item=emojiItems[index];setDraft(current=>applyEmojiSuggestion(current,item));return}
    const rest=index-emojiItems.length
    if(rest<mentionItems.length){pickMention(mentionItems[rest]);return}
    pickCommand(commandItems[rest-mentionItems.length].name)
  }
  function suggestionKeys(e:React.KeyboardEvent<HTMLDivElement>){
    if(!suggestionCount)return
    if(e.key==='ArrowDown'){e.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionCount-1)+1)%suggestionCount)}
    else if(e.key==='ArrowUp'){e.preventDefault();setSuggestionIndex(current=>(Math.min(current,suggestionCount-1)-1+suggestionCount)%suggestionCount)}
    else if(e.key==='Enter'||e.key==='Tab'){e.preventDefault();pickSuggestion(activeSuggestion);setSuggestionIndex(0)}
    else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();setSuggestionsClosed(true)}
  }
  useEffect(()=>{setSelectedTopic(null);setSelectedThread(null);setQuoted(null);setEditing(null);setEditingReply(null);setQuotedReply(null);setProfileUser(null);setSearchOpen(false);setToolMenu(null);setDetailsOpen(false);setEmojiOpen(false);setReplies([]);setDraft('');setTopicTitle('');attachments.clear();topicAttachments.clear();forumReplyAttachments.clear();setMessages([]);if(!channel)return;setLoading(true);setError('');api<Message[]>(`${base}/messages`,auth.accessToken).then(setMessages).catch(()=>setError('Mesajlar yüklenemedi.')).finally(()=>setLoading(false))},[channel?.id,auth.accessToken])
  useEffect(()=>{api<OrganizationMember[]>(`/organizations/${organization.id}/members`,auth.accessToken).then(setMembers).catch(()=>setMembers([]))},[organization.id,auth.accessToken])
  const loadUserGroups=useCallback(()=>{api<UserGroupSummary[]>(`/organizations/${organization.id}/user-groups`,auth.accessToken).then(setUserGroups).catch(()=>setUserGroups([]))},[organization.id,auth.accessToken])
  useEffect(loadUserGroups,[loadUserGroups])
  useStickyScroll(messageFeed,[messages.length,channel?.id])
  function feedPinned(){const feed=messageFeed.current;return !feed||feed.scrollHeight-feed.scrollTop-feed.clientHeight<120}
  function scrollFeedToBottom(){requestAnimationFrame(()=>{const feed=messageFeed.current;if(feed)feed.scrollTop=feed.scrollHeight})}
  useRealtimeEvents(auth.accessToken,organization.id,{
    'usergroup:changed':()=>loadUserGroups(),
    'usergroup:removed':()=>loadUserGroups(),
    'message:created':(payload:{channelId:string;parentId:string|null;parentReplies?:number;message:Message})=>{
      if(payload.channelId!==channel?.id)return
      if(!payload.parentId){const pinned=feedPinned();setMessages(items=>upsertMessage(items,{_count:{replies:0},...payload.message}));if(pinned)scrollFeedToBottom();return}
      setMessages(items=>items.map(item=>item.id===payload.parentId?{...item,_count:{replies:payload.parentReplies??((item._count?.replies||0)+1)},replies:[{createdAt:payload.message.createdAt,author:{displayName:payload.message.author.displayName}}]}:item))
      if((selectedThread||selectedTopic)?.id===payload.parentId)setReplies(items=>upsertMessage(items,payload.message))
    },
    'message:updated':(payload:{channelId:string;message:Message})=>{
      if(payload.channelId!==channel?.id)return
      const patch=(items:Message[])=>items.map(item=>item.id===payload.message.id?{...item,...payload.message}:item)
      setMessages(patch);setReplies(patch)
      setSelectedThread(current=>current&&current.id===payload.message.id?{...current,...payload.message}:current)
      setSelectedTopic(current=>current&&current.id===payload.message.id?{...current,...payload.message}:current)
    },
    'message:deleted':(payload:{channelId:string;messageId:string;parentId:string|null;parentReplies:number})=>{
      if(payload.channelId!==channel?.id)return
      setReplies(items=>items.filter(item=>item.id!==payload.messageId))
      setMessages(items=>items.filter(item=>item.id!==payload.messageId).map(item=>item.id===payload.parentId?{...item,_count:{replies:payload.parentReplies}}:item))
      if((selectedThread||selectedTopic)?.id===payload.messageId){setSelectedThread(null);setSelectedTopic(null);setReplies([])}
      if(editingReply?.id===payload.messageId)setEditingReply(null)
      if(editing?.id===payload.messageId){setEditing(null);setDraft('')}
    },
    'message:reactions':(payload:{channelId:string;messageId:string;rows:{emoji:string;userId:string}[]})=>{
      if(payload.channelId!==channel?.id)return
      const reactions=summarizeReactions(payload.rows,auth.user.id),pinned=feedPinned()
      const patch=(items:Message[])=>items.map(item=>item.id===payload.messageId?{...item,reactions}:item)
      setMessages(patch);setReplies(patch)
      setSelectedThread(current=>current&&current.id===payload.messageId?{...current,reactions}:current)
      setSelectedTopic(current=>current&&current.id===payload.messageId?{...current,reactions}:current)
      if(pinned)scrollFeedToBottom()
    },
    'presence:update':(payload:{userId:string;status:string})=>setMembers(items=>items.map(item=>item.id===payload.userId?{...item,status:payload.status}:item)),
  })
  useEffect(()=>{
    const escape=(e:globalThis.KeyboardEvent)=>{
      if(e.key!=='Escape')return
      if(toolMenu){setToolMenu(null)}
      else if(emojiOpen){setEmojiOpen(false)}
      else if(profileUser){setProfileUser(null)}
      else if(detailsOpen){setDetailsOpen(false)}
      else if(searchOpen){setSearchOpen(false)}
      else if(forwarding||deleting||historyMessage){setForwarding(null);setDeleting(null);setHistoryMessage(null)}
      else if(editing){setEditing(null);setDraft('')}
      else if(quoted){setQuoted(null)}
      else if(editingReply){setEditingReply(null)}
      else if(quotedReply){setQuotedReply(null)}
      else if(selectedThread||selectedTopic){setSelectedThread(null);setSelectedTopic(null);setReplies([])}
      else return
      e.preventDefault()
    }
    document.addEventListener('keydown',escape)
    return()=>document.removeEventListener('keydown',escape)
  })
  function openMentionedProfile(username:string){
    const target=members.find(item=>item.username.toLocaleLowerCase('tr-TR')===username.toLocaleLowerCase('tr-TR'))
    if(target)setProfileUser(target)
  }
  function mentionFromProfile(user:ProfileUser){setDraft(current=>`${current}${current&&!current.endsWith(' ')?' ':''}@${user.username} `);setProfileUser(null);requestAnimationFrame(()=>composerInput.current?.focus())}
  const profilePanel=profileUser?<ProfilePanel user={profileUser} member={members.find(item=>item.id===profileUser.id)} onClose={()=>setProfileUser(null)} onMention={()=>mentionFromProfile(profileUser)} onDirect={onDirectMessage&&profileUser.id!==auth.user.id?()=>onDirectMessage(profileUser):undefined}/>:null
  const starred=preference?.starred===true,notify=preference?.notify||'MENTIONS'
  const searchMessages=async(term:string)=>{try{return await api<SearchHit[]>(`${base}/search?q=${encodeURIComponent(term)}`,auth.accessToken)}catch{return[]}}
  // Etkinlik akışından gelen "mesaja git" isteği: mesajlar yüklenince kaydırır.
  useEffect(()=>{
    if(!jumpRequest||loading)return
    void jumpToMessage({id:jumpRequest.messageId,parentId:jumpRequest.parentId}as SearchHit)
    onJumpHandled?.()
  },[jumpRequest?.messageId,loading])

  async function jumpToMessage(hit:SearchHit){
    setSearchOpen(false)
    setFlashId(hit.id)
    // Thread yanıtıysa önce kök mesajın thread'i açılır, sonra yanıta kaydırılır.
    if(hit.parentId){
      const root=messages.find(item=>item.id===hit.parentId)
      if(root)await openReplies(root,forum)
    }
    for(let attempt=0;attempt<12;attempt++){
      const node=document.getElementById(`msg-${hit.id}`)
      if(node){node.scrollIntoView({block:'center',behavior:'smooth'});break}
      await new Promise(resolve=>requestAnimationFrame(()=>resolve(null)))
    }
    window.setTimeout(()=>setFlashId(current=>current===hit.id?null:current),2800)
  }
  const sidePanel=profilePanel||(searchOpen&&channel?<SearchPanel channelName={channel.name} onClose={()=>setSearchOpen(false)} onSearch={searchMessages} onOpen={jumpToMessage}/>:null)
  useEffect(()=>{if(profileRequest){setProfileUser(profileRequest);onProfileHandled?.()}},[profileRequest])
  function wrapDraft(command:'bold'|'italic'){composerInput.current?.format(command)}
  function addLink(){composerInput.current?.openLink()}
  function pickMention(item:MentionSuggestion){setDraft(current=>current.replace(/@([a-zA-Z0-9_.-]*)$/,`@${item.value} `));requestAnimationFrame(()=>composerInput.current?.focus())}
  function pickCommand(name:string){setDraft(current=>current.replace(/\/([a-zA-Z]*)$/,`/${name} `));requestAnimationFrame(()=>composerInput.current?.focus())}
  async function send(e:FormEvent<HTMLFormElement>){e.preventDefault();if(!channel)return;broadcast.stop();const raw=expandSlashCommand(draft.trim());if(!raw&&!attachments.files.length)return;setSending(true);setError('');try{const{expandEmojiShortcodes}=await import('./emojiShortcodes');const content=attachments.appendTo(expandEmojiShortcodes(raw));if(editing){const updated=await api<Message>(`${base}/messages/${editing.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({content})});setMessages(items=>items.map(item=>item.id===updated.id?{...item,...updated}:item));setEditing(null)}else{const message=await api<Message>(`${base}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content,quotedMessageId:quoted?.id})});setMessages(items=>upsertMessage(items,{...message,_count:{replies:0}}));setQuoted(null)}setDraft('');attachments.clear()}catch(x:any){setError(x.message||'Mesaj gönderilemedi.')}finally{setSending(false)}}
  async function sendTopic(e:FormEvent<HTMLFormElement>){e.preventDefault();const raw=topicTitle.trim()||(topicAttachments.files[0]?attachmentTitle(topicAttachments.files[0]):'');if(!raw&&!topicAttachments.files.length)return;setSending(true);try{const{expandEmojiShortcodes}=await import('./emojiShortcodes');const content=topicAttachments.appendTo(expandEmojiShortcodes(raw)),message=await api<Message>(`${base}/messages`,auth.accessToken,{method:'POST',body:JSON.stringify({content})});setMessages(items=>upsertMessage(items,{...message,_count:{replies:0}}));setTopicTitle('');topicAttachments.clear()}catch(x:any){setError(x.message||'Konu açılamadı.')}finally{setSending(false)}}
  async function removeMessage(){if(!deleting)return;const root=selectedThread||selectedTopic,wasReply=replies.some(item=>item.id===deleting.id);try{await api(`${base}/messages/${deleting.id}`,auth.accessToken,{method:'DELETE'});if(wasReply){setReplies(items=>items.filter(item=>item.id!==deleting.id));if(editingReply?.id===deleting.id)setEditingReply(null);if(root)setMessages(items=>items.map(item=>item.id===root.id?{...item,_count:{replies:Math.max(0,(item._count?.replies||1)-1)}}:item))}else setMessages(items=>items.filter(item=>item.id!==deleting.id));setDeleting(null)}catch(x:any){setError(x.message||'Mesaj silinemedi.');setDeleting(null)}}
  async function toggleReaction(message:Message,emoji:string){try{const reactions=await api<Reaction[]>(`${base}/messages/${message.id}/reactions`,auth.accessToken,{method:'POST',body:JSON.stringify({emoji})});const apply=(items:Message[])=>items.map(item=>item.id===message.id?{...item,reactions}:item);const pinned=feedPinned();setMessages(apply);setReplies(apply);setSelectedThread(current=>current&&current.id===message.id?{...current,reactions}:current);setSelectedTopic(current=>current&&current.id===message.id?{...current,reactions}:current);if(pinned)scrollFeedToBottom()}catch(x:any){setError(x.message||'Tepki verilemedi.')}}
  async function showHistory(message:Message){setHistoryMessage(message);setHistory([]);try{setHistory(await api<HistoryItem[]>(`${base}/messages/${message.id}/history`,auth.accessToken))}catch{setError('Düzenleme geçmişi yüklenemedi.')}}
  async function chooseForward(message:Message){setForwarding(message);try{const groups=await api<CategoryChoice[]>(`/organizations/${organization.id}/projects/${project.id}/channels`,auth.accessToken);setForwardChannels(groups.flatMap(group=>group.channels))}catch{setError('Kanallar yüklenemedi.')}}
  async function forwardTo(targetChannelId:string){if(!forwarding)return;try{const sent=await api<Message>(`${base}/messages/${forwarding.id}/forward`,auth.accessToken,{method:'POST',body:JSON.stringify({targetChannelId})});if(targetChannelId===channel?.id)setMessages(items=>upsertMessage(items,{...sent,_count:{replies:0}}));setForwarding(null)}catch(x:any){setError(x.message||'Mesaj iletilemedi.')}}
  async function openReplies(message:Message,forum:boolean){setSelectedTopic(forum?message:null);setSelectedThread(forum?null:message);if(forum)forumReplyAttachments.clear();setEditingReply(null);setQuotedReply(null);setReplies([]);setLoading(true);setError('');const path=forum?`${base}/topics/${message.id}/replies`:`${base}/messages/${message.id}/replies`;try{setReplies(await api<Message[]>(path,auth.accessToken))}catch{setError('Yanıtlar yüklenemedi.')}finally{setLoading(false)}}
  async function reply(raw:string,forum:boolean){const root=forum?selectedTopic:selectedThread;if(!root||!raw)return;if(editingReply){setSending(true);setError('');try{const{expandEmojiShortcodes}=await import('./emojiShortcodes');const updated=await api<Message>(`${base}/messages/${editingReply.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({content:expandEmojiShortcodes(raw)})});setReplies(items=>items.map(item=>item.id===updated.id?{...item,...updated}:item));setEditingReply(null)}catch(x:any){setError(x.message||'Yanıt düzenlenemedi.')}finally{setSending(false)}return}setSending(true);setError('');const path=forum?`${base}/topics/${root.id}/replies`:`${base}/messages/${root.id}/replies`;try{const{expandEmojiShortcodes}=await import('./emojiShortcodes');const content=expandEmojiShortcodes(raw);const created=await api<Message&{parentReplies?:number}>(path,auth.accessToken,{method:'POST',body:JSON.stringify({content,quotedMessageId:quotedReply?.id})});setQuotedReply(null);const{parentReplies,...message}=created;setReplies(items=>upsertMessage(items,message));setMessages(items=>items.map(item=>item.id===root.id?{...item,_count:{replies:parentReplies??((item._count?.replies||0)+1)},replies:[{createdAt:message.createdAt,author:{displayName:message.author.displayName}}]}:item))}catch(x:any){setError(x.message||'Yanıt gönderilemedi.')}finally{setSending(false)}}
  if(!channel)return <section className="channel-content-empty"/>
  const forum=channel.type==='FORUM',voice=channel.type==='VOICE'
  // Kanal bazlı yetkiler: yoksa kuruluş geneli geçerli sayılır.
  const may=(permission:string)=>perms?perms.canIn(channel.id,permission):true
  const canSend=may('message.send'),canReact=may('message.react')
  const canCall=may('call.start'),canConnect=may('voice.connect'),canManage=may('channel.manage'),canPermissions=may('channel.permissions')
  const headerText=channel.topic?.trim()||(forum?'Konular ve tartışmalar':voice?'Sesli oda ve yazılı sohbet':channel.type==='ANNOUNCEMENT'?'Proje duyuruları':'Ekip konuşmaları')
  const chatContent=(<div className="chat-layout" {...drop.handlers}><DropZone active={drop.dragging} label={canAttach?`#${channel.name} kanalına yüklenecek`:'Bu kanalda dosya paylaşamazsın'}/><div className="channel-chat-main"><div className="message-feed" ref={messageFeed}>{voice&&<div className="voice-call-card"><span><ChannelIcon type="VOICE"/></span><div><b>{voiceRoom&&voiceRoom.length?`Odada ${voiceRoom.length} kişi var`:'Sesli odaya katıl'}</b><small>{voiceRoom&&voiceRoom.length?voiceRoom.map(participant=>participant.displayName).join(', '):'Sesli sohbet, kamera ve ekran paylaşımı'}</small></div><button onClick={()=>onCallChannel?.(channel.id)} disabled={activeCallChannelId===channel.id||!canConnect}>{activeCallChannelId===channel.id?'Bağlısın':canConnect?'Katıl':'Yetkin yok'}</button></div>}{!voice&&voiceRoom&&voiceRoom.length>0&&activeCallChannelId!==channel.id&&<div className="voice-call-card ongoing"><span><ChannelIcon type="VOICE"/></span><div><b>Bu kanalda bir arama sürüyor</b><small>{voiceRoom.map(participant=>participant.displayName).join(', ')}</small></div><button onClick={()=>onCallChannel?.(channel.id)}>Katıl</button></div>}{!voice&&!loading&&!messages.length&&<div className="channel-welcome"><span><ChannelIcon type={channel.type}/></span><h1>#{channel.name} kanalına hoş geldin</h1><p>Bu kanalın başlangıcı burası.</p></div>}{loading&&!selectedThread?<p className="messages-state">Mesajlar yükleniyor…</p>:messages.map((message,index)=><div className="message-block" key={message.id}>{(index===0||new Date(messages[index-1].createdAt).toDateString()!==new Date(message.createdAt).toDateString())&&<DayDivider date={message.createdAt}/>}<ChannelMessage message={message} channelId={channel.id} own={message.author.id===auth.user.id} onThread={()=>openReplies(message,false)} onQuote={()=>{setQuoted(message);setEditing(null);requestAnimationFrame(()=>composerInput.current?.focus())}} onEdit={()=>{setEditing(message);setQuoted(null);setDraft(message.content)}} onForward={()=>chooseForward(message)} onDelete={()=>setDeleting(message)} onHistory={()=>showHistory(message)} onReact={emoji=>{if(canReact)void toggleReaction(message,emoji)}} latest={message.id===freshMessage} flash={message.id===flashId}/></div>)}</div><form ref={composerForm} className="message-composer slack-composer" onSubmit={send}>{quoted&&<div className="composer-reference"><span><b>{quoted.author.displayName}</b> adlı kullanıcıya yanıt veriliyor<small>{quoted.content}</small></span><button type="button" onClick={()=>setQuoted(null)}>×</button></div>}{editing&&<div className="composer-reference editing"><span><b>Mesaj düzenleniyor</b><small>Değişiklik geçmişe kaydedilecek</small></span><button type="button" onClick={()=>{setEditing(null);setDraft("")}}>×</button></div>}<EmojiSuggestions items={emojiItems} onPick={item=>setDraft(current=>applyEmojiSuggestion(current,item))} activeIndex={activeSuggestion} onHover={setSuggestionIndex}/><ComposerSuggestions mentions={mentionItems} commands={commandItems} onMention={pickMention} onCommand={pickCommand} activeIndex={activeSuggestion-emojiItems.length} onHover={index=>setSuggestionIndex(emojiItems.length+index)}/>{typists.length>0&&<p className="typing-line"><i/><i/><i/>{typingText(typists.map(item=>item.name))}</p>}<AttachmentQueue state={attachments}/><div className="composer-input-row">{canAttach&&<AttachmentPicker state={attachments}/>}<RichInput ref={composerInput} value={draft} onChange={value=>{setDraft(value.slice(0,4000));setSuggestionsClosed(false);setSuggestionIndex(0);if(value.trim())broadcast.typing();else broadcast.stop()}} onKeyDown={suggestionKeys} onEnter={()=>composerForm.current?.requestSubmit()} onStateChange={setFormat} onFiles={files=>void attachments.add(files)} placeholder={canSend?`#${channel.name} kanalına mesaj gönder`:'Bu kanalda yazma yetkin yok'} disabled={!canSend}/></div><div className="composer-toolbar"><span><button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={e=>e.preventDefault()} onClick={()=>wrapDraft('bold')}><ComposerIcon name="bold"/></button><button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={e=>e.preventDefault()} onClick={()=>wrapDraft('italic')}><ComposerIcon name="italic"/></button><button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={e=>e.preventDefault()} onClick={addLink}><ComposerIcon name="link"/></button><i/><button type="button" className={emojiOpen?"active":""} title="Emoji seç" onClick={()=>setEmojiOpen(open=>!open)}><ComposerIcon name="emoji"/></button><button type="button" title="Birini etiketle" onMouseDown={e=>e.preventDefault()} onClick={()=>composerInput.current?.insert('@')}><ComposerIcon name="mention"/></button></span><button className="composer-send" type="submit" disabled={!canSend||sending||attachments.busy||(!draft.trim()&&!attachments.files.length)}><ComposerIcon name="send"/></button></div>{emojiOpen&&<div className="emoji-picker-popover"><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel onSelect={emoji=>composerInput.current?.insert(emoji)}/></Suspense></div>}</form></div>{selectedThread&&(<aside className="thread-panel" style={{width:thread.width}}><ResizeHandle edge="left" onMouseDown={thread.start} label="Thread genişliği"/><header><div><b>Thread</b><small>#{channel.name}</small></div><button onClick={()=>{setSelectedThread(null);setReplies([])}}>×</button></header><RootMessage message={selectedThread} className="message slack-message thread-root" onReact={emoji=>toggleReaction(selectedThread,emoji)} actions={{own:selectedThread.author.id===auth.user.id,link:`p2://channels/${channel.id}/messages/${selectedThread.id}`,onQuote:()=>{setQuotedReply(selectedThread);setEditingReply(null)},onEdit:()=>setEditing(selectedThread),onDelete:()=>setDeleting(selectedThread),onForward:()=>chooseForward(selectedThread),onHistory:()=>showHistory(selectedThread)}}/><div className="thread-divider"><span>{replies.length?`${replies.length} yanıt`:'Henüz yanıt yok'}</span></div><ReplyList divider={false} replies={replies} loading={loading} channelId={channel.id} userId={auth.user.id} onEdit={setEditingReply} onForward={chooseForward} onDelete={setDeleting} onHistory={showHistory} onReact={toggleReaction} onQuote={message=>{setQuotedReply(message);setEditingReply(null)}} flashId={flashId}/><ReplyComposer token={auth.accessToken} sending={sending} placeholder="Thread'e yanıt yaz…" onSubmit={content=>reply(content,false)} editing={editingReply} onCancelEdit={()=>setEditingReply(null)} quoted={quotedReply} onCancelQuote={()=>setQuotedReply(null)} mentions={members}/></aside>)}</div>)
  return <MemberContext.Provider value={members}><ProfileContext.Provider value={user=>setProfileUser({...user,...members.find(item=>item.id===user.id),...(user.id===auth.user.id?auth.user:{})})}><MentionContext.Provider value={openMentionedProfile}><section className={forum?'channel-content forum-view':'channel-content'}><header><button type="button" className={starred?'tool star-inline starred':'tool star-inline'} title={starred?'Yıldızı kaldır':'Yıldıza ekle'} aria-label="Yıldıza ekle" onClick={()=>onPreference?.({starred:!starred})}><ComposerIcon name="star"/></button><ChannelIcon type={channel.type}/><div><h2>{channel.name}</h2><p>{headerText}</p></div><nav className="channel-tools">
      {voice&&<button type="button" className={voiceChatOpen?'tool voice-chat-toggle active':'tool voice-chat-toggle'} title={voiceChatOpen?'Sohbeti gizle':'Sohbeti göster'} aria-label={voiceChatOpen?'Sohbeti gizle':'Sohbeti göster'} onClick={()=>setVoiceChatOpen(open=>!open)}><ComposerIcon name="thread"/><span>{voiceChatOpen?'Sohbeti gizle':'Sohbeti göster'}</span></button>}
      <button type="button" className={toolMenu?.kind==='members'?'tool facepile active':'tool facepile'} title="Üyeler" aria-label="Üyeler" onClick={e=>{const box=e.currentTarget.getBoundingClientRect();setToolMenu(toolMenu?.kind==='members'?null:{kind:'members',menu:{x:box.right,y:box.bottom+6,up:box.top-6,right:true}})}}>{members.slice(0,3).map(member=><span key={member.id} className={avatarHue(member.id)}>{member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}</span>)}<b>{members.length}</b></button>
      <button type="button" className={activeCallChannelId===channel.id?'tool call-active':'tool'} title={activeCallChannelId===channel.id?'Aramadasın':'Bu kanalda arama başlat'} aria-label="Arama başlat" disabled={voice?!canConnect:!canCall} onClick={()=>onCallChannel?.(channel.id,channel.type!=='VOICE')}><ComposerIcon name="call"/></button>
      <button type="button" className={searchOpen?'tool search-toggle active':'tool search-toggle'} title="Kanalda ara" aria-label="Kanalda ara" onClick={()=>{setSearchOpen(open=>!open);setToolMenu(null);setProfileUser(null)}}><ComposerIcon name="search"/></button>
      <button type="button" className={`tool${notify==='MUTED'?' muted':''}${toolMenu?.kind==='notify'?' active':''}`} title="Bildirim tercihleri" aria-label="Bildirim tercihleri" onClick={e=>{const box=e.currentTarget.getBoundingClientRect();setToolMenu({kind:'notify',menu:{x:box.right,y:box.bottom+6,up:box.top-6,right:true}})}}><ComposerIcon name="bell"/></button>
      {splitPane&&<button type="button" className="tool" title="Bölünmüş görünümü kapat" aria-label="Bölünmüş görünümü kapat" onClick={onCloseSplit}>✕</button>}
      <button type="button" className={toolMenu?.kind==='more'?'tool more active':'tool more'} title="Diğer işlemler" aria-label="Diğer işlemler" onClick={e=>{const box=e.currentTarget.getBoundingClientRect();setToolMenu(toolMenu?.kind==='more'?null:{kind:'more',menu:{x:box.right,y:box.bottom+6,up:box.top-6,right:true}})}}><ComposerIcon name="more"/></button>
    </nav></header>{toolMenu&&<MessageMenuPopup menu={toolMenu.menu} onClose={()=>setToolMenu(null)} className={toolMenu.kind==='members'?'message-context-menu members-popover':'message-context-menu tool-menu'}>{
      toolMenu.kind==='members'?<MembersPopover members={members} meId={auth.user.id} onSelect={user=>{setProfileUser(user);setToolMenu(null)}}/>
      :toolMenu.kind==='notify'?<>
      <p className="menu-title">Sana neler bildirilsin?</p>
      <button onClick={()=>{onPreference?.({notify:'ALL'});setToolMenu(null)}}><span>{notify==='ALL'?'✓':''}</span><div><b>Tüm yeni mesajlar</b><small>Mesajlar ve takip ettiğin thread'ler</small></div></button>
      <button onClick={()=>{onPreference?.({notify:'MENTIONS'});setToolMenu(null)}}><span>{notify==='MENTIONS'?'✓':''}</span><div><b>Yalnızca etiketler</b><small>@sen, @here, @everyone</small></div></button>
      <button onClick={()=>{onPreference?.({notify:'MUTED'});setToolMenu(null)}}><span>{notify==='MUTED'?'✓':''}</span><div><b>Sessize al</b><small>Bildirim gelmez, kanal sessizleşir</small></div></button>
      <hr/>
      <button className="menu-coming"><span>⚙</span><div><b>Daha fazla seçenek</b><small>yakında</small></div></button>
    </>:<>
      <button onClick={()=>{setToolMenu(null);setDetailsOpen(true)}}><span>ⓘ</span>Kanal detayları</button>
      <button onClick={()=>{void navigator.clipboard.writeText(`p2://channels/${channel.id}`);setToolMenu(null)}}><span>🔗</span>Kanal bağlantısını kopyala</button>
      <button onClick={()=>{onPreference?.({starred:!starred});setToolMenu(null)}}><span>★</span>{starred?'Yıldızı kaldır':'Yıldıza ekle'}</button>
      <button onClick={()=>{setSearchOpen(true);setToolMenu(null)}}><span>⌕</span>Kanalda ara</button>
      <hr/>
      <button disabled={!canManage} onClick={()=>{setToolMenu(null);setChannelSettings(true)}}><span>✎</span>Ayarları düzenle</button>
      <button disabled={!canPermissions} onClick={()=>{setToolMenu(null);setChannelPerms(true)}}><span>🔐</span>Kanal izinleri</button>
      <button disabled={!onSplit||splitPane} onClick={()=>{setToolMenu(null);onSplit?.(channel)}}><span>◫</span>Bölünmüş görünümde aç</button>
      <button onClick={()=>{setToolMenu(null);openConversationWindow('channel',channel.id,channel.name,{org:organization.id,project:project.id})}}><span>⧉</span>Yeni pencerede aç</button>
      <hr/>
      <button className="danger" onClick={()=>{setToolMenu(null);setLeaveOpen(true)}}><span>⤳</span>Kanaldan ayrıl</button>
    </>}</MessageMenuPopup>}{forum?(<div className="forum-layout" {...drop.handlers}><DropZone active={drop.dragging} label={selectedTopic?'Bu konuya yanıt olarak eklenecek':`#${channel.name} forumunda yeni konuya eklenecek`}/>{selectedTopic?<div className="forum-topic-view"><button className="forum-back" onClick={()=>{setSelectedTopic(null);setReplies([])}}>← Konulara dön</button><RootMessage message={selectedTopic} className="forum-topic-head" onReact={emoji=>toggleReaction(selectedTopic,emoji)}/><ReplyList replies={replies} loading={loading} channelId={channel.id} userId={auth.user.id} onEdit={setEditingReply} onForward={chooseForward} onDelete={setDeleting} onHistory={showHistory} onReact={toggleReaction} onQuote={message=>{setQuotedReply(message);setEditingReply(null)}} flashId={flashId}/><ReplyComposer token={auth.accessToken} sending={sending} placeholder="Bu konuya yanıt yaz…" onSubmit={content=>reply(content,true)} attachmentState={forumReplyAttachments} editing={editingReply} onCancelEdit={()=>setEditingReply(null)} quoted={quotedReply} onCancelQuote={()=>setQuotedReply(null)}/></div>:<div className="forum-feed">{!loading&&!messages.length&&<div className="forum-intro"><h1>{channel.name}</h1><p>Fikirleri ve geri bildirimleri ayrı başlıklar altında düzenle.</p></div>}{loading?<p className="messages-state">Konular yükleniyor…</p>:messages.map(message=><article className="forum-topic-card" key={message.id} onClick={()=>openReplies(message,true)} tabIndex={0} onKeyDown={e=>{if(e.key==='Enter')openReplies(message,true)}}><Avatar message={message}/><div><h3>{forumTopicTitle(message.content)}</h3><small>{message.author.displayName} · {new Date(message.createdAt).toLocaleString('tr-TR')}{attachmentsFromContent(message.content).length?` · ${attachmentsFromContent(message.content).length} ek`:''}</small></div><b>{message._count?.replies||0} yanıt</b></article>)}{!loading&&!messages.length&&<p className="messages-state">Henüz konu açılmadı.</p>}<form className="forum-topic-composer" onSubmit={sendTopic}><AttachmentQueue state={topicAttachments}/><div><AttachmentPicker state={topicAttachments}/><input name="content" maxLength={4000} value={topicTitle} onChange={event=>setTopicTitle(event.target.value)} placeholder={topicAttachments.files.length?`Başlık boşsa “${attachmentTitle(topicAttachments.files[0])}” kullanılır`:'Yeni bir konu başlığı yaz…'}/><button disabled={!canSend||sending||topicAttachments.busy||(!topicTitle.trim()&&!topicAttachments.files.length)}>{sending?'…':'Konu aç'}</button></div></form></div>}</div>):voice?<div className={voiceChatOpen?'voice-room-shell chat-open':'voice-room-shell'}><VoiceRoomStage name={channel.name} participants={voiceRoom||[]} members={members} me={auth.user.id} levels={callApi.levels} joined={activeCallChannelId===channel.id} onJoin={()=>onCallChannel?.(channel.id)} onProfile={setProfileUser}/>{voiceChatOpen&&<aside className="voice-room-chat" style={{width:voiceChat.width}}><ResizeHandle edge="left" onMouseDown={voiceChat.start} label="Oda sohbeti genişliği"/><header><div><b>Oda sohbeti</b><small>#${channel.name}</small></div><button type="button" onClick={()=>setVoiceChatOpen(false)}>×</button></header>{chatContent}</aside>}</div>:chatContent}{forwarding&&<div className="action-modal-layer" onMouseDown={()=>setForwarding(null)}><section className="message-action-dialog" onMouseDown={e=>e.stopPropagation()}><header><div><small>MESAJI İLET</small><h3>Bir kanal seç</h3></div><button onClick={()=>setForwarding(null)}>×</button></header><div className="forward-channel-list">{forwardChannels.filter(item=>item.type!=="VOICE"||true).map(item=><button key={item.id} onClick={()=>forwardTo(item.id)}><ChannelIcon type={item.type}/><span>{item.name}</span></button>)}</div></section></div>}{deleting&&<div className="action-modal-layer" onMouseDown={()=>setDeleting(null)}><section className="message-action-dialog compact" onMouseDown={e=>e.stopPropagation()}><h3>Mesaj silinsin mi?</h3><p>Mesajlar yalnızca gönderildikten sonraki 2 saat içinde silinebilir.</p><footer><button onClick={()=>setDeleting(null)}>Vazgeç</button><button className="danger" onClick={removeMessage}>Mesajı sil</button></footer></section></div>}{historyMessage&&<div className="action-modal-layer" onMouseDown={()=>setHistoryMessage(null)}><section className="message-action-dialog" onMouseDown={e=>e.stopPropagation()}><header><div><small>DÜZENLEME GEÇMİŞİ</small><h3>Eski sürümler</h3></div><button onClick={()=>setHistoryMessage(null)}>×</button></header><div className="history-list">{history.map(item=><article key={item.id}><small>{new Date(item.createdAt).toLocaleString("tr-TR")}</small><p>{item.content}</p></article>)}{!history.length&&<p>Önceki sürüm bulunamadı.</p>}</div></section></div>}{sidePanel}{detailsOpen&&<div className="action-modal-layer" onMouseDown={()=>setDetailsOpen(false)}><section className="message-action-dialog" onMouseDown={e=>e.stopPropagation()}><header><div><small>KANAL DETAYLARI</small><h3>#{channel.name}</h3></div><button onClick={()=>setDetailsOpen(false)}>×</button></header><div className="channel-details">
      <div><small>Tür</small><b>{channel.type==='TEXT'?'Metin kanalı':channel.type==='ANNOUNCEMENT'?'Duyuru kanalı':channel.type==='VOICE'?'Sesli oda':'Forum'}</b></div>
      <div><small>Kanal kimliği</small><b>{channel.id.slice(0,8)}</b></div>
      <div><small>Üye</small><b>{members.length} kişi</b></div>
      <div><small>Bildirim</small><b>{notify==='ALL'?'Tüm mesajlar':notify==='MUTED'?'Sessize alındı':'Yalnızca etiketler'}</b></div>
      <div><small>Yıldız</small><b>{starred?'Yıldızlı':'Yıldızsız'}</b></div>
    </div></section></div>}{channelPerms&&<ChannelPermissionsDialog auth={auth} organization={organization} project={project} channel={channel} onClose={()=>setChannelPerms(false)}/>}{channelSettings&&<ChannelSettingsDialog auth={auth} organization={organization} project={project} channel={channel} onClose={()=>setChannelSettings(false)} onSaved={updated=>{setChannelSettings(false);onChannelUpdated?.(updated)}}/>}{leaveOpen&&<div className="action-modal-layer" onMouseDown={()=>setLeaveOpen(false)}><section className="message-action-dialog compact" onMouseDown={e=>e.stopPropagation()}><h3>#{channel.name} kanalından ayrılasın mı?</h3><p>Kanal kenar çubuğundan kaldırılır ve bildirim almazsın. Proje menüsündeki “Kanallara göz at” ile istediğin zaman geri dönebilirsin.</p><footer><button onClick={()=>setLeaveOpen(false)}>Vazgeç</button><button className="danger" onClick={()=>{setLeaveOpen(false);onLeave?.(channel)}}>Kanaldan ayrıl</button></footer></section></div>}{error&&<div className="channel-content-error">{error}</div>}</section></MentionContext.Provider></ProfileContext.Provider></MemberContext.Provider>
}

function ThreadReply({message,channelId,own,flash,onQuote,onEdit,onForward,onDelete,onHistory,onReact}:{message:Message;channelId:string;own:boolean;flash?:boolean;onQuote:()=>void;onEdit:()=>void;onForward:()=>void;onDelete:()=>void;onHistory:()=>void;onReact:(emoji:string)=>void}){
  const tools=useMessageTools()
  return <article id={`msg-${message.id}`} className={flash?'message slack-message thread-reply flash':'message slack-message thread-reply'} onContextMenu={e=>{e.preventDefault();tools.openAtPoint(e.clientX,e.clientY)}}>
    <Avatar message={message}/>
    <div className="slack-message-content"><MessageBody message={message}/><ReactionBar reactions={message.reactions||[]} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/></div>
    <MessageTools tools={tools} actions={{own,content:message.content,editedAt:message.editedAt,link:`p2://channels/${channelId}/messages/${message.id}`,onReact,onQuote,onEdit,onDelete,onForward,onHistory}}/>
  </article>
}
function ReplyList({replies,loading,channelId,userId,divider=true,onEdit,onForward,onDelete,onHistory,onReact,onQuote,flashId}:{replies:Message[];loading:boolean;channelId:string;userId:string;divider?:boolean;onEdit:(message:Message)=>void;onForward:(message:Message)=>void;onDelete:(message:Message)=>void;onHistory:(message:Message)=>void;onReact:(message:Message,emoji:string)=>void;onQuote:(message:Message)=>void;flashId?:string|null}){const feed=useRef<HTMLDivElement>(null),reactionRows=replies.map(item=>(item.reactions||[]).length).join('|');useStickyScroll(feed,[replies.length]);useEffect(()=>{const node=feed.current;if(!node||node.scrollHeight-node.scrollTop-node.clientHeight>150)return;requestAnimationFrame(()=>{if(feed.current)feed.current.scrollTop=feed.current.scrollHeight})},[reactionRows]);return <div className="forum-replies" ref={feed}>{divider&&<div className="reply-divider"><span>{replies.length} yanıt</span></div>}{loading?<p className="messages-state">Yanıtlar yükleniyor…</p>:replies.map(message=><ThreadReply key={message.id} message={message} channelId={channelId} own={message.author.id===userId} flash={message.id===flashId} onQuote={()=>onQuote(message)} onEdit={()=>onEdit(message)} onForward={()=>onForward(message)} onDelete={()=>onDelete(message)} onHistory={()=>onHistory(message)} onReact={emoji=>onReact(message,emoji)}/>)}{!loading&&!replies.length&&<p className="messages-state">Henüz yanıt yok. İlk yanıtı sen yaz.</p>}</div>}
export type ComposerQuote={content:string;author:{displayName:string}}
export function ReplyComposer({token,sending,placeholder,onSubmit,editing,onCancelEdit,quoted,onCancelQuote,mentions=[],attachmentState}:{token:string;sending:boolean;placeholder:string;onSubmit:(content:string)=>void|Promise<void>;editing:ComposerQuote|null;onCancelEdit:()=>void;quoted:ComposerQuote|null;onCancelQuote:()=>void;mentions?:{username:string;displayName:string}[];attachmentState?:ReturnType<typeof useAttachments>}){
  const[value,setValue]=useState(''),[emojiOpen,setEmojiOpen]=useState(false),[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const localAttachments=useAttachments(token),attachments=attachmentState||localAttachments
  const editor=useRef<RichHandle>(null),form=useRef<HTMLFormElement>(null),suggestions=useEmojiSuggestions(value)
  const[mentionIndex,setMentionIndex]=useState(0),[mentionsClosed,setMentionsClosed]=useState(false)
  // Thread içinde de etiketleme: yalnızca bu kanalın/grubun kişileri önerilir.
  const mentionMatch=value.match(/@([a-zA-Z0-9_.-]*)$/)
  const mentionNeedle=(mentionMatch?.[1]||'').toLocaleLowerCase('tr-TR')
  const mentionItems:MentionSuggestion[]=mentionMatch&&!mentionsClosed?mentions
    .filter(person=>person.username.toLocaleLowerCase('tr-TR').startsWith(mentionNeedle))
    .map(person=>({label:`@${person.username}`,description:person.displayName,value:person.username,avatar:userInitials(person.displayName)})).slice(0,8):[]
  const activeMention=mentionItems.length?Math.min(mentionIndex,mentionItems.length-1):-1
  function pickMention(item:MentionSuggestion){setValue(current=>current.replace(/@([a-zA-Z0-9_.-]*)$/,`@${item.value} `));setMentionIndex(0)}
  useEffect(()=>{if(editing)setValue(editing.content)},[editing])
  useEscape(emojiOpen,()=>setEmojiOpen(false))
  return <form ref={form} className="message-composer slack-composer reply-composer" onSubmit={async e=>{e.preventDefault();const content=attachments.appendTo(value);if(!content)return;await onSubmit(content);setValue('');attachments.clear()}}>
    {quoted&&!editing&&<div className="composer-reference"><span><b>{quoted.author.displayName}</b> adlı kullanıcıya yanıt veriliyor<small>{quoted.content}</small></span><button type="button" onClick={onCancelQuote}>×</button></div>}
    {editing&&<div className="composer-reference editing"><span><b>Yanıt düzenleniyor</b><small>Değişiklik geçmişe kaydedilecek</small></span><button type="button" onClick={()=>{setValue('');onCancelEdit()}}>×</button></div>}
    <ComposerSuggestions mentions={mentionItems} commands={[]} onMention={pickMention} onCommand={()=>{}} activeIndex={activeMention} onHover={setMentionIndex}/>
    <EmojiSuggestions items={suggestions} onPick={item=>setValue(current=>applyEmojiSuggestion(current,item))}/>
    <AttachmentQueue state={attachments}/><div className="composer-input-row"><AttachmentPicker state={attachments}/><RichInput ref={editor} value={value} onChange={next=>{setValue(next.slice(0,4000));setMentionsClosed(false);setMentionIndex(0)}} onKeyDown={event=>{
      if(!mentionItems.length)return
      if(event.key==='ArrowDown'){event.preventDefault();setMentionIndex(current=>(Math.min(current,mentionItems.length-1)+1)%mentionItems.length)}
      else if(event.key==='ArrowUp'){event.preventDefault();setMentionIndex(current=>(Math.min(current,mentionItems.length-1)-1+mentionItems.length)%mentionItems.length)}
      else if(event.key==='Enter'||event.key==='Tab'){event.preventDefault();const item=mentionItems[activeMention];if(item)pickMention(item)}
      else if(event.key==='Escape'){event.preventDefault();event.stopPropagation();setMentionsClosed(true)}
    }} onEnter={()=>form.current?.requestSubmit()} onStateChange={setFormat} onFiles={files=>void attachments.add(files)} placeholder={placeholder}/></div>
    <div className="composer-toolbar"><span>
      <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={e=>e.preventDefault()} onClick={()=>editor.current?.format('bold')}><ComposerIcon name="bold"/></button>
      <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={e=>e.preventDefault()} onClick={()=>editor.current?.format('italic')}><ComposerIcon name="italic"/></button>
      <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={e=>e.preventDefault()} onClick={()=>editor.current?.openLink()}><ComposerIcon name="link"/></button><i/>
      <button type="button" className={`emoji-toggle${emojiOpen?' active':''}`} title="Emoji seç" onMouseDown={e=>e.preventDefault()} onClick={()=>setEmojiOpen(open=>!open)}><ComposerIcon name="emoji"/></button>
      <button type="button" title="Birini etiketle" onMouseDown={e=>e.preventDefault()} onClick={()=>editor.current?.insert('@')}><ComposerIcon name="mention"/></button>
    </span><button className="composer-send" type="submit" disabled={sending}><ComposerIcon name="send"/></button></div>
    {emojiOpen&&<div className="emoji-picker-popover reply-picker"><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel onDismiss={()=>setEmojiOpen(false)} onSelect={emoji=>editor.current?.insert(emoji)}/></Suspense></div>}
  </form>
}
