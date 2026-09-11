import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import GitDiff from './GitDiff'
import { ComposerIcon } from './ComposerIcons'
import ProviderMark from './ProviderMarks'
import {
  fileState, gitBridge, relativeTime,
  type GitCommit, type GitFile, type GitInfo, type Repo,
} from './gitClient'

type Tab='changes'|'history'|'branches'|'lfs'
type UiMode='desktop'|'compact'
type Feedback={tone:'ok'|'error';text:string}|null
const UI_MODE_KEY='p2:git-ui-mode:v2'
const MAX_RENDERED_FILES=600

// P2'nin git istemcisi: depoyu bağla, değişiklikleri sahnele, commit'le, uzak
// sunucuyla eşitle. Depo kullanıcının diskinde durur; komutlar Electron ana
// sürecinde onun git kurulumuyla çalışır, o yüzden GitHub/Gitea/GitLab farketmez.
export default function GitPanel({repoPath,repositories,onSelectRepo,onClose,onForget}:{repoPath:string;repositories:Repo[];onSelectRepo:(path:string)=>void;onClose:()=>void;onForget:(path:string)=>void}){
  const bridge=gitBridge()
  const active=repoPath
  const[info,setInfo]=useState<GitInfo|null>(null)
  // Compact, P2'nin varsayılan GitHub Desktop düzenidir. Yeni anahtar eski
  // sürümde ters anlamla kaydedilmiş "desktop" seçimini taşımamayı sağlar.
  const[mode,setMode]=useState<UiMode>(()=>window.localStorage.getItem(UI_MODE_KEY)==='desktop'?'desktop':'compact')
  const[tab,setTab]=useState<Tab>('changes')
  const[selected,setSelected]=useState<{path:string;staged:boolean;untracked:boolean}|null>(null)
  const[diff,setDiff]=useState('')
  const[commit,setCommit]=useState<GitCommit|null>(null)
  const[message,setMessage]=useState('')
  const[description,setDescription]=useState('')
  const[fileFilter,setFileFilter]=useState('')
  const[repoFilter,setRepoFilter]=useState('')
  const[repoOpen,setRepoOpen]=useState(false)
  const[lfsLoadedFor,setLfsLoadedFor]=useState('')
  const[busy,setBusy]=useState(false)
  const[feedback,setFeedback]=useState<Feedback>(null)
  const[newBranch,setNewBranch]=useState('')
  const[branchOpen,setBranchOpen]=useState(false)
  const[lfsPattern,setLfsPattern]=useState('')
  const repoMenu=useRef<HTMLSpanElement>(null)
  const activePath=useRef(active)
  const refreshJobs=useRef(new Set<string>())
  activePath.current=active

  const refresh=useCallback(async(path:string)=>{
    if(!bridge||!path||refreshJobs.current.has(path))return
    refreshJobs.current.add(path)
    try{
      const result=await bridge.info({cwd:path})
      if(activePath.current!==path)return
      if(result?.ok)setInfo(result as GitInfo)
      else{setInfo(null);setFeedback({tone:'error',text:result?.error||'Depo okunamadı.'})}
    }finally{refreshJobs.current.delete(path)}
  },[bridge])

  const refreshStatus=useCallback(async(path:string)=>{
    if(!bridge||!path||refreshJobs.current.has(path))return
    refreshJobs.current.add(path)
    try{
      const result=await bridge.status({cwd:path})
      if(result?.ok&&activePath.current===path)setInfo(current=>current?{...current,status:result.status}:current)
    }finally{refreshJobs.current.delete(path)}
  },[bridge])

  useEffect(()=>{
    setInfo(null);setSelected(null);setCommit(null);setDiff('');setBranchOpen(false);setRepoOpen(false);setLfsLoadedFor('')
    void refresh(active)
  },[active,refresh])
  // Depo dışarıdan da değişebilir. Sık yenilemede yalnızca ucuz status çağrısı
  // çalışır; dal/geçmiş/LFS sorguları her turda yeniden başlatılmaz.
  useEffect(()=>{
    if(!active)return
    const timer=window.setInterval(()=>{void refreshStatus(active)},4000)
    return()=>window.clearInterval(timer)
  },[active,refreshStatus])
  useEffect(()=>{
    if(tab!=='lfs'||!bridge||!active||!info||lfsLoadedFor===active)return
    let cancelled=false
    void bridge.lfs({cwd:active,action:'status'}).then(result=>{
      if(cancelled)return
      if(result?.ok&&result.lfs)setInfo(current=>current?{...current,lfs:result.lfs}:current)
      else setFeedback({tone:'error',text:result?.error||'LFS bilgisi alınamadı.'})
      setLfsLoadedFor(active)
    })
    return()=>{cancelled=true}
  },[active,bridge,info,lfsLoadedFor,tab])
  useEffect(()=>{
    if(!repoOpen)return
    const outside=(event:PointerEvent)=>{if(!repoMenu.current?.contains(event.target as Node))setRepoOpen(false)}
    const escape=(event:KeyboardEvent)=>{if(event.key==='Escape')setRepoOpen(false)}
    document.addEventListener('pointerdown',outside)
    document.addEventListener('keydown',escape)
    return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape)}
  },[repoOpen])

  const staged=useMemo(()=>info?.status.files.filter(file=>file.staged)||[],[info])
  const changed=useMemo(()=>info?.status.files.filter(file=>file.unstaged)||[],[info])

  async function act(work:()=>Promise<{ok:boolean;error?:string;output?:string;auth?:boolean}>,done:string){
    if(!bridge||busy)return
    setBusy(true);setFeedback(null)
    try{
      const result=await work()
      if(result?.ok)setFeedback({tone:'ok',text:result.output?.split('\n').slice(-2).join(' ')||done})
      else setFeedback({tone:'error',text:result?.auth?`${result.error}\nKimlik doğrulama başarısız — uzak sunucu kullanıcı adı/token istiyor.`:result?.error||'İşlem başarısız.'})
      await refreshStatus(active)
    }finally{setBusy(false)}
  }

  async function openFile(file:GitFile,fromStaged:boolean){
    setCommit(null)
    setSelected({path:file.path,staged:fromStaged,untracked:file.untracked})
    if(!bridge)return
    const result=await bridge.diff({cwd:active,path:file.path,staged:fromStaged,untracked:file.untracked&&!fromStaged})
    setDiff(result?.ok?result.diff:'')
  }
  async function openCommit(item:GitCommit){
    setSelected(null);setCommit(item)
    if(!bridge)return
    const result=await bridge.commitDiff({cwd:active,hash:item.hash})
    setDiff(result?.ok?result.diff:'')
  }
  useEffect(()=>{const first=info?.status.files[0];if(mode==='compact'&&tab==='changes'&&!selected&&first)void openFile(first,first.staged&&!first.unstaged)},[info,mode,tab,selected])

  const status=info?.status
  const upstream=status?.upstream||''
  const canCommit=staged.length>0&&message.trim().length>0
  const chooseMode=(next:UiMode)=>{setMode(next);window.localStorage.setItem(UI_MODE_KEY,next)}
  const syncAction=status?.behind?'pull':status?.ahead?'push':'fetch'
  const visibleFiles=useMemo(()=>{
    const query=fileFilter.trim().toLocaleLowerCase('tr-TR')
    return query?(status?.files||[]).filter(file=>file.path.toLocaleLowerCase('tr-TR').includes(query)):(status?.files||[])
  },[fileFilter,status?.files])
  const visibleRepos=useMemo(()=>{
    const query=repoFilter.trim().toLocaleLowerCase('tr-TR')
    return query?repositories.filter(repo=>repo.name.toLocaleLowerCase('tr-TR').includes(query)||repo.path.toLocaleLowerCase('tr-TR').includes(query)):repositories
  },[repoFilter,repositories])

  return <section className={`git-panel git-mode-${mode} git-tab-${tab}`}>
    <header className="git-head">
      <span className="git-repo" ref={repoMenu}>
        <button type="button" className="git-back" onClick={onClose} title="Depo listesine dön">‹ Depolar</button>
        <button type="button" className="git-desktop-toolbar-button repo-button" aria-haspopup="listbox" aria-expanded={repoOpen} onClick={()=>setRepoOpen(value=>!value)}><small>Current repository</small><b className="git-repo-name" title={info?.root||active}>{info?.name||repositories.find(repo=>repo.path===active)?.name||'…'}</b><svg className={repoOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
        {mode==='compact'&&repoOpen&&<div className="git-repo-popover"><label className="git-repo-filter"><ComposerIcon name="search"/><input autoFocus value={repoFilter} onChange={event=>setRepoFilter(event.target.value)} placeholder="Filter"/></label><header>Other</header><div className="git-repo-options" role="listbox">{visibleRepos.map(repo=><button type="button" role="option" aria-selected={repo.path===active} className={repo.path===active?'active':''} key={repo.path} title={repo.path} onClick={()=>{setRepoOpen(false);setRepoFilter('');onSelectRepo(repo.path)}}><ProviderMark kind={repo.provider||'git'} size={15}/><span>{repo.name}</span>{repo.path===active&&<i/>}</button>)}{!visibleRepos.length&&<p>Repo bulunamadı.</p>}</div><button type="button" className="git-manage-repos" onClick={()=>{setRepoOpen(false);onClose()}}>Tüm depoları yönet</button></div>}
        {mode==='desktop'&&<><b className="git-repo-name" title={info?.root}>{info?.name||'…'}</b><button type="button" className="git-button subtle" title="Listeden çıkar (dosyalar silinmez)" onClick={()=>onForget(active)}>Listeden çıkar</button></>}
      </span>
      <span className="git-branchbar">
        {status&&<>
          <button type="button" className="git-desktop-toolbar-button branch-button" onClick={()=>setBranchOpen(value=>!value)}><small>Current branch</small><b title={info?.root}>⑂ {status.branch||'(dal yok)'}</b><svg className={branchOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
          {mode==='desktop'&&<>{upstream&&<small>↑{status.ahead} ↓{status.behind} · {upstream}</small>}{!upstream&&<small>uzak dal yok</small>}</>}
        </>}
        {branchOpen&&<div className="git-branch-popover"><header><b>Switch branches</b><button onClick={()=>setBranchOpen(false)}>×</button></header><div>{(info?.branches.local||[]).map(item=><button key={item.name} className={item.current?'active':''} disabled={busy||item.current} onClick={()=>{setBranchOpen(false);void act(()=>bridge!.branch({cwd:active,action:'checkout',name:item.name}),`${item.name} dalına geçildi.`)}}><span>⑂</span><b>{item.name}</b>{item.current&&<i>✓</i>}</button>)}</div><button className="new-branch" onClick={()=>{setBranchOpen(false);setTab('branches')}}>＋ New branch</button></div>}
      </span>
      <span className="git-actions">
        {mode==='compact'?<button type="button" className="git-desktop-sync" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:syncAction,branch:status?.branch,setUpstream:syncAction==='push'&&!upstream}),`${syncAction} tamam.`)}><span>{syncAction==='pull'?'↓':syncAction==='push'?'↑':'↻'}</span><b>{syncAction==='pull'?`Pull origin${status?.behind?` (${status.behind})`:''}`:syncAction==='push'?`Push origin${status?.ahead?` (${status.ahead})`:''}`:'Fetch origin'}</b><small>{upstream||'Last fetched just now'}</small></button>:<><button type="button" className="git-button" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'fetch'}),'Fetch tamam.')}>Fetch</button><button type="button" className="git-button" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'pull',branch:status?.branch}),'Pull tamam.')}>Pull{status?.behind?` (${status.behind})`:''}</button><button type="button" className="git-button primary" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'push',branch:status?.branch,setUpstream:!upstream}),'Push tamam.')}>Push{status?.ahead?` (${status.ahead})`:''}</button></>}
        <span className="git-compact-control" title={mode==='compact'?'Compact modu kapat':'Compact modu aç'}><span>Compact mod</span><button type="button" className={mode==='compact'?'git-compact-toggle active':'git-compact-toggle'} role="switch" aria-checked={mode==='compact'} aria-label="Compact modu aç veya kapat" onClick={()=>chooseMode(mode==='compact'?'desktop':'compact')}><i/></button></span>
      </span>
    </header>

    {feedback&&<p className={feedback.tone==='ok'?'git-note':'git-note error'}>{feedback.text}</p>}

    {active&&<div className="git-body">
      <aside className="git-side">
        <div className="git-tabs">
          <button type="button" className={tab==='changes'?'active':undefined} onClick={()=>setTab('changes')}>{mode==='compact'?'Changes':'Değişiklikler'} <em>{status?.files.length||0}</em></button>
          <button type="button" className={tab==='history'?'active':undefined} onClick={()=>setTab('history')}>{mode==='compact'?'History':'Geçmiş'}</button>
          <button type="button" className={tab==='branches'?'active':undefined} onClick={()=>setTab('branches')}>Dallar</button>
          <button type="button" className={tab==='lfs'?'active':undefined} onClick={()=>setTab('lfs')}>LFS</button>
        </div>

        {tab==='changes'&&mode==='compact'&&<div className="git-files desktop-files">
          <label className="desktop-filter"><ComposerIcon name="search"/><input value={fileFilter} onChange={event=>setFileFilter(event.target.value)} placeholder="Filter" aria-label="Değişen dosyaları filtrele"/></label>
          <header className="desktop-files-head"><label><input type="checkbox" checked={Boolean(status?.files.length)&&status!.files.every(file=>file.staged)} onChange={event=>{const files=status?.files||[];void act(()=>event.target.checked?bridge!.stage({cwd:active,paths:files.map(file=>file.path)}):bridge!.unstage({cwd:active,paths:files.map(file=>file.path)}),event.target.checked?'All changes selected.':'All changes unselected.')}}/><b>{status?.files.length||0} changed files</b></label></header>
          {visibleFiles.slice(0,MAX_RENDERED_FILES).map(file=>{const stagedView=file.staged&&!file.unstaged;return <div className={selected?.path===file.path?'desktop-file active':'desktop-file'} key={file.path}><input type="checkbox" checked={file.staged} aria-label={`${file.path} commit kapsamı`} onChange={event=>void act(()=>event.target.checked?bridge!.stage({cwd:active,paths:[file.path]}):bridge!.unstage({cwd:active,paths:[file.path]}),event.target.checked?'Change selected.':'Change unselected.')}/><button type="button" title={file.path} onClick={()=>void openFile(file,stagedView)}><span>{file.path}</span></button><i className={`git-badge ${fileState(file).tone}`}>{fileState(file).label.slice(0,1)}</i></div>})}
          {visibleFiles.length>MAX_RENDERED_FILES&&<p className="desktop-file-limit">İlk {MAX_RENDERED_FILES} dosya gösteriliyor · Aradığın dosya için filtreyi kullan.</p>}
          {Boolean(status?.files.length)&&!visibleFiles.length&&<p className="git-empty small">Filtreyle eşleşen dosya yok.</p>}
          {!status?.files.length&&<div className="desktop-clean"><span>✓</span><b>No local changes</b><p>There are no uncommitted changes in this repository.</p><button onClick={()=>void bridge!.reveal({cwd:active})}>Show in Explorer</button></div>}
        </div>}

        {tab==='changes'&&mode==='desktop'&&<div className="git-files">
          <div className="git-group">
            <header>
              <b>Sahnelenen · {staged.length}</b>
              {staged.length>0&&<button type="button" disabled={busy} onClick={()=>void act(()=>bridge!.unstage({cwd:active,paths:staged.map(file=>file.path)}),'Sahneden çıkarıldı.')}>Tümünü çıkar</button>}
            </header>
            {staged.map(file=><button type="button" key={`s-${file.path}`} className={selected?.path===file.path&&selected.staged?'git-file active':'git-file'} onClick={()=>void openFile(file,true)}>
              <i className={`git-badge ${fileState(file).tone}`}>{fileState(file).label}</i>
              <span>{file.path}{file.from&&<em> ← {file.from}</em>}</span>
              <b onClick={event=>{event.stopPropagation();void act(()=>bridge!.unstage({cwd:active,paths:[file.path]}),'Sahneden çıkarıldı.')}} title="Sahneden çıkar">−</b>
            </button>)}
            {!staged.length&&<p className="git-empty small">Sahnelenmiş değişiklik yok.</p>}
          </div>

          <div className="git-group">
            <header>
              <b>Değişenler · {changed.length}</b>
              {changed.length>0&&<button type="button" disabled={busy} onClick={()=>void act(()=>bridge!.stage({cwd:active,paths:changed.map(file=>file.path)}),'Tümü sahnelendi.')}>Tümünü sahnele</button>}
            </header>
            {changed.map(file=><button type="button" key={`c-${file.path}`} className={selected?.path===file.path&&!selected.staged?'git-file active':'git-file'} onClick={()=>void openFile(file,false)}>
              <i className={`git-badge ${fileState(file).tone}`}>{fileState(file).label}</i>
              <span>{file.path}</span>
              <b onClick={event=>{event.stopPropagation();void act(()=>bridge!.stage({cwd:active,paths:[file.path]}),'Sahnelendi.')}} title="Sahnele">+</b>
            </button>)}
            {!changed.length&&<p className="git-empty small">Çalışma alanı temiz.</p>}
          </div>
        </div>}

        {tab==='history'&&<div className="git-log">
          {(info?.log||[]).map(item=><button type="button" key={item.hash} className={commit?.hash===item.hash?'git-commit active':'git-commit'} onClick={()=>void openCommit(item)}>
            <b>{item.subject}</b>
            <small>{item.author} · {relativeTime(item.at)} · {item.short}</small>
            {item.refs.length>0&&<span className="git-refs">{item.refs.map(ref=><em key={ref}>{ref.replace('HEAD -> ','')}</em>)}</span>}
          </button>)}
          {!info?.log.length&&<p className="git-empty small">Henüz commit yok.</p>}
        </div>}

        {tab==='branches'&&<div className="git-branches">
          <div className="git-newbranch">
            <input value={newBranch} placeholder="yeni-dal-adı" onChange={event=>setNewBranch(event.target.value.replace(/\s+/g,'-'))}/>
            <button type="button" className="git-button" disabled={busy||newBranch.trim().length<2} onClick={()=>{void act(()=>bridge!.branch({cwd:active,action:'create',name:newBranch.trim()}),'Dal oluşturuldu.');setNewBranch('')}}>Oluştur</button>
          </div>
          <p className="git-group-title">Yerel</p>
          {(info?.branches.local||[]).map(item=><div key={item.name} className={item.current?'git-branch current':'git-branch'}>
            <button type="button" disabled={busy||item.current} onClick={()=>void act(()=>bridge!.branch({cwd:active,action:'checkout',name:item.name}),`${item.name} dalına geçildi.`)}>
              <b>{item.name}</b><small>{item.upstream||'uzak dal yok'} · {relativeTime(item.at)}</small>
            </button>
            {!item.current&&<button type="button" className="git-button subtle" disabled={busy} title="Dalı sil" onClick={()=>void act(()=>bridge!.branch({cwd:active,action:'delete',name:item.name}),'Dal silindi.')}>×</button>}
          </div>)}
          <p className="git-group-title">Uzak</p>
          {(info?.branches.remote||[]).map(item=><div key={item.name} className="git-branch">
            <button type="button" disabled={busy} onClick={()=>void act(()=>bridge!.branch({cwd:active,action:'create',name:item.name.split('/').slice(1).join('/'),from:item.name}),'Uzak dal alındı.')}>
              <b>{item.name}</b><small>{relativeTime(item.at)}</small>
            </button>
          </div>)}
        </div>}

        {tab==='lfs'&&<div className="git-lfs">
          {lfsLoadedFor!==active&&<p className="git-empty small">LFS bilgisi yükleniyor…</p>}
          {lfsLoadedFor===active&&!info?.lfs.installed&&<>
            <p className="git-empty small">Bu depoda Git LFS kurulu değil.</p>
            <button type="button" className="git-button primary" disabled={busy} onClick={()=>void act(()=>bridge!.lfs({cwd:active,action:'install'}),'LFS kuruldu.').then(()=>setLfsLoadedFor(''))}>LFS kur</button>
          </>}
          {lfsLoadedFor===active&&info?.lfs.installed&&<>
            <p className="git-group-title">İzlenen desenler</p>
            {info.lfs.patterns.map(pattern=><div className="git-branch" key={pattern}>
              <button type="button" disabled><b>{pattern}</b></button>
              <button type="button" className="git-button subtle" disabled={busy} title="İzlemeyi bırak" onClick={()=>void act(()=>bridge!.lfs({cwd:active,action:'untrack',pattern}),'Desen kaldırıldı.').then(()=>setLfsLoadedFor(''))}>×</button>
            </div>)}
            {!info.lfs.patterns.length&&<p className="git-empty small">Henüz desen yok.</p>}
            <div className="git-newbranch">
              <input value={lfsPattern} placeholder="*.psd" onChange={event=>setLfsPattern(event.target.value)}/>
              <button type="button" className="git-button" disabled={busy||!lfsPattern.trim()} onClick={()=>{void act(()=>bridge!.lfs({cwd:active,action:'track',pattern:lfsPattern.trim()}),'Desen izleniyor.').then(()=>setLfsLoadedFor(''));setLfsPattern('')}}>İzle</button>
            </div>
            <p className="git-group-title">LFS dosyaları · {info.lfs.files.length}</p>
            <div className="git-lfs-files">{info.lfs.files.slice(0,60).map(file=><code key={file}>{file}</code>)}</div>
          </>}
        </div>}
      </aside>

      <div className="git-main">
        <div className="git-preview">
          {commit&&<header className="git-preview-head"><b>{commit.subject}</b><small>{commit.author} · {new Date(commit.at).toLocaleString('tr-TR')} · {commit.short}</small></header>}
          {selected&&<header className="git-preview-head">
            <b>{selected.path}</b>
            <small>{selected.staged?'sahnelenmiş hâli':'çalışma alanındaki hâli'}</small>
            <span>
              <button type="button" className="git-button subtle" onClick={()=>void bridge!.reveal({cwd:active,path:selected.path})}>Klasörde göster</button>
              {!selected.staged&&<button type="button" className="git-button danger" disabled={busy} onClick={()=>void act(()=>bridge!.discard({cwd:active,paths:selected.untracked?[]:[selected.path],untracked:selected.untracked?[selected.path]:[]}),'Değişiklik geri alındı.')}>Değişikliği geri al</button>}
            </span>
          </header>}
          <GitDiff diff={diff} empty={selected||commit?'Bu seçimde gösterilecek fark yok.':'Soldan bir dosya ya da commit seç.'}/>
        </div>

        <form className="git-commit" onSubmit={event=>{
          event.preventDefault()
          if(!canCommit)return
          const fullMessage=`${message.trim()}${description.trim()?`\n\n${description.trim()}`:''}`
          void act(()=>bridge!.commit({cwd:active,message:fullMessage}),'Commit oluşturuldu.').then(()=>{setMessage('');setDescription('')})
        }}>
          {mode==='compact'?<div className="desktop-commit-fields"><span className="desktop-commit-avatar" aria-hidden="true">G</span><input value={message} placeholder={staged.length?'Summary (required)':'Select files to commit'} onChange={event=>setMessage(event.target.value)} onKeyDown={event=>{if(event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&canCommit){event.preventDefault();event.currentTarget.form?.requestSubmit()}}}/><textarea value={description} rows={4} placeholder="Description" onChange={event=>setDescription(event.target.value)}/></div>:<textarea value={message} rows={3} placeholder={staged.length?'Commit mesajı — ilk satır özet, boş satırdan sonra ayrıntı':'Önce değişiklik sahnele'}
            onChange={event=>setMessage(event.target.value)}
            onKeyDown={event=>{if(event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&canCommit){event.preventDefault();event.currentTarget.form?.requestSubmit()}}}/>}
          <div className="git-commit-actions">
            <small>{staged.length?`${staged.length} dosya sahnelendi · Ctrl+Enter`:'Sahnelenmiş dosya yok'}</small>
            <button type="button" className="git-button" disabled={busy||!status?.files.length} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'push'}),'Zulaya alındı.')}>Zulala</button>
            {(status?.stashes||0)>0&&<button type="button" className="git-button" disabled={busy} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'pop'}),'Zuladan alındı.')}>Zuladan al ({status?.stashes})</button>}
            <button type="submit" className="git-button primary" disabled={busy||!canCommit}>{mode==='compact'?`Commit ${staged.length} ${staged.length===1?'file':'files'} to ${status?.branch||'current branch'}`:<><ComposerIcon name="send"/> Commit</>}</button>
          </div>
        </form>
      </div>
    </div>}
  </section>
}
