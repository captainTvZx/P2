import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { avatarHue, userInitials } from './initials'
import { useRealtimeEvents } from './realtime'
import { useEscape } from './useEscape'

// Görev panosu: sprint'ler ve kanban sütunları. Veriler kuruluş düzeyinde
// yaşar, canlı olaylarla (task:*, sprint:*) herkeste aynı anda güncellenir.
export type Sprint={id:string;name:string;goal:string|null;status:'PLANNED'|'ACTIVE'|'DONE';startsAt:string|null;endsAt:string|null}
export type Task={id:string;number:number;title:string;description:string|null;status:TaskStatus;priority:Priority;points:number|null;position:number;assigneeId:string|null;createdById:string;sprintId:string|null;dueAt:string|null}
export type TaskStatus='BACKLOG'|'TODO'|'IN_PROGRESS'|'REVIEW'|'DONE'
type Priority='LOW'|'MEDIUM'|'HIGH'|'URGENT'
type Member={id:string;displayName:string;username?:string;avatarUrl?:string|null}

const COLUMNS:{id:TaskStatus;label:string}[]=[
  {id:'TODO',label:'Yapılacak'},
  {id:'IN_PROGRESS',label:'Devam ediyor'},
  {id:'REVIEW',label:'İncelemede'},
  {id:'DONE',label:'Bitti'},
]
const PRIORITIES:{id:Priority;label:string}[]=[
  {id:'LOW',label:'Düşük'},{id:'MEDIUM',label:'Normal'},{id:'HIGH',label:'Yüksek'},{id:'URGENT',label:'Acil'},
]
const priorityLabel=(value:Priority)=>PRIORITIES.find(item=>item.id===value)?.label||value
const dayText=(value:string|null)=>value?new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'short'}):''

function Avatar({member,size=22}:{member?:Member;size?:number}){
  if(!member)return <span className="board-avatar empty" style={{width:size,height:size}} title="Atanmadı">–</span>
  return <span className={`board-avatar ${avatarHue(member.id)}`} style={{width:size,height:size}} title={member.displayName}>
    {member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}
  </span>
}

async function request<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await p2Fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

