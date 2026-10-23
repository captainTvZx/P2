import { useState } from 'react'
import { avatarHue, userInitials } from './initials'
import { presenceClass, statusLabel } from './ProfilePanel'
import { PermissionToggle, type PermState } from './PermissionRow'

// Ana akış bilinçli olarak iki parçadır: rolün yetkilerini seç, rolü kişiye ver.
// Eski grup verileri sunucuda korunur ama günlük yetki yönetimini kalabalıklaştırmaz.
export type TeamRole={id:string;handle:string;name:string;description:string|null;color:string|null;everyone:boolean;founder:boolean;permissions:string[];members:TeamPerson[];groups:TeamGroupRef[]}
export type TeamGroupRef={id:string;handle:string;name:string;color:string|null;memberCount:number}
export type TeamPerson={id:string;username:string;displayName:string;avatarUrl:string|null;status:string}
export type TeamGroup={id:string;handle:string;name:string;description:string|null;color:string|null;members:TeamPerson[];roles:{id:string;handle:string;name:string;color:string|null}[]}
export type TeamMember={
  id:string;username:string;displayName:string;email:string;avatarUrl:string|null;status:string
  twoFactorEnabled:boolean;owner:boolean;channelMessages:number;joinedAt:string
  lastClientVersion:string|null;lastClientSeenAt:string|null
  roles:{id:string;handle:string;name:string;color:string|null;founder?:boolean;viaGroup?:boolean}[]
  groups:{id:string;handle:string;name:string;color:string|null}[]
}
export type PermissionInfo={id:string;group:string;label:string;hint:string}
export type MemberOverride={allow:string[];deny:string[];effective:string[]}

export const ROLE_COLORS=['#5865f2','#3ba55d','#faa61a','#ed4245','#9b59b6','#1abc9c','#e91e63','#95a5a6']
export function Tag({handle,color,title,onRemove}:{handle:string;color?:string|null;title?:string;onRemove?:()=>void}){
  return <span className="team-tag" title={title} style={color?{background:`${color}22`,color,borderColor:`${color}55`}:undefined}>
    @{handle}{onRemove&&<button type="button" onClick={onRemove} aria-label="Kaldır">×</button>}
  </span>
}
const day=(value:string)=>new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'long',year:'numeric'})

type Tab='members'|'roles'|'groups'
type Actions={
  assignRole:(userId:string,roleId:string)=>void
  unassignRole:(userId:string,roleId:string)=>void
  addToGroup:(groupId:string,userId:string)=>void
  removeFromGroup:(groupId:string,userId:string)=>void
  linkRoleGroup:(roleId:string,groupId:string)=>void
  unlinkRoleGroup:(roleId:string,groupId:string)=>void
  toggleRolePermission:(role:TeamRole,permission:string)=>void
  setMemberPermission:(member:TeamMember,permission:string,next:PermState)=>void
  resetMemberPermissions:(member:TeamMember)=>void
  createRole:(form:{handle:string;name:string;description:string;color:string})=>void
  createGroup:(form:{handle:string;name:string;description:string;color:string})=>void
  renameRole:(role:TeamRole,patch:{name?:string;color?:string})=>void
  deleteRole:(role:TeamRole)=>void
  deleteGroup:(group:TeamGroup)=>void
  revokeSessions:(member:TeamMember)=>void
  transferOwner:(member:TeamMember)=>void
  removeMember:(member:TeamMember)=>void
}

function Picker({label,options,onPick,empty}:{label:string;options:{id:string;label:string}[];onPick:(id:string)=>void;empty:string}){
  if(!options.length)return <span className="team-picker-empty">{empty}</span>
  return <select className="team-picker" value="" onChange={event=>{if(event.target.value)onPick(event.target.value)}}>
    <option value="">{label}</option>
    {options.map(option=><option key={option.id} value={option.id}>{option.label}</option>)}
  </select>
}

