import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import ConfirmDialog from './ConfirmDialog'
import { avatarHue, userInitials } from './initials'
import { useRealtimeEvents } from './realtime'
import { useEscape } from './useEscape'

// Görev panosu: sprint'ler ve kanban sütunları. Her proje kendi panosunu
// taşır (görev numaraları da proje içinde sıralıdır); canlı olaylarla
// (task:*, sprint:*) ekipte herkeste aynı anda güncellenir.
export type Sprint={id:string;name:string;goal:string|null;status:'PLANNED'|'ACTIVE'|'DONE';startsAt:string|null;endsAt:string|null;projectId?:string;boardId?:string}
// Bir projede birden çok pano olur: her takım kendi kanbanını yönetir.
export type BoardInfo={id:string;name:string;position:number;projectId:string}
export type Task={id:string;number:number;title:string;description:string|null;status:TaskStatus;priority:Priority;points:number|null;position:number;assigneeId:string|null;createdById:string;sprintId:string|null;projectId:string;boardId?:string;dueAt:string|null}
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

function Avatar({member,size=24,label}:{member?:Member;size?:number;label?:string}){
  const box={width:size,height:size,fontSize:Math.max(8,Math.round(size*0.36))}
  if(!member)return <span className="board-avatar empty" style={box} title={label||'Atanmadı'} aria-label={label||'Atanmadı'}>+</span>
  const title=label?`${label}: ${member.displayName}`:member.displayName
  return <span className={`board-avatar ${avatarHue(member.id)}`} style={box} title={title} aria-label={title}>
    {member.avatarUrl?<img src={member.avatarUrl} alt=""/>:userInitials(member.displayName)}
  </span>
}

// Sprint ilerlemesi: bugünün aralıktaki yeri ve kalan gün.
function sprintProgress(sprint?:Sprint|null){
  if(!sprint?.startsAt||!sprint?.endsAt)return null
  const start=new Date(sprint.startsAt).getTime(),end=new Date(sprint.endsAt).getTime(),now=Date.now()
  if(!(end>start))return null
  const percent=Math.min(100,Math.max(0,Math.round(((now-start)/(end-start))*100)))
  const left=Math.ceil((end-now)/86400000)
  return {percent,left}
}

async function request<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await p2Fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}

