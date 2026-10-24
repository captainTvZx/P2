import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent, type MouseEvent as ReactMouseEvent } from 'react'
import { createPortal } from 'react-dom'
import { API_BASE, p2Fetch } from './apiBase'
import { AttachmentPicker, AttachmentQueue, DropZone, attachmentsFromContent, useAttachments, useFileDrop, withoutAttachments } from './attachments'
import ConfirmDialog from './ConfirmDialog'
import { ComposerIcon } from './ComposerIcons'
import MessageRichText from './MessageRichText'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import { avatarHue, userInitials } from './initials'
import { useRealtimeEvents } from './realtime'
import { useSettings, updateSettings } from './settings'
import EmojiPanel from './EmojiPanel'
import { anchorFrom, MessageMenuPopup, MessageTools, ReactionBar, useMessageTools, type MessageMenu, type Reaction } from './messageMenu'
import { useDismissOutside } from './useDismiss'
import { useEscape } from './useEscape'
import { ComposerSuggestions, type MentionSuggestion } from './ChannelContent'
import { KIND_LABEL, KIND_SHORT, TASK_KINDS, parseLabels, taskKind, taskLabels, type TaskKind } from './api/tasks'

// Görev panosu: sprint'ler ve kanban sütunları. Her proje kendi panosunu
// taşır (görev numaraları da proje içinde sıralıdır); canlı olaylarla
// (task:*, sprint:*) ekipte herkeste aynı anda güncellenir.
export type Sprint={id:string;name:string;goal:string|null;status:'PLANNED'|'ACTIVE'|'DONE';startsAt:string|null;endsAt:string|null;projectId?:string;boardId?:string}
// Bir projede birden çok pano olur: her takım kendi kanbanını yönetir.
export type BoardInfo={id:string;name:string;position:number;projectId:string}
export type TaskComment={id:string;taskId:string;authorId:string;content:string;createdAt:string;updatedAt:string;reactions?:Reaction[]}
export type Task={id:string;number:number;title:string;description:string|null;status:TaskStatus;priority:Priority;kind?:string;labels?:string;points:number|null;position:number;assigneeId:string|null;createdById:string;sprintId:string|null;projectId:string;boardId?:string;dueAt:string|null}
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
// Kısa bildirim: sprint açma gibi işlerin sonucunu kullanıcı görsün.
const bildir=(text:string,tone:'ok'|'error'='ok')=>window.dispatchEvent(new CustomEvent('p2:toast',{detail:{text,tone}}))
const priorityLabel=(value:Priority)=>PRIORITIES.find(item=>item.id===value)?.label||value
// Görev türü rozeti (hata/özellik…) ve etiket çipleri: kartta ve ayrıntı panelinde.
function KindBadge({task}:{task:Pick<Task,'kind'>}){const kind=taskKind(task);return kind==='TASK'?null:<span className={`board-kind ${kind}`} title={`Tür: ${KIND_LABEL[kind]}`}>{KIND_SHORT[kind]}</span>}
function LabelChips({task,max=3}:{task:Pick<Task,'labels'>;max?:number}){const labels=taskLabels(task);if(!labels.length)return null;return <span className="board-labels">{labels.slice(0,max).map(label=><em key={label}>{label}</em>)}{labels.length>max&&<em title={labels.slice(max).join(', ')}>+{labels.length-max}</em>}</span>}
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
  const[boardId,setBoardId]=useState<string>(()=>{try{return localStorage.getItem(memory)||''}catch{return ''}}),[pendingTaskId,setPendingTaskId]=useState('')
  const[tasks,setTasks]=useState<Task[]>([]),[sprints,setSprints]=useState<Sprint[]>([]),[loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState('')
  const base=boardId?`${root}/${boardId}`:''
  const pick=useCallback((id:string)=>{setBoardId(id);try{localStorage.setItem(memory,id)}catch{}},[memory])
  useEffect(()=>{const open=(event:Event)=>{const detail=(event as CustomEvent<{boardId:string|null;taskId:string;replay?:boolean}>).detail;if(detail?.replay)return;if(detail?.boardId)pick(detail.boardId);if(detail?.taskId)setPendingTaskId(detail.taskId)};window.addEventListener('p2:open-task',open);return()=>window.removeEventListener('p2:open-task',open)},[pick])
  useEffect(()=>{if(loading||!pendingTaskId)return;const taskId=pendingTaskId;setPendingTaskId('');window.setTimeout(()=>window.dispatchEvent(new CustomEvent('p2:open-task',{detail:{taskId,replay:true}})),0)},[loading,pendingTaskId,boardId])
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
    'task:comment':(payload:{comment:TaskComment;projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
      setComments(current=>{
        const list=current[payload.comment.taskId]
        if(!list)return current
        const found=list.some(item=>item.id===payload.comment.id)
        return {...current,[payload.comment.taskId]:found?list.map(item=>item.id===payload.comment.id?payload.comment:item):[...list,payload.comment]}
      })
    },
    'task:comment:reactions':(payload:{commentId:string;taskId:string;rows:{emoji:string;userId:string}[];projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
      // Sunucu ham satırları yollar; "benim tepkim mi" bilgisi burada hesaplanır.
      const ozet=new Map<string,Reaction>()
      for(const row of payload.rows||[]){
        const item=ozet.get(row.emoji)||{emoji:row.emoji,count:0,reacted:false}
        item.count+=1
        if(row.userId===auth.user.id)item.reacted=true
        ozet.set(row.emoji,item)
      }
      const reactions=[...ozet.values()]
      setComments(current=>current[payload.taskId]?{...current,[payload.taskId]:current[payload.taskId].map(item=>item.id===payload.commentId?{...item,reactions}:item)}:current)
    },
    'task:comment:deleted':(payload:{commentId:string;taskId:string;projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
      setComments(current=>current[payload.taskId]?{...current,[payload.taskId]:current[payload.taskId].filter(item=>item.id!==payload.commentId)}:current)
    },
    'sprint:deleted':(payload:{sprintId:string;projectId?:string;boardId?:string})=>{
      if(!mine(payload.projectId,payload.boardId))return
      setSprints(items=>items.filter(item=>item.id!==payload.sprintId))
      setTasks(items=>items.map(task=>task.sprintId===payload.sprintId?{...task,sprintId:null}:task))
    },
  })
  // Açık görevin yorumları panel açılınca çekilir, canlı olaylarla tazelenir.
  const[comments,setComments]=useState<Record<string,TaskComment[]>>({})
  const loadComments=useCallback((taskId:string)=>{
    if(!base)return
    request<TaskComment[]>(`${base}/tasks/${taskId}/comments`,token)
      .then(list=>setComments(current=>({...current,[taskId]:list||[]})))
      .catch(()=>{})
  },[base,token])
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
    token={token}
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
    canCreate={perms.can('task.create')} canManage={perms.can('board.manage')} canDeleteAny={perms.can('message.delete_unlimited')}
    busy={busy} error={error}
    onCreate={draft=>quiet(send<Task>(`${base}/tasks`,{method:'POST',body:JSON.stringify(draft)}).then(task=>{mergeTask(task);bildir(`#${task.number} “${task.title}” oluşturuldu.`)}))}
    onUpdate={(id,patch)=>quiet(send<Task>(`${base}/tasks/${id}`,{method:'PATCH',body:JSON.stringify(patch)}).then(mergeTask))}
    onDelete={id=>quiet(send(`${base}/tasks/${id}`,{method:'DELETE'}).then(()=>{setTasks(items=>items.filter(item=>item.id!==id));bildir('Görev silindi.')}))}
    onMove={(id,status,order)=>{
      // Sürükle bırak anında görünsün diye önce yerel sıra yazılır, sonra sunucu.
      setTasks(items=>items.map(item=>item.id===id?{...item,status}:item).map(item=>{const index=order.indexOf(item.id);return index<0?item:{...item,position:index}}))
      return quiet(send(`${base}/tasks/${id}/move`,{method:'PATCH',body:JSON.stringify({status,order})}).catch(failure=>{load();throw failure}))
    }}
    onSprint={sprint=>{
      const{id,...body}=sprint
      return quiet(send<Sprint>(id?`${base}/sprints/${id}`:`${base}/sprints`,{method:id?'PATCH':'POST',body:JSON.stringify(body)}).then(async saved=>{await load();return saved}))
    }}
    comments={comments}
    onLoadComments={loadComments}
    onComment={(taskId,content)=>quiet(send<TaskComment>(`${base}/tasks/${taskId}/comments`,{method:'POST',body:JSON.stringify({content})}).then(comment=>
      setComments(current=>({...current,[taskId]:[...(current[taskId]||[]).filter(item=>item.id!==comment.id),comment]}))))}
    onCommentDelete={(taskId,commentId)=>quiet(send(`${base}/tasks/${taskId}/comments/${commentId}`,{method:'DELETE'}).then(()=>
      setComments(current=>({...current,[taskId]:(current[taskId]||[]).filter(item=>item.id!==commentId)}))))}
    onCommentEdit={(taskId,commentId,content)=>quiet(send<TaskComment>(`${base}/tasks/${taskId}/comments/${commentId}`,{method:'PATCH',body:JSON.stringify({content})}).then(comment=>
      setComments(current=>({...current,[taskId]:(current[taskId]||[]).map(item=>item.id===comment.id?{...comment,reactions:item.reactions}:item)}))))}
    onCommentReact={(taskId,commentId,emoji)=>quiet(send<Reaction[]>(`${base}/tasks/${taskId}/comments/${commentId}/reactions`,{method:'POST',body:JSON.stringify({emoji})}).then(reactions=>
      setComments(current=>({...current,[taskId]:(current[taskId]||[]).map(item=>item.id===commentId?{...item,reactions}:item)}))))}
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
      <KindBadge task={task}/>
      {task.points!==null&&<em>{task.points}p</em>}
      <i className="board-priority" title={`Öncelik: ${priorityLabel(task.priority)}`}/>
    </div>
    <p>{task.title}</p>
    <LabelChips task={task}/>
    <footer>
      <Avatar member={owner} label="Atanan"/>
      {task.description&&<span className="board-card-mark" title="Açıklaması var">≡</span>}
      {task.dueAt&&<small className={late?'late':''} title={new Date(task.dueAt).toLocaleDateString('tr-TR')}>{dayText(task.dueAt)}</small>}
    </footer>
  </article>
}

// UI showroom da bu görsel bileşeni sahte veriyle kullanır.
export function Board({token='',projectName,boards,boardId,onSelectBoard,onCreateBoard,onRenameBoard,onDeleteBoard,comments,onLoadComments,onComment,onCommentDelete,onCommentEdit,onCommentReact,tasks,sprints,members,me,canCreate,canManage,canDeleteAny=false,onCreate,onUpdate,onDelete,onMove,onSprint,onSprintDelete,busy,error}:{
  token?:string
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
  canDeleteAny?:boolean
  onCreate:(draft:Partial<Task>)=>Promise<unknown>
  onUpdate:(id:string,patch:Partial<Task>)=>Promise<unknown>
  onDelete:(id:string)=>Promise<unknown>
  onMove:(id:string,status:TaskStatus,order:string[])=>Promise<unknown>
  onSprint:(sprint:Partial<Sprint>&{id?:string})=>Promise<unknown>
  onSprintDelete:(id:string)=>Promise<unknown>
  comments?:Record<string,TaskComment[]>
  onLoadComments?:(taskId:string)=>void
  onComment?:(taskId:string,content:string)=>Promise<unknown>
  onCommentDelete?:(taskId:string,commentId:string)=>Promise<unknown>
  onCommentEdit?:(taskId:string,commentId:string,content:string)=>Promise<unknown>
  onCommentReact?:(taskId:string,commentId:string,emoji:string)=>Promise<unknown>
  busy?:boolean
  error?:string
}){
  const active=sprints.find(item=>item.status==='ACTIVE')||null
  // Varsayılan görünüm: aktif sprint, yoksa backlog. Kullanıcı bir kez seçim
  // yaptıysa liste tazelense de o seçim korunur.
  const[scope,setScope]=useState<string>(active?active.id:'backlog')
  const scopeChosen=useRef(false)
  const[assignee,setAssignee]=useState<'all'|'mine'|string>('all')
  const[search,setSearch]=useState('')
  const[editing,setEditing]=useState<Task|null>(null)
  // Karta tıklandığında sağdan ayrıntı paneli açılır (profil paneliyle aynı
  // davranış); düzenleme penceresi oradaki "Düzenle" ile gelir.
  const[viewing,setViewing]=useState<string>('')
  useEffect(()=>{const open=(event:Event)=>{const id=(event as CustomEvent<{taskId:string}>).detail?.taskId;if(id)setViewing(id)};window.addEventListener('p2:open-task',open);return()=>window.removeEventListener('p2:open-task',open)},[])
  const[creating,setCreating]=useState<TaskStatus|null>(null)
  const[sprintPanel,setSprintPanel]=useState(false)
  const[boardMenu,setBoardMenu]=useState<{board:BoardInfo;menu:MessageMenu}|null>(null)
  const[taskMenu,setTaskMenu]=useState<{task:Task;menu:MessageMenu}|null>(null)
  const[renaming,setRenaming]=useState<BoardInfo|null>(null)
  const[addingBoard,setAddingBoard]=useState(false)
  const[removingBoard,setRemovingBoard]=useState<BoardInfo|null>(null)
  const[removingTask,setRemovingTask]=useState<Task|null>(null)
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
  useEffect(()=>{
    if(scopeChosen.current)return
    if(active&&scope!==active.id)setScope(active.id)
    else if(!active&&scope!=='backlog'&&!sprints.some(item=>item.id===scope))setScope('backlog')
  },[active?.id,sprints.length])
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
  function openTaskMenu(event:ReactMouseEvent,task:Task){
    event.preventDefault()
    event.stopPropagation()
    setTaskMenu({task,menu:{x:event.clientX,y:event.clientY,up:event.clientY}})
  }
  function moveTaskStatus(task:Task,status:TaskStatus){
    const target=byColumn(status).filter(item=>item.id!==task.id)
    void onMove(task.id,status,[...target,task].map(item=>item.id))
    setTaskMenu(null)
    bildir(`#${task.number} “${COLUMNS.find(item=>item.id===status)?.label||status}” sütununa taşındı.`)
  }
  function moveTaskSprint(task:Task,sprintId:string|null){
    void onUpdate(task.id,{sprintId})
    setTaskMenu(null)
    const name=sprintId?sprints.find(item=>item.id===sprintId)?.name:'Backlog'
    bildir(`#${task.number} “${name||'sprint'}” alanına taşındı.`)
  }
  const cardProps=(task:Task,status:TaskStatus)=>({
    className:`board-card priority-${task.priority.toLowerCase()}${dragging===task.id?' dragging':''}${flash===task.id?' flash':''}`,
    draggable:true,
    onDragStart:(event:DragEvent)=>{dragged.current=task.id;setDragging(task.id);event.dataTransfer.setData('text/plain',task.id);event.dataTransfer.effectAllowed='move'},
    onDragEnd:endDrag,
    onDragOver:(event:DragEvent)=>aim(event,status,task),
    onDrop:drop,
    onClick:()=>setViewing(task.id),
    onContextMenu:(event:ReactMouseEvent)=>openTaskMenu(event,task),
  })

  // Boşluk yalnızca sürükleme sürerken çizilir: dragend kaçsa bile iz kalmaz.
  const open=tasks.find(item=>item.id===viewing)||null
  const slotHere=(status:TaskStatus,before:string|null)=>Boolean(dragging)&&slot?.status===status&&slot.before===before
  const progress=sprintProgress(scope==='backlog'?null:sprints.find(item=>item.id===scope))
  const contextTask=taskMenu?.task||null
  const contextCanEdit=Boolean(contextTask&&(canManage||contextTask.createdById===me||contextTask.assigneeId===me))

  return <section className="board-page">
    {list.length>0&&<nav className="board-tabs" aria-label="Panolar">
      <span className="board-tabs-project">{projectName}</span>
      <div className="board-tabs-list">
        {list.map(item=><button type="button" key={item.id} className={item.id===boardId?'board-tab active':'board-tab'}
          onClick={()=>onSelectBoard?.(item.id)}
          onContextMenu={event=>{if(!canManage)return;event.preventDefault();setBoardMenu({board:item,menu:{x:event.clientX,y:event.clientY,up:event.clientY}})}}
          onDoubleClick={()=>{if(canManage)setRenaming(item)}}
          title={item.id===boardId&&canManage?'Çift tıkla: yeniden adlandır':item.name}>{item.name}</button>)}
        {canManage&&<button type="button" className="board-tab add" onClick={()=>setAddingBoard(true)} title="Yeni pano">＋</button>}
      </div>
      {canManage&&current&&<div className="board-tab-tools">
        <button type="button" className="board-button subtle board-menu-toggle" onClick={event=>setBoardMenu({board:current,menu:anchorFrom(event.currentTarget.getBoundingClientRect(),true)})} aria-label="Pano ayarları">⋯</button>
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
        <select value={scope} onChange={event=>{scopeChosen.current=true;setScope(event.target.value)}} aria-label="Sprint seç">
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
              return <button type="button" key={task.id} className={`board-row priority-${task.priority.toLowerCase()}${flash===task.id?' flash':''}${viewing===task.id?' picked':''}`} onClick={()=>setViewing(task.id)} onContextMenu={event=>openTaskMenu(event,task)}>
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

    {boardMenu&&<MessageMenuPopup menu={boardMenu.menu} onClose={()=>setBoardMenu(null)} className="message-context-menu board-context-menu">
      <button type="button" onClick={()=>{const hedef=boardMenu.board;setBoardMenu(null);onSelectBoard?.(hedef.id)}}><span>↗</span>Panoyu aç</button>
      <button type="button" onClick={()=>{const hedef=boardMenu.board;setBoardMenu(null);setRenaming(hedef)}}><span>✎</span>Panoyu yeniden adlandır</button>
      <hr/>
      <button type="button" className="danger" disabled={list.length<2} onClick={()=>{const hedef=boardMenu.board;setBoardMenu(null);setRemovingBoard(hedef)}}>
        <span>×</span>{list.length<2?'Son pano silinemez':'Panoyu sil'}
      </button>
    </MessageMenuPopup>}

    {taskMenu&&contextTask&&<MessageMenuPopup menu={taskMenu.menu} onClose={()=>setTaskMenu(null)} className="message-context-menu board-task-menu">
      <button type="button" onClick={()=>{setViewing(contextTask.id);setTaskMenu(null)}}><span>↗</span>Görevi aç</button>
      {contextCanEdit&&<button type="button" onClick={()=>{setEditing(contextTask);setTaskMenu(null)}}><span>✎</span>Düzenle</button>}
      {contextCanEdit&&<><hr/><p className="board-menu-label">Duruma taşı</p>
        {COLUMNS.map(column=><button type="button" key={column.id} disabled={columnOf(contextTask)===column.id} onClick={()=>moveTaskStatus(contextTask,column.id)}><span>→</span>{column.label}</button>)}
        <hr/><p className="board-menu-label">Sprint’e taşı</p>
        <button type="button" disabled={!contextTask.sprintId} onClick={()=>moveTaskSprint(contextTask,null)}><span>↩</span>Backlog</button>
        {sprints.filter(item=>item.status!=='DONE'&&item.id!==contextTask.sprintId).map(sprint=><button type="button" key={sprint.id} onClick={()=>moveTaskSprint(contextTask,sprint.id)}><span>→</span>{sprint.name}<small>{sprint.status==='ACTIVE'?'aktif':''}</small></button>)}
        <hr/><button type="button" className="danger" onClick={()=>{setTaskMenu(null);setRemovingTask(contextTask)}}><span>×</span>Görevi sil</button>
      </>}
    </MessageMenuPopup>}

    {open&&<TaskPanel
      task={open}
      token={token}
      me={me}
      comments={comments?.[open.id]}
      onLoadComments={()=>onLoadComments?.(open.id)}
      onComment={content=>onComment?.(open.id,content)}
      onCommentDelete={commentId=>onCommentDelete?.(open.id,commentId)}
      onCommentEdit={(commentId,content)=>onCommentEdit?.(open.id,commentId,content)}
      onCommentReact={(commentId,emoji)=>onCommentReact?.(open.id,commentId,emoji)}
      members={members}
      sprints={sprints}
      canEdit={canManage||open.createdById===me||open.assigneeId===me}
      canManage={canManage} canDeleteAny={canDeleteAny}
      busy={busy}
      onClose={()=>setViewing('')}
      onEdit={()=>{setEditing(open);setViewing('')}}
      onAssign={id=>void onUpdate(open.id,{assigneeId:id} as Partial<Task>)}
      onStatus={status=>void onUpdate(open.id,{status} as Partial<Task>)}
      onSprintChange={sprintId=>{
        void onUpdate(open.id,{sprintId} as Partial<Task>)
        bildir(sprintId?`#${open.number} “${sprints.find(item=>item.id===sprintId)?.name||'sprint'}” sprint’ine alındı.`:`#${open.number} backlog’a döndü.`)
      }}
      onDelete={async()=>{await onDelete(open.id);setViewing('')}}/>}

    {(creating||editing)&&<TaskDialog
      token={token}
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

    {sprintPanel&&<SprintDialog sprints={sprints} tasks={tasks} members={members} busy={busy}
      onClose={()=>setSprintPanel(false)} onSave={onSprint} onDelete={onSprintDelete}
      onMoveToSprint={(sprintId,ids)=>Promise.all(ids.map(id=>onUpdate(id,{sprintId} as Partial<Task>)))}
      onFinish={async(sprint,target,newSprintName)=>{
        let targetId=target
        if(target==='new'){
          const created=await onSprint({name:newSprintName||`${sprint.name} devamı`}) as Sprint|undefined
          if(!created?.id)return
          targetId=created.id
        }
        const unfinished=tasks.filter(task=>task.sprintId===sprint.id&&task.status!=='DONE')
        await Promise.all(unfinished.map(task=>onUpdate(task.id,{sprintId:targetId==='backlog'?null:targetId})))
        await onSprint({id:sprint.id,status:'DONE',endsAt:new Date().toISOString()})
      }}/>}

    {addingBoard&&<NameDialog title="Yeni pano" hint="Her takım kendi panosunu kullanabilir: “Oyun”, “Sanat”, “Backend” gibi." action="Pano oluştur" busy={busy}
      onClose={()=>setAddingBoard(false)} onSave={async name=>{await onCreateBoard?.(name);setAddingBoard(false)}}/>}
    {renaming&&<NameDialog title="Panoyu yeniden adlandır" initial={renaming.name} action="Kaydet" busy={busy}
      onClose={()=>setRenaming(null)} onSave={async name=>{await onRenameBoard?.(renaming.id,name);setRenaming(null)}}/>}
    {removingBoard&&<ConfirmDialog title="Pano silinsin mi?"
      body={`“${removingBoard.name}” panosu, içindeki sprint ve görevlerle birlikte silinecek. Bu işlem geri alınamaz.`}
      confirmLabel="Panoyu sil" danger
      onCancel={()=>setRemovingBoard(null)}
      onConfirm={()=>{const hedef=removingBoard;setRemovingBoard(null);void onDeleteBoard?.(hedef.id)}}/>}
    {removingTask&&<ConfirmDialog title="Görev silinsin mi?"
      body={`#${removingTask.number} “${removingTask.title}” panodan kaldırılacak.`}
      confirmLabel="Görevi sil" danger
      onCancel={()=>setRemovingTask(null)}
      onConfirm={()=>{const hedef=removingTask;setRemovingTask(null);void onDelete(hedef.id)}}/>}
  </section>
}

function TaskDialog({task,status,sprintId,members,sprints,canDelete,busy,token,onClose,onSave,onDelete}:{
  task:Task|null;status:TaskStatus;sprintId:string|null;members:Member[];sprints:Sprint[]
  canDelete:boolean;busy?:boolean;token:string;onClose:()=>void;onSave:(draft:Partial<Task>)=>Promise<unknown>;onDelete:()=>Promise<unknown>
}){
  const[title,setTitle]=useState(task?.title||'')
  // Açıklama sohbetle aynı dili konuşur: kalın/italik/bağlantı ve dosya ekleri.
  // Metin ile ekler ayrılır; yoksa düzenlerken ham "[[p2-file…]]" satırları görünür.
  const[description,setDescription]=useState(withoutAttachments(task?.description||''))
  const editor=useRef<RichHandle>(null)
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const[mentionIndex,setMentionIndex]=useState(0)
  const attachments=useAttachments(token,attachmentsFromContent(task?.description||''))
  const drop=useFileDrop(files=>void attachments.add(files))
  const[assigneeId,setAssigneeId]=useState(task?.assigneeId||'')
  const[priority,setPriority]=useState<Priority>(task?.priority||'MEDIUM')
  const[kind,setKind]=useState<TaskKind>(task?taskKind(task):'TASK')
  const[labelsText,setLabelsText]=useState(task?taskLabels(task).join(', '):'')
  const[points,setPoints]=useState(task?.points?.toString()||'')
  const[dueAt,setDueAt]=useState(task?.dueAt?task.dueAt.slice(0,10):'')
  const[sprint,setSprint]=useState(task?task.sprintId||'':sprintId||'')
  const[column,setColumn]=useState<TaskStatus>(task?.status||status)
  const mentionMatch=/(?:^|\s)@([\w.-]*)$/.exec(description),mentionQuery=(mentionMatch?.[1]||'').toLocaleLowerCase('tr-TR')
  const mentionItems:MentionSuggestion[]=mentionMatch?members.filter(member=>member.username&&(`${member.displayName} ${member.username}`).toLocaleLowerCase('tr-TR').includes(mentionQuery)).slice(0,8).map(member=>({label:member.displayName,description:`@${member.username}`,value:`@${member.username}`,avatar:userInitials(member.displayName)})):[]
  const pickMention=(item:MentionSuggestion)=>{setDescription(value=>value.replace(/@([\w.-]*)$/,`${item.value} `));setMentionIndex(0);requestAnimationFrame(()=>editor.current?.focus())}
  useEscape(true,onClose)
  const save=()=>{
    if(title.trim().length<2||attachments.busy)return
    void onSave({
      title:title.trim(),description:attachments.appendTo(description),assigneeId,priority,status:column,
      kind,labels:parseLabels(labelsText) as unknown as string,
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
      <div className="board-field board-description" {...drop.handlers}>
        <span>Açıklama</span>
        <div className="board-editor">
          <DropZone active={drop.dragging} label="Göreve eklenecek"/>
          <ComposerSuggestions mentions={mentionItems} commands={[]} onMention={pickMention} onCommand={()=>{}} activeIndex={mentionItems.length?Math.min(mentionIndex,mentionItems.length-1):-1} onHover={setMentionIndex}/>
          <RichInput ref={editor} value={description} onChange={value=>{setDescription(value.slice(0,6000));setMentionIndex(0)}}
            onStateChange={setFormat} onFiles={files=>void attachments.add(files)}
            onKeyDown={event=>{if(!mentionItems.length)return;if(event.key==='ArrowDown'){event.preventDefault();setMentionIndex(index=>(index+1)%mentionItems.length)}else if(event.key==='ArrowUp'){event.preventDefault();setMentionIndex(index=>(index-1+mentionItems.length)%mentionItems.length)}else if(event.key==='Enter'||event.key==='Tab'){event.preventDefault();pickMention(mentionItems[Math.min(mentionIndex,mentionItems.length-1)])}}}
            placeholder="Ayrıntılar, kabul kriterleri… Dosya ve görselleri buraya sürükleyebilirsin."/>
          <AttachmentQueue state={attachments}/>
          <div className="board-editor-tools">
            <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('bold')}><ComposerIcon name="bold"/></button>
            <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('italic')}><ComposerIcon name="italic"/></button>
            <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.openLink()}><ComposerIcon name="link"/></button>
            <button type="button" title="Birini etiketle" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.insert('@')}><ComposerIcon name="mention"/></button>
            <i/>
            <AttachmentPicker state={attachments}/>
            {attachments.busy&&<small>Yükleniyor…</small>}
            {attachments.error&&<small className="board-editor-error">{attachments.error}</small>}
          </div>
        </div>
      </div>
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
        <label className="board-field"><span>Tür</span>
          <select value={kind} onChange={event=>setKind(event.target.value as TaskKind)}>
            {TASK_KINDS.map(item=><option key={item} value={item}>{KIND_LABEL[item]}</option>)}
          </select>
        </label>
        <label className="board-field"><span>Etiketler</span>
          <input value={labelsText} onChange={event=>setLabelsText(event.target.value)} placeholder="ui, ses, seviye-3" title="Virgülle ayır; en çok 10 etiket"/>
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
        <button type="button" className="board-button primary" disabled={busy||attachments.busy||title.trim().length<2} onClick={save}>{task?'Kaydet':'Oluştur'}</button>
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


// Görev ayrıntı paneli: sağdan açılır, profil paneliyle aynı dil. Hızlı
// işlemler (atama, sütun) burada; tam düzenleme penceresi "Düzenle" ile.
function TaskPanel({task,token,me,members,sprints,canEdit,canManage,canDeleteAny,busy,comments,onLoadComments,onComment,onCommentDelete,onCommentEdit,onCommentReact,onClose,onEdit,onAssign,onStatus,onSprintChange,onDelete}:{
  task:Task;token:string;me:string;members:Member[];sprints:Sprint[];canEdit:boolean;canManage:boolean;canDeleteAny:boolean;busy?:boolean
  comments?:TaskComment[]
  onLoadComments?:()=>void
  onComment?:(content:string)=>Promise<unknown>|void
  onCommentDelete?:(commentId:string)=>Promise<unknown>|void
  onCommentEdit?:(commentId:string,content:string)=>Promise<unknown>|void
  onCommentReact?:(commentId:string,emoji:string)=>Promise<unknown>|void
  onClose:()=>void;onEdit:()=>void;onAssign:(id:string)=>void;onStatus:(status:TaskStatus)=>void
  onSprintChange:(sprintId:string)=>void;onDelete:()=>Promise<unknown>
}){
  const[removing,setRemoving]=useState(false)
  // Görev ayrıntısı sağda panel ya da ekranın ortasında geniş pencere olarak
  // açılabilir; tercih ayarlarda saklanır.
  const settings=useSettings()
  const genis=settings.taskView==='center'
  // Sağ panelde dışarı tıklama kapatır (ortadaki pencerede katmana tıklamak zaten kapatıyor).
  const kutu=useDismissOutside<HTMLElement>(onClose,!genis)
  useEffect(()=>{onLoadComments?.()},[task.id])
  useEscape(true,onClose)
  const owner=members.find(item=>item.id===task.assigneeId)
  const author=members.find(item=>item.id===task.createdById)
  const sprint=sprints.find(item=>item.id===task.sprintId)
  const late=task.dueAt&&new Date(task.dueAt)<new Date()&&task.status!=='DONE'
  const column=COLUMNS.find(item=>item.id===(task.status==='BACKLOG'?'TODO':task.status))
  const govde=<>
    <header>
      <div><code>#{task.number}</code><b>{column?.label||task.status}</b></div>
      <span className="task-panel-tools">
        <button type="button" title={genis?'Sağ panele al':'Ortada geniş aç'} aria-label={genis?'Sağ panele al':'Ortada geniş aç'}
          onClick={()=>updateSettings({taskView:genis?'panel':'center'})}><ComposerIcon name={genis?'collapse':'expand'}/></button>
        <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat"><ComposerIcon name="close"/></button>
      </span>
    </header>
    <div className="task-panel-body">
      <h3>{task.title}</h3>
      <div className="task-panel-chips">
        <span className={`board-row-priority ${task.priority.toLowerCase()}`}>{priorityLabel(task.priority)}</span>
        {task.points!==null&&<span className="task-chip">{task.points} puan</span>}
        <span className="task-chip">{sprint?sprint.name:'Backlog'}</span>
        {task.dueAt&&<span className={late?'task-chip late':'task-chip'}>{dayText(task.dueAt)}</span>}
      </div>

      <section className="task-panel-block">
        <h4>Atanan</h4>
        <div className="task-panel-person">
          <Avatar member={owner} size={30} label="Atanan"/>
          {canEdit
            ? <select value={task.assigneeId||''} disabled={busy} onChange={event=>onAssign(event.target.value)} aria-label="Atanan kişi">
                <option value="">Kimseye atanmadı</option>
                {members.map(member=><option key={member.id} value={member.id}>{member.displayName}</option>)}
              </select>
            : <b>{owner?owner.displayName:'Kimseye atanmadı'}</b>}
        </div>
      </section>

      <section className="task-panel-block">
        <h4>Sütun</h4>
        <div className="task-panel-columns">
          {COLUMNS.map(item=><button type="button" key={item.id}
            className={item.id===(task.status==='BACKLOG'?'TODO':task.status)?'active':undefined}
            disabled={!canEdit||busy}
            onClick={()=>onStatus(item.id)}>{item.label}</button>)}
        </div>
      </section>

      <section className="task-panel-block">
        <h4>Sprint</h4>
        {canEdit
          ? <select className="task-panel-sprint" value={task.sprintId||''} disabled={busy} onChange={event=>onSprintChange(event.target.value)} aria-label="Görevin sprint’i">
              <option value="">Backlog</option>
              {sprints.map(item=><option key={item.id} value={item.id}>{item.name}{item.status==='ACTIVE'?' · aktif':''}</option>)}
            </select>
          : <b className="task-panel-value">{sprint?sprint.name:'Backlog'}</b>}
      </section>

      <section className="task-panel-block">
        <h4>Açıklama</h4>
        {task.description
          ? <div className="task-panel-text"><MessageRichText content={task.description}/></div>
          : <p className="task-panel-empty">Açıklama yazılmamış.</p>}
      </section>

      <section className="task-panel-block">
        <h4>Ayrıntı</h4>
        <dl className="task-panel-meta">
          <div><dt>Tür</dt><dd>{KIND_LABEL[taskKind(task)]}</dd></div>
          {taskLabels(task).length>0&&<div><dt>Etiketler</dt><dd><LabelChips task={task} max={10}/></dd></div>}
          <div><dt>Açan</dt><dd>{author?author.displayName:'—'}</dd></div>
          <div><dt>Bitiş</dt><dd className={late?'late':undefined}>{task.dueAt?new Date(task.dueAt).toLocaleDateString('tr-TR',{day:'numeric',month:'long',year:'numeric'}):'—'}</dd></div>
        </dl>
      </section>

      <TaskComments task={task} token={token} members={members} me={me} canManage={canManage} canDeleteAny={canDeleteAny} comments={comments}
        onComment={onComment} onCommentDelete={onCommentDelete} onCommentEdit={onCommentEdit} onCommentReact={onCommentReact}/>
    </div>
    {canEdit&&<footer>
      <button type="button" className="board-button danger" disabled={busy} onClick={()=>setRemoving(true)}>Sil</button>
      <span className="board-spacer"/>
      <button type="button" className="board-button primary" disabled={busy} onClick={onEdit}>Düzenle</button>
    </footer>}
    {removing&&<ConfirmDialog title="Görev silinsin mi?" body={`“${task.title}” panodan kaldırılacak.`} confirmLabel="Görevi sil" danger
      onCancel={()=>setRemoving(false)} onConfirm={()=>{setRemoving(false);void onDelete()}}/>}
  </>
  // Sarmalayıcı her iki görünümde de aynı: yalnızca sınıflar değişir. Farklı
  // etiketler kullanılınca React ağacı söküp yeniden kuruyor ve panel bir kare
  // boyutsuz/yanlış yerde beliriyordu.
  return createPortal(
    <div className={genis?'task-center-layer':'task-side-layer'} onMouseDown={genis?(event=>{if(event.target===event.currentTarget)onClose()}):undefined}>
      <section ref={kutu as any} className={genis?'task-panel task-center':'task-panel task-side'} aria-label={`#${task.number} görevi`}>{govde}</section>
    </div>,document.body)
}

// Tek yorum: sohbet mesajıyla aynı araçlar — üç nokta menüsü ve tepkiler.
// Thread yok: görev zaten konunun kendisi.
function TaskCommentRow({comment,author,own,canManage,canDeleteAny,onDelete,onEdit,onReact}:{
  comment:TaskComment;author?:Member;own:boolean;canManage:boolean;canDeleteAny:boolean
  onDelete:()=>void;onEdit:(content:string)=>void;onReact:(emoji:string)=>void
}){
  const tools=useMessageTools()
  const[editing,setEditing]=useState(false)
  const[draft,setDraft]=useState(comment.content)
  const withinWindow=Date.now()-new Date(comment.createdAt).getTime()<=2*60*60*1000
  const editable=own&&withinWindow
  const deletable=editable||canManage||canDeleteAny
  const kaydet=()=>{const value=draft.trim();if(value&&value!==comment.content)onEdit(value);setEditing(false)}
  return <article className="task-comment" onContextMenu={event=>{event.preventDefault();tools.openAtPoint(event.clientX,event.clientY)}}>
    <Avatar member={author} size={26}/>
    <div>
      <header>
        <b>{author?author.displayName:'Bilinmeyen'}</b>
        <small>{new Date(comment.createdAt).toLocaleString('tr-TR',{day:'numeric',month:'short',hour:'2-digit',minute:'2-digit'})}</small>
        {comment.updatedAt!==comment.createdAt&&<em className="task-comment-edited">düzenlendi</em>}
      </header>
      {editing
        ? <div className="task-comment-edit">
            <textarea autoFocus value={draft} onChange={event=>setDraft(event.target.value)}
              onKeyDown={event=>{if(event.key==='Enter'&&!event.shiftKey){event.preventDefault();kaydet()}if(event.key==='Escape'){event.preventDefault();setEditing(false);setDraft(comment.content)}}}/>
            <div className="task-comment-edit-actions">
              <button type="button" className="board-button" onClick={()=>{setEditing(false);setDraft(comment.content)}}>Vazgeç</button>
              <button type="button" className="board-button primary" onClick={kaydet}>Kaydet</button>
            </div>
          </div>
        : <MessageRichText content={comment.content}/>}
      <ReactionBar reactions={comment.reactions||[]} onToggle={onReact} onAddAt={box=>tools.picker.openAt(box)}/>
    </div>
    <MessageTools tools={tools} actions={{
      own,editable,deletable,content:comment.content,noun:'Yorumu',pinnable:false,
      editedAt:comment.updatedAt!==comment.createdAt?comment.updatedAt:null,
      reactions:comment.reactions,
      onReact,
      onEdit:()=>{setDraft(comment.content);setEditing(true)},
      onDelete,
    }}/>
  </article>
}

// Görevin altındaki konuşma: sohbetle aynı yazım araçları, ek ve emoji.
function TaskComments({task,token,members,me,canManage,canDeleteAny,comments,onComment,onCommentDelete,onCommentEdit,onCommentReact}:{
  task:Task;token:string;members:Member[];me:string;canManage:boolean;canDeleteAny:boolean;comments?:TaskComment[]
  onComment?:(content:string)=>Promise<unknown>|void
  onCommentDelete?:(commentId:string)=>Promise<unknown>|void
  onCommentEdit?:(commentId:string,content:string)=>Promise<unknown>|void
  onCommentReact?:(commentId:string,emoji:string)=>Promise<unknown>|void
}){
  const[draft,setDraft]=useState('')
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const[emojiMenu,setEmojiMenu]=useState<MessageMenu|null>(null)
  const[mentionIndex,setMentionIndex]=useState(0)
  const[sending,setSending]=useState(false)
  const editor=useRef<RichHandle>(null)
  const attachments=useAttachments(token)
  const drop=useFileDrop(files=>void attachments.add(files))
  const liste=comments||[]
  const mentionMatch=/(?:^|\s)@([\w.-]*)$/.exec(draft),mentionQuery=(mentionMatch?.[1]||'').toLocaleLowerCase('tr-TR')
  const mentionItems:MentionSuggestion[]=mentionMatch?members.filter(member=>member.username&&(`${member.displayName} ${member.username}`).toLocaleLowerCase('tr-TR').includes(mentionQuery)).slice(0,8).map(member=>({label:member.displayName,description:`@${member.username}`,value:`@${member.username}`,avatar:userInitials(member.displayName)})):[]
  const pickMention=(item:MentionSuggestion)=>{setDraft(value=>value.replace(/@([\w.-]*)$/,`${item.value} `));setMentionIndex(0);requestAnimationFrame(()=>editor.current?.focus())}
  async function gonder(){
    const content=attachments.appendTo(draft.trim())
    if(!content||sending||attachments.busy)return
    setSending(true)
    try{ await onComment?.(content); setDraft(''); attachments.clear() }
    finally{ setSending(false) }
  }
  return <section className="task-panel-block task-comments">
    <h4>Konuşma{liste.length?` · ${liste.length}`:''}</h4>
    <div className="task-comment-list">
      {liste.map(comment=><TaskCommentRow key={comment.id} comment={comment}
        author={members.find(item=>item.id===comment.authorId)}
        own={comment.authorId===me}
        canManage={canManage}
        canDeleteAny={canDeleteAny}
        onDelete={()=>onCommentDelete?.(comment.id)}
        onEdit={content=>onCommentEdit?.(comment.id,content)}
        onReact={emoji=>onCommentReact?.(comment.id,emoji)}/>)}
      {!liste.length&&<p className="task-panel-empty">Henüz yorum yok. İlk sözü sen söyle.</p>}
    </div>

    <div className="board-editor task-comment-editor" {...drop.handlers}>
      <DropZone active={drop.dragging} label={`#${task.number} görevine eklenecek`}/>
      <ComposerSuggestions mentions={mentionItems} commands={[]} onMention={pickMention} onCommand={()=>{}} activeIndex={mentionItems.length?Math.min(mentionIndex,mentionItems.length-1):-1} onHover={setMentionIndex}/>
      <RichInput ref={editor} value={draft} onChange={value=>{setDraft(value.slice(0,6000));setMentionIndex(0)}}
        onStateChange={setFormat} onFiles={files=>void attachments.add(files)}
        onKeyDown={event=>{if(!mentionItems.length)return;if(event.key==='ArrowDown'){event.preventDefault();setMentionIndex(index=>(index+1)%mentionItems.length)}else if(event.key==='ArrowUp'){event.preventDefault();setMentionIndex(index=>(index-1+mentionItems.length)%mentionItems.length)}else if(event.key==='Enter'||event.key==='Tab'){event.preventDefault();pickMention(mentionItems[Math.min(mentionIndex,mentionItems.length-1)])}}}
        onEnter={()=>void gonder()}
        placeholder="Bu görev hakkında yaz…"/>
      <AttachmentQueue state={attachments}/>
      <div className="board-editor-tools">
        <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('bold')}><ComposerIcon name="bold"/></button>
        <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('italic')}><ComposerIcon name="italic"/></button>
        <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.openLink()}><ComposerIcon name="link"/></button>
        <button type="button" className={`emoji-toggle${emojiMenu?' active':''}`} title="Emoji" onMouseDown={event=>event.preventDefault()} onClick={event=>setEmojiMenu(current=>current?null:anchorFrom(event.currentTarget.getBoundingClientRect()))}><ComposerIcon name="emoji"/></button>
        <button type="button" title="Birini etiketle" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.insert('@')}><ComposerIcon name="mention"/></button>
        <AttachmentPicker state={attachments}/>
        <i/>
        {attachments.error&&<small className="board-editor-error">{attachments.error}</small>}
        <button type="button" className="board-button primary" disabled={sending||attachments.busy||(!draft.trim()&&!attachments.files.length)} onClick={()=>void gonder()}>{sending?'…':'Gönder'}</button>
      </div>
      {emojiMenu&&<MessageMenuPopup menu={emojiMenu} onClose={()=>setEmojiMenu(null)} className="reaction-picker-popup task-comment-emoji-menu">
        <EmojiPanel onDismiss={()=>setEmojiMenu(null)} onSelect={emoji=>{editor.current?.insert(emoji);setEmojiMenu(null)}}/>
      </MessageMenuPopup>}
    </div>
  </section>
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

function SprintDialog({sprints,tasks,members,busy,onClose,onSave,onDelete,onMoveToSprint,onFinish}:{
  sprints:Sprint[];tasks:Task[];members:Member[];busy?:boolean;onClose:()=>void
  onSave:(sprint:Partial<Sprint>&{id?:string})=>Promise<unknown>
  onDelete:(id:string)=>Promise<unknown>
  onMoveToSprint:(sprintId:string,taskIds:string[])=>Promise<unknown>
  onFinish:(sprint:Sprint,target:'backlog'|'new'|string,newSprintName?:string)=>Promise<unknown>
}){
  // Pencere tek iş yapar: ya listeyi gösterir, ya sprint formunu, ya da
  // backlog'dan görev aktarma ekranını. (Hepsi bir aradayken hangi düğmenin
  // neyi kaydettiği belirsizdi.)
  const[ekran,setEkran]=useState<'liste'|'form'|'aktar'|'bitir'>(sprints.length?'liste':'form')
  const[editing,setEditing]=useState<Sprint|null>(null)
  const[hedefSprint,setHedefSprint]=useState<Sprint|null>(null)
  const[removing,setRemoving]=useState<Sprint|null>(null)
  const[finishing,setFinishing]=useState<Sprint|null>(null)
  useEscape(true,onClose)

  const araligi=(sprint:Sprint)=>sprint.startsAt&&sprint.endsAt
    ?`${dayText(sprint.startsAt)} – ${dayText(sprint.endsAt)}`
    :sprint.endsAt?`bitiş ${dayText(sprint.endsAt)}`:'tarih yok'
  const sayac=(sprint:Sprint)=>tasks.filter(task=>task.sprintId===sprint.id).length
  const backlog=tasks.filter(task=>!task.sprintId)

  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal board-dialog sprint-dialog">
      <header>
        <div>
          <b>{ekran==='liste'?'Sprint’ler':ekran==='form'?(editing?'Sprint’i düzenle':'Yeni sprint'):ekran==='bitir'?'Sprint’i bitir':`Backlog’dan aktar${hedefSprint?` · ${hedefSprint.name}`:''}`}</b>
          <small>{ekran==='liste'
            ? 'Aynı anda tek sprint aktif olur; yeni sprint başlayınca diğeri planlanana döner.'
            : ekran==='form'
              ? 'Ad, hedef ve takvimden tarih aralığı.'
              : ekran==='bitir'
                ? 'Bitmemiş görevlerin nereye taşınacağını seç.'
                : 'Sprint’e alınacak görevleri seç.'}</small>
        </div>
        <button type="button" onClick={onClose} aria-label="Kapat">×</button>
      </header>

      {ekran==='liste'&&<>
        <div className="sprint-list">
          {sprints.map(sprint=>{
            const ilerleme=sprintProgress(sprint)
            return <article key={sprint.id} className={`sprint-row ${sprint.status.toLowerCase()}`}>
              <div>
                <b>{sprint.name}</b>
                <small>{sprint.goal||'Hedef yazılmadı'}</small>
                <span className="sprint-range">{araligi(sprint)} · {sayac(sprint)} görev{ilerleme&&sprint.status==='ACTIVE'?` · ${ilerleme.left>0?`${ilerleme.left} gün kaldı`:'süre doldu'}`:''}</span>
              </div>
              <span className={`sprint-state ${sprint.status.toLowerCase()}`}>{sprint.status==='ACTIVE'?'Aktif':sprint.status==='DONE'?'Bitti':'Planlandı'}</span>
              <div className="sprint-actions">
                {sprint.status!=='DONE'&&<button type="button" className="board-button" disabled={busy||!backlog.length} title={backlog.length?'Backlog’daki görevleri bu sprint’e al':'Backlog boş'}
                  onClick={()=>{setHedefSprint(sprint);setEkran('aktar')}}>Görev aktar</button>}
                <button type="button" className="board-button" disabled={busy} onClick={()=>{setEditing(sprint);setEkran('form')}}>Düzenle</button>
                {sprint.status==='PLANNED'&&<button type="button" className="board-button" disabled={busy} onClick={async()=>{await onSave({id:sprint.id,status:'ACTIVE'});bildir(`“${sprint.name}” başlatıldı.`)}}>Başlat</button>}
                {sprint.status==='ACTIVE'&&<button type="button" className="board-button" disabled={busy} onClick={()=>{setFinishing(sprint);setEkran('bitir')}}>Erken bitir</button>}
                <button type="button" className="board-button danger" disabled={busy} onClick={()=>setRemoving(sprint)}>Sil</button>
              </div>
            </article>
          })}
          {!sprints.length&&<p className="board-empty">Henüz sprint yok.</p>}
        </div>
        <footer>
          <span className="board-spacer"/>
          <button type="button" className="board-button" onClick={onClose}>Kapat</button>
          <button type="button" className="board-button primary" onClick={()=>{setEditing(null);setEkran('form')}}>＋ Yeni sprint</button>
        </footer>
      </>}

      {ekran==='form'&&<SprintForm sprint={editing} busy={busy}
        onCancel={()=>{
          setEditing(null)
          // İlk sprint henüz oluşturulmadıysa geri dönülecek bir liste ekranı
          // yoktur; bu durumda Vazgeç doğrudan pencereyi kapatır.
          if(!sprints.length){onClose();return}
          setEkran('liste')
        }}
        onSave={async draft=>{
          await onSave(draft)
          bildir(editing?`“${draft.name}” güncellendi.`:`“${draft.name}” oluşturuldu.`)
          setEditing(null);setEkran('liste')
        }}/>}

      {ekran==='aktar'&&hedefSprint&&<BacklogPicker sprint={hedefSprint} tasks={backlog} members={members} busy={busy}
        onCancel={()=>{setHedefSprint(null);setEkran('liste')}}
        onMove={async ids=>{
          await onMoveToSprint(hedefSprint.id,ids)
          bildir(`${ids.length} görev “${hedefSprint.name}” sprint’ine alındı.`)
          setHedefSprint(null);setEkran('liste')
        }}/>}

      {ekran==='bitir'&&finishing&&<FinishSprint sprint={finishing} sprints={sprints} tasks={tasks} busy={busy}
        onCancel={()=>{setFinishing(null);setEkran('liste')}}
        onFinish={async(target,newName)=>{
          await onFinish(finishing,target,newName)
          bildir(`“${finishing.name}” bitirildi; tamamlanmamış görevler aktarıldı.`)
          setFinishing(null);setEkran('liste')
        }}/>}

      {removing&&<ConfirmDialog title="Sprint silinsin mi?"
        body={`“${removing.name}” silinecek. Görevleri silinmez, backlog’a döner.`}
        confirmLabel="Sprint’i sil" danger
        onCancel={()=>setRemoving(null)}
        onConfirm={async()=>{const hedef=removing;setRemoving(null);await onDelete(hedef.id);bildir(`“${hedef.name}” silindi, görevleri backlog’a döndü.`)}}/>}
    </section>
  </div>
}

function FinishSprint({sprint,sprints,tasks,busy,onCancel,onFinish}:{
  sprint:Sprint;sprints:Sprint[];tasks:Task[];busy?:boolean
  onCancel:()=>void;onFinish:(target:'backlog'|'new'|string,newName?:string)=>Promise<unknown>
}){
  const[target,setTarget]=useState<'backlog'|'new'|string>('backlog')
  const[newName,setNewName]=useState(`${sprint.name} devamı`)
  const unfinished=tasks.filter(task=>task.sprintId===sprint.id&&task.status!=='DONE')
  const alternatives=sprints.filter(item=>item.id!==sprint.id&&item.status!=='DONE')
  const tamamla=()=>{if(target==='new'&&newName.trim().length<2)return;void onFinish(target,target==='new'?newName.trim():undefined)}
  return <>
    <div className="sprint-finish">
      <div className="sprint-finish-summary">
        <b>{sprint.name}</b>
        <span>{unfinished.length} tamamlanmamış görev</span>
        <p>Tamamlanan görevler bu sprint’in geçmişinde kalır. Diğer görevler seçtiğin hedefe taşınır.</p>
      </div>
      {unfinished.length>0&&<label className="board-field wide"><span>Tamamlanmamış görevler</span>
        <select autoFocus value={target} onChange={event=>setTarget(event.target.value)}>
          <option value="backlog">Backlog’a taşı</option>
          {alternatives.map(item=><option key={item.id} value={item.id}>{item.name}{item.status==='ACTIVE'?' · aktif':''}</option>)}
          <option value="new">Yeni sprint oluştur ve taşı</option>
        </select>
      </label>}
      {unfinished.length>0&&target==='new'&&<label className="board-field wide"><span>Yeni sprint adı</span>
        <input value={newName} maxLength={60} onChange={event=>setNewName(event.target.value)} onKeyDown={event=>{if(event.key==='Enter')tamamla()}}/>
      </label>}
    </div>
    <footer>
      <span className="board-spacer"/>
      <button type="button" className="board-button" onClick={onCancel}>Vazgeç</button>
      <button type="button" className="board-button primary" disabled={busy||(target==='new'&&newName.trim().length<2)} onClick={tamamla}>Sprint’i bitir</button>
    </footer>
  </>
}

// Sprint formu: ad, hedef ve takvimden aralık. Listeyle aynı anda görünmez.
function SprintForm({sprint,busy,onCancel,onSave}:{
  sprint:Sprint|null;busy?:boolean;onCancel:()=>void
  onSave:(draft:Partial<Sprint>&{id?:string;name:string})=>Promise<unknown>
}){
  const[name,setName]=useState(sprint?.name||'')
  const[goal,setGoal]=useState(sprint?.goal||'')
  const[from,setFrom]=useState(sprint?.startsAt?gunAnahtar(new Date(sprint.startsAt)):'')
  const[to,setTo]=useState(sprint?.endsAt?gunAnahtar(new Date(sprint.endsAt)):'')
  const kaydet=()=>{
    if(name.trim().length<2)return
    void onSave({
      ...(sprint?{id:sprint.id}:{}),
      name:name.trim(),goal:goal.trim(),
      // Gün başı ve gün sonu: sprint seçilen günlerin tamamını kapsar.
      ...(from?{startsAt:gunOku(from).toISOString()}:{}),
      ...(to?{endsAt:new Date(gunOku(to).setHours(23,59,59,0)).toISOString()}:{}),
    } as Partial<Sprint>&{id?:string;name:string})
  }
  return <>
    <div className="sprint-form">
      <div className="board-grid">
        <label className="board-field wide"><span>Sprint adı</span>
          <input autoFocus value={name} maxLength={60} onChange={event=>setName(event.target.value)} placeholder="Sprint 1" onKeyDown={event=>{if(event.key==='Enter')kaydet()}}/>
        </label>
        <label className="board-field wide"><span>Hedef</span>
          <input value={goal} maxLength={240} onChange={event=>setGoal(event.target.value)} placeholder="Bu sprint’te ne bitecek?"/>
        </label>
      </div>
      <RangeCalendar from={from} to={to} onPick={(basla,bit)=>{setFrom(basla);setTo(bit)}}/>
    </div>
    <footer>
      <span className="board-spacer"/>
      <button type="button" className="board-button" onClick={onCancel}>Vazgeç</button>
      <button type="button" className="board-button primary" disabled={busy||name.trim().length<2} onClick={kaydet}>{sprint?'Kaydet':'Sprint oluştur'}</button>
    </footer>
  </>
}

// Backlog'dan sprint'e toplu aktarım.
function BacklogPicker({sprint,tasks,members,busy,onCancel,onMove}:{
  sprint:Sprint;tasks:Task[];members:Member[];busy?:boolean
  onCancel:()=>void;onMove:(ids:string[])=>Promise<unknown>
}){
  const[secili,setSecili]=useState<string[]>([])
  const[search,setSearch]=useState('')
  const needle=search.trim().toLocaleLowerCase('tr-TR')
  const liste=tasks.filter(task=>!needle||`${task.number} ${task.title}`.toLocaleLowerCase('tr-TR').includes(needle))
  const cevir=(id:string)=>setSecili(current=>current.includes(id)?current.filter(item=>item!==id):[...current,id])
  const puan=secili.reduce((total,id)=>total+(tasks.find(task=>task.id===id)?.points||0),0)
  return <>
    <div className="backlog-picker">
      <div className="backlog-picker-head">
        <div className="admin-search"><span>⌕</span><input value={search} onChange={event=>setSearch(event.target.value)} placeholder="Backlog’da ara"/></div>
        <button type="button" className="board-button" onClick={()=>setSecili(secili.length===liste.length?[]:liste.map(task=>task.id))}>
          {secili.length===liste.length&&liste.length?'Seçimi kaldır':'Tümünü seç'}
        </button>
      </div>
      <div className="backlog-list">
        {liste.map(task=>{
          const owner=members.find(item=>item.id===task.assigneeId)
          return <label key={task.id} className={secili.includes(task.id)?'backlog-item picked':'backlog-item'}>
            <input type="checkbox" checked={secili.includes(task.id)} onChange={()=>cevir(task.id)}/>
            <code>#{task.number}</code>
            <span>{task.title}</span>
            <em className={`board-row-priority ${task.priority.toLowerCase()}`}>{priorityLabel(task.priority)}</em>
            {task.points!==null&&<small>{task.points}p</small>}
            <Avatar member={owner} size={20} label="Atanan"/>
          </label>
        })}
        {!liste.length&&<p className="board-empty">{tasks.length?'Eşleşen görev yok.':'Backlog boş.'}</p>}
      </div>
    </div>
    <footer>
      <b className="backlog-summary">{secili.length} görev{puan?` · ${puan} puan`:''}</b>
      <span className="board-spacer"/>
      <button type="button" className="board-button" onClick={onCancel}>Vazgeç</button>
      <button type="button" className="board-button primary" disabled={busy||!secili.length} onClick={()=>void onMove(secili)}>“{sprint.name}” sprint’ine al</button>
    </footer>
  </>
}
