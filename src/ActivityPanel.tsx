import { avatarHue, userInitials } from './initials'
import { CallIcon } from './CallIcons'
import { useEscape } from './useEscape'

export type ActivityItem={
  id:string
  kind:'MENTION'|'REPLY'|'REACTION'
  preview:string
  emoji:string|null
  projectId:string|null
  channelId:string|null
  channelName:string|null
  groupId:string|null
  groupName:string|null
  messageId:string|null
  parentId:string|null
  readAt:string|null
  createdAt:string
  actor:{id:string;username:string;displayName:string}
}

// "3 dk önce", "dün 14:20" gibi kısa zaman etiketi.
function when(value:string){
  const date=new Date(value),now=Date.now(),diff=(now-date.getTime())/1000
  if(diff<60)return 'az önce'
  if(diff<3600)return `${Math.floor(diff/60)} dk önce`
  if(diff<86400)return `${Math.floor(diff/3600)} sa önce`
  if(diff<172800)return `dün ${date.toLocaleTimeString('tr-TR',{hour:'2-digit',minute:'2-digit'})}`
  return date.toLocaleDateString('tr-TR',{day:'numeric',month:'long'})
}

function title(item:ActivityItem){
  if(item.kind==='MENTION')return 'seni etiketledi'
  if(item.kind==='REPLY')return 'mesajına yanıt verdi'
  return `mesajına ${item.emoji||'👍'} tepkisi verdi`
}

function place(item:ActivityItem){
  if(item.groupName)return item.groupName
  if(item.channelName)return `#${item.channelName}`
  return 'Sohbet'
}

export default function ActivityPanel({items,loading,onClose,onOpen,onReadAll}:{
  items:ActivityItem[]
  loading:boolean
  onClose:()=>void
  onOpen:(item:ActivityItem)=>void
  onReadAll:()=>void
}){
  useEscape(true,onClose)
  const unread=items.filter(item=>!item.readAt).length
  return <aside className="activity-panel">
    <header>
      <div><b>Etkinlik</b><small>{unread?`${unread} yeni`:'Hepsi okundu'}</small></div>
      <span className="activity-head-actions">
        {unread>0&&<button type="button" className="activity-read" onClick={onReadAll}>Tümünü okundu say</button>}
        <button type="button" onClick={onClose} aria-label="Kapat">×</button>
      </span>
    </header>

    <div className="activity-list">
      {loading&&<p className="activity-empty">Yükleniyor…</p>}
      {!loading&&!items.length&&<div className="activity-blank">
        <span><CallIcon name="personAdd"/></span>
        <b>Henüz etkinlik yok</b>
        <p>Biri seni etiketlediğinde, mesajına yanıt verdiğinde ya da tepki bıraktığında burada görürsün.</p>
      </div>}
      {items.map(item=><button type="button" key={item.id} className={item.readAt?'activity-row':'activity-row unread'} onClick={()=>onOpen(item)}>
        <span className={`activity-avatar ${avatarHue(item.actor.id)}`}>
          {userInitials(item.actor.displayName)}
          <i className={`activity-kind ${item.kind.toLowerCase()}`}>{item.kind==='MENTION'?'@':item.kind==='REPLY'?'↩':item.emoji||'♥'}</i>
        </span>
        <span className="activity-body">
          <span className="activity-title"><b>{item.actor.displayName}</b> {title(item)}</span>
          <span className="activity-preview">{item.preview||'(mesaj)'}</span>
          <span className="activity-meta">{place(item)} · {when(item.createdAt)}</span>
        </span>
      </button>)}
    </div>
  </aside>
}
