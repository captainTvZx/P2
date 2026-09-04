import { avatarHue, userInitials } from './initials'
import { ResizeHandle, useResizable } from './useResizable'

export type ProfileUser={id:string;username:string;displayName:string;email?:string;title?:string|null;avatarUrl?:string|null}
export type ProfileMember=ProfileUser&{status:string;role:string}

export function presenceClass(status?:string){return status==='ONLINE'?'presence online':status==='AWAY'?'presence away':status==='BUSY'?'presence busy':'presence'}
export function statusLabel(status?:string){return status==='ONLINE'?'Çevrimiçi':status==='AWAY'?'Uzakta':status==='BUSY'?'Meşgul':status==='INVISIBLE'?'Görünmez':'Çevrimdışı'}
function roleLabel(role?:string){return role==='OWNER'?'Kurucu':role==='ADMIN'?'Yönetici':role?'Üye':''}

export default function ProfilePanel({user,member,onClose,onMention,onDirect}:{user:ProfileUser;member?:ProfileMember;onClose:()=>void;onMention?:()=>void;onDirect?:()=>void}){
  const panel=useResizable('p2:profile-width',340,280,580,-1)
  const role=roleLabel(member?.role)
  return <aside className="profile-panel" aria-label="Profil" style={{width:panel.width}}>
    <ResizeHandle edge="left" onMouseDown={panel.start} label="Profil genişliği"/>
    <header><b>Profil</b><button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button></header>
    <div className="profile-scroll">
      <div className={`profile-portrait ${avatarHue(user.id)}`}>{(member?.avatarUrl||user.avatarUrl)?<img src={member?.avatarUrl||user.avatarUrl||''} alt=""/>:<span>{userInitials(user.displayName)}</span>}</div>
      <h3>{user.displayName}</h3>
      {(member?.title||user.title)&&<p className="profile-title">{member?.title||user.title}</p>}
      <p className={member?.status==='ONLINE'?'profile-line online':member?.status==='AWAY'?'profile-line away':member?.status==='BUSY'?'profile-line busy':'profile-line'}><i className={presenceClass(member?.status)}/>{statusLabel(member?.status)}</p>
      {role&&<p className="profile-line"><em className="profile-role">{role}</em></p>}
      {(onDirect||onMention)&&<div className="profile-actions">
        {onDirect&&<button type="button" className="primary" onClick={onDirect}>Mesaj gönder</button>}
        {onMention&&<button type="button" onClick={onMention}>@ ile etiketle</button>}
      </div>}
      <div className="profile-section">
        <b>İletişim bilgileri</b>
        <div className="profile-field"><span>@</span><div><small>Kullanıcı adı</small><a href={`p2://users/${user.username}`} onClick={e=>{e.preventDefault();void navigator.clipboard.writeText(`@${user.username}`)}} title="Kopyalamak için tıkla">@{user.username}</a></div></div>
        {(member?.email||user.email)&&<div className="profile-field"><span>✉</span><div><small>E-posta</small><a href={`mailto:${member?.email||user.email}`}>{member?.email||user.email}</a></div></div>}
      </div>
    </div>
  </aside>
}
