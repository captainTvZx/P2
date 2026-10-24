import { useCallback, useEffect, useState } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { imageSrc } from './ProjectSettingsDialog'
import { avatarHue, userInitials } from './initials'
import ChannelIcon from './ChannelIcons'
import ConfirmDialog from './ConfirmDialog'
import { presenceClass, statusLabel } from './ProfilePanel'
import { useEscape } from './useEscape'
import type { PermState } from './PermissionRow'
import AdminTeam, { type TeamGroup, type TeamMember, type TeamRole } from './AdminTeam'
import AdminAccess from './AdminAccess'

type Auth={user:{id:string;displayName:string};accessToken:string}
type Organization={id:string;name:string;slug:string}

type Overview={
  organization:{id:string;name:string;slug:string;createdAt:string}
  server:{version:string}
  members:{total:number;owners:number;roles:number;online:number;joinedThisWeek:number;twoFactor:number}
  content:{projects:number;channels:number;deletedChannels:number;groups:number;channelMessages:number;directMessages:number;groupMessages:number;messages:number}
  invites:number
  sessions:number
  storage:{bytes:number;files:number}
}
type MemberRole={id:string;handle:string;name:string;color:string|null;founder?:boolean;viaGroup?:boolean}
type MemberGroup={id:string;handle:string;name:string;color:string|null}
type Member=TeamMember
type Invite={id:string;code:string;maxUses:number|null;useCount:number;expiresAt:string|null;createdAt:string;createdBy:{id:string;displayName:string}}
export type UserGroupMemberRow={id:string;username:string;displayName:string;avatarUrl:string|null;status:string}
type UserGroup=TeamGroup
type PermissionInfo={id:string;group:string;label:string;hint:string;channel?:boolean}
type PermissionMember={id:string;username:string;displayName:string;avatarUrl:string|null;owner:boolean;roleIds:string[];allow:string[];deny:string[];effective:string[]}
type RoleGroupRef={id:string;handle:string;name:string;color:string|null;memberCount:number}
type AdminRole=TeamRole
type PermissionGroup={id:string;handle:string;name:string;color?:string|null;description?:string|null;everyone:boolean;founder:boolean;permissions:string[];memberCount:number;memberIds:string[];groups:RoleGroupRef[]}
type PermissionData={catalog:PermissionInfo[];members:PermissionMember[];roles:PermissionGroup[];groups:RoleGroupRef[]}
type AdminChannel={id:string;name:string;topic:string|null;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';deletedAt:string|null;purgeAt:string|null;messages:number;category:string;project:string;projectId:string}

type Section='overview'|'team'|'access'|'invites'|'library'|'channels'|'trash'|'organization'
// Her bölüm kendi yetkisine bağlı: yetkisi olmayan sekmeyi hiç görmez.
const sections:Array<{id:Section;label:string;hint:string;needs?:string}>=[
  {id:'overview',label:'Genel Bakış',hint:'Kuruluşun özeti'},
  {id:'team',label:'Ekip ve Yetkiler',hint:'Rol oluştur, yetki seç, kişiye ata'},
  {id:'access',label:'Erişim',hint:'Proje ve kanal görünürlüğü',needs:'member.roles'},
  {id:'invites',label:'Davetler',hint:'Davet kodları ve sınırları',needs:'member.invite'},
  {id:'library',label:'Kütüphane',hint:'Hangi projeler uygulama olarak listelenir',needs:'release.publish'},
  {id:'channels',label:'Kanallar',hint:'Tüm kanallar ve mesaj sayıları'},
  {id:'trash',label:'Geri Dönüşüm',hint:'Silinen projeleri kalıcı kaldır',needs:'organization.manage'},
  {id:'organization',label:'Kuruluş',hint:'Ad, adres ve bilgiler',needs:'organization.manage'},
]

type LibraryProject={id:string;name:string;iconUrl?:string|null;libraryEnabled?:boolean;updatedAt?:string}
type DeletedProject={id:string;name:string;slug:string;deletedAt:string;purgeAt:string|null;createdAt:string;owner:string;categories:number;members:number}

const typeLabels:Record<AdminChannel['type'],string>={TEXT:'Metin',ANNOUNCEMENT:'Duyuru',VOICE:'Ses',FORUM:'Forum'}

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await p2Fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

function bytes(size:number){
  if(size<1024)return `${size} B`
  if(size<1024*1024)return `${(size/1024).toFixed(1)} KB`
  if(size<1024*1024*1024)return `${(size/1024/1024).toFixed(1)} MB`
  return `${(size/1024/1024/1024).toFixed(2)} GB`
}
const day=(value:string|null)=>value?new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'long',year:'numeric'}):'—'