// Panoyu sunucuya bağlayan kapsayıcı: veriyi çeker, canlı olayları dinler ve
// yazma isteklerini yapar. Görsel kısım aşağıdaki Board bileşeninde.
export default function BoardPage({auth,organization,project,people,perms}:{
  auth:{accessToken:string;user:{id:string;displayName:string;avatarUrl?:string|null}}
  organization:{id:string}
  project:{id:string;name:string}
  people:Member[]
  perms:{can:(permission:string)=>boolean}
}){
  const token=auth.accessToken,root=`/organizations/${organization.id}/projects/${project.id}/boards`
  const[boards,setBoards]=useState<BoardInfo[]>([])
  // Hangi panodaydık: proje bazında hatırlanır.
  const memory=`p2:board:${project.id}`
  const[boardId,setBoardId]=useState<string>(()=>{try{return localStorage.getItem(memory)||''}catch{return ''}})
  const[tasks,setTasks]=useState<Task[]>([]),[sprints,setSprints]=useState<Sprint[]>([]),[loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState('')
  const base=boardId?`${root}/${boardId}`:''
  const pick=useCallback((id:string)=>{setBoardId(id);try{localStorage.setItem(memory,id)}catch{}},[memory])
  const loadBoards=useCallback(()=>{
    request<BoardInfo[]>(root,token)
      .then(list=>{
        setBoards(list||[])
        // Kayıtlı pano silinmişse ilk panoya düşülür.
        setBoardId(current=>{
          const next=list?.some(item=>item.id===current)?current:(list?.[0]?.id||'')
          if(next!==current){try{localStorage.setItem(memory,next)}catch{}}
          return next
        })
      })
      .catch((failure:any)=>{setError(failure?.message||'Panolar yüklenemedi.');setLoading(false)})
  },[root,token,memory])
  useEffect(loadBoards,[loadBoards])
  const load=useCallback(()=>{
    if(!base){setLoading(false);return}
    request<{sprints:Sprint[];tasks:Task[]}>(base,token)
      .then(data=>{setSprints(data.sprints||[]);setTasks(data.tasks||[]);setError('')})
      .catch((failure:any)=>setError(failure?.message||'Pano yüklenemedi.'))
      .finally(()=>setLoading(false))
  },[base,token])
  useEffect(()=>{setLoading(true);load()},[load])
  const mergeTask=(task:Task)=>setTasks(items=>items.some(item=>item.id===task.id)?items.map(item=>item.id===task.id?task:item):[...items,task])
  // Canlı olaylar kuruluşun tamamına yayınlanır; başka panoya ait olanlar
  // burada elenir (pano kimliği yoksa projeye bakılır).
  const mine=(value:string|undefined,board?:string)=>board?board===boardId:(!value||value===project.id)
  useRealtimeEvents(token,organization.id,{
    'board:updated':(payload:{board:BoardInfo})=>{
      if(payload.board?.projectId!==project.id)return
      setBoards(items=>items.some(item=>item.id===payload.board.id)?items.map(item=>item.id===payload.board.id?payload.board:item):[...items,payload.board].sort((a,b)=>a.position-b.position))
    },
    'board:deleted':(payload:{boardId:string;projectId:string})=>{
      if(payload.projectId!==project.id)return
      setBoards(items=>items.filter(item=>item.id!==payload.boardId))
      // Açık olan pano silindiyse ilk panoya geçilir.
      setBoardId(current=>{
        if(current!==payload.boardId)return current
        const next=boards.find(item=>item.id!==payload.boardId)?.id||''
        try{localStorage.setItem(memory,next)}catch{}
        return next
      })
    },
    'task:updated':(payload:{task:Task})=>{if(mine(payload.task?.projectId,payload.task?.boardId))mergeTask(payload.task)},
    'task:deleted':(payload:{taskId:string;projectId?:string;boardId?:string})=>{if(mine(payload.projectId,payload.boardId))setTasks(items=>items.filter(item=>item.id!==payload.taskId))},
    'task:moved':(payload:{tasks:Task[];projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
      setTasks(items=>{const moved=new Map(payload.tasks.map(task=>[task.id,task]));return items.map(item=>moved.get(item.id)||item)})
    },
    'sprint:updated':(payload:{sprint:Sprint;reload?:boolean})=>{
      if(!mine(payload.sprint?.projectId,payload.sprint?.boardId))return
      setSprints(items=>items.some(item=>item.id===payload.sprint.id)?items.map(item=>item.id===payload.sprint.id?payload.sprint:item):[payload.sprint,...items])
      // Sprint aktifleşince diğerleri sunucuda planlanana döndüğü için listeyi tazele.
      if(payload.reload)load()
    },
    'sprint:deleted':(payload:{sprintId:string;projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
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
  if(!boardId)return <section className="board-page"><p className="board-empty">{error||'Bu projede henüz pano yok.'}</p></section>
  return <Board
    projectName={project.name}
    boards={boards} boardId={boardId} onSelectBoard={pick}
    onCreateBoard={name=>quiet(send<BoardInfo>(root,{method:'POST',body:JSON.stringify({name})}).then(board=>{
      setBoards(items=>[...items.filter(item=>item.id!==board.id),board].sort((a,b)=>a.position-b.position))
      pick(board.id)
    }))}
    onRenameBoard={(id,name)=>quiet(send<BoardInfo>(`${root}/${id}`,{method:'PATCH',body:JSON.stringify({name})}).then(board=>
      setBoards(items=>items.map(item=>item.id===board.id?board:item))))}
    onDeleteBoard={id=>quiet(send(`${root}/${id}`,{method:'DELETE'}).then(()=>{
      const kalan=boards.filter(item=>item.id!==id)
      setBoards(kalan)
      if(boardId===id)pick(kalan[0]?.id||'')
    }))}
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


// Kanban kartı. Board'un içinde tanımlanmamalı: iç içe tanımlanan bileşen her
// render'da yeni bir tip olur, React kartı söküp yeniden kurar ve sürükleme
// yarıda kopar.
function Card({task,owner,drag}:{task:Task;owner?:Member;drag:Record<string,unknown>}){
  const late=task.dueAt&&new Date(task.dueAt)<new Date()&&task.status!=='DONE'
  return <article {...drag}>
    <div className="board-card-top">
      <code>#{task.number}</code>
      {task.points!==null&&<em>{task.points}p</em>}
      <i className="board-priority" title={`Öncelik: ${priorityLabel(task.priority)}`}/>
    </div>
    <p>{task.title}</p>
    <footer>
      <Avatar member={owner} label="Atanan"/>
      {task.description&&<span className="board-card-mark" title="Açıklaması var">≡</span>}
      {task.dueAt&&<small className={late?'late':''} title={new Date(task.dueAt).toLocaleDateString('tr-TR')}>{dayText(task.dueAt)}</small>}
    </footer>
  </article>
}

// UI showroom da bu görsel bileşeni sahte veriyle kullanır.
export function Board({projectName,boards,boardId,onSelectBoard,onCreateBoard,onRenameBoard,onDeleteBoard,tasks,sprints,members,me,canCreate,canManage,onCreate,onUpdate,onDelete,onMove,onSprint,onSprintDelete,busy,error}:{
  projectName?:string
  boards?:BoardInfo[]
  boardId?:string
  onSelectBoard?:(id:string)=>void
  onCreateBoard?:(name:string)=>Promise<unknown>
  onRenameBoard?:(id:string,name:string)=>Promise<unknown>
  onDeleteBoard?:(id:string)=>Promise<unknown>
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
  const[boardMenu,setBoardMenu]=useState(false)
  const[renaming,setRenaming]=useState<BoardInfo|null>(null)
  const[addingBoard,setAddingBoard]=useState(false)
  const[removingBoard,setRemovingBoard]=useState<BoardInfo|null>(null)
  const list=boards||[]
  const current=list.find(item=>item.id===boardId)
  // Pano mu liste mi: tercih kişide kalır.
  const[view,setView]=useState<'board'|'list'>(()=>{try{return localStorage.getItem('p2:board-view')==='list'?'list':'board'}catch{return 'board'}})
  const choose=(next:'board'|'list')=>{setView(next);try{localStorage.setItem('p2:board-view',next)}catch{}}
  const dragged=useRef<string>('')
  const[dragging,setDragging]=useState('')
  // Sürüklerken kartın nereye düşeceği: hedef sütun ve hangi kartın önü.
  const[slot,setSlot]=useState<{status:TaskStatus;before:string|null}|null>(null)
  const[flash,setFlash]=useState('')
  useEffect(()=>{if(active&&scope==='backlog'&&!sprints.some(item=>item.id===scope))setScope(active.id)},[active?.id])
  useEffect(()=>{if(!flash)return;const timer=window.setTimeout(()=>setFlash(''),900);return()=>window.clearTimeout(timer)},[flash])

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

  function endDrag(){dragged.current='';setDragging('');setSlot(null)}
  // Fare kartın üst yarısındaysa kart onun önüne, alt yarısındaysa arkasına düşer.
  function aim(event:DragEvent,status:TaskStatus,task?:Task){
    event.preventDefault()
    event.dataTransfer.dropEffect='move'
    if(!task){setSlot(current=>current&&current.status===status&&current.before===null?current:{status,before:null});return}
    const box=(event.currentTarget as HTMLElement).getBoundingClientRect()
    const before=event.clientY<box.top+box.height/2?task.id:null
    const list=byColumn(status)
    const next=before||list[list.findIndex(item=>item.id===task.id)+1]?.id||null
    setSlot(current=>current&&current.status===status&&current.before===next?current:{status,before:next})
  }
  function drop(event:DragEvent){
    event.preventDefault()
    event.stopPropagation()
    const target=slot
    const id=dragged.current||event.dataTransfer.getData('text/plain')
    endDrag()
    if(!id||!target)return
    const task=tasks.find(item=>item.id===id)
    if(!task)return
    const column=byColumn(target.status).filter(item=>item.id!==id)
    const index=target.before?column.findIndex(item=>item.id===target.before):column.length
    const at=index<0?column.length:index
    const order=[...column.slice(0,at),task,...column.slice(at)].map(item=>item.id)
    if(task.status===target.status&&order.join()===byColumn(target.status).map(item=>item.id).join())return
    setFlash(id)
    void onMove(id,target.status,order)
  }
  const cardProps=(task:Task,status:TaskStatus)=>({
    className:`board-card priority-${task.priority.toLowerCase()}${dragging===task.id?' dragging':''}${flash===task.id?' flash':''}`,
    draggable:true,
    onDragStart:(event:DragEvent)=>{dragged.current=task.id;setDragging(task.id);event.dataTransfer.setData('text/plain',task.id);event.dataTransfer.effectAllowed='move'},
    onDragEnd:endDrag,
    onDragOver:(event:DragEvent)=>aim(event,status,task),
    onDrop:drop,
    onClick:()=>setEditing(task),
  })

  // Boşluk yalnızca sürükleme sürerken çizilir: dragend kaçsa bile iz kalmaz.
  const slotHere=(status:TaskStatus,before:string|null)=>Boolean(dragging)&&slot?.status===status&&slot.before===before
  const progress=sprintProgress(scope==='backlog'?null:sprints.find(item=>item.id===scope))

  return <section className="board-page">
    {list.length>0&&<nav className="board-tabs" aria-label="Panolar">
      <span className="board-tabs-project">{projectName}</span>
      <div className="board-tabs-list">
        {list.map(item=><button type="button" key={item.id} className={item.id===boardId?'board-tab active':'board-tab'}
          onClick={()=>onSelectBoard?.(item.id)}
          onDoubleClick={()=>{if(canManage)setRenaming(item)}}
          title={item.id===boardId&&canManage?'Çift tıkla: yeniden adlandır':item.name}>{item.name}</button>)}
        {canManage&&<button type="button" className="board-tab add" onClick={()=>setAddingBoard(true)} title="Yeni pano">＋</button>}
      </div>
      {canManage&&current&&<div className="board-tab-tools">
        <button type="button" className="board-button subtle" onClick={()=>setBoardMenu(open=>!open)} aria-label="Pano ayarları">⋯</button>
        {boardMenu&&<div className="board-tab-menu" onMouseLeave={()=>setBoardMenu(false)}>
          <button type="button" onClick={()=>{setBoardMenu(false);setRenaming(current)}}>Panoyu yeniden adlandır</button>
          <button type="button" className="danger" disabled={list.length<2} onClick={()=>{setBoardMenu(false);setRemovingBoard(current)}}>
            {list.length<2?'Son pano silinemez':'Panoyu sil'}
          </button>
        </div>}
      </div>}
    </nav>}

    <header className="board-head">
      <div className="board-title">
        <h2>{current?current.name:(projectName?`${projectName} · Görevler`:'Görevler')}</h2>
        <p>
          <span>{scope==='backlog'?'Sprint’e alınmamış işler':sprints.find(item=>item.id===scope)?.goal||'Sprint panosu'}</span>
          {progress&&<span className="board-sprint-progress" title={`Sprint %${progress.percent} tamamlandı`}>
            <i style={{['--board-sprint-fill' as any]:`${progress.percent}%`}}/>
            <b>{progress.left>0?`${progress.left} gün kaldı`:'süre doldu'}</b>
          </span>}
        </p>
      </div>
      <div className="board-controls">
        <div className="board-views" role="tablist" aria-label="Görünüm">
          <button type="button" role="tab" aria-selected={view==='board'} className={view==='board'?'active':undefined} onClick={()=>choose('board')}>Pano</button>
          <button type="button" role="tab" aria-selected={view==='list'} className={view==='list'?'active':undefined} onClick={()=>choose('list')}>Liste</button>
        </div>
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

    {view==='board'&&<div className="board-columns">
      {COLUMNS.map(column=>{
        const cards=byColumn(column.id)
        return <section key={column.id} className={`board-column ${column.id.toLowerCase()}${dragging&&slot?.status===column.id?' over':''}`}
          onDragOver={event=>aim(event,column.id)}
          onDragLeave={event=>{if(!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node))setSlot(current=>current?.status===column.id?null:current)}}
          onDrop={drop}>
          <header>
            <b>{column.label}</b>
            <span>{cards.length}{points(column.id)?` · ${points(column.id)} puan`:''}</span>
            {canCreate&&<button type="button" title="Bu sütuna görev ekle" aria-label={`${column.label} sütununa görev ekle`} onClick={()=>setCreating(column.id)}>＋</button>}
          </header>
          <div className="board-cards">
            {cards.map(task=><div key={task.id} className="board-slot-wrap">
              {slotHere(column.id,task.id)&&<div className="board-drop-slot"/>}
              <Card task={task} owner={person(task.assigneeId)} drag={cardProps(task,column.id)}/>
            </div>)}
            {slotHere(column.id,null)&&<div className="board-drop-slot"/>}
            {!cards.length&&!slotHere(column.id,null)&&<p className="board-empty">{canCreate?'Kart yok — buraya sürükle':'Kart yok'}</p>}
          </div>
        </section>
      })}
    </div>}

    {view==='list'&&<div className="board-list">
      {COLUMNS.map(column=>{
        const cards=byColumn(column.id)
        if(!cards.length)return null
        return <section key={column.id} className={`board-list-group ${column.id.toLowerCase()}`}>
          <header>
            <b>{column.label}</b><span>{cards.length}{points(column.id)?` · ${points(column.id)} puan`:''}</span>
            {canCreate&&<button type="button" onClick={()=>setCreating(column.id)}>＋ Görev</button>}
          </header>
          <div className="board-list-rows">
            {cards.map(task=>{
              const late=task.dueAt&&new Date(task.dueAt)<new Date()&&task.status!=='DONE'
              return <button type="button" key={task.id} className={`board-row priority-${task.priority.toLowerCase()}${flash===task.id?' flash':''}`} onClick={()=>setEditing(task)}>
                <code>#{task.number}</code>
                <span className="board-row-title">{task.title}</span>
                <span className="board-row-sprint">{sprints.find(item=>item.id===task.sprintId)?.name||'Backlog'}</span>
                <span className={`board-row-priority ${task.priority.toLowerCase()}`}>{priorityLabel(task.priority)}</span>
                <span className="board-row-points">{task.points!==null?`${task.points}p`:'—'}</span>
                <span className={`board-row-due${late?' late':''}`}>{task.dueAt?dayText(task.dueAt):'—'}</span>
                <Avatar member={person(task.assigneeId)} size={22} label="Atanan"/>
              </button>
            })}
          </div>
        </section>
      })}
      {!visible.length&&<p className="board-empty">Bu görünümde görev yok.</p>}
    </div>}

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

    {addingBoard&&<NameDialog title="Yeni pano" hint="Her takım kendi panosunu kullanabilir: “Oyun”, “Sanat”, “Backend” gibi." action="Pano oluştur" busy={busy}
      onClose={()=>setAddingBoard(false)} onSave={async name=>{await onCreateBoard?.(name);setAddingBoard(false)}}/>}
    {renaming&&<NameDialog title="Panoyu yeniden adlandır" initial={renaming.name} action="Kaydet" busy={busy}
      onClose={()=>setRenaming(null)} onSave={async name=>{await onRenameBoard?.(renaming.id,name);setRenaming(null)}}/>}
    {removingBoard&&<ConfirmDialog title="Pano silinsin mi?"
      body={`“${removingBoard.name}” panosu, içindeki sprint ve görevlerle birlikte silinecek. Bu işlem geri alınamaz.`}
      confirmLabel="Panoyu sil" danger
      onCancel={()=>setRemovingBoard(null)}
      onConfirm={()=>{const hedef=removingBoard;setRemovingBoard(null);void onDeleteBoard?.(hedef.id)}}/>}
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

// Sprint takvimi: iki tıkla tarih aralığı seçilir (ilk tık başlangıç, ikinci
// tık bitiş). Tarayıcının tarih kutusundan farkı, sprint'in kaç gün sürdüğünü
// ve hangi günleri kapsadığını seçerken göstermesi.
const GUNLER=['Pt','Sa','Ça','Pe','Cu','Ct','Pz']
const gunAnahtar=(date:Date)=>`${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`
const gunOku=(value:string)=>{const[year,month,day]=value.split('-').map(Number);return new Date(year,(month||1)-1,day||1)}
const gunFarki=(from:string,to:string)=>Math.round((gunOku(to).getTime()-gunOku(from).getTime())/86400000)+1
const gunYazi=(value:string)=>value?gunOku(value).toLocaleDateString('tr-TR',{day:'numeric',month:'long'}):'—'

function RangeCalendar({from,to,onPick}:{from:string;to:string;onPick:(from:string,to:string)=>void}){
  const[ay,setAy]=useState(()=>{const base=from?gunOku(from):new Date();return new Date(base.getFullYear(),base.getMonth(),1)})
  const bugun=gunAnahtar(new Date())
  // Ay ızgarası pazartesiden başlar; önceki ayın son günleriyle doldurulur.
  const ilk=new Date(ay.getFullYear(),ay.getMonth(),1)
  const kaydir=(ilk.getDay()+6)%7
  const gunSayisi=new Date(ay.getFullYear(),ay.getMonth()+1,0).getDate()
  const hucreler=Array.from({length:Math.ceil((kaydir+gunSayisi)/7)*7},(_,index)=>{
    const gun=index-kaydir+1
    return gun>=1&&gun<=gunSayisi?gunAnahtar(new Date(ay.getFullYear(),ay.getMonth(),gun)):''
  })
  function sec(key:string){
    // İlk tık aralığı başlatır; ikinci tık bitişi koyar (geriye tıklanırsa yer değişir).
    if(!from||to||key<from)onPick(key,'')
    else onPick(from,key)
  }
  return <div className="sprint-calendar">
    <header>
      <button type="button" aria-label="Önceki ay" onClick={()=>setAy(new Date(ay.getFullYear(),ay.getMonth()-1,1))}>‹</button>
      <b>{ay.toLocaleDateString('tr-TR',{month:'long',year:'numeric'})}</b>
      <button type="button" aria-label="Sonraki ay" onClick={()=>setAy(new Date(ay.getFullYear(),ay.getMonth()+1,1))}>›</button>
    </header>
    <div className="sprint-calendar-week">{GUNLER.map(gun=><span key={gun}>{gun}</span>)}</div>
    <div className="sprint-calendar-grid">
      {hucreler.map((key,index)=>{
        if(!key)return <span key={`bos-${index}`} className="sprint-day empty"/>
        const secili=key===from||key===to
        const arada=Boolean(from&&to&&key>from&&key<to)
        return <button type="button" key={key}
          className={`sprint-day${secili?' picked':''}${arada?' between':''}${key===bugun?' today':''}${key===from&&to?' start':''}${key===to?' end':''}`}
          onClick={()=>sec(key)}>{Number(key.slice(8))}</button>
      })}
    </div>
    <footer>
      {[{label:'1 hafta',gun:7},{label:'2 hafta',gun:14},{label:'1 ay',gun:30}].map(secenek=>
        <button type="button" key={secenek.gun} onClick={()=>{
          const basla=from?gunOku(from):new Date()
          const bit=new Date(basla.getFullYear(),basla.getMonth(),basla.getDate()+secenek.gun-1)
          onPick(gunAnahtar(basla),gunAnahtar(bit))
        }}>{secenek.label}</button>)}
      <span className="board-spacer"/>
      <b>{from&&to?`${gunYazi(from)} → ${gunYazi(to)} · ${gunFarki(from,to)} gün`:from?`${gunYazi(from)} → bitişi seç`:'Başlangıç gününe tıkla'}</b>
    </footer>
  </div>
}

// Tek alanlı pencere: pano adı sorarken kullanılır.
function NameDialog({title,hint,action,initial='',busy,onClose,onSave}:{
  title:string;hint?:string;action:string;initial?:string;busy?:boolean
  onClose:()=>void;onSave:(name:string)=>Promise<unknown>|void
}){
  const[name,setName]=useState(initial)
  useEscape(true,onClose)
  const kaydet=()=>{if(name.trim().length>=2)void onSave(name.trim())}
  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal board-dialog name-dialog">
      <header><div><b>{title}</b>{hint&&<small>{hint}</small>}</div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <label className="board-field"><span>Ad</span>
        <input autoFocus value={name} maxLength={60} onChange={event=>setName(event.target.value)} placeholder="Örn. Oyun takımı" onKeyDown={event=>{if(event.key==='Enter')kaydet()}}/>
      </label>
      <footer>
        <span className="board-spacer"/>
        <button type="button" className="board-button" onClick={onClose}>Vazgeç</button>
        <button type="button" className="board-button primary" disabled={busy||name.trim().length<2} onClick={kaydet}>{action}</button>
      </footer>
    </section>
  </div>
}

function SprintDialog({sprints,busy,onClose,onSave,onDelete}:{
  sprints:Sprint[];busy?:boolean;onClose:()=>void
  onSave:(sprint:Partial<Sprint>&{id?:string})=>Promise<unknown>;onDelete:(id:string)=>Promise<unknown>
}){
  const[editing,setEditing]=useState<Sprint|null>(null)
  const[name,setName]=useState('')
  const[goal,setGoal]=useState('')
  const[from,setFrom]=useState('')
  const[to,setTo]=useState('')
  const[removing,setRemoving]=useState<Sprint|null>(null)
  useEscape(true,onClose)
  const temizle=()=>{setEditing(null);setName('');setGoal('');setFrom('');setTo('')}
  function duzenle(sprint:Sprint){
    setEditing(sprint);setName(sprint.name);setGoal(sprint.goal||'')
    setFrom(sprint.startsAt?gunAnahtar(new Date(sprint.startsAt)):'')
    setTo(sprint.endsAt?gunAnahtar(new Date(sprint.endsAt)):'')
  }
  async function kaydet(){
    if(name.trim().length<2)return
    await onSave({
      ...(editing?{id:editing.id}:{}),
      name:name.trim(),goal:goal.trim(),
      // Gün başı ve gün sonu: sprint seçilen günlerin tamamını kapsar.
      ...(from?{startsAt:gunOku(from).toISOString()}:{}),
      ...(to?{endsAt:new Date(gunOku(to).setHours(23,59,59,0)).toISOString()}:{}),
    } as Partial<Sprint>&{id?:string})
    temizle()
  }
  const araligi=(sprint:Sprint)=>sprint.startsAt&&sprint.endsAt
    ?`${dayText(sprint.startsAt)} – ${dayText(sprint.endsAt)}`
    :sprint.endsAt?`bitiş ${dayText(sprint.endsAt)}`:'tarih yok'

  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal board-dialog sprint-dialog">
      <header><div><b>Sprint’ler</b><small>Aynı anda tek sprint aktif olur; yeni sprint başlayınca diğeri planlanana döner.</small></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="sprint-list">
        {sprints.map(sprint=>{
          const ilerleme=sprintProgress(sprint)
          return <article key={sprint.id} className={`sprint-row ${sprint.status.toLowerCase()}${editing?.id===sprint.id?' editing':''}`}>
            <div>
              <b>{sprint.name}</b>
              <small>{sprint.goal||'Hedef yazılmadı'}</small>
              <span className="sprint-range">{araligi(sprint)}{ilerleme&&sprint.status==='ACTIVE'?` · ${ilerleme.left>0?`${ilerleme.left} gün kaldı`:'süre doldu'}`:''}</span>
            </div>
            <span className={`sprint-state ${sprint.status.toLowerCase()}`}>{sprint.status==='ACTIVE'?'Aktif':sprint.status==='DONE'?'Bitti':'Planlandı'}</span>
            <div className="sprint-actions">
              <button type="button" className="board-button" disabled={busy} onClick={()=>duzenle(sprint)}>Düzenle</button>
              {sprint.status!=='ACTIVE'&&<button type="button" className="board-button" disabled={busy} onClick={()=>void onSave({id:sprint.id,status:'ACTIVE'})}>Başlat</button>}
              {sprint.status==='ACTIVE'&&<button type="button" className="board-button" disabled={busy} onClick={()=>void onSave({id:sprint.id,status:'DONE'})}>Bitir</button>}
              <button type="button" className="board-button danger" disabled={busy} onClick={()=>setRemoving(sprint)}>Sil</button>
            </div>
          </article>
        })}
        {!sprints.length&&<p className="board-empty">Henüz sprint yok. Aşağıdan ilkini aç.</p>}
      </div>

      <div className="sprint-form">
        <div className="board-grid">
          <label className="board-field wide"><span>{editing?'Sprint adı':'Yeni sprint adı'}</span><input value={name} onChange={event=>setName(event.target.value)} placeholder="Sprint 1"/></label>
          <label className="board-field wide"><span>Hedef</span><input value={goal} onChange={event=>setGoal(event.target.value)} placeholder="Bu sprint’te ne bitecek?"/></label>
        </div>
        <RangeCalendar from={from} to={to} onPick={(basla,bit)=>{setFrom(basla);setTo(bit)}}/>
      </div>

      <footer>
        {editing&&<button type="button" className="board-button" onClick={temizle}>Yeni sprint’e dön</button>}
        <span className="board-spacer"/>
        <button type="button" className="board-button" onClick={onClose}>Kapat</button>
        <button type="button" className="board-button primary" disabled={busy||name.trim().length<2} onClick={()=>void kaydet()}>{editing?'Kaydet':'Sprint oluştur'}</button>
      </footer>

      {removing&&<ConfirmDialog title="Sprint silinsin mi?"
        body={`“${removing.name}” silinecek. Görevleri silinmez, backlog’a düşer.`}
        confirmLabel="Sprint’i sil" danger
        onCancel={()=>setRemoving(null)}
        onConfirm={()=>{const hedef=removing;setRemoving(null);if(editing?.id===hedef.id)temizle();void onDelete(hedef.id)}}/>}
    </section>
  </div>
}
