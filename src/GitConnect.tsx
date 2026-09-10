import { useEffect, useRef, useState } from 'react'
import ProviderMark, { PROVIDER_LABEL } from './ProviderMarks'
import type { GitConnection, ProviderKind } from './gitClient'

// "GitHub ile giriş yap" ekranı. Giriş tarayıcıda yapılır, jeton bu bilgisayarda
// şifreli kalır ve oturum kalıcıdır. Sunucular git istemcileri için hazır bir
// kayıt sunduğundan önceden hiçbir şey tanımlamak gerekmiyor; yalnızca sunucu
// bunu kapatmışsa kendi uygulama kimliğini girme paneli açılıyor.
type Stage={kind:'idle'}|{kind:'waiting'}|{kind:'device';userCode:string;url:string}

const PROVIDERS:Array<{kind:ProviderKind;hint:string;needsHost:boolean;appUrl:(host:string)=>string;appHelp:string}>=[
  {kind:'github',hint:'github.com',needsHost:false,
    appUrl:()=>'https://github.com/settings/applications/new',
    appHelp:'GitHub → Settings → Developer settings → OAuth Apps → New OAuth App. “Enable Device Flow” kutusunu işaretle; Client ID’yi buraya yapıştır.'},
  {kind:'gitlab',hint:'gitlab.com ya da kendi sunucun',needsHost:true,
    appUrl:host=>`${host.replace(/\/+$/,'')}/-/profile/applications`,
    appHelp:'GitLab → Preferences → Applications. “Confidential” kapalı olsun, kapsam olarak api ve read_user seç, Redirect URI alanına aşağıdaki adresi yaz.'},
  {kind:'gitea',hint:'kendi Gitea sunucun',needsHost:true,
    appUrl:host=>`${host.replace(/\/+$/,'')}/user/settings/applications`,
    appHelp:'Gitea → Ayarlar → Uygulamalar → OAuth2 uygulaması oluştur. Gizli anahtar gerekmez (PKCE), Redirect URI alanına aşağıdaki adresi yaz.'},
]

