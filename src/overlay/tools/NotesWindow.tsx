import { useEffect, useMemo } from 'react'
import MessageRichText from '../../MessageRichText'
import ProjectNotesPanel, { noteTimeAgo, noteTitle, VisibilityBadge } from '../../ProjectNotesPanel'
import { useProjectNotes } from '../../hooks/useProjectNotes'
import { useMyPermissions } from '../../permissions'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import { IconNotes } from '../icons'
import type { OverlayToolProps } from '../overlayTypes'
import './notes.css'

// Notlar penceresi (overlay): oyunun projesine ait özel/ekip notları. Panel açıkken tam editör
// (liste | başlık + zengin metin); sabitliyken ilk sabitli not salt okunur ekranda kalır
// ("oynarken not gözümün önünde dursun"). Veri REST (kiralanan jeton) + aynalanan `note:changed`.

export default function NotesWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const projectId=props.projectId||overlaySessionInfo()?.projectId||auth.selectedProjectId||''
  const me=String((auth.user as {id?:string}|null)?.id||'')
  const organizationId=String((auth.organization as {id?:string}|null)?.id||'')
  const perms=useMyPermissions(auth.ready?auth.accessToken:'',organizationId)
  const canManage=perms.can('channel.manage')
  // Hook burada tutulur: sabitli (salt okunur) görünüm ile editör aynı listeyi paylaşır.
  const api=useProjectNotes({token:auth.accessToken,organizationId,projectId,me,enabled:auth.ready&&(props.visible||props.pinned)})
  const pinnedNote=useMemo(()=>api.notes.find(note=>note.pinned)||null,[api.notes])
  useEffect(()=>{props.setTitle?.(`Notlar${api.notes.length?` · ${api.notes.length}`:''}`)},[api.notes.length])

  if(!auth.ready||!organizationId||!projectId)return <div className="p2-empty"><small>Oturum bekleniyor…</small></div>

  if(props.pinned&&props.minimized){
    if(!pinnedNote)return null
    return <div className="overlay-notes-mini" title={noteTitle(pinnedNote)}><IconNotes size={14}/><span>{noteTitle(pinnedNote)}</span></div>
  }
  if(props.pinned){
    // Sabitli pencereye girdi gelmez: ilk sabitli not salt okunur çizilir.
    if(!pinnedNote)return <div className="overlay-notes-pinned"><div className="p2-empty"><b>Sabitli not yok</b><small>Bir notu sabitlersen oyun sırasında burada kalır.</small></div></div>
    return <div className="overlay-notes-pinned" aria-label="Sabitli not">
      <header><b>{noteTitle(pinnedNote)}</b><VisibilityBadge visibility={pinnedNote.visibility}/><small>{noteTimeAgo(pinnedNote.updatedAt)}</small></header>
      <div className="overlay-notes-read">{pinnedNote.body.trim()?<MessageRichText content={pinnedNote.body}/>:<small style={{color:'var(--p2-text-4)'}}>Not boş.</small>}</div>
    </div>
  }
  return <div className="overlay-notes">
    <ProjectNotesPanel api={api} token={auth.accessToken} organizationId={organizationId} projectId={projectId} me={me} canManage={canManage} compact autoCreate/>
  </div>
}
