import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { createPortal } from 'react-dom'
import Avatar from '../../ui/Avatar'
import RichInput, { type RichFormatState, type RichHandle } from '../../RichComposer'
import { AttachmentQueue, attachmentsFromContent, useAttachments, withoutAttachments } from '../../attachments'
import { ComposerIcon } from '../../ComposerIcons'
import { BOARD_COLUMNS, KIND_LABEL, KIND_SHORT, PRIORITY_LABEL, STATUS_LABEL, TASK_KINDS, TASK_PRIORITIES, TASK_STATUSES, parseLabels, taskKind, taskLabels, type Task, type TaskKind, type TaskPriority, type TaskStatus, type UpdateTaskInput } from '../../api/tasks'
import { useProjectTasks, type ProjectTasksDemo } from '../../hooks/useProjectTasks'
import { useMyPermissions } from '../../permissions'
import { overlaySessionInfo, useOverlayAuth, useOverlaySnapshot } from '../../overlaySession'
import { dueFromInput, dueInputValue } from '../../taskRules'
import { IconCapture } from '../icons'
import { instanceId, openInstance } from '../layoutStore'
import { clearOpenTarget, useOpenTarget } from '../openTargets'
import { useRecentShots, type RecentShot } from '../screenshotStore'
import { pushOverlayToast } from '../toastStore'
import type { OverlayToolProps } from '../overlayTypes'
import './tasks.css'

// Görevler penceresi (overlay). Görünüm: Liste (sütun başlıklarıyla gruplu satırlar) ya da Pano (kompakt kanban);
// "Benim" süzgeci yalnız bana atananları bırakır. Pano seçici ("Tüm panolar" dahil); her pano "Ayrı pencerede aç" ile
// kendi penceresine (tasks:<boardId>) taşınabilir — modüler. Açıklama sohbetle aynı zengin editör: kalın/italik/bağlantı,
// dosya ve görsel ekleme (yapıştır/sürükle/seç), son ekran görüntüsünü ekleme. Tür (hata/özellik…) ve etiketler.
// Sabitli pencere panel açıkken tam arayüzdür; yalnız panel kapalıyken (hayalet) salt-okunur liste çizilir.
// `demo` verilirse ağ yok (showroom): sahte oturum, yetkiler açık, veriler yerelde değişir.
type Person={id:string;displayName:string;avatarUrl?:string|null}
type View='list'|'board'
type OpenTaskTarget={taskId?:string;boardId?:string|null;projectId?:string|null}
export type TasksWindowDemo=ProjectTasksDemo&{me:string;people:Person[]}
type Props=OverlayToolProps&{demo?:TasksWindowDemo}
type Drag={taskId:string;title:string;x:number;y:number;over:TaskStatus|null}
type Draft={title:string;description:string;status:TaskStatus;priority:TaskPriority;kind:TaskKind;labels:string;assigneeId:string;due:string}

const DRAG_START_PX=6
const ALL_BOARDS='*'
const LIST_ORDER:TaskStatus[]=['IN_PROGRESS','REVIEW','TODO','BACKLOG','DONE']
const dayText=(value:string|null)=>value?new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'short'}):''
const isLate=(value:string|null)=>Boolean(value)&&Date.parse(value as string)<Date.now()
const message=(failure:unknown,fallback:string)=>(failure as {message?:string})?.message||fallback
function openInMain(task:Task){window.p2overlay?.openInMain({kind:'task',id:task.id,boardId:task.boardId,projectId:task.projectId})}
const readView=():View=>{try{return localStorage.getItem('p2:overlay:tasks:view')==='board'?'board':'list'}catch{return 'list'}}
const dataUrlFile=async(shot:RecentShot)=>{
  const response=await fetch(shot.jpegDataUrl as string)
  const blob=await response.blob()
  const name=(shot.path.split(/[\\/]/).pop()||'ekran-goruntusu.png').replace(/\.png$/i,'')+'.jpg'
  return new File([blob],name,{type:'image/jpeg'})
}

function OpenIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/></svg>}
function PlusIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg>}
function BackIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M19 12H5M11 6l-6 6 6 6"/></svg>}
function WindowIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="5" width="13" height="11" rx="2"/><path d="M8 19h11a2 2 0 0 0 2-2V9"/></svg>}
function ListIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round"><path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01"/></svg>}
function BoardIcon(){return <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M9.5 4.5v15M14.5 4.5v15"/></svg>}

function KindBadge({task}:{task:Pick<Task,'kind'>}){const kind=taskKind(task);return kind==='TASK'?null:<span className={`ot-kind ${kind}`} title={`Tür: ${KIND_LABEL[kind]}`}>{KIND_SHORT[kind]}</span>}
function Labels({task,max=3}:{task:Pick<Task,'labels'>;max?:number}){const labels=taskLabels(task);if(!labels.length)return null;return <span className="ot-labels">{labels.slice(0,max).map(label=><em key={label}>{label}</em>)}{labels.length>max&&<em title={labels.slice(max).join(', ')}>+{labels.length-max}</em>}</span>}

// ---- Zengin açıklama editörü: sohbet kutusuyla aynı dil (kalın/italik/bağlantı + ekler) ----
function DescriptionEditor({token,value,onChange,attachments,disabled,placeholder,shots}:{token:string;value:string;onChange(next:string):void;attachments:ReturnType<typeof useAttachments>;disabled?:boolean;placeholder:string;shots:RecentShot[]}){
  const editor=useRef<RichHandle>(null)
  const[format,setFormat]=useState<RichFormatState>({bold:false,italic:false,link:false})
  const[shotPick,setShotPick]=useState(false)
  const[shotBusy,setShotBusy]=useState(false)
  const usable=shots.filter(shot=>shot.jpegDataUrl)
  async function attachShot(shot:RecentShot){
    setShotPick(false);setShotBusy(true)
    try{await attachments.add([await dataUrlFile(shot)])}
    catch{pushOverlayToast({id:`shot-attach-${Date.now()}`,kind:'error',title:'Ekran görüntüsü eklenemedi',duration:3500})}
    finally{setShotBusy(false)}
  }
  void token
  return <div className={`ot-editor${disabled?' disabled':''}`}>
    <RichInput ref={editor} value={value} onChange={next=>onChange(next.slice(0,6000))} disabled={disabled} onStateChange={setFormat} onFiles={files=>void attachments.add(files)} placeholder={placeholder}/>
    <AttachmentQueue state={attachments}/>
    {!disabled&&<div className="ot-editor-tools">
      <button type="button" className={format.bold?'active':undefined} title="Kalın (Ctrl+B)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('bold')}><ComposerIcon name="bold"/></button>
      <button type="button" className={format.italic?'active':undefined} title="İtalik (Ctrl+I)" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.format('italic')}><ComposerIcon name="italic"/></button>
      <button type="button" className={format.link?'active':undefined} title="Bağlantı ekle" onMouseDown={event=>event.preventDefault()} onClick={()=>editor.current?.openLink()}><ComposerIcon name="link"/></button>
      <i/>
      <button type="button" title="Dosya ya da görsel ekle" aria-label="Dosya ekle" onClick={()=>attachments.input.current?.click()}><ComposerIcon name="plus"/></button>
      <input ref={attachments.input} type="file" multiple hidden accept="*/*" onChange={event=>{if(event.target.files)void attachments.add(event.target.files);event.target.value=''}}/>
      <span className="ot-shot-pick-wrap">
        <button type="button" title={usable.length?'Son ekran görüntüsünü ekle':'Bu oturumda ekran görüntüsü yok (F12 ile çek)'} aria-label="Ekran görüntüsü ekle" disabled={!usable.length||shotBusy} onClick={()=>setShotPick(open=>!open)}><IconCapture size={15}/></button>
        {shotPick&&<div className="ot-shot-pick" role="menu">
          {usable.slice(0,6).map(shot=><button type="button" key={shot.id} role="menuitem" onClick={()=>void attachShot(shot)}><img src={shot.jpegDataUrl} alt=""/><small>{shot.width}×{shot.height}</small></button>)}
        </div>}
      </span>
      {(attachments.busy||shotBusy)&&<small>Yükleniyor…</small>}
      {attachments.error&&<small className="error">{attachments.error}</small>}
      <small className="ot-editor-hint">Görseli yapıştırabilir ya da sürükleyebilirsin</small>
    </div>}
  </div>
}