export default function GitConnect({connections,onConnected,onDisconnect,compact}:{
  connections:GitConnection[]
  onConnected:(connection:GitConnection)=>void
  onDisconnect:(id:string)=>void
  compact?:boolean
}){
  const bridge=window.p2?.git
  const[kind,setKind]=useState<ProviderKind>('gitea')
  const[host,setHost]=useState('')
  const[clientId,setClientId]=useState('')
  const[setup,setSetup]=useState(false)
  const[manual,setManual]=useState(false)
  const[token,setToken]=useState('')
  const[stage,setStage]=useState<Stage>({kind:'idle'})
  const[error,setError]=useState('')
  const[copied,setCopied]=useState(false)
  const polling=useRef<number|undefined>(undefined)

  const provider=PROVIDERS.find(item=>item.kind===kind)!
  const effectiveHost=provider.needsHost?host.trim().replace(/\/+$/,''):'https://github.com'

  useEffect(()=>()=>{if(polling.current)window.clearInterval(polling.current)},[])
  useEffect(()=>{setError('');setSetup(false);setStage({kind:'idle'})},[kind,host])

  async function saveApp(){
    if(!bridge||!clientId.trim())return
    const result=await bridge.saveApp({kind,host:effectiveHost,clientId:clientId.trim()})
    if(result?.ok){setClientId('');setSetup(false);setError('')}
    else setError(result?.error||'Kaydedilemedi.')
  }

  // GitHub cihaz akışı: kod gösterilir, onaylanana kadar sorgulanır.
  async function loginWithDevice(){
    if(!bridge)return
    setError('');setStage({kind:'waiting'})
    const started=await bridge.deviceStart({kind,host:effectiveHost})
    if(!started?.ok){setStage({kind:'idle'});setError(started?.error||'Giriş başlatılamadı.');if(started?.needsApp)setSetup(true);return}
    setStage({kind:'device',userCode:started.userCode,url:started.verificationUri})
    window.open(started.verificationUri,'_blank','noreferrer')
    polling.current=window.setInterval(async()=>{
      const result=await bridge.devicePoll({ticket:started.ticket})
      if(!result?.ok){window.clearInterval(polling.current);setStage({kind:'idle'});setError(result?.error||'Giriş tamamlanamadı.');return}
      if(result.status==='ok'){window.clearInterval(polling.current);setStage({kind:'idle'});onConnected(result.connection);return}
      if(result.status==='denied'||result.status==='expired'){
        window.clearInterval(polling.current);setStage({kind:'idle'})
        setError(result.status==='denied'?'Giriş reddedildi.':'Kodun süresi doldu, yeniden dene.')
      }
    },Math.max(started.interval,5)*1000)
  }

  // GitLab / Gitea: tarayıcıda onay, 127.0.0.1'e dönüşte oturum açılır.
  async function loginWithBrowser(){
    if(!bridge)return
    setError('');setStage({kind:'waiting'})
    const result=await bridge.login({kind,host:effectiveHost})
    setStage({kind:'idle'})
    if(result?.ok)onConnected(result.connection)
    else{setError(result?.error||'Giriş tamamlanamadı.');if(result?.needsApp)setSetup(true)}
  }

  async function connectWithToken(){
    if(!bridge||!token.trim())return
    setError('')
    const result=await bridge.connectToken({kind,host:effectiveHost,token:token.trim()})
    if(result?.ok){setToken('');setManual(false);onConnected(result.connection)}
    else setError(result?.error||'Bağlanılamadı.')
  }

  const canLogin=!provider.needsHost||effectiveHost.startsWith('http')

  return <div className={compact?'git-connect compact':'git-connect'}>
    {!compact&&<header>
      <h2>Hesabınla giriş yap</h2>
      <p>Tarayıcıda onayla, gerisi kendiliğinden olsun. Oturum bu bilgisayarda şifreli saklanır; bir daha giriş yapman gerekmez.</p>
    </header>}

    {connections.length>0&&<div className="git-accounts">
      {connections.map(item=><div className="git-account" key={item.id}>
        {item.avatar?<img src={item.avatar} alt=""/>:<ProviderMark kind={item.kind} size={22}/>}
        <span><b>{item.name||item.login}</b><small>{PROVIDER_LABEL[item.kind]} · {item.host.replace(/^https?:\/\//,'')}{item.auth==='pat'?' · jeton':''}</small></span>
        <button type="button" className="git-button subtle" onClick={()=>onDisconnect(item.id)}>Çıkış yap</button>
      </div>)}
    </div>}

    <div className="git-connect-form">
      <div className="git-provider-tabs">
        {PROVIDERS.map(item=><button type="button" key={item.kind} className={kind===item.kind?'active':undefined} onClick={()=>setKind(item.kind)}>
          <ProviderMark kind={item.kind} size={22}/>
          <b>{PROVIDER_LABEL[item.kind]}</b><small>{item.hint}</small>
        </button>)}
      </div>

      {provider.needsHost&&<label className="git-field"><span>Sunucu adresi</span>
        <input value={host} placeholder={kind==='gitlab'?'https://gitlab.com':'http://159.146.90.40:3000'} onChange={event=>setHost(event.target.value)}/>
      </label>}

      {stage.kind==='device'&&<div className="git-device">
        <p>Tarayıcıda açılan sayfaya bu kodu gir:</p>
        <b onClick={()=>{void navigator.clipboard.writeText(stage.userCode);setCopied(true);window.setTimeout(()=>setCopied(false),1500)}} title="Kopyalamak için tıkla">{stage.userCode}</b>
        <small>{copied?'Kod kopyalandı ✓':'Onayladıktan sonra buraya dönmen yeterli — giriş kendiliğinden tamamlanır.'}</small>
        <a href={stage.url} target="_blank" rel="noreferrer">Sayfa açılmadıysa buradan aç</a>
      </div>}

      {stage.kind==='waiting'&&<p className="git-hint">Tarayıcıda onay bekleniyor…</p>}

      {stage.kind==='idle'&&<>
        <button type="button" className={`git-signin ${kind}`} disabled={!canLogin}
          onClick={()=>void (kind==='github'?loginWithDevice():loginWithBrowser())}>
          <ProviderMark kind={kind} size={20}/>
          {PROVIDER_LABEL[kind]} ile giriş yap
        </button>
        <p className="git-hint">Onay ekranı sunucunun git istemcileri için ayırdığı kaydı gösterir; yetkiyi istediğin an oradan geri alabilirsin.</p>
      </>}

      {setup&&<div className="git-setup">
        <p>{provider.appHelp}</p>
        {kind!=='github'&&<label className="git-field"><span>Redirect URI</span>
          <input readOnly value="http://127.0.0.1/callback" onFocus={event=>event.currentTarget.select()}/>
          <small>Port her girişte değişir; GitLab ve Gitea 127.0.0.1 için bunu kabul eder.</small>
        </label>}
        {effectiveHost&&<a className="git-button subtle" href={provider.appUrl(effectiveHost)} target="_blank" rel="noreferrer">{PROVIDER_LABEL[kind]} uygulama sayfasını aç</a>}
        <label className="git-field"><span>Client ID</span>
          <input value={clientId} placeholder="Ov23li… / 0f1e2d…" onChange={event=>setClientId(event.target.value)} onKeyDown={event=>{if(event.key==='Enter')void saveApp()}}/>
        </label>
        <button type="button" className="git-button primary" disabled={!clientId.trim()} onClick={()=>void saveApp()}>Kimliği kaydet</button>
      </div>}

      {error&&<p className="git-note error">{error}</p>}

      <p className="git-hint">
        <button type="button" className="git-link" onClick={()=>setManual(value=>!value)}>{manual?'Jetonla bağlanmayı kapat':'Jetonla bağlan'}</button>
        {!setup&&<> · <button type="button" className="git-link" onClick={()=>setSetup(true)}>kendi uygulama kimliğimi gireyim</button></>}
      </p>
      {manual&&<div className="git-setup">
        <label className="git-field"><span>Erişim jetonu</span>
          <input type="password" value={token} placeholder="ghp_… / glpat-… / gitea jetonu" autoComplete="off"
            onChange={event=>setToken(event.target.value)} onKeyDown={event=>{if(event.key==='Enter')void connectWithToken()}}/>
        </label>
        <button type="button" className="git-button" disabled={!token.trim()} onClick={()=>void connectWithToken()}>Jetonla bağlan</button>
      </div>}
    </div>
  </div>
}
