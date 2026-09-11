import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import GitDiff from './GitDiff'
import { ComposerIcon } from './ComposerIcons'
import {
  fileState, gitBridge, relativeTime,
  type GitCommit, type GitFile, type GitInfo, type Repo,
} from './gitClient'

type Tab='changes'|'history'|'branches'
type UiMode='desktop'|'compact'
type FileFilter='included'|'excluded'|'new'|'modified'|'deleted'
type SettingsTab='remote'|'ignored'|'config'|'lfs'
type ForkDetailTab='commit'|'changes'|'tree'
type CommitFile={path:string;from:string|null;state:string}
type CommitStats={additions:number;deletions:number}
type RepositorySettings={remoteUrl:string;ignored:string;userName:string;userEmail:string;lfs:{installed:boolean;patterns:string[];files:string[]}}
type GitOperation='fetch'|'pull'|'push'|'commit'
type ActionResult={ok:boolean;error?:string;output?:string;auth?:boolean}
type Feedback={tone:'ok'|'error';text:string}|null
type ForkGraphSegment={from:number;to:number;start:number;end:number;color:number}
type ForkGraphRow={lane:number;color:number;merge:boolean;segments:ForkGraphSegment[]}
type ForkGraph={rows:Map<string,ForkGraphRow>;maxLanes:number}
type ForkPathEntry={path:string;state?:string}
type ForkPathNode={name:string;path:string;entry?:ForkPathEntry;children:ForkPathNode[]}
const UI_MODE_KEY='p2:git-ui-mode:v2'
const MAX_RENDERED_FILES=600

function RepositoryIcon({className=''}:{className?:string}){
  return <svg className={className} viewBox="0 0 18 18" aria-hidden="true"><rect x="2.5" y="3" width="13" height="9" rx="1"/><path d="M6.5 15h5M9 12v3"/></svg>
}

function SettingsIcon(){
  return <svg viewBox="0 0 18 18" aria-hidden="true"><circle cx="9" cy="9" r="2.6"/><path d="M7.5 2.3h3l.5 1.8 1.2.7 1.8-.6 1.5 2.6-1.3 1.3v1.7l1.3 1.3-1.5 2.6-1.8-.6-1.2.7-.5 1.8h-3L7 13.8l-1.2-.7-1.8.6-1.5-2.6 1.3-1.3V8.1L2.5 6.8 4 4.2l1.8.6L7 4.1l.5-1.8Z"/></svg>
}

function FileIcon(){
  return <svg viewBox="0 0 18 18" aria-hidden="true"><path d="M4 2.5h6l4 4v9H4z"/><path d="M10 2.5v4h4"/></svg>
}

function FolderIcon(){
  return <svg className="fork-folder-icon" viewBox="0 0 18 18" aria-hidden="true"><path d="M2.5 4.5h5l1.6 2h6.4v8H2.5z"/></svg>
}

function CopyIcon(){
  return <svg viewBox="0 0 18 18" aria-hidden="true"><rect x="6" y="5.5" width="8" height="9" rx="1"/><path d="M4 12.5H3.5a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1V4"/></svg>
}

function SyncIcon({action}:{action:'fetch'|'pull'|'push'}){
  if(action==='fetch')return <ComposerIcon name="refresh"/>
  return <svg viewBox="0 0 18 18" aria-hidden="true"><path d={action==='pull'?'M9 2.5v10M5.5 9 9 12.5 12.5 9':'M9 15.5v-10M5.5 9 9 5.5 12.5 9'}/><path d="M3 15.5h12"/></svg>
}

function ForkToolIcon({name}:{name:'stash'|'folder'}){
  if(name==='stash')return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 6h12l-1 10H5L4 6Z"/><path d="M3 3.5h14M7 9.5h6"/></svg>
  return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M2.5 5.5h6l1.5 2h7.5v8H2.5z"/><path d="m11.5 4 2-2 2 2M13.5 2v7"/></svg>
}

function ForkTreeIcon({name}:{name:'chevron'|'branch'|'remote'}){
  if(name==='chevron')return <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m5 6 3 3 3-3"/></svg>
  if(name==='branch')return <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="5" cy="4" r="1.5"/><circle cx="5" cy="12" r="1.5"/><circle cx="12" cy="5" r="1.5"/><path d="M5 5.5v5M12 6.5c0 3.4-4 2.2-5.5 4"/></svg>
  return <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="5" cy="8" r="3"/><circle cx="11" cy="8" r="3"/><path d="M8 3v10"/></svg>
}

function GitHeaderControls({mode,onSettings,onMode}:{mode:UiMode;onSettings:()=>void;onMode:(mode:UiMode)=>void}){
  return <><button type="button" className="git-settings-trigger" title="Repository settings" aria-label="Repository settings" onClick={onSettings}><SettingsIcon/></button><span className="git-compact-control" title={mode==='compact'?'Compact modu kapat':'Compact modu aç'}><span>Compact mod</span><button type="button" className={mode==='compact'?'git-compact-toggle active':'git-compact-toggle'} role="switch" aria-checked={mode==='compact'} aria-label="Compact modu aç veya kapat" onClick={()=>onMode(mode==='compact'?'desktop':'compact')}><i/></button></span></>
}

function fetchedLabel(at:number,now:number){
  if(!at)return'Never fetched'
  const minutes=Math.max(0,Math.floor((now-at)/60000))
  if(minutes<1)return'Last fetched just now'
  if(minutes===1)return'Last fetched 1 minute ago'
  if(minutes<60)return`Last fetched ${minutes} minutes ago`
  const hours=Math.floor(minutes/60)
  if(hours===1)return'Last fetched 1 hour ago'
  if(hours<24)return`Last fetched ${hours} hours ago`
  const days=Math.floor(hours/24)
  return`Last fetched ${days} ${days===1?'day':'days'} ago`
}

const commitFileTone=(state:string)=>state==='A'?'added':state==='D'?'deleted':state==='R'||state==='C'?'moved':'changed'

function buildForkGraph(commits:GitCommit[]):ForkGraph{
  let lanes:{hash:string;color:number}[]=[]
  let nextColor=0
  let maxLanes=1
  const rows=new Map<string,ForkGraphRow>()
  for(const commit of commits){
    let currentIndex=lanes.findIndex(lane=>lane.hash===commit.hash)
    if(currentIndex<0){lanes=[{hash:commit.hash,color:nextColor++},...lanes];currentIndex=0}
    const before=lanes.slice()
    const current=before[currentIndex]
    const after=before.filter((_,index)=>index!==currentIndex)
    let insertAt=Math.min(currentIndex,after.length)
    for(const [parentIndex,parent] of commit.parents.entries()){
      if(after.some(lane=>lane.hash===parent))continue
      after.splice(insertAt,0,{hash:parent,color:parentIndex===0?current.color:nextColor++})
      insertAt++
    }
    const segments:ForkGraphSegment[]=[]
    before.forEach((lane,index)=>{
      if(index===currentIndex){segments.push({from:index,to:index,start:0,end:16,color:lane.color});return}
      const target=after.findIndex(next=>next.hash===lane.hash)
      if(target>=0)segments.push({from:index,to:target,start:0,end:32,color:lane.color})
    })
    commit.parents.forEach(parent=>{const target=after.findIndex(lane=>lane.hash===parent);if(target>=0)segments.push({from:currentIndex,to:target,start:16,end:32,color:after[target].color})})
    rows.set(commit.hash,{lane:currentIndex,color:current.color,merge:commit.parents.length>1,segments})
    lanes=after
    maxLanes=Math.max(maxLanes,before.length,after.length)
  }
  return{rows,maxLanes}
}

