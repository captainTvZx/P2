import { describe, expect, it } from 'vitest'
import { applyNoteEvent, sortNotes } from './notesModel'
import type { ProjectNote } from './api/notes'

const author={id:'a',displayName:'Ayşe'}
function note(over:Partial<ProjectNote>):ProjectNote{
  return {id:'n1',projectId:'p',authorId:'a',title:'',body:'',visibility:'PRIVATE',pinned:false,version:1,createdAt:'2026-10-23T10:00:00.000Z',updatedAt:'2026-10-23T10:00:00.000Z',author,...over}
}

describe('sortNotes',()=>{
  it('sabitliler önce, sonra en son güncellenen',()=>{
    const list=[note({id:'old',updatedAt:'2026-10-20T00:00:00.000Z'}),note({id:'new',updatedAt:'2026-10-23T00:00:00.000Z'}),note({id:'pin',pinned:true,updatedAt:'2026-10-01T00:00:00.000Z'})]
    expect(sortNotes(list).map(item=>item.id)).toEqual(['pin','new','old'])
  })
})

describe('applyNoteEvent',()=>{
  const base=[note({id:'n1',version:2}),note({id:'n2'})]
  it('created: yeni notu ekler, aynı not iki kez eklenmez',()=>{
    const added=applyNoteEvent(base,{projectId:'p',action:'created',noteId:'n3',note:note({id:'n3'})},'a')
    expect(added).toHaveLength(3)
    expect(applyNoteEvent(added,{projectId:'p',action:'created',noteId:'n3',note:note({id:'n3'})},'a')).toHaveLength(3)
  })
  it('updated: daha yeni sürüm yazılır, eski sürüm yok sayılır',()=>{
    const newer=applyNoteEvent(base,{projectId:'p',action:'updated',noteId:'n1',note:note({id:'n1',version:3,title:'Yeni'})},'a')
    expect(newer.find(item=>item.id==='n1')?.title).toBe('Yeni')
    const stale=applyNoteEvent(newer,{projectId:'p',action:'updated',noteId:'n1',note:note({id:'n1',version:1,title:'Eski'})},'a')
    expect(stale.find(item=>item.id==='n1')?.title).toBe('Yeni')
  })
  it('updated: yerel taslağı olan not üzerine yazılmaz',()=>{
    const result=applyNoteEvent(base,{projectId:'p',action:'updated',noteId:'n1',note:note({id:'n1',version:9,title:'Uzak'})},'a',new Set(['n1']))
    expect(result.find(item=>item.id==='n1')?.version).toBe(2)
  })
  it('deleted: notu kaldırır; hiddenFor benimse (TEAM→PRIVATE geçişi) dokunmaz',()=>{
    expect(applyNoteEvent(base,{projectId:'p',action:'deleted',noteId:'n2'},'a')).toHaveLength(1)
    expect(applyNoteEvent(base,{projectId:'p',action:'deleted',noteId:'n2',hiddenFor:'a'},'a')).toHaveLength(2)
    expect(applyNoteEvent(base,{projectId:'p',action:'deleted',noteId:'n2',hiddenFor:'b'},'a')).toHaveLength(1)
  })
})
