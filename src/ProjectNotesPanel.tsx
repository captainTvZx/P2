import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import MessageRichText from './MessageRichText'
import NotesEditor from './NotesEditor'
import { attachmentsFromContent } from './attachments'
import { useProjectNotes, type ProjectNotesApi } from './hooks/useProjectNotes'
import type { NoteVisibility, ProjectNote } from './api/notes'
import './project-notes.css'

// Proje notları paneli: solda liste (sabitliler önce), sağda başlık + editör.
// Kütüphane "Notlar" sekmesi (geniş; zengin NotesEditor) ve overlay Notlar penceresi (dar, üst üste düzen;
// `compact` → hafif markdown editörü: biçim çubuğu + büyük yazı alanı) aynı bileşen.
// Kayıt otomatik (800 ms); çakışmada sunucudaki sürüm yüklenir ve satır içi uyarı gösterilir.

export const NOTE_CONFLICT_TEXT='Not başka bir cihazda değişti, yeniden yüklendi'
const NARROW_WIDTH=520

export function noteTimeAgo(value:string|number){
  const diff=(Date.now()-new Date(value).getTime())/1000
  if(diff<60)return 'az önce'
  if(diff<3600)return `${Math.max(1,Math.floor(diff/60))} dk önce`
  if(diff<86400)return `${Math.floor(diff/3600)} sa önce`
  if(diff<172800)return 'dün'
  return new Date(value).toLocaleDateString('tr-TR',{day:'numeric',month:'short'})
}
export const noteTitle=(note:Pick<ProjectNote,'title'>)=>note.title.trim()||'Adsız not'

const PinIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8.5 3.5h7l-1 6.5 3.5 3.5v2h-6v5.5l-.5 1-.5-1V15.5h-6v-2l3.5-3.5z"/></svg>
const TrashIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9.5 7V4h5v3M6.5 7l1 13.5h9l1-13.5M10 11v6M14 11v6"/></svg>
const BackIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19 12H5M11 6l-6 6 6 6"/></svg>
const PlusIcon=()=><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>
const FMT_ICONS={
  bold:<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5h6a4 4 0 0 1 0 8H7V5Z"/><path d="M7 13h7a4 4 0 0 1 0 8H7v-8Z"/></svg>,
  italic:<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 5h8M6 19h8M14 5 10 19"/></svg>,
  list:<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6h11M9 12h11M9 18h11"/><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01"/></svg>,
  link:<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10 13.2a4.6 4.6 0 0 0 6.94.5l2.6-2.6a4.6 4.6 0 0 0-6.5-6.5l-1.5 1.48"/><path d="M14 10.8a4.6 4.6 0 0 0-6.94-.5l-2.6 2.6a4.6 4.6 0 0 0 6.5 6.5l1.48-1.48"/></svg>,
}

export function VisibilityBadge({visibility}:{visibility:NoteVisibility}){
  return visibility==='TEAM'?<span className="p2-badge soft">Ekip</span>:<span className="p2-badge muted">Özel</span>
}

// ---- Kompakt markdown editörü (overlay): metin alanı + Kalın / İtalik / Liste / Bağlantı yardımcıları ----
// Gövde mesaj biçimindedir (markdown + ek işaretleri); ek satırları metinden ayrılır, kayıtta geri eklenir.
type FmtKind='bold'|'italic'|'list'|'link'
const FMT:{id:FmtKind;label:string;title:string}[]=[
  {id:'bold',label:'Kalın',title:'Kalın (Ctrl+B)'},
  {id:'italic',label:'İtalik',title:'İtalik (Ctrl+I)'},
  {id:'list',label:'Liste',title:'Madde listesi (Ctrl+L)'},
  {id:'link',label:'Bağlantı',title:'Bağlantı ekle (Ctrl+K)'},
]
function splitBody(body:string){
  const text:string[]=[],markers:string[]=[]
  for(const line of body.split(/\r?\n/))(attachmentsFromContent(line).length?markers:text).push(line)
  return{text:text.join('\n'),markers}
}
export function applyFormat(text:string,start:number,end:number,kind:FmtKind):{text:string;start:number;end:number}{
  const before=text.slice(0,start),selected=text.slice(start,end),after=text.slice(end)
  if(kind==='bold'||kind==='italic'){
    const mark=kind==='bold'?'**':'*'
    if(selected.startsWith(mark)&&selected.endsWith(mark)&&selected.length>=mark.length*2){
      const inner=selected.slice(mark.length,-mark.length)
      return{text:before+inner+after,start,end:start+inner.length}
    }
    const inner=selected||(kind==='bold'?'kalın metin':'italik metin')
    return{text:before+mark+inner+mark+after,start:start+mark.length,end:start+mark.length+inner.length}
  }
  if(kind==='link'){
    const label=selected||'bağlantı',url='https://'
    const inserted=`[${label}](${url})`
    const urlStart=start+label.length+3
    return{text:before+inserted+after,start:urlStart,end:urlStart+url.length}
  }
  // Liste: seçimin kapsadığı satırların başına "- " eklenir; hepsi zaten maddeyse kaldırılır.
  const lineStart=before.lastIndexOf('\n')+1
  const lineEndIndex=after.indexOf('\n')
  const lineEnd=lineEndIndex===-1?text.length:end+lineEndIndex
  const block=text.slice(lineStart,lineEnd)
  const lines=block.split('\n')
  const allListed=lines.every(line=>/^\s*- /.test(line)||!line.trim())
  const next=lines.map(line=>allListed?line.replace(/^(\s*)- /,'$1'):(!line.trim()&&lines.length>1?line:`- ${line}`)).join('\n')
  return{text:text.slice(0,lineStart)+next+text.slice(lineEnd),start:lineStart,end:lineStart+next.length}
}

