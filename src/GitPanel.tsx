import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import GitDiff from './GitDiff'
import { ComposerIcon } from './ComposerIcons'
import {
  fileState, gitAvailable, gitBridge, loadRepos, relativeTime, saveRepos,
  type GitCommit, type GitFile, type GitInfo, type Repo,
} from './gitClient'

type Tab='changes'|'history'|'branches'|'lfs'
type Feedback={tone:'ok'|'error';text:string}|null

// P2'nin git istemcisi: depoyu bağla, değişiklikleri sahnele, commit'le, uzak
// sunucuyla eşitle. Depo kullanıcının diskinde durur; komutlar Electron ana
// sürecinde onun git kurulumuyla çalışır, o yüzden GitHub/Gitea/GitLab farketmez.
export default function GitPanel({onClose}:{onClose:()=>void}){
  const bridge=gitBridge()
  const[repos,setRepos]=useState<Repo[]>(()=>gitAvailable()?loadRepos():[])
  const[active,setActive]=useState<string>(()=>repos[0]?.path||'')
  const[info,setInfo]=useState<GitInfo|null>(null)
  const[tab,setTab]=useState<Tab>('changes')
  const[selected,setSelected]=useState<{path:string;staged:boolean;untracked:boolean}|null>(null)
  const[diff,setDiff]=useState('')
  const[commit,setCommit]=useState<GitCommit|null>(null)
  const[message,setMessage]=useState('')
  const[busy,setBusy]=useState(false)
  const[feedback,setFeedback]=useState<Feedback>(null)
  const[cloning,setCloning]=useState(false)
  const[cloneUrl,setCloneUrl]=useState('')
  const[newBranch,setNewBranch]=useState('')
  const[lfsPattern,setLfsPattern]=useState('')
  const messageBox=useRef<HTMLTextAreaElement>(null)

  const refresh=useCallback(async(path:string)=>{
    if(!bridge||!path)return
    const result=await bridge.info({cwd:path})
    if(result?.ok)setInfo(result as GitInfo)
    else{setInfo(null);setFeedback({tone:'error',text:result?.error||'Depo okunamadı.'})}
  },[bridge])

  useEffect(()=>{void refresh(active)},[active,refresh])
  // Depo dışarıdan da değişebilir (editör, terminal); panel açıkken düzenli tazelenir.
  useEffect(()=>{
    if(!active)return
    const timer=window.setInterval(()=>{void refresh(active)},4000)
    return()=>window.clearInterval(timer)
  },[active,refresh])

  const staged=useMemo(()=>info?.status.files.filter(file=>file.staged)||[],[info])
  const changed=useMemo(()=>info?.status.files.filter(file=>file.unstaged)||[],[info])

  async function act(work:()=>Promise<{ok:boolean;error?:string;output?:string;auth?:boolean}>,done:string){
    if(!bridge||busy)return
    setBusy(true);setFeedback(null)
    try{
      const result=await work()
      if(result?.ok)setFeedback({tone:'ok',text:result.output?.split('\n').slice(-2).join(' ')||done})
      else setFeedback({tone:'error',text:result?.auth?`${result.error}\nKimlik doğrulama başarısız — uzak sunucu kullanıcı adı/token istiyor.`:result?.error||'İşlem başarısız.'})
      await refresh(active)
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

  function remember(repo:Repo){
    setRepos(current=>{
      const next=[repo,...current.filter(item=>item.path!==repo.path)]
      saveRepos(next)
      return next
    })
    setActive(repo.path)
  }

  async function addRepo(){
    if(!bridge)return
    const picked=await bridge.pickFolder()
    if(!picked)return
    if(!picked.ok){setFeedback({tone:'error',text:picked.error});return}
    remember({path:picked.path,name:picked.name})
  }
  async function cloneRepo(){
    if(!bridge||!cloneUrl.trim())return
    setBusy(true);setFeedback(null)
    try{
      const result=await bridge.clone({url:cloneUrl.trim()})
      if(!result)return
      if(result.ok){remember({path:result.path,name:result.name});setCloneUrl('');setCloning(false);setFeedback({tone:'ok',text:'Depo klonlandı.'})}
      else setFeedback({tone:'error',text:result.error})
    }finally{setBusy(false)}
  }
  function forgetRepo(path:string){
    setRepos(current=>{
      const next=current.filter(item=>item.path!==path)
      saveRepos(next)
      if(active===path)setActive(next[0]?.path||'')
      return next
    })
  }

  if(!gitAvailable())return <section className="git-panel">
    <header className="git-head"><b>Kaynak Kodu</b><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
    <p className="git-empty">Git istemcisi yalnızca masaüstü uygulamasında çalışır — depolar senin diskinde durur ve komutlar kendi git kurulumunla koşar.</p>
  </section>

  const status=info?.status
  const upstream=status?.upstream||''
  const canCommit=staged.length>0&&message.trim().length>0

  return <section className="git-panel">
    <header className="git-head">
      <span className="git-repo">
        <select value={active} onChange={event=>{setActive(event.target.value);setSelected(null);setCommit(null);setDiff('')}}>
          {!repos.length&&<option value="">Depo yok</option>}
          {repos.map(repo=><option key={repo.path} value={repo.path}>{repo.name}</option>)}
        </select>
        <button type="button" className="git-button" onClick={()=>void addRepo()}>Klasör aç</button>
        <button type="button" className="git-button" onClick={()=>setCloning(value=>!value)}>Klonla</button>
        {active&&<button type="button" className="git-button subtle" title="Listeden çıkar (dosyalar silinmez)" onClick={()=>forgetRepo(active)}>Kaldır</button>}
      </span>
      <span className="git-branchbar">
        {status&&<>
          <b title={info?.root}>{status.branch||'(dal yok)'}</b>
          {upstream&&<small>↑{status.ahead} ↓{status.behind} · {upstream}</small>}
          {!upstream&&<small>uzak dal yok</small>}
        </>}
      </span>
      <span className="git-actions">
        <button type="button" className="git-button" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'fetch'}),'Fetch tamam.')}>Fetch</button>
        <button type="button" className="git-button" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'pull',branch:status?.branch}),'Pull tamam.')}>Pull{status?.behind?` (${status.behind})`:''}</button>
        <button type="button" className="git-button primary" disabled={busy||!active} onClick={()=>void act(()=>bridge!.sync({cwd:active,action:'push',branch:status?.branch,setUpstream:!upstream}),'Push tamam.')}>Push{status?.ahead?` (${status.ahead})`:''}</button>
        <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button>
      </span>
    </header>

    {cloning&&<div className="git-clone">
      <input value={cloneUrl} placeholder="https://sunucu/kullanici/depo.git" onChange={event=>setCloneUrl(event.target.value)}/>
      <button type="button" className="git-button primary" disabled={busy||!cloneUrl.trim()} onClick={()=>void cloneRepo()}>Klasör seç ve klonla</button>
    </div>}

    {feedback&&<p className={feedback.tone==='ok'?'git-note':'git-note error'}>{feedback.text}</p>}

    {!active&&<p className="git-empty">Başlamak için bir depo klasörü aç ya da uzak bir depoyu klonla.</p>}

    {active&&<div className="git-body">
      <aside className="git-side">
        <div className="git-tabs">
          <button type="button" className={tab==='changes'?'active':undefined} onClick={()=>setTab('changes')}>Değişiklikler <em>{status?.files.length||0}</em></button>
          <button type="button" className={tab==='history'?'active':undefined} onClick={()=>setTab('history')}>Geçmiş</button>
          <button type="button" className={tab==='branches'?'active':undefined} onClick={()=>setTab('branches')}>Dallar</button>
          <button type="button" className={tab==='lfs'?'active':undefined} onClick={()=>setTab('lfs')}>LFS</button>
        </div>

        {tab==='changes'&&<div className="git-files">
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
          {!info?.lfs.installed&&<>
            <p className="git-empty small">Bu depoda Git LFS kurulu değil.</p>
            <button type="button" className="git-button primary" disabled={busy} onClick={()=>void act(()=>bridge!.lfs({cwd:active,action:'install'}),'LFS kuruldu.')}>LFS kur</button>
          </>}
          {info?.lfs.installed&&<>
            <p className="git-group-title">İzlenen desenler</p>
            {info.lfs.patterns.map(pattern=><div className="git-branch" key={pattern}>
              <button type="button" disabled><b>{pattern}</b></button>
              <button type="button" className="git-button subtle" disabled={busy} title="İzlemeyi bırak" onClick={()=>void act(()=>bridge!.lfs({cwd:active,action:'untrack',pattern}),'Desen kaldırıldı.')}>×</button>
            </div>)}
            {!info.lfs.patterns.length&&<p className="git-empty small">Henüz desen yok.</p>}
            <div className="git-newbranch">
              <input value={lfsPattern} placeholder="*.psd" onChange={event=>setLfsPattern(event.target.value)}/>
              <button type="button" className="git-button" disabled={busy||!lfsPattern.trim()} onClick={()=>{void act(()=>bridge!.lfs({cwd:active,action:'track',pattern:lfsPattern.trim()}),'Desen izleniyor.');setLfsPattern('')}}>İzle</button>
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
          void act(()=>bridge!.commit({cwd:active,message}),'Commit oluşturuldu.').then(()=>setMessage(''))
        }}>
          <textarea ref={messageBox} value={message} rows={3} placeholder={staged.length?'Commit mesajı — ilk satır özet, boş satırdan sonra ayrıntı':'Önce değişiklik sahnele'}
            onChange={event=>setMessage(event.target.value)}
            onKeyDown={event=>{if(event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&canCommit){event.preventDefault();event.currentTarget.form?.requestSubmit()}}}/>
          <div className="git-commit-actions">
            <small>{staged.length?`${staged.length} dosya sahnelendi · Ctrl+Enter`:'Sahnelenmiş dosya yok'}</small>
            <button type="button" className="git-button" disabled={busy||!status?.files.length} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'push'}),'Zulaya alındı.')}>Zulala</button>
            {(status?.stashes||0)>0&&<button type="button" className="git-button" disabled={busy} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'pop'}),'Zuladan alındı.')}>Zuladan al ({status?.stashes})</button>}
            <button type="submit" className="git-button primary" disabled={busy||!canCommit}><ComposerIcon name="send"/> Commit</button>
          </div>
        </form>
      </div>
    </div>}
  </section>
}