export default function AdminTeam({
  members,roles,groups,catalog,overrides,me,busy,can,owner,actions,
}:{
  members:TeamMember[]
  roles:TeamRole[]
  groups:TeamGroup[]
  catalog:PermissionInfo[]
  overrides:Record<string,MemberOverride>
  me:string
  busy:boolean
  can:{roles:boolean;groups:boolean;kick:boolean}
  owner:boolean
  actions:Actions
}){
  const[tab,setTab]=useState<Tab>('members')
  const[search,setSearch]=useState('')
  const[selected,setSelected]=useState<Record<Tab,string>>({members:'',roles:'',groups:''})
  const[creating,setCreating]=useState(false)
  const[form,setForm]=useState({handle:'',name:'',description:'',color:ROLE_COLORS[0]})
  const categories=[...new Set(catalog.map(item=>item.group))]
  const needle=search.trim().toLocaleLowerCase('tr-TR')
  const has=(value:string)=>!needle||value.toLocaleLowerCase('tr-TR').includes(needle)

  const memberList=members.filter(item=>has(item.displayName)||has(item.username)||has(item.email))
  const roleList=roles.filter(item=>has(item.name)||has(item.handle))
  const groupList=groups.filter(item=>has(item.name)||has(item.handle))
  const member=members.find(item=>item.id===selected.members)||memberList[0]
  const role=roles.find(item=>item.id===selected.roles)||roleList[0]
  const group=groups.find(item=>item.id===selected.groups)||groupList[0]
  const pick=(id:string)=>setSelected(current=>({...current,[tab]:id}))

  function startCreate(){setForm({handle:'',name:'',description:'',color:ROLE_COLORS[0]});setCreating(true)}
  function submitCreate(){
    if(tab==='roles')actions.createRole(form)
    else actions.createGroup(form)
    setCreating(false)
  }

  return <div className="team">
    <div className="team-tabs">
      <button type="button" className={tab==='members'?'active':undefined} onClick={()=>{setTab('members');setCreating(false)}}>Üyeler <em>{members.length}</em></button>
      <button type="button" className={tab==='roles'?'active':undefined} onClick={()=>{setTab('roles');setCreating(false)}}>Roller <em>{roles.length}</em></button>
      {tab==='roles'&&can.roles&&<button type="button" className="team-create" onClick={startCreate}>＋ Yeni rol</button>}
    </div>

    <p className="settings-hint team-explainer">
      Basit akış: <b>Roller</b> bölümünde bir rol oluşturup yetkilerini aç; sonra <b>Üyeler</b> bölümünden bu rolü kişiye ver.
    </p>

    <div className="team-body">
      <aside className="team-list">
        <div className="admin-search"><span>⌕</span><input value={search} onChange={event=>setSearch(event.target.value)} placeholder={tab==='members'?'Kişi ara':tab==='roles'?'Rol ara':'Grup ara'}/></div>

        {tab==='members'&&memberList.map(item=><button type="button" key={item.id} className={member?.id===item.id?'active':undefined} onClick={()=>pick(item.id)}>
          <span className={`admin-avatar small ${avatarHue(item.id)}`}>{item.avatarUrl?<img src={item.avatarUrl} alt=""/>:userInitials(item.displayName)}<i className={presenceClass(item.status)}/></span>
          <span className="admin-member-name"><b>{item.displayName}</b><small>{item.owner?'Kurucu':item.roles.length?item.roles.map(role=>role.name).join(', '):'rol yok'}</small></span>
        </button>)}

        {tab==='roles'&&roleList.map(item=><button type="button" key={item.id} className={role?.id===item.id?'active':undefined} onClick={()=>pick(item.id)}>
          <i className="team-dot" style={{background:item.color||'var(--p2-neutral-fallback)'}}/>
          <span className="admin-member-name"><b>{item.name}</b><small>{item.founder?'Kuruluş sahibi':item.everyone?'Kuruluştaki herkes':`${item.members.length} kişi · ${item.groups.length} grup`} · {item.permissions.length} yetki</small></span>
        </button>)}

        {tab==='groups'&&groupList.map(item=><button type="button" key={item.id} className={group?.id===item.id?'active':undefined} onClick={()=>pick(item.id)}>
          <i className="team-dot" style={{background:item.color||'var(--p2-neutral-fallback)'}}/>
          <span className="admin-member-name"><b>{item.name}</b><small>@{item.handle} · {item.members.length} kişi · {item.roles.length} rol</small></span>
        </button>)}

        {tab==='roles'&&!roleList.length&&<p className="settings-hint">Rol yok.</p>}
        {tab==='groups'&&!groupList.length&&<p className="settings-hint">Henüz grup yok. Yukarıdan bir grup oluştur.</p>}
      </aside>

      <div className="team-detail">
        {creating&&<section className="settings-block team-form">
          <h4>{tab==='roles'?'Yeni rol':'Yeni grup'}</h4>
          <div className="team-form-grid">
            <label className="settings-field"><span>Rol adı</span><input value={form.name} maxLength={40} placeholder="Örn. Moderatör" onChange={event=>{const name=event.target.value;setForm(current=>({...current,name,handle:name.toLocaleLowerCase('tr-TR').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/ı/g,'i').replace(/ş/g,'s').replace(/ğ/g,'g').replace(/ü/g,'u').replace(/ö/g,'o').replace(/ç/g,'c').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'').slice(0,24)}))}}/></label>
            <label className="settings-field"><span>Açıklama</span><input value={form.description} maxLength={160} placeholder="isteğe bağlı" onChange={event=>setForm(current=>({...current,description:event.target.value}))}/></label>
          </div>
          <div className="settings-field"><span>Renk</span><div className="role-colors">{ROLE_COLORS.map(color=><button type="button" key={color} className={form.color===color?'active':undefined} style={{background:color}} aria-label={`Renk ${color}`} onClick={()=>setForm(current=>({...current,color}))}/>)}</div></div>
          <div className="settings-row">
            <button type="button" className="settings-button primary" disabled={busy||form.handle.length<2||form.name.trim().length<2} onClick={submitCreate}>Oluştur</button>
            <button type="button" className="settings-button" onClick={()=>setCreating(false)}>Vazgeç</button>
          </div>
        </section>}

        {!creating&&tab==='members'&&member&&<>
          <header className="team-head">
            <span className={`admin-avatar ${avatarHue(member.id)}`}>{member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}<i className={presenceClass(member.status)}/></span>
            <div>
              <h4>{member.displayName}{member.id===me&&<em> · sen</em>}</h4>
              <small>@{member.username} · {member.email}</small>
              <small>{statusLabel(member.status)} · {member.channelMessages} mesaj · {day(member.joinedAt)} katıldı · {member.twoFactorEnabled?'2FA açık':'2FA kapalı'}</small>
              <small>Son P2 Client: {member.lastClientVersion?`v${member.lastClientVersion}`:'bilinmiyor'}{member.lastClientSeenAt?` · ${new Date(member.lastClientSeenAt).toLocaleString('tr-TR')}`:''}</small>
            </div>
            <span className="team-head-actions">
              {can.kick&&<button type="button" className="settings-button" disabled={busy} onClick={()=>actions.revokeSessions(member)}>Oturumları kapat</button>}
              {owner&&!member.owner&&<button type="button" className="settings-button" disabled={busy} onClick={()=>actions.transferOwner(member)}>Sahipliği devret</button>}
              {can.kick&&<button type="button" className="settings-button danger" disabled={busy||member.owner||member.id===me} onClick={()=>actions.removeMember(member)}>Çıkar</button>}
            </span>
          </header>

          <section className="settings-block">
            <h4>Roller</h4>
            <div className="team-chips">
              {member.roles.map(item=><Tag key={item.id} handle={item.handle} color={item.color}
                title={item.viaGroup?'Grup üyeliğinden geliyor':item.founder?'Kuruluş sahipliğinden geliyor':undefined}
                onRemove={can.roles&&!item.viaGroup&&!item.founder?()=>actions.unassignRole(member.id,item.id):undefined}/>)}
              {!member.roles.length&&<small>Bu kişide rol yok.</small>}
              {can.roles&&<Picker label="＋ rol ver" empty={roles.some(item=>!item.everyone&&!item.founder)?'Tüm roller verilmiş':'Önce Roller sekmesinden bir rol oluştur'}
                options={roles.filter(item=>!item.everyone&&!item.founder&&!member.roles.some(row=>row.id===item.id)).map(item=>({id:item.id,label:item.name}))}
                onPick={roleId=>actions.assignRole(member.id,roleId)}/>}
            </div>
          </section>

          {can.roles&&!member.owner&&Boolean((overrides[member.id]?.allow.length||0)+(overrides[member.id]?.deny.length||0))&&<section className="settings-block">
            <h4>Eski özel ayarlar</h4>
            <p className="settings-hint">Bu kişide eski sistemden kalan özel yetki ayarları var. Basit rol sistemine geçmek için temizleyebilirsin.</p>
            <button type="button" className="settings-button" disabled={busy} onClick={()=>actions.resetMemberPermissions(member)}>Özel ayarları temizle</button>
          </section>}
        </>}

        {!creating&&tab==='roles'&&role&&<>
          <header className="team-head">
            <i className="team-dot large" style={{background:role.color||'var(--p2-neutral-fallback)'}}/>
            <div>
              <h4>{role.name}</h4>
              <small>@{role.handle} · {role.founder?'temel rol · kuruluş sahibi':role.everyone?'temel rol · kuruluştaki herkes':`${role.members.length} kişi · ${role.groups.length} grup`}</small>
              {role.description&&<small>{role.description}</small>}
            </div>
            <span className="team-head-actions">
              {can.roles&&!role.founder&&!role.everyone&&<>
                <div className="role-colors">{ROLE_COLORS.map(color=><button type="button" key={color} className={role.color===color?'active':undefined} style={{background:color}} aria-label={`Renk ${color}`} onClick={()=>actions.renameRole(role,{color})}/>)}</div>
                <button type="button" className="settings-button danger" disabled={busy} onClick={()=>actions.deleteRole(role)}>Rolü sil</button>
              </>}
              {(role.founder||role.everyone)&&<span className="admin-owner-badge">temel rol</span>}
            </span>
          </header>

          {!role.founder&&!role.everyone&&<section className="settings-block">
            <h4>Bu role sahip kişiler</h4>
            <div className="team-chips">
              {role.members.map(item=><Tag key={item.id} handle={item.username} onRemove={can.roles?()=>actions.unassignRole(item.id,role.id):undefined}/>)}
              {!role.members.length&&<small>Bu rol henüz doğrudan kimseye verilmedi.</small>}
              {can.roles&&<Picker label="＋ kişiye ver" empty="Herkeste var"
                options={members.filter(item=>!role.members.some(row=>row.id===item.id)).map(item=>({id:item.id,label:item.displayName}))}
                onPick={userId=>actions.assignRole(userId,role.id)}/>}
            </div>
          </section>}

          <section className="settings-block">
            <h4>Yetkiler</h4>
            <p className="settings-hint">{role.founder
              ? 'Kurucu rolünde tüm yetkiler her zaman açıktır ve değiştirilemez.'
              : role.everyone
              ? 'Buradaki her yetki kuruluştaki herkeste varsayılan olarak açılır.'
              : 'Açtığın yetkiler bu role sahip herkese verilir.'}</p>
            {categories.map(name=><div className="team-perm-group" key={name}>
              <small>{name}</small>
              <div className="perm-list">
                {catalog.filter(item=>item.group===name).map(item=><PermissionToggle
                  key={item.id} label={item.label} hint={item.hint}
                  on={role.permissions.includes(item.id)} disabled={busy||!can.roles||role.founder}
                  onChange={()=>actions.toggleRolePermission(role,item.id)}/>)}
              </div>
            </div>)}
          </section>
        </>}

        {!creating&&tab==='groups'&&group&&<>
          <header className="team-head">
            <i className="team-dot large" style={{background:group.color||'var(--p2-neutral-fallback)'}}/>
            <div>
              <h4>{group.name}</h4>
              <small>@{group.handle} · {group.members.length} kişi · {group.roles.length} bağlı rol</small>
              {group.description&&<small>{group.description}</small>}
            </div>
            <span className="team-head-actions">
              {can.groups&&<button type="button" className="settings-button danger" disabled={busy} onClick={()=>actions.deleteGroup(group)}>Grubu sil</button>}
            </span>
          </header>

          <section className="settings-block">
            <h4>Üyeler</h4>
            <div className="team-chips">
              {group.members.map(item=><Tag key={item.id} handle={item.username} onRemove={can.groups?()=>actions.removeFromGroup(group.id,item.id):undefined}/>)}
              {!group.members.length&&<small>Bu grupta henüz kimse yok.</small>}
              {can.groups&&<Picker label="＋ kişi ekle" empty="Herkes eklendi"
                options={members.filter(item=>!group.members.some(row=>row.id===item.id)).map(item=>({id:item.id,label:item.displayName}))}
                onPick={userId=>actions.addToGroup(group.id,userId)}/>}
            </div>
          </section>

          <section className="settings-block">
            <h4>Bağlı roller</h4>
            <p className="settings-hint">Bu gruba bağlanan rolün yetkileri, gruptaki herkese geçer.</p>
            <div className="team-chips">
              {group.roles.map(item=><Tag key={item.id} handle={item.handle} color={item.color} onRemove={can.roles?()=>actions.unlinkRoleGroup(item.id,group.id):undefined}/>)}
              {!group.roles.length&&<small>Bu gruba bağlı rol yok — grubun yetkisi yalnızca @everyone rolünden gelir.</small>}
              {can.roles&&<Picker label="＋ rol bağla" empty={roles.some(item=>!item.everyone&&!item.founder)?'Tüm roller bağlı':'Önce Roller sekmesinden bir rol oluştur'}
                options={roles.filter(item=>!item.everyone&&!item.founder&&!group.roles.some(row=>row.id===item.id)).map(item=>({id:item.id,label:item.name}))}
                onPick={roleId=>actions.linkRoleGroup(roleId,group.id)}/>}
            </div>
          </section>
        </>}
      </div>
    </div>
  </div>
}