function Stat({label,value,hint}:{label:string;value:string|number;hint?:string}){
  return <div className="admin-stat"><small>{label}</small><b>{value}</b>{hint&&<span>{hint}</span>}</div>
}

export default function AdminPanel({auth,organization,perms,onClose,onOrganizationRenamed}:{
  auth:Auth
  organization:Organization
  perms:{can:(permission:string)=>boolean;owner:boolean}
  onClose:()=>void
  onOrganizationRenamed:(name:string)=>void
}){
  const[section,setSection]=useState<Section>('overview')
  const[libraryProjects,setLibraryProjects]=useState<LibraryProject[]>([])
  // Geri dönüşümdeki projeler: aynı proje kodunu yeniden kullanabilmek için
  // buradan kalıcı silinir.
  const[trash,setTrash]=useState<DeletedProject[]>([])
  const[purging,setPurging]=useState<DeletedProject|null>(null)
  const[purgeBusy,setPurgeBusy]=useState(false)
  const[overview,setOverview]=useState<Overview|null>(null)
  const[members,setMembers]=useState<Member[]>([])
  const[invites,setInvites]=useState<Invite[]>([])
  const[channels,setChannels]=useState<AdminChannel[]>([])
  const[userGroups,setUserGroups]=useState<UserGroup[]>([])
  const[roles,setRoles]=useState<AdminRole[]>([])
  const[permissions,setPermissions]=useState<PermissionData|null>(null)
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  const[notice,setNotice]=useState('')
  const[search,setSearch]=useState('')
  const[removing,setRemoving]=useState<Member|null>(null),[transferring,setTransferring]=useState<Member|null>(null)
  const[name,setName]=useState(organization.name)
  const[inviteUses,setInviteUses]=useState('')
  const[inviteDays,setInviteDays]=useState('')
  const owner=perms.owner
  const canRoles=perms.can('member.roles'),canKick=perms.can('member.kick'),canInvite=perms.can('member.invite')
  const canGroups=perms.can('usergroup.manage'),canOrg=perms.can('organization.manage')
  const visibleSections=sections.filter(item=>!item.needs||perms.can(item.needs))
  useEffect(()=>{if(!visibleSections.some(item=>item.id===section))setSection(visibleSections[0]?.id||'overview')},[perms,section])
  const base=`/organizations/${organization.id}/admin`
  useEscape(!removing,onClose)

  const load=useCallback(async(target:Section)=>{
    setError('')
    try{
      if(target==='overview'||target==='organization')setOverview(await api<Overview>(`${base}/overview`,auth.accessToken))
      if(target==='team'){
        const[people,roleRows,groupRows]=await Promise.all([
          api<Member[]>(`${base}/members`,auth.accessToken),
          api<AdminRole[]>(`/organizations/${organization.id}/roles`,auth.accessToken),
          api<UserGroup[]>(`/organizations/${organization.id}/user-groups`,auth.accessToken),
        ])
        setMembers(people);setRoles(roleRows);setUserGroups(groupRows)
        // Kişiye özel yetkiler yalnızca yetkisi olana gösterilir.
        try{setPermissions(await api<PermissionData>(`${base}/permissions`,auth.accessToken))}catch{}
      }
      if(target==='invites')setInvites(await api<Invite[]>(`${base}/invites`,auth.accessToken))
      if(target==='channels')setChannels(await api<AdminChannel[]>(`${base}/channels`,auth.accessToken))
      if(target==='library')setLibraryProjects(await api<LibraryProject[]>(`/organizations/${organization.id}/projects`,auth.accessToken))
      if(target==='trash')setTrash(await api<DeletedProject[]>(`${base}/projects/trash`,auth.accessToken))
    }catch(problem:any){setError(problem?.message||'Yönetim verileri yüklenemedi.')}
  },[base,auth.accessToken,organization.id])
  useEffect(()=>{void load(section)},[section,load])

  async function run(action:()=>Promise<void>,done:string){
    setBusy(true);setError('');setNotice('')
    try{await action();setNotice(done)}
    catch(problem:any){setError(problem?.message||'İşlem tamamlanamadı.')}
    finally{setBusy(false)}
  }

  const removeMember=(member:Member)=>run(async()=>{
    await api(`${base}/members/${member.id}`,auth.accessToken,{method:'DELETE'})
    setMembers(current=>current.filter(item=>item.id!==member.id))
  },`${member.displayName} kuruluştan çıkarıldı.`)

  const revokeSessions=(member:Member)=>run(async()=>{
    const result=await api<{closed:number}>(`${base}/members/${member.id}/sessions/revoke`,auth.accessToken,{method:'POST'})
    setNotice(`${member.displayName} için ${result.closed} oturum kapatıldı.`)
  },`${member.displayName} oturumları kapatıldı.`)

  const createInvite=()=>run(async()=>{
    const body:{maxUses?:number;expiresInDays?:number}={}
    if(Number(inviteUses)>0)body.maxUses=Number(inviteUses)
    if(Number(inviteDays)>0)body.expiresInDays=Number(inviteDays)
    const invite=await api<Invite>(`${base}/invites`,auth.accessToken,{method:'POST',body:JSON.stringify(body)})
    setInvites(current=>[invite,...current])
    setInviteUses('');setInviteDays('')
  },'Yeni davet kodu oluşturuldu.')

  // Kuruluşun varsayılan davet kodunu yeniler; eski kod anında geçersiz olur.
  const rotateInvite=()=>run(async()=>{
    const invite=await api<Invite>(`${base}/invites/rotate`,auth.accessToken,{method:'POST'})
    setInvites(await api<Invite[]>(`${base}/invites`,auth.accessToken))
    setNotice(`Yeni davet kodu: ${invite.code}`)
  },'Davet kodu yenilendi.')

  const revokeInvite=(invite:Invite)=>run(async()=>{
    await api(`${base}/invites/${invite.id}`,auth.accessToken,{method:'DELETE'})
    setInvites(current=>current.filter(item=>item.id!==invite.id))
  },'Davet kodu iptal edildi.')

  const roleBase=`/organizations/${organization.id}/roles`

  const transferOwner=(member:Member)=>run(async()=>{
    await api(`${base}/members/${member.id}/transfer-owner`,auth.accessToken,{method:'POST'})
    await refreshTeam()
  },'Kuruluş sahipliği devredildi.')
  // Ekip ekranının tüm eylemleri: her biri değişikliği kaydedip listeleri tazeler.
  const refreshTeam=async()=>{
    const[people,roleRows,groupRows]=await Promise.all([
      api<Member[]>(`${base}/members`,auth.accessToken),
      api<AdminRole[]>(roleBase,auth.accessToken),
      api<UserGroup[]>(`/organizations/${organization.id}/user-groups`,auth.accessToken),
    ])
    setMembers(people);setRoles(roleRows);setUserGroups(groupRows)
    if(canRoles){try{setPermissions(await api<PermissionData>(`${base}/permissions`,auth.accessToken))}catch{}}
  }
  const teamAction=(work:()=>Promise<unknown>,done:string)=>run(async()=>{await work();await refreshTeam()},done)
  const teamActions={
    assignRole:(userId:string,roleId:string)=>teamAction(()=>api(`${roleBase}/${roleId}/members/${userId}`,auth.accessToken,{method:'POST'}),'Rol verildi.'),
    unassignRole:(userId:string,roleId:string)=>teamAction(()=>api(`${roleBase}/${roleId}/members/${userId}`,auth.accessToken,{method:'DELETE'}),'Rol geri alındı.'),
    addToGroup:(groupId:string,userId:string)=>teamAction(()=>api(`/organizations/${organization.id}/user-groups/${groupId}/members`,auth.accessToken,{method:'POST',body:JSON.stringify({userIds:[userId]})}),'Kişi gruba eklendi.'),
    removeFromGroup:(groupId:string,userId:string)=>teamAction(()=>api(`/organizations/${organization.id}/user-groups/${groupId}/members/${userId}`,auth.accessToken,{method:'DELETE'}),'Kişi gruptan çıkarıldı.'),
    linkRoleGroup:(roleId:string,groupId:string)=>teamAction(()=>api(`${roleBase}/${roleId}/groups/${groupId}`,auth.accessToken,{method:'POST'}),'Rol gruba bağlandı.'),
    unlinkRoleGroup:(roleId:string,groupId:string)=>teamAction(()=>api(`${roleBase}/${roleId}/groups/${groupId}`,auth.accessToken,{method:'DELETE'}),'Rolün grup bağı kaldırıldı.'),
    toggleRolePermission:(role:AdminRole,permission:string)=>teamAction(()=>{
      const next=role.permissions.includes(permission)?role.permissions.filter(item=>item!==permission):[...role.permissions,permission]
      return api(`${roleBase}/${role.id}/permissions`,auth.accessToken,{method:'PATCH',body:JSON.stringify({permissions:next})})
    },'Rol yetkileri güncellendi.'),
    setMemberPermission:(member:Member,permission:string,next:PermState)=>teamAction(()=>{
      const row=permissions?.members.find(item=>item.id===member.id)
      const allow=(row?.allow||[]).filter(item=>item!==permission)
      const deny=(row?.deny||[]).filter(item=>item!==permission)
      if(next==='allow')allow.push(permission)
      else if(next==='deny')deny.push(permission)
      return api(`${base}/members/${member.id}/permissions`,auth.accessToken,{method:'PATCH',body:JSON.stringify({allow,deny})})
    },'Kişi yetkileri güncellendi.'),
    resetMemberPermissions:(member:Member)=>teamAction(()=>api(`${base}/members/${member.id}/permissions`,auth.accessToken,{method:'PATCH',body:JSON.stringify({allow:[],deny:[]})}),'Özel ayarlar temizlendi.'),
    createRole:(form:{handle:string;name:string;description:string;color:string})=>teamAction(()=>api(roleBase,auth.accessToken,{method:'POST',body:JSON.stringify({handle:form.handle.trim().toLowerCase(),name:form.name.trim(),description:form.description.trim(),color:form.color})}),'Rol oluşturuldu.'),
    createGroup:(form:{handle:string;name:string;description:string;color:string})=>teamAction(()=>api(`/organizations/${organization.id}/user-groups`,auth.accessToken,{method:'POST',body:JSON.stringify({handle:form.handle.trim().toLowerCase(),name:form.name.trim(),description:form.description.trim(),color:form.color})}),'Grup oluşturuldu.'),
    renameRole:(role:AdminRole,patch:{name?:string;color?:string})=>teamAction(()=>api(`${roleBase}/${role.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify(patch)}),'Rol güncellendi.'),
    deleteRole:(role:AdminRole)=>teamAction(()=>api(`${roleBase}/${role.id}`,auth.accessToken,{method:'DELETE'}),`${role.name} silindi.`),
    deleteGroup:(group:UserGroup)=>teamAction(()=>api(`/organizations/${organization.id}/user-groups/${group.id}`,auth.accessToken,{method:'DELETE'}),`${group.name} silindi.`),
    revokeSessions:(member:Member)=>void revokeSessions(member),
    transferOwner:(member:Member)=>setTransferring(member),
    removeMember:(member:Member)=>setRemoving(member),
  }

  const rename=()=>run(async()=>{
    const updated=await api<{name:string}>(base,auth.accessToken,{method:'PATCH',body:JSON.stringify({name:name.trim()})})
    onOrganizationRenamed(updated.name)
  },'Kuruluş adı güncellendi.')

  const normalized=search.trim().toLocaleLowerCase('tr-TR')
  const visibleMembers=members.filter(member=>!normalized||member.displayName.toLocaleLowerCase('tr-TR').includes(normalized)||member.username.toLocaleLowerCase('tr-TR').includes(normalized)||member.email.toLocaleLowerCase('tr-TR').includes(normalized))
  const liveChannels=channels.filter(channel=>!channel.deletedAt)
  const trashChannels=channels.filter(channel=>channel.deletedAt)

  return <div className="settings-layer admin-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="settings-modal admin-modal" role="dialog" aria-label="Yönetim paneli">
      <nav>
        <header><b>Yönetim</b><small>{organization.name} · {perms.owner?'Kuruluş sahibi':'Üye'}</small></header>
        {visibleSections.map(item=><button type="button" key={item.id} className={section===item.id?'active':undefined} onClick={()=>setSection(item.id)}>
          <b>{item.label}</b><small>{item.hint}</small>
        </button>)}
      </nav>
      <div className="settings-body">
        <header>
          <h3>{sections.find(item=>item.id===section)?.label}</h3>
          <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button>
        </header>
        <div className={`settings-scroll${section==='access'?' access-scroll':''}`}>
          {error&&<p className="settings-warn">{error}</p>}
          {notice&&!error&&<p className="settings-hint">{notice}</p>}

          {section==='overview'&&(overview?<>
            <section className="settings-block">
              <h4>Ekip</h4>
              <div className="admin-stats">
                <Stat label="Toplam üye" value={overview.members.total} hint={`${overview.members.online} kişi çevrimiçi`}/>
                <Stat label="Rol" value={overview.members.roles} hint={`${overview.members.owners} kuruluş sahibi`}/>
                <Stat label="Bu hafta katılan" value={overview.members.joinedThisWeek}/>
                <Stat label="2FA açık" value={`${overview.members.twoFactor}/${overview.members.total}`} hint={overview.members.twoFactor===overview.members.total?'Herkes korumalı':'Kalanları teşvik et'}/>
              </div>
            </section>
            <section className="settings-block">
              <h4>İçerik</h4>
              <div className="admin-stats">
                <Stat label="Mesaj" value={overview.content.messages} hint={`${overview.content.channelMessages} kanal · ${overview.content.directMessages} bireysel · ${overview.content.groupMessages} grup`}/>
                <Stat label="Kanal" value={overview.content.channels} hint={overview.content.deletedChannels?`${overview.content.deletedChannels} silinmiş`:'Silinmiş kanal yok'}/>
                <Stat label="Proje" value={overview.content.projects}/>
                <Stat label="Grup" value={overview.content.groups}/>
              </div>
            </section>
            <section className="settings-block">
              <h4>Sistem</h4>
              <div className="admin-stats">
                <Stat label="Dosya alanı" value={bytes(overview.storage.bytes)} hint={`${overview.storage.files} dosya`}/>
                <Stat label="Açık oturum" value={overview.sessions}/>
                <Stat label="P2 Server" value={`v${overview.server.version}`}/>
                <Stat label="Davet kodu" value={overview.invites}/>
                <Stat label="Kuruluş yaşı" value={day(overview.organization.createdAt)}/>
              </div>
            </section>
          </>:<p className="settings-hint">Yükleniyor…</p>)}

          {section==='team'&&<AdminTeam
            members={members} roles={roles} groups={userGroups} catalog={permissions?.catalog||[]}
            overrides={Object.fromEntries((permissions?.members||[]).map(row=>[row.id,{allow:row.allow,deny:row.deny,effective:row.effective}]))}
            me={auth.user.id} busy={busy} owner={owner} can={{roles:canRoles,groups:canGroups,kick:canKick}}
            actions={teamActions}
          />}

          {section==='access'&&<AdminAccess auth={auth} organizationId={organization.id}/>}

          {section==='invites'&&<>
            <section className="settings-block">
              <h4>Sunucu bağlantısı</h4>
              <p className="settings-hint">Yeni kullanıcı önce bu sunucu adresiyle P2'ye bağlanır, ardından aşağıdaki davet kodlarından birini kullanır.</p>
              <div className="admin-server-link"><span className="admin-server-link-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/></svg></span><div className="admin-server-link-text"><small>Sunucu adresi</small><code title={API_BASE.replace(/\/api\/?$/,'')}>{API_BASE.replace(/\/api\/?$/,'')}</code></div><button type="button" className="settings-button primary" onClick={()=>{void navigator.clipboard.writeText(API_BASE.replace(/\/api\/?$/,''));setNotice('Sunucu bağlantısı kopyalandı.')}}>Kopyala</button></div>
            </section>
            <section className="settings-block">
              <h4>Yeni davet kodu</h4>
              <div className="admin-invite-form">
                <label className="settings-field"><span>Kullanım sınırı</span><input value={inviteUses} inputMode="numeric" placeholder="sınırsız" onChange={event=>setInviteUses(event.target.value.replace(/\D/g,'').slice(0,4))}/></label>
                <label className="settings-field"><span>Geçerlilik (gün)</span><input value={inviteDays} inputMode="numeric" placeholder="süresiz" onChange={event=>setInviteDays(event.target.value.replace(/\D/g,'').slice(0,3))}/></label>
                <button type="button" className="settings-button primary" disabled={busy||!canInvite} onClick={()=>void createInvite()}>Kod oluştur</button>
              </div>
            </section>
            <section className="settings-block">
              <h4>Davet kodları · {invites.length}</h4>
              <div className="settings-row">
                <button type="button" className="settings-button" disabled={busy||!canInvite} onClick={()=>void rotateInvite()} title="Varsayılan kodu yenile — eski kod geçersiz olur">Varsayılan kodu yenile</button>
              </div>
              <div className="admin-rows">
                {invites.map(invite=><div className="admin-invite" key={invite.id}>
                  <code>{invite.code}</code>
                  <div className="admin-invite-info">
                    <small>{invite.maxUses?`${invite.useCount}/${invite.maxUses} kullanım`:`${invite.useCount} kullanım · sınırsız`}</small>
                    <small>{invite.expiresAt?`${day(invite.expiresAt)} tarihinde biter`:'Süresiz'} · {invite.createdBy.displayName} oluşturdu</small>
                  </div>
                  <span className="admin-member-actions">
                    <button type="button" className="settings-button" onClick={()=>{void navigator.clipboard.writeText(invite.code);setNotice('Davet kodu kopyalandı.')}}>Kopyala</button>
                    <button type="button" className="settings-button danger" disabled={busy||invites.length<2||!canInvite} onClick={()=>void revokeInvite(invite)}>İptal et</button>
                  </span>
                </div>)}
                {!invites.length&&<p className="settings-hint">Henüz davet kodu yok.</p>}
              </div>
            </section>
          </>}

          {section==='trash'&&<>
            <section className="settings-block">
              <h4>Silinen projeler · {trash.length}</h4>
              <p className="settings-hint">Silinen projeler 30 gün geri dönüşümde bekler. Proje kodu bu süre boyunca kayıtlı kaldığı için aynı adla yeni bir proje açılamaz; kalıcı silmek kodu serbest bırakır ve projenin kanalları, mesajları, panoları geri dönülmez biçimde kaldırılır.</p>
              <div className="admin-rows">
                {trash.map(project=><div className="admin-channel" key={project.id}>
                  <span className="admin-channel-icon">🗑</span>
                  <div className="admin-member-name">
                    <b>{project.name}</b>
                    <small>Kod: {project.slug} · Sahip: {project.owner}</small>
                    <small>
                      {new Date(project.deletedAt).toLocaleDateString('tr-TR',{day:'numeric',month:'long',year:'numeric'})} tarihinde silindi
                      {project.purgeAt?` · ${new Date(project.purgeAt).toLocaleDateString('tr-TR',{day:'numeric',month:'long'})} tarihinde kendiliğinden kaldırılacak`:''}
                    </small>
                  </div>
                  <button type="button" className="settings-button danger" disabled={purgeBusy} onClick={()=>setPurging(project)}>Kalıcı sil</button>
                </div>)}
                {!trash.length&&<p className="admin-empty">Geri dönüşüm boş.</p>}
              </div>
            </section>
          </>}
          {section==='library'&&<>
            <section className="settings-block">
              <h4>Kütüphanede uygulamalar · {libraryProjects.filter(item=>item.libraryEnabled).length}</h4>
              <p className="settings-hint">Açık olan projeler ekibin Kütüphane'sinde oyun/uygulama olarak görünür; sürüm yüklenip çalıştırılabilir. Bir sürüm yayınlandığında proje kendiliğinden açılır. Aynı anahtar proje ayarlarında da vardır.</p>
              <div className="admin-rows">
                {libraryProjects.map(item=><div className="admin-library-row" key={item.id}>
                  <span className="admin-avatar small">{item.iconUrl?<img src={imageSrc(item.iconUrl)} alt=""/>:item.name.slice(0,2).toLocaleUpperCase('tr-TR')}</span>
                  <div className="admin-member-name"><b>{item.name}</b><small>{item.libraryEnabled?'Kütüphanede listeleniyor':'Kütüphanede görünmüyor'}</small></div>
                  <button type="button" className={`p2-toggle${item.libraryEnabled?' on':''}`} role="switch" aria-checked={Boolean(item.libraryEnabled)} aria-label={`${item.name}: kütüphanede uygulama`} disabled={busy} onClick={()=>void run(async()=>{const saved=await api<LibraryProject>(`/organizations/${organization.id}/projects/${item.id}`,auth.accessToken,{method:'PATCH',body:JSON.stringify({libraryEnabled:!item.libraryEnabled})});setLibraryProjects(rows=>rows.map(row=>row.id===item.id?{...row,libraryEnabled:saved.libraryEnabled}:row));window.dispatchEvent(new CustomEvent('p2:project-updated',{detail:saved}))},item.libraryEnabled?`${item.name} kütüphaneden kaldırıldı.`:`${item.name} kütüphaneye eklendi.`)}><i/></button>
                </div>)}
                {!libraryProjects.length&&<p className="settings-hint">Proje yok.</p>}
              </div>
            </section>
          </>}

          {section==='channels'&&<>
            <section className="settings-block">
              <h4>Kanallar · {liveChannels.length}</h4>
              <div className="admin-rows">
                {liveChannels.map(channel=><div className="admin-channel" key={channel.id}>
                  <span className="admin-channel-icon"><ChannelIcon type={channel.type}/></span>
                  <div className="admin-member-name">
                    <b>{channel.name}</b>
                    <small>{channel.project} · {channel.category} · {typeLabels[channel.type]}</small>
                    {channel.topic&&<small>{channel.topic}</small>}
                  </div>
                  <b className="admin-channel-count">{channel.messages}<small>mesaj</small></b>
                </div>)}
                {!liveChannels.length&&<p className="settings-hint">Kanal yok.</p>}
              </div>
            </section>
            {trashChannels.length>0&&<section className="settings-block">
              <h4>Silinmiş kanallar · {trashChannels.length}</h4>
              <p className="settings-hint">Silinen kanallar 30 gün saklanır, sonra kalıcı olarak temizlenir.</p>
              <div className="admin-rows">
                {trashChannels.map(channel=><div className="admin-channel deleted" key={channel.id}>
                  <span className="admin-channel-icon"><ChannelIcon type={channel.type}/></span>
                  <div className="admin-member-name">
                    <b>{channel.name}</b>
                    <small>{channel.project} · {day(channel.deletedAt)} silindi · {day(channel.purgeAt)} tarihinde temizlenecek</small>
                  </div>
                  <b className="admin-channel-count">{channel.messages}<small>mesaj</small></b>
                </div>)}
              </div>
            </section>}
          </>}

          {section==='organization'&&<>
            <section className="settings-block">
              <h4>Kuruluş adı</h4>
              <label className="settings-field"><span>Ad</span><input value={name} maxLength={60} disabled={!canOrg} onChange={event=>setName(event.target.value)}/></label>
              {!canOrg&&<p className="settings-hint">Kuruluş adını değiştirmek için “Kuruluşu düzenle” yetkisi gerekir.</p>}
              <button type="button" className="settings-button primary" disabled={!canOrg||busy||!name.trim()||name.trim()===organization.name} onClick={()=>void rename()}>Kaydet</button>
            </section>
            {overview&&<section className="settings-block">
              <h4>Bilgiler</h4>
              <div className="channel-details">
                <div><small>Adres</small><b>{overview.organization.slug}</b></div>
                <div><small>Kimlik</small><b>{overview.organization.id.slice(0,8)}</b></div>
                <div><small>Kuruluş tarihi</small><b>{day(overview.organization.createdAt)}</b></div>
                <div><small>Toplam üye</small><b>{overview.members.total} kişi</b></div>
                <div><small>Dosya alanı</small><b>{bytes(overview.storage.bytes)}</b></div>
              </div>
            </section>}
          </>}
        </div>
      </div>
    </section>
    {purging&&<ConfirmDialog
      title="Proje kalıcı olarak silinsin mi?"
      body={`“${purging.name}” (kod: ${purging.slug}) ve projeye bağlı tüm kanallar, mesajlar, panolar ve görevler geri dönülmez biçimde silinecek. Silindikten sonra aynı kodla yeni bir proje açabilirsin.`}
      confirmLabel="Kalıcı olarak sil" danger
      onCancel={()=>setPurging(null)}
      onConfirm={async()=>{
        const hedef=purging
        setPurging(null);setPurgeBusy(true)
        try{
          await api(`${base}/projects/${hedef.id}/purge`,auth.accessToken,{method:'DELETE'})
          setTrash(items=>items.filter(item=>item.id!==hedef.id))
          window.dispatchEvent(new CustomEvent('p2:toast',{detail:{text:`“${hedef.name}” kalıcı olarak silindi; “${hedef.slug}” kodu artık kullanılabilir.`,tone:'ok'}}))
        }catch(problem:any){
          window.dispatchEvent(new CustomEvent('p2:toast',{detail:{text:problem?.message||'Proje silinemedi.'}}))
        }finally{ setPurgeBusy(false) }
      }}/>}
    {transferring&&<ConfirmDialog title="Sahipliği devret" body={`${transferring.displayName} kuruluşun sahibi olacak, sen düz üyeye döneceksin. Bu işlemi geri almak için yeni sahibin sahipliği sana devretmesi gerekir.`} confirmLabel="Sahipliği devret" danger onCancel={()=>setTransferring(null)} onConfirm={()=>{const target=transferring;setTransferring(null);void transferOwner(target)}}/>}
    {removing&&<ConfirmDialog title="Üyeyi çıkar" body={`${removing.displayName} kuruluştan çıkarılacak. Gruplardan da çıkarılır ve kuruluş içeriğine erişemez.`} confirmLabel="Kuruluştan çıkar" danger onCancel={()=>setRemoving(null)} onConfirm={()=>{const target=removing;setRemoving(null);void removeMember(target)}}/>}
  </div>
}
