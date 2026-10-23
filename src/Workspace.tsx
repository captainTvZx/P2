import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import ContextLayer from './ui/ContextLayer'
import Modal from './ui/Modal'
import Logo from './ui/Logo'
import type { Organization } from './App'
import { API_BASE, p2Fetch } from './apiBase'
import ChannelContent from './ChannelContent'
import ChannelBrowser from './ChannelBrowser'
import AdminPanel from './AdminPanel'
import BoardPage from './BoardPage'
import DocumentsPage from './DocumentsPage'
import { useMyPermissions, type PermissionApi } from './permissions'
import { popoutTarget } from './popout'
import CallOverlay from './CallOverlay'
import { CallProvider, OverlayCallProvider, useCall, type CallParticipant } from './CallProvider'
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
import { MessageMenuPopup } from './messageMenu'
import LibraryPage from './LibraryPage'
import ProjectSettingsDialog, { imageSrc } from './ProjectSettingsDialog'
import { useLibraryAuth, useLibraryAutoUpdate } from './library'
import { setActivity as setMyActivity, activityText } from './activity'
import { pushToast } from './Toasts'
import { useOverlayMirror } from './overlayMirror'
import { useOverlayNotifications } from './overlayNotify'


type Auth={user:{id:string;username:string;displayName:string;email?:string;title?:string|null;avatarUrl?:string|null};accessToken:string}
type Project={id:string;name:string;slug:string;description?:string|null;engine?:string|null;ownerId:string;defaultChannelId?:string|null;coverUrl?:string|null;iconUrl?:string|null;createdAt:string;updatedAt:string}
type Friend={id:string;displayName:string;status:string}
type Channel={id:string;name:string;topic?:string|null;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';position:number;unread?:number}
// Ana alanda ve bölünmüş görünümde gösterilebilecek sohbetler.
type PaneTarget={kind:'channel';channel:Channel|null}|{kind:'person';person:ProfileMember}|{kind:'group';group:ChatGroup}
type ChannelCategory={id:string;name:string;position:number;channels:Channel[]}
const API=API_BASE
async function request<T>(path:string,token:string,init?:RequestInit):Promise<T>{const response=await p2Fetch(API+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}});const data=await response.json().catch(()=>({}));if(!response.ok)throw data;return data}
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

// Eski ve yeni bütün sağ tık menülerini gerçek çizilmiş boyutlarıyla ekranın
// içinde tutar. Özellikle listenin son satırında menü aşağı sığmıyorsa menüyü
// tıklanan noktanın üstüne kaydırır; sabit satır/yükseklik tahmini kullanmaz.
function ContextMenuViewportGuard(){
  useEffect(()=>{
    const gap=8
    let frame=0
    const observed=new WeakSet<Element>()
    const resize=new ResizeObserver(()=>schedule())
    const place=()=>{
      frame=0
      document.querySelectorAll<HTMLElement>('.channel-context-menu,.p2-menu').forEach(menu=>{
        if(!observed.has(menu)){observed.add(menu);resize.observe(menu)}
        const box=menu.getBoundingClientRect()
        let left=box.left
        let top=box.top
        if(box.right>window.innerWidth-gap)left-=box.right-(window.innerWidth-gap)
        if(box.bottom>window.innerHeight-gap)top-=box.bottom-(window.innerHeight-gap)
        left=Math.max(gap,left)
        top=Math.max(gap,top)
        if(Math.abs(left-box.left)>.5)menu.style.left=`${left}px`
        if(Math.abs(top-box.top)>.5)menu.style.top=`${top}px`
      })
    }
    function schedule(){if(frame)return;frame=window.requestAnimationFrame(place)}
    const mutations=new MutationObserver(schedule)
    mutations.observe(document.body,{childList:true,subtree:true})
    window.addEventListener('resize',schedule)
    schedule()
    return()=>{if(frame)window.cancelAnimationFrame(frame);mutations.disconnect();resize.disconnect();window.removeEventListener('resize',schedule)}
  },[])
  return null
}

function ProjectCreator({auth,organization,onClose,onCreated}:{auth:Auth;organization:Organization;onClose:()=>void;onCreated:(project:Project)=>void}){
  const[name,setName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
  const slug=useMemo(()=>name.toLocaleLowerCase('tr-TR').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/ı/g,'i').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'').slice(0,40),[name])
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const values=Object.fromEntries(new FormData(e.currentTarget));try{onCreated(await request<Project>(`/organizations/${organization.id}/projects`,auth.accessToken,{method:'POST',body:JSON.stringify({name,slug,description:values.description})}))}catch(x:any){setError(x.message||'Proje oluşturulamadı.');setBusy(false)}}
  return <Modal size="md" kicker="YENİ PROJE" title="Projenin temelini atalım." subtitle="Bu bilgileri daha sonra proje ayarlarından değiştirebilirsin." onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="submit" form="project-create-form" className={busy?'p2-btn primary is-busy':'p2-btn primary'} disabled={busy||slug.length<2}>{busy?'Proje hazırlanıyor…':'Projeyi oluştur'} <span>→</span></button></>}><form id="project-create-form" className="p2-form" onSubmit={submit}><label className="p2-field"><span>Proje adı</span><input className="p2-input" value={name} onChange={e=>setName(e.target.value)} minLength={2} maxLength={60} placeholder="Örn. Project Aurora" autoFocus required/></label><label className="p2-field"><span>Proje kodu</span><div className="slug-preview"><span>{organization.slug}/</span>{slug||'proje-kodu'}</div></label><label className="p2-field"><span>Kısa açıklama</span><textarea className="p2-textarea" name="description" maxLength={240} rows={3} placeholder="Ekibine projenin amacını kısaca anlat…"/></label>{error&&<div className="error">{error}</div>}</form></Modal>
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
  })}{menu&&<MessageMenuPopup menu={{x:menu.x,y:menu.y,up:menu.y}} onClose={()=>setMenu(null)} className="channel-context-menu person-context-menu voice-member-menu">
    <button className="plain profile-entry" onClick={()=>{onProfile(people.find(item=>item.id===menu.participant.userId)||self);setMenu(null)}}>Profil</button>
    {menu.participant.userId!==me&&menu.participant.screen&&<button className="plain profile-entry" onClick={()=>{call.watch(menu.participant.userId);setMenu(null)}}>{call.watching.includes(menu.participant.userId)?'Yayını kapat':'Yayını izle'}</button>}
    <hr/>
    {menu.participant.userId===me?<>
      <label className="voice-volume-control"><span>Mikrofon sesi</span><b>{settings.inputVolume}%</b><input type="range" min="0" max="100" value={settings.inputVolume} onChange={event=>updateSettings({inputVolume:Number(event.target.value)})}/></label>
      <button className="check-entry" onClick={call.toggleMute}><span>{call.muted?'Kendimi susturmayı kaldır':'Kendimi sustur'}</span><i className={call.muted?'checked':''}/></button>
      <button className="check-entry" onClick={call.toggleDeafen}><span>{call.deafened?'Sağırlaştırmayı kaldır':'Kendimi sağırlaştır'}</span><i className={call.deafened?'checked':''}/></button>
    </>:<>
      <label className="voice-volume-control"><span>Mikrofon sesi</span><b>{call.userVolumes[menu.participant.userId]??100}%</b><input type="range" min="0" max="100" disabled={call.locallyMuted.has(menu.participant.userId)} value={call.userVolumes[menu.participant.userId]??100} onChange={event=>call.setUserVolume(menu.participant.userId,Number(event.target.value))}/></label>
      <button className="check-entry" onClick={()=>call.toggleLocalMute(menu.participant.userId)}><span>Sustur</span><i className={call.locallyMuted.has(menu.participant.userId)?'checked':''}/></button>
      <button className="check-entry" onClick={()=>call.togglePeerDeafen(menu.participant.userId)}><span>Sağırlaştır</span><i className={call.deafenedFor.has(menu.participant.userId)?'checked':''}/></button>
    </>}
  </MessageMenuPopup>}</div>
}

function ChannelCreator({auth,organization,project,category,onClose,onCreated}:{auth:Auth;organization:Organization;project:Project;category:ChannelCategory;onClose:()=>void;onCreated:(channel:Channel)=>void}){
  const[name,setName]=useState(''),[type,setType]=useState<Channel['type']>('TEXT'),[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');try{const channel=await request<Channel>(`/organizations/${organization.id}/projects/${project.id}/channels`,auth.accessToken,{method:'POST',body:JSON.stringify({categoryId:category.id,name,type})});onCreated(channel)}catch(x:any){setError(x.message||'Kanal oluşturulamadı.');setBusy(false)}}
  return <Modal size="sm" kicker={category.name} title="Kanal oluştur" onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="submit" form="channel-create-form" className={busy?'p2-btn primary is-busy':'p2-btn primary'} disabled={busy||!name.trim()}>{busy?'Oluşturuluyor…':'Kanalı oluştur'}</button></>}><form id="channel-create-form" className="p2-form" onSubmit={submit}><label className="p2-field"><span>Kanal adı</span><div className="channel-name-input"><span>#</span><input value={name} onChange={e=>setName(e.target.value)} minLength={1} maxLength={40} placeholder="yeni-kanal" autoFocus required/></div></label><fieldset className="channel-type-options"><legend>Kanal türü</legend>{channelTypeOptions.map(option=><button type="button" key={option.value} className={type===option.value?'active':''} onClick={()=>setType(option.value)}><span>{channelIcons[option.value]}</span><div><b>{option.label}</b><small>{option.description}</small></div><i/></button>)}</fieldset>{error&&<div className="channel-create-error">{error}</div>}</form></Modal>
}