// ---- Satır (liste görünümü) ----
function Row({task,me,person,boardName,ghost,flash,onOpen,onMove}:{task:Task;me:string;person?:Person;boardName?:string;ghost:boolean;flash:boolean;onOpen:()=>void;onMove:(status:TaskStatus)=>void}){
  const late=task.status!=='DONE'&&isLate(task.dueAt)
  const main=<>
    <span className="ot-row-title" title={task.title}><em>#{task.number}</em><KindBadge task={task}/>{task.title}</span>
    <span className="ot-row-meta">
      {boardName&&<span title={boardName}>{boardName}</span>}
      <Labels task={task}/>
      {task.dueAt&&<span className={`ot-due${late?' late':''}`} title={late?'Teslim tarihi geçti':'Teslim tarihi'}>{late?'Gecikti · ':''}{dayText(task.dueAt)}</span>}
    </span>
  </>
  return <div className={`ot-row st-${task.status}${flash?' flash':''}${task.assigneeId===me?' mine':''}`}>
    <i className={`ot-dot ${task.priority}`} title={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`} aria-label={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`}/>
    {ghost?<div className="ot-row-main">{main}</div>:<button type="button" className="ot-row-main" title="Ayrıntı ve düzenleme" onClick={onOpen}>{main}</button>}
    {task.assigneeId&&<Avatar id={task.assigneeId} name={person?.displayName||'Üye'} url={person?.avatarUrl} size="xs"/>}
    {!ghost&&<div className="ot-row-actions">
      <select className="p2-select ot-select" value={task.status} aria-label={`${task.title}: sütun`} title="Sütunu değiştir" onChange={event=>onMove(event.target.value as TaskStatus)}>
        {TASK_STATUSES.map(status=><option key={status} value={status}>{STATUS_LABEL[status]}</option>)}
      </select>
    </div>}
  </div>
}

// ---- Kanban kartı: tıklayınca ayrıntı paneli; basılı tutup sürükleyince başka sütuna taşınır. ----
function Card({task,me,person,flash,dragging,onOpen,onDragStart}:{task:Task;me:string;person?:Person;flash:boolean;dragging:boolean;onOpen:()=>void;onDragStart:(event:ReactPointerEvent<HTMLButtonElement>)=>void}){
  return <button type="button" className={`ot-card${task.assigneeId===me?' mine':''}${flash?' flash':''}${dragging?' dragging':''}`} title="Ayrıntı · sürükleyerek taşı" onClick={onOpen} onPointerDown={onDragStart}>
    <i className={`ot-dot ${task.priority}`} aria-label={`Öncelik: ${PRIORITY_LABEL[task.priority]||task.priority}`}/>
    <span className="ot-card-main">
      <span className="ot-card-title"><em>#{task.number}</em>{task.title}</span>
      {(taskKind(task)!=='TASK'||taskLabels(task).length>0)&&<span className="ot-card-tags"><KindBadge task={task}/><Labels task={task} max={2}/></span>}
    </span>
    {task.assigneeId&&<Avatar id={task.assigneeId} name={person?.displayName||'Üye'} url={person?.avatarUrl} size="xs"/>}
  </button>
}

