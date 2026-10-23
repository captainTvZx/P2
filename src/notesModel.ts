import type { NoteChangedEvent, ProjectNote } from './api/notes'

// Proje notlarının saf liste kuralları (tarayıcı/DOM bağımlılığı yok; vitest node ortamında koşar).
// useProjectNotes bunları kullanır.

export function sortNotes(notes:ProjectNote[]){
  return [...notes].sort((a,b)=>Number(b.pinned)-Number(a.pinned)||new Date(b.updatedAt).getTime()-new Date(a.updatedAt).getTime())
}

// Canlı olayı listeye uygular. `editing`: yerel bekleyen değişikliği olan notlar (kendi kaydımızın
// yankısı ya da başkasının güncellemesi gelirse yerel taslak korunur; sonraki kayıt 409 ile çözülür).
// `hiddenFor`: TEAM→PRIVATE geçişinde ekibe giden "silindi" olayı yazara uygulanmaz.
export function applyNoteEvent(notes:ProjectNote[],event:NoteChangedEvent,me:string,editing:Set<string>=new Set()){
  if(event.action==='deleted'){
    if(event.hiddenFor&&event.hiddenFor===me)return notes
    return notes.filter(note=>note.id!==event.noteId)
  }
  const incoming=event.note
  if(!incoming)return notes
  const index=notes.findIndex(note=>note.id===incoming.id)
  if(index<0)return sortNotes([...notes,incoming])
  if(editing.has(incoming.id))return notes
  if(incoming.version<notes[index].version)return notes
  const next=[...notes];next[index]=incoming
  return sortNotes(next)
}