function CompactNoteEditor({value,onChange,placeholder,autoFocus}:{value:string;onChange:(body:string)=>void;placeholder:string;autoFocus?:boolean}){
  const area=useRef<HTMLTextAreaElement>(null)
  const parts=useMemo(()=>splitBody(value),[value])
  const pendingSelection=useRef<{start:number;end:number}|null>(null)
  const emit=(text:string)=>onChange([text,...parts.markers].filter((line,index)=>index===0||line).join('\n'))
  function format(kind:FmtKind){
    const node=area.current
    if(!node)return
    const result=applyFormat(parts.text,node.selectionStart,node.selectionEnd,kind)
    pendingSelection.current={start:result.start,end:result.end}
    emit(result.text)
  }
  useLayoutEffect(()=>{
    const selection=pendingSelection.current
    if(!selection||!area.current)return
    pendingSelection.current=null
    area.current.focus()
    area.current.setSelectionRange(selection.start,selection.end)
  },[parts.text])
  function onKeyDown(event:KeyboardEvent<HTMLTextAreaElement>){
    if(!(event.ctrlKey||event.metaKey)||event.altKey)return
    const key=event.key.toLowerCase()
    const kind:FmtKind|null=key==='b'?'bold':key==='i'?'italic':key==='k'?'link':key==='l'?'list':null
    if(!kind)return
    event.preventDefault();format(kind)
  }
  return <div className="pn-compact-editor">
    <div className="pn-fmt" role="toolbar" aria-label="Biçim">
      {FMT.map(item=><button type="button" key={item.id} className="pn-fmt-btn" title={item.title} onMouseDown={event=>event.preventDefault()} onClick={()=>format(item.id)}>{FMT_ICONS[item.id]}<span>{item.label}</span></button>)}
      <span className="pn-fmt-hint" title="Kalın: **metin** · İtalik: *metin* · Liste: - madde · Bağlantı: [ad](https://…)">Markdown</span>
    </div>
    <textarea ref={area} className="pn-text" value={parts.text} placeholder={placeholder} aria-label="Not metni" maxLength={4000} spellCheck={false} autoFocus={autoFocus} onChange={event=>emit(event.target.value)} onKeyDown={onKeyDown}/>
    {parts.markers.length>0&&<small className="pn-attach-note">{parts.markers.length} ek bu notta kalır (ekleri ana pencereden yönet).</small>}
  </div>
}

export type ProjectNotesPanelProps={
  token:string
  organizationId:string
  projectId:string
  me:string
  canManage:boolean           // channel.manage: ekip notlarını da düzenler/siler
  compact?:boolean            // overlay: kompakt markdown editörü, durum satırı altta
  onConflict?:(note:ProjectNote)=>void
  onError?:(message:string)=>void
  autoCreate?:boolean         // liste boşsa ilk notu otomatik aç (overlay)
  api?:ProjectNotesApi        // dışarıdan verilen hook (overlay sabitli görünüm için paylaşılır)
}

