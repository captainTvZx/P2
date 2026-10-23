import { expect, test } from 'vitest'
import type { BoardInfo, Task } from './api/tasks'
import { dueFromInput, dueInputValue, mergeMoved, mergeTasks, patchTask, removeTask, sortMine, upsertBoard } from './taskRules'

// useProjectTasks'ın saf kuralları: canlı olay birleştirme ve "bana atananlar" sıralaması.
const task=(patch:Partial<Task>):Task=>({
  id:'t1',number:1,title:'Görev',description:null,status:'TODO',priority:'MEDIUM',points:null,position:0,
  assigneeId:'me',createdById:'u1',sprintId:null,projectId:'p1',boardId:'b1',dueAt:null,
  createdAt:'2026-10-20T10:00:00.000Z',updatedAt:'2026-10-20T10:00:00.000Z',...patch,
})

test('mergeTasks: var olan görev yerine konur, yeni görev sona eklenir', () => {
  const list=[task({id:'a',title:'A'}),task({id:'b',title:'B'})]
  expect(mergeTasks(list,task({id:'a',title:'A2'})).map(item=>item.title)).toEqual(['A2','B'])
  expect(mergeTasks(list,task({id:'c',title:'C'})).map(item=>item.id)).toEqual(['a','b','c'])
})

test('mergeMoved: yalnız listedeki görevler yenilenir, yabancı görev eklenmez', () => {
  const list=[task({id:'a',status:'TODO'}),task({id:'b',status:'TODO'})]
  const next=mergeMoved(list,[task({id:'b',status:'DONE'}),task({id:'z',boardId:'b9'})])
  expect(next.map(item=>`${item.id}:${item.status}`)).toEqual(['a:TODO','b:DONE'])
})

test('removeTask: olmayan kimlik listeyi değiştirmez', () => {
  const list=[task({id:'a'}),task({id:'b'})]
  expect(removeTask(list,'b').map(item=>item.id)).toEqual(['a'])
  expect(removeTask(list,'yok')).toHaveLength(2)
})

test('upsertBoard: konuma göre sıralı tutar, aynı panoyu günceller', () => {
  const boards:BoardInfo[]=[{id:'b1',name:'Genel',position:0,projectId:'p1'},{id:'b3',name:'Sanat',position:2,projectId:'p1'}]
  const next=upsertBoard(boards,{id:'b2',name:'Kod',position:1,projectId:'p1'})
  expect(next.map(board=>board.name)).toEqual(['Genel','Kod','Sanat'])
  expect(upsertBoard(next,{id:'b2',name:'Kod 2',position:1,projectId:'p1'}).map(board=>board.name)).toEqual(['Genel','Kod 2','Sanat'])
})

test('sortMine: bana atanmış ve bitmemiş görevler; önce öncelik, sonra son güncellenen', () => {
  const list=[
    task({id:'low',priority:'LOW',updatedAt:'2026-10-22T10:00:00.000Z'}),
    task({id:'done',priority:'URGENT',status:'DONE'}),
    task({id:'other',priority:'URGENT',assigneeId:'u2'}),
    task({id:'none',priority:'URGENT',assigneeId:null}),
    task({id:'high-old',priority:'HIGH',updatedAt:'2026-10-19T10:00:00.000Z'}),
    task({id:'high-new',priority:'HIGH',updatedAt:'2026-10-21T10:00:00.000Z'}),
    task({id:'urgent',priority:'URGENT',status:'BACKLOG'}),
    task({id:'medium',priority:'MEDIUM',status:'REVIEW'}),
  ]
  expect(sortMine(list,'me').map(item=>item.id)).toEqual(['urgent','high-new','high-old','medium','low'])
  expect(sortMine(list,'')).toEqual([])
})

test('patchTask: yalnız verilen alanlar değişir; boş atanan ve açıklama null olur', () => {
  const list=[task({id:'a',title:'A',description:'eski',assigneeId:'me'}),task({id:'b'})]
  const next=patchTask(list,'a',{title:'  Yeni  ',description:'',assigneeId:'',priority:'HIGH',dueAt:null})
  expect(next[0]).toMatchObject({title:'Yeni',description:null,assigneeId:null,priority:'HIGH',dueAt:null,status:'TODO'})
  expect(next[1]).toBe(list[1])
})

test('dueInputValue / dueFromInput: yerel gün gidip gelir, saat korunur', () => {
  const iso=new Date(2026,9,28,14,30).toISOString()
  expect(dueInputValue(iso)).toBe('2026-10-28')
  expect(dueInputValue(null)).toBe('')
  const moved=dueFromInput('2026-11-02',iso) as string
  expect(new Date(moved).getHours()).toBe(14)
  expect(dueInputValue(moved)).toBe('2026-11-02')
  expect(new Date(dueFromInput('2026-11-02',null) as string).getHours()).toBe(17)
  expect(dueFromInput('',iso)).toBeNull()
})