function DeleteChannelDialog({auth,organization,project,channel,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;channel:Channel;onClose:()=>void;onDeleted:()=>void}){
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}/channels/${channel.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Kanal silinemedi.');setBusy(false)}}
  return <Modal size="sm" className="centered" title={<>#{channel.name} silinsin mi?</>} onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="button" className={busy?'p2-btn danger is-busy':'p2-btn danger'} onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Kanalı sil'}</button></>}><span className="p2-danger-mark">!</span><p>Kanal hemen çalışma alanından kaldırılacak. Mesajlar 30 gün boyunca saklanacak ve ardından geri döndürülemez biçimde kalıcı olarak silinecek.</p>{error&&<div className="channel-create-error">{error}</div>}</Modal>
}

function DeleteCategoryDialog({auth,organization,project,category,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;category:ChannelCategory;onClose:()=>void;onDeleted:()=>void}){
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}/channel-categories/${category.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Kategori silinemedi.');setBusy(false)}}
  return <Modal size="sm" className="centered" title={<>{category.name} kategorisi silinsin mi?</>} onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="button" className={busy?'p2-btn danger is-busy':'p2-btn danger'} onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Kategoriyi sil'}</button></>}><span className="p2-danger-mark">!</span><p>Kategori çalışma alanından kaldırılacak. Daha önce silinen kanallara ait veriler 30 günlük saklama süresi tamamlanana kadar korunacak.</p>{error&&<div className="channel-create-error">{error}</div>}</Modal>
}

function CategoryCreator({auth,organization,project,onClose,onCreated}:{auth:Auth;organization:Organization;project:Project;onClose:()=>void;onCreated:(category:ChannelCategory)=>void}){
  const[name,setName]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');try{onCreated(await request<ChannelCategory>(`/organizations/${organization.id}/projects/${project.id}/channel-categories`,auth.accessToken,{method:'POST',body:JSON.stringify({name})}))}catch(x:any){setError(x.message||'Kategori oluşturulamadı.');setBusy(false)}}
  return <Modal size="sm" kicker="YENİ KATEGORİ" title="Kanallarını grupla" onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="submit" form="category-create-form" className={busy?'p2-btn primary is-busy':'p2-btn primary'} disabled={busy||!name.trim()}>{busy?'Oluşturuluyor…':'Kategoriyi oluştur'}</button></>}><form id="category-create-form" className="p2-form" onSubmit={submit}><label className="p2-field"><span>Kategori adı</span><input className="p2-input" value={name} onChange={e=>setName(e.target.value)} minLength={1} maxLength={32} placeholder="Örn. 🎨 Tasarım & Fikirler" autoFocus required/></label>{error&&<div className="channel-create-error">{error}</div>}</form></Modal>
}

function DeleteProjectDialog({auth,organization,project,onClose,onDeleted}:{auth:Auth;organization:Organization;project:Project;onClose:()=>void;onDeleted:()=>void}){
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  async function remove(){setBusy(true);setError('');try{await request(`/organizations/${organization.id}/projects/${project.id}`,auth.accessToken,{method:'DELETE'});onDeleted()}catch(x:any){setError(x.message||'Proje silinemedi.');setBusy(false)}}
  return <Modal size="sm" className="centered" kicker="PROJEYİ SİL" title={<>{project.name} silinsin mi?</>} onClose={onClose} footer={<><button type="button" className="p2-btn ghost" onClick={onClose}>Vazgeç</button><button type="button" className={busy?'p2-btn danger is-busy':'p2-btn danger'} onClick={remove} disabled={busy}>{busy?'Siliniyor…':'Projeyi sil'}</button></>}><span className="p2-danger-mark">!</span><p>Proje; kategorileri, kanalları ve mesajlarıyla birlikte hemen çalışma alanından kaldırılacak. Tüm veriler 30 gün saklandıktan sonra geri döndürülemez biçimde kalıcı olarak silinecek.</p>{error&&<div className="channel-create-error">{error}</div>}</Modal>
}

export function ChannelSidebar({auth,organization,project,categories,selected,onSelect,onDefaultChannel,onChannelCreated,onChannelDeleted,onCategoryCreated,onCategoryDeleted,people,selectedPerson,unread,channelUnread,onSelectPerson,onViewProfile,onCallPerson,onResize,typing,voiceRooms,voiceLevels,prefs,onStar,onHidden,onOpenAdmin,perms,activities,groups,selectedGroup,onSelectGroup,onCreateGroup,onLeaveGroup}:{groups:ChatGroup[];selectedGroup:ChatGroup|null;onSelectGroup:(group:ChatGroup)=>void;onCreateGroup:()=>void;onLeaveGroup:(group:ChatGroup)=>void;auth:Auth;organization:Organization;project:Project;categories:ChannelCategory[];selected:Channel|null;onSelect:(channel:Channel)=>void;onDefaultChannel:(channelId:string|null)=>void;onChannelCreated:(categoryId:string,channel:Channel)=>void;onChannelDeleted:(channelId:string)=>void;onCategoryCreated:(category:ChannelCategory)=>void;onCategoryDeleted:(categoryId:string)=>void;people:ProfileMember[];selectedPerson:ProfileMember|null;unread:Record<string,number>;channelUnread:Record<string,number>;onSelectPerson:(person:ProfileMember)=>void;onViewProfile:(person:ProfileMember)=>void;activities?:Record<string,{kind:string;projectName:string;detail?:string;percent?:number}|null>;onCallPerson:(userId:string)=>void;onResize:(event:React.MouseEvent)=>void;typing:Set<string>;voiceRooms:Record<string,CallParticipant[]>;voiceLevels:Record<string,number>;prefs:Record<string,{starred:boolean;notify:string;hidden?:boolean}>;onStar:(kind:'CHANNEL'|'PERSON',targetId:string,starred:boolean)=>void;onHidden:(channelId:string,hidden:boolean)=>void;onOpenAdmin?:()=>void;perms:PermissionApi}){
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
  </div>}</section>}{categories.map(category=>{const channels=category.channels.filter(channel=>!prefs[`CHANNEL:${channel.id}`]?.hidden&&(!normalized||channel.name.toLocaleLowerCase('tr-TR').includes(normalized)));if(normalized&&!channels.length)return null;return <section className="channel-category" key={category.id} onContextMenu={e=>{if(category.channels.length)return;e.preventDefault();setContextMenu({x:e.clientX,y:e.clientY,category})}}><header><button className="category-toggle" onClick={()=>toggle(category.id)}><i className={collapsed.has(category.id)?'chevron collapsed':'chevron'}/>{category.name}</button>{perms.can('channel.manage')&&<button className="add-channel" onClick={()=>setCreatingCategory(category)} aria-label={`${category.name} kategorisine kanal ekle`} title="Kanal ekle"><span/></button>}</header>{!collapsed.has(category.id)&&<div>{channels.map(channel=><div className="voice-channel-block"><button key={channel.id} className={selected?.id===channel.id&&!selectedPerson&&!selectedGroup?'channel-row active':'channel-row'} onClick={()=>onSelect(channel)} onContextMenu={e=>{e.preventDefault();setContextMenu({x:e.clientX,y:e.clientY,channel})}}><span className={`channel-type ${channel.type.toLowerCase()}`}>{channelIcons[channel.type]}</span><b>{channel.name}</b><span className="voice-channel-badges">{project.defaultChannelId===channel.id&&<small className="channel-home" title="Başlangıç kanalı: proje açılınca ilk bu kanal gelir">⌂</small>}{channel.type==='VOICE'&&(voiceRooms[channel.id]?.length?<em className="voice-count">{voiceRooms[channel.id].length}</em>:<small>SES</small>)}{channelUnread[channel.id]>0&&<em className="direct-unread">{channelUnread[channel.id]>99?'99+':channelUnread[channel.id]}</em>}</span></button>{channel.type==='VOICE'&&voiceRooms[channel.id]?.length?<VoiceMembers participants={voiceRooms[channel.id]} people={people} self={{...auth.user,status:'ONLINE',role:organization.role}} levels={voiceLevels} onProfile={onViewProfile}/>:null}</div>)}</div>}</section>})}{!categories.length&&<div className="empty-categories"><b>İlk kategorini oluştur</b><p>Kanallarını düzenlemek için bir kategoriyle başla.</p><button onClick={()=>setCreatingCategoryGroup(true)}>Kategori oluştur</button></div>}<section className="channel-category people-section"><header><button className="category-toggle" onClick={()=>setPeopleCollapsed(value=>!value)}><i className={peopleCollapsed?'chevron collapsed':'chevron'}/>KİŞİLER — {people.length}</button>{peopleCollapsed&&hiddenUnread>0&&<em className="direct-unread">{hiddenUnread>99?'99+':hiddenUnread}</em>}</header>{!peopleCollapsed&&<div>{visiblePeople.map(person=><button key={person.id} className={`person-row${selectedPerson?.id===person.id?' active':''}${unread[person.id]>0?' unread':''}`} onClick={()=>onSelectPerson(person)} onContextMenu={e=>{e.preventDefault();setPersonMenu({x:e.clientX,y:e.clientY,person})}} title={`@${person.username}`}><span className={`person-avatar ${avatarHue(person.id)}`}>{person.avatarUrl?<img src={person.avatarUrl} alt=""/>:userInitials(person.displayName)}<i className={presenceClass(person.status)}/></span><span className="person-name"><b>{person.displayName}</b>{activities?.[person.id]&&<small className="person-activity">{activityText(activities[person.id])}</small>}</span>{typing.has(person.id)?<em className="person-typing"><i/><i/><i/></em>:unread[person.id]>0?<em className="direct-unread">{unread[person.id]>99?'99+':unread[person.id]}</em>:null}</button>)}{!people.length&&<p className="people-empty">Kuruluşta başka kimse yok.</p>}{!!people.length&&!visiblePeople.length&&<p className="people-empty">Eşleşen kişi yok.</p>}</div>}</section><section className="channel-category groups-section"><header><button className="category-toggle" onClick={()=>setGroupsCollapsed(value=>!value)}><i className={groupsCollapsed?'chevron collapsed':'chevron'}/>GRUPLAR — {groups.length}</button><button className="add-channel" title="Grup oluştur" aria-label="Grup oluştur" onClick={onCreateGroup}><span/></button></header>{!groupsCollapsed&&<div>{groups.map(group=><div key={group.id} role="button" tabIndex={0} className={`person-row group-row${selectedGroup?.id===group.id?' active':''}${group.unread>0?' unread':''}`} onClick={()=>onSelectGroup(group)} onKeyDown={event=>{if(event.key==='Enter')onSelectGroup(group)}} title={group.members.map(member=>member.displayName).join(', ')}><span className="group-avatar">{userInitials(group.name)}</span><b>{group.name}</b>{group.unread>0&&<em className="direct-unread">{group.unread>99?'99+':group.unread}</em>}<button type="button" className="group-leave" title="Gruptan ayrıl" onClick={event=>{event.stopPropagation();onLeaveGroup(group)}}>×</button></div>)}{!groups.length&&<p className="people-empty">Henüz grup yok. Sağdaki + ile oluştur ya da bir kanaldan arama başlat.</p>}</div>}</section></div>{creatingCategoryGroup&&<CategoryCreator auth={auth} organization={organization} project={project} onClose={()=>setCreatingCategoryGroup(false)} onCreated={category=>{onCategoryCreated(category);setCreatingCategoryGroup(false)}}/>}{creatingCategory&&<ChannelCreator auth={auth} organization={organization} project={project} category={creatingCategory} onClose={()=>setCreatingCategory(null)} onCreated={channel=>{onChannelCreated(creatingCategory.id,channel);onSelect(channel);setCollapsed(current=>{const next=new Set(current);next.delete(creatingCategory.id);return next});setCreatingCategory(null)}}/>}{contextMenu&&<ContextLayer onMouseDown={()=>setContextMenu(null)}><div className="channel-context-menu" style={{left:contextMenu.x,top:contextMenu.y}} onMouseDown={e=>e.stopPropagation()}>{contextMenu.channel&&<button className="plain" onClick={()=>{onStar('CHANNEL',contextMenu.channel!.id,!prefs[`CHANNEL:${contextMenu.channel!.id}`]?.starred);setContextMenu(null)}}>{prefs[`CHANNEL:${contextMenu.channel!.id}`]?.starred?'Yıldızı kaldır':'Yıldıza ekle'}</button>}{contextMenu.channel&&perms.can('channel.manage')&&<button className="plain" onClick={()=>{onDefaultChannel(project.defaultChannelId===contextMenu.channel!.id?null:contextMenu.channel!.id);setContextMenu(null)}}>{project.defaultChannelId===contextMenu.channel!.id?'Başlangıç kanalı olmaktan çıkar':'Başlangıç kanalı yap'}</button>}{(contextMenu.channel?perms.canIn(contextMenu.channel.id,'channel.manage'):perms.can('channel.manage'))
      ?<button onClick={()=>{if(contextMenu.channel)setDeletingChannel(contextMenu.channel);if(contextMenu.category)setDeletingCategory(contextMenu.category);setContextMenu(null)}}>Sil</button>
      :<button className="plain" disabled>Silme yetkin yok</button>}</div></ContextLayer>}{projectMenu&&<ContextLayer onMouseDown={()=>setProjectMenu(null)}><div className="channel-context-menu invite-menu" style={{left:projectMenu.x-224,top:projectMenu.y}} onMouseDown={e=>e.stopPropagation()}>
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
    </div></ContextLayer>}{personMenu&&<ContextLayer onMouseDown={()=>setPersonMenu(null)}><div className="channel-context-menu person-context-menu" style={{left:Math.min(personMenu.x,window.innerWidth-190),top:Math.min(personMenu.y,window.innerHeight-90)}} onMouseDown={e=>e.stopPropagation()}><button onClick={()=>{onViewProfile(personMenu.person);setPersonMenu(null)}}>Profili görüntüle</button><button onClick={()=>{onSelectPerson(personMenu.person);setPersonMenu(null)}}>Mesaj gönder</button><button onClick={()=>{onCallPerson(personMenu.person.id);setPersonMenu(null)}}>Sesli ara</button><button onClick={()=>{onStar('PERSON',personMenu.person.id,!prefs[`PERSON:${personMenu.person.id}`]?.starred);setPersonMenu(null)}}>{prefs[`PERSON:${personMenu.person.id}`]?.starred?'Yıldızı kaldır':'Yıldıza ekle'}</button></div></ContextLayer>}{deletingChannel&&<DeleteChannelDialog auth={auth} organization={organization} project={project} channel={deletingChannel} onClose={()=>setDeletingChannel(null)} onDeleted={()=>{onChannelDeleted(deletingChannel.id);setDeletingChannel(null)}}/>}{deletingCategory&&<DeleteCategoryDialog auth={auth} organization={organization} project={project} category={deletingCategory} onClose={()=>setDeletingCategory(null)} onDeleted={()=>{onCategoryDeleted(deletingCategory.id);setDeletingCategory(null)}}/>}{browsingChannels&&<ChannelBrowser categories={categories} prefs={prefs} onClose={()=>setBrowsingChannels(false)} onToggle={onHidden} onOpen={channel=>{onSelect(channel);setBrowsingChannels(false)}}/>}</aside>
}

