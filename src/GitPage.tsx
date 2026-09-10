import { useCallback, useEffect, useState } from 'react'
import GitConnect from './GitConnect'
import GitPanel from './GitPanel'
import ProviderMark, { PROVIDER_LABEL } from './ProviderMarks'
import { gitAvailable, gitBridge, loadRepos, saveRepos, type GitConnection, type RemoteRepo, type Repo } from './gitClient'

// Kaynak kodu sayfası. En hızlı yol yukarıdaki adres kutusu: depo adresini
// yapıştır, gerekiyorsa tarayıcıda bir kez onayla, klonlama kendiliğinden sürer.
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
  const[address,setAddress]=useState('')
  const[step,setStep]=useState('')

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

  // Yapıştırılan adres: sunucuyu tanı, gerekiyorsa giriş yaptır, sonra klonla.
  async function cloneAddress(){
    if(!bridge||!address.trim())return
    const url=address.trim()
    setNotice('');setStep('Sunucu tanınıyor…')
    const found=await bridge.detect({url})
    if(found?.ok&&!found.connected&&found.kind)setStep(`${PROVIDER_LABEL[found.kind as GitConnection['kind']]||'Sunucu'} tarayıcıda açıldı — onayla, sonrası kendiliğinden.`)
    else setStep('Klonlanıyor…')
    const result=await bridge.cloneUrl({url})
    setStep('')
    if(!result){setNotice('');return}
    if(!result.ok){setNotice(result.error);return}
    setAddress('')
    if(result.connection){
      const fresh=result.connection as GitConnection
      setConnections(current=>[fresh,...current.filter(item=>item.id!==fresh.id)])
      void loadRemote(fresh)
    }
    remember({path:result.path,name:result.name,remote:url.replace(/\.git$/,'')})
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

  const pasteBox=<div className="git-address">
    <ProviderMark kind="git" size={18}/>
    <input value={address} placeholder="Depo adresini yapıştır — https://… ya da http://…/proje.git" disabled={Boolean(step)}
      onChange={event=>setAddress(event.target.value)} onKeyDown={event=>{if(event.key==='Enter')void cloneAddress()}}/>
    <button type="button" className="git-button primary" disabled={!address.trim()||Boolean(step)} onClick={()=>void cloneAddress()}>
      {step?'Sürüyor…':'Klonla'}
    </button>
  </div>

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
    <div className="git-page-lead">
      <h2>Kaynak kodu</h2>
      <p>Bir depo adresi yapıştır — giriş gerekiyorsa tarayıcıda tek onayla halledilir.</p>
      {pasteBox}
      {step&&<p className="git-hint">{step}</p>}
      {notice&&<p className="git-note error">{notice}</p>}
    </div>
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

    {pasteBox}
    {step&&<p className="git-hint">{step}</p>}
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
          {connection.avatar?<img src={connection.avatar} alt=""/>:<ProviderMark kind={connection.kind} size={20}/>}
          {connection.name||connection.login}
          <small>{PROVIDER_LABEL[connection.kind]} · {connection.host.replace(/^https?:\/\//,'')}</small>
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