function buildPathTree(entries:ForkPathEntry[]):ForkPathNode[]{
  type MutableNode={name:string;path:string;entry?:ForkPathEntry;children:Map<string,MutableNode>}
  const root:MutableNode={name:'',path:'',children:new Map()}
  for(const entry of entries){
    const parts=entry.path.replace(/\\/g,'/').split('/').filter(Boolean)
    let parent=root
    parts.forEach((name,index)=>{
      const path=parts.slice(0,index+1).join('/')
      let node=parent.children.get(name)
      if(!node){node={name,path,children:new Map()};parent.children.set(name,node)}
      if(index===parts.length-1)node.entry=entry
      parent=node
    })
  }
  const convert=(node:MutableNode):ForkPathNode=>({name:node.name,path:node.path,entry:node.entry,children:[...node.children.values()].sort((a,b)=>Number(Boolean(a.entry))-Number(Boolean(b.entry))||a.name.localeCompare(b.name)).map(convert)})
  return[...root.children.values()].sort((a,b)=>Number(Boolean(a.entry))-Number(Boolean(b.entry))||a.name.localeCompare(b.name)).map(convert)
}

function ForkCommitGraph({row,width}:{row:ForkGraphRow;width:number}){
  const x=(lane:number)=>7+lane*12
  return <svg className="fork-graph-svg" viewBox={`0 0 ${width} 32`} preserveAspectRatio="none" aria-hidden="true">{row.segments.map((segment,index)=>{const from=x(segment.from),to=x(segment.to),middle=(segment.start+segment.end)/2;return <path key={index} d={`M ${from} ${segment.start} C ${from} ${middle}, ${to} ${middle}, ${to} ${segment.end}`} stroke={`var(--fork-graph-${segment.color%6})`}/>})}<circle className={row.merge?'merge':''} cx={x(row.lane)} cy="16" r={row.merge?4.2:2.5} stroke={`var(--fork-graph-${row.color%6})`}/></svg>
}

function ForkPathTree({entries,selected,onSelect,expandAll=false}:{entries:ForkPathEntry[];selected?:string;onSelect?:(entry:ForkPathEntry)=>void;expandAll?:boolean}){
  const nodes=useMemo(()=>buildPathTree(entries),[entries])
  const signature=entries.map(entry=>entry.path).join('\0')
  const[expanded,setExpanded]=useState<Set<string>>(new Set())
  useEffect(()=>{if(!expandAll){setExpanded(new Set());return}const directories:string[]=[];const visit=(items:ForkPathNode[])=>items.forEach(node=>{if(node.children.length){directories.push(node.path);visit(node.children)}});visit(nodes);setExpanded(new Set(directories))},[expandAll,signature])
  const toggle=(path:string)=>setExpanded(current=>{const next=new Set(current);if(next.has(path))next.delete(path);else next.add(path);return next})
  const render=(items:ForkPathNode[],depth=0):ReactNode=>items.map(node=>node.children.length?<div className="fork-path-directory" key={node.path}><button type="button" className={expanded.has(node.path)?'fork-path-folder open':'fork-path-folder'} style={{paddingLeft:8+depth*14}} onClick={()=>toggle(node.path)}><ForkTreeIcon name="chevron"/><FolderIcon/><b>{node.name}</b></button>{expanded.has(node.path)&&render(node.children,depth+1)}</div>:<button type="button" key={node.path} className={selected===node.path?'fork-path-file active':'fork-path-file'} style={{paddingLeft:25+depth*14}} onClick={()=>node.entry&&onSelect?.(node.entry)}><FileIcon/><span>{node.name}</span>{node.entry?.state&&<i className={`git-badge ${commitFileTone(node.entry.state)}`}>{node.entry.state}</i>}</button>)
  return <div className="fork-path-tree">{render(nodes)}{!nodes.length&&<p>Dosya bulunamadı.</p>}</div>
}

