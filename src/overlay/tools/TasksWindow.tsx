import { useEffect, useMemo, useRef, useState, type FormEvent, type PointerEvent as ReactPointerEvent } from 'react'
import Avatar from '../../ui/Avatar'
import { BOARD_COLUMNS, PRIORITY_LABEL, STATUS_LABEL, TASK_PRIORITIES, TASK_STATUSES, type Task, type TaskPriority, type TaskStatus, type UpdateTaskInput } from '../../api/tasks'
import { useProjectTasks, type MineRow, type ProjectTasksDemo } from '../../hooks/useProjectTasks'
import { useMyPermissions } from '../../permissions'
import { overlaySessionInfo, useOverlayAuth, useOverlaySnapshot } from '../../overlaySession'
import { dueFromInput, dueInputValue } from '../../taskRules'
import { clearOpenTarget, useOpenTarget } from '../openTargets'
import { pushOverlayToast } from '../toastStore'
import type { OverlayToolProps } from '../overlayTypes'
import './tasks.css'

// Görevler penceresi (overlay): "bana atananlar" listesi ve kompakt kanban. Üstte her zaman görünen
// "Yeni görev" (yetki yoksa pasif + ipucu), satıra/karta tıklayınca satır içi düzenleme paneli
// (başlık, açıklama, öncelik, sütun, atanan, teslim), sütunlar arası sürükle-bırak (pointer tabanlı;
// offscreen pencerede HTML5 drag güvenilir değil). Sabitliyken yalnız "Bana atananlar" salt-okunur çizilir.
// `demo` verilirse ağ yok (showroom): sahte oturum, yetkiler açık, veriler yerelde değişir.
type Person={id:string;displayName:string;avatarUrl?:string|null}
type Tab='mine'|'board'
type OpenTaskTarget={taskId?:string;boardId?:string|null;projectId?:string|null}
export type TasksWindowDemo=ProjectTasksDemo&{me:string;people:Person[]}
type Props=OverlayToolProps&{demo?:TasksWindowDemo}
type Drag={taskId:string;title:string;x:number;y:number;over:TaskStatus|null}

const DRAG_START_PX=6
const dayText=(value:string|null)=>value?new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'short'}):''
const isLate=(value:string|null)=>Boolean(value)&&Date.parse(value as string)<Date.now()
const message=(failure:unknown,fallback:string)=>(failure as {message?:string})?.message||fallback
function openInMain(task:Task){window.p2overlay?.openInMain({kind:'task',id:task.id,boardId:task.boardId,projectId:task.projectId})}

function OpenIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/></svg>}
function PlusIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg>}
function BackIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M19 12H5M11 6l-6 6 6 6"/></svg>}

// "Bana atananlar" satırı: öncelik noktası, başlık, pano · sütun · teslim; tıklayınca ayrıntı paneli.
function MineItem({row,pinned,flash,onOpen,onMove}:{row:MineRow;pinned:boolean;flash:boolean;onOpen:()=>void;onMove:(status:TaskStatus)=>void}){
  const{task}=row
  const late=task.status!=='DONE'&&isLate(task.dueAt)
  const main=<>
    <span className="ot-row-title" title={task.title}><em>#{task.number}</em>{task.title}</span>
    <span className="ot-row-meta">
      <span title={row.boardName}>{row.boardName}</span>
      <span className={`ot-status ${task.status}`}>{STATUS_LABEL[task.status]||task.status}</span>
      {task.dueAt&&<span className={`ot-due${late?' late':''}`} title={late?'Teslim tarihi geçti':'Teslim tarihi'}>{late?'Gecikti · ':''}{dayText(task.dueAt)}</span>}
    </span>
  </>
  return <div className={`ot-row${flash?' flash':''}`}>
    <i className={`ot-dot ${task.priority}`} title={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`} aria-label={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`}/>
    {pinned?<div className="ot-row-main">{main}</div>:<button type="button" className="ot-row-main" title="Ayrıntı ve düzenleme" onClick={onOpen}>{main}</button>}
    {!pinned&&<div className="ot-row-actions">
      <select className="p2-select ot-select" value={task.status} aria-label={`${task.title}: sütun`} title="Sütunu değiştir" onChange={event=>onMove(event.target.value as TaskStatus)}>
        {TASK_STATUSES.map(status=><option key={status} value={status}>{STATUS_LABEL[status]}</option>)}
      </select>
    </div>}
  </div>
}

