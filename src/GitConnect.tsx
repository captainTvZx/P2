import { useEffect, useRef, useState } from 'react'
import ProviderMark, { PROVIDER_LABEL } from './ProviderMarks'
import type { GitConnection, ProviderKind } from './gitClient'

type Stage={kind:'idle'}|{kind:'waiting';provider:ProviderKind}|{kind:'device';provider:'github';userCode:string;url:string}
const PROVIDERS:Array<{kind:ProviderKind;host:string;description:string}>=[
  {kind:'github',host:'https://github.com',description:'GitHub hesabındaki kişisel ve ortak depolar'},
  {kind:'gitlab',host:'https://gitlab.com',description:'GitLab hesabındaki projeler ve gruplar'},
  {kind:'gitea',host:'http://159.146.90.40:3000',description:'Gitea hesabındaki özel ve açık depolar'},
]

export default function GitConnect({connections,onConnected,onDisconnect,compact}:{connections:GitConnection[];onConnected:(connection:GitConnection)=>void;onDisconnect:(id:string)=>void;compact?:boolean}){
  const bridge=window.p2?.git,[stage,setStage]=useState<Stage>({kind:'idle'}),[error,setError]=useState(''),[copied,setCopied]=useState(false),polling=useRef<number|undefined>(undefined)
  useEffect(()=>()=>{if(polling.current)window.clearInterval(polling.current)},[])
  const connected=new Set(connections.map(item=>item.kind))

  async function login(kind:ProviderKind,host:string){
    if(!bridge)return
    setError('')
    if(kind==='github'){
      setStage({kind:'waiting',provider:kind})
      const started=await bridge.deviceStart({kind,host})
      if(!started?.ok){setStage({kind:'idle'});setError(started?.error||'GitHub girişi başlatılamadı.');return}
      setStage({kind:'device',provider:'github',userCode:started.userCode,url:started.verificationUri})
      window.open(started.verificationUri,'_blank','noreferrer')
      let delay=Math.max(started.interval,5)*1000
      const poll=async()=>{
        const result=await bridge.devicePoll({ticket:started.ticket})
        if(!result?.ok||result.status==='error'){setStage({kind:'idle'});setError(result?.error||'GitHub hesabı alınamadı.');return}
        if(result.status==='ok'){setStage({kind:'idle'});onConnected(result.connection);return}
        if(result.status==='denied'||result.status==='expired'){setStage({kind:'idle'});setError(result.status==='denied'?'Giriş reddedildi.':'Giriş kodunun süresi doldu.');return}
        if(result.status==='slow')delay+=5000
        polling.current=window.setTimeout(()=>void poll(),delay)
      }
      polling.current=window.setTimeout(()=>void poll(),delay)
      return
    }
    setStage({kind:'waiting',provider:kind})
    const result=await bridge.login({kind,host})
    setStage({kind:'idle'})
    if(result?.ok)onConnected(result.connection)
    else setError(result?.error||`${PROVIDER_LABEL[kind]} girişi tamamlanamadı.`)
  }

  return <div className={compact?'git-connect compact':'git-connect'}>
    {!compact&&<header><h2>Git hesabını bağla</h2><p>Sağlayıcını seçip tarayıcıda hesabınla giriş yap. Bağlandıktan sonra erişebildiğin depolar otomatik olarak burada listelenir.</p></header>}
    {connections.length>0&&<div className="git-accounts">{connections.map(item=><div className="git-account" key={item.id}>{item.avatar?<img src={item.avatar} alt=""/>:<ProviderMark kind={item.kind} size={22}/>}<span><b>{item.name||item.login}</b><small>{PROVIDER_LABEL[item.kind]} · @{item.login}</small></span><button type="button" className="git-button subtle" onClick={()=>onDisconnect(item.id)}>Bağlantıyı kes</button></div>)}</div>}
    <div className="git-provider-login-grid">{PROVIDERS.map(provider=>{
      const busy=stage.kind!=='idle',active=stage.kind!=='idle'&&stage.provider===provider.kind,isConnected=connected.has(provider.kind)
      return <article className={`git-provider-login ${provider.kind}${isConnected?' connected':''}`} key={provider.kind}><ProviderMark kind={provider.kind} size={28}/><div><b>{PROVIDER_LABEL[provider.kind]}</b><small>{provider.description}</small></div>{isConnected?<span className="git-connected-label">Bağlı</span>:<button type="button" className={`git-signin ${provider.kind}`} disabled={busy} onClick={()=>void login(provider.kind,provider.host)}>{active?'Giriş bekleniyor…':`${PROVIDER_LABEL[provider.kind]} ile giriş yap`}</button>}</article>
    })}</div>
    {stage.kind==='device'&&<div className="git-device"><p>GitHub sayfasında bu kodu gir:</p><b onClick={()=>{void navigator.clipboard.writeText(stage.userCode);setCopied(true);window.setTimeout(()=>setCopied(false),1500)}}>{stage.userCode}</b><small>{copied?'Kod kopyalandı ✓':'Koda tıklayarak kopyalayabilirsin. Onaydan sonra bu ekran otomatik devam eder.'}</small><a href={stage.url} target="_blank" rel="noreferrer">GitHub giriş sayfasını tekrar aç</a></div>}
    {stage.kind==='waiting'&&<p className="git-login-wait">Tarayıcıda {PROVIDER_LABEL[stage.provider]} girişinin tamamlanması bekleniyor…</p>}
    {error&&<p className="git-note error">{error}</p>}
  </div>
}
