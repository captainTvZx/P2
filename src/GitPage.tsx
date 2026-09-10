import { useCallback, useEffect, useState } from 'react'
import GitConnect from './GitConnect'
import GitPanel from './GitPanel'
import { gitAvailable, gitBridge, loadRepos, saveRepos, type GitConnection, type RemoteRepo, type Repo } from './gitClient'

// Kaynak kodu sayfası. Açılışta bağlı hesapları yükler; hiç bağlantı yoksa önce
// sağlayıcı bağlama ekranı çıkar, sonra erişimi olan depolar listelenir.
export default function GitPage(){
  const bridge=gitBridge()
  const[connections,setConnections]=useState<GitConnection[]>([])
  const[ready,setReady]=useState(false)
  const[repos,setRepos]=useState<Record<string,RemoteRepo[]>>({})
  const[loading,setLoading]=useState<string|null>(null)
  const[errors,setErrors]=useState<Record<string,string>>({})
  const[local,setLocal]=useState<Repo[]>(()=>gitAvailable()?loadRepos():[])
  const[open,setOpen]=useState<string>('')
  const[search,setSearch]=useState('')
  const[adding,setAdding]=useState(false)
  const[cloning,setCloning]=useState<string|null>(null)
  const[notice,setNotice]=useState('')

  const loadRemote=useCallback(async(connection:GitConnection)=>{
    if(!bridge)return
    setLoading(connection.id)
    const result=await bridge.remoteRepos({id:connection.id})
    setLoading(null)
    if(result?.ok)setRepos(current=>({...current,[connection.id]:result.repos}))
    else setErrors(current=>({...current,[connection.id]:result?.error||'Depolar alınamadı.'}))
  },[bridge])

  useEffect(()=>{
    if(!bridge){setReady(true);return}
    void bridge.connections().then(rows=>{
      setConnections(rows as GitConnection[])
      setReady(true)
      ;(rows as GitConnection[]).forEach(row=>void loadRemote(row))
    })
  },[bridge,loadRemote])

  function remember(repo:Repo){
    setLocal(current=>{
      const next=[repo,...current.filter(item=>item.path!==repo.path)]
      saveRepos(next)
      return next
    })
    setOpen(repo.path)
  }

  async function cloneRemote(repo:RemoteRepo){
    if(!bridge)return
    setCloning(repo.id);setNotice('')
    try{
      const result=await bridge.clone({url:repo.url,name:repo.name})
      if(!result)return
      if(result.ok)remember({path:result.path,name:result.name,remote:repo.fullName})
      else setNotice(result.error)
    }finally{setCloning(null)}
  }

  async function openFolder(){
    if(!bridge)return
    const picked=await bridge.pickFolder()
    if(!picked)return
    if(!picked.ok){setNotice(picked.error);return}
    remember({path:picked.path,name:picked.name})
  }

  if(!gitAvailable())return <section className="git-page">
    <div className="git-page-empty">
      <h2>Kaynak kodu</h2>
      <p>Git istemcisi yalnızca masaüstü uygulamasında çalışır: depolar senin diskinde durur ve komutlar kendi git kurulumunla koşar.</p>
    </div>
  </section>

  // Bir depo açıldıysa sayfa tamamen o depoya döner.
  if(open)return <GitPanel repoPath={open} onClose={()=>setOpen('')} onForget={path=>{
    setLocal(current=>{const next=current.filter(item=>item.path!==path);saveRepos(next);return next})
    setOpen('')
  }}/>

  if(!ready)return <section className="git-page"><p className="git-empty">Yükleniyor…</p></section>

  if(!connections.length)return <section className="git-page">
    <GitConnect connections={connections}
      onDisconnect={id=>{void bridge!.disconnect({id});setConnections(current=>current.filter(item=>item.id!==id))}}
      onConnected={connection=>{setConnections([connection]);void loadRemote(connection)}}/>
    <div className="git-page-alt">
      <span>Hesap bağlamadan da diskindeki bir depoyu açabilirsin.</span>
      <button type="button" className="git-button" onClick={()=>void openFolder()}>Klasör aç</button>
    </div>
  </section>

  const needle=search.trim().toLocaleLowerCase('tr-TR')
  const matches=(repo:RemoteRepo)=>!needle||repo.fullName.toLocaleLowerCase('tr-TR').includes(needle)||repo.description.toLocaleLowerCase('tr-TR').includes(needle)

  return <section className="git-page">
    <header className="git-page-head">
      <div>
        <h2>Kaynak kodu</h2>
        <p>Erişimin olan depolar. Klonladığın depo diskinde durur, buradan yönetirsin.</p>
      </div>
      <div className="git-page-head-actions">
        <div className="admin-search"><span>⌕</span><input value={search} onChange={event=>setSearch(event.target.value)} placeholder="Depo ara"/></div>
        <button type="button" className="git-button" onClick={()=>void openFolder()}>Klasör aç</button>
        <button type="button" className="git-button" onClick={()=>setAdding(value=>!value)}>{adding?'Kapat':'Hesap ekle'}</button>
      </div>
    </header>

    {notice&&<p className="git-note error">{notice}</p>}

    {adding&&<GitConnect compact connections={connections}
      onConnected={connection=>{setConnections(current=>[connection,...current.filter(item=>item.id!==connection.id)]);setAdding(false);void loadRemote(connection)}}
      onDisconnect={id=>{void bridge!.disconnect({id});setConnections(current=>current.filter(item=>item.id!==id));setRepos(current=>{const next={...current};delete next[id];return next})}}/>}

    {local.length>0&&<section className="git-block">
      <h3>Bu bilgisayarda · {local.length}</h3>
      <div className="git-cards">
        {local.map(repo=><button type="button" className="git-card local" key={repo.path} onClick={()=>setOpen(repo.path)}>
          <b>{repo.name}</b>
          <small title={repo.path}>{repo.remote||repo.path}</small>
          <em>Aç</em>
        </button>)}
      </div>
    </section>}

    {connections.map(connection=>{
      const list=(repos[connection.id]||[]).filter(matches)
      const cloned=new Set(local.map(item=>item.name))
      return <section className="git-block" key={connection.id}>
        <h3>
          {connection.avatar?<img src={connection.avatar} alt=""/>:null}
          {connection.name||connection.login}
          <small>{connection.kind==='github'?'GitHub':connection.kind==='gitlab'?'GitLab':'Gitea'} · {connection.host.replace(/^https?:\/\//,'')}</small>
          <button type="button" className="git-button subtle" onClick={()=>void loadRemote(connection)}>Yenile</button>
        </h3>
        {loading===connection.id&&<p className="git-empty small">Depolar alınıyor…</p>}
        {errors[connection.id]&&<p className="git-note error">{errors[connection.id]}</p>}
        <div className="git-cards">
          {list.map(repo=><div className="git-card" key={repo.id}>
            <b>{repo.fullName}{repo.private&&<i className="git-private">özel</i>}</b>
            <small>{repo.description||'Açıklama yok'}</small>
            <span className="git-card-actions">
              <button type="button" className="git-button primary" disabled={cloning===repo.id} onClick={()=>void cloneRemote(repo)}>
                {cloning===repo.id?'Klonlanıyor…':cloned.has(repo.name)?'Yeniden klonla':'Klonla'}
              </button>
              {repo.web&&<a className="git-button subtle" href={repo.web} target="_blank" rel="noreferrer">Web</a>}
            </span>
          </div>)}
          {!loading&&!list.length&&!errors[connection.id]&&<p className="git-empty small">{needle?'Aramayla eşleşen depo yok.':'Bu hesapta depo görünmüyor.'}</p>}
        </div>
      </section>
    })}
  </section>
}
