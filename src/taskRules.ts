import type { BoardInfo, Task, TaskPriority } from './api/tasks'

// Görev listesi kuralları: saf fonksiyonlar (pencere/API yok), useProjectTasks ve spec'i kullanır.

// Öncelik sırası: acil en üstte.
export const PRIORITY_RANK:Record<TaskPriority,number>={URGENT:0,HIGH:1,MEDIUM:2,LOW:3}
const rankOf=(priority:string)=>PRIORITY_RANK[priority as TaskPriority]??PRIORITY_RANK.MEDIUM

// Canlı `task:updated` / oluşturma yanıtı: varsa yerine koy, yoksa sona ekle (sunucu konum sırasını korur).
export function mergeTasks(list:Task[],task:Task):Task[]{
  return list.some(item=>item.id===task.id)?list.map(item=>item.id===task.id?task:item):[...list,task]
}
// `task:moved`: yalnız yükte gelen görevler yenilenir, listede olmayanlar eklenmez (başka panonun görevi olabilir).
export function mergeMoved(list:Task[],moved:Task[]):Task[]{
  const map=new Map(moved.map(task=>[task.id,task]))
  return list.map(item=>map.get(item.id)||item)
}
export function removeTask(list:Task[],taskId:string):Task[]{
  return list.filter(item=>item.id!==taskId)
}
// `board:updated`: konuma göre sıralı tutulur.
export function upsertBoard(boards:BoardInfo[],board:BoardInfo):BoardInfo[]{
  const next=boards.some(item=>item.id===board.id)?boards.map(item=>item.id===board.id?board:item):[...boards,board]
  return next.sort((a,b)=>a.position-b.position||String(a.createdAt||'').localeCompare(String(b.createdAt||'')))
}

// "Bana atananlar": bana atanmış ve bitmemiş görevler; önce öncelik (acil → düşük), sonra en son güncellenen.
export function sortMine(tasks:Task[],me:string):Task[]{
  if(!me)return[]
  return tasks
    .filter(task=>task.assigneeId===me&&task.status!=='DONE')
    .sort((a,b)=>rankOf(a.priority)-rankOf(b.priority)||Date.parse(b.updatedAt||'')-Date.parse(a.updatedAt||'')||a.number-b.number)
}

// Düzenleme yaması → görev alanları (iyimser güncelleme). Boş atanan/açıklama sunucudaki gibi null olur.
export type TaskPatch={title?:string;description?:string;status?:Task['status'];priority?:TaskPriority;kind?:Task['kind'];labels?:string[];assigneeId?:string;dueAt?:string|null}
export function patchTask(list:Task[],taskId:string,patch:TaskPatch):Task[]{
  return list.map(item=>{
    if(item.id!==taskId)return item
    const next:Task={...item}
    if(patch.title!==undefined)next.title=patch.title.trim()
    if(patch.description!==undefined)next.description=patch.description.trim()||null
    if(patch.status!==undefined)next.status=patch.status
    if(patch.priority!==undefined)next.priority=patch.priority
    if(patch.kind!==undefined)next.kind=patch.kind
    if(patch.labels!==undefined)next.labels=patch.labels.join(',')
    if(patch.assigneeId!==undefined)next.assigneeId=patch.assigneeId||null
    if(patch.dueAt!==undefined)next.dueAt=patch.dueAt
    return next
  })
}

// Teslim tarihi ↔ <input type="date"> (yerel gün). Saat korunur; yeni tarihte 17:00 kullanılır.
export function dueInputValue(iso:string|null):string{
  if(!iso)return ''
  const date=new Date(iso)
  if(Number.isNaN(date.getTime()))return ''
  const pad=(value:number)=>String(value).padStart(2,'0')
  return `${date.getFullYear()}-${pad(date.getMonth()+1)}-${pad(date.getDate())}`
}
export function dueFromInput(value:string,previous:string|null):string|null{
  const match=/^(\d{4})-(\d{2})-(\d{2})$/.exec(value.trim())
  if(!match)return null
  const before=previous?new Date(previous):null
  const keepTime=before&&!Number.isNaN(before.getTime())
  const date=new Date(Number(match[1]),Number(match[2])-1,Number(match[3]),keepTime?before.getHours():17,keepTime?before.getMinutes():0,0,0)
  return date.toISOString()
}