// overlay: oyun içi overlay sohbet penceresi (ray yok, yalnız kanal kenar çubuğu + pane; proje oyunun projesi).
function WorkspaceBody({auth,organization,logout,overlay}:{auth:Auth;organization:Organization;logout:()=>void;overlay?:OverlayChatMode}){
  const callApi=useCall()
  const my=useMyPermissions(auth.accessToken,organization.id)
  useLibraryAuth(auth)
  const[activities,setActivities]=useState<Record<string,{kind:string;projectId?:string;projectName:string;detail?:string;percent?:number;since?:number}|null>>({})
  const sidebar=useResizable('p2:sidebar-width',252,190,430,1)
  const dmTypists=useTypists(auth.accessToken,organization.id,auth.user.id,payload=>!!payload.conversationId)
  const[projects,setProjects]=useState<Project[]>([]),[friends,setFriends]=useState<Friend[]>([]),[selected,setSelected]=useState<Project|null>(null),[loading,setLoading]=useState(true),[creating,setCreating]=useState(false),[projectContext,setProjectContext]=useState<{project:Project;x:number;y:number}|null>(null),[deletingProject,setDeletingProject]=useState<Project|null>(null),[profileOpen,setProfileOpen]=useState(false),[people,setPeople]=useState<ProfileMember[]>([]),[selectedPerson,setSelectedPerson]=useState<ProfileMember|null>(null),[unread,setUnread]=useState<Record<string,number>>({}),[channelUnread,setChannelUnread]=useState<Record<string,number>>({}),[profileRequest,setProfileRequest]=useState<ProfileUser|null>(null),[pendingChannel,setPendingChannel]=useState<string|null>(popoutTarget?.kind==='channel'?popoutTarget.id:null),[pendingDocument,setPendingDocument]=useState<string|null>(null),[pendingTask,setPendingTask]=useState<{boardId:string|null;taskId:string}|null>(null),[splitTarget,setSplitTarget]=useState<PaneTarget|null>(null),[prefs,setPrefs]=useState<Record<string,{starred:boolean;notify:string;hidden?:boolean}>>({}),[settingsOpen,setSettingsOpen]=useState(false),[settingsSection,setSettingsSection]=useState('media'),[projectSettingsFor,setProjectSettingsFor]=useState<Project|null>(null),[adminOpen,setAdminOpen]=useState(false),[libraryOpen,setLibraryOpen]=useState(false),[boardOpen,setBoardOpen]=useState(false),[documentsOpen,setDocumentsOpen]=useState(false),[orgName,setOrgName]=useState(organization.name),[inviteCode,setInviteCode]=useState(organization.inviteCode),[preference,setPreference]=useState('ONLINE'),[myStatus,setMyStatus]=useState('ONLINE')
  // Sol raydaki proje rozetleri: proje başına okunmamış mesaj sayısı. Seçili projenin
  // sayısı kanal listesinden türetilir, diğerleri sunucudan gelir ve canlı olaylarla artar.
  const[projectUnread,setProjectUnread]=useState<Record<string,number>>({})
  const unreadProject=useRef('')
  useEffect(()=>{request<Record<string,number>>(`/organizations/${organization.id}/projects/unread`,auth.accessToken).then(setProjectUnread).catch(()=>{})},[auth.accessToken,organization.id])
  useEffect(()=>{
    if(!selected||unreadProject.current!==selected.id)return
    const total=Object.values(channelUnread).reduce((sum,value)=>sum+(value||0),0)
    setProjectUnread(current=>current[selected.id]===total?current:{...current,[selected.id]:total})
  },[channelUnread,selected?.id])
  const projectOrderKey=`p2:project-order:${organization.id}:${auth.user.id}`
  const applyProjectOrder=useCallback((items:Project[])=>{try{const saved=JSON.parse(localStorage.getItem(projectOrderKey)||'[]') as string[],rank=new Map(saved.map((id,index)=>[id,index]));return items.map((item,index)=>({item,index})).sort((a,b)=>(rank.get(a.item.id)??saved.length+a.index)-(rank.get(b.item.id)??saved.length+b.index)).map(row=>row.item)}catch{return items}},[projectOrderKey])
  const persistProjectOrder=useCallback((items:Project[])=>{try{localStorage.setItem(projectOrderKey,JSON.stringify(items.map(item=>item.id)))}catch{}},[projectOrderKey])
  const lastViewKey=`p2:last-view:${organization.id}`
  const lastView=useRef<{kind:'channel'|'person'|'group';id:string;projectId?:string}|null>(null)
  if(lastView.current===null&&!popoutTarget){
    try{const raw=window.localStorage.getItem(lastViewKey);lastView.current=raw?JSON.parse(raw):null}catch{lastView.current=null}
  }
  const rememberView=useCallback((value:{kind:'channel'|'person'|'group';id:string;projectId?:string}|null)=>{
    try{value?window.localStorage.setItem(lastViewKey,JSON.stringify(value)):window.localStorage.removeItem(lastViewKey)}catch{}
  },[lastViewKey])
  const[categories,setCategories]=useState<ChannelCategory[]>([]),[selectedChannel,setSelectedChannel]=useState<Channel|null>(null)
  // Çalışma saatleri: saat dışında tercih otomatik "Uzakta/Meşgul"a alınır ve bu OTOMATİK atama
  // yerelde işaretlenir; saat içine girince yalnız o işaret hâlâ geçerliyse (kullanıcı elle
  // Meşgul/Uzakta/Görünmez seçmediyse) Çevrimiçi'ye dönülür. AFK ise sunucu idle ile Uzakta gösterir.
  const preferenceRef=useRef(preference);preferenceRef.current=preference
  useEffect(()=>{
    if(overlay)return // çalışma saati otomatiği yalnız ana pencerede
    const KEY='p2:autoPresence'
    const readAuto=()=>{try{return window.localStorage.getItem(KEY)||''}catch{return ''}}
    const writeAuto=(value:string)=>{try{value?window.localStorage.setItem(KEY,value):window.localStorage.removeItem(KEY)}catch{}}
    const evaluate=()=>{
      const value=getSettings(),now=new Date(),minutes=now.getHours()*60+now.getMinutes(),[sh,sm]=value.workStart.split(':').map(Number),[eh,em]=value.workEnd.split(':').map(Number),outside=!value.workDays.includes(now.getDay())||minutes<(sh*60+sm)||minutes>=(eh*60+em)
      const auto=readAuto(),current=preferenceRef.current
      if(outside){
        const next=value.outsideHoursStatus
        if(next==='none'){if(auto&&current===auto){emitRealtime('presence:prefer',{status:'ONLINE'})}if(auto)writeAuto('');return}
        // Yalnız Çevrimiçi'deyken ya da önceki otomatik atamayı güncellerken müdahale et.
        if(next!==current&&(current==='ONLINE'||current===auto)){writeAuto(next);emitRealtime('presence:prefer',{status:next})}
        else if(next===current&&!auto){/* kullanıcı zaten elle aynı durumda; dokunma */}
      }else if(auto){
        if(current===auto)emitRealtime('presence:prefer',{status:'ONLINE'})
        writeAuto('')
      }
    }
    evaluate();const timer=window.setInterval(evaluate,60_000),unsubscribe=subscribeSettings(evaluate)
    return()=>{window.clearInterval(timer);unsubscribe()}
  },[])
  const[groups,setGroups]=useState<ChatGroup[]>([]),[selectedGroup,setSelectedGroup]=useState<ChatGroup|null>(null)
  // ---- Oyun içi overlay: kimlik/anlık görüntü aynası + bildirim yönlendirici ----
  // Overlay penceresinden gelen "uygulamada aç" istekleri buradaki mevcut seçicilere bağlanır.
  const appSettings=useSettings()
  const openProjectView=useCallback((projectId:string|undefined,view:'library'|'board'|'chat')=>{
    const project=projectId?projects.find(item=>item.id===projectId):selected
    if(project&&project.id!==selected?.id)setSelected(project)
    setLibraryOpen(view==='library');setBoardOpen(view==='board');setDocumentsOpen(false)
  },[projects,selected])
  useOverlayMirror({
    auth,organization,callApi,people,activities,voiceRooms:callApi.voiceRooms,projects,selectedProjectId:selected?.id||null,unread,channelUnread,prefs,
    channels:categories.flatMap(category=>category.channels.map(channel=>({id:channel.id,name:channel.name,type:channel.type,projectId:selected?.id||''}))),
    openers:{
      person:id=>{const person=people.find(item=>item.id===id);if(person){openProjectView(undefined,'chat');openPerson(person)}},
      channel:(id,projectId)=>{
        openProjectView(projectId,'chat');setSelectedPerson(null);setSelectedGroup(null)
        const channel=categories.flatMap(category=>category.channels).find(item=>item.id===id)
        if(channel&&(!projectId||projectId===selected?.id))setSelectedChannel(channel);else setPendingChannel(id)
      },
      group:id=>{const group=groups.find(item=>item.id===id);if(group){openProjectView(undefined,'chat');setSelectedGroup(group);setSelectedPerson(null)}},
      task:(taskId,boardId,projectId)=>{openProjectView(projectId,'board');setPendingTask({boardId:boardId||null,taskId})},
      profile:id=>{const person=people.find(item=>item.id===id);if(person)setProfileRequest(person)},
      settings:section=>{setSettingsSection(section||'library');setSettingsOpen(true)},
      library:projectId=>openProjectView(projectId,'library'),
      overview:projectId=>openProjectView(projectId,'library'),
      url:url=>{if(/^https?:\/\//i.test(url))window.open(url,'_blank','noopener')},
    },
  })
  const overlayNotify=useOverlayNotifications({
    me:auth.user.id,
    settings:{overlayNotifications:appSettings.overlayNotifications,playToasts:appSettings.playToasts},
    people,activities,callApi,
    projectName:projectId=>projects.find(item=>item.id===projectId)?.name,
  })
  const refreshProjectPeople=useCallback(async()=>{
    if(!selected){setPeople([]);setSelectedPerson(null);return}
    const items=await request<ProfileMember[]>(`/organizations/${organization.id}/members?projectId=${encodeURIComponent(selected.id)}`,auth.accessToken)
    const visible=items.filter(item=>item.id!==auth.user.id)
    setPeople(visible)
    setSelectedPerson(current=>current&&!visible.some(item=>item.id===current.id)?null:current)
    setSplitTarget(current=>current?.kind==='person'&&!visible.some(item=>item.id===current.person.id)?null:current)
  },[auth.accessToken,auth.user.id,organization.id,selected?.id])
  const refreshProjectGroups=useCallback(async()=>{
    if(!selected){setGroups([]);setSelectedGroup(null);return}
    const items=await request<ChatGroup[]>(`/organizations/${organization.id}/groups?projectId=${encodeURIComponent(selected.id)}`,auth.accessToken)
    setGroups(items)
    setSelectedGroup(current=>current&&!items.some(item=>item.id===current.id)?null:current)
    setSplitTarget(current=>current?.kind==='group'&&!items.some(item=>item.id===current.group.id)?null:current)
  },[auth.accessToken,organization.id,selected?.id])
  useEffect(()=>{
    // Proje değişirken önce eski projenin kişi/gruplarını kapat; ağ yanıtı gelene
    // kadar önceki projeden içerik kısa süreliğine bile görünmesin.
    setPeople([]);setGroups([]);setSelectedPerson(null);setSelectedGroup(null)
    setSplitTarget(current=>current?.kind==='person'||current?.kind==='group'?null:current)
    void refreshProjectPeople().catch(()=>setPeople([]))
    void refreshProjectGroups().catch(()=>setGroups([]))
  },[refreshProjectPeople,refreshProjectGroups])
  useEffect(()=>{
    if(popoutTarget)return
    if(selectedPerson)rememberView({kind:'person',id:selectedPerson.id})
    else if(selectedGroup)rememberView({kind:'group',id:selectedGroup.id})
    else if(selectedChannel)rememberView({kind:'channel',id:selectedChannel.id,projectId:selected?.id})
  },[selectedPerson?.id,selectedGroup?.id,selectedChannel?.id,selected?.id,rememberView])
  // Listeler yüklendikçe son ekran geri açılır; bir kez uygulanır.
  const restored=useRef(false)
  useEffect(()=>{
    const target=lastView.current
    if(restored.current||!target||popoutTarget)return
    // Projenin başlangıç kanalı tanımlıysa açılışta o gelir; son bakılan ekran geri yüklenmez.
    if(selected?.defaultChannelId){restored.current=true;return}
    if(target.kind==='person'){const person=people.find(item=>item.id===target.id);if(person){restored.current=true;setSelectedPerson(person);setSelectedGroup(null)}return}
    if(target.kind==='group'){const group=groups.find(item=>item.id===target.id);if(group){restored.current=true;setSelectedGroup(group);setSelectedPerson(null)}return}
    if(!categories.length)return
    restored.current=true
    const channel=categories.flatMap(category=>category.channels).find(item=>item.id===target.id)
    if(channel)setSelectedChannel(channel)
  },[people,groups,categories])

  // Pencere geri açıldığında ekran değişmemeli. Bazı arka plan olayları (kanal
  // listesi tazeleme, arama durumu) geri dönüşte seçimi kendiliğinden
  // değiştirebiliyordu; kısa bir süre boyunca açık ekran korunur ve kullanıcı
  // bir şeye dokunduğu an koruma bırakılır.
  const shownView=useRef({channel:null as string|null,person:null as string|null,group:null as string|null})
  const guardUntil=useRef(0)
  useEffect(()=>{
    const current={channel:selectedChannel?.id||null,person:selectedPerson?.id||null,group:selectedGroup?.id||null}
    const guard=shownView.current
    if(Date.now()<guardUntil.current&&(current.channel!==guard.channel||current.person!==guard.person||current.group!==guard.group)){
      if(guard.person){const person=people.find(item=>item.id===guard.person);if(person){setSelectedPerson(person);setSelectedGroup(null);return}}
      if(guard.group){const group=groups.find(item=>item.id===guard.group);if(group){setSelectedGroup(group);setSelectedPerson(null);return}}
      if(guard.channel){
        const channel=categories.flatMap(category=>category.channels).find(item=>item.id===guard.channel)
        if(channel){setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null);return}
      }
    }
    shownView.current=current
  },[selectedChannel?.id,selectedPerson?.id,selectedGroup?.id,people,groups,categories])
  useEffect(()=>{
    // Koruma yalnızca pencere simge durumundan/tepsiden geri geldiğinde başlar.
    // Pencereye tıklayarak odaklanmak korumayı başlatmaz; yoksa o tıklamayla
    // yapılan kanal seçimi geri alınıyordu.
    const release=()=>{guardUntil.current=0}
    const hold=()=>{guardUntil.current=Date.now()+2500}
    const stop=window.p2?.windowControls?.onRestored?.(hold)
    // Dışarıdan dosya sürüklemek de bir etkileşimdir: pencere öne gelince
    // koruma devreye girip açık sohbeti eski ekranla (çoğunlukla sesli oda)
    // değiştiriyordu, dosya bırakılacak yer gözden kayboluyordu.
    const events=['pointerdown','keydown','wheel','dragenter','dragover','drop']
    events.forEach(name=>window.addEventListener(name,release,true))
    return()=>{
      stop?.()
      events.forEach(name=>window.removeEventListener(name,release,true))
    }
  },[])
  const[activity,setActivity]=useState<ActivityItem[]>([]),[activityOpen,setActivityOpen]=useState(false),[activityLoading,setActivityLoading]=useState(true)
  const[jumpRequest,setJumpRequest]=useState<{messageId:string;parentId:string|null}|null>(null)
  const activityUnread=activity.filter(item=>!item.readAt).length
  const libraryStore=useLibraryAutoUpdate(auth,organization.id,projects,overlayNotify.notify)
  const libraryBadge=Object.values(libraryStore.jobs).filter(job=>job.active).length
  // Ne yaptığımı ekibe bildir: oyun oynuyorsam ana süreç bildirir; burada "hangi
  // projede/görünümde çalışıyorum" bilgisi gönderilir.
  useEffect(()=>{
    // Oyun açıkken ekip "X oynuyor" görür; kapanınca hangi görünümde olduğuma dönülür.
    if(libraryStore.running.length){const runningId=libraryStore.running[0],game=projects.find(item=>item.id===runningId);if(game)setMyActivity({kind:'playing',projectId:game.id,projectName:game.name});return}
    if(!selected){setMyActivity(null);return}
    const detail=libraryOpen?'Kütüphane':boardOpen?'Görev panosu':documentsOpen?'Dokümanlar':selectedGroup?selectedGroup.name:selectedPerson?undefined:selectedChannel?(selectedChannel.type==='VOICE'?`${selectedChannel.name} (sesli)`:`#${selectedChannel.name}`):undefined
    setMyActivity({kind:'working',projectId:selected.id,projectName:selected.name,detail})
  },[selected?.id,selected?.name,selectedChannel?.id,selectedChannel?.name,selectedChannel?.type,selectedPerson?.id,selectedGroup?.id,selectedGroup?.name,boardOpen,documentsOpen,libraryOpen,libraryStore.running.length])
  // "Çalıştır"a basınca çevrimiçi ekibe Steam tarzı duyuru (Ayarlar > Kütüphane'den kapatılır).
  const announced=useRef<Set<string>>(new Set())
  useEffect(()=>{
    const now=new Set(libraryStore.running)
    if(overlay)return // oyun içi pencere duyuru yapmaz (ana pencere zaten yaptı)
    for(const projectId of now){if(announced.current.has(projectId))continue;announced.current.add(projectId);const game=projects.find(item=>item.id===projectId);if(game&&getSettings().announcePlay&&getSettings().shareActivity)emitRealtime('library:announce',{organizationId:organization.id,projectId:game.id,projectName:game.name})}
    for(const projectId of [...announced.current])if(!now.has(projectId))announced.current.delete(projectId)
  },[libraryStore.running.join(','),projects,organization.id])
  useEffect(()=>window.p2?.library?.onPlay(({projectId})=>{const project=projects.find(item=>item.id===projectId);if(project){setSelected(project);setLibraryOpen(true);setBoardOpen(false);setDocumentsOpen(false)}}),[projects])
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
    if(item.documentId){const project=projects.find(row=>row.id===item.projectId);if(project)setSelected(project);setPendingDocument(item.documentId);setDocumentsOpen(true);setBoardOpen(false);return}
    if(item.taskId){const project=projects.find(row=>row.id===item.projectId);if(project)setSelected(project);setPendingTask({boardId:item.boardId,taskId:item.taskId});setBoardOpen(true);setDocumentsOpen(false);return}
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
  useEffect(()=>{if(!documentsOpen||!pendingDocument)return;const id=window.setTimeout(()=>{window.dispatchEvent(new CustomEvent('p2:open-document',{detail:{documentId:pendingDocument}}));setPendingDocument(null)},40);return()=>window.clearTimeout(id)},[documentsOpen,pendingDocument])
  useEffect(()=>{if(!boardOpen||!pendingTask)return;const id=window.setTimeout(()=>{window.dispatchEvent(new CustomEvent('p2:open-task',{detail:pendingTask}));setPendingTask(null)},60);return()=>window.clearTimeout(id)},[boardOpen,pendingTask])
  const[groupMessages,setGroupMessages]=useState<GroupMessage[]>([]),[splitGroupMessages,setSplitGroupMessages]=useState<GroupMessage[]>([]),[creatingGroup,setCreatingGroup]=useState(false),[leavingGroup,setLeavingGroup]=useState<ChatGroup|null>(null),[removingMember,setRemovingMember]=useState<{group:ChatGroup;userId:string;name:string}|null>(null),[groupBusy,setGroupBusy]=useState(false),[groupError,setGroupError]=useState('')
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
      if(channel){openedCall.current=active.id;setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null)}
    }
  },[callApi.call?.id,callApi.call?.participants.length,groups,people,categories])

  // Kullanıcı bir sohbeti kendi açtığında, süren arama için otomatik açma
  // tamamlanmış sayılır; yoksa arka plan tazelemeleri (presence, üye listesi)
  // ekranı arama odasına geri çekiyordu.
  const callHandled=()=>{openedCall.current=callApi.call?.id||openedCall.current}
  // Grup seçimi ve kişi seçimi birbirini kapatır.
  const openGroup=(group:ChatGroup)=>{callHandled();setSelectedGroup(group);setSelectedPerson(null)}
  async function createGroup(name:string,userIds:string[]){
    setGroupBusy(true);setGroupError('')
    try{
      if(!selected)throw new Error('Önce bir proje seç.')
      const group=await request<ChatGroup>(`/organizations/${organization.id}/groups`,auth.accessToken,{method:'POST',body:JSON.stringify({projectId:selected.id,name:name||undefined,userIds})})
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
  const refreshProjects=useCallback(async(initial=false)=>{
    const items=await request<Project[]>(`/organizations/${organization.id}/projects`,auth.accessToken)
    const ordered=applyProjectOrder(items);setProjects(ordered)
    // Yalnız popout penceresi açılışta hedef projeyi seçer; sonraki tazelemelerde (ör. uzun
    // aradan sonra jeton yenilenince) açık proje korunur, ilk projeye atlanmaz.
    const targetProjectId=popoutTarget?.projectId||overlay?.projectId||''
    setSelected(current=>ordered.find(item=>item.id===(initial&&targetProjectId?targetProjectId:current?.id))||ordered[0]||null)
  },[auth.accessToken,organization.id,popoutTarget?.projectId,overlay?.projectId,applyProjectOrder])
  useEffect(()=>{void refreshProjects(true).finally(()=>setLoading(false))},[refreshProjects])
  useEffect(()=>{
    const rail=document.querySelector<HTMLElement>('.project-stage .rail-projects');if(!rail)return
    let source=''
    const buttons=()=>[...rail.querySelectorAll<HTMLButtonElement>('.rail-project')]
    const clear=()=>buttons().forEach(button=>button.classList.remove('dragging','drag-target'))
    const start=(event:DragEvent)=>{const button=(event.target as HTMLElement).closest<HTMLButtonElement>('.rail-project');if(!button)return;const index=buttons().indexOf(button);source=projects[index]?.id||'';if(!source)return;button.classList.add('dragging');if(event.dataTransfer){event.dataTransfer.effectAllowed='move';event.dataTransfer.setData('text/plain',source)}}
    const over=(event:DragEvent)=>{if(!source)return;event.preventDefault();const button=(event.target as HTMLElement).closest<HTMLButtonElement>('.rail-project');if(!button)return;buttons().forEach(item=>item.classList.toggle('drag-target',item===button&&projects[buttons().indexOf(item)]?.id!==source));if(event.dataTransfer)event.dataTransfer.dropEffect='move'}
    const drop=(event:DragEvent)=>{if(!source)return;event.preventDefault();const button=(event.target as HTMLElement).closest<HTMLButtonElement>('.rail-project'),target=button?projects[buttons().indexOf(button)]?.id:'';if(target&&target!==source)setProjects(items=>{const from=items.findIndex(item=>item.id===source),to=items.findIndex(item=>item.id===target);if(from<0||to<0)return items;const next=[...items],[moved]=next.splice(from,1);next.splice(to,0,moved);persistProjectOrder(next);return next});source='';clear()}
    const end=()=>{source='';clear()}
    buttons().forEach(button=>button.draggable=true);rail.addEventListener('dragstart',start);rail.addEventListener('dragover',over);rail.addEventListener('drop',drop);rail.addEventListener('dragend',end)
    return()=>{rail.removeEventListener('dragstart',start);rail.removeEventListener('dragover',over);rail.removeEventListener('drop',drop);rail.removeEventListener('dragend',end)}
  },[projects,persistProjectOrder])
  // Kanal listesi yetki sürümü değişince de yeniden çekilir. Gelen listede açık
  // kanal yoksa eskiden ilk kanala atlanıyordu; geçici bir eksiklikte ekran
  // kendiliğinden değişiyordu. Artık seçim korunur, gerçek silme/yetki kaybı
  // canlı olaylarla (channel:deleted) ele alınır.
  useEffect(()=>{if(!selected){setCategories([]);setSelectedChannel(null);setChannelUnread({});return}const projectId=selected.id;request<ChannelCategory[]>(`/organizations/${organization.id}/projects/${projectId}/channels`,auth.accessToken).then(items=>{setCategories(items);const all=items.flatMap(item=>item.channels);unreadProject.current=projectId;setChannelUnread(Object.fromEntries(all.filter(channel=>(channel.unread||0)>0).map(channel=>[channel.id,channel.unread||0])));setSelectedChannel(current=>{
      // Sıra: bildirimden/popout'tan gelen hedef → zaten açık kanal → projenin başlangıç
      // kanalı → son bakılan kanal → ilk kanal.
      const pending=all.find(channel=>channel.id===pendingChannel);if(pending)return pending
      const kept=all.find(channel=>channel.id===current?.id);if(kept)return kept
      const home=all.find(channel=>channel.id===selected.defaultChannelId);if(home)return home
      return all.find(channel=>lastView.current?.kind==='channel'&&channel.id===lastView.current.id)||current||all[0]||null
    });setPendingChannel(null)}).catch(()=>{})},[auth.accessToken,organization.id,selected?.id,my.version])
  // Ayrılınan kanal açık kalmasın: tercihler kanallardan sonra yüklendiği için
  // seçim burada da bir kez daha doğrulanır.
  useEffect(()=>{
    if(!selectedChannel||!prefs[`CHANNEL:${selectedChannel.id}`]?.hidden)return
    const visible=categories.flatMap(category=>category.channels).filter(channel=>!prefs[`CHANNEL:${channel.id}`]?.hidden)
    setSelectedChannel(visible[0]||null)
  },[prefs,categories,selectedChannel?.id])
  // Görünen kanal seçildiğinde kalıcı okunmamış işaretini temizle. Bu, ilk
  // açılışta otomatik seçilen kanal ve etkinlikten açılan kanal için de geçerli.
  useEffect(()=>{
    if(!selected||boardOpen||documentsOpen||libraryOpen)return
    if(!selectedPerson&&!selectedGroup&&selectedChannel)markChannelRead(selectedChannel.id)
    if(splitTarget?.kind==='channel'&&splitTarget.channel)markChannelRead(splitTarget.channel.id)
  },[selected?.id,selectedChannel?.id,selectedPerson?.id,selectedGroup?.id,splitTarget?.kind==='channel'?splitTarget.channel?.id:null,boardOpen,documentsOpen])
  useEffect(()=>{
    const readVisible=()=>{
      if(boardOpen||documentsOpen||libraryOpen)return
      if(!selectedPerson&&!selectedGroup&&selectedChannel)markChannelRead(selectedChannel.id)
      if(splitTarget?.kind==='channel'&&splitTarget.channel)markChannelRead(splitTarget.channel.id)
    }
    window.addEventListener('focus',readVisible)
    return()=>window.removeEventListener('focus',readVisible)
  },[selected?.id,selectedChannel?.id,selectedPerson?.id,selectedGroup?.id,splitTarget?.kind==='channel'?splitTarget.channel?.id:null,boardOpen,documentsOpen])
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
      // Oyun içi overlay hazırsa kart oraya gider; masaüstü bildirimi bastırılır (sayaç yine arttı).
      const inOverlay=overlayNotify.notify({type:'dm',from:{id:from,displayName:payload.message.author.displayName,avatarUrl:people.find(item=>item.id===from)?.avatarUrl},content:payload.message.content})
      if(!popoutTarget&&!inOverlay)showMessageNotification(payload.message.author.displayName,payload.message.content,{userId:from})
    },
    'group:created':(payload:{group:ChatGroup})=>{
      if(payload.group.projectId!==selected?.id)return
      setGroups(current=>current.some(item=>item.id===payload.group.id)?current.map(item=>item.id===payload.group.id?payload.group:item):[payload.group,...current])
    },
    'activity:new':(payload:{activity:ActivityItem})=>{
      setActivity(current=>current.some(item=>item.id===payload.activity.id)?current:[payload.activity,...current].slice(0,200))
      const item=payload.activity
      if(item.kind==='ASSIGNED'&&item.taskId)overlayNotify.notify({type:'task-assigned',activityId:item.id,taskId:item.taskId,boardId:item.boardId,projectId:item.projectId,preview:item.preview,actor:{id:item.actor.id,displayName:item.actor.displayName,avatarUrl:people.find(person=>person.id===item.actor.id)?.avatarUrl}})
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
      if(payload.group.projectId!==selected?.id)return
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
      const inOverlay=overlayNotify.notify({type:'group-message',groupId:payload.groupId,groupName:groups.find(item=>item.id===payload.groupId)?.name,author:{id:payload.message.author.id,displayName:payload.message.author.displayName,avatarUrl:people.find(item=>item.id===payload.message.author.id)?.avatarUrl},content:payload.message.content})
      if(!inOverlay)showMessageNotification(payload.message.author.displayName,payload.message.content,{})
    },
    'presence:update':(payload:{userId:string;status:string;activity?:any})=>{setPeople(items=>items.map(item=>item.id===payload.userId?{...item,status:payload.status}:item));setActivities(current=>({...current,[payload.userId]:payload.activity??null}))},
    'presence:activity':(payload:{userId:string;activity:any})=>setActivities(current=>({...current,[payload.userId]:payload.activity??null})),
    'library:playing':(payload:{user:{id:string;username:string;displayName:string;avatarUrl:string|null};projectId:string;projectName:string})=>{
      if(!getSettings().playToasts||payload.user.id===auth.user.id)return
      // Oyun içi overlay hazırsa kart oyunun üstünde görünür; masaüstü toast'ı ve OS bildirimi atlanır.
      if(overlayNotify.notify({type:'play',user:payload.user,projectId:payload.projectId,projectName:payload.projectName}))return
      const open=()=>{const project=projects.find(item=>item.id===payload.projectId);if(project){setSelected(project);setLibraryOpen(true);setBoardOpen(false);setDocumentsOpen(false)}}
      pushToast({kind:'play',user:payload.user,title:payload.user.displayName,text:`${payload.projectName} projesini çalıştırdı`,onClick:open})
      if(!document.hasFocus())showMessageNotification(payload.user.displayName,`${payload.projectName} projesini çalıştırdı`,{projectId:payload.projectId})
    },
    // Kuruluşa katılmak tek başına seçili projede görünmek için yeterli değil.
    'organization:member:joined':()=>{void refreshProjectPeople()},
    'presence:self':(payload:{preference:string;status:string})=>{setPreference(payload.preference);setMyStatus(payload.status)},
    'message:created':(payload:{projectId:string;channelId:string;channelName?:string;message:{content:string;author:{id:string;displayName:string}}})=>{
      if(payload.message.author.id===auth.user.id)return
      const watching=!boardOpen&&!documentsOpen&&!libraryOpen&&document.hasFocus()&&((!selectedPerson&&!selectedGroup&&selectedChannel?.id===payload.channelId)||(splitTarget?.kind==='channel'&&splitTarget.channel?.id===payload.channelId))
      if(!watching)setChannelUnread(current=>({...current,[payload.channelId]:(current[payload.channelId]||0)+1}))
      if(payload.projectId!==selected?.id)setProjectUnread(current=>({...current,[payload.projectId]:(current[payload.projectId]||0)+1}))
      const rule=prefs[`CHANNEL:${payload.channelId}`]?.notify||'MENTIONS'
      const mentioned=mentionsMe(payload.message.content,auth.user.username)
      if(rule==='MUTED'||!mentioned)return
      if(watching)return
      playMessageSound()
      const inOverlay=overlayNotify.notify({type:'channel-message',projectId:payload.projectId,channelId:payload.channelId,channelName:payload.channelName,author:{id:payload.message.author.id,displayName:payload.message.author.displayName,avatarUrl:people.find(item=>item.id===payload.message.author.id)?.avatarUrl},content:payload.message.content,mentioned,rule})
      if(!inOverlay)showMessageNotification(`${payload.message.author.displayName}${payload.channelName?` · #${payload.channelName}`:''}`,payload.message.content,{projectId:payload.projectId,channelId:payload.channelId})
    },
    'organization:invite':(payload:{organizationId:string;inviteCode:string})=>{if(payload.organizationId===organization.id)setInviteCode(payload.inviteCode)},
    'organization:updated':(payload:{organization:{id:string;name:string}})=>{if(payload.organization.id===organization.id)setOrgName(payload.organization.name)},
    // Kuruluştan çıkarıldıysam çalışma alanı artık geçerli değil: baştan yüklenir.
    'organization:removed':(payload:{organizationId:string})=>{if(payload.organizationId===organization.id)window.location.reload()},
    // Kuruluştan çıkarılan kişi listeden hemen düşer; onunla açık sohbet varsa kapanır.
    'organization:member:removed':(payload:{organizationId:string;userId:string})=>{
      if(payload.organizationId!==organization.id||payload.userId===auth.user.id)return
      setPeople(items=>items.filter(item=>item.id!==payload.userId))
      setSelectedPerson(current=>current?.id===payload.userId?null:current)
      setSplitTarget(current=>current?.kind==='person'&&current.person.id===payload.userId?null:current)
      setProfileRequest(current=>current?.id===payload.userId?null:current)
      setUnread(current=>{const next={...current};delete next[payload.userId];return next})
    },
    // Proje olayları herkese yayınlanabilir; listeyi sunucudan tekrar alarak yalnızca
    // bu kişinin erişebildiği projeleri gösteririz.
    'project:created':()=>{void refreshProjects()},
    'project:updated':(payload:{project:Project})=>{setProjects(items=>items.map(item=>item.id===payload.project.id?{...item,...payload.project}:item));setSelected(current=>current?.id===payload.project.id?{...current,...payload.project}:current)},
    'project:permissions':()=>{void refreshProjects()},
    'project:members':(payload:{projectId:string})=>{if(payload.projectId===selected?.id){void refreshProjectPeople();void refreshProjectGroups()}},
    'project:deleted':(payload:{projectId:string})=>setProjects(items=>{const next=items.filter(project=>project.id!==payload.projectId);persistProjectOrder(next);setSelected(current=>current?.id===payload.projectId?(next[0]||null):current);return next}),
    'category:created':(payload:{projectId:string;category:ChannelCategory})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.some(item=>item.id===payload.category.id)?items:[...items,payload.category])},
    'category:deleted':(payload:{projectId:string;categoryId:string})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.filter(item=>item.id!==payload.categoryId))},
    'channel:created':(payload:{projectId:string;categoryId:string;channel:Channel})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>category.id===payload.categoryId&&!category.channels.some(item=>item.id===payload.channel.id)?{...category,channels:[...category.channels,payload.channel]}:category))},
    'channel:updated':(payload:{projectId:string;channel:Channel})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>({...category,channels:category.channels.map(channel=>channel.id===payload.channel.id?{...channel,...payload.channel}:channel)})));setSelectedChannel(current=>current?.id===payload.channel.id?{...current,...payload.channel}:current);setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===payload.channel.id?{kind:'channel',channel:{...current.channel,...payload.channel}}:current)},
    'channel:deleted':(payload:{projectId:string;channelId:string})=>{if(payload.projectId!==selected?.id)return;setCategories(items=>items.map(category=>({...category,channels:category.channels.filter(channel=>channel.id!==payload.channelId)})));setSelectedChannel(current=>current?.id===payload.channelId?null:current);setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===payload.channelId?null:current)},
    'channel:permissions':(payload:{projectId:string;channelId:string;visible:boolean})=>{
      if(payload.projectId!==selected?.id)return
      request<ChannelCategory[]>(`/organizations/${organization.id}/projects/${selected.id}/channels`,auth.accessToken).then(items=>{
        setCategories(items)
        const all=items.flatMap(category=>category.channels)
        setChannelUnread(Object.fromEntries(all.filter(channel=>(channel.unread||0)>0).map(channel=>[channel.id,channel.unread||0])))
        setSelectedChannel(current=>current&&!all.some(channel=>channel.id===current.id)?(all[0]||null):current)
      }).catch(()=>{})
    },
  })
  useEffect(()=>{request<Array<{user:{id:string};unread:number}>>(`/organizations/${organization.id}/direct`,auth.accessToken).then(items=>setUnread(Object.fromEntries(items.filter(item=>item.unread>0).map(item=>[item.user.id,item.unread])))).catch(()=>{})},[auth.accessToken,organization.id])
  useEscape(Boolean(projectContext),()=>setProjectContext(null))
  useEscape(profileOpen,()=>setProfileOpen(false))
  useEffect(()=>{
    if(overlay)return // oyun içi pencere odak/AFK durumunu bildirmez (panel kapalıyken zaten odaksız)
    let timer:number|undefined
    const away=(value:boolean)=>emitRealtime('presence:idle',{away:value})
    const blurred=()=>{window.clearTimeout(timer);timer=window.setTimeout(()=>away(true),AWAY_DELAY)}
    const focused=()=>{window.clearTimeout(timer);away(false)}
    const visibility=()=>{if(document.hidden)blurred();else focused()}
    window.addEventListener('blur',blurred);window.addEventListener('focus',focused);document.addEventListener('visibilitychange',visibility)
    if(!document.hasFocus())blurred()
    return()=>{window.clearTimeout(timer);window.removeEventListener('blur',blurred);window.removeEventListener('focus',focused);document.removeEventListener('visibilitychange',visibility)}
  },[])
  // Elle seçim otomatik işareti siler: çalışma saati mantığı bu seçimin üstüne yazmaz.
  function choosePresence(next:string){setPreference(next);try{window.localStorage.removeItem('p2:autoPresence')}catch{};emitRealtime('presence:prefer',{status:next})}
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
  function openPerson(person:ProfileMember){callHandled();setSelectedPerson(person);setSelectedGroup(null);setUnread(current=>({...current,[person.id]:0}))}
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
  function created(project:Project){setProjects(items=>{const next=items.some(item=>item.id===project.id)?items:[project,...items];persistProjectOrder(next);return next});setSelected(project);setCreating(false)}
  function projectDeleted(projectId:string){setProjects(items=>{const next=items.filter(project=>project.id!==projectId);persistProjectOrder(next);setSelected(current=>current?.id===projectId?(next[0]||null):current);return next});setDeletingProject(null);setCategories([]);setSelectedChannel(null)}
  // Oyun içi overlay: bildirim kartından ya da araç çubuğundan gelen hedef (kanal/kişi/grup) listeler geldikçe seçilir.
  useEffect(()=>{
    const target=overlay?.target
    if(!target)return
    if(target.kind==='channel'){
      const channel=categories.flatMap(category=>category.channels).find(item=>item.id===target.id)
      if(channel){setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null)}else setPendingChannel(target.id)
    }else if(target.kind==='person'){
      const person=people.find(item=>item.id===target.id)
      if(person)openPerson(person)
    }else if(target.kind==='group'){
      const group=groups.find(item=>item.id===target.id)
      if(group){setSelectedGroup(group);setSelectedPerson(null)}
    }
  },[overlay?.target,categories,people,groups])
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
  // Proje başlangıç kanalı: kanal yönetme yetkisi olan herkes değiştirebilir.
  async function setDefaultChannel(channelId:string|null){
    if(!selected)return
    try{
      const project=await request<Project>(`/organizations/${organization.id}/projects/${selected.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({defaultChannelId:channelId})})
      setProjects(items=>items.map(item=>item.id===project.id?{...item,...project}:item))
      setSelected(current=>current?.id===project.id?{...current,...project}:current)
    }catch{}
  }
  function updateChannel(channel:Channel){
    setCategories(items=>items.map(category=>({...category,channels:category.channels.map(item=>item.id===channel.id?{...item,...channel}:item)})))
    setSelectedChannel(current=>current?.id===channel.id?{...current,...channel}:current)
    setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===channel.id?{kind:'channel',channel:{...current.channel,...channel}}:current)
  }
  function markChannelRead(channelId:string){
    setChannelUnread(current=>({...current,[channelId]:0}))
    if(selected)void request(`/organizations/${organization.id}/projects/${selected.id}/channels/${channelId}/read`,auth.accessToken,{method:'POST'}).catch(()=>{})
  }
  // Kanaldan ayrılmak kanalı silmez; yalnızca bu kişinin kenar çubuğundan kaldırır.
  function leaveChannel(channel:Channel){
    markChannelRead(channel.id)
    savePreference('CHANNEL',channel.id,{hidden:true,notify:'MUTED'})
    setSplitTarget(current=>current?.kind==='channel'&&current.channel?.id===channel.id?null:current)
    setSelectedChannel(current=>{
      if(current?.id!==channel.id)return current
      const rest=categories.flatMap(category=>category.channels).filter(item=>item.id!==channel.id&&!prefs[`CHANNEL:${item.id}`]?.hidden)
      return rest[0]||null
    })
  }
  if(loading)return <main className="splash"><Logo size="xl" pop/><p>Projeler kontrol ediliyor…</p></main>
  if(!projects.length)return <main className="first-project-screen"><div className="first-project"><div className="tutorial-badge">01 · BAŞLANGIÇ</div><span className="project-orb">◆</span><h1>İlk projeni oluştur.</h1><p>Ekibini, iletişimini, dosyalarını ve çalışma sürecini tek bir merkezden yönet.</p><div className="tutorial-steps"><div><b>1</b><span><strong>Projeni kur</strong><small>Çalışman için ortak bir merkez oluştur.</small></span></div><i>→</i><div><b>2</b><span><strong>Ekibini yönet</strong><small>Rolleri, görevleri ve iletişimi birleştir.</small></span></div><i>→</i><div><b>3</b><span><strong>Paylaş ve üret</strong><small>Çıktıları ekibinle geliştir ve paylaş.</small></span></div></div>{my.can('project.create')?<button className="first-project-button" onClick={()=>setCreating(true)}>＋ İlk projeni oluştur <span>→</span></button>:<p className="first-project-locked">Henüz erişebildiğin bir proje yok ve proje oluşturma yetkin bulunmuyor. Kuruluş yöneticisinden seni bir projeye eklemesini ya da “Proje oluştur” yetkisi vermesini iste.</p>}</div>{creating&&<ProjectCreator auth={auth} organization={organization} onClose={()=>setCreating(false)} onCreated={created}/>}</main>
  // Bireysel ve grup panelleri de aynı bölünmüş/popout yapısını kullanır.
  function personPane(person:ProfileMember,split:boolean){
    return <DirectConversation key={split?`split-${person.id}`:'main'} auth={auth} organization={organization} person={person} canDeleteAny={my.can('message.delete_unlimited')}
      onCall={()=>void callApi.startDirect(person.id)} inCallWith={callApi.call?.participants.some(participant=>participant.userId===person.id)||false}
      profileRequest={split?null:profileRequest} onProfileHandled={()=>setProfileRequest(null)} onRead={userId=>setUnread(current=>({...current,[userId]:0}))}
      starred={!!prefs[`PERSON:${person.id}`]?.starred} onStar={starred=>savePreference('PERSON',person.id,{starred})}
      onSplit={()=>setSplitTarget({kind:'person',person})} splitPane={split} onCloseSplit={()=>setSplitTarget(null)}/>
  }
  function groupPane(group:ChatGroup,split:boolean){
    const apply=split?setSplitGroupMessages:setGroupMessages
    return <GroupChat key={split?`split-${group.id}`:'main'} auth={auth} organizationId={organization.id} group={group} messages={split?splitGroupMessages:groupMessages} canDeleteAny={my.can('message.delete_unlimited')} people={people}
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
  // Panodaki atama listesi kendini de içerir (kişiler listesi oturum sahibini
  // dışarıda tutuyor, panoda ise kişi kendine görev atayabilmeli).
  // Not: burası erken dönüşlerden (yükleniyor / proje yok) sonra çalışıyor,
  // bu yüzden hook kullanılmaz — useMemo eklendiğinde hook sırası bozulup
  // arayüz hiç çizilmiyordu (React #310).
  const boardPeople=[{...auth.user,status:myStatus,role:organization.role} as ProfileMember,...people]
  const mainTarget:PaneTarget|null=selectedGroup?{kind:'group',group:selectedGroup}:selectedPerson?{kind:'person',person:selectedPerson}:selected?{kind:'channel',channel:selectedChannel}:null

  function popoutTitle(){
    if(selectedGroup)return selectedGroup.name
    if(selectedPerson)return selectedPerson.displayName
    if(selectedChannel)return `#${selectedChannel.name}`
    return selected?.name||'P2'
  }

  // Popout penceresi: rail ve kenar çubuğu olmadan yalnızca seçilen sohbet.
  const popoutReady=popoutTarget?.kind==='person'?!!selectedPerson:popoutTarget?.kind==='group'?!!selectedGroup:!!selectedChannel
  // Oyun içi overlay sohbet penceresi: ray yok; kenar çubuğu + seçili sohbet. Diyaloglar (modal/menü) overlay yüzeyinde açılır.
  if(overlay)return <main className="project-blank project-stage overlay-stage" style={{gridTemplateColumns:`${Math.max(200,Math.min(300,sidebar.width))}px minmax(0,1fr)`}} data-project-title={popoutTitle()}>{selected&&<ChannelSidebar groups={groups} selectedGroup={selectedGroup} onSelectGroup={openGroup} onCreateGroup={()=>{setGroupError('');setCreatingGroup(true)}} onLeaveGroup={group=>setLeavingGroup(group)} people={people} selectedPerson={selectedPerson} unread={unread} channelUnread={channelUnread} onSelectPerson={openPerson} onViewProfile={setProfileRequest} activities={activities} onCallPerson={userId=>void callApi.startDirect(userId)} onResize={sidebar.start} typing={new Set(dmTypists.map(item=>item.userId))} voiceRooms={callApi.voiceRooms} voiceLevels={callApi.levels} perms={my} prefs={prefs} onStar={(kind,targetId,starred)=>savePreference(kind,targetId,{starred})} onHidden={(channelId,hidden)=>savePreference('CHANNEL',channelId,{hidden,...(hidden?{notify:'MUTED'}:{notify:'MENTIONS'})})} auth={auth} organization={org} project={selected} categories={categories} onOpenAdmin={staff?()=>setAdminOpen(true):undefined} selected={selectedChannel} onSelect={channel=>{callHandled();setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null);setChannelUnread(current=>({...current,[channel.id]:0}))}} onDefaultChannel={channelId=>void setDefaultChannel(channelId)} onChannelCreated={(categoryId,channel)=>setCategories(items=>items.map(category=>category.id===categoryId&&!category.channels.some(item=>item.id===channel.id)?{...category,channels:[...category.channels,channel]}:category))} onChannelDeleted={channelId=>{setCategories(items=>items.map(category=>({...category,channels:category.channels.filter(channel=>channel.id!==channelId)})));setSelectedChannel(current=>current?.id===channelId?null:current)}} onCategoryCreated={category=>setCategories(items=>items.some(item=>item.id===category.id)?items:[...items,category])} onCategoryDeleted={categoryId=>setCategories(items=>items.filter(category=>category.id!==categoryId))}/>}{pane(mainTarget,false)||<section className="channel-content-empty"/>}{creatingGroup&&<GroupCreator people={people} busy={groupBusy} error={groupError} onClose={()=>setCreatingGroup(false)} onCreate={(name,userIds)=>void createGroup(name,userIds)}/>}{removingMember&&<ConfirmDialog title="Gruptan çıkar" body={`${removingMember.name} "${removingMember.group.name}" grubundan çıkarılacak. Diğer üyeler bunu görecek.`} confirmLabel="Gruptan çıkar" danger onCancel={()=>setRemovingMember(null)} onConfirm={()=>{void removeGroupMember(removingMember.group.id,removingMember.userId);setRemovingMember(null)}}/>}{leavingGroup&&<ConfirmDialog title="Gruptan ayrıl" body={`"${leavingGroup.name}" grubundan ayrılacaksın. Diğer üyeler ayrıldığını görecek.`} confirmLabel="Gruptan ayrıl" danger onCancel={()=>setLeavingGroup(null)} onConfirm={()=>{void leaveGroup(leavingGroup);setLeavingGroup(null)}}/>}</main>
  if(popoutTarget)return <main className="project-blank project-stage popout-stage" style={{gridTemplateColumns:'minmax(0,1fr)'}} data-project-title={popoutTitle()}>{popoutReady?pane(mainTarget,false):<section className="channel-content-empty"/>}</main>
  if(projects.length)return <main className={callApi.call?'project-blank project-stage in-call':'project-blank project-stage'} style={{gridTemplateColumns:boardOpen||documentsOpen||libraryOpen?'72px minmax(0,1fr)':`72px ${sidebar.width}px minmax(0,1fr)`}} data-project-title={selected?.name||'Proje'}><aside className="project-rail" aria-label="Projeler"><div className="rail-projects">{projects.map((project,index)=><button key={project.id} className={selected?.id===project.id&&!libraryOpen&&!documentsOpen?'rail-project active':'rail-project'} onClick={()=>{setSelected(current=>{/* Aynı projeye tekrar tıklamak sohbete döndürür; başka projeye geçince pano açıksa o projenin panosu gelir. */if(current?.id===project.id){setBoardOpen(false);setDocumentsOpen(false);setLibraryOpen(false)}return project});setSelectedPerson(null)}} onContextMenu={e=>{e.preventDefault();setProjectContext({project,x:e.clientX,y:e.clientY})}} aria-label={project.name} title={project.name}><span className={`rail-avatar tone-${index%6}`}>{project.iconUrl?<img src={imageSrc(project.iconUrl)} alt=""/>:projectInitials(project.name)}</span>{selected?.id!==project.id&&(projectUnread[project.id]||0)>0&&<em className="rail-unread">{projectUnread[project.id]>99?'99+':projectUnread[project.id]}</em>}</button>)}</div><div className="rail-actions"><button className={activityOpen?'rail-action activity active':'rail-action activity'} aria-label="Etkinlik" title="Etkinlik" onClick={()=>setActivityOpen(open=>!open)}><ComposerIcon name="bell"/>{activityUnread>0&&<em>{activityUnread>99?'99+':activityUnread}</em>}</button><button className={boardOpen?'rail-action board active':'rail-action board'} aria-label="Görevler" title={selected?`${selected.name} görev panosu`:'Görevler (pano)'} disabled={!selected} onClick={()=>{setBoardOpen(true);setDocumentsOpen(false);setLibraryOpen(false)}}><ComposerIcon name="board"/></button><button className={documentsOpen?'rail-action documents active':'rail-action documents'} aria-label="Dokümanlar" title={selected?`${selected.name} dokümanları`:'Dokümanlar'} disabled={!selected} onClick={()=>{setDocumentsOpen(true);setBoardOpen(false);setLibraryOpen(false)}}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 3.5h7l4 4V20H7z"/><path d="M14 3.5V8h4M10 12h5M10 15.5h5"/></svg></button><button className={libraryOpen?'rail-action library active':'rail-action library'} aria-label="Kütüphane" title={selected?`${selected.name} kütüphanesi`:'Kütüphane'} disabled={!selected} onClick={()=>{setLibraryOpen(true);setBoardOpen(false);setDocumentsOpen(false)}}><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M12 3v10M8 9l4 4 4-4M5 20h14M5 20v-4M19 20v-4"/></svg>{libraryBadge>0&&<em>{libraryBadge>9?'9+':libraryBadge}</em>}</button>{my.can('project.create')&&<button className="rail-action" aria-label="Yeni proje" title="Yeni proje" onClick={()=>setCreating(true)}>＋</button>}<span className="rail-separator"/><button className={`rail-action rail-profile ${avatarHue(auth.user.id)}${profileOpen?' active':''}`} aria-label="Profilim" title={`${auth.user.displayName} · @${auth.user.username}`} onClick={()=>setProfileOpen(open=>!open)}>{auth.user.avatarUrl?<img src={auth.user.avatarUrl} alt=""/>:userInitials(auth.user.displayName)}<i className={presenceClass(preference==='INVISIBLE'?'INVISIBLE':myStatus)}/></button></div>{profileOpen&&<div className="rail-profile-layer" onMouseDown={()=>setProfileOpen(false)}><section className="rail-profile-card" onMouseDown={e=>e.stopPropagation()}><header><span className={`rail-avatar ${avatarHue(auth.user.id)}`}>{auth.user.avatarUrl?<img src={auth.user.avatarUrl} alt=""/>:userInitials(auth.user.displayName)}</span><div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div></header><p className="rail-profile-status"><i className={presenceClass(preference==='INVISIBLE'?'INVISIBLE':myStatus)}/>{preference==='INVISIBLE'?'Görünmez':statusLabel(myStatus)}</p>
      <div className="rail-status-list">{presenceOptions.map(option=><button key={option.value} type="button" className={preference===option.value?'active':undefined} onClick={()=>choosePresence(option.value)}><i className={presenceClass(option.value)}/>{option.label}</button>)}</div>{staff&&<button className="rail-profile-admin" onClick={()=>{setProfileOpen(false);setAdminOpen(true)}}>🛡 Yönetim paneli<small>{organization.role==='OWNER'?'Sahip':'Yönetici'}</small></button>}<button className="rail-profile-settings" onClick={()=>{setProfileOpen(false);setSettingsOpen(true)}}>⚙ Ayarlar<small>Ctrl + ,</small></button><button className="rail-profile-logout" onClick={logout}>Çıkış yap</button></section></div>}</aside>{documentsOpen&&selected&&<DocumentsPage key={selected.id} auth={auth} organization={organization} project={selected}/>} {boardOpen&&selected&&<BoardPage key={selected.id} auth={auth} organization={organization} project={selected} people={boardPeople} perms={my}/>}{libraryOpen&&selected&&<LibraryPage key={selected.id} auth={auth} organization={organization} project={selected} perms={my} projects={projects} people={people} activities={activities} onSelectProject={p=>{const found=projects.find(x=>x.id===p.id);if(found)setSelected(found)}} onOpenSettings={()=>{setSettingsSection('library');setSettingsOpen(true)}} onOpenBoard={()=>{setBoardOpen(true);setDocumentsOpen(false);setLibraryOpen(false)}}/>}{!boardOpen&&!documentsOpen&&!libraryOpen&&selected&&<ChannelSidebar groups={groups} selectedGroup={selectedGroup} onSelectGroup={openGroup} onCreateGroup={()=>{setGroupError('');setCreatingGroup(true)}} onLeaveGroup={group=>setLeavingGroup(group)} people={people} selectedPerson={selectedPerson} unread={unread} channelUnread={channelUnread} onSelectPerson={openPerson} onViewProfile={setProfileRequest} activities={activities} onCallPerson={userId=>void callApi.startDirect(userId)} onResize={sidebar.start} typing={new Set(dmTypists.map(item=>item.userId))} voiceRooms={callApi.voiceRooms} voiceLevels={callApi.levels} perms={my} prefs={prefs} onStar={(kind,targetId,starred)=>savePreference(kind,targetId,{starred})} onHidden={(channelId,hidden)=>savePreference('CHANNEL',channelId,{hidden,...(hidden?{notify:'MUTED'}:{notify:'MENTIONS'})})} auth={auth} organization={org} project={selected} categories={categories} onOpenAdmin={staff?()=>setAdminOpen(true):undefined} selected={selectedChannel} onSelect={channel=>{callHandled();setSelectedChannel(channel);setSelectedPerson(null);setSelectedGroup(null);setChannelUnread(current=>({...current,[channel.id]:0}))}} onDefaultChannel={channelId=>void setDefaultChannel(channelId)} onChannelCreated={(categoryId,channel)=>setCategories(items=>items.map(category=>category.id===categoryId&&!category.channels.some(item=>item.id===channel.id)?{...category,channels:[...category.channels,channel]}:category))} onChannelDeleted={channelId=>{setCategories(items=>items.map(category=>({...category,channels:category.channels.filter(channel=>channel.id!==channelId)})));setSelectedChannel(current=>current?.id===channelId?null:current)}} onCategoryCreated={category=>setCategories(items=>items.some(item=>item.id===category.id)?items:[...items,category])} onCategoryDeleted={categoryId=>setCategories(items=>items.filter(category=>category.id!==categoryId))}/>}{!boardOpen&&!documentsOpen&&!libraryOpen&&(splitTarget?<div className="split-stage">{pane(mainTarget,false)}{pane(splitTarget,true)}</div>:pane(mainTarget,false))}{creating&&<ProjectCreator auth={auth} organization={organization} onClose={()=>setCreating(false)} onCreated={created}/>}{projectContext&&<ContextLayer onMouseDown={()=>setProjectContext(null)}><div className="channel-context-menu project-context-menu" style={{left:projectContext.x,top:projectContext.y}} onMouseDown={e=>e.stopPropagation()}>{my.can('channel.manage')&&<button onClick={()=>{setProjectSettingsFor(projectContext.project);setProjectContext(null)}}>Proje ayarları…</button>}<button onClick={()=>{setDeletingProject(projectContext.project);setProjectContext(null)}}>Projeyi sil</button></div></ContextLayer>}<CallOverlay me={auth.user.id} people={people} onOpenPerson={userId=>{const person=people.find(item=>item.id===userId);if(person)setProfileRequest(person)}}/>{removingMember&&<ConfirmDialog title="Gruptan çıkar" body={`${removingMember.name} "${removingMember.group.name}" grubundan çıkarılacak. Diğer üyeler bunu görecek.`} confirmLabel="Gruptan çıkar" danger onCancel={()=>setRemovingMember(null)} onConfirm={()=>{void removeGroupMember(removingMember.group.id,removingMember.userId);setRemovingMember(null)}}/>}{leavingGroup&&<ConfirmDialog title="Gruptan ayrıl" body={`"${leavingGroup.name}" grubundan ayrılacaksın. Diğer üyeler ayrıldığını görecek.`} confirmLabel="Gruptan ayrıl" danger onCancel={()=>setLeavingGroup(null)} onConfirm={()=>{void leaveGroup(leavingGroup);setLeavingGroup(null)}}/>}{activityOpen&&<ActivityPanel items={activity} loading={activityLoading} onClose={()=>setActivityOpen(false)} onOpen={item=>void openActivity(item)} onReadAll={()=>void readAllActivity()}/>}{creatingGroup&&<GroupCreator people={people} busy={groupBusy} error={groupError} onClose={()=>setCreatingGroup(false)} onCreate={(name,userIds)=>void createGroup(name,userIds)}/>}{adminOpen&&<AdminPanel auth={auth} organization={org} perms={my} onClose={()=>setAdminOpen(false)} onOrganizationRenamed={setOrgName}/>}{settingsOpen&&<SettingsModal auth={auth} preference={preference} initialSection={settingsSection} onPresence={choosePresence} onLogout={()=>{setSettingsOpen(false);logout()}} onClose={()=>{setSettingsOpen(false);setSettingsSection('media')}}/>}{projectSettingsFor&&<ProjectSettingsDialog token={auth.accessToken} organizationId={organization.id} project={projectSettingsFor} onClose={()=>setProjectSettingsFor(null)} onSaved={()=>setProjectSettingsFor(null)}/>}{deletingProject&&<DeleteProjectDialog auth={auth} organization={organization} project={deletingProject} onClose={()=>setDeletingProject(null)} onDeleted={()=>projectDeleted(deletingProject.id)}/>}</main>
}

// Arama katmanı tüm çalışma alanını sarar: gelen arama ve aktif arama paneli her ekranda görünür.
export type OverlayChatMode={projectId:string;target?:{kind:'channel'|'person'|'group';id:string;projectId?:string}|null}
export default function Workspace(props:{auth:Auth;organization:Organization;logout:()=>void;overlay?:OverlayChatMode}){
  // Oyun içi overlay penceresinde WebRTC açılmaz: ses durumu ana pencereden aynalanır.
  if(props.overlay)return <OverlayCallProvider me={props.auth.user.id} organizationId={props.organization.id}>
    <ContextMenuViewportGuard/>
    <WorkspaceBody {...props}/>
  </OverlayCallProvider>
  return <CallProvider auth={props.auth} organizationId={props.organization.id}>
    <ContextMenuViewportGuard/>
    <WorkspaceBody {...props}/>
  </CallProvider>
}
