import { FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import type { Organization } from './App'
import { API_BASE } from './apiBase'
import ChannelContent from './ChannelContent'
import ChannelBrowser from './ChannelBrowser'
import AdminPanel from './AdminPanel'
import GitPanel from './GitPanel'
import { useMyPermissions, type PermissionApi } from './permissions'
import { popoutTarget } from './popout'
import CallOverlay from './CallOverlay'
import { CallProvider, useCall, type CallParticipant } from './CallProvider'
import { CallIcon } from './CallIcons'
import ChannelIcon from './ChannelIcons'
import DirectConversation from './DirectMessages'
import GroupChat from './GroupChat'
import GroupCreator from './GroupCreator'
import ConfirmDialog from './ConfirmDialog'
import ActivityPanel, { type ActivityItem } from './ActivityPanel'
import { ComposerIcon } from './ComposerIcons'
import type { ChatGroup, GroupMessage } from './groupTypes'
import { publishGroupReactions, publishGroupReply } from './groupBus'
import { playMessageSound, showMessageNotification } from './notifications'
import { emitRealtime } from './realtime'
import { getSettings, subscribeSettings, updateSettings, useSettings } from './settings'
import { presenceClass, statusLabel, type ProfileMember, type ProfileUser } from './ProfilePanel'
import SettingsModal from './SettingsModal'
import { useRealtimeEvents } from './realtime'
import { useEscape } from './useEscape'
import { ResizeHandle, useResizable } from './useResizable'
import { useTypists } from './useTyping'
import { avatarHue, userInitials } from './initials'


type Auth={user:{id:string;username:string;displayName:string;email?:string;title?:string|null;avatarUrl?:string|null};accessToken:string}
type Project={id:string;name:string;slug:string;description?:string|null;engine?:string|null;ownerId:string;createdAt:string;updatedAt:string}
type Friend={id:string;displayName:string;status:string}
type Channel={id:string;name:string;topic?:string|null;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';position:number}
// Ana alanda ve bölünmüş görünümde gösterilebilecek sohbetler.
type PaneTarget={kind:'channel';channel:Channel|null}|{kind:'person';person:ProfileMember}|{kind:'group';group:ChatGroup}
type ChannelCategory={id:string;name:string;position:number;channels:Channel[]}
const API=API_BASE
async function request<T>(path:string,token:string,init?:RequestInit):Promise<T>{const response=await fetch(API+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}});const data=await response.json().catch(()=>({}));if(!response.ok)throw data;return data}
const AWAY_DELAY=3*60*1000
// Mesajda benden söz ediliyor mu? (@kullanıcıadı, @here, @everyone)
function mentionsMe(content:string,username:string){
  const tokens=content.match(/@([a-zA-Z0-9_.]+)/g)||[]
  const mine=username.toLocaleLowerCase('tr-TR')
  return tokens.some(token=>{const value=token.slice(1).toLocaleLowerCase('tr-TR');return value===mine||value==='here'||value==='everyone'})
}
const presenceOptions=[{value:'ONLINE',label:'Çevrimiçi'},{value:'AWAY',label:'Uzakta'},{value:'BUSY',label:'Meşgul'},{value:'INVISIBLE',label:'Görünmez'}]
const engineNames:Record<string,string>={UNREAL:'Unreal Engine',UNITY:'Unity',GODOT:'Godot',CUSTOM:'Özel motor'}
const projectInitials=(name:string)=>name.trim().split(/\s+/).filter(Boolean).slice(0,3).map(word=>word[0]).join('').toLocaleUpperCase('tr-TR')||'P'

function ProjectCreator({auth,organization,onClose,onCreated}:{auth:Auth;organization:Organization;onClose:()=>void;onCreated:(project:Project)=>void}){
  useEscape(true,onClose)
  const[name,setName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
  const slug=useMemo(()=>name.toLocaleLowerCase('tr-TR').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/ı/g,'i').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'').slice(0,40),[name])
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const values=Object.fromEntries(new FormData(e.currentTarget));try{onCreated(await request<Project>(`/organizations/${organization.id}/projects`,auth.accessToken,{method:'POST',body:JSON.stringify({name,slug,description:values.description})}))}catch(x:any){setError(x.message||'Proje oluşturulamadı.');setBusy(false)}}
  return <div className="project-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="project-modal"><header><div><p className="eyebrow">YENİ PROJE</p><h2>Projenin temelini atalım.</h2><p>Bu bilgileri daha sonra proje ayarlarından değiştirebilirsin.</p></div><button className="modal-close" onClick={onClose}>×</button></header><form onSubmit={submit}><label>Proje adı<input value={name} onChange={e=>setName(e.target.value)} minLength={2} maxLength={60} placeholder="Örn. Project Aurora" autoFocus required/></label><label>Proje kodu<div className="slug-preview"><span>{organization.slug}/</span>{slug||'proje-kodu'}</div></label><label>Kısa açıklama<textarea name="description" maxLength={240} rows={3} placeholder="Ekibine projenin amacını kısaca anlat…"/></label>{error&&<div className="error">{error}</div>}<footer><button type="button" onClick={onClose}>Vazgeç</button><button className="create-project-primary" disabled={busy||slug.length<2}>{busy?'Proje hazırlanıyor…':'Projeyi oluştur'} <span>→</span></button></footer></form></section></div>
}

const channelIcons:Record<Channel['type'],React.ReactNode>={TEXT:<ChannelIcon type="TEXT"/>,ANNOUNCEMENT:<ChannelIcon type="ANNOUNCEMENT"/>,VOICE:<ChannelIcon type="VOICE"/>,FORUM:<ChannelIcon type="FORUM"/>}
const channelTypeOptions:Array<{value:Channel['type'];label:string;description:string}>=[
  {value:'TEXT',label:'Metin',description:'Mesajlar ve günlük konuşmalar'},
  {value:'ANNOUNCEMENT',label:'Duyuru',description:'Önemli güncellemeler'},
  {value:'VOICE',label:'Ses',description:'Canlı ekip görüşmeleri'},
  {value:'FORUM',label:'Forum',description:'Başlıklar altında tartışmalar'},
]

function VoiceMembers({participants,people,self,levels,onProfile}:{participants:CallParticipant[];people:ProfileMember[];self:ProfileMember;levels:Record<string,number>;onProfile:(person:ProfileMember)=>void}){
  const call=useCall(),settings=useSettings(),me=self.id
  const[menu,setMenu]=useState<{participant:CallParticipant;x:number;y:number}|null>(null)
  useEscape(Boolean(menu),()=>setMenu(null))
  return <div className="voice-members">{participants.map(participant=>{
    const person=people.find(item=>item.id===participant.userId)||(participant.userId===me?self:undefined)
    const level=participant.muted?0:(levels[participant.userId]||0)
    const localMuted=call.locallyMuted.has(participant.userId)
    return <button type="button" className={`voice-member${level>.08?' speaking':''}${localMuted?' locally-muted':''}`} key={participant.userId} style={{'--voice-level':level} as React.CSSProperties} onClick={()=>{if(person)onProfile(person)}} onContextMenu={event=>{event.preventDefault();setMenu({participant,x:event.clientX,y:event.clientY})}} title={person?`${participant.displayName} profilini aç`:participant.displayName}>
      <span className={`voice-member-avatar ${avatarHue(participant.userId)}`}>{participant.avatarUrl||person?.avatarUrl?<img src={participant.avatarUrl||person!.avatarUrl!} alt=""/>:userInitials(participant.displayName)}</span>
      <span className="voice-member-name">{participant.displayName}{participant.userId===me&&<small>SEN</small>}{participant.screen&&<small className="live-badge">YAYINDA</small>}</span>
      {(participant.muted||localMuted||participant.deafened)&&<span className="voice-member-muted" title={participant.deafened?'Mikrofonu ve sesi kapalı':localMuted?'Senin için sessizde':'Mikrofonu kapalı'}><CallIcon name="micOff"/>{participant.deafened&&<CallIcon name="headphonesOff"/>}</span>}
    </button>
  })}{menu&&<div className="channel-context-layer" onMouseDown={()=>setMenu(null)}><div className="channel-context-menu person-context-menu voice-member-menu" style={{left:Math.min(menu.x,window.innerWidth-250),top:Math.max(8,Math.min(menu.y,window.innerHeight-246))}} onMouseDown={event=>event.stopPropagation()}>
    <button className="plain profile-entry" onClick={()=>{onProfile(people.find(item=>item.id===menu.participant.userId)||self);setMenu(null)}}>Profil</button>
    {menu.participant.userId!==me&&menu.participant.screen&&<button className="plain profile-entry" onClick={()=>{call.watch(menu.participant.userId);setMenu(null)}}>{call.watching.includes(menu.participant.userId)?'Yayını kapat':'Yayını izle'}</button>}
    <hr/>
    {menu.participant.userId===me?<>
      <label className="voice-volume-control"><span>Mikrofon sesi</span><b>{settings.inputVolume}%</b><input type="range" min="0" max="200" value={settings.inputVolume} onChange={event=>updateSettings({inputVolume:Number(event.target.value)})}/></label>
      <button className="check-entry" onClick={call.toggleMute}><span>{call.muted?'Kendimi susturmayı kaldır':'Kendimi sustur'}</span><i className={call.muted?'checked':''}/></button>
      <button className="check-entry" onClick={call.toggleDeafen}><span>{call.deafened?'Sağırlaştırmayı kaldır':'Kendimi sağırlaştır'}</span><i className={call.deafened?'checked':''}/></button>
    </>:<>
      <label className="voice-volume-control"><span>Mikrofon sesi</span><b>{call.userVolumes[menu.participant.userId]??100}%</b><input type="range" min="0" max="200" disabled={call.locallyMuted.has(menu.participant.userId)} value={call.userVolumes[menu.participant.userId]??100} onChange={event=>call.setUserVolume(menu.participant.userId,Number(event.target.value))}/></label>
      <button className="check-entry" onClick={()=>call.toggleLocalMute(menu.participant.userId)}><span>Sustur</span><i className={call.locallyMuted.has(menu.participant.userId)?'checked':''}/></button>
      <button className="check-entry" onClick={()=>call.togglePeerDeafen(menu.participant.userId)}><span>Sağırlaştır</span><i className={call.deafenedFor.has(menu.participant.userId)?'checked':''}/></button>
    </>}
  </div></div>}</div>
}

