import { request } from './client'

// Görev panosu API'si (server/src/tasks.ts ile birebir şekiller). Overlay "Görevler" penceresi
// ve ileride BoardPage dışındaki hafif görünümler bunu kullanır; sunucu tarafında değişiklik yok.
export const TASK_STATUSES=['BACKLOG','TODO','IN_PROGRESS','REVIEW','DONE'] as const
export const TASK_PRIORITIES=['LOW','MEDIUM','HIGH','URGENT'] as const
export type TaskStatus=typeof TASK_STATUSES[number]
export type TaskPriority=typeof TASK_PRIORITIES[number]

// Pano sütunları (BoardPage ile aynı dört sütun; BACKLOG sprint birikimidir, sütun olarak çizilmez).
export const BOARD_COLUMNS:TaskStatus[]=['TODO','IN_PROGRESS','REVIEW','DONE']
export const STATUS_LABEL:Record<TaskStatus,string>={BACKLOG:'Birikim',TODO:'Yapılacak',IN_PROGRESS:'Sürüyor',REVIEW:'İncelemede',DONE:'Bitti'}
export const PRIORITY_LABEL:Record<TaskPriority,string>={LOW:'Düşük',MEDIUM:'Normal',HIGH:'Yüksek',URGENT:'Acil'}

// Görev türü (hata, özellik…) ve serbest etiketler. Sunucu etiketleri virgülle ayrılmış tek dizge saklar;
// istemci `taskLabels()` ile diziye çevirir, gönderirken dizi yollar.
export const TASK_KINDS=['TASK','BUG','FEATURE','IMPROVEMENT','DOCS'] as const
export type TaskKind=typeof TASK_KINDS[number]
export const KIND_LABEL:Record<TaskKind,string>={TASK:'Görev',BUG:'Hata',FEATURE:'Özellik',IMPROVEMENT:'İyileştirme',DOCS:'Belge'}
export const KIND_SHORT:Record<TaskKind,string>={TASK:'Görev',BUG:'Hata',FEATURE:'Özellik',IMPROVEMENT:'İyileştir',DOCS:'Belge'}
export const taskKind=(task:Pick<Task,'kind'>|{kind?:string|null}):TaskKind=>(TASK_KINDS as readonly string[]).includes(String(task.kind||''))?task.kind as TaskKind:'TASK'
export const taskLabels=(task:{labels?:string|null}):string[]=>String(task.labels||'').split(',').map(item=>item.trim()).filter(Boolean)
export const parseLabels=(raw:string):string[]=>{const out:string[]=[];const seen=new Set<string>();for(const part of raw.split(/[,\n]/)){const label=part.trim().replace(/\s+/g,' ').slice(0,24);const key=label.toLocaleLowerCase('tr-TR');if(label&&!seen.has(key)){seen.add(key);out.push(label);if(out.length>=10)break}}return out}

export type BoardInfo={id:string;name:string;position:number;projectId:string;createdById?:string;createdAt?:string}
export type Sprint={id:string;name:string;goal:string|null;status:'PLANNED'|'ACTIVE'|'DONE';startsAt:string|null;endsAt:string|null;projectId?:string;boardId?:string}
export type Task={
  id:string
  number:number
  title:string
  description:string|null
  status:TaskStatus
  priority:TaskPriority
  kind?:TaskKind|string       // eski sunucu göndermeyebilir → 'TASK' sayılır
  labels?:string              // 'ui,ses' (virgülle ayrılmış)
  points:number|null
  position:number
  assigneeId:string|null
  createdById:string
  sprintId:string|null
  projectId:string
  boardId:string
  dueAt:string|null
  createdAt:string
  updatedAt:string
}
export type BoardContent={board:BoardInfo;sprints:Sprint[];tasks:Task[]}
export type CreateTaskInput={
  title:string                 // 2–160 karakter
  description?:string          // mesaj biçimi + ek işaretleri, ≤12000
  status?:TaskStatus
  priority?:TaskPriority
  kind?:TaskKind
  labels?:string[]
  assigneeId?:string
  sprintId?:string
  points?:number
  dueAt?:string                // ISO 8601
}

// PATCH .../tasks/:taskId: oluşturan ya da atanan düzenler (başkasınınki için board.manage); boş dizge
// assigneeId → "kimseye atanmadı", dueAt null → teslim tarihi kaldırılır. Sunucu `task:updated` yayınlar.
export type UpdateTaskInput={
  title?:string
  description?:string
  status?:TaskStatus
  priority?:TaskPriority
  kind?:TaskKind
  labels?:string[]
  assigneeId?:string           // '' = kimseye atanmadı
  sprintId?:string
  points?:number
  dueAt?:string|null
}

export function boardsRoot(organizationId:string,projectId:string){
  return `/organizations/${encodeURIComponent(organizationId)}/projects/${encodeURIComponent(projectId)}/boards`
}

// Projedeki panolar; hiç yoksa sunucu "Genel" panosunu açıp onu döndürür.
export function listBoards(token:string,organizationId:string,projectId:string){
  return request<BoardInfo[]>(boardsRoot(organizationId,projectId),token)
}
export function getBoard(token:string,organizationId:string,projectId:string,boardId:string){
  return request<BoardContent>(`${boardsRoot(organizationId,projectId)}/${encodeURIComponent(boardId)}`,token)
}
// `task.create` yetkisi gerekir; sunucu `task:updated` olayı yayınlar.
export function createTask(token:string,organizationId:string,projectId:string,boardId:string,input:CreateTaskInput){
  return request<Task>(`${boardsRoot(organizationId,projectId)}/${encodeURIComponent(boardId)}/tasks`,token,{method:'POST',body:JSON.stringify(input)})
}
export function updateTask(token:string,organizationId:string,projectId:string,boardId:string,taskId:string,patch:UpdateTaskInput){
  return request<Task>(`${boardsRoot(organizationId,projectId)}/${encodeURIComponent(boardId)}/tasks/${encodeURIComponent(taskId)}`,token,{method:'PATCH',body:JSON.stringify(patch)})
}
// Kendi görevini (oluşturan ya da atanan) herkes taşır, başkasınınki için `board.manage`; `task:moved` yayınlanır.
export function moveTask(token:string,organizationId:string,projectId:string,boardId:string,taskId:string,status:TaskStatus){
  return request<{ok:boolean;tasks:Task[]}>(`${boardsRoot(organizationId,projectId)}/${encodeURIComponent(boardId)}/tasks/${encodeURIComponent(taskId)}/move`,token,{method:'PATCH',body:JSON.stringify({status})})
}