// ---- Ortak alanlar: tür / öncelik / sütun / etiket / atanan / teslim ----
function Fields({draft,set,people,me,editable,withAssignee}:{draft:Draft;set(patch:Partial<Draft>):void;people:Person[];me:string;editable:boolean;withAssignee:boolean}){
  return <>
    <div className="ot-form-row">
      <label>Tür<select className="p2-select ot-select" value={draft.kind} disabled={!editable} onChange={event=>set({kind:event.target.value as TaskKind})}>{TASK_KINDS.map(item=><option key={item} value={item}>{KIND_LABEL[item]}</option>)}</select></label>
      <label>Öncelik<select className="p2-select ot-select" value={draft.priority} disabled={!editable} onChange={event=>set({priority:event.target.value as TaskPriority})}>{TASK_PRIORITIES.map(item=><option key={item} value={item}>{PRIORITY_LABEL[item]}</option>)}</select></label>
      <label>Sütun<select className="p2-select ot-select" value={draft.status} disabled={!editable} onChange={event=>set({status:event.target.value as TaskStatus})}>{TASK_STATUSES.map(item=><option key={item} value={item}>{STATUS_LABEL[item]}</option>)}</select></label>
    </div>
    <div className="ot-form-row">
      <label className="grow">Etiketler<input className="p2-input ot-select" value={draft.labels} disabled={!editable} placeholder="ui, ses, seviye-3" title="Virgülle ayır; en çok 10 etiket" onChange={event=>set({labels:event.target.value})}/></label>
      {withAssignee&&<label>Atanan<select className="p2-select ot-select" value={draft.assigneeId} disabled={!editable} onChange={event=>set({assigneeId:event.target.value})}>
        <option value="">Atanmamış</option>
        {people.map(person=><option key={person.id} value={person.id}>{person.id===me?`${person.displayName} (ben)`:person.displayName}</option>)}
      </select></label>}
      <label>Teslim<span className="ot-date"><input type="date" className="p2-input ot-select" value={draft.due} disabled={!editable} aria-label="Teslim tarihi" onChange={event=>set({due:event.target.value})}/>{draft.due&&editable&&<button type="button" className="p2-btn sm icon ghost" title="Teslim tarihini kaldır" aria-label="Teslim tarihini kaldır" onClick={()=>set({due:''})}>×</button>}</span></label>
    </div>
  </>
}

function QuickCreate({token,boardId,boardName,me,people,shots,onCreate,onClose}:{token:string;boardId:string;boardName:string;me:string;people:Person[];shots:RecentShot[];onCreate:(boardId:string,input:{title:string;description?:string;status:TaskStatus;priority:TaskPriority;kind:TaskKind;labels:string[];assigneeId?:string;dueAt?:string})=>Promise<unknown>;onClose:()=>void}){
  const[draft,setDraft]=useState<Draft>({title:'',description:'',status:'TODO',priority:'MEDIUM',kind:'TASK',labels:'',assigneeId:me,due:''})
  const attachments=useAttachments(token)
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  const set=(patch:Partial<Draft>)=>setDraft(current=>({...current,...patch}))
  const valid=draft.title.trim().length>=2&&!attachments.busy
  async function submit(event:FormEvent){
    event.preventDefault()
    if(!valid||busy)return
    setBusy(true);setError('')
    try{
      const dueAt=draft.due?dueFromInput(draft.due,null):null
      await onCreate(boardId,{title:draft.title.trim(),description:attachments.appendTo(draft.description)||undefined,status:draft.status,priority:draft.priority,kind:draft.kind,labels:parseLabels(draft.labels),assigneeId:draft.assigneeId||undefined,dueAt:dueAt||undefined})
      pushOverlayToast({id:`task-created-${Date.now()}`,kind:'success',title:'Görev oluşturuldu',text:draft.title.trim(),duration:3500})
      onClose()
    }catch(failure){setError(message(failure,'Görev oluşturulamadı.'))}
    finally{setBusy(false)}
  }
  return <form className="ot-form" onSubmit={submit} aria-label="Yeni görev">
    <div className="ot-form-head"><div><b className="ot-form-title">Yeni görev</b><small className="ot-muted">{boardName} panosuna eklenir</small></div><button type="button" className="p2-btn sm icon ghost" aria-label="Kapat" title="Kapat (Esc)" onClick={onClose}>×</button></div>
    <input className="p2-input" value={draft.title} maxLength={160} placeholder="Görev başlığı" aria-label="Görev başlığı" autoFocus onChange={event=>set({title:event.target.value})} onKeyDown={event=>{if(event.key==='Escape')onClose()}}/>
    <DescriptionEditor token={token} value={draft.description} onChange={description=>set({description})} attachments={attachments} shots={shots} placeholder="Açıklama (isteğe bağlı): adımlar, kabul kriterleri… Görsel ve dosya ekleyebilirsin."/>
    <Fields draft={draft} set={set} people={people} me={me} editable withAssignee/>
    <div className="ot-form-actions">
      {error?<span className="ot-error" title={error}>{error}</span>:<span style={{flex:1}}/>}
      <button type="submit" className="p2-btn sm primary" disabled={!valid||busy}>{busy?'Oluşturuluyor…':'Oluştur'}</button>
    </div>
  </form>
}