// Panoyu sunucuya bağlayan kapsayıcı: veriyi çeker, canlı olayları dinler ve
// yazma isteklerini yapar. Görsel kısım aşağıdaki Board bileşeninde.
export default function BoardPage({auth,organization,people,perms}:{
  auth:{accessToken:string;user:{id:string;displayName:string;avatarUrl?:string|null}}
  organization:{id:string}
  people:Member[]
  perms:{can:(permission:string)=>boolean}
}){
  const token=auth.accessToken,base=`/organizations/${organization.id}/board`
  const[tasks,setTasks]=useState<Task[]>([]),[sprints,setSprints]=useState<Sprint[]>([]),[loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState('')
  const load=useCallback(()=>{
    request<{sprints:Sprint[];tasks:Task[]}>(base,token)
      .then(data=>{setSprints(data.sprints||[]);setTasks(data.tasks||[]);setError('')})
      .catch((failure:any)=>setError(failure?.message||'Pano yüklenemedi.'))
      .finally(()=>setLoading(false))
  },[base,token])
  useEffect(load,[load])
  const mergeTask=(task:Task)=>setTasks(items=>items.some(item=>item.id===task.id)?items.map(item=>item.id===task.id?task:item):[...items,task])
  useRealtimeEvents(token,organization.id,{
    'task:updated':(payload:{task:Task})=>mergeTask(payload.task),
    'task:deleted':(payload:{taskId:string})=>setTasks(items=>items.filter(item=>item.id!==payload.taskId)),
    'task:moved':(payload:{tasks:Task[]})=>setTasks(items=>{const moved=new Map(payload.tasks.map(task=>[task.id,task]));return items.map(item=>moved.get(item.id)||item)}),
    'sprint:updated':(payload:{sprint:Sprint;reload?:boolean})=>{
      setSprints(items=>items.some(item=>item.id===payload.sprint.id)?items.map(item=>item.id===payload.sprint.id?payload.sprint:item):[payload.sprint,...items])
      // Sprint aktifleşince diğerleri sunucuda planlanana döndüğü için listeyi tazele.
      if(payload.reload)load()
    },
    'sprint:deleted':(payload:{sprintId:string})=>{
      setSprints(items=>items.filter(item=>item.id!==payload.sprintId))
      setTasks(items=>items.map(task=>task.sprintId===payload.sprintId?{...task,sprintId:null}:task))
    },
  })
  async function send<T>(path:string,init:RequestInit){
    setBusy(true);setError('')
    try{return await request<T>(path,token,init)}
    catch(failure:any){setError(failure?.message||'İşlem tamamlanamadı.');throw failure}
    finally{setBusy(false)}
  }
  const quiet=(action:Promise<unknown>)=>action.catch(()=>{})

  if(loading)return <section className="board-page"><p className="board-empty">Pano yükleniyor…</p></section>
  return <Board
    tasks={tasks} sprints={sprints} members={people} me={auth.user.id}
    canCreate={perms.can('task.create')} canManage={perms.can('board.manage')}
    busy={busy} error={error}
    onCreate={draft=>quiet(send<Task>(`${base}/tasks`,{method:'POST',body:JSON.stringify(draft)}).then(mergeTask))}
    onUpdate={(id,patch)=>quiet(send<Task>(`${base}/tasks/${id}`,{method:'PATCH',body:JSON.stringify(patch)}).then(mergeTask))}
    onDelete={id=>quiet(send(`${base}/tasks/${id}`,{method:'DELETE'}).then(()=>setTasks(items=>items.filter(item=>item.id!==id))))}
    onMove={(id,status,order)=>{
      // Sürükle bırak anında görünsün diye önce yerel sıra yazılır, sonra sunucu.
      setTasks(items=>items.map(item=>item.id===id?{...item,status}:item).map(item=>{const index=order.indexOf(item.id);return index<0?item:{...item,position:index}}))
      return quiet(send(`${base}/tasks/${id}/move`,{method:'PATCH',body:JSON.stringify({status,order})}).catch(failure=>{load();throw failure}))
    }}
    onSprint={sprint=>{
      const{id,...body}=sprint
      return quiet(send<Sprint>(id?`${base}/sprints/${id}`:`${base}/sprints`,{method:id?'PATCH':'POST',body:JSON.stringify(body)}).then(()=>load()))
    }}
    onSprintDelete={id=>quiet(send(`${base}/sprints/${id}`,{method:'DELETE'}).then(()=>{
      setSprints(items=>items.filter(item=>item.id!==id))
      setTasks(items=>items.map(task=>task.sprintId===id?{...task,sprintId:null}:task))
    }))}/>
}

// UI showroom da bu görsel bileşeni sahte veriyle kullanır.
export function Board({tasks,sprints,members,me,canCreate,canManage,onCreate,onUpdate,onDelete,onMove,onSprint,onSprintDelete,busy,error}:{
  tasks:Task[]
  sprints:Sprint[]
  members:Member[]
  me:string
  canCreate:boolean
  canManage:boolean
  onCreate:(draft:Partial<Task>)=>Promise<unknown>
  onUpdate:(id:string,patch:Partial<Task>)=>Promise<unknown>
  onDelete:(id:string)=>Promise<unknown>
  onMove:(id:string,status:TaskStatus,order:string[])=>Promise<unknown>
  onSprint:(sprint:Partial<Sprint>&{id?:string})=>Promise<unknown>
  onSprintDelete:(id:string)=>Promise<unknown>
  busy?:boolean
  error?:string
}){
  const active=sprints.find(item=>item.status==='ACTIVE')||null
  const[scope,setScope]=useState<string>(active?active.id:'backlog')
  const[assignee,setAssignee]=useState<'all'|'mine'|string>('all')
  const[search,setSearch]=useState('')
  const[editing,setEditing]=useState<Task|null>(null)
  const[creating,setCreating]=useState<TaskStatus|null>(null)
  const[sprintPanel,setSprintPanel]=useState(false)
  const dragged=useRef<string>('')
  const[dropTarget,setDropTarget]=useState<TaskStatus|''>('')
  useEffect(()=>{if(active&&scope==='backlog'&&!sprints.some(item=>item.id===scope))setScope(active.id)},[active?.id])

  const person=(id:string|null)=>members.find(item=>item.id===id)||undefined
  const visible=useMemo(()=>{
    const needle=search.trim().toLocaleLowerCase('tr-TR')
    return tasks.filter(task=>{
      if(scope==='backlog'?task.sprintId:task.sprintId!==scope)return false
      if(assignee==='mine'&&task.assigneeId!==me)return false
      if(assignee!=='all'&&assignee!=='mine'&&task.assigneeId!==assignee)return false
      if(needle&&!`${task.number} ${task.title}`.toLocaleLowerCase('tr-TR').includes(needle))return false
      return true
    })
  },[tasks,scope,assignee,me,search])
  // Sunucu BACKLOG durumunu da kabul eder; panoda backlog ayrı bir sütun değil
  // (sprint'i olmayan görev), o yüzden böyle bir kart "Yapılacak"ta görünür.
  const columnOf=(task:Task)=>task.status==='BACKLOG'?'TODO':task.status
  const byColumn=(status:TaskStatus)=>visible.filter(task=>columnOf(task)===status).sort((a,b)=>a.position-b.position||a.number-b.number)
  const points=(status:TaskStatus)=>byColumn(status).reduce((total,task)=>total+(task.points||0),0)

  function drop(event:DragEvent,status:TaskStatus,beforeId?:string){
    event.preventDefault()
    setDropTarget('')
    const id=dragged.current||event.dataTransfer.getData('text/plain')
    dragged.current=''
    if(!id)return
    const task=tasks.find(item=>item.id===id)
    if(!task)return
    // Yeni sütunun sırası: sürüklenen kart hedef kartın önüne yerleşir.
    const column=byColumn(status).filter(item=>item.id!==id)
    const index=beforeId?column.findIndex(item=>item.id===beforeId):column.length
    const order=[...column.slice(0,index<0?column.length:index),task,...column.slice(index<0?column.length:index)].map(item=>item.id)
    void onMove(id,status,order)
  }

  return <section className="board-page">
    <header className="board-head">
      <div className="board-title">
        <h2>Görevler</h2>
        <p>{scope==='backlog'?'Sprint’e alınmamış işler':sprints.find(item=>item.id===scope)?.goal||'Sprint panosu'}</p>
      </div>
      <div className="board-controls">
        <select value={scope} onChange={event=>setScope(event.target.value)} aria-label="Sprint seç">
          {sprints.map(sprint=><option key={sprint.id} value={sprint.id}>{sprint.name}{sprint.status==='ACTIVE'?' · aktif':sprint.status==='DONE'?' · bitti':''}</option>)}
          <option value="backlog">Backlog</option>
        </select>
        <select value={assignee} onChange={event=>setAssignee(event.target.value)} aria-label="Kişiye göre">
          <option value="all">Herkes</option>
          <option value="mine">Bana atananlar</option>
          {members.map(member=><option key={member.id} value={member.id}>{member.displayName}</option>)}
        </select>
        <div className="admin-search"><span>⌕</span><input value={search} onChange={event=>setSearch(event.target.value)} placeholder="Görev ara"/></div>
        {canManage&&<button type="button" className="board-button" onClick={()=>setSprintPanel(true)}>Sprint’ler</button>}
        {canCreate&&<button type="button" className="board-button primary" onClick={()=>setCreating('TODO')}>Yeni görev</button>}
      </div>
    </header>

    {/* Uyarı satırları hep aynı ızgara satırında: yokken sütunlar yukarı kaymaz. */}
    <div className="board-notes">
      {error&&<p className="board-note error">{error}</p>}
      {!sprints.length&&canManage&&<p className="board-note">Henüz sprint yok. “Sprint’ler” bölümünden ilk sprint’i açabilirsin; görevler o zamana kadar backlog’da birikir.</p>}
    </div>

    <div className="board-columns">
      {COLUMNS.map(column=>{
        const cards=byColumn(column.id)
        return <section key={column.id} className={dropTarget===column.id?'board-column over':'board-column'}
          onDragOver={event=>{event.preventDefault();setDropTarget(column.id)}}
          onDragLeave={()=>setDropTarget(current=>current===column.id?'':current)}
          onDrop={event=>drop(event,column.id)}>
          <header>
            <b>{column.label}</b>
            <span>{cards.length}{points(column.id)?` · ${points(column.id)} puan`:''}</span>
            {canCreate&&<button type="button" title="Bu sütuna görev ekle" onClick={()=>setCreating(column.id)}>＋</button>}
          </header>
          <div className="board-cards">
            {cards.map(task=><article key={task.id} className={`board-card priority-${task.priority.toLowerCase()}`} draggable
              onDragStart={event=>{dragged.current=task.id;event.dataTransfer.setData('text/plain',task.id);event.dataTransfer.effectAllowed='move'}}
              onDragEnd={()=>{dragged.current='';setDropTarget('')}}
              onDrop={event=>{event.stopPropagation();drop(event,column.id,task.id)}}
              onClick={()=>setEditing(task)}>
              <div className="board-card-top">
                <code>#{task.number}</code>
                {task.points!==null&&<em>{task.points}p</em>}
                <i className="board-priority" title={priorityLabel(task.priority)}/>
              </div>
              <p>{task.title}</p>
              <footer>
                <Avatar member={person(task.assigneeId)}/>
                {task.dueAt&&<small className={new Date(task.dueAt)<new Date()&&task.status!=='DONE'?'late':''}>{dayText(task.dueAt)}</small>}
              </footer>
            </article>)}
            {!cards.length&&<p className="board-empty">Kart yok</p>}
          </div>
        </section>
      })}
    </div>

    {(creating||editing)&&<TaskDialog
      task={editing}
      status={creating||'TODO'}
      sprintId={scope==='backlog'?null:scope}
      members={members}
      sprints={sprints}
      canDelete={Boolean(editing&&(canManage||editing.createdById===me||editing.assigneeId===me))}
      busy={busy}
      onClose={()=>{setCreating(null);setEditing(null)}}
      onSave={async draft=>{
        if(editing)await onUpdate(editing.id,draft)
        else await onCreate(draft)
        setCreating(null);setEditing(null)
      }}
      onDelete={async()=>{if(editing){await onDelete(editing.id);setEditing(null)}}}/>}

    {sprintPanel&&<SprintDialog sprints={sprints} busy={busy} onClose={()=>setSprintPanel(false)} onSave={onSprint} onDelete={onSprintDelete}/>}
  </section>
}

function TaskDialog({task,status,sprintId,members,sprints,canDelete,busy,onClose,onSave,onDelete}:{
  task:Task|null;status:TaskStatus;sprintId:string|null;members:Member[];sprints:Sprint[]
  canDelete:boolean;busy?:boolean;onClose:()=>void;onSave:(draft:Partial<Task>)=>Promise<unknown>;onDelete:()=>Promise<unknown>
}){
  const[title,setTitle]=useState(task?.title||'')
  const[description,setDescription]=useState(task?.description||'')
  const[assigneeId,setAssigneeId]=useState(task?.assigneeId||'')
  const[priority,setPriority]=useState<Priority>(task?.priority||'MEDIUM')
  const[points,setPoints]=useState(task?.points?.toString()||'')
  const[dueAt,setDueAt]=useState(task?.dueAt?task.dueAt.slice(0,10):'')
  const[sprint,setSprint]=useState(task?task.sprintId||'':sprintId||'')
  const[column,setColumn]=useState<TaskStatus>(task?.status||status)
  useEscape(true,onClose)
  const save=()=>{
    if(title.trim().length<2)return
    void onSave({
      title:title.trim(),description,assigneeId,priority,status:column,
      points:points.trim()===''?null:Number(points),
      dueAt:dueAt?new Date(dueAt).toISOString():null,
      sprintId:sprint||null,
    } as Partial<Task>)
  }
  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal board-dialog">
      <header>
        <div><b>{task?`#${task.number} görevi düzenle`:'Yeni görev'}</b><small>{task?'Değişiklikler herkese anında yansır.':'Panoya yeni bir kart ekler.'}</small></div>
        <button type="button" onClick={onClose} aria-label="Kapat">×</button>
      </header>
      <label className="board-field"><span>Başlık</span>
        <input autoFocus value={title} onChange={event=>setTitle(event.target.value)} placeholder="Ne yapılacak?" onKeyDown={event=>{if(event.key==='Enter')save()}}/>
      </label>
      <label className="board-field"><span>Açıklama</span>
        <textarea value={description} onChange={event=>setDescription(event.target.value)} rows={4} placeholder="Ayrıntılar, kabul kriterleri…"/>
      </label>
      <div className="board-grid">
        <label className="board-field"><span>Atanan</span>
          <select value={assigneeId} onChange={event=>setAssigneeId(event.target.value)}>
            <option value="">Atanmadı</option>
            {members.map(member=><option key={member.id} value={member.id}>{member.displayName}</option>)}
          </select>
        </label>
        <label className="board-field"><span>Öncelik</span>
          <select value={priority} onChange={event=>setPriority(event.target.value as Priority)}>
            {PRIORITIES.map(item=><option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
        </label>
        <label className="board-field"><span>Sütun</span>
          <select value={column} onChange={event=>setColumn(event.target.value as TaskStatus)}>
            {COLUMNS.map(item=><option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
        </label>
        <label className="board-field"><span>Sprint</span>
          <select value={sprint} onChange={event=>setSprint(event.target.value)}>
            <option value="">Backlog</option>
            {sprints.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        <label className="board-field"><span>Puan</span>
          <input value={points} inputMode="numeric" onChange={event=>setPoints(event.target.value.replace(/[^0-9]/g,'').slice(0,3))} placeholder="—"/>
        </label>
        <label className="board-field"><span>Bitiş</span>
          <input type="date" value={dueAt} onChange={event=>setDueAt(event.target.value)}/>
        </label>
      </div>
      <footer>
        {task&&canDelete&&<button type="button" className="board-button danger" disabled={busy} onClick={()=>void onDelete()}>Sil</button>}
        <span className="board-spacer"/>
        <button type="button" className="board-button" onClick={onClose}>Vazgeç</button>
        <button type="button" className="board-button primary" disabled={busy||title.trim().length<2} onClick={save}>{task?'Kaydet':'Oluştur'}</button>
      </footer>
    </section>
  </div>
}

function SprintDialog({sprints,busy,onClose,onSave,onDelete}:{
  sprints:Sprint[];busy?:boolean;onClose:()=>void
  onSave:(sprint:Partial<Sprint>&{id?:string})=>Promise<unknown>;onDelete:(id:string)=>Promise<unknown>
}){
  const[name,setName]=useState('')
  const[goal,setGoal]=useState('')
  const[endsAt,setEndsAt]=useState('')
  useEscape(true,onClose)
  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal board-dialog">
      <header><div><b>Sprint’ler</b><small>Aynı anda tek sprint aktif olur; yeni sprint başlayınca diğeri planlanana döner.</small></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="sprint-list">
        {sprints.map(sprint=><article key={sprint.id} className={`sprint-row ${sprint.status.toLowerCase()}`}>
          <div><b>{sprint.name}</b><small>{sprint.goal||'Hedef yazılmadı'}</small></div>
          <span className="sprint-state">{sprint.status==='ACTIVE'?'Aktif':sprint.status==='DONE'?'Bitti':'Planlandı'}{sprint.endsAt?` · ${dayText(sprint.endsAt)}`:''}</span>
          <div className="sprint-actions">
            {sprint.status!=='ACTIVE'&&<button type="button" className="board-button" disabled={busy} onClick={()=>void onSave({id:sprint.id,status:'ACTIVE'})}>Başlat</button>}
            {sprint.status==='ACTIVE'&&<button type="button" className="board-button" disabled={busy} onClick={()=>void onSave({id:sprint.id,status:'DONE'})}>Bitir</button>}
            <button type="button" className="board-button danger" disabled={busy} onClick={()=>void onDelete(sprint.id)}>Sil</button>
          </div>
        </article>)}
        {!sprints.length&&<p className="board-empty">Henüz sprint yok.</p>}
      </div>
      <div className="board-grid">
        <label className="board-field wide"><span>Yeni sprint adı</span><input value={name} onChange={event=>setName(event.target.value)} placeholder="Sprint 1"/></label>
        <label className="board-field wide"><span>Hedef</span><input value={goal} onChange={event=>setGoal(event.target.value)} placeholder="Bu sprint’te ne bitecek?"/></label>
        <label className="board-field"><span>Bitiş</span><input type="date" value={endsAt} onChange={event=>setEndsAt(event.target.value)}/></label>
      </div>
      <footer>
        <span className="board-spacer"/>
        <button type="button" className="board-button" onClick={onClose}>Kapat</button>
        <button type="button" className="board-button primary" disabled={busy||name.trim().length<2} onClick={async()=>{
          await onSave({name:name.trim(),goal:goal.trim(),endsAt:endsAt?new Date(endsAt).toISOString():undefined})
          setName('');setGoal('');setEndsAt('')
        }}>Sprint oluştur</button>
      </footer>
    </section>
  </div>
}
