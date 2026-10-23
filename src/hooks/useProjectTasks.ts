import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createTask, getBoard, listBoards, moveTask, updateTask, type BoardInfo, type CreateTaskInput, type Task, type TaskStatus, type UpdateTaskInput } from '../api/tasks'
import { useRealtimeEvents } from '../realtime'
import { mergeMoved, mergeTasks, patchTask, removeTask, sortMine, upsertBoard } from '../taskRules'

// Bir projenin tüm panoları ve görevleri (overlay "Görevler" penceresi için asgari pano verisi).
// BoardPage'in tek-pano yükleyicisinin küçültülmüş hali: tüm panolar çekilir, canlı olaylar
// (task:updated|deleted|moved, board:updated|deleted) projeye göre süzülüp yerinde uygulanır.
// `demo` verilirse ağ yok: liste sabit veriden kurulur, oluşturma/taşıma/düzenleme yerelde kalır (showroom).
export type ProjectTasksDemo={boards:BoardInfo[];tasks:Task[]}
export type UseProjectTasksInput={
  token:string
  organizationId:string
  projectId:string|null
  me:string
  demo?:ProjectTasksDemo
}
export type MineRow={task:Task;boardName:string}

const groupByBoard=(tasks:Task[])=>{
  const out:Record<string,Task[]>={}
  for(const task of tasks)(out[task.boardId]||=(out[task.boardId]=[])).push(task)
  return out
}