export default function ProjectNotesPanel(props:ProjectNotesPanelProps){
  const{token,organizationId,projectId,me,canManage}=props
  const[hint,setHint]=useState<{text:string;tone:'warn'|'danger'}|null>(null)
  const hintTimer=useRef<number|null>(null)
  const showHint=(text:string,tone:'warn'|'danger'='warn')=>{
    setHint({text,tone})
    if(hintTimer.current)window.clearTimeout(hintTimer.current)
    hintTimer.current=window.setTimeout(()=>setHint(null),6000)
  }
  const own=useProjectNotes({token,organizationId,projectId,me,enabled:!props.api,
    onConflict:note=>{showHint(NOTE_CONFLICT_TEXT);props.onConflict?.(note)},
    onError:message=>{showHint(message,'danger');props.onError?.(message)}})
  const api=props.api||own
  const{notes,loading,savingIds}=api

  const[selectedId,setSelectedId]=useState<string|null>(null)
  const[confirmDelete,setConfirmDelete]=useState(false)
  const[narrow,setNarrow]=useState(false)
  const[showList,setShowList]=useState(true)
  const root=useRef<HTMLDivElement>(null)

  // Genişlik ölçümü: dar pencerede liste ↔ editör arasında geçiş yapılır.
  useLayoutEffect(()=>{
    const node=root.current
    if(!node||typeof ResizeObserver==='undefined')return
    const apply=()=>setNarrow(node.clientWidth>0&&node.clientWidth<NARROW_WIDTH)
    apply()
    const observer=new ResizeObserver(apply)
    observer.observe(node)
    return()=>observer.disconnect()
  },[])

  const selected=useMemo(()=>notes.find(note=>note.id===selectedId)||null,[notes,selectedId])
  // Seçili not silinirse / ilk yüklemede ilk not seçilir.
  useEffect(()=>{
    if(selected||!notes.length){if(!notes.length&&selectedId)setSelectedId(null);return}
    if(!narrow)setSelectedId(notes[0].id)
  },[notes,selected,narrow])
  useEffect(()=>{setConfirmDelete(false)},[selectedId])
  useEffect(()=>()=>{if(hintTimer.current)window.clearTimeout(hintTimer.current)},[])

  async function createNew(){
    const note=await api.create({visibility:'PRIVATE'})
    if(!note)return
    setSelectedId(note.id);setShowList(false)
  }
  // Overlay: ilk açılışta liste boşsa bir not açılır (yalnız bir kez; kullanıcı silerse yeniden üretilmez).
  const autoCreated=useRef(false)
  useEffect(()=>{if(props.autoCreate&&!loading&&!notes.length&&!autoCreated.current){autoCreated.current=true;void createNew()}},[loading])

  const editable=selected?api.canEdit(selected,canManage):false
  const saving=selected?savingIds.has(selected.id):false
  const openNote=(id:string)=>{setSelectedId(id);setShowList(false)}
  const pane=narrow?(showList||!selected?'show-list':'show-editor'):''
  const compact=Boolean(props.compact)
  const status=selected?<span className={`pn-status${saving?' saving':''}`}>{saving?'Kaydediliyor…':'Kaydedildi'}</span>:null

  return <div ref={root} className={`pn${narrow?' narrow':''}${compact?' compact':''}`}>
    <div className="pn-toolbar">
      {narrow&&selected&&!showList&&<button type="button" className="p2-btn sm icon ghost pn-icon" title="Listeye dön" aria-label="Listeye dön" onClick={()=>setShowList(true)}><BackIcon/></button>}
      <button type="button" className="p2-btn sm primary pn-new" onClick={()=>void createNew()}><PlusIcon/>Yeni not</button>
      {selected&&editable&&(!narrow||!showList)&&<div className="p2-tabs segment" role="tablist" aria-label="Görünürlük">
        <button type="button" role="tab" className={`p2-tab${selected.visibility==='PRIVATE'?' active':''}`} aria-selected={selected.visibility==='PRIVATE'} title="Yalnız sen görürsün" onClick={()=>api.setVisibility(selected.id,'PRIVATE')}>Özel</button>
        <button type="button" role="tab" className={`p2-tab${selected.visibility==='TEAM'?' active':''}`} aria-selected={selected.visibility==='TEAM'} title="Projedeki herkes görür" onClick={()=>api.setVisibility(selected.id,'TEAM')}>Ekip</button>
      </div>}
      {selected&&editable&&(!narrow||!showList)&&<button type="button" className={`p2-btn sm icon ghost pn-icon${selected.pinned?' on':''}`} title={selected.pinned?'Sabitlemeyi kaldır':'Sabitle (listede üstte kalır; overlay sabitli görünümde gösterilir)'} aria-label={selected.pinned?'Sabitlemeyi kaldır':'Sabitle'} aria-pressed={selected.pinned} onClick={()=>api.togglePin(selected.id)}><PinIcon/></button>}
      {selected&&editable&&(!narrow||!showList)&&<button type="button" className="p2-btn sm icon ghost pn-icon" title="Notu sil" aria-label="Notu sil" onClick={()=>setConfirmDelete(true)}><TrashIcon/></button>}
      <span className="spacer"/>
      {!compact&&status}
      {compact&&narrow&&showList&&notes.length>0&&<span className="pn-count">{notes.length} not</span>}
    </div>
    {hint&&<div className={`pn-hint${hint.tone==='danger'?' danger':''}`} role="status">{hint.text}<span className="spacer"/><button type="button" className="p2-btn sm ghost" onClick={()=>setHint(null)}>Tamam</button></div>}
    {confirmDelete&&selected&&<div className="pn-hint danger" role="alertdialog" aria-label="Notu sil"><span>"{noteTitle(selected)}" silinecek.</span><span className="spacer"/><button type="button" className="p2-btn sm ghost" onClick={()=>setConfirmDelete(false)}>Vazgeç</button><button type="button" className="p2-btn sm danger" onClick={()=>{setConfirmDelete(false);void api.remove(selected.id)}}>Sil</button></div>}

    <div className={`pn-body ${pane}`}>
      <div className="pn-list" role="listbox" aria-label="Notlar">
        {loading&&!notes.length&&<div className="p2-empty pn-list-empty"><small>Notlar yükleniyor…</small></div>}
        {!loading&&!notes.length&&<div className="p2-empty pn-list-empty"><b>Henüz not yok</b><small>"Yeni not" ile başla. Özel notları yalnız sen görürsün; Ekip notları projedeki herkese açıktır.</small></div>}
        {notes.map(note=><button type="button" key={note.id} role="option" aria-selected={note.id===selectedId} className={`pn-item${note.id===selectedId?' active':''}`} onClick={()=>openNote(note.id)}>
          <b className={note.title.trim()?undefined:'untitled'}>{noteTitle(note)}</b>
          <span className="pn-item-side">{note.pinned&&<i className="pn-pin" title="Sabitli"><PinIcon/></i>}<VisibilityBadge visibility={note.visibility}/></span>
          <small>{noteTimeAgo(note.updatedAt)}{note.visibility==='TEAM'&&note.authorId!==me?` · ${note.author.displayName}`:''}</small>
        </button>)}
      </div>

      <div className="pn-editor">
        {!selected&&<div className="p2-empty"><b>Not seç</b><small>Soldaki listeden bir not seç ya da yeni bir not oluştur.</small></div>}
        {selected&&editable&&<>
          <div className="pn-editor-head"><input className="p2-input" value={selected.title} maxLength={200} placeholder="Başlık" aria-label="Not başlığı" onChange={event=>api.save(selected.id,{title:event.target.value})}/></div>
          {compact
            ?<CompactNoteEditor key={selected.id} value={selected.body} onChange={body=>{if(body!==selected.body)api.save(selected.id,{body})}} placeholder="Notunu yaz… Kalın için **metin**, madde için satır başına - koy."/>
            :<NotesEditor key={selected.id} token={token} value={selected.body} onChange={body=>{if(body!==selected.body)api.save(selected.id,{body})}} placeholder="Notunu yaz… (biçim, emoji, görsel ve dosya eklenebilir)"/>}
          <div className="pn-editor-meta"><VisibilityBadge visibility={selected.visibility}/><span>{selected.authorId===me?'Senin notun':<>Yazan: <b>{selected.author.displayName}</b></>}</span><span>· {noteTimeAgo(selected.updatedAt)}</span>{compact&&<><span className="spacer"/>{status}</>}</div>
        </>}
        {selected&&!editable&&<>
          <div className="pn-read"><h4>{noteTitle(selected)}</h4><MessageRichText content={selected.body}/></div>
          <div className="pn-editor-meta"><VisibilityBadge visibility={selected.visibility}/><span>Yazan: <b>{selected.author.displayName}</b></span><span>· {noteTimeAgo(selected.updatedAt)}</span><span>· salt okunur</span></div>
        </>}
      </div>
    </div>
  </div>
}
