import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createNote, isNoteConflict, listNotes, removeNote, updateNote, type NoteChangedEvent, type NotePatch, type NoteVisibility, type ProjectNote } from '../api/notes'
import { useRealtimeEvents } from '../realtime'
import { applyNoteEvent, sortNotes } from '../notesModel'

// Proje notları: liste + iyimser düzenleme + 800 ms gecikmeli otomatik kayıt + canlı `note:changed`.
// Ana pencerede (Kütüphane sekmesi) ve overlay penceresinde (aynalanan olaylar) aynı hook çalışır.
// Çakışma (409 NOTE_CONFLICT): sunucudaki güncel not yüklenir, bekleyen yerel değişiklik atılır,
// `onConflict` çağrılır (çağıran toast ya da satır içi uyarı gösterir).

export const NOTE_AUTOSAVE_MS=800
export { applyNoteEvent, sortNotes }

export type UseProjectNotesInput={
  token:string
  organizationId:string
  projectId:string|null
  me:string
  enabled?:boolean
  onConflict?:(note:ProjectNote)=>void
  onError?:(message:string)=>void
}

type Pending={patch:NotePatch;timer:number|null;inflight:boolean}

export function useProjectNotes(input:UseProjectNotesInput){
  const{token,organizationId,projectId,me}=input
  const enabled=input.enabled!==false&&Boolean(token&&organizationId&&projectId)
  const[notes,setNotes]=useState<ProjectNote[]>([])
  const[loading,setLoading]=useState(true)
  const[error,setError]=useState('')
  const[savingIds,setSavingIds]=useState<Set<string>>(()=>new Set())
  const notesRef=useRef(notes);notesRef.current=notes
  const pending=useRef(new Map<string,Pending>())
  const callbacks=useRef({onConflict:input.onConflict,onError:input.onError});callbacks.current={onConflict:input.onConflict,onError:input.onError}
  const scope=useRef({token,organizationId,projectId});scope.current={token,organizationId,projectId}

  const fail=(problem:unknown,fallback:string)=>{
    const message=(problem as {message?:string})?.message||fallback
    setError(message);callbacks.current.onError?.(message)
  }

  // Yeniden yükleme bekleyen yerel yamaları ezmez (overlay gizlenip açılınca taslak korunur).
  const reload=useCallback(async()=>{
    if(!enabled||!projectId)return
    try{
      const items=await listNotes(token,organizationId,projectId)
      setNotes(sortNotes(items.map(item=>{const local=pending.current.get(item.id);return local?{...item,...local.patch}:item})));setError('')
    }catch(problem){fail(problem,'Notlar yüklenemedi.')}
    finally{setLoading(false)}
  },[enabled,token,organizationId,projectId])

  // Kapsam (kuruluş/proje) değişince liste sıfırlanır; jeton yenilenmesi ya da görünürlük değişimi
  // yalnız yeniden yükler. Eski kapsamın bekleyen yamaları atılır (başka proje).
  const scopeKey=`${organizationId}|${projectId}`
  useEffect(()=>{
    pending.current.forEach(item=>{if(item.timer)window.clearTimeout(item.timer)})
    pending.current.clear()
    setNotes([]);setLoading(true)
  },[scopeKey])
  useEffect(()=>{void reload()},[reload])

  // Bekleyen yamayı sunucuya yazar; yanıt gelene kadar aynı not için yeni yamalar birikir.
  const flush=useCallback(async(id:string)=>{
    const item=pending.current.get(id)
    const{token,organizationId,projectId}=scope.current
    if(!item||item.inflight||!projectId)return
    if(item.timer){window.clearTimeout(item.timer);item.timer=null}
    const current=notesRef.current.find(note=>note.id===id)
    if(!current){pending.current.delete(id);return}
    const patch=item.patch
    item.patch={};item.inflight=true
    setSavingIds(set=>new Set(set).add(id))
    try{
      const saved=await updateNote(token,organizationId,projectId,id,{...patch,version:current.version})
      const rest=pending.current.get(id)
      setNotes(list=>sortNotes(list.map(note=>note.id===id?{...saved,...(rest?.patch||{})}:note)))
      setError('')
    }catch(problem){
      if(isNoteConflict(problem)){
        pending.current.delete(id)
        setNotes(list=>sortNotes(list.map(note=>note.id===id?problem.note:note)))
        callbacks.current.onConflict?.(problem.note)
      }else{
        // Başarısız yama geri alınmaz; kullanıcı yazmaya devam edebilir, bir sonraki kayıt yeniden dener.
        const rest=pending.current.get(id)
        if(rest)rest.patch={...patch,...rest.patch}
        fail(problem,'Not kaydedilemedi.')
      }
    }finally{
      const rest=pending.current.get(id)
      if(rest){rest.inflight=false;if(Object.keys(rest.patch).length)rest.timer=window.setTimeout(()=>void flush(id),NOTE_AUTOSAVE_MS);else pending.current.delete(id)}
      setSavingIds(set=>{const next=new Set(set);next.delete(id);return next})
    }
  },[])

  // İyimser güncelle + gecikmeli kaydet. `immediate`: sabitle/görünürlük gibi tek tıklık işlemler.
  const save=useCallback((id:string,patch:NotePatch,options:{immediate?:boolean}={})=>{
    setNotes(list=>list.map(note=>note.id===id?{...note,...patch}:note))
    let item=pending.current.get(id)
    if(!item){item={patch:{},timer:null,inflight:false};pending.current.set(id,item)}
    item.patch={...item.patch,...patch}
    if(item.timer)window.clearTimeout(item.timer)
    item.timer=options.immediate?null:window.setTimeout(()=>void flush(id),NOTE_AUTOSAVE_MS)
    if(options.immediate)void flush(id)
  },[flush])

  const flushAll=useCallback(()=>{pending.current.forEach((item,id)=>{if(!item.inflight&&Object.keys(item.patch).length)void flush(id)})},[flush])
  useEffect(()=>()=>{flushAll()},[flushAll])

  const create=useCallback(async(init:NotePatch={})=>{
    if(!enabled||!projectId)return null
    try{
      const note=await createNote(token,organizationId,projectId,init)
      setNotes(list=>list.some(item=>item.id===note.id)?list:sortNotes([...list,note]))
      setError('')
      return note
    }catch(problem){fail(problem,'Not oluşturulamadı.');return null}
  },[enabled,token,organizationId,projectId])

  const remove=useCallback(async(id:string)=>{
    if(!enabled||!projectId)return false
    const item=pending.current.get(id)
    if(item?.timer)window.clearTimeout(item.timer)
    pending.current.delete(id)
    const before=notesRef.current
    setNotes(list=>list.filter(note=>note.id!==id))
    try{await removeNote(token,organizationId,projectId,id);return true}
    catch(problem){setNotes(before);fail(problem,'Not silinemedi.');return false}
  },[enabled,token,organizationId,projectId])

  const togglePin=useCallback((id:string)=>{
    const note=notesRef.current.find(item=>item.id===id)
    if(note)save(id,{pinned:!note.pinned},{immediate:true})
  },[save])
  const setVisibility=useCallback((id:string,visibility:NoteVisibility)=>save(id,{visibility},{immediate:true}),[save])

  useRealtimeEvents(enabled?token:'',organizationId,{
    'note:changed':(event:NoteChangedEvent)=>{
      if(!projectId||event.projectId!==projectId)return
      const editing=new Set([...pending.current.entries()].filter(([,item])=>item.inflight||Object.keys(item.patch).length).map(([id])=>id))
      setNotes(list=>applyNoteEvent(list,event,me,editing))
    },
  })

  const canEdit=useCallback((note:ProjectNote,canManage:boolean)=>note.authorId===me||(note.visibility==='TEAM'&&canManage),[me])

  return useMemo(()=>({notes,loading,error,savingIds,reload,create,save,flush,flushAll,remove,togglePin,setVisibility,canEdit}),[notes,loading,error,savingIds,reload,create,save,flush,flushAll,remove,togglePin,setVisibility,canEdit])
}
export type ProjectNotesApi=ReturnType<typeof useProjectNotes>