export function useProjectTasks(input:UseProjectTasksInput){
  const{token,organizationId,projectId,me,demo}=input
  const[boards,setBoards]=useState<BoardInfo[]>(()=>demo?.boards||[])
  const[tasksByBoard,setTasksByBoard]=useState<Record<string,Task[]>>(()=>demo?groupByBoard(demo.tasks):{})
  const[loading,setLoading]=useState(!demo)
  const[error,setError]=useState('')
  // Proje değişince eski isteklerin yanıtı düşürülür.
  const generation=useRef(0)
  const ready=Boolean(demo)||Boolean(token&&organizationId&&projectId)

  const loadBoard=useCallback(async(boardId:string,gen:number)=>{
    if(demo||!token||!organizationId||!projectId)return
    try{
      const content=await getBoard(token,organizationId,projectId,boardId)
      if(gen!==generation.current)return
      setTasksByBoard(current=>({...current,[boardId]:content.tasks||[]}))
    }catch{}
  },[token,organizationId,projectId,demo])

  const refresh=useCallback(async()=>{
    if(demo){setLoading(false);return}
    if(!token||!organizationId||!projectId){setBoards([]);setTasksByBoard({});setLoading(false);return}
    const gen=++generation.current
    try{
      const list=await listBoards(token,organizationId,projectId)
      if(gen!==generation.current)return
      const contents=await Promise.all(list.map(async board=>{try{return[board.id,(await getBoard(token,organizationId,projectId,board.id)).tasks||[]] as const}catch{return[board.id,[]] as const}}))
      if(gen!==generation.current)return
      setBoards(list)
      setTasksByBoard(Object.fromEntries(contents))
      setError('')
    }catch(failure){
      if(gen!==generation.current)return
      setError((failure as {message?:string})?.message||'Panolar yüklenemedi.')
    }finally{
      if(gen===generation.current)setLoading(false)
    }
  },[token,organizationId,projectId,demo])
  useEffect(()=>{if(!demo)setLoading(true);void refresh()},[refresh])

  const sameProject=(value:string|undefined|null)=>Boolean(projectId)&&(!value||value===projectId)
  useRealtimeEvents(demo?'':token,organizationId,{
    'board:updated':(payload:{board:BoardInfo})=>{
      const board=payload?.board
      if(!board||board.projectId!==projectId)return
      setBoards(current=>upsertBoard(current,board))
      // Yeni açılan panonun görevleri henüz yok: bir kez çekilir.
      setTasksByBoard(current=>{if(current[board.id])return current;void loadBoard(board.id,generation.current);return{...current,[board.id]:[]}})
    },
    'board:deleted':(payload:{boardId:string;projectId:string})=>{
      if(!sameProject(payload?.projectId))return
      setBoards(current=>current.filter(board=>board.id!==payload.boardId))
      setTasksByBoard(current=>{if(!(payload.boardId in current))return current;const next={...current};delete next[payload.boardId];return next})
    },
    'task:updated':(payload:{task:Task})=>{
      const task=payload?.task
      if(!task||!sameProject(task.projectId)||!task.boardId)return
      setTasksByBoard(current=>({...current,[task.boardId]:mergeTasks(current[task.boardId]||[],task)}))
    },
    'task:deleted':(payload:{taskId:string;projectId?:string;boardId?:string})=>{
      if(!sameProject(payload?.projectId))return
      setTasksByBoard(current=>{
        const keys=payload.boardId?[payload.boardId]:Object.keys(current)
        let changed=false
        const next={...current}
        for(const key of keys){const list=current[key];if(!list)continue;const trimmed=removeTask(list,payload.taskId);if(trimmed.length!==list.length){next[key]=trimmed;changed=true}}
        return changed?next:current
      })
    },
    'task:moved':(payload:{tasks:Task[];projectId?:string;boardId?:string})=>{
      if(!sameProject(payload?.projectId)||!Array.isArray(payload.tasks)||!payload.tasks.length)return
      const boardId=payload.boardId||payload.tasks[0].boardId
      setTasksByBoard(current=>current[boardId]?{...current,[boardId]:mergeMoved(current[boardId],payload.tasks)}:current)
    },
  })

  const boardName=useMemo(()=>Object.fromEntries(boards.map(board=>[board.id,board.name])) as Record<string,string>,[boards])
  const all=useMemo(()=>Object.values(tasksByBoard).flat(),[tasksByBoard])
  const mine=useMemo<MineRow[]>(()=>sortMine(all,me).map(task=>({task,boardName:boardName[task.boardId]||'Pano'})),[all,me,boardName])
  const allRef=useRef(all);allRef.current=all

  const create=useCallback(async(boardId:string,draft:CreateTaskInput)=>{
    if(!ready)throw{code:'NO_SESSION',message:'Oturum hazır değil.'}
    let task:Task
    if(demo){
      const now=new Date().toISOString()
      const number=allRef.current.reduce((max,item)=>Math.max(max,item.number),0)+1
      task={id:`demo-task-${number}`,number,title:draft.title,description:draft.description||null,status:draft.status||'TODO',priority:draft.priority||'MEDIUM',points:draft.points??null,position:-number,assigneeId:draft.assigneeId||null,createdById:me,sprintId:draft.sprintId||null,projectId:projectId||'demo',boardId,dueAt:draft.dueAt||null,createdAt:now,updatedAt:now}
    }else task=await createTask(token,organizationId,projectId as string,boardId,draft)
    setTasksByBoard(current=>({...current,[boardId]:mergeTasks(current[boardId]||[],task)}))
    return task
  },[ready,demo,token,organizationId,projectId,me])

  // İyimser taşıma: sütun hemen değişir, sunucu reddederse geri alınır.
  const move=useCallback(async(taskId:string,status:TaskStatus)=>{
    if(!ready)throw{code:'NO_SESSION',message:'Oturum hazır değil.'}
    const task=allRef.current.find(item=>item.id===taskId)
    if(!task||task.status===status)return
    const previous=task
    setTasksByBoard(current=>({...current,[task.boardId]:mergeTasks(current[task.boardId]||[],{...task,status,updatedAt:new Date().toISOString()})}))
    if(demo)return
    try{
      const result=await moveTask(token,organizationId,projectId as string,task.boardId,taskId,status)
      if(result?.tasks?.length)setTasksByBoard(current=>({...current,[task.boardId]:mergeMoved(current[task.boardId]||[],result.tasks)}))
    }catch(failure){
      setTasksByBoard(current=>({...current,[task.boardId]:mergeTasks(current[task.boardId]||[],previous)}))
      throw failure
    }
  },[ready,demo,token,organizationId,projectId])

  // İyimser düzenleme (başlık, açıklama, öncelik, atanan, teslim, sütun): sunucu reddederse geri alınır.
  const update=useCallback(async(taskId:string,patch:UpdateTaskInput)=>{
    if(!ready)throw{code:'NO_SESSION',message:'Oturum hazır değil.'}
    const task=allRef.current.find(item=>item.id===taskId)
    if(!task)throw{code:'TASK_NOT_FOUND',message:'Görev bulunamadı.'}
    const previous=task
    setTasksByBoard(current=>({...current,[task.boardId]:patchTask(current[task.boardId]||[],taskId,{...patch,dueAt:patch.dueAt===undefined?undefined:patch.dueAt})}))
    if(demo)return
    try{
      const saved=await updateTask(token,organizationId,projectId as string,task.boardId,taskId,patch)
      setTasksByBoard(current=>({...current,[task.boardId]:mergeTasks(current[task.boardId]||[],saved)}))
      return saved
    }catch(failure){
      setTasksByBoard(current=>({...current,[task.boardId]:mergeTasks(current[task.boardId]||[],previous)}))
      throw failure
    }
  },[ready,demo,token,organizationId,projectId])

  return{boards,tasksByBoard,mine,loading,error,refresh,create,move,update}
}
export type ProjectTasksApi=ReturnType<typeof useProjectTasks>