// Kanban kartı: tıklayınca ayrıntı paneli; basılı tutup sürükleyince başka sütuna taşınır.
function Card({task,me,person,flash,dragging,onOpen,onDragStart}:{task:Task;me:string;person?:Person;flash:boolean;dragging:boolean;onOpen:()=>void;onDragStart:(event:ReactPointerEvent<HTMLButtonElement>)=>void}){
  return <button type="button" className={`ot-card${task.assigneeId===me?' mine':''}${flash?' flash':''}${dragging?' dragging':''}`} title="Ayrıntı · sürükleyerek taşı" onClick={onOpen} onPointerDown={onDragStart}>
    <i className={`ot-dot ${task.priority}`} aria-label={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`}/>
    <span className="ot-card-title"><em>#{task.number}</em>{task.title}</span>
    {task.assigneeId&&<Avatar id={task.assigneeId} name={person?.displayName||'Üye'} url={person?.avatarUrl} size="xs"/>}
  </button>
}

function QuickCreate({boardId,me,canAssign,onCreate,onClose}:{boardId:string;me:string;canAssign:boolean;onCreate:(boardId:string,input:{title:string;description?:string;status:TaskStatus;priority:TaskPriority;assigneeId?:string})=>Promise<unknown>;onClose:()=>void}){
  const[title,setTitle]=useState(''),[description,setDescription]=useState('')
  const[status,setStatus]=useState<TaskStatus>('TODO'),[priority,setPriority]=useState<TaskPriority>('MEDIUM')
  const[assignMe,setAssignMe]=useState(true)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  const valid=title.trim().length>=2
  async function submit(event:FormEvent){
    event.preventDefault()
    if(!valid||busy)return
    setBusy(true);setError('')
    try{
      await onCreate(boardId,{title:title.trim(),description:description.trim()||undefined,status,priority,assigneeId:assignMe&&canAssign?me:undefined})
      pushOverlayToast({id:`task-created-${Date.now()}`,kind:'success',title:'Görev oluşturuldu',text:title.trim(),duration:3500})
      onClose()
    }catch(failure){setError(message(failure,'Görev oluşturulamadı.'))}
    finally{setBusy(false)}
  }
  return <form className="ot-form" onSubmit={submit} aria-label="Yeni görev">
    <div className="ot-form-head"><span className="p2-kicker">Yeni görev</span><button type="button" className="p2-btn sm ghost" onClick={onClose}>Vazgeç</button></div>
    <input className="p2-input" value={title} maxLength={160} placeholder="Görev başlığı" aria-label="Görev başlığı" autoFocus onChange={event=>setTitle(event.target.value)} onKeyDown={event=>{if(event.key==='Escape')onClose()}}/>
    <textarea className="p2-textarea" value={description} maxLength={12000} placeholder="Açıklama (isteğe bağlı)" aria-label="Açıklama" rows={2} onChange={event=>setDescription(event.target.value)}/>
    <div className="ot-form-row">
      <label>Sütun<select className="p2-select ot-select" value={status} onChange={event=>setStatus(event.target.value as TaskStatus)}>{TASK_STATUSES.map(item=><option key={item} value={item}>{STATUS_LABEL[item]}</option>)}</select></label>
      <label>Öncelik<select className="p2-select ot-select" value={priority} onChange={event=>setPriority(event.target.value as TaskPriority)}>{TASK_PRIORITIES.map(item=><option key={item} value={item}>{PRIORITY_LABEL[item]}</option>)}</select></label>
    </div>
    <div className="ot-form-actions">
      <span className="ot-assign" onClick={()=>setAssignMe(value=>!value)}>
        <button type="button" className="p2-toggle" role="switch" aria-checked={assignMe} aria-label="Bana ata" onClick={event=>{event.stopPropagation();setAssignMe(value=>!value)}}><i/></button>
        Bana ata
      </span>
      {error&&<span className="ot-error" title={error}>{error}</span>}
      {!error&&<span style={{flex:1}}/>}
      <button type="submit" className="p2-btn sm primary" disabled={!valid||busy}>{busy?'Oluşturuluyor…':'Oluştur'}</button>
    </div>
  </form>
}

// Satır içi ayrıntı/düzenleme paneli: Kaydet yalnız değişen alanları yollar; sütun değişimi "taşı" ucundan gider.
function TaskDetail({task,me,people,boardName,editable,onBack,onMove,onUpdate}:{task:Task;me:string;people:Map<string,Person>;boardName:string;editable:boolean;onBack:()=>void;onMove:(status:TaskStatus)=>Promise<void>;onUpdate:(patch:UpdateTaskInput)=>Promise<unknown>}){
  const[title,setTitle]=useState(task.title),[description,setDescription]=useState(task.description||'')
  const[status,setStatus]=useState<TaskStatus>(task.status),[priority,setPriority]=useState<TaskPriority>(task.priority)
  const[assigneeId,setAssigneeId]=useState(task.assigneeId||''),[due,setDue]=useState(dueInputValue(task.dueAt))
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  const options=useMemo(()=>{
    const list=[...people.values()].sort((a,b)=>a.displayName.localeCompare(b.displayName,'tr'))
    // Atanan kişi ekip listesinde yoksa (ayrılmış üye) seçenek olarak yine de gösterilir.
    if(task.assigneeId&&!people.has(task.assigneeId))list.push({id:task.assigneeId,displayName:'Üye'})
    return list
  },[people,task.assigneeId])
  const dueIso=due?dueFromInput(due,task.dueAt):null
  const patch:UpdateTaskInput={}
  if(title.trim()!==task.title)patch.title=title.trim()
  if(description.trim()!==(task.description||''))patch.description=description.trim()
  if(priority!==task.priority)patch.priority=priority
  if(assigneeId!==(task.assigneeId||''))patch.assigneeId=assigneeId
  if(dueInputValue(dueIso)!==dueInputValue(task.dueAt))patch.dueAt=dueIso
  const dirty=Object.keys(patch).length>0||status!==task.status
  const valid=title.trim().length>=2
  async function submit(event:FormEvent){
    event.preventDefault()
    if(!dirty||!valid||busy||!editable)return
    setBusy(true);setError('')
    try{
      if(status!==task.status)await onMove(status)
      if(Object.keys(patch).length)await onUpdate(patch)
      pushOverlayToast({id:`task-saved-${task.id}`,tag:`task-saved-${task.id}`,kind:'success',title:'Görev güncellendi',text:title.trim(),duration:3000})
      onBack()
    }catch(failure){setError(message(failure,'Görev kaydedilemedi.'))}
    finally{setBusy(false)}
  }
  return <form className="ot-detail" onSubmit={submit} aria-label={`Görev #${task.number}`}>
    <div className="ot-detail-head">
      <button type="button" className="p2-btn sm icon ghost" title="Listeye dön" aria-label="Listeye dön" onClick={onBack}><BackIcon/></button>
      <span className="ot-detail-id"><em>#{task.number}</em><span className={`ot-status ${task.status}`}>{STATUS_LABEL[task.status]}</span><small title={boardName}>{boardName}</small></span>
      <button type="button" className="p2-btn sm secondary" title="Görevi ana pencerede aç" onClick={()=>openInMain(task)}><OpenIcon/>Uygulamada aç</button>
    </div>
    <input className="p2-input ot-detail-title" value={title} maxLength={160} placeholder="Görev başlığı" aria-label="Görev başlığı" disabled={!editable} onChange={event=>setTitle(event.target.value)} onKeyDown={event=>{if(event.key==='Escape')onBack()}}/>
    <textarea className="p2-textarea" value={description} maxLength={12000} placeholder={editable?'Açıklama (isteğe bağlı)':'Açıklama yok'} aria-label="Açıklama" rows={3} disabled={!editable} onChange={event=>setDescription(event.target.value)}/>
    <div className="ot-form-row">
      <label>Sütun<select className="p2-select ot-select" value={status} disabled={!editable} onChange={event=>setStatus(event.target.value as TaskStatus)}>{TASK_STATUSES.map(item=><option key={item} value={item}>{STATUS_LABEL[item]}</option>)}</select></label>
      <label>Öncelik<select className="p2-select ot-select" value={priority} disabled={!editable} onChange={event=>setPriority(event.target.value as TaskPriority)}>{TASK_PRIORITIES.map(item=><option key={item} value={item}>{PRIORITY_LABEL[item]}</option>)}</select></label>
    </div>
    <div className="ot-form-row">
      <label>Atanan<select className="p2-select ot-select" value={assigneeId} disabled={!editable} onChange={event=>setAssigneeId(event.target.value)}>
        <option value="">Atanmamış</option>
        {options.map(person=><option key={person.id} value={person.id}>{person.id===me?`${person.displayName} (ben)`:person.displayName}</option>)}
      </select></label>
      <label>Teslim<span className="ot-date"><input type="date" className="p2-input ot-select" value={due} disabled={!editable} aria-label="Teslim tarihi" onChange={event=>setDue(event.target.value)}/>{due&&editable&&<button type="button" className="p2-btn sm icon ghost" title="Teslim tarihini kaldır" aria-label="Teslim tarihini kaldır" onClick={()=>setDue('')}>×</button>}</span></label>
    </div>
    {!editable&&<div className="ot-muted">Bu görevi yalnız oluşturan, atanan kişi ya da pano yöneticisi düzenleyebilir.</div>}
    <div className="ot-form-actions">
      {error?<span className="ot-error" title={error}>{error}</span>:<span className="ot-muted" style={{flex:1}}>{dirty?'Kaydedilmemiş değişiklik var':''}</span>}
      <button type="button" className="p2-btn sm ghost" onClick={onBack}>{dirty&&editable?'İptal':'Kapat'}</button>
      {editable&&<button type="submit" className="p2-btn sm primary" disabled={!dirty||!valid||busy}>{busy?'Kaydediliyor…':'Kaydet'}</button>}
    </div>
  </form>
}

export default function TasksWindow(props:Props){
  const{demo}=props
  const auth=useOverlayAuth()
  const snapshot=useOverlaySnapshot()
  const projectId=demo?'demo':(props.projectId||overlaySessionInfo()?.projectId||auth.selectedProjectId||null)
  const me=demo?demo.me:String((auth.user as {id?:string}|null)?.id||'')
  const organizationId=demo?'demo':String((auth.organization as {id?:string}|null)?.id||'')
  const token=demo?'':auth.accessToken
  const perms=useMyPermissions(token,organizationId)
  const tasks=useProjectTasks({token,organizationId,projectId,me,demo})
  const open=useOpenTarget<OpenTaskTarget>('tasks')

  const[tab,setTab]=useState<Tab>('mine')
  const[boardId,setBoardId]=useState('')
  const[creating,setCreating]=useState(false)
  const[selectedId,setSelectedId]=useState('')
  const[flashId,setFlashId]=useState('')
  const[drag,setDrag]=useState<Drag|null>(null)
  const dragRef=useRef<{taskId:string;title:string;startX:number;startY:number;active:boolean;pointerId:number}|null>(null)
  const suppressClick=useRef(false)
  const columns=useRef<HTMLDivElement>(null)

  // Kişiler: ekip anlık görüntüsü + ben (atanan avatarları ve "Atanan" seçimi için).
  const people=useMemo(()=>{
    const map=new Map<string,Person>()
    const source=demo?demo.people:(snapshot?.people||[]) as Person[]
    for(const item of source)if(item?.id)map.set(item.id,item)
    const self=demo?demo.people.find(person=>person.id===demo.me)||{id:demo.me,displayName:'Ben'}:(auth.user as Person|null)
    if(self?.id)map.set(self.id,self)
    return map
  },[snapshot?.people,auth.user,demo])

  // Seçili pano: ilk pano; silinirse yine ilkine düşülür.
  useEffect(()=>{if(!tasks.boards.length){setBoardId('');return}setBoardId(current=>tasks.boards.some(board=>board.id===current)?current:tasks.boards[0].id)},[tasks.boards])
  useEffect(()=>{props.setTitle?.(`Görevler${tasks.mine.length?` · ${tasks.mine.length}`:''}`)},[tasks.mine.length])
  // Overlay görünür olunca pano yeniden çekilir (ana pencerede yapılan değişiklikler zaten canlı gelir; bu emniyet).
  useEffect(()=>{if(!props.visible||demo||!auth.ready)return;void tasks.refresh()},[props.visible,auth.ready,projectId])
  // Ana pencereden "görevi aç" hedefi (örn. "sana bir görev atadı" kartı): panoya geç, kartı vurgula.
  useEffect(()=>{
    if(!open?.target)return
    const target=open.target
    if(target.projectId&&projectId&&target.projectId!==projectId){clearOpenTarget('tasks');return}
    if(target.boardId)setBoardId(target.boardId)
    if(target.taskId){setFlashId(target.taskId);setTab(target.boardId?'board':'mine');setSelectedId('')}
    clearOpenTarget('tasks')
  },[open?.at])
  useEffect(()=>{if(!flashId)return;const timer=window.setTimeout(()=>setFlashId(''),4000);return()=>window.clearTimeout(timer)},[flashId])
  useEffect(()=>{if(props.pinned){setTab('mine');setCreating(false);setSelectedId('');setDrag(null)}},[props.pinned])

  async function move(task:Task,status:TaskStatus){
    try{await tasks.move(task.id,status)}
    catch(failure){pushOverlayToast({id:`task-move-${task.id}`,tag:`task-move-${task.id}`,kind:'error',title:'Görev taşınamadı',text:message(failure,'Bu görevi taşıma yetkin yok.'),duration:5000});throw failure}
  }

  // ---- Sürükle-bırak (pointer): eşik aşılınca başlar, hangi sütunun üstündeyse oraya bırakılır. ----
  function dragStart(task:Task,event:ReactPointerEvent<HTMLButtonElement>){
    if(event.button!==0||props.pinned)return
    dragRef.current={taskId:task.id,title:task.title,startX:event.clientX,startY:event.clientY,active:false,pointerId:event.pointerId}
  }
  const columnAt=(x:number,y:number):TaskStatus|null=>{
    const node=document.elementFromPoint(x,y)?.closest<HTMLElement>('[data-status]')
    return node&&columns.current?.contains(node)?node.dataset.status as TaskStatus:null
  }
  useEffect(()=>{
    const onMove=(event:PointerEvent)=>{
      const current=dragRef.current
      if(!current)return
      if(!current.active){
        if(Math.hypot(event.clientX-current.startX,event.clientY-current.startY)<DRAG_START_PX)return
        current.active=true
      }
      const box=columns.current?.getBoundingClientRect()
      setDrag({taskId:current.taskId,title:current.title,x:event.clientX-(box?.left||0),y:event.clientY-(box?.top||0),over:columnAt(event.clientX,event.clientY)})
    }
    const finish=(event:PointerEvent)=>{
      const current=dragRef.current
      dragRef.current=null
      if(!current)return
      if(!current.active){setDrag(null);return}
      // Bırakma sonrası gelen click ayrıntı panelini açmasın.
      suppressClick.current=true;window.setTimeout(()=>{suppressClick.current=false},0)
      setDrag(null)
      const status=event.type==='pointerup'?columnAt(event.clientX,event.clientY):null
      const task=Object.values(tasks.tasksByBoard).flat().find(item=>item.id===current.taskId)
      if(status&&task&&task.status!==status)void move(task,status).catch(()=>{})
    }
    window.addEventListener('pointermove',onMove)
    window.addEventListener('pointerup',finish)
    window.addEventListener('pointercancel',finish)
    return()=>{window.removeEventListener('pointermove',onMove);window.removeEventListener('pointerup',finish);window.removeEventListener('pointercancel',finish)}
  },[tasks.tasksByBoard])
  const openDetail=(taskId:string)=>{if(suppressClick.current)return;setCreating(false);setSelectedId(taskId)}

  if(!demo&&(!auth.ready||!token||!organizationId))return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  if(!projectId)return <div className="p2-empty"><b>Proje seçili değil</b><small>Görevler oyunun projesine göre listelenir.</small></div>

  // Sabitli + küçültülmüş: tek satır özet (açık görev sayısı ve en öncelikli iş).
  if(props.pinned&&props.minimized){
    if(!tasks.mine.length)return null
    const first=tasks.mine[0].task
    return <div className="ot-pill" aria-label="Bana atanan görevler"><span className="p2-badge soft">{tasks.mine.length}</span><i className={`ot-dot ${first.priority}`}/><b>{first.title}</b></div>
  }

  // Yetkiler geç gelebilir: liste boşken (henüz yüklenmedi) izinli sayılır; sunucu zaten son sözü söyler.
  const permsLoaded=perms.owner||perms.permissions.length>0||perms.roles.length>0
  const canCreate=Boolean(demo)||!permsLoaded||perms.can('task.create')
  const canManage=Boolean(demo)||!permsLoaded||perms.can('board.manage')
  const board=tasks.boards.find(item=>item.id===boardId)||null
  const boardTasks=boardId?tasks.tasksByBoard[boardId]||[]:[]
  const showBoard=tab==='board'&&!props.pinned
  const all=Object.values(tasks.tasksByBoard).flat()
  const selected=selectedId?all.find(task=>task.id===selectedId)||null:null
  const createHint=!canCreate?'Görev oluşturma yetkin yok':!board?(tasks.loading?'Pano yükleniyor…':'Önce bir pano gerekir'):'Seçili panoya yeni görev ekle'
  const boardNames=Object.fromEntries(tasks.boards.map(item=>[item.id,item.name])) as Record<string,string>

  return <div className={`ot${drag?' dragging':''}`} aria-label="Görevler">
    <div className="ot-head">
      {!props.pinned&&<div className="p2-tabs segment" role="tablist" aria-label="Görev görünümü">
        <button type="button" role="tab" className={`p2-tab${tab==='mine'?' active':''}`} aria-selected={tab==='mine'} onClick={()=>{setTab('mine');setSelectedId('')}}>Bana atananlar{tasks.mine.length?<span className="p2-badge soft">{tasks.mine.length}</span>:null}</button>
        <button type="button" role="tab" className={`p2-tab${tab==='board'?' active':''}`} aria-selected={tab==='board'} onClick={()=>{setTab('board');setSelectedId('')}}>Pano</button>
      </div>}
      {props.pinned&&<span className="p2-kicker">Bana atananlar{tasks.mine.length?` · ${tasks.mine.length}`:''}</span>}
      <span className="ot-grow"/>
      {showBoard&&tasks.boards.length>1&&<select className="p2-select ot-select ot-board-pick" value={boardId} aria-label="Pano" title="Pano" onChange={event=>setBoardId(event.target.value)}>
        {tasks.boards.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}
      </select>}
      {!props.pinned&&<button type="button" className="p2-btn sm primary ot-new" title={createHint} aria-label={`Yeni görev: ${createHint}`} disabled={!canCreate||!board||creating} onClick={()=>{setSelectedId('');setCreating(true)}}><PlusIcon/>Yeni görev</button>}
    </div>
    {!props.pinned&&!canCreate&&<div className="ot-muted ot-note">Görev oluşturma yetkin yok; görevleri görüntüleyip kendi görevlerini taşıyabilirsin.</div>}

    {creating&&board&&<QuickCreate boardId={board.id} me={me} canAssign={Boolean(me)} onCreate={tasks.create} onClose={()=>setCreating(false)}/>}

    {selected&&!props.pinned?<TaskDetail key={selected.id} task={selected} me={me} people={people} boardName={boardNames[selected.boardId]||'Pano'}
      editable={canManage||selected.createdById===me||selected.assigneeId===me}
      onBack={()=>setSelectedId('')} onMove={status=>move(selected,status)} onUpdate={patch=>tasks.update(selected.id,patch)}/>
    :<>
      {tasks.error&&!tasks.boards.length&&<div className="p2-empty"><b>Pano yüklenemedi</b><small>{tasks.error}</small><button type="button" className="p2-btn sm secondary" onClick={()=>void tasks.refresh()}>Yeniden dene</button></div>}

      {!showBoard&&<div className="ot-body" role="list" aria-label="Bana atanan görevler">
        {tasks.loading&&!tasks.mine.length&&<div className="p2-empty"><small>Görevler yükleniyor…</small></div>}
        {!tasks.loading&&!tasks.mine.length&&!tasks.error&&<div className="p2-empty"><b>Sana atanmış açık görev yok</b><small>{props.pinned?'Yeni atamalar burada görünür.':'"Yeni görev" ile bir iş aç ya da Pano sekmesinden bir görevi kendine ata.'}</small></div>}
        {tasks.mine.map(row=><MineItem key={row.task.id} row={row} pinned={props.pinned} flash={flashId===row.task.id} onOpen={()=>openDetail(row.task.id)} onMove={status=>void move(row.task,status).catch(()=>{})}/>)}
      </div>}

      {showBoard&&<>
        {tasks.loading&&!board&&!tasks.error&&<div className="p2-empty"><small>Pano yükleniyor…</small></div>}
        {!tasks.loading&&!board&&!tasks.error&&<div className="p2-empty"><b>Bu projede pano yok</b><small>Ana penceredeki Görevler sayfasından bir pano açınca burada görünür.</small><button type="button" className="p2-btn sm secondary" onClick={()=>void tasks.refresh()}>Yenile</button></div>}
        {board&&<div ref={columns} className="ot-columns" aria-label={`${board.name} panosu`}>
          {BOARD_COLUMNS.map(status=>{
            const items=boardTasks.filter(task=>task.status===status)
            return <section key={status} className={`ot-col${drag?.over===status?' over':''}`} aria-label={STATUS_LABEL[status]} data-status={status}>
              <header className="ot-col-head"><i className={status}/>{STATUS_LABEL[status]}<b>{items.length}</b></header>
              <div className="ot-col-body">
                {items.map(task=><Card key={task.id} task={task} me={me} person={task.assigneeId?people.get(task.assigneeId):undefined} flash={flashId===task.id} dragging={drag?.taskId===task.id} onOpen={()=>openDetail(task.id)} onDragStart={event=>dragStart(task,event)}/>)}
                {!items.length&&<div className="ot-col-empty">{drag?'Buraya bırak':'Boş'}</div>}
              </div>
            </section>
          })}
          {drag&&<div className="ot-ghost" style={{left:drag.x,top:drag.y}} aria-hidden="true">{drag.title}</div>}
        </div>}
      </>}
    </>}
  </div>
}