function ForkDetailMeta({commit}:{commit:GitCommit}){
  const initials=commit.author.trim().split(/\s+/).slice(0,2).map(part=>part[0]).join('').toUpperCase()
  return <header className="fork-detail-meta"><i>{initials}</i><b>{commit.author}</b><code>{commit.short}</code><time>{new Date(commit.at).toLocaleString('tr-TR')}</time><strong>{commit.subject}</strong></header>
}

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
  const[commitFiles,setCommitFiles]=useState<CommitFile[]>([])
  const[commitTree,setCommitTree]=useState<string[]>([])
  const[commitTreeLoading,setCommitTreeLoading]=useState(false)
  const[commitFile,setCommitFile]=useState('')
  const[commitStats,setCommitStats]=useState<CommitStats>({additions:0,deletions:0})
  const[historyLoading,setHistoryLoading]=useState(false)
  const[message,setMessage]=useState('')
  const[description,setDescription]=useState('')
  const[fileFilter,setFileFilter]=useState('')
  const[fileFilters,setFileFilters]=useState<FileFilter[]>([])
  const[filterOpen,setFilterOpen]=useState(false)
  const[repoFilter,setRepoFilter]=useState('')
  const[repoOpen,setRepoOpen]=useState(false)
  const[branchFilter,setBranchFilter]=useState('')
  const[forkFilter,setForkFilter]=useState('')
  const[forkDetailTab,setForkDetailTab]=useState<ForkDetailTab>('commit')
  const[forkTreeOpen,setForkTreeOpen]=useState(()=>new Set(['branches','remotes','remote:origin']))
  const[creatingBranch,setCreatingBranch]=useState(false)
  const[mergePicking,setMergePicking]=useState(false)
  const[settingsOpen,setSettingsOpen]=useState(false)
  const[settingsTab,setSettingsTab]=useState<SettingsTab>('remote')
  const[settings,setSettings]=useState<RepositorySettings|null>(null)
  const[settingsBusy,setSettingsBusy]=useState(false)
  const[settingsError,setSettingsError]=useState('')
  const[lastFetchedAt,setLastFetchedAt]=useState(0)
  const[clock,setClock]=useState(()=>Date.now())
  const[operation,setOperation]=useState<GitOperation|null>(null)
  const[completedOperation,setCompletedOperation]=useState<GitOperation|null>(null)
  const[busy,setBusy]=useState(false)
  const[feedback,setFeedback]=useState<Feedback>(null)
  const[newBranch,setNewBranch]=useState('')
  const[branchOpen,setBranchOpen]=useState(false)
  const[lfsPattern,setLfsPattern]=useState('')
  const repoMenu=useRef<HTMLSpanElement>(null)
  const forkRepoMenu=useRef<HTMLDivElement>(null)
  const filterMenu=useRef<HTMLDivElement>(null)
  const activePath=useRef(active)
  const refreshJobs=useRef(new Set<string>())
  const commitRequest=useRef(0)
  const commitTreeRequest=useRef(0)
  const operationLock=useRef(false)
  const completionTimer=useRef<number|undefined>(undefined)
  activePath.current=active

  const refresh=useCallback(async(path:string)=>{
    if(!bridge||!path||refreshJobs.current.has(path))return
    refreshJobs.current.add(path)
    try{
      const result=await bridge.info({cwd:path})
      if(activePath.current!==path)return
      if(result?.ok){setInfo(result as GitInfo);setLastFetchedAt(Number(result.lastFetchedAt)||0)}
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
    setInfo(null);setSelected(null);setCommit(null);setCommitFiles([]);setCommitTree([]);setCommitTreeLoading(false);setCommitFile('');setDiff('');setBranchOpen(false);setRepoOpen(false);setSettingsOpen(false);setLastFetchedAt(0);setOperation(null);setCompletedOperation(null)
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
    const timer=window.setInterval(()=>setClock(Date.now()),30000)
    return()=>window.clearInterval(timer)
  },[])
  useEffect(()=>()=>{if(completionTimer.current!==undefined)window.clearTimeout(completionTimer.current)},[])
  useEffect(()=>{
    if(!repoOpen&&!filterOpen)return
    const outside=(event:PointerEvent)=>{
      if(repoOpen&&!repoMenu.current?.contains(event.target as Node)&&!forkRepoMenu.current?.contains(event.target as Node))setRepoOpen(false)
      if(filterOpen&&!filterMenu.current?.contains(event.target as Node))setFilterOpen(false)
    }
    const escape=(event:KeyboardEvent)=>{if(event.key==='Escape'){setRepoOpen(false);setFilterOpen(false)}}
    document.addEventListener('pointerdown',outside)
    document.addEventListener('keydown',escape)
    return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',escape)}
  },[filterOpen,repoOpen])

  const staged=useMemo(()=>info?.status.files.filter(file=>file.staged)||[],[info])
  const changed=useMemo(()=>info?.status.files.filter(file=>file.unstaged)||[],[info])

  async function act(work:()=>Promise<ActionResult>,done:string):Promise<ActionResult|null>{
    if(!bridge||busy)return null
    setBusy(true);setFeedback(null)
    try{
      const result=await work()
      if(result?.ok)setFeedback({tone:'ok',text:result.output?.split('\n').slice(-2).join(' ')||done})
      else setFeedback({tone:'error',text:result?.auth?`${result.error}\nKimlik doğrulama başarısız — uzak sunucu kullanıcı adı/token istiyor.`:result?.error||'İşlem başarısız.'})
      await refreshStatus(active)
      return result
    }finally{setBusy(false)}
  }

  async function performOperation(kind:GitOperation,work:()=>Promise<ActionResult>,done:string){
    if(operationLock.current)return null
    operationLock.current=true
    setOperation(kind);setCompletedOperation(null)
    const started=performance.now()
    try{
      const result=await act(work,done)
      const remaining=Math.max(0,520-(performance.now()-started))
      if(remaining)await new Promise(resolve=>window.setTimeout(resolve,remaining))
      setOperation(null)
      if(result?.ok){
        setCompletedOperation(kind)
        if(completionTimer.current!==undefined)window.clearTimeout(completionTimer.current)
        completionTimer.current=window.setTimeout(()=>setCompletedOperation(null),900)
      }
      return result
    }finally{operationLock.current=false;setOperation(null)}
  }

  async function runSync(action:'fetch'|'pull'|'push'){
    if(!bridge)return
    const result=await performOperation(action,()=>bridge.sync({cwd:active,action,branch:status?.branch,setUpstream:action==='push'&&!upstream}),`${action} tamam.`)
    if(result?.ok&&action!=='push'){const now=Date.now();setLastFetchedAt(now);setClock(now)}
  }

  async function runCommit(fullMessage:string){
    if(!bridge)return
    const result=await performOperation('commit',()=>bridge.commit({cwd:active,message:fullMessage}),'Commit oluşturuldu.')
    if(result?.ok){setMessage('');setDescription('');void refresh(active)}
  }

  async function openFile(file:GitFile,fromStaged:boolean){
    setCommit(null)
    setSelected({path:file.path,staged:fromStaged,untracked:file.untracked})
    if(!bridge)return
    const result=await bridge.diff({cwd:active,path:file.path,staged:fromStaged,untracked:file.untracked&&!fromStaged})
    setDiff(result?.ok?result.diff:'')
  }
  async function openCommit(item:GitCommit){
    const request=++commitRequest.current
    commitTreeRequest.current++
    setSelected(null);setCommit(item);setCommitFiles([]);setCommitTree([]);setCommitTreeLoading(false);setCommitFile('');setDiff('');setHistoryLoading(true)
    if(!bridge)return
    const result=await bridge.commitFiles({cwd:active,hash:item.hash})
    if(request!==commitRequest.current)return
    if(!result?.ok){setHistoryLoading(false);setFeedback({tone:'error',text:result?.error||'Commit dosyaları okunamadı.'});return}
    const files=result.files as CommitFile[]
    setCommitFiles(files);setCommitStats({additions:result.additions||0,deletions:result.deletions||0})
    const first=files[0]
    if(!first){setHistoryLoading(false);return}
    setCommitFile(first.path)
    const preview=await bridge.commitDiff({cwd:active,hash:item.hash,path:first.path})
    if(request===commitRequest.current){setDiff(preview?.ok?preview.diff:'');setHistoryLoading(false)}
  }
  async function openCommitTree(){
    setForkDetailTab('tree')
    if(!bridge?.commitTree||!commit||commitTree.length||commitTreeLoading)return
    const request=++commitTreeRequest.current
    const hash=commit.hash
    setCommitTreeLoading(true)
    const result=await bridge.commitTree({cwd:active,hash})
    if(request!==commitTreeRequest.current)return
    if(result?.ok)setCommitTree(result.files||[])
    else setFeedback({tone:'error',text:result?.error||'Commit dosya ağacı okunamadı.'})
    setCommitTreeLoading(false)
  }
  async function openCommitFile(file:CommitFile){
    if(!bridge||!commit)return
    const request=++commitRequest.current
    setCommitFile(file.path);setDiff('');setHistoryLoading(true)
    const result=await bridge.commitDiff({cwd:active,hash:commit.hash,path:file.path})
    if(request===commitRequest.current){setDiff(result?.ok?result.diff:'');setHistoryLoading(false)}
  }
  async function showSettings(){
    if(!bridge)return
    setSettingsOpen(true);setSettings(null);setSettingsError('');setSettingsBusy(true)
    const result=await bridge.repositorySettings({cwd:active})
    if(result?.ok)setSettings({remoteUrl:result.remoteUrl||'',ignored:result.ignored||'',userName:result.userName||'',userEmail:result.userEmail||'',lfs:result.lfs||{installed:false,patterns:[],files:[]}})
    else setSettingsError(result?.error||'Repository ayarları yüklenemedi.')
    setSettingsBusy(false)
  }
  async function saveSettings(){
    if(!bridge||!settings||settingsBusy)return
    setSettingsBusy(true);setSettingsError('')
    const result=await bridge.saveRepositorySettings({cwd:active,remoteUrl:settings.remoteUrl,ignored:settings.ignored,userName:settings.userName,userEmail:settings.userEmail})
    if(result?.ok){setSettingsOpen(false);setFeedback({tone:'ok',text:'Repository ayarları kaydedildi.'});await refreshStatus(active)}
    else setSettingsError(result?.error||'Ayarlar kaydedilemedi.')
    setSettingsBusy(false)
  }
  async function changeLfs(action:'install'|'track'|'untrack',pattern?:string){
    if(!bridge||!settings||settingsBusy)return
    setSettingsBusy(true);setSettingsError('')
    const result=await bridge.lfs({cwd:active,action,pattern})
    if(result?.ok&&result.lfs)setSettings(current=>current?{...current,lfs:result.lfs}:current)
    else setSettingsError(result?.error||'LFS ayarı değiştirilemedi.')
    setSettingsBusy(false)
  }
  async function chooseBranch(name:string){
    if(!bridge||busy)return
    if(mergePicking){
      await act(()=>bridge.branch({cwd:active,action:'merge',name}),`${name}, ${status?.branch||'geçerli dal'} dalına birleştirildi.`)
      setMergePicking(false)
    }else{
      await act(()=>bridge.branch({cwd:active,action:'checkout',name}),`${name} dalına geçildi.`)
      setBranchOpen(false)
    }
    await refresh(active)
  }
  async function chooseRemoteBranch(name:string){
    const localName=name.split('/').slice(1).join('/')
    if(!localName)return
    if(!bridge||busy)return
    const existing=(info?.branches.local||[]).find(branch=>branch.name===localName)
    if(existing?.current){setBranchOpen(false);return}
    if(existing){await act(()=>bridge.branch({cwd:active,action:'checkout',name:localName}),`${localName} dalına geçildi.`);setBranchOpen(false);await refresh(active);return}
    const result=await act(()=>bridge.branch({cwd:active,action:'create',name:localName,from:name}),`${localName} dalı oluşturuldu.`)
    if(result?.ok){setBranchOpen(false);await refresh(active)}
  }
  async function createBranch(){
    const name=newBranch.trim()
    if(!bridge||busy||name.length<2)return
    await act(()=>bridge.branch({cwd:active,action:'create',name}),'Dal oluşturuldu.')
    setNewBranch('');setCreatingBranch(false);setBranchOpen(false)
    await refresh(active)
  }
  const toggleForkTree=(key:string)=>setForkTreeOpen(current=>{const next=new Set(current);if(next.has(key))next.delete(key);else next.add(key);return next})
  useEffect(()=>{const first=info?.status.files[0];if(mode==='compact'&&tab==='changes'&&!selected&&first)void openFile(first,first.staged&&!first.unstaged)},[info,mode,tab,selected])
  useEffect(()=>{const first=info?.log[0];if(mode==='desktop'&&tab==='history'&&!commit&&first)void openCommit(first)},[commit,info?.log,mode,tab])

  const status=info?.status
  const upstream=status?.upstream||''
  const canCommit=staged.length>0&&message.trim().length>0
  const chooseMode=(next:UiMode)=>{
    setMode(next)
    if(next==='desktop')setTab('history')
    window.localStorage.setItem(UI_MODE_KEY,next)
  }
  const syncAction=status?.behind?'pull':status?.ahead?'push':'fetch'
  const syncVisualAction:typeof syncAction=operation&&operation!=='commit'?operation:completedOperation&&completedOperation!=='commit'?completedOperation:syncAction
  const syncRunning=operation===syncVisualAction
  const syncComplete=completedOperation===syncVisualAction
  const syncTitle=syncRunning?(syncVisualAction==='fetch'?'Fetching origin…':syncVisualAction==='pull'?'Pulling origin…':'Pushing origin…')
    :syncComplete?(syncVisualAction==='fetch'?'Fetched origin':syncVisualAction==='pull'?'Pulled origin':'Pushed origin')
    :syncVisualAction==='pull'?`Pull origin${status?.behind?` (${status.behind})`:''}`:syncVisualAction==='push'?`Push origin${status?.ahead?` (${status.ahead})`:''}`:'Fetch origin'
  const lastFetchedText=fetchedLabel(lastFetchedAt,clock)
  const visibleFiles=useMemo(()=>{
    const query=fileFilter.trim().toLocaleLowerCase('tr-TR')
    return (status?.files||[]).filter(file=>{
      if(query&&!file.path.toLocaleLowerCase('tr-TR').includes(query))return false
      if(!fileFilters.length)return true
      const state=fileState(file)
      return fileFilters.some(filter=>filter==='included'?file.staged
        :filter==='excluded'?!file.staged
        :filter==='new'?state.tone==='added'
        :filter==='deleted'?state.tone==='deleted'
        :state.tone==='changed'||state.tone==='moved'||state.tone==='conflict')
    })
  },[fileFilter,fileFilters,status?.files])
  const fileFilterOptions=useMemo(()=>({
    included:{label:'Included in commit',count:(status?.files||[]).filter(file=>file.staged).length},
    excluded:{label:'Excluded from commit',count:(status?.files||[]).filter(file=>!file.staged).length},
    new:{label:'New files',count:(status?.files||[]).filter(file=>fileState(file).tone==='added').length},
    modified:{label:'Modified files',count:(status?.files||[]).filter(file=>['changed','moved','conflict'].includes(fileState(file).tone)).length},
    deleted:{label:'Deleted files',count:(status?.files||[]).filter(file=>fileState(file).tone==='deleted').length},
  }),[status?.files])
  const localBranches=info?.branches.local||[]
  const currentBranch=localBranches.find(branch=>branch.current)
  const branchNeedle=branchFilter.trim().toLocaleLowerCase('tr-TR')
  const otherBranches=localBranches.filter(branch=>!branch.current&&(!branchNeedle||branch.name.toLocaleLowerCase('tr-TR').includes(branchNeedle)))
  const forkNeedle=forkFilter.trim().toLocaleLowerCase('tr-TR')
  const forkLocalBranches=localBranches.filter(branch=>!forkNeedle||branch.name.toLocaleLowerCase('tr-TR').includes(forkNeedle))
  const forkRemoteBranches=(info?.branches.remote||[]).filter(branch=>!forkNeedle||branch.name.toLocaleLowerCase('tr-TR').includes(forkNeedle))
  const visibleRepos=useMemo(()=>{
    const query=repoFilter.trim().toLocaleLowerCase('tr-TR')
    return query?repositories.filter(repo=>repo.name.toLocaleLowerCase('tr-TR').includes(query)||repo.path.toLocaleLowerCase('tr-TR').includes(query)):repositories
  },[repoFilter,repositories])
  const forkGraph=useMemo(()=>buildForkGraph(info?.log||[]),[info?.log])
  const forkGraphWidth=Math.max(30,Math.min(112,forkGraph.maxLanes*12+14))
  const forkChangedEntries=useMemo(()=>commitFiles.map(file=>({path:file.path,state:file.state})),[commitFiles])
  const forkTreeEntries=useMemo(()=>commitTree.map(path=>({path})),[commitTree])

  return <section className={`git-panel git-mode-${mode} git-tab-${tab}${branchOpen?' git-branch-open':''}`}>
    {mode==='desktop'&&<>
      <header className="fork-head">
        <div className="fork-toolbar fork-toolbar-left">
          <button type="button" className={operation==='fetch'?'is-running':completedOperation==='fetch'?'is-success':''} disabled={busy||Boolean(operation)} onClick={()=>void runSync('fetch')}><SyncIcon action="fetch"/><span>Fetch</span></button>
          <button type="button" className={operation==='pull'?'is-running':completedOperation==='pull'?'is-success':''} disabled={busy||Boolean(operation)} onClick={()=>void runSync('pull')}><SyncIcon action="pull"/><span>Pull</span></button>
          <button type="button" className={operation==='push'?'is-running':completedOperation==='push'?'is-success':''} disabled={busy||Boolean(operation)} onClick={()=>void runSync('push')}><SyncIcon action="push"/><span>Push</span></button>
          <button type="button" disabled={busy||Boolean(operation)||!status?.files.length} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'push'}),'Zulaya alındı.')}><ForkToolIcon name="stash"/><span>Stash</span></button>
        </div>

        <div className="fork-toolbar-center">
          <div className="fork-repo-menu" ref={forkRepoMenu}>
            <button type="button" className="fork-repo-selector" aria-haspopup="listbox" aria-expanded={repoOpen} onClick={()=>setRepoOpen(value=>!value)}><span>{info?.name||repositories.find(repo=>repo.path===active)?.name||'…'}</span><small><ComposerIcon name="branch"/>{status?.branch||'(dal yok)'}</small><svg className={repoOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
            {repoOpen&&<div className="git-repo-popover fork-repo-popover"><label className="git-repo-filter"><ComposerIcon name="search"/><input autoFocus value={repoFilter} onChange={event=>setRepoFilter(event.target.value)} placeholder="Filter"/></label><header>Repositories</header><div className="git-repo-options" role="listbox">{visibleRepos.map(repo=><button type="button" role="option" aria-selected={repo.path===active} className={repo.path===active?'active':''} key={repo.path} title={repo.path} onClick={()=>{setRepoOpen(false);setRepoFilter('');onSelectRepo(repo.path)}}><RepositoryIcon className="git-repository-icon"/><span>{repo.name}</span>{repo.path===active&&<i/>}</button>)}</div><button type="button" className="git-manage-repos" onClick={()=>{setRepoOpen(false);onClose()}}>Manage repositories</button></div>}
          </div>
          <button type="button" className="fork-branch-selector" onClick={()=>setBranchOpen(value=>!value)}><ComposerIcon name="branch"/><span>Branch</span><small>{status?.branch||'(dal yok)'}</small><svg className={branchOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
          {branchOpen&&<div className="git-branch-popover fork-branch-popover"><header><b>Switch branches</b><button type="button" onClick={()=>setBranchOpen(false)}>×</button></header><div>{localBranches.map(item=><button type="button" key={item.name} className={item.current?'active':''} disabled={busy||item.current} onClick={()=>void chooseBranch(item.name)}><span className="git-inline-icon"><ComposerIcon name="branch"/></span><b>{item.name}</b>{item.current&&<i>✓</i>}</button>)}</div><button type="button" className="new-branch" onClick={()=>{setBranchOpen(false);setTab('branches')}}>＋ New branch</button></div>}
        </div>

        <div className="fork-toolbar fork-toolbar-right">
          <button type="button" onClick={()=>void bridge!.reveal({cwd:active})}><ForkToolIcon name="folder"/><span>Open in</span></button>
          <div className="git-actions fork-common-actions"><GitHeaderControls mode={mode} onSettings={()=>void showSettings()} onMode={chooseMode}/></div>
        </div>
      </header>
    </>}
    <header className="git-head">
      <span className="git-repo" ref={mode==='compact'?repoMenu:undefined}>
        <button type="button" className="git-back" onClick={onClose} title="Depo listesine dön">‹ Depolar</button>
        <button type="button" className="git-desktop-toolbar-button repo-button" aria-haspopup="listbox" aria-expanded={repoOpen} onClick={()=>setRepoOpen(value=>!value)}><RepositoryIcon className="git-toolbar-icon"/><span className="git-toolbar-copy"><small>Current repository</small><b className="git-repo-name" title={info?.root||active}>{info?.name||repositories.find(repo=>repo.path===active)?.name||'…'}</b></span><svg className={repoOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
        {mode==='compact'&&repoOpen&&<div className="git-repo-popover"><label className="git-repo-filter"><ComposerIcon name="search"/><input autoFocus value={repoFilter} onChange={event=>setRepoFilter(event.target.value)} placeholder="Filter"/></label><header>Other</header><div className="git-repo-options" role="listbox">{visibleRepos.map(repo=><button type="button" role="option" aria-selected={repo.path===active} className={repo.path===active?'active':''} key={repo.path} title={repo.path} onClick={()=>{setRepoOpen(false);setRepoFilter('');onSelectRepo(repo.path)}}><RepositoryIcon className="git-repository-icon"/><span>{repo.name}</span>{repo.path===active&&<i/>}</button>)}{!visibleRepos.length&&<p>Repo bulunamadı.</p>}</div><button type="button" className="git-manage-repos" onClick={()=>{setRepoOpen(false);onClose()}}>Tüm depoları yönet</button></div>}
        {mode==='desktop'&&<><b className="git-repo-name" title={info?.root}>{info?.name||'…'}</b><button type="button" className="git-button subtle" title="Listeden çıkar (dosyalar silinmez)" onClick={()=>onForget(active)}>Listeden çıkar</button></>}
      </span>
      <span className="git-branchbar">
        {status&&<>
          <button type="button" className="git-desktop-toolbar-button branch-button" onClick={()=>setBranchOpen(value=>!value)}><span className="git-toolbar-icon"><ComposerIcon name="branch"/></span><span className="git-toolbar-copy"><small>Current branch</small><b title={info?.root}>{status.branch||'(dal yok)'}</b></span><svg className={branchOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button>
          {mode==='desktop'&&<>{upstream&&<small>↑{status.ahead} ↓{status.behind} · {upstream}</small>}{!upstream&&<small>uzak dal yok</small>}</>}
        </>}
        {branchOpen&&mode==='desktop'&&<div className="git-branch-popover"><header><b>Switch branches</b><button onClick={()=>setBranchOpen(false)}>×</button></header><div>{localBranches.map(item=><button key={item.name} className={item.current?'active':''} disabled={busy||item.current} onClick={()=>void chooseBranch(item.name)}><span className="git-inline-icon"><ComposerIcon name="branch"/></span><b>{item.name}</b>{item.current&&<i>✓</i>}</button>)}</div><button className="new-branch" onClick={()=>{setBranchOpen(false);setTab('branches')}}>＋ New branch</button></div>}
      </span>
      <span className="git-actions">
        {mode==='compact'?<button type="button" className={`git-desktop-sync sync-${syncVisualAction}${syncRunning?' is-running':''}${syncComplete?' is-success':''}`} disabled={busy||Boolean(operation)||!active} onClick={()=>void runSync(syncAction)}><span className="git-sync-icon"><SyncIcon action={syncVisualAction}/></span><span className="git-toolbar-copy"><b>{syncTitle}</b><small>{lastFetchedText}</small></span></button>:<><button type="button" className="git-button" disabled={busy||Boolean(operation)||!active} onClick={()=>void runSync('fetch')}>Fetch</button><button type="button" className="git-button" disabled={busy||Boolean(operation)||!active} onClick={()=>void runSync('pull')}>Pull{status?.behind?` (${status.behind})`:''}</button><button type="button" className="git-button primary" disabled={busy||Boolean(operation)||!active} onClick={()=>void runSync('push')}>Push{status?.ahead?` (${status.ahead})`:''}</button></>}
        <GitHeaderControls mode={mode} onSettings={()=>void showSettings()} onMode={chooseMode}/>
      </span>
    </header>

    {feedback&&<p className={feedback.tone==='ok'?'git-note':'git-note error'}>{feedback.text}</p>}

    {active&&<div className="git-body">
      <aside className="git-side">
        {mode==='desktop'&&<div className="fork-sidebar">
          <header><b title={info?.root}>{info?.name||'Repository'}</b><button type="button" title="Repository settings" onClick={()=>void showSettings()}><SettingsIcon/></button></header>
          <nav className="fork-sidebar-main">
            <button type="button" className={tab==='changes'?'active':''} onClick={()=>{setTab('changes');setBranchOpen(false)}}><FileIcon/><span>Local Changes</span>{Boolean(status?.files.length)&&<em>{status!.files.length}</em>}</button>
            <button type="button" className={tab==='history'?'active':''} onClick={()=>{setTab('history');setBranchOpen(false)}}><span className="fork-commits-icon">▤</span><span>All Commits</span></button>
          </nav>
          <label className="fork-tree-filter"><ComposerIcon name="search"/><input value={forkFilter} onChange={event=>setForkFilter(event.target.value)} placeholder="Filter"/></label>
          <div className="fork-tree">
            <section>
              <button type="button" className="fork-tree-heading" aria-expanded={forkTreeOpen.has('branches')} onClick={()=>toggleForkTree('branches')}><ForkTreeIcon name="chevron"/><b>Branches</b></button>
              {forkTreeOpen.has('branches')&&<div className="fork-tree-children">{forkLocalBranches.map(branch=><button type="button" key={branch.name} className={branch.current?'fork-tree-leaf current':'fork-tree-leaf'} disabled={busy||branch.current} onClick={()=>void chooseBranch(branch.name)}>{branch.current?<span className="fork-current-check">✓</span>:<ForkTreeIcon name="branch"/>}<em>{branch.name}</em></button>)}</div>}
            </section>
            <section>
              <button type="button" className="fork-tree-heading" aria-expanded={forkTreeOpen.has('remotes')} onClick={()=>toggleForkTree('remotes')}><ForkTreeIcon name="chevron"/><b>Remotes</b></button>
              {forkTreeOpen.has('remotes')&&<div className="fork-tree-children fork-remote-list">{info?.remotes.map(remote=>{const remoteKey=`remote:${remote.name}`;return <div className="fork-remote" key={remote.name}><button type="button" className="fork-tree-heading remote" aria-expanded={forkTreeOpen.has(remoteKey)} onClick={()=>toggleForkTree(remoteKey)}><ForkTreeIcon name="chevron"/><ForkTreeIcon name="remote"/><b>{remote.name}</b></button>{forkTreeOpen.has(remoteKey)&&<div className="fork-tree-children">{forkRemoteBranches.filter(branch=>branch.name.startsWith(`${remote.name}/`)).map(branch=><button type="button" className="fork-tree-leaf" key={branch.name} disabled={busy} onClick={()=>void chooseRemoteBranch(branch.name)}><ForkTreeIcon name="branch"/><em>{branch.name.split('/').slice(1).join('/')}</em></button>)}</div>}</div>})}</div>}
            </section>
            <div className="fork-tree-static"><b>Tags</b></div>
            <div className="fork-tree-static"><b>Stashes</b>{Boolean(status?.stashes)&&<small>{status!.stashes}</small>}</div>
            <div className="fork-tree-static"><b>Submodules</b></div>
          </div>
        </div>}
        <div className="git-tabs">
          <button type="button" className={tab==='changes'?'active':undefined} onClick={()=>{setTab('changes');setBranchOpen(false)}}>{mode==='compact'?'Changes':'Değişiklikler'} <em>{status?.files.length||0}</em></button>
          <button type="button" className={tab==='history'?'active':undefined} onClick={()=>{setTab('history');setBranchOpen(false)}}>{mode==='compact'?'History':'Geçmiş'}</button>
          <button type="button" className={tab==='branches'?'active':undefined} onClick={()=>setTab('branches')}>Dallar</button>
        </div>

        {tab==='changes'&&mode==='compact'&&<div className="git-files desktop-files">
          <div className="desktop-filter" ref={filterMenu}><button type="button" className={fileFilters.length?'desktop-filter-button active':'desktop-filter-button'} aria-label="Filtre seçenekleri" aria-expanded={filterOpen} onClick={()=>setFilterOpen(value=>!value)}><svg viewBox="0 0 18 18" aria-hidden="true"><path d="M3 5h12M5.5 9h7M7.5 13h3"/></svg><svg className={filterOpen?'git-chevron open':'git-chevron'} viewBox="0 0 16 16" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg></button><label><ComposerIcon name="search"/><input value={fileFilter} onChange={event=>setFileFilter(event.target.value)} placeholder="Filter" aria-label="Değişen dosyaları filtrele"/></label>{filterOpen&&<div className="desktop-filter-popover"><header><b>Filter Options</b><button type="button" onClick={()=>setFilterOpen(false)}>×</button></header>{(Object.keys(fileFilterOptions) as FileFilter[]).map(key=><label key={key}><input type="checkbox" checked={fileFilters.includes(key)} onChange={()=>setFileFilters(current=>current.includes(key)?current.filter(item=>item!==key):[...current,key])}/><span>{fileFilterOptions[key].label} ({fileFilterOptions[key].count})</span></label>)}{fileFilters.length>0&&<button type="button" className="desktop-filter-clear" onClick={()=>setFileFilters([])}>Clear filters</button>}</div>}</div>
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

        {tab==='history'&&mode==='compact'&&<div className="git-log">
          {mode==='compact'&&<button type="button" className="git-history-compare" disabled><span className="git-inline-icon"><ComposerIcon name="branch"/></span>No branches to compare</button>}
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
            <button type="button" disabled={busy} onClick={()=>void chooseRemoteBranch(item.name)}>
              <b>{item.name}</b><small>{relativeTime(item.at)}</small>
            </button>
          </div>)}
        </div>}

      </aside>

      {mode==='compact'&&branchOpen&&<aside className="git-branch-drawer">
        <div className="git-branch-tools"><label><ComposerIcon name="search"/><input autoFocus value={branchFilter} onChange={event=>setBranchFilter(event.target.value)} placeholder="Filter"/></label><button type="button" onClick={()=>setCreatingBranch(value=>!value)}>New branch</button></div>
        {creatingBranch&&<form className="git-branch-create" onSubmit={event=>{event.preventDefault();void createBranch()}}><input autoFocus value={newBranch} placeholder="new-branch-name" onChange={event=>setNewBranch(event.target.value.replace(/\s+/g,'-'))}/><button type="submit" disabled={busy||newBranch.trim().length<2}>Create</button></form>}
        <div className="git-branch-list">
          {currentBranch&&(!branchNeedle||currentBranch.name.toLocaleLowerCase('tr-TR').includes(branchNeedle))&&<><header>Default branch</header><button type="button" className="current" disabled><span>✓</span><b>{currentBranch.name}</b><small>{relativeTime(currentBranch.at)}</small></button></>}
          {otherBranches.length>0&&<><header>Other branches</header>{otherBranches.map(branch=><button type="button" key={branch.name} disabled={busy} onClick={()=>void chooseBranch(branch.name)}><span className="git-inline-icon"><ComposerIcon name="branch"/></span><b>{branch.name}</b><small>{relativeTime(branch.at)}</small></button>)}</>}
          {!currentBranch&&!otherBranches.length&&<p>Filtreyle eşleşen dal yok.</p>}
        </div>
        <button type="button" className={mergePicking?'git-merge-branch active':'git-merge-branch'} onClick={()=>setMergePicking(value=>!value)}><ComposerIcon name="branch"/>{mergePicking?'Birleştirilecek dalı seç':'Choose a branch to merge into'} <b>{status?.branch||'current branch'}</b></button>
      </aside>}

      <div className="git-main">
        {mode==='desktop'&&tab==='history'?<div className="fork-history-workspace">
          <section className="fork-commit-table" style={{'--fork-graph-width':`${forkGraphWidth}px`} as CSSProperties}>
            <header><span>Graph</span><span>Commit</span><span>Author</span><span>SHA</span><span>Date</span></header>
            <div className="fork-commit-rows">
              {(info?.log||[]).map(item=><button type="button" key={item.hash} className={commit?.hash===item.hash?'active':''} onClick={()=>{setForkDetailTab('commit');void openCommit(item)}}>
                <span className="fork-graph">{forkGraph.rows.get(item.hash)&&<ForkCommitGraph row={forkGraph.rows.get(item.hash)!} width={forkGraphWidth}/>}</span>
                <span className="fork-commit-subject">{item.refs.length>0&&<span className="fork-row-refs">{item.refs.slice(0,2).map(ref=><em key={ref}>{ref.replace('HEAD -> ','')}</em>)}</span>}<b>{item.subject}</b></span>
                <span className="fork-author"><i>{item.author.trim().split(/\s+/).slice(0,2).map(part=>part[0]).join('').toUpperCase()}</i>{item.author}</span>
                <code>{item.short}</code>
                <time dateTime={new Date(item.at).toISOString()}>{new Date(item.at).toLocaleString('tr-TR',{day:'2-digit',month:'short',year:'numeric',hour:'2-digit',minute:'2-digit'})}</time>
              </button>)}
              {!info?.log.length&&<p className="git-empty">Henüz commit yok.</p>}
            </div>
          </section>
          <section className="fork-inspector">
            <nav>
              <button type="button" className={forkDetailTab==='commit'?'active':''} onClick={()=>setForkDetailTab('commit')}>Commit</button>
              <button type="button" className={forkDetailTab==='changes'?'active':''} onClick={()=>setForkDetailTab('changes')}>Changes <em>{commitFiles.length}</em></button>
              <button type="button" className={forkDetailTab==='tree'?'active':''} onClick={()=>void openCommitTree()}>File Tree</button>
            </nav>
            {!commit?<p className="git-empty">İncelemek için bir commit seç.</p>:forkDetailTab==='commit'?<div className="fork-commit-overview">
              <div className="fork-author-card"><span>{commit.author.trim().split(/\s+/).slice(0,2).map(part=>part[0]).join('').toUpperCase()}</span><div><b>{commit.author}</b><small>{commit.email}</small><time>{new Date(commit.at).toLocaleString('tr-TR')}</time></div></div>
              <dl><dt>REFS</dt><dd>{commit.refs.length?commit.refs.map(ref=><em key={ref}>{ref.replace('HEAD -> ','')}</em>):'—'}</dd><dt>SHA</dt><dd><code>{commit.hash}</code><button type="button" title="SHA kopyala" onClick={()=>void navigator.clipboard.writeText(commit.hash)}><CopyIcon/></button></dd><dt>PARENTS</dt><dd>{commit.parents.length?commit.parents.map(parent=><code key={parent}>{parent.slice(0,7)}</code>):'—'}</dd></dl>
              <div className="fork-commit-message"><b>{commit.subject}</b><span><i>+{commitStats.additions}</i><em>−{commitStats.deletions}</em></span></div>
              <div className="fork-overview-files">{historyLoading&&!commitFiles.length?<p>Dosyalar yükleniyor…</p>:commitFiles.map(file=><button type="button" key={`${file.state}-${file.path}`} title={file.path} onClick={()=>{setForkDetailTab('changes');void openCommitFile(file)}}><i className={`git-badge ${commitFileTone(file.state)}`}>{file.state}</i><span>{file.path}</span></button>)}</div>
            </div>:forkDetailTab==='changes'?<div className="fork-detail-pane"><ForkDetailMeta commit={commit}/><div className="fork-change-detail">
              <aside><ForkPathTree key={`${commit.hash}:changes`} entries={forkChangedEntries} selected={commitFile} expandAll onSelect={entry=>{const file=commitFiles.find(item=>item.path===entry.path);if(file)void openCommitFile(file)}}/></aside>
              <section><header>{commitFile||'Dosya seç'}</header>{historyLoading?<p className="git-empty">Diff yükleniyor…</p>:<GitDiff diff={diff} empty={commitFile?'Bu dosyada gösterilecek metin farkı yok.':'Commit içinden bir dosya seç.'}/>}</section>
            </div></div>:<div className="fork-detail-pane"><ForkDetailMeta commit={commit}/><div className="fork-tree-detail"><aside>{commitTreeLoading?<p className="fork-tree-loading">Dosya ağacı yükleniyor…</p>:<ForkPathTree key={`${commit.hash}:tree`} entries={forkTreeEntries}/>}</aside><section/></div></div>}
          </section>
        </div>:mode==='compact'&&tab==='history'?<div className="git-history-detail">
          {commit?<>
            <header className="git-history-head"><div><b>{commit.subject}</b><span>{commit.author} · {relativeTime(commit.at)} <code>{commit.short}</code><button type="button" title="Commit kimliğini kopyala" onClick={()=>void navigator.clipboard.writeText(commit.hash)}><CopyIcon/></button></span></div><strong><i>+{commitStats.additions}</i><em>−{commitStats.deletions}</em></strong></header>
            <aside className="git-history-files"><header>{commitFiles.length} changed files</header>{commitFiles.map(file=><button type="button" className={commitFile===file.path?'active':''} key={`${file.state}-${file.path}`} title={file.path} onClick={()=>void openCommitFile(file)}><span>{file.path}</span><i className={`git-badge ${commitFileTone(file.state)}`}>{file.state}</i></button>)}</aside>
            <section className="git-history-preview"><header><span>{commitFile||'Dosya seç'}</span><button type="button" title="Repository settings" aria-label="Repository settings" onClick={()=>void showSettings()}><SettingsIcon/></button></header>{historyLoading?<p className="git-empty">Diff yükleniyor…</p>:<GitDiff diff={diff} empty={commitFile?'Bu dosyada gösterilecek metin farkı yok.':'Commit içinden bir dosya seç.'}/>}</section>
          </>:<p className="git-empty git-history-empty">Geçmişten bir commit seç.</p>}
        </div>:<div className="git-preview">
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
        </div>}

        <form className="git-commit" onSubmit={event=>{
          event.preventDefault()
          if(!canCommit)return
          const fullMessage=`${message.trim()}${description.trim()?`\n\n${description.trim()}`:''}`
          void runCommit(fullMessage)
        }}>
          {mode==='compact'?<div className="desktop-commit-fields"><input value={message} placeholder={staged.length?'Summary (required)':'Select files to commit'} onChange={event=>setMessage(event.target.value)} onKeyDown={event=>{if(event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&canCommit){event.preventDefault();event.currentTarget.form?.requestSubmit()}}}/><textarea value={description} rows={4} placeholder="Description" onChange={event=>setDescription(event.target.value)}/></div>:<textarea value={message} rows={3} placeholder={staged.length?'Commit mesajı — ilk satır özet, boş satırdan sonra ayrıntı':'Önce değişiklik sahnele'}
            onChange={event=>setMessage(event.target.value)}
            onKeyDown={event=>{if(event.key==='Enter'&&(event.ctrlKey||event.metaKey)&&canCommit){event.preventDefault();event.currentTarget.form?.requestSubmit()}}}/>}
          <div className="git-commit-actions">
            <small>{staged.length?`${staged.length} dosya sahnelendi · Ctrl+Enter`:'Sahnelenmiş dosya yok'}</small>
            <button type="button" className="git-button" disabled={busy||!status?.files.length} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'push'}),'Zulaya alındı.')}>Zulala</button>
            {(status?.stashes||0)>0&&<button type="button" className="git-button" disabled={busy} onClick={()=>void act(()=>bridge!.stash({cwd:active,action:'pop'}),'Zuladan alındı.')}>Zuladan al ({status?.stashes})</button>}
            <button type="submit" className={`git-button primary git-commit-submit${operation==='commit'?' is-running':''}${completedOperation==='commit'?' is-success':''}`} disabled={busy||Boolean(operation)||!canCommit}>{operation==='commit'?<><span className="git-operation-spinner"/>Committing…</>:completedOperation==='commit'?<><span className="git-operation-check">✓</span>Committed</>:mode==='compact'?`Commit ${staged.length} ${staged.length===1?'file':'files'} to ${status?.branch||'current branch'}`:<><ComposerIcon name="send"/> Commit</>}</button>
          </div>
        </form>
      </div>
    </div>}

    {settingsOpen&&<div className="git-settings-backdrop" role="presentation" onMouseDown={event=>{if(event.target===event.currentTarget&&!settingsBusy)setSettingsOpen(false)}}><section className="git-settings-dialog" role="dialog" aria-modal="true" aria-label="Repository settings"><header><b>Repository settings</b><button type="button" aria-label="Kapat" disabled={settingsBusy} onClick={()=>setSettingsOpen(false)}>×</button></header><div className="git-settings-layout"><nav>{([{id:'remote',label:'Remote'},{id:'ignored',label:'Ignored files'},{id:'config',label:'Git config'},{id:'lfs',label:'Git LFS'}] as {id:SettingsTab;label:string}[]).map(item=><button type="button" className={settingsTab===item.id?'active':''} key={item.id} onClick={()=>setSettingsTab(item.id)}><span>{item.id==='remote'?<ComposerIcon name="link"/>:item.id==='ignored'?<FileIcon/>:item.id==='config'?<SettingsIcon/>:<ComposerIcon name="code"/>}</span>{item.label}</button>)}</nav><main>{settingsBusy&&!settings&&<p className="git-settings-loading">Ayarlar yükleniyor…</p>}{settingsError&&<p className="git-settings-error">{settingsError}</p>}{settings&&settingsTab==='remote'&&<label className="git-settings-field"><span>Primary remote repository (origin) URL</span><input value={settings.remoteUrl} onChange={event=>setSettings(current=>current?{...current,remoteUrl:event.target.value}:current)} placeholder="https://example.com/owner/repository.git"/></label>}{settings&&settingsTab==='ignored'&&<label className="git-settings-field grow"><span>.gitignore</span><textarea spellCheck={false} value={settings.ignored} onChange={event=>setSettings(current=>current?{...current,ignored:event.target.value}:current)} placeholder={'node_modules/\ndist/\n.env'}/><small>Her satıra yok sayılacak bir dosya, klasör veya desen yaz.</small></label>}{settings&&settingsTab==='config'&&<div className="git-settings-fields"><label className="git-settings-field"><span>Commit author name</span><input value={settings.userName} onChange={event=>setSettings(current=>current?{...current,userName:event.target.value}:current)} placeholder="Ad Soyad"/></label><label className="git-settings-field"><span>Commit author email</span><input type="email" value={settings.userEmail} onChange={event=>setSettings(current=>current?{...current,userEmail:event.target.value}:current)} placeholder="name@example.com"/></label></div>}{settings&&settingsTab==='lfs'&&<div className="git-settings-lfs"><p>{settings.lfs.installed?'Git LFS bu repository için kullanılabilir.':'Git LFS henüz kurulu değil.'}</p>{!settings.lfs.installed&&<button type="button" disabled={settingsBusy} onClick={()=>void changeLfs('install')}>Install Git LFS</button>}{settings.lfs.installed&&<><form onSubmit={event=>{event.preventDefault();const pattern=lfsPattern.trim();if(pattern){void changeLfs('track',pattern);setLfsPattern('')}}}><input value={lfsPattern} onChange={event=>setLfsPattern(event.target.value)} placeholder="*.psd"/><button type="submit" disabled={settingsBusy||!lfsPattern.trim()}>Track pattern</button></form><div>{settings.lfs.patterns.map(pattern=><span key={pattern}><code>{pattern}</code><button type="button" disabled={settingsBusy} onClick={()=>void changeLfs('untrack',pattern)}>×</button></span>)}{!settings.lfs.patterns.length&&<small>İzlenen LFS deseni yok.</small>}</div></>}</div>}</main></div><footer><button type="button" className="primary" disabled={settingsBusy||!settings} onClick={()=>void saveSettings()}>{settingsBusy?'Kaydediliyor…':'Save'}</button><button type="button" disabled={settingsBusy} onClick={()=>setSettingsOpen(false)}>Cancel</button></footer></section></div>}
  </section>
}