function ChannelCreator({auth,organization,project,category,onClose,onCreated}:{auth:Auth;organization:Organization;project:Project;category:ChannelCategory;onClose:()=>void;onCreated:(channel:Channel)=>void}){
  useEscape(true,onClose)
  const[name,setName]=useState(''),[type,setType]=useState<Channel['type']>('TEXT'),[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');try{const channel=await request<Channel>(`/organizations/${organization.id}/projects/${project.id}/channels`,auth.accessToken,{method:'POST',body:JSON.stringify({categoryId:category.id,name,type})});onCreated(channel)}catch(x:any){setError(x.message||'Kanal oluşturulamadı.');setBusy(false)}}
  return <div className="channel-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="channel-modal"><header><div><small>{category.name}</small><h2>Kanal oluştur</h2></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header><form onSubmit={submit}><label>Kanal adı<div className="channel-name-input"><span>#</span><input value={name} onChange={e=>setName(e.target.value)} minLength={1} maxLength={40} placeholder="yeni-kanal" autoFocus required/></div></label><fieldset><legend>Kanal türü</legend>{channelTypeOptions.map(option=><button type="button" key={option.value} className={type===option.value?'active':''} onClick={()=>setType(option.value)}><span>{channelIcons[option.value]}</span><div><b>{option.label}</b><small>{option.description}</small></div><i/></button>)}</fieldset>{error&&<div className="channel-create-error">{error}</div>}<footer><button type="button" onClick={onClose}>Vazgeç</button><button className="channel-create-primary" disabled={busy||!name.trim()}>{busy?'Oluşturuluyor…':'Kanalı oluştur'}</button></footer></form></section></div>
}

function DeleteChannelDialog({auth,organization,project,channel,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;channel:Channel;onClose:()=>void;onDeleted:()=>void}){
  useEscape(true,onClose)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}/channels/${channel.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Kanal silinemedi.');setBusy(false)}}
  return <div className="channel-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="delete-channel-dialog"><span>!</span><h2>#{channel.name} silinsin mi?</h2><p>Kanal hemen çalışma alanından kaldırılacak. Mesajlar 30 gün boyunca saklanacak ve ardından geri döndürülemez biçimde kalıcı olarak silinecek.</p>{error&&<div className="channel-create-error">{error}</div>}<footer><button onClick={onClose}>Vazgeç</button><button className="delete-channel-primary" onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Kanalı sil'}</button></footer></section></div>
}

function DeleteCategoryDialog({auth,organization,project,category,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;category:ChannelCategory;onClose:()=>void;onDeleted:()=>void}){
  useEscape(true,onClose)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}/channel-categories/${category.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Kategori silinemedi.');setBusy(false)}}
  return <div className="channel-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="delete-channel-dialog"><span>!</span><h2>{category.name} kategorisi silinsin mi?</h2><p>Kategori çalışma alanından kaldırılacak. Daha önce silinen kanallara ait veriler 30 günlük saklama süresi tamamlanana kadar korunacak.</p>{error&&<div className="channel-create-error">{error}</div>}<footer><button onClick={onClose}>Vazgeç</button><button className="delete-channel-primary" onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Kategoriyi sil'}</button></footer></section></div>
}

function CategoryCreator({auth,organization,project,onClose,onCreated}:{auth:Auth;organization:Organization;project:Project;onClose:()=>void;onCreated:(category:ChannelCategory)=>void}){
  useEscape(true,onClose)
  const[name,setName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');try{onCreated(await request<ChannelCategory>(`/organizations/${organization.id}/projects/${project.id}/channel-categories`,auth.accessToken,{method:'POST',body:JSON.stringify({name})}))}catch(x:any){setError(x.message||'Kategori oluşturulamadı.');setBusy(false)}}
  return <div className="channel-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="category-create-dialog"><header><div><small>YENİ KATEGORİ</small><h2>Kanallarını grupla</h2></div><button onClick={onClose} aria-label="Kapat">×</button></header><form onSubmit={submit}><label>Kategori adı<input value={name} onChange={e=>setName(e.target.value)} minLength={1} maxLength={32} placeholder="Örn. 🎨 Tasarım & Fikirler" autoFocus required/></label>{error&&<div className="channel-create-error">{error}</div>}<footer><button type="button" onClick={onClose}>Vazgeç</button><button className="channel-create-primary" disabled={busy||!name.trim()}>{busy?'Oluşturuluyor…':'Kategoriyi oluştur'}</button></footer></form></section></div>
}

function DeleteProjectDialog({auth,organization,project,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;onClose:()=>void;onDeleted:()=>void}){
  useEscape(true,onClose)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Proje silinemedi.');setBusy(false)}}
  return <div className="channel-modal-backdrop" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}><section className="delete-channel-dialog delete-project-dialog"><span>!</span><small>PROJEYİ SİL</small><h2>{project.name} silinsin mi?</h2><p>Proje; kategorileri, kanalları ve mesajlarıyla birlikte hemen çalışma alanından kaldırılacak. Tüm veriler 30 gün saklandıktan sonra geri döndürülemez biçimde kalıcı olarak silinecek.</p>{error&&<div className="channel-create-error">{error}</div>}<footer><button onClick={onClose}>Vazgeç</button><button className="delete-channel-primary" onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Projeyi sil'}</button></footer></section></div>
}

function ChannelSidebar({auth,organization,project,categories,selected,onSelect,onChannelCreated,onChannelDeleted,onCategoryCreated,onCategoryDeleted,people,selectedPerson,unread,channelUnread,onSelectPerson,onViewProfile,onCallPerson,onResize,typing,voiceRooms,voiceLevels,prefs,onStar,onHidden,onOpenAdmin,perms,groups,selectedGroup,onSelectGroup,onCreateGroup,onLeaveGroup}:{groups:ChatGroup[];selectedGroup:ChatGroup|null;onSelectGroup:(group:ChatGroup)=>void;onCreateGroup:()=>void;onLeaveGroup:(group:ChatGroup)=>void;auth:Auth;organization:Organization;project:Project;categories:ChannelCategory[];selected:Channel|null;onSelect:(channel:Channel)=>void;onChannelCreated:(categoryId:string,channel:Channel)=>void;onChannelDeleted:(channelId:string)=>void;onCategoryCreated:(category:ChannelCategory)=>void;onCategoryDeleted:(categoryId:string)=>void;people:ProfileMember[];selectedPerson:ProfileMember|null;unread:Record<string,number>;channelUnread:Record<string,number>;onSelectPerson:(person:ProfileMember)=>void;onViewProfile:(person:ProfileMember)=>void;onCallPerson:(userId:string)=>void;onResize:(event:React.MouseEvent)=>void;typing:Set<string>;voiceRooms:Record<string,CallParticipant[]>;voiceLevels:Record<string,number>;prefs:Record<string,{starred:boolean;notify:string;hidden?:boolean}>;onStar:(kind:'CHANNEL'|'PERSON',targetId:string,starred:boolean)=>void;onHidden:(channelId:string,hidden:boolean)=>void;onOpenAdmin?:()=>void;perms:PermissionApi}){
  const[collapsed,setCollapsed]=useState<Set<string>>(new Set()),[search,setSearch]=useState(''),[creatingCategory,setCreatingCategory]=useState<ChannelCategory|null>(null),[creatingCategoryGroup,setCreatingCategoryGroup]=useState(false),[deletingChannel,setDeletingChannel]=useState<Channel|null>(null),[deletingCategory,setDeletingCategory]=useState<ChannelCategory|null>(null),[contextMenu,setContextMenu]=useState<{x:number;y:number;channel?:Channel;category?:ChannelCategory}|null>(null),[personMenu,setPersonMenu]=useState<{x:number;y:number;person:ProfileMember}|null>(null),[peopleCollapsed,setPeopleCollapsed]=useState(false),[groupsCollapsed,setGroupsCollapsed]=useState(false),[projectMenu,setProjectMenu]=useState<{x:number;y:number;chooseCategory?:boolean}|null>(null),[copied,setCopied]=useState(false),[browsingChannels,setBrowsingChannels]=useState(false)
  const toggle=(id:string)=>setCollapsed(current=>{const next=new Set(current);next.has(id)?next.delete(id):next.add(id);return next})
  useEscape(Boolean(contextMenu),()=>setContextMenu(null))
  useEscape(Boolean(personMenu),()=>setPersonMenu(null))
  useEscape(Boolean(projectMenu),()=>setProjectMenu(null))
  const normalized=search.trim().toLocaleLowerCase('tr-TR')
  const[starredCollapsed,setStarredCollapsed]=useState(false)
  const starredChannels=categories.flatMap(category=>category.channels).filter(channel=>prefs[`CHANNEL:${channel.id}`]?.starred&&!prefs[`CHANNEL:${channel.id}`]?.hidden)
  const hiddenChannels=categories.flatMap(category=>category.channels).filter(channel=>prefs[`CHANNEL:${channel.id}`]?.hidden)
  const starredPeople=people.filter(person=>prefs[`PERSON:${person.id}`]?.starred)
  const visiblePeople=people.filter(person=>!normalized||person.displayName.toLocaleLowerCase('tr-TR').includes(normalized)||person.username.toLocaleLowerCase('tr-TR').includes(normalized))
  const hiddenUnread=people.reduce((total,person)=>total+(unread[person.id]||0),0)
  return <aside className="channel-sidebar"><ResizeHandle edge="right" onMouseDown={onResize} label="Kenar çubuğu genişliği"/><header className="channel-project-header"><div title={organization.inviteCode?`Davet kodu: ${organization.inviteCode} — kopyalamak için ••• menüsünü kullan`:undefined}><b>{project.name}</b><small>PROJE KANALLARI</small></div><button aria-label="Proje menüsü" title="Proje menüsü" onClick={e=>{const box=e.currentTarget.getBoundingClientRect();setCopied(false);setProjectMenu(projectMenu?null:{x:Math.min(box.right,window.innerWidth-236),y:box.bottom+6})}}>•••</button></header><div className="channel-search"><span>⌕</span><input value={search} onChange={e=>setSearch(e.target.value)} placeholder="Kanal ve kişi ara"/></div><div className="channel-scroll">{(starredChannels.length>0||starredPeople.length>0)&&<section className="channel-category starred-section"><header><button className="category-toggle" onClick={()=>setStarredCollapsed(value=>!value)}><i className={starredCollapsed?'chevron collapsed':'chevron'}/>YILDIZLI</button></header>{!starredCollapsed&&<div>
    {starredChannels.map(channel=><button key={`star-${channel.id}`} className={selected?.id===channel.id&&!selectedPerson&&!selectedGroup?'channel-row active':'channel-row'} onClick={()=>onSelect(channel)}><span className={`channel-type ${channel.type.toLowerCase()}`}>{channelIcons[channel.type]}</span><b>{channel.name}</b></button>)}
    {starredPeople.map(person=><button key={`star-${person.id}`} className={selectedPerson?.id===person.id?'person-row active':'person-row'} onClick={()=>onSelectPerson(person)} onContextMenu={e=>{e.preventDefault();setPersonMenu({x:e.clientX,y:e.clientY,person})}}><span className={`person-avatar ${avatarHue(person.id)}`}>{person.avatarUrl?<img src={person.avatarUrl} alt=""/>:userInitials(person.displayName)}<i className={presenceClass(person.status)}/></span><b>{person.displayName}</b></button>)}
  </div>}</section>}{categories.map(category=>{const channels=category.channels.filter(channel=>!prefs[`CHANNEL:${channel.id}`]?.hidden&&(!normalized||channel.name.toLocaleLowerCase('tr-TR').includes(normalized)));if(normalized&&!channels.length)return null;return <section className="channel-category" key={category.id} onContextMenu={e=>{if(category.channels.length)return;e.preventDefault();setContextMenu({x:e.clientX,y:e.clientY,category})}}><header><button className="category-toggle" onClick={()=>toggle(category.id)}><i className={collapsed.has(category.id)?'chevron collapsed':'chevron'}/>{category.name}</button>{perms.can('channel.manage')&&<button className="add-channel" onClick={()=>setCreatingCategory(category)} aria-label={`${category.name} kategorisine kanal ekle`} title="Kanal ekle"><span/></button>}</header>{!collapsed.has(category.id)&&<div>{channels.map(channel=><div className="voice-channel-block"><button key={channel.id} className={selected?.id===channel.id&&!selectedPerson&&!selectedGroup?'channel-row active':'channel-row'} onClick={()=>onSelect(channel)} onContextMenu={e=>{e.preventDefault();setContextMenu({x:e.clientX,y:e.clientY,channel})}}><span className={`channel-type ${channel.type.toLowerCase()}`}>{channelIcons[channel.type]}</span><b>{channel.name}</b><span className="voice-channel-badges">{channel.type==='VOICE'&&(voiceRooms[channel.id]?.length?<em className="voice-count">{voiceRooms[channel.id].length}</em>:<small>SES</small>)}{channelUnread[channel.id]>0&&<em className="direct-unread">{channelUnread[channel.id]>99?'99+':channelUnread[channel.id]}</em>}</span></button>{channel.type==='VOICE'&&voiceRooms[channel.id]?.length?<VoiceMembers participants={voiceRooms[channel.id]} people={people} self={{...auth.user,status:'ONLINE',role:organization.role}} levels={voiceLevels} onProfile={onViewProfile}/>:null}</div>)}</div>}</section>})}{!categories.length&&<div className="empty-categories"><b>İlk kategorini oluştur</b><p>Kanallarını düzenlemek için bir kategoriyle başla.</p><button onClick={()=>setCreatingCategoryGroup(true)}>Kategori oluştur</button></div>}<section className="channel-category people-section"><header><button className="category-toggle" onClick={()=>setPeopleCollapsed(value=>!value)}><i className={peopleCollapsed?'chevron collapsed':'chevron'}/>KİŞİLER — {people.length}</button>{peopleCollapsed&&hiddenUnread>0&&<em className="direct-unread">{hiddenUnread>99?'99+':hiddenUnread}</em>}</header>{!peopleCollapsed&&<div>{visiblePeople.map(person=><button key={person.id} className={`person-row${selectedPerson?.id===person.id?' active':''}${unread[person.id]>0?' unread':''}`} onClick={()=>onSelectPerson(person)} onContextMenu={e=>{e.preventDefault();setPersonMenu({x:e.clientX,y:e.clientY,person})}} title={`@${person.username}`}><span className={`person-avatar ${avatarHue(person.id)}`}>{person.avatarUrl?<img src={person.avatarUrl} alt=""/>:userInitials(person.displayName)}<i className={presenceClass(person.status)}/></span><b>{person.displayName}</b>{typing.has(person.id)?<em className="person-typing"><i/><i/><i/></em>:unread[person.id]>0?<em className="direct-unread">{unread[person.id]>99?'99+':unread[person.id]}</em>:null}</button>)}{!people.length&&<p className="people-empty">Kuruluşta başka kimse yok.</p>}{!!people.length&&!visiblePeople.length&&<p className="people-empty">Eşleşen kişi yok.</p>}</div>}</section><section className="channel-category groups-section"><header><button className="category-toggle" onClick={()=>setGroupsCollapsed(value=>!value)}><i className={groupsCollapsed?'chevron collapsed':'chevron'}/>GRUPLAR — {groups.length}</button><button className="add-channel" title="Grup oluştur" aria-label="Grup oluştur" onClick={onCreateGroup}><span/></button></header>{!groupsCollapsed&&<div>{groups.map(group=><div key={group.id} role="button" tabIndex={0} className={`person-row group-row${selectedGroup?.id===group.id?' active':''}${group.unread>0?' unread':''}`} onClick={()=>onSelectGroup(group)} onKeyDown={event=>{if(event.key==='Enter')onSelectGroup(group)}} title={group.members.map(member=>member.displayName).join(', ')}><span className="group-avatar">{userInitials(group.name)}</span><b>{group.name}</b>{group.unread>0&&<em className="direct-unread">{group.unread>99?'99+':group.unread}</em>}<button type="button" className="group-leave" title="Gruptan ayrıl" onClick={event=>{event.stopPropagation();onLeaveGroup(group)}}>×</button></div>)}{!groups.length&&<p className="people-empty">Henüz grup yok. Sağdaki + ile oluştur ya da bir kanaldan arama başlat.</p>}</div>}</section></div>{creatingCategoryGroup&&<CategoryCreator auth={auth} organization={organization} project={project} onClose={()=>setCreatingCategoryGroup(false)} onCreated={category=>{onCategoryCreated(category);setCreatingCategoryGroup(false)}}/>}{creatingCategory&&<ChannelCreator auth={auth} organization={organization} project={project} category={creatingCategory} onClose={()=>setCreatingCategory(null)} onCreated={channel=>{onChannelCreated(creatingCategory.id,channel);onSelect(channel);setCollapsed(current=>{const next=new Set(current);next.delete(creatingCategory.id);return next});setCreatingCategory(null)}}/>}{contextMenu&&<div className="channel-context-layer" onMouseDown={()=>setContextMenu(null)}><div className="channel-context-menu" style={{left:contextMenu.x,top:contextMenu.y}} onMouseDown={e=>e.stopPropagation()}>{contextMenu.channel&&<button className="plain" onClick={()=>{onStar('CHANNEL',contextMenu.channel!.id,!prefs[`CHANNEL:${contextMenu.channel!.id}`]?.starred);setContextMenu(null)}}>{prefs[`CHANNEL:${contextMenu.channel!.id}`]?.starred?'Yıldızı kaldır':'Yıldıza ekle'}</button>}{(contextMenu.channel?perms.canIn(contextMenu.channel.id,'channel.manage'):perms.can('channel.manage'))
      ?<button onClick={()=>{if(contextMenu.channel)setDeletingChannel(contextMenu.channel);if(contextMenu.category)setDeletingCategory(contextMenu.category);setContextMenu(null)}}>Sil</button>
      :<button className="plain" disabled>Silme yetkin yok</button>}</div></div>}{projectMenu&&<div className="channel-context-layer" onMouseDown={()=>setProjectMenu(null)}><div className="channel-context-menu invite-menu" style={{left:projectMenu.x-224,top:projectMenu.y}} onMouseDown={e=>e.stopPropagation()}>
      <p className="menu-title">PROJE İŞLEMLERİ</p>
      <button className="project-menu-action" disabled={!perms.can('channel.manage')} onClick={()=>{setCreatingCategoryGroup(true);setProjectMenu(null)}}><span>＋</span><div><b>Kategori oluştur</b><small>Kanalları yeni bir başlık altında grupla</small></div></button>
      <button className="project-menu-action" disabled={!categories.length||!perms.can('channel.manage')} onClick={()=>setProjectMenu(current=>current?{...current,chooseCategory:!current.chooseCategory}:current)}><span>#</span><div><b>Kanal oluştur</b><small>{categories.length?'Ekleneceği kategoriyi seç':'Önce bir kategori oluştur'}</small></div></button>
      {projectMenu.chooseCategory&&<div className="project-menu-categories">{categories.map(category=><button key={category.id} onClick={()=>{setCreatingCategory(category);setProjectMenu(null)}}><i className="chevron collapsed"/>{category.name}</button>)}</div>}
      <button className="project-menu-action" onClick={()=>{setBrowsingChannels(true);setProjectMenu(null)}}><span>◎</span><div><b>Kanallara göz at</b><small>{hiddenChannels.length?`${hiddenChannels.length} kanaldan ayrıldın`:'Tüm kanalları gör, katıl ya da ayrıl'}</small></div></button>
      <button className="project-menu-action" disabled={!organization.inviteCode} onClick={()=>{if(!organization.inviteCode)return;void navigator.clipboard.writeText(organization.inviteCode);setCopied(true);window.setTimeout(()=>setCopied(false),1800)}}><span>＠</span><div><b>Kişiyi davet et</b><small>{copied?'Davet kodu kopyalandı ✓':'Kuruluş davet kodunu kopyala'}</small></div></button>
      <hr/>
      <p className="menu-title">KURULUŞ</p>
      <p className="invite-org">{organization.name}</p>
      {onOpenAdmin&&<button className="project-menu-action" onClick={()=>{setProjectMenu(null);onOpenAdmin()}}><span>🛡</span><div><b>Yönetim paneli</b><small>Üyeler, roller, davetler ve kanallar</small></div></button>}
      {perms.can('member.invite')&&organization.inviteCode?<>
        <p className="menu-title">DAVET KODU</p>
        <button className="invite-code" onClick={()=>{void navigator.clipboard.writeText(organization.inviteCode||'');setCopied(true);window.setTimeout(()=>setCopied(false),1800)}} title="Kopyalamak için tıkla"><code>{organization.inviteCode}</code><span>{copied?'Kopyalandı ✓':'Kopyala'}</span></button>
        <p className="invite-hint">Bu kodu paylaşarak ekibini kuruluşa davet edebilirsin.</p>
      </>:<p className="invite-hint">Davet kodunu görmek için “Davet kodu yönet” yetkisi gerekir.</p>}
    </div></div>}{personMenu&&<div className="channel-context-layer" onMouseDown={()=>setPersonMenu(null)}><div className="channel-context-menu person-context-menu" style={{left:Math.min(personMenu.x,window.innerWidth-190),top:Math.min(personMenu.y,window.innerHeight-90)}} onMouseDown={e=>e.stopPropagation()}><button onClick={()=>{onViewProfile(personMenu.person);setPersonMenu(null)}}>Profili görüntüle</button><button onClick={()=>{onSelectPerson(personMenu.person);setPersonMenu(null)}}>Mesaj gönder</button><button onClick={()=>{onCallPerson(personMenu.person.id);setPersonMenu(null)}}>Sesli ara</button><button onClick={()=>{onStar('PERSON',personMenu.person.id,!prefs[`PERSON:${personMenu.person.id}`]?.starred);setPersonMenu(null)}}>{prefs[`PERSON:${personMenu.person.id}`]?.starred?'Yıldızı kaldır':'Yıldıza ekle'}</button></div></div>}{deletingChannel&&<DeleteChannelDialog auth={auth} organization={organization} project={project} channel={deletingChannel} onClose={()=>setDeletingChannel(null)} onDeleted={()=>{onChannelDeleted(deletingChannel.id);setDeletingChannel(null)}}/>}{deletingCategory&&<DeleteCategoryDialog auth={auth} organization={organization} project={project} category={deletingCategory} onClose={()=>setDeletingCategory(null)} onDeleted={()=>{onCategoryDeleted(deletingCategory.id);setDeletingCategory(null)}}/>}{browsingChannels&&<ChannelBrowser categories={categories} prefs={prefs} onClose={()=>setBrowsingChannels(false)} onToggle={onHidden} onOpen={channel=>{onSelect(channel);setBrowsingChannels(false)}}/>}</aside>
}

function WorkspaceBody({auth,organization,logout}:{auth:Auth;organization:Organization;logout:()=>void}){
  const callApi=useCall()
  const my=useMyPermissions(auth.accessToken,organization.id)
  const sidebar=useResizable('p2:sidebar-width',252,190,430,1)
  const dmTypists=useTypists(auth.accessToken,organization.id,auth.user.id,payload=>!!payload.conversationId)
  const[projects,setProjects]=useState<Project[]>([]),[friends,setFriends]=useState<Friend[]>([]),[selected,setSelected]=useState<Project|null>(null),[loading,setLoading]=useState(true),[creating,setCreating]=useState(false),[projectContext,setProjectContext]=useState<{project:Project;x:number;y:number}|null>(null),[deletingProject,setDeletingProject]=useState<Project|null>(null),[profileOpen,setProfileOpen]=useState(false),[people,setPeople]=useState<ProfileMember[]>([]),[selectedPerson,setSelectedPerson]=useState<ProfileMember|null>(null),[unread,setUnread]=useState<Record<string,number>>({}),[channelUnread,setChannelUnread]=useState<Record<string,number>>({}),[profileRequest,setProfileRequest]=useState<ProfileUser|null>(null),[pendingChannel,setPendingChannel]=useState<string|null>(popoutTarget?.kind==='channel'?popoutTarget.id:null),[splitTarget,setSplitTarget]=useState<PaneTarget|null>(null),[prefs,setPrefs]=useState<Record<string,{starred:boolean;notify:string;hidden?:boolean}>>({}),[settingsOpen,setSettingsOpen]=useState(false),[adminOpen,setAdminOpen]=useState(false),[gitOpen,setGitOpen]=useState(false),[orgName,setOrgName]=useState(organization.name),[inviteCode,setInviteCode]=useState(organization.inviteCode),[preference,setPreference]=useState('ONLINE'),[myStatus,setMyStatus]=useState('ONLINE')
  const[categories,setCategories]=useState<ChannelCategory[]>([]),[selectedChannel,setSelectedChannel]=useState<Channel|null>(null)
  useEffect(()=>{
    let applied=''
    const evaluate=()=>{const value=getSettings(),now=new Date(),minutes=now.getHours()*60+now.getMinutes(),[sh,sm]=value.workStart.split(':').map(Number),[eh,em]=value.workEnd.split(':').map(Number),outside=!value.workDays.includes(now.getDay())||minutes<(sh*60+sm)||minutes>=(eh*60+em);const next=outside?value.outsideHoursStatus:'none';if(next!=='none'&&next!==applied){applied=next;emitRealtime('presence:prefer',{status:next})}if(!outside)applied=''}
    evaluate();const timer=window.setInterval(evaluate,60_000),unsubscribe=subscribeSettings(evaluate)
    return()=>{window.clearInterval(timer);unsubscribe()}
  },[])
  const[groups,setGroups]=useState<ChatGroup[]>([]),[selectedGroup,setSelectedGroup]=useState<ChatGroup|null>(null)
  const[activity,setActivity]=useState<ActivityItem[]>([]),[activityOpen,setActivityOpen]=useState(false),[activityLoading,setActivityLoading]=useState(true)
  const[jumpRequest,setJumpRequest]=useState<{messageId:string;parentId:string|null}|null>(null)
  const activityUnread=activity.filter(item=>!item.readAt).length
  useEffect(()=>{
    request<{items:ActivityItem[];unread:number}>(`/organizations/${organization.id}/activity`,auth.accessToken)
      .then(payload=>setActivity(payload.items)).catch(()=>{}).finally(()=>setActivityLoading(false))
  },[auth.accessToken,organization.id])
  async function readAllActivity(){
    setActivity(current=>current.map(item=>({...item,readAt:item.readAt||new Date().toISOString()})))
    try{await request(`/organizations/${organization.id}/activity/read`,auth.accessToken,{method:'POST'})}catch{}
  }
  // Etkinliğe tıklayınca ilgili yere gidip mesajı vurgular.
  async function openActivity(item:ActivityItem){
    setActivityOpen(false)
    setActivity(current=>current.map(row=>row.id===item.id?{...row,readAt:row.readAt||new Date().toISOString()}:row))
    if(!item.readAt)try{await request(`/organizations/${organization.id}/activity/${item.id}/read`,auth.accessToken,{method:'POST'})}catch{}
    if(item.groupId){
      const group=groups.find(row=>row.id===item.groupId)
      if(group){setSelectedGroup(group);setSelectedPerson(null)}
      return
    }
    if(item.channelId){
      const project=projects.find(row=>row.id===item.projectId)
      if(project&&project.id!==selected?.id)setSelected(project)
      const channel=categories.flatMap(category=>category.channels).find(row=>row.id===item.channelId)
      if(channel){setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null)}
      if(item.messageId)setJumpRequest({messageId:item.messageId,parentId:item.parentId})
    }
  }
  const[groupMessages,setGroupMessages]=useState<GroupMessage[]>([]),[splitGroupMessages,setSplitGroupMessages]=useState<GroupMessage[]>([]),[creatingGroup,setCreatingGroup]=useState(false),[leavingGroup,setLeavingGroup]=useState<ChatGroup|null>(null),[removingMember,setRemovingMember]=useState<{group:ChatGroup;userId:string;name:string}|null>(null),[groupBusy,setGroupBusy]=useState(false),[groupError,setGroupError]=useState('')
  useEffect(()=>{request<ChatGroup[]>(`/organizations/${organization.id}/groups`,auth.accessToken).then(setGroups).catch(()=>{})},[auth.accessToken,organization.id])
  useEffect(()=>{
    if(!selectedGroup){setGroupMessages([]);return}
    setGroupMessages([])
    request<GroupMessage[]>(`/organizations/${organization.id}/groups/${selectedGroup.id}/messages`,auth.accessToken).then(setGroupMessages).catch(()=>{})
  },[selectedGroup?.id,auth.accessToken,organization.id])
  // Bölünmüş panelde açık grup kendi mesaj listesini taşır.
  useEffect(()=>{
    const target=splitTarget
    if(target?.kind!=='group'){setSplitGroupMessages([]);return}
    setSplitGroupMessages([])
    request<GroupMessage[]>(`/organizations/${organization.id}/groups/${target.group.id}/messages`,auth.accessToken).then(setSplitGroupMessages).catch(()=>{})
  },[splitTarget?.kind==='group'?splitTarget.group.id:null,organization.id,auth.accessToken])
  // Aramaya katıldığımda ilgili konuşma kendiliğinden açılır (kabul ettiğim çağrı,
  // katıldığım grup araması ya da sesli oda).
  const openedCall=useRef('')
  useEffect(()=>{
    const active=callApi.call
    if(!active||!active.participants.some(item=>item.userId===auth.user.id)){if(!active)openedCall.current='';return}
    if(openedCall.current===active.id)return
    // Hedef henüz yüklenmediyse işaretlemeyip sonraki güncellemede tekrar denenir.
    if(active.kind==='group'&&active.groupId){
      const group=groups.find(item=>item.id===active.groupId)
      if(!group)return
      openedCall.current=active.id
      setSelectedGroup(group);setSelectedPerson(null);setSelectedChannel(current=>current)
      return
    }
    if(active.kind==='direct'){
      const otherId=active.participants.find(item=>item.userId!==auth.user.id)?.userId||active.caller
      const person=people.find(item=>item.id===otherId)
      if(!person)return
      openedCall.current=active.id
      setSelectedPerson(person);setSelectedGroup(null);setUnread(current=>({...current,[person.id]:0}))
      return
    }
    if(active.kind==='channel'&&active.channelId){
      // Kanal aramasında sunucu bir grup açar; grup gelir gelmez ona geçilir.
      const group=groups.find(item=>item.channelId===active.channelId)
      if(group){openedCall.current=active.id;setSelectedGroup(group);setSelectedPerson(null);return}
      const channel=categories.flatMap(category=>category.channels).find(item=>item.id===active.channelId)
      if(channel){setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null)}
    }
  },[callApi.call?.id,callApi.call?.participants.length,groups,people,categories])

  // Grup seçimi ve kişi seçimi birbirini kapatır.
  const openGroup=(group:ChatGroup)=>{setSelectedGroup(group);setSelectedPerson(null)}
  async function createGroup(name:string,userIds:string[]){
    setGroupBusy(true);setGroupError('')
    try{
      const group=await request<ChatGroup>(`/organizations/${organization.id}/groups`,auth.accessToken,{method:'POST',body:JSON.stringify({name:name||undefined,userIds})})
      setGroups(current=>current.some(item=>item.id===group.id)?current:[group,...current])
      openGroup(group)
      setCreatingGroup(false)
    }catch(problem:any){setGroupError(problem?.message||'Grup oluşturulamadı.')}
    finally{setGroupBusy(false)}
  }
  async function leaveGroup(group:ChatGroup){
    try{await request(`/organizations/${organization.id}/groups/${group.id}/leave`,auth.accessToken,{method:'POST'})}catch{}
    setGroups(current=>current.filter(item=>item.id!==group.id))
    setSelectedGroup(current=>current?.id===group.id?null:current)
  }
  async function removeGroupMember(groupId:string,userId:string){
    try{
      await request(`/organizations/${organization.id}/groups/${groupId}/members/${userId}`,auth.accessToken,{method:'DELETE'})
      setGroups(current=>current.map(item=>item.id===groupId?{...item,members:item.members.filter(member=>member.id!==userId)}:item))
      setSelectedGroup(current=>current?.id===groupId?{...current,members:current.members.filter(member=>member.id!==userId)}:current)
    }catch{}
  }
  async function addGroupMembers(groupId:string,userIds:string[]){
    try{
      const group=await request<ChatGroup>(`/organizations/${organization.id}/groups/${groupId}/members`,auth.accessToken,{method:'POST',body:JSON.stringify({userIds})})
      setGroups(current=>current.map(item=>item.id===group.id?group:item))
      setSelectedGroup(current=>current?.id===group.id?group:current)
    }catch{}
  }
  useEffect(()=>{request<Project[]>(`/organizations/${organization.id}/projects`,auth.accessToken).then(items=>{setProjects(items);setSelected(items.find(item=>item.id===popoutTarget?.projectId)||items[0]||null)}).finally(()=>setLoading(false))},[auth.accessToken,organization.id])
  useEffect(()=>{if(!selected){setCategories([]);setSelectedChannel(null);return}setCategories([]);request<ChannelCategory[]>(`/organizations/${organization.id}/projects/${selected.id}/channels`,auth.accessToken).then(items=>{setCategories(items);const all=items.flatMap(item=>item.channels);setSelectedChannel(all.find(channel=>channel.id===pendingChannel)||all[0]||null);setPendingChannel(null)}).catch(()=>setCategories([]))},[auth.accessToken,organization.id,selected?.id,my.version])
  // Ayrılınan kanal açık kalmasın: tercihler kanallardan sonra yüklendiği için
  // seçim burada da bir kez daha doğrulanır.
  useEffect(()=>{
    if(!selectedChannel||!prefs[`CHANNEL:${selectedChannel.id}`]?.hidden)return
    const visible=categories.flatMap(category=>category.channels).filter(channel=>!prefs[`CHANNEL:${channel.id}`]?.hidden)
    setSelectedChannel(visible[0]||null)
  },[prefs,categories,selectedChannel?.id])
  // Bir grup hem ana panelde hem bölünmüş panelde açık olabilir; canlı olaylar ikisine de işlenir.
  const groupOpen=(groupId:string)=>selectedGroup?.id===groupId||(splitTarget?.kind==='group'&&splitTarget.group.id===groupId)
  function applyGroupMessages(groupId:string,update:(items:GroupMessage[])=>GroupMessage[]){
    if(selectedGroup?.id===groupId)setGroupMessages(update)
    if(splitTarget?.kind==='group'&&splitTarget.group.id===groupId)setSplitGroupMessages(update)
  }
  useRealtimeEvents(auth.accessToken,organization.id,{
    'direct:created':(payload:{message:{content:string;author:{id:string;displayName:string}}})=>{
      const from=payload.message.author.id
      if(from===auth.user.id)return
      const open=selectedPerson?.id===from&&document.hasFocus()
      if(!open){setUnread(current=>({...current,[from]:(current[from]||0)+1}))}
      // Açık ve odaktaki sohbete gelen mesaj rahatsız etmesin; diğer her durumda haber ver.
      if(open)return
      playMessageSound()
      if(!popoutTarget)showMessageNotification(payload.message.author.displayName,payload.message.content,{userId:from})
    },
    'group:created':(payload:{group:ChatGroup})=>{
      setGroups(current=>current.some(item=>item.id===payload.group.id)?current.map(item=>item.id===payload.group.id?payload.group:item):[payload.group,...current])
    },
    'activity:new':(payload:{activity:ActivityItem})=>{
      setActivity(current=>current.some(item=>item.id===payload.activity.id)?current:[payload.activity,...current].slice(0,200))
    },
    'group:reply':(payload:{groupId:string;parentId:string;message:GroupMessage;replies:number})=>{
      publishGroupReply(payload)
      applyGroupMessages(payload.groupId,current=>current.map(item=>item.id===payload.parentId?{...item,_count:{replies:payload.replies}}:item))
    },
    'group:message:updated':(payload:{groupId:string;message:GroupMessage})=>{
      applyGroupMessages(payload.groupId,current=>current.map(item=>item.id===payload.message.id?payload.message:item))
    },
    'group:message:deleted':(payload:{groupId:string;messageId:string})=>{
      applyGroupMessages(payload.groupId,current=>current.filter(item=>item.id!==payload.messageId))
    },
    'group:reactions':(payload:{groupId:string;messageId:string;rows:{emoji:string;userId:string}[]})=>{
      publishGroupReactions(payload)
      applyGroupMessages(payload.groupId,current=>current.map(item=>item.id===payload.messageId?{...item,reactions:payload.rows}:item))
    },
    'group:left':(payload:{groupId:string})=>{
      setGroups(current=>current.filter(item=>item.id!==payload.groupId))
      setSelectedGroup(current=>current?.id===payload.groupId?null:current)
      setSplitTarget(current=>current?.kind==='group'&&current.group.id===payload.groupId?null:current)
    },
    'group:updated':(payload:{group:ChatGroup})=>{
      setGroups(current=>current.map(item=>item.id===payload.group.id?{...payload.group,unread:item.unread}:item))
      setSelectedGroup(current=>current?.id===payload.group.id?{...payload.group,unread:current.unread}:current)
      setSplitTarget(current=>current?.kind==='group'&&current.group.id===payload.group.id?{kind:'group',group:{...payload.group,unread:current.group.unread}}:current)
    },
    'group:message':(payload:{groupId:string;message:GroupMessage})=>{
      const mine=payload.message.author.id===auth.user.id
      const showing=groupOpen(payload.groupId)
      const open=showing&&document.hasFocus()
      // Açık gruba gelen mesaj pencere odakta olmasa da anında listeye girer.
      if(showing)applyGroupMessages(payload.groupId,current=>current.some(item=>item.id===payload.message.id)?current:[...current,payload.message])
      setGroups(current=>current.map(item=>item.id===payload.groupId?{...item,unread:open||mine?item.unread:(item.unread||0)+1,lastMessage:{id:payload.message.id,content:payload.message.content,createdAt:payload.message.createdAt,authorId:payload.message.author.id}}:item))
      if(mine||open)return
      playMessageSound()
      showMessageNotification(payload.message.author.displayName,payload.message.content,{})
    },
    'presence:update':(payload:{userId:string;status:string})=>setPeople(items=>items.map(item=>item.id===payload.userId?{...item,status:payload.status}:item)),
    'presence:self':(payload:{preference:string;status:string})=>{setPreference(payload.preference);setMyStatus(payload.status)},
    'message:created':(payload:{projectId:string;channelId:string;channelName?:string;message:{content:string;author:{id:string;displayName:string}}})=>{
      if(payload.message.author.id===auth.user.id)return
      const watching=!selectedPerson&&selectedChannel?.id===payload.channelId&&document.hasFocus()
      if(!watching)setChannelUnread(current=>({...current,[payload.channelId]:(current[payload.channelId]||0)+1}))
      const rule=prefs[`CHANNEL:${payload.channelId}`]?.notify||'MENTIONS'
      if(rule==='MUTED')return
      if(rule!=='ALL'&&!mentionsMe(payload.message.content,auth.user.username))return
      if(watching)return
      playMessageSound()
      showMessageNotification(`${payload.message.author.displayName}${payload.channelName?` · #${payload.channelName}`:''}`,payload.message.content,{projectId:payload.projectId,channelId:payload.channelId})
    },
    'organization:invite':(payload:{organizationId:string;inviteCode:string})=>{if(payload.organizationId===organization.id)setInviteCode(payload.inviteCode)},
    'organization:updated':(payload:{organization:{id:string;name:string}})=>{if(payload.organization.id===organization.id)setOrgName(payload.organization.name)},
    // Kuruluştan çıkarıldıysam çalışma alanı artık geçerli değil: baştan yüklenir.
    'organization:removed':(payload:{organizationId:string})=>{if(payload.organizationId===organization.id)window.location.reload()},
    'project:created':(payload:{project:Project})=>setProjects(items=>items.some(item=>item.id===payload.project.id)?items:[payload.project,...items]),
    'project:deleted':(payload:{projectId:string})=>setProjects(items=>{const next=items.filter(project=>project.id!==payload.projectId);setSelected(current=>current?.id===payload.projectId?(next[0]||null):current);return next}),
    'category:created':(payload:{projectId:string;category:ChannelCategory})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.some(item=>item.id===payload.category.id)?items:[...items,payload.category])},
    'category:deleted':(payload:{projectId:string;categoryId:string})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.filter(item=>item.id!==payload.categoryId))},
    'channel:created':(payload:{projectId:string;categoryId:string;channel:Channel})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>category.id===payload.categoryId&&!category.channels.some(item=>item.id===payload.channel.id)?{...category,channels:[...category.channels,payload.channel]}:category))},
    'channel:updated':(payload:{projectId:string;channel:Channel})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>({...category,channels:category.channels.map(channel=>channel.id===payload.channel.id?{...channel,...payload.channel}:channel)})));setSelectedChannel(current=>current?.id===payload.channel.id?{...current,...payload.channel}:current);setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===payload.channel.id?{kind:'channel',channel:{...current.channel,...payload.channel}}:current)},
    'channel:deleted':(payload:{projectId:string;channelId:string})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>({...category,channels:category.channels.filter(channel=>channel.id!==payload.channelId)})));setSelectedChannel(current=>current?.id===payload.channelId?null:current);setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===payload.channelId?null:current)},
  })
  useEffect(()=>{request<ProfileMember[]>(`/organizations/${organization.id}/members`,auth.accessToken).then(items=>setPeople(items.filter(item=>item.id!==auth.user.id))).catch(()=>{})},[auth.accessToken,organization.id,auth.user.id])
  useEffect(()=>{request<Array<{user:{id:string};unread:number}>>(`/organizations/${organization.id}/direct`,auth.accessToken).then(items=>setUnread(Object.fromEntries(items.filter(item=>item.unread>0).map(item=>[item.user.id,item.unread])))).catch(()=>{})},[auth.accessToken,organization.id])
  useEscape(Boolean(projectContext),()=>setProjectContext(null))
  useEscape(profileOpen,()=>setProfileOpen(false))
  useEffect(()=>{
    let timer:number|undefined
    const away=(value:boolean)=>emitRealtime('presence:idle',{away:value})
    const blurred=()=>{window.clearTimeout(timer);timer=window.setTimeout(()=>away(true),AWAY_DELAY)}
    const focused=()=>{window.clearTimeout(timer);away(false)}
    const visibility=()=>{if(document.hidden)blurred();else focused()}
    window.addEventListener('blur',blurred);window.addEventListener('focus',focused);document.addEventListener('visibilitychange',visibility)
    if(!document.hasFocus())blurred()
    return()=>{window.clearTimeout(timer);window.removeEventListener('blur',blurred);window.removeEventListener('focus',focused);document.removeEventListener('visibilitychange',visibility)}
  },[])
  function choosePresence(next:string){setPreference(next);emitRealtime('presence:prefer',{status:next})}
  // Kapatma tercihi ana sürece bildirilir; kullanıcı diyalogda seçim yaparsa ayarlara yazılır.
  useEffect(()=>{
    window.p2?.windowControls.setCloseAction?.(getSettings().closeAction)
    return window.p2?.windowControls.onCloseAction?.(action=>updateSettings({closeAction:action}))
  },[])
  // Ctrl + , ile ayarlar açılır (yazı yazarken de çalışır, tarayıcı kısayolu değil)
  useEffect(()=>{
    const shortcut=(event:globalThis.KeyboardEvent)=>{if((event.ctrlKey||event.metaKey)&&event.key===','){event.preventDefault();setSettingsOpen(true)}}
    document.addEventListener('keydown',shortcut)
    return()=>document.removeEventListener('keydown',shortcut)
  },[])
  useEffect(()=>{request<Array<{kind:string;targetId:string;starred:boolean;notify:string;hidden?:boolean}>>(`/organizations/${organization.id}/preferences`,auth.accessToken).then(items=>setPrefs(Object.fromEntries(items.map(item=>[`${item.kind}:${item.targetId}`,{starred:item.starred,notify:item.notify,hidden:!!item.hidden}])))).catch(()=>{})},[auth.accessToken,organization.id])
  function savePreference(kind:'CHANNEL'|'PERSON',targetId:string,patch:{starred?:boolean;notify?:string;hidden?:boolean}){
    const key=`${kind}:${targetId}`
    setPrefs(current=>({...current,[key]:{starred:patch.starred??current[key]?.starred??false,notify:patch.notify??current[key]?.notify??'MENTIONS',hidden:patch.hidden??current[key]?.hidden??false}}))
    void request(`/organizations/${organization.id}/preferences`,auth.accessToken,{method:'PUT',body:JSON.stringify({kind,targetId,...patch})}).catch(()=>{})
  }
  function openPerson(person:ProfileMember){setSelectedPerson(person);setSelectedGroup(null);setUnread(current=>({...current,[person.id]:0}))}
  useEffect(()=>window.p2?.notifications?.onActivate(data=>{
    if(data?.userId){const person=people.find(item=>item.id===data.userId);if(person)openPerson(person);return}
    if(data?.channelId){
      setSelectedPerson(null)
      const open=categories.flatMap(category=>category.channels).find(channel=>channel.id===data.channelId)
      if(open&&selected?.id===data.projectId){setSelectedChannel(open);return}
      setPendingChannel(data.channelId)
      const project=projects.find(item=>item.id===data.projectId)
      if(project)setSelected(project)
    }
  }),[people,projects,categories,selected?.id])
  function openDirect(user:ProfileUser){const person=people.find(item=>item.id===user.id);if(person)openPerson(person)}
  function created(project:Project){setProjects(items=>items.some(item=>item.id===project.id)?items:[project,...items]);setSelected(project);setCreating(false)}
  function projectDeleted(projectId:string){setProjects(items=>{const next=items.filter(project=>project.id!==projectId);setSelected(current=>current?.id===projectId?(next[0]||null):current);return next});setDeletingProject(null);setCategories([]);setSelectedChannel(null)}
  // Popout penceresi hedefini listeler yüklendikçe seçer.
  useEffect(()=>{
    const target=popoutTarget
    if(target?.kind!=='person')return
    const person=people.find(item=>item.id===target.id)
    if(person)setSelectedPerson(current=>current?.id===person.id?current:person)
  },[people])
  useEffect(()=>{
    const target=popoutTarget
    if(target?.kind!=='group')return
    const group=groups.find(item=>item.id===target.id)
    if(group)setSelectedGroup(current=>current?.id===group.id?current:group)
  },[groups])
  // Bir kanal panelini kurar; bölünmüş görünümde iki tanesi yan yana çizilir.
  function channelPane(channel:Channel|null,split:boolean){
    return <ChannelContent key={split?`split-${channel?.id}`:'main'} auth={auth} organization={organization} project={selected!} channel={channel} onDirectMessage={openDirect}
      profileRequest={split?null:profileRequest} onProfileHandled={()=>setProfileRequest(null)} jumpRequest={split?null:jumpRequest} onJumpHandled={()=>setJumpRequest(null)}
      onCallChannel={(channelId,ring)=>void callApi.joinChannel(channelId,ring)} activeCallChannelId={callApi.call?.channelId||null}
      voiceRoom={channel?callApi.voiceRooms[channel.id]||[]:[]} preference={channel?prefs[`CHANNEL:${channel.id}`]:undefined}
      onPreference={patch=>{if(channel)savePreference('CHANNEL',channel.id,patch)}}
      onSplit={target=>setSplitTarget({kind:'channel',channel:target})} onLeave={leaveChannel} onChannelUpdated={updateChannel} perms={my}
      splitPane={split} onCloseSplit={()=>setSplitTarget(null)}/>
  }
  function updateChannel(channel:Channel){
    setCategories(items=>items.map(category=>({...category,channels:category.channels.map(item=>item.id===channel.id?{...item,...channel}:item)})))
    setSelectedChannel(current=>current?.id===channel.id?{...current,...channel}:current)
    setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===channel.id?{kind:'channel',channel:{...current.channel,...channel}}:current)
  }
  // Kanaldan ayrılmak kanalı silmez; yalnızca bu kişinin kenar çubuğundan kaldırır.
  function leaveChannel(channel:Channel){
    savePreference('CHANNEL',channel.id,{hidden:true,notify:'MUTED'})
    setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===channel.id?null:current)
    setSelectedChannel(current=>{
      if(current?.id!==channel.id)return current
      const rest=categories.flatMap(category=>category.channels).filter(item=>item.id!==channel.id&&!prefs[`CHANNEL:${item.id}`]?.hidden)
      return rest[0]||null
    })
  }
  if(loading)return <main className="splash"><span className="logo">P2</span><p>Projeler kontrol ediliyor…</p></main>
  if(!projects.length)return <main className="first-project-screen"><div className="first-project"><div className="tutorial-badge">01 · BAŞLANGIÇ</div><span className="project-orb">◆</span><h1>İlk projeni oluştur.</h1><p>Ekibini, iletişimini, dosyalarını ve çalışma sürecini tek bir merkezden yönet.</p><div className="tutorial-steps"><div><b>1</b><span><strong>Projeni kur</strong><small>Çalışman için ortak bir merkez oluştur.</small></span></div><i>→</i><div><b>2</b><span><strong>Ekibini yönet</strong><small>Rolleri, görevleri ve iletişimi birleştir.</small></span></div><i>→</i><div><b>3</b><span><strong>Paylaş ve üret</strong><small>Çıktıları ekibinle geliştir ve paylaş.</small></span></div></div><button className="first-project-button" onClick={()=>setCreating(true)}>＋ İlk projeni oluştur <span>→</span></button></div>{creating&&<ProjectCreator auth={auth} organization={organization} onClose={()=>setCreating(false)} onCreated={created}/>}</main>
  // Bireysel ve grup panelleri de aynı bölünmüş/popout yapısını kullanır.
  function personPane(person:ProfileMember,split:boolean){
    return <DirectConversation key={split?`split-${person.id}`:'main'} auth={auth} organization={organization} person={person}
      onCall={()=>void callApi.startDirect(person.id)} inCallWith={callApi.call?.participants.some(participant=>participant.userId===person.id)||false}
      profileRequest={split?null:profileRequest} onProfileHandled={()=>setProfileRequest(null)} onRead={userId=>setUnread(current=>({...current,[userId]:0}))}
      starred={!!prefs[`PERSON:${person.id}`]?.starred} onStar={starred=>savePreference('PERSON',person.id,{starred})}
      onSplit={()=>setSplitTarget({kind:'person',person})} splitPane={split} onCloseSplit={()=>setSplitTarget(null)}/>
  }
  function groupPane(group:ChatGroup,split:boolean){
    const apply=split?setSplitGroupMessages:setGroupMessages
    return <GroupChat key={split?`split-${group.id}`:'main'} auth={auth} organizationId={organization.id} group={group} messages={split?splitGroupMessages:groupMessages} people={people}
      onSend={message=>{apply(current=>current.some(item=>item.id===message.id)?current.map(item=>item.id===message.id?message:item):[...current,message]);setGroups(current=>current.map(item=>item.id===group.id?{...item,lastMessage:{id:message.id,content:message.content,createdAt:message.createdAt,authorId:message.author.id}}:item))}}
      onRead={()=>setGroups(current=>current.map(item=>item.id===group.id?{...item,unread:0}:item))}
      onAddMembers={userIds=>void addGroupMembers(group.id,userIds)}
      onRemoveMember={userId=>setRemovingMember({group,userId,name:group.members.find(member=>member.id===userId)?.displayName||'Üye'})}
      onProfile={userId=>{const person=people.find(item=>item.id===userId);if(person)setProfileRequest(person)}}
      onLeave={()=>setLeavingGroup(group)} onSplit={()=>setSplitTarget({kind:'group',group})} splitPane={split} onCloseSplit={()=>setSplitTarget(null)}/>
  }
  function pane(target:PaneTarget|null,split:boolean){
    if(!target)return null
    if(target.kind==='group')return groupPane(target.group,split)
    if(target.kind==='person')return personPane(target.person,split)
    return selected?channelPane(target.channel,split):null
  }
  const staff=my.can('admin.panel')
  const org={...organization,name:orgName,inviteCode}
  const mainTarget:PaneTarget|null=selectedGroup?{kind:'group',group:selectedGroup}:selectedPerson?{kind:'person',person:selectedPerson}:selected?{kind:'channel',channel:selectedChannel}:null

  function popoutTitle(){
    if(selectedGroup)return selectedGroup.name
    if(selectedPerson)return selectedPerson.displayName
    if(selectedChannel)return `#${selectedChannel.name}`
    return selected?.name||'P2'
  }

  // Popout penceresi: rail ve kenar çubuğu olmadan yalnızca seçilen sohbet.
  const popoutReady=popoutTarget?.kind==='person'?!!selectedPerson:popoutTarget?.kind==='group'?!!selectedGroup:!!selectedChannel
  if(popoutTarget)return <main className="project-blank project-stage popout-stage" style={{gridTemplateColumns:'minmax(0,1fr)'}} data-project-title={popoutTitle()}>{popoutReady?pane(mainTarget,false):<section className="channel-content-empty"/>}</main>
  if(projects.length)return <main className={callApi.call?'project-blank project-stage in-call':'project-blank project-stage'} style={{gridTemplateColumns:`72px ${sidebar.width}px minmax(0,1fr)`}} data-project-title={selected?.name||'Proje'}><aside className="project-rail" aria-label="Projeler"><div className="rail-projects">{projects.map((project,index)=><button key={project.id} className={selected?.id===project.id?'rail-project active':'rail-project'} onClick={()=>{setSelected(project);setSelectedPerson(null)}} onContextMenu={e=>{e.preventDefault();setProjectContext({project,x:e.clientX,y:e.clientY})}} aria-label={project.name} title={project.name}><span className={`rail-avatar tone-${index%6}`}>{projectInitials(project.name)}</span></button>)}</div><div className="rail-actions"><button className={activityOpen?'rail-action activity active':'rail-action activity'} aria-label="Etkinlik" title="Etkinlik" onClick={()=>setActivityOpen(open=>!open)}><ComposerIcon name="bell"/>{activityUnread>0&&<em>{activityUnread>99?'99+':activityUnread}</em>}</button><button className={gitOpen?'rail-action code active':'rail-action code'} aria-label="Kaynak kodu" title="Kaynak kodu (Git)" onClick={()=>setGitOpen(open=>!open)}><ComposerIcon name="code"/></button><button className="rail-action" aria-label="Yeni proje" title="Yeni proje" onClick={()=>setCreating(true)}>＋</button><span className="rail-separator"/><button className={`rail-action rail-profile ${avatarHue(auth.user.id)}${profileOpen?' active':''}`} aria-label="Profilim" title={`${auth.user.displayName} · @${auth.user.username}`} onClick={()=>setProfileOpen(open=>!open)}>{auth.user.avatarUrl?<img src={auth.user.avatarUrl} alt=""/>:userInitials(auth.user.displayName)}<i className={presenceClass(preference==='INVISIBLE'?'INVISIBLE':myStatus)}/></button></div>{profileOpen&&<div className="rail-profile-layer" onMouseDown={()=>setProfileOpen(false)}><section className="rail-profile-card" onMouseDown={e=>e.stopPropagation()}><header><span className={`rail-avatar ${avatarHue(auth.user.id)}`}>{auth.user.avatarUrl?<img src={auth.user.avatarUrl} alt=""/>:userInitials(auth.user.displayName)}</span><div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div></header><p className="rail-profile-status"><i className={presenceClass(preference==='INVISIBLE'?'INVISIBLE':myStatus)}/>{preference==='INVISIBLE'?'Görünmez':statusLabel(myStatus)}</p>
      <div className="rail-status-list">{presenceOptions.map(option=><button key={option.value} type="button" className={preference===option.value?'active':undefined} onClick={()=>choosePresence(option.value)}><i className={presenceClass(option.value)}/>{option.label}</button>)}</div>{staff&&<button className="rail-profile-admin" onClick={()=>{setProfileOpen(false);setAdminOpen(true)}}>🛡 Yönetim paneli<small>{organization.role==='OWNER'?'Sahip':'Yönetici'}</small></button>}<button className="rail-profile-settings" onClick={()=>{setProfileOpen(false);setSettingsOpen(true)}}>⚙ Ayarlar<small>Ctrl + ,</small></button><button className="rail-profile-logout" onClick={logout}>Çıkış yap</button></section></div>}</aside>{selected&&<ChannelSidebar groups={groups} selectedGroup={selectedGroup} onSelectGroup={openGroup} onCreateGroup={()=>{setGroupError('');setCreatingGroup(true)}} onLeaveGroup={group=>setLeavingGroup(group)} people={people} selectedPerson={selectedPerson} unread={unread} channelUnread={channelUnread} onSelectPerson={openPerson} onViewProfile={setProfileRequest} onCallPerson={userId=>void callApi.startDirect(userId)} onResize={sidebar.start} typing={new Set(dmTypists.map(item=>item.userId))} voiceRooms={callApi.voiceRooms} voiceLevels={callApi.levels} perms={my} prefs={prefs} onStar={(kind,targetId,starred)=>savePreference(kind,targetId,{starred})} onHidden={(channelId,hidden)=>savePreference('CHANNEL',channelId,{hidden,...(hidden?{notify:'MUTED'}:{notify:'MENTIONS'})})} auth={auth} organization={org} project={selected} categories={categories} onOpenAdmin={staff?()=>setAdminOpen(true):undefined} selected={selectedChannel} onSelect={channel=>{setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null);setChannelUnread(current=>({...current,[channel.id]:0}))}} onChannelCreated={(categoryId,channel)=>setCategories(items=>items.map(category=>category.id===categoryId&&!category.channels.some(item=>item.id===channel.id)?{...category,channels:[...category.channels,channel]}:category))} onChannelDeleted={channelId=>{setCategories(items=>items.map(category=>({...category,channels:category.channels.filter(channel=>channel.id!==channelId)})));setSelectedChannel(current=>current?.id===channelId?null:current)}} onCategoryCreated={category=>setCategories(items=>items.some(item=>item.id===category.id)?items:[...items,category])} onCategoryDeleted={categoryId=>setCategories(items=>items.filter(category=>category.id!==categoryId))}/>}{splitTarget?<div className="split-stage">{pane(mainTarget,false)}{pane(splitTarget,true)}</div>:pane(mainTarget,false)}{creating&&<ProjectCreator auth={auth} organization={organization} onClose={()=>setCreating(false)} onCreated={created}/>}{projectContext&&<div className="channel-context-layer" onMouseDown={()=>setProjectContext(null)}><div className="channel-context-menu project-context-menu" style={{left:projectContext.x,top:projectContext.y}} onMouseDown={e=>e.stopPropagation()}><button onClick={()=>{setDeletingProject(projectContext.project);setProjectContext(null)}}>Projeyi sil</button></div></div>}<CallOverlay me={auth.user.id} people={people} onOpenPerson={userId=>{const person=people.find(item=>item.id===userId);if(person)setProfileRequest(person)}}/>{removingMember&&<ConfirmDialog title="Gruptan çıkar" body={`${removingMember.name} "${removingMember.group.name}" grubundan çıkarılacak. Diğer üyeler bunu görecek.`} confirmLabel="Gruptan çıkar" danger onCancel={()=>setRemovingMember(null)} onConfirm={()=>{void removeGroupMember(removingMember.group.id,removingMember.userId);setRemovingMember(null)}}/>}{leavingGroup&&<ConfirmDialog title="Gruptan ayrıl" body={`"${leavingGroup.name}" grubundan ayrılacaksın. Diğer üyeler ayrıldığını görecek.`} confirmLabel="Gruptan ayrıl" danger onCancel={()=>setLeavingGroup(null)} onConfirm={()=>{void leaveGroup(leavingGroup);setLeavingGroup(null)}}/>}{activityOpen&&<ActivityPanel items={activity} loading={activityLoading} onClose={()=>setActivityOpen(false)} onOpen={item=>void openActivity(item)} onReadAll={()=>void readAllActivity()}/>}{creatingGroup&&<GroupCreator people={people} busy={groupBusy} error={groupError} onClose={()=>setCreatingGroup(false)} onCreate={(name,userIds)=>void createGroup(name,userIds)}/>}{gitOpen&&<div className="git-layer" onMouseDown={event=>{if(event.target===event.currentTarget)setGitOpen(false)}}><GitPanel onClose={()=>setGitOpen(false)}/></div>}{adminOpen&&<AdminPanel auth={auth} organization={org} perms={my} onClose={()=>setAdminOpen(false)} onOrganizationRenamed={setOrgName}/>}{settingsOpen&&<SettingsModal auth={auth} preference={preference} onPresence={choosePresence} onLogout={()=>{setSettingsOpen(false);logout()}} onClose={()=>setSettingsOpen(false)}/>}{deletingProject&&<DeleteProjectDialog auth={auth} organization={organization} project={deletingProject} onClose={()=>setDeletingProject(null)} onDeleted={()=>projectDeleted(deletingProject.id)}/>}</main>
  return <main className="launcher"><aside className="sidebar"><div className="side-brand"><span className="logo">P2</span><div><b>{organization.name}</b><small>{organization.role}</small></div></div><p className="section-title">PROJELER</p><nav>{projects.map(project=><button className={selected?.id===project.id?'active':''} key={project.id} onClick={()=>setSelected(project)}><span className="project-glyph">◆</span><span>{project.name}<small>{engineNames[project.engine||'']||'Proje'}</small></span></button>)}{loading&&<div className="empty-small">Projeler yükleniyor…</div>}</nav><button className="new-project" onClick={()=>setCreating(true)}>＋ Yeni proje</button></aside><section className="workspace"><header><div><p className="eyebrow">{organization.name.toUpperCase()}</p><h2>{selected?.name||'Projeler'}</h2></div><div className="profile"><span>{userInitials(auth.user.displayName)}</span><div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div><button onClick={logout}>Çıkış</button></div></header>{!loading&&!projects.length?<div className="first-project"><div className="tutorial-badge">01 · BAŞLANGIÇ</div><span className="project-orb">◆</span><h1>İlk projeni oluştur.</h1><p>Oyununun build’lerini yüklemek, sürümlerini yönetmek ve ekibinle test etmek için bir proje alanı aç.</p><div className="tutorial-steps"><div><b>1</b><span><strong>Projeyi tanımla</strong><small>Adını ve kullandığın motoru seç.</small></span></div><i>→</i><div><b>2</b><span><strong>Build yükle</strong><small>İlk oynanabilir sürümünü paylaş.</small></span></div><i>→</i><div><b>3</b><span><strong>Ekibini çağır</strong><small>Birlikte indirip test edin.</small></span></div></div><button className="first-project-button" onClick={()=>setCreating(true)}>＋ İlk projeni oluştur <span>→</span></button></div>:selected?<div className="project-home"><div className="project-hero"><div><p className="eyebrow">PROJE ÇALIŞMA ALANI</p><h1>{selected.name}</h1><p>{selected.description||'Bu proje için henüz bir açıklama eklenmedi.'}</p></div><span>{engineNames[selected.engine||'']||'Özel proje'}</span></div><div className="project-empty-build"><span>⬡</span><h3>Henüz build yok</h3><p>Bir sonraki adımda ilk sürümünü buraya yükleyebileceksin.</p><button disabled>Build yükleme yakında</button></div></div>:null}</section><aside className="friends"><header><b>EKİP</b><span>{friends.filter(f=>f.status==='ONLINE').length} çevrimiçi</span></header>{friends.map(friend=><div className="friend" key={friend.id}><i className={friend.status.toLowerCase()}/><span><b>{friend.displayName}</b><small>{friend.status==='ONLINE'?'Çevrimiçi':'Çevrimdışı'}</small></span></div>)}{!friends.length&&<div className="friends-empty">Ekip üyeleri burada görünecek.</div>}<button className="friends-add">＋ Ekip üyesi davet et</button></aside>{creating&&<ProjectCreator auth={auth} organization={organization} onClose={()=>setCreating(false)} onCreated={created}/>}</main>
}

// Arama katmanı tüm çalışma alanını sarar: gelen arama ve aktif arama paneli her ekranda görünür.
export default function Workspace(props:{auth:Auth;organization:Organization;logout:()=>void}){
  return <CallProvider auth={props.auth} organizationId={props.organization.id}>
    <WorkspaceBody {...props}/>
  </CallProvider>
}
