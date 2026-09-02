import { FormEvent, useState } from 'react'
import { avatarHue, userInitials } from './initials'
import { useEscape } from './useEscape'
import type { GroupMember } from './groupTypes'

// Elle grup kurma: kişileri seç, istersen bir ad ver.
export default function GroupCreator({people,busy,error,onClose,onCreate}:{
  people:GroupMember[]
  busy:boolean
  error:string
  onClose:()=>void
  onCreate:(name:string,userIds:string[])=>void
}){
  const[selected,setSelected]=useState<string[]>([])
  const[name,setName]=useState('')
  const[search,setSearch]=useState('')
  useEscape(true,onClose)

  const normalized=search.trim().toLocaleLowerCase('tr-TR')
  const visible=people.filter(person=>!normalized||person.displayName.toLocaleLowerCase('tr-TR').includes(normalized)||person.username.toLocaleLowerCase('tr-TR').includes(normalized))
  const toggle=(id:string)=>setSelected(current=>current.includes(id)?current.filter(item=>item!==id):[...current,id])

  function submit(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    if(!selected.length||busy)return
    onCreate(name.trim(),selected)
  }

  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <form className="share-modal group-modal" onSubmit={submit}>
      <header>
        <div><b>Grup oluştur</b><small>Birlikte yazışacağın kişileri seç</small></div>
        <button type="button" onClick={onClose} aria-label="Kapat">×</button>
      </header>

      <label className="group-name">Grup adı<input value={name} onChange={event=>setName(event.target.value.slice(0,60))} placeholder="İsteğe bağlı — boş bırakırsan isimlerden oluşur"/></label>
      <input className="group-search" value={search} onChange={event=>setSearch(event.target.value)} placeholder="Kişi ara"/>

      <div className="group-people">
        {visible.map(person=><button type="button" key={person.id} className={selected.includes(person.id)?'group-person active':'group-person'} onClick={()=>toggle(person.id)}>
          <span className={`person-avatar ${avatarHue(person.id)}`}>{userInitials(person.displayName)}</span>
          <span className="group-person-name"><b>{person.displayName}</b><small>@{person.username}</small></span>
          <i className={selected.includes(person.id)?'group-check on':'group-check'}/>
        </button>)}
        {!visible.length&&<p className="share-empty">Kişi bulunamadı.</p>}
      </div>

      {error&&<p className="call-error">{error}</p>}
      <footer>
        <span className="group-count">{selected.length} kişi seçildi</span>
        <button type="button" className="share-cancel" onClick={onClose}>Vazgeç</button>
        <button type="submit" className="share-start" disabled={busy||!selected.length}>{busy?'Oluşturuluyor…':'Grubu oluştur'}</button>
      </footer>
    </form>
  </div>
}
