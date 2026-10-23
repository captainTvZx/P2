import { request } from './client'

// Proje notları REST sarmalayıcısı: hem ana pencere (Kütüphane "Notlar" sekmesi) hem oyun içi
// overlay (Notlar penceresi) aynı uçları kullanır. Gövde mesaj biçimindedir (markdown + ek işaretleri).
export type NoteVisibility='PRIVATE'|'TEAM'
export type NoteAuthor={id:string;username?:string;displayName:string;avatarUrl?:string|null}
export type ProjectNote={
  id:string
  projectId:string
  authorId:string
  title:string
  body:string
  visibility:NoteVisibility
  pinned:boolean
  version:number
  createdAt:string
  updatedAt:string
  author:NoteAuthor
}
export type NotePatch={title?:string;body?:string;visibility?:NoteVisibility;pinned?:boolean}
export type NoteConflict={code:'NOTE_CONFLICT';message?:string;note:ProjectNote}
export type NoteChangedEvent={projectId:string;action:'created'|'updated'|'deleted';noteId:string;note?:ProjectNote;hiddenFor?:string}

export const notesRoot=(organizationId:string,projectId:string)=>`/organizations/${organizationId}/projects/${projectId}/notes`

export function listNotes(token:string,organizationId:string,projectId:string,options:{before?:string;limit?:number}={}){
  const query=new URLSearchParams()
  if(options.before)query.set('before',options.before)
  if(options.limit)query.set('limit',String(options.limit))
  const suffix=query.toString()
  return request<ProjectNote[]>(`${notesRoot(organizationId,projectId)}${suffix?`?${suffix}`:''}`,token)
}
export function createNote(token:string,organizationId:string,projectId:string,init:NotePatch={}){
  return request<ProjectNote>(notesRoot(organizationId,projectId),token,{method:'POST',body:JSON.stringify(init)})
}
// `version` iyimser kilit: sunucudaki ile aynı değilse 409 {code:'NOTE_CONFLICT', note} fırlatılır.
export function updateNote(token:string,organizationId:string,projectId:string,noteId:string,patch:NotePatch&{version?:number}){
  return request<ProjectNote>(`${notesRoot(organizationId,projectId)}/${noteId}`,token,{method:'PATCH',body:JSON.stringify(patch)})
}
export function removeNote(token:string,organizationId:string,projectId:string,noteId:string){
  return request<{ok:boolean}>(`${notesRoot(organizationId,projectId)}/${noteId}`,token,{method:'DELETE'})
}
export function isNoteConflict(error:unknown):error is NoteConflict{
  return Boolean(error&&typeof error==='object'&&(error as {code?:string}).code==='NOTE_CONFLICT'&&(error as {note?:unknown}).note)
}