// Satır içi ayrıntı/düzenleme paneli: Kaydet yalnız değişen alanları yollar; sütun değişimi "taşı" ucundan gider.
function TaskDetail({token,task,me,people,boardName,editable,shots,onBack,onMove,onUpdate}:{token:string;task:Task;me:string;people:Person[];boardName:string;editable:boolean;shots:RecentShot[];onBack:()=>void;onMove:(status:TaskStatus)=>Promise<void>;onUpdate:(patch:UpdateTaskInput)=>Promise<unknown>}){
  const[draft,setDraft]=useState<Draft>({title:task.title,description:withoutAttachments(task.description||''),status:task.status,priority:task.priority,kind:taskKind(task),labels:taskLabels(task).join(', '),assigneeId:task.assigneeId||'',due:dueInputValue(task.dueAt)})
  const attachments=useAttachments(token,attachmentsFromContent(task.description||''))
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  const set=(patch:Partial<Draft>)=>setDraft(current=>({...current,...patch}))
  const options=useMemo(()=>{
    const list=[...people].sort((a,b)=>a.displayName.localeCompare(b.displayName,'tr'))
    // Atanan kişi ekip listesinde yoksa (ayrılmış üye) seçenek olarak yine de gösterilir.
    if(task.assigneeId&&!people.some(person=>person.id===task.assigneeId))list.push({id:task.assigneeId,displayName:'Üye'})
    return list
  },[people,task.assigneeId])
  const dueIso=draft.due?dueFromInput(draft.due,task.dueAt):null
  const description=attachments.appendTo(draft.description)
  const labels=parseLabels(draft.labels)
  const patch:UpdateTaskInput={}
  if(draft.title.trim()!==task.title)patch.title=draft.title.trim()
  if(description!==(task.description||''))patch.description=description
  if(draft.priority!==task.priority)patch.priority=draft.priority
  if(draft.kind!==taskKind(task))patch.kind=draft.kind
  if(labels.join(',')!==taskLabels(task).join(','))patch.labels=labels
  if(draft.assigneeId!==(task.assigneeId||''))patch.assigneeId=draft.assigneeId
  if(dueInputValue(dueIso)!==dueInputValue(task.dueAt))patch.dueAt=dueIso
  const dirty=Object.keys(patch).length>0||draft.status!==task.status
  const valid=draft.title.trim().length>=2&&!attachments.busy
  async function submit(event:FormEvent){
    event.preventDefault()
    if(!dirty||!valid||busy||!editable)return
    setBusy(true);setError('')
    try{
      if(draft.status!==task.status)await onMove(draft.status)
      if(Object.keys(patch).length)await onUpdate(patch)
      pushOverlayToast({id:`task-saved-${task.id}`,tag:`task-saved-${task.id}`,kind:'success',title:'Görev güncellendi',text:draft.title.trim(),duration:3000})
      onBack()
    }catch(failure){setError(message(failure,'Görev kaydedilemedi.'))}
    finally{setBusy(false)}
  }
  return <form className="ot-detail" onSubmit={submit} aria-label={`Görev #${task.number}`}>
    <div className="ot-detail-head">
      <button type="button" className="p2-btn sm icon ghost" title="Listeye dön" aria-label="Listeye dön" onClick={onBack}><BackIcon/></button>
      <span className="ot-detail-id"><em>#{task.number}</em><KindBadge task={task}/><span className={`ot-status ${task.status}`}>{STATUS_LABEL[task.status]}</span><small title={boardName}>{boardName}</small></span>
      <button type="button" className="p2-btn sm secondary" title="Görevi ana pencerede aç" onClick={()=>openInMain(task)}><OpenIcon/>Uygulamada aç</button>
    </div>
    <input className="p2-input ot-detail-title" value={draft.title} maxLength={160} placeholder="Görev başlığı" aria-label="Görev başlığı" disabled={!editable} onChange={event=>set({title:event.target.value})} onKeyDown={event=>{if(event.key==='Escape')onBack()}}/>
    <DescriptionEditor token={token} value={draft.description} onChange={value=>set({description:value})} attachments={attachments} disabled={!editable} shots={shots} placeholder={editable?'Açıklama (isteğe bağlı)':'Açıklama yok'}/>
    <Fields draft={draft} set={set} people={options} me={me} editable={editable} withAssignee/>
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
  const instance=props.instance||''
  const winId=instance?instanceId('tasks',instance):'tasks'
  const open=useOpenTarget<OpenTaskTarget>(winId)
  const shots=useRecentShots()
  const ghost=!props.visible   // panel kapalı: sabitli hayalet, salt-okunur

  const[view,setView]=useState<View>(readView)
  const[onlyMine,setOnlyMine]=useState(false)
  const[boardId,setBoardId]=useState(instance||ALL_BOARDS)
  const[creating,setCreating]=useState(false)
  const[selectedId,setSelectedId]=useState('')
  const[flashId,setFlashId]=useState('')
  const[drag,setDrag]=useState<Drag|null>(null)
  const dragRef=useRef<{taskId:string;title:string;startX:number;startY:number;active:boolean;pointerId:number}|null>(null)
  const suppressClick=useRef(false)
  const columns=useRef<HTMLDivElement>(null)
  const changeView=(next:View)=>{setView(next);try{localStorage.setItem('p2:overlay:tasks:view',next)}catch{}}

  // Kişiler: ekip anlık görüntüsü + ben (atanan avatarları ve "Atanan" seçimi için).
  const people=useMemo(()=>{
    const map=new Map<string,Person>()
    const source=demo?demo.people:(snapshot?.people||[]) as Person[]
    for(const item of source)if(item?.id)map.set(item.id,item)
    const self=demo?demo.people.find(person=>person.id===demo.me)||{id:demo.me,displayName:'Ben'}:(auth.user as Person|null)
    if(self?.id)map.set(self.id,self)
    return map
  },[snapshot?.people,auth.user,demo])
  const peopleList=useMemo(()=>[...people.values()].sort((a,b)=>a.displayName.localeCompare(b.displayName,'tr')),[people])

  // Pano seçimi: örnek pencere panoya kilitlidir; temel pencerede "Tüm panolar" ya da seçilen pano (silinirse tümüne düşer).
  useEffect(()=>{
    if(instance){setBoardId(instance);return}
    setBoardId(current=>current===ALL_BOARDS||tasks.boards.some(board=>board.id===current)?current:ALL_BOARDS)
  },[tasks.boards,instance])
  const board=tasks.boards.find(item=>item.id===boardId)||null
  useEffect(()=>{
    const count=tasks.mine.length
    props.setTitle?.(instance?`Görevler · ${board?.name||'Pano'}`:`Görevler${count?` · ${count}`:''}`)
  },[tasks.mine.length,instance,board?.name])
  // Overlay görünür olunca pano yeniden çekilir (ana pencerede yapılan değişiklikler zaten canlı gelir; bu emniyet).
  useEffect(()=>{if(!props.visible||demo||!auth.ready)return;void tasks.refresh()},[props.visible,auth.ready,projectId])
  // Ana pencereden "görevi aç" hedefi (örn. "sana bir görev atadı" kartı): panoya geç, kartı vurgula.
  useEffect(()=>{
    if(!open?.target)return
    const target=open.target
    if(target.projectId&&projectId&&target.projectId!==projectId){clearOpenTarget(winId);return}
    if(target.boardId&&!instance)setBoardId(target.boardId)
    if(target.taskId){setFlashId(target.taskId);setSelectedId('');setOnlyMine(false)}
    clearOpenTarget(winId)
  },[open?.at])
  useEffect(()=>{if(!flashId)return;const timer=window.setTimeout(()=>setFlashId(''),4000);return()=>window.clearTimeout(timer)},[flashId])
  useEffect(()=>{if(ghost){setCreating(false);setSelectedId('');setDrag(null)}},[ghost])

  async function move(task:Task,status:TaskStatus){
    try{await tasks.move(task.id,status)}
    catch(failure){pushOverlayToast({id:`task-move-${task.id}`,tag:`task-move-${task.id}`,kind:'error',title:'Görev taşınamadı',text:message(failure,'Bu görevi taşıma yetkin yok.'),duration:5000});throw failure}
  }

  // ---- Sürükle-bırak (pointer): eşik aşılınca başlar, hangi sütunun üstündeyse oraya bırakılır. ----
  function dragStart(task:Task,event:ReactPointerEvent<HTMLButtonElement>){
    if(event.button!==0||ghost)return
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
  const openDetail=useCallback((taskId:string)=>{if(suppressClick.current)return;setCreating(false);setSelectedId(taskId)},[])

  if(!demo&&(!auth.ready||!token||!organizationId))return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>
  if(!projectId)return <div className="p2-empty"><b>Proje seçili değil</b><small>Görevler oyunun projesine göre listelenir.</small></div>

  const boardNames=Object.fromEntries(tasks.boards.map(item=>[item.id,item.name])) as Record<string,string>
  const scope=boardId===ALL_BOARDS?Object.values(tasks.tasksByBoard).flat():(tasks.tasksByBoard[boardId]||[])
  // Hayalet (panel kapalı) projenin TÜM görevlerini gösterir; "Benim" süzgeci yalnız panel açıkken.
  const filtered=onlyMine&&!ghost?scope.filter(task=>task.assigneeId===me&&task.status!=='DONE'):scope
  const all=Object.values(tasks.tasksByBoard).flat()
  const selected=selectedId?all.find(task=>task.id===selectedId)||null:null

  // Sabitli + küçültülmüş: tek satır özet (açık görev sayısı ve en öncelikli iş).
  if(props.pinned&&props.minimized){
    if(!tasks.mine.length)return null
    const first=tasks.mine[0].task
    return <div className="ot-pill" aria-label="Bana atanan görevler"><span className="p2-badge soft">{tasks.mine.length}</span><i className={`ot-dot ${first.priority}`}/><b>{first.title}</b></div>
  }
  // Örnek pencere: panosu silinmişse açıklayıcı boş durum.
  if(instance&&!tasks.loading&&!board)return <div className="p2-empty"><b>Pano bulunamadı</b><small>Bu pano silinmiş olabilir. Pencereyi kapatıp Görevler'den yeniden açabilirsin.</small><button type="button" className="p2-btn sm secondary" onClick={props.requestClose}>Kapat</button></div>

  // Yetkiler geç gelebilir: liste boşken (henüz yüklenmedi) izinli sayılır; sunucu zaten son sözü söyler.
  const permsLoaded=perms.owner||perms.permissions.length>0||perms.roles.length>0
  const canCreate=Boolean(demo)||!permsLoaded||perms.can('task.create')
  const canManage=Boolean(demo)||!permsLoaded||perms.can('board.manage')
  const createBoard=board||tasks.boards[0]||null
  const createHint=!canCreate?'Görev oluşturma yetkin yok':!createBoard?(tasks.loading?'Pano yükleniyor…':'Önce bir pano gerekir'):`${createBoard.name} panosuna yeni görev ekle`
  const showBoard=view==='board'&&!ghost
  const mineCount=scope.filter(task=>task.assigneeId===me&&task.status!=='DONE').length

  return <div className={`ot${drag?' dragging':''}${ghost?' ghost':''}`} aria-label="Görevler">
    {!ghost&&<div className="ot-head">
      {!instance&&<select className="p2-select ot-select ot-board-pick" value={boardId} aria-label="Pano" title="Pano" onChange={event=>setBoardId(event.target.value)}>
        <option value={ALL_BOARDS}>Tüm panolar</option>
        {tasks.boards.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}
      </select>}
      <div className="p2-tabs segment ot-view" role="tablist" aria-label="Görünüm">
        <button type="button" role="tab" className={`p2-tab${view==='list'?' active':''}`} aria-selected={view==='list'} title="Liste" onClick={()=>{changeView('list');setSelectedId('')}}><ListIcon/><span>Liste</span></button>
        <button type="button" role="tab" className={`p2-tab${view==='board'?' active':''}`} aria-selected={view==='board'} title="Pano (kanban)" onClick={()=>{changeView('board');setSelectedId('')}}><BoardIcon/><span>Pano</span></button>
      </div>
      <button type="button" className={`ot-filter${onlyMine?' on':''}`} aria-pressed={onlyMine} title={onlyMine?'Tüm görevleri göster':'Yalnız bana atananları göster'} onClick={()=>setOnlyMine(value=>!value)}>Benim{mineCount?<span className="p2-badge soft">{mineCount}</span>:null}</button>
      <span className="ot-grow"/>
      {!instance&&board&&<button type="button" className="p2-btn sm icon ghost" title={`${board.name} panosunu ayrı pencerede aç`} aria-label="Ayrı pencerede aç" onClick={()=>openInstance('tasks',board.id)}><WindowIcon/></button>}
      <button type="button" className="p2-btn sm primary ot-new" title={createHint} aria-label={`Yeni görev: ${createHint}`} disabled={!canCreate||!createBoard||creating} onClick={()=>{setSelectedId('');setCreating(true)}}><PlusIcon/>Yeni görev</button>
    </div>}
    {ghost&&<span className="ot-ghost-head"><b>{board?board.name:'Görevler'}</b><small>{filtered.filter(task=>task.status!=='DONE').length} açık · {filtered.filter(task=>task.assigneeId===me&&task.status!=='DONE').length} bana atanmış</small></span>}
    {!ghost&&!canCreate&&<div className="ot-muted ot-note">Görev oluşturma yetkin yok; görevleri görüntüleyip kendi görevlerini taşıyabilirsin.</div>}

    {creating&&createBoard&&createPortal(<div className="p2-modal-layer dim ot-modal-layer" onMouseDown={()=>setCreating(false)}><div className="ot-modal" role="dialog" aria-label="Yeni görev" onMouseDown={event=>event.stopPropagation()}><QuickCreate token={token} boardId={createBoard.id} boardName={createBoard.name} me={me} people={peopleList} shots={shots} onCreate={tasks.create} onClose={()=>setCreating(false)}/></div></div>,document.body)}

    {selected&&!ghost?<TaskDetail key={selected.id} token={token} task={selected} me={me} people={peopleList} boardName={boardNames[selected.boardId]||'Pano'} shots={shots}
      editable={canManage||selected.createdById===me||selected.assigneeId===me}
      onBack={()=>setSelectedId('')} onMove={status=>move(selected,status)} onUpdate={patch=>tasks.update(selected.id,patch)}/>
    :<>
      {tasks.error&&!tasks.boards.length&&<div className="p2-empty"><b>Pano yüklenemedi</b><small>{tasks.error}</small><button type="button" className="p2-btn sm secondary" onClick={()=>void tasks.refresh()}>Yeniden dene</button></div>}

      {!showBoard&&<div className="ot-body" role="list" aria-label="Görev listesi">
        {tasks.loading&&!filtered.length&&<div className="p2-empty"><small>Görevler yükleniyor…</small></div>}
        {!tasks.loading&&!filtered.length&&!tasks.error&&<div className="p2-empty"><b>{onlyMine&&!ghost?'Sana atanmış açık görev yok':'Görev yok'}</b><small>{ghost?'Panoya görev eklenince burada görünür.':onlyMine?'"Benim" süzgecini kaldırınca tüm görevler görünür.':'"Yeni görev" ile bir iş aç.'}</small></div>}
        {LIST_ORDER.map(status=>{
          const items=filtered.filter(task=>task.status===status)
          if(!items.length)return null
          return <section key={status} className="ot-group" aria-label={STATUS_LABEL[status]}>
            <header className="ot-group-head"><span className={`ot-status ${status}`}>{STATUS_LABEL[status]}</span><b>{items.length}</b></header>
            {items.map(task=><Row key={task.id} task={task} me={me} person={task.assigneeId?people.get(task.assigneeId):undefined} boardName={boardId===ALL_BOARDS?boardNames[task.boardId]:undefined} ghost={ghost} flash={flashId===task.id} onOpen={()=>openDetail(task.id)} onMove={next=>void move(task,next).catch(()=>{})}/>)}
          </section>
        })}
      </div>}

      {showBoard&&<>
        {tasks.loading&&!tasks.boards.length&&!tasks.error&&<div className="p2-empty"><small>Pano yükleniyor…</small></div>}
        {!tasks.loading&&!tasks.boards.length&&!tasks.error&&<div className="p2-empty"><b>Bu projede pano yok</b><small>Ana penceredeki Görevler sayfasından bir pano açınca burada görünür.</small><button type="button" className="p2-btn sm secondary" onClick={()=>void tasks.refresh()}>Yenile</button></div>}
        {tasks.boards.length>0&&<div ref={columns} className="ot-columns" aria-label={board?`${board.name} panosu`:'Tüm panolar'}>
          {BOARD_COLUMNS.map(status=>{
            const items=filtered.filter(task=>task.status===status)
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
