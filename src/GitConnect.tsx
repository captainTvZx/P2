import { useState } from 'react'
import type { GitConnection, ProviderKind } from './gitClient'

// Sağlayıcı bağlama ekranı. Jeton yalnızca ana sürece gider ve orada şifreli
// saklanır; arayüz jetonu hiçbir yerde tutmaz.
const PROVIDERS:Array<{kind:ProviderKind;name:string;host:string;hint:string;scopes:string;tokenUrl:(host:string)=>string}>=[
  {kind:'github',name:'GitHub',host:'https://github.com',hint:'github.com hesabın',scopes:'repo',tokenUrl:()=>'https://github.com/settings/tokens/new?scopes=repo&description=P2'},
  {kind:'gitlab',name:'GitLab',host:'https://gitlab.com',hint:'gitlab.com ya da kendi sunucun',scopes:'api, read_repository, write_repository',tokenUrl:host=>`${host.replace(/\/+$/,'')}/-/user_settings/personal_access_tokens`},
  {kind:'gitea',name:'Gitea',host:'',hint:'kendi Gitea sunucun',scopes:'repository, user',tokenUrl:host=>`${host.replace(/\/+$/,'')}/user/settings/applications`},
]

export default function GitConnect({connections,onConnected,onDisconnect,compact}:{
  connections:GitConnection[]
  onConnected:(connection:GitConnection)=>void
  onDisconnect:(id:string)=>void
  compact?:boolean
}){
  const[kind,setKind]=useState<ProviderKind>('github')
  const[host,setHost]=useState('')
  const[token,setToken]=useState('')
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  const provider=PROVIDERS.find(item=>item.kind===kind)!
  const effectiveHost=kind==='github'?'https://github.com':(host.trim()||provider.host)

  async function connect(){
    const bridge=window.p2?.git
    if(!bridge||busy)return
    setBusy(true);setError('')
    try{
      const result=await bridge.connect({kind,host:effectiveHost,token:token.trim()})
      if(result?.ok){setToken('');setHost('');onConnected(result.connection)}
      else setError(result?.error||'Bağlanılamadı.')
    }finally{setBusy(false)}
  }

  return <div className={compact?'git-connect compact':'git-connect'}>
    {!compact&&<header>
      <h2>Kaynak kodunu bağla</h2>
      <p>GitHub, GitLab ya da kendi Gitea sunucundaki depolara eriş. Erişim jetonu bu bilgisayarda şifreli saklanır, depo ayarlarına yazılmaz.</p>
    </header>}

    {connections.length>0&&<div className="git-accounts">
      {connections.map(item=><div className="git-account" key={item.id}>
        {item.avatar?<img src={item.avatar} alt=""/>:<i>{(item.name||item.login||'?').slice(0,1).toUpperCase()}</i>}
        <span><b>{item.name||item.login}</b><small>{item.kind==='github'?'GitHub':item.kind==='gitlab'?'GitLab':'Gitea'} · {item.host.replace(/^https?:\/\//,'')}</small></span>
        <button type="button" className="git-button subtle" onClick={()=>onDisconnect(item.id)}>Bağlantıyı kes</button>
      </div>)}
    </div>}

    <div className="git-connect-form">
      <div className="git-provider-tabs">
        {PROVIDERS.map(item=><button type="button" key={item.kind} className={kind===item.kind?'active':undefined} onClick={()=>{setKind(item.kind);setError('')}}>
          <b>{item.name}</b><small>{item.hint}</small>
        </button>)}
      </div>

      {kind!=='github'&&<label className="git-field"><span>Sunucu adresi</span>
        <input value={host} placeholder={provider.host||'https://git.sirketim.com'} onChange={event=>setHost(event.target.value)}/>
      </label>}

      <label className="git-field"><span>Erişim jetonu</span>
        <input type="password" value={token} placeholder="ghp_… / glpat-… / gitea jetonu" autoComplete="off" onChange={event=>setToken(event.target.value)}
          onKeyDown={event=>{if(event.key==='Enter')void connect()}}/>
      </label>

      <p className="git-hint">
        Gereken yetkiler: <code>{provider.scopes}</code>
        {effectiveHost&&<> · <a href={provider.tokenUrl(effectiveHost)} target="_blank" rel="noreferrer">jeton oluştur</a></>}
      </p>

      {error&&<p className="git-note error">{error}</p>}
      <button type="button" className="git-button primary" disabled={busy||!token.trim()||(kind!=='github'&&!effectiveHost)} onClick={()=>void connect()}>
        {busy?'Doğrulanıyor…':'Bağlan'}
      </button>
    </div>
  </div>
}
