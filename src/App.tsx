import { FormEvent, useEffect, useRef, useState } from 'react'
import Workspace from './Workspace'
import UpdateBanner from './UpdateBanner'
import Toasts from './Toasts'
import { API_BASE, p2Fetch, setClientAuth, setSessionPersistence } from './apiBase'
import { closeRealtime } from './realtime'
import { userInitials } from './initials'
type User={id:string;email:string;username:string;displayName:string;status:string;title?:string|null;avatarUrl?:string|null;twoFactorEnabled?:boolean}; type Auth={user:User;accessToken:string;refreshToken:string}; type Project={id:string;name:string}; type Friend={id:string;username:string;displayName:string;status:string}
export type {Auth,User}
export type Organization={id:string;name:string;slug:string;role:'OWNER'|'ADMIN'|'MEMBER';inviteCode?:string}
const API=API_BASE
async function api<T>(path:string,init?:RequestInit,token?:string):Promise<T>{let r:Response;const abort=new AbortController();const timer=setTimeout(()=>abort.abort(),init?.signal?60000:10000);try{r=await p2Fetch(API+path,{...init,signal:init?.signal||abort.signal,headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})}})}catch{throw{message:'P2 sunucusuna ulaşılamıyor. PostgreSQL ve backend durumunu kontrol edin.',unreachable:true}}finally{clearTimeout(timer)}const data=await r.json().catch(()=>({}));if(!r.ok)throw data;return data}

function AuthScreen({done}:{done:(a:Auth,r:boolean)=>void}){const[register,setRegister]=useState(false),[busy,setBusy]=useState(false),[remember,setRemember]=useState(true),[error,setError]=useState(''),[twoFactor,setTwoFactor]=useState(false);async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const v=Object.fromEntries(new FormData(e.currentTarget));try{const a=await api<Auth>(register?'/auth/register':'/auth/login',{method:'POST',body:JSON.stringify(register?v:{identifier:v.identifier,password:v.password,rememberMe:remember,...(v.code?{code:String(v.code).replace(/\D/g,'')}:{})})});done(a,remember)}catch(x:any){if(x?.code==='TWO_FACTOR_REQUIRED'){setTwoFactor(true);setError('')}else setError(x.message||'Bir hata oluştu.')}finally{setBusy(false)}}return <main className="auth-page"><section className="auth-copy"><span className="logo">P2</span><p className="eyebrow">BUILD · SHARE · PLAY</p><h1>Oyunların<br/>buluşma noktası.</h1><p>Ekibini topla, sürümlerini paylaş ve birlikte test et.</p></section><section className="auth-card"><h2>{register?'Hesap oluştur':'Tekrar hoş geldin'}</h2><p>{register?'Geliştirmeye başlamak için aramıza katıl.':'P2 hesabınla devam et.'}</p><form onSubmit={submit}>{register&&<><label>E-posta<input name="email" type="email" required/></label><label>Kullanıcı adı<input name="username" minLength={3} maxLength={24} pattern="[a-zA-Z0-9_.]+" title="Boşluk kullanma; harf, rakam, nokta ve alt çizgi serbest. Büyük/küçük harf yazdığın gibi korunur." autoCapitalize="off" autoCorrect="off" spellCheck={false} required/></label><label>Görünen ad<input name="displayName" minLength={2} required/></label></>}{!register&&<label>E-posta veya kullanıcı adı<input name="identifier" required autoFocus/></label>}<label>Parola<input name="password" type="password" minLength={8} pattern="(?=.*\p{Ll})(?=.*\p{Lu})(?=.*\d).{8,}" title="En az 8 karakter; en az bir büyük harf, bir küçük harf ve bir rakam içermeli." required/></label>{register&&<label>Parola tekrarı<input name="passwordConfirm" type="password" minLength={8} required/></label>}{twoFactor&&!register&&<label>Doğrulama kodu<input name="code" inputMode="numeric" autoComplete="one-time-code" maxLength={6} placeholder="123456" autoFocus required/><small className="auth-note">Doğrulayıcı uygulamandaki 6 haneli kodu gir.</small></label>}{!register&&<label className="check"><input type="checkbox" checked={remember} onChange={e=>setRemember(e.target.checked)}/> Beni hatırla</label>}{error&&<div className="error">{error}</div>}<button disabled={busy}>{busy?'Bağlanıyor…':register?'Hesap oluştur':twoFactor?'Doğrula ve gir':'Giriş yap'} <span>→</span></button></form><p className="switch">{register?'Zaten hesabın var mı?':'P2’de yeni misin?'} <button onClick={()=>{setRegister(!register);setError('');setTwoFactor(false)}}>{register?'Giriş yap':'Hesap oluştur'}</button></p></section></main>}

function Launcher({auth,logout}:{auth:Auth;logout:()=>void}){const[projects,setProjects]=useState<Project[]>([]),[friends,setFriends]=useState<Friend[]>([]);useEffect(()=>{Promise.all([api<Project[]>('/projects',{},auth.accessToken),api<Friend[]>('/friends',{},auth.accessToken)]).then(([p,f])=>{setProjects(p);setFriends(f)}).catch(()=>{})},[auth.accessToken]);return <main className="launcher"><aside className="sidebar"><div className="side-brand"><span className="logo">P2</span><b>LIBRARY</b></div><p className="section-title">PROJELERİM</p><nav>{projects.map(p=><button key={p.id}>◈ {p.name}</button>)}{!projects.length&&<div className="empty-small">Henüz proje yok.</div>}</nav><button className="new-project">＋ Yeni proje</button></aside><section className="workspace"><header><div><p className="eyebrow">P2 WORKSPACE</p><h2>Hoş geldin, {auth.user.displayName}</h2></div><div className="profile"><span>{userInitials(auth.user.displayName)}</span><div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div><button onClick={logout}>Çıkış</button></div></header><div className="welcome"><span className="orb">P2</span><h1>İlk projen için hazırsın.</h1><p>Build’lerini ekibinle paylaşacağın ve oyunlarını test edeceğin alan burada.</p><button>＋ Proje oluştur</button></div></section><aside className="friends"><header><b>EKİP</b><span>{friends.filter(f=>f.status==='ONLINE').length} çevrimiçi</span></header>{friends.map(f=><div className="friend" key={f.id}><i className={f.status.toLowerCase()}/><span><b>{f.displayName}</b><small>{f.status==='ONLINE'?'Çevrimiçi':'Çevrimdışı'}</small></span></div>)}{!friends.length&&<div className="friends-empty">Arkadaşların burada görünecek.</div>}<button className="friends-add">＋ Arkadaş ekle</button></aside></main>}

function OrganizationOnboarding({auth,complete}:{auth:Auth;complete:(organization:Organization)=>void}){
  const[mode,setMode]=useState<'choose'|'create'|'join'|'created'>('choose'),[error,setError]=useState(''),[busy,setBusy]=useState(false),[created,setCreated]=useState<Organization|null>(null)
  const slugify=(value:string)=>value.toLocaleLowerCase('tr-TR').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/ı/g,'i').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'').slice(0,40)
  async function create(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const name=String(new FormData(e.currentTarget).get('name')||'').trim();try{const organization=await api<Organization>('/organizations',{method:'POST',body:JSON.stringify({name,slug:slugify(name)})},auth.accessToken);setCreated(organization);setMode('created')}catch(x:any){setError(x.message||'Kuruluş oluşturulamadı.')}finally{setBusy(false)}}
  async function join(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const code=String(new FormData(e.currentTarget).get('code')||'');try{complete(await api<Organization>('/organizations/join',{method:'POST',body:JSON.stringify({code})},auth.accessToken))}catch(x:any){setError(x.message||'Kuruluşa katılınamadı.')}finally{setBusy(false)}}
  if(mode==='created'&&created)return <main className="org-page"><section className="org-dialog success"><span className="success-mark">✓</span><p className="eyebrow">ÇALIŞMA ALANI HAZIR</p><h1>{created.name}</h1><p>Kuruluşun oluşturuldu. Ekip arkadaşlarını bu davet koduyla içeri alabilirsin.</p><div className="invite-code"><div><small>DAVET KODU</small><span>{created.inviteCode}</span></div><button onClick={()=>navigator.clipboard.writeText(created.inviteCode||'')}>Kopyala</button></div><button className="primary" onClick={()=>complete(created)}>Çalışma alanına geç <span>→</span></button></section></main>
  const title=mode==='choose'?'Ekibin için bir alan seç.':mode==='create'?'Kuruluşunu oluşturalım.':'Ekibine katıl.'
  const subtitle=mode==='choose'?'Projeler, build’ler ve ekip üyeleri tek bir çalışma alanında buluşur.':mode==='create'?'İsim vermen yeterli; davet kodunu bir sonraki adımda hazırlayacağız.':'Yöneticinin paylaştığı davet kodunu gir ve kaldığın yerden devam et.'
  return <main className="org-page"><section className="org-shell">
    <aside className="org-intro"><span className="org-brand">P2</span><div><p className="eyebrow">P2 ORGANIZATIONS</p><h2>Birlikte üretmek için tek bir merkez.</h2><p>Ekibini, projelerini ve sürümlerini dağılmadan yönet.</p></div><ul><li><i>✓</i> Ortak proje ve kanal yapısı</li><li><i>✓</i> Tek kodla güvenli davet</li><li><i>✓</i> Ekip rolleri ve izinler</li></ul><small>Kuruluş ayarlarını daha sonra değiştirebilirsin.</small></aside>
    <div className="org-workflow"><header><span className="org-step">{mode==='choose'?'BAŞLANGIÇ':mode==='create'?'YENİ KURULUŞ':'DAVETLE KATIL'}</span><h1>{title}</h1><p>{subtitle}</p></header>
      {mode==='choose'?<div className="org-options"><button className="featured" onClick={()=>setMode('create')}><em>ÖNERİLEN</em><span className="option-icon">＋</span><b>Kuruluş oluştur</b><small>Yeni bir ekip kur, projelerini yönet ve üyelerini davet et.</small><i>Oluşturmaya başla <span>→</span></i></button><button onClick={()=>setMode('join')}><span className="option-icon">↗</span><b>Bir kuruluşa katıl</b><small>Mevcut ekibinden aldığın davet koduyla çalışma alanına gir.</small><i>Davet kodunu gir <span>→</span></i></button></div>:<form className="org-form" onSubmit={mode==='create'?create:join}>{mode==='create'?<label>Kuruluş adı<input name="name" minLength={2} maxLength={60} placeholder="Örn. Northstar Games" autoFocus required/><small>Çalışma alanı adresi addan otomatik hazırlanır.</small></label>:<label>Davet kodu<input name="code" minLength={6} maxLength={20} placeholder="P2-XXXXXXXX" autoFocus required/><small>Kod büyük/küçük harfe duyarlı değildir.</small></label>}{error&&<div className="error">{error}</div>}<div className="form-actions"><button type="button" onClick={()=>{setMode('choose');setError('')}}>← Geri</button><button className="primary" disabled={busy}>{busy?'İşleniyor…':mode==='create'?'Kuruluşu oluştur':'Kuruluşa katıl'} <span>→</span></button></div></form>}
    </div>
  </section></main>
}

function OrganizationGate({auth,logout}:{auth:Auth;logout:()=>void}){const[loading,setLoading]=useState(true),[organization,setOrganization]=useState<Organization|null>(null);useEffect(()=>{api<Organization[]>('/organizations',{},auth.accessToken).then(items=>{if(items.length)setOrganization(items[0])}).finally(()=>setLoading(false))},[auth.accessToken]);if(loading)return <main className="splash"><span className="logo">P2</span><p>Kuruluşlar yükleniyor…</p></main>;return organization?<Workspace auth={auth} organization={organization} logout={logout}/>:<OrganizationOnboarding auth={auth} complete={setOrganization}/>}

// Masaüstü sürümü iki şekilde kurulabilir: sunucuyu bu bilgisayarda barındırmak ya da
// yalnızca istemci olarak başka birinin sunucusuna bağlanmak. Seçim ilk açılışta sorulur
// ve Ayarlar > Uygulama > Sunucu adresi bölümünden değiştirilebilir.
export function readInstallMode():'host'|'client'|''{
  try{return(window.localStorage.getItem('p2:mode')as'host'|'client'|null)||''}catch{return ''}
}
function ModePicker({done,demo=false}:{done:()=>void;demo?:boolean}){
  const[mode,setMode]=useState<'choose'|'client'>('choose')
  // "Bir sunucu oluştur": bu bilgisayar sunucu olur; sıradaki ekran PostgreSQL + yönetici kurulumudur.
  const[address,setAddress]=useState(window.p2?.suggestedServer||'')
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  function chooseHost(){
    if(demo)return
    try{window.localStorage.removeItem('p2:apiUrl');window.localStorage.setItem('p2:mode','host')}catch{}
    done()
  }
  async function connect(e:FormEvent<HTMLFormElement>){
    e.preventDefault();setBusy(true);setError('')
    if(demo){setError('UI showroom: gerçek bir sunucu bağlantısı kurulmadı.');setBusy(false);return}
    const raw=address.trim().replace(/\/+$/,'')
    if(!raw){setError('Sunucu adresini yaz.');setBusy(false);return}
    const base=/^https?:\/\//i.test(raw)?raw:`https://${raw}`
    // Adres site kökü de olabilir, doğrudan API de; hangisi yanıt veriyorsa o kullanılır.
    for(const candidate of[base,`${base}/api`]){
      try{
        const health=await fetch(`${candidate}/health`).then(r=>r.json())
        if(health?.status!=='ok')continue
        try{window.localStorage.setItem('p2:apiUrl',candidate);window.localStorage.setItem('p2:mode','client')}catch{}
        window.location.reload();return
      }catch{}
    }
    setError('Bu adreste P2 sunucusu bulunamadı. Adresi ve sunucunun açık olduğunu kontrol et.')
    setBusy(false)
  }
  if(mode==='client')return <main className="setup-page"><section className="setup-shell mode-shell">
    <header><span className="logo">P2</span><div><p className="eyebrow">BİR SUNUCUYA KATIL</p><h2>Sunucu adresini yaz</h2></div></header>
    <form onSubmit={connect}>
      <label className="mode-field">Sunucu adresi<input value={address} onChange={e=>setAddress(e.target.value)} placeholder="https://sunucu.ornek.com ya da http://192.168.1.20:4000" autoFocus/></label>
      <p className="mode-hint">Sunucuyu kuran kişinin paylaştığı adres. Bu bilgisayara hiçbir şey kurulmaz; sadece bağlanır.</p>
      {error&&<div className="error">{error}</div>}
      <div className="mode-actions">
        <button type="button" className="mode-back" onClick={()=>setMode('choose')} disabled={busy}>Geri</button>
        <button className="install-button" disabled={busy}>{busy?'Bağlanılıyor…':'Bağlan'} <span>→</span></button>
      </div>
    </form>
  </section></main>
  return <main className="setup-page"><section className="setup-shell mode-shell">
    <header><span className="logo">P2</span><div><p className="eyebrow">HOŞ GELDİN</p><h2>Nasıl başlamak istersin?</h2></div></header>
    <div className="mode-choices">
      <button type="button" onClick={()=>setMode('client')}>
        <b>Bir sunucuya katıl</b>
        <small>Ekibinin sunucusu zaten varsa adresini yaz ve giriş yap. Kurulum yok.</small>
      </button>
      <button type="button" onClick={chooseHost}>
        <b>Bir sunucu oluştur</b>
        <small>P2 sunucusunu bu bilgisayarda kur. PostgreSQL bağlantısı ve ilk yönetici hesabı istenir; ekibin sana bağlanır.</small>
      </button>
    </div>
    <p className="mode-hint">Seçimini sonradan Ayarlar &gt; Uygulama &gt; Sunucu adresi bölümünden değiştirebilirsin.</p>
  </section></main>
}

// Parola gücü: uzunluk ve karakter çeşitliliği; sunucunun istediği en az kural
// (8+, büyük, küçük, rakam) sağlanmadan "kabul edilebilir" olmaz.
function passwordStrength(value:string){
  if(!value)return{score:0,label:'',ok:false}
  const rules=[value.length>=8,/\p{Lu}/u.test(value),/\p{Ll}/u.test(value),/\d/.test(value)]
  const ok=rules.every(Boolean)
  let score=rules.filter(Boolean).length
  if(/[^\p{L}\d]/u.test(value))score++
  if(value.length>=12)score++
  if(value.length>=16)score++
  const label=!ok?'Zayıf — 8+ karakter, büyük/küçük harf ve rakam gerekli':score<=4?'Orta':score===5?'Güçlü':'Çok güçlü'
  return{score:Math.min(6,score),label,ok}
}
function PasswordMeter({value,confirm}:{value:string;confirm:string}){
  const strength=passwordStrength(value)
  if(!value&&!confirm)return null
  const tone=!strength.ok?'weak':strength.score<=4?'fair':'strong'
  return <div className={`password-meter ${tone}`}>
    <div className="password-bar"><i style={{width:`${(strength.score/6)*100}%`}}/></div>
    <small>{strength.label}{confirm&&<> · {confirm===value?<b className="match">Parolalar eşleşiyor ✓</b>:<b className="mismatch">Parolalar eşleşmiyor</b>}</>}</small>
  </div>
}

// Sunucu yanıt vermiyorsa kullanıcı körlemesine beklemez: neden, günlük ve çıkış yolları.
function ServerProblem({mode,reason,retry,reset,changeAddress}:{mode:'host'|'client'|'';reason?:string;retry:()=>void;reset:()=>void;changeAddress:()=>void}){
  const missingDb=/does not exist/i.test(reason||'')
  const[log,setLog]=useState(''),[status,setStatus]=useState<{alive:boolean;configured:boolean;port:number}|null>(null),[starting,setStarting]=useState(false),[copied,setCopied]=useState(false)
  const load=async()=>{const[l,st]=await Promise.all([window.p2?.database.serverLog?.(),window.p2?.database.serverStatus?.()]);setLog(l||'');if(st)setStatus(st)}
  useEffect(()=>{void load();const timer=window.setInterval(()=>void load(),4000);return()=>window.clearInterval(timer)},[])
  async function copyAddress(){await navigator.clipboard.writeText(API);setCopied(true);window.setTimeout(()=>setCopied(false),1600)}
  const description=mode==='client'?'Kaydedilen uzak sunucuya şu anda erişilemiyor. Adresi kontrol edebilir veya yeniden deneyebilirsin.':'Yerel P2 servisi hazır görünmüyor. Durumu kontrol edip sunucuyu yeniden başlatabilirsin.'
  return <main className="setup-page server-problem-page"><section className="setup-shell mode-shell server-problem-shell">
    <header><span className="server-problem-icon" aria-hidden="true">!</span><div><p className="eyebrow">{mode==='client'?'UZAK SUNUCU':'YEREL SUNUCU'}</p><h2>{mode==='client'?'Sunucuya bağlanamadık':'Sunucu yanıt vermiyor'}</h2><p className="server-problem-lead">{description}</p></div></header>
    <div className="server-endpoint">
      <span className="server-endpoint-symbol" aria-hidden="true">⌁</span>
      <div><small>BAĞLANTI ADRESİ</small><code>{API}</code></div>
      <button type="button" onClick={copyAddress}>{copied?'Kopyalandı':'Kopyala'}</button>
    </div>
    {mode!=='client'&&<div className="server-status-row"><span className={status?.alive?'online':'offline'}><i/>{status?.alive?'Sunucu süreci çalışıyor':'Sunucu süreci kapalı'}</span><span>Port <b>{status?.port||'—'}</b></span><span>{status?.configured?'Yapılandırılmış':'Yapılandırma bekliyor'}</span></div>}
    {reason&&<div className="server-error-detail"><b>Bağlantı ayrıntısı</b><p>{missingDb?`Veritabanı bulunamadı: ${reason.replace(/^.*?Migration uygulanamadı: /,'')}. PostgreSQL'de silinmiş ya da başka bir sunucuya taşınmış olabilir. "Kurulumu yeniden yap" ile veritabanını yeniden oluşturabilirsin.`:reason}</p></div>}
    {mode!=='client'&&<>
      <details className="server-diagnostics"><summary>Tanılama günlüğünü göster</summary><p>{status?.alive?'API çalışıyor ancak isteklere yanıt vermiyor; PostgreSQL bağlantısı bekliyor olabilir.':'Sunucu süreci çalışmıyor. Yeniden başlatmayı dene; sorun sürerse aşağıdaki son günlük satırlarını incele.'}</p><pre className="server-log">{log||'Günlük boş (%ProgramData%\\P2\\server.log).'}</pre></details>
    </>}
    <div className="mode-actions server-actions">
      <button type="button" className="install-button" onClick={retry}>Yeniden dene <span>→</span></button>
      {mode!=='client'&&<button type="button" className="mode-back" disabled={starting} onClick={async()=>{setStarting(true);await window.p2?.database.serverStart?.();window.setTimeout(()=>{setStarting(false);retry()},6000)}}>{starting?'Başlatılıyor…':'Sunucuyu başlat'}</button>}
      {mode==='client'?<button type="button" className="mode-back" onClick={changeAddress}>Adresi değiştir</button>:<button type="button" className="mode-back" onClick={reset}>{missingDb?'Kurulumu yeniden yap':'Kurulumu sıfırla'}</button>}
    </div>
  </section></main>
}

function Setup({complete,back}:{complete:(auth:Auth)=>void;back:()=>void}){
  const[error,setError]=useState(''),[busy,setBusy]=useState(false),[stage,setStage]=useState('')
  const[password,setPassword]=useState(''),[confirm,setConfirm]=useState('')
  const[installService,setInstallService]=useState(true)
  const[serviceScript,setServiceScript]=useState('')
  useEffect(()=>{void window.p2?.database.serviceStatus?.().then(state=>{if(state?.script)setServiceScript(state.script)})},[])
  async function submit(e:FormEvent<HTMLFormElement>){
    e.preventDefault();setBusy(true);setError('');const v=Object.fromEntries(new FormData(e.currentTarget))
    if(!passwordStrength(String(v.password)).ok){setError('Yönetici parolası en az 8 karakter olmalı; büyük harf, küçük harf ve rakam içermeli.');setBusy(false);return}
    if(v.password!==v.passwordConfirm){setError('Parolalar eşleşmiyor.');setBusy(false);return}
    setStage('Veritabanı oluşturuluyor ve tablolar hazırlanıyor…')
    const result=await window.p2?.database.install({host:String(v.dbHost),port:Number(v.dbPort),username:String(v.dbUser),password:String(v.dbPassword),database:String(v.dbName),ssl:v.dbSsl==='on',installService})
    if(!result?.ok){setError(result?.message||'Veritabanı kurulamadı.');setBusy(false);setStage('');return}
    // Sunucu süreci (ya da servis) Electron'u açıp migration'ları geçtikten sonra yanıt verir; bu 5-20 sn sürer.
    let online=false;const started=Date.now()
    for(let i=0;i<120;i++){setStage(`${installService?'Sunucu servisi':'Sunucu'} başlatılıyor… ${Math.round((Date.now()-started)/1000)} sn`);await new Promise(r=>setTimeout(r,500));try{await api('/health');online=true;break}catch{}}
    if(!online){
      const log=await window.p2?.database.serverLog?.()
      setError(`Sunucu 60 saniyede yanıt vermedi.${installService?' Servis kaydı için gelen yönetici izni penceresini onayladığından emin ol.':''}${log?`\n\nSon günlük:\n${log}`:'\n\nGünlük dosyası bulunamadı (%ProgramData%\\P2\\server.log).'}`)
      setBusy(false);setStage('');return
    }
    setStage('Yönetici hesabı oluşturuluyor…')
    try{const auth=await api<Auth>('/setup/admin',{method:'POST',body:JSON.stringify({email:v.email,username:v.username,displayName:v.displayName,password:v.password,passwordConfirm:v.passwordConfirm})});await window.p2?.session.save(auth.refreshToken);complete(auth)}catch(x:any){setError(x.message||'Admin hesabı oluşturulamadı.');setBusy(false)}
  }
  return <main className="setup-page"><section className="setup-shell"><header><span className="logo">P2</span><div><p className="eyebrow">BİR SUNUCU OLUŞTUR</p><h2>P2 sunucunu hazırla</h2></div><button type="button" className="mode-back setup-back" onClick={back} disabled={busy}>‹ Geri</button></header><form onSubmit={submit}><fieldset><legend><span>1</span> PostgreSQL bağlantısı</legend><div className="form-grid"><label>Sunucu<input name="dbHost" defaultValue="127.0.0.1" required/></label><label>Port<input name="dbPort" type="number" defaultValue="5432" required/></label><label>Yönetici kullanıcı<input name="dbUser" defaultValue="postgres" required/></label><label>Veritabanı adı<input name="dbName" defaultValue="p2" pattern="[a-z][a-z0-9_]*" required/></label><label className="wide">PostgreSQL parolası<input name="dbPassword" type="password" required/></label><label className="check wide"><input name="dbSsl" type="checkbox"/> SSL bağlantısı kullan</label></div></fieldset><fieldset><legend><span>2</span> İlk yönetici hesabı</legend><div className="form-grid"><label>E-posta<input name="email" type="email" required/></label><label>Kullanıcı adı<input name="username" minLength={3} maxLength={24} pattern="[a-zA-Z0-9_.]+" title="Boşluk kullanma; harf, rakam, nokta ve alt çizgi serbest. Büyük/küçük harf yazdığın gibi korunur." autoCapitalize="off" autoCorrect="off" spellCheck={false} required/></label><label className="wide">Görünen ad<input name="displayName" minLength={2} required/></label><label>Parola<input name="password" type="password" minLength={8} value={password} onChange={e=>setPassword(e.target.value)} title="En az 8 karakter; en az bir büyük harf, bir küçük harf ve bir rakam içermeli." required/></label><label>Parola tekrarı<input name="passwordConfirm" type="password" minLength={8} value={confirm} onChange={e=>setConfirm(e.target.value)} required/></label><div className="wide"><PasswordMeter value={password} confirm={confirm}/></div></div></fieldset><fieldset><legend><span>3</span> Sunucu servisi</legend><label className="check service-check"><input type="checkbox" checked={installService} onChange={e=>setInstallService(e.target.checked)}/> Sunucu servisini kaydet ve başlat (önerilir)</label><p className="setup-note">{installService?'Windows bir yönetici izni penceresi açar; onaylayınca sunucu bilgisayar her açıldığında kendiliğinden, arka planda ve sessizce çalışır. P2 kapalıyken de ekibin bağlanabilir.':<>Servis kaydedilmez. Sunucu yalnızca sen P2\u2019yi açtığında, oturumun süresince çalışır; bilgisayar yeniden başlayınca durur. Sonradan kalıcı hale getirmek için kurulum klasöründeki <code>p2-server-install.bat</code> dosyasını çalıştır{serviceScript?<> (<button type="button" className="link-button" onClick={()=>void window.p2?.database.openServerFolder?.()}>klasörü göster</button>)</>:null}.</>}</p></fieldset>{error&&<div className="error">{error}</div>}<button className="install-button" disabled={busy}>{busy?(stage||'P2 kuruluyor…'):'Kurulumu tamamla'} <span>→</span></button><small>Veritabanı parolası Windows güvenli kasasında şifrelenir; servis için %ProgramData%\\P2 altında yalnızca yöneticilerin okuyabildiği bir kopya tutulur.</small></form></section></main>
}

// UI showroom gerçek giriş/kurulum bileşenlerini kullanır; yalnızca geliştirme
// derlemesinde DevShowcase tarafından içe aktarılırlar.
export {AuthScreen,OrganizationOnboarding,ModePicker,ServerProblem,Setup}

export default function App(){const[auth,setAuth]=useState<Auth|null>(null),[loading,setLoading]=useState(true),[setup,setSetup]=useState(false),[problem,setProblem]=useState(false),[problemReason,setProblemReason]=useState(''),[mode,setMode]=useState(readInstallMode());useEffect(()=>{const update=(event:Event)=>setAuth(current=>current?{...current,user:{...current.user,...(event as CustomEvent<User>).detail}}:current);const refreshed=(event:Event)=>{const next=(event as CustomEvent<Auth>).detail;setClientAuth(next);setAuth(next)};const expired=()=>{setClientAuth(null);closeRealtime();void window.p2?.session.clear();setAuth(null)};window.addEventListener('p2:profile-updated',update);window.addEventListener('p2:auth-refreshed',refreshed);window.addEventListener('p2:session-expired',expired);return()=>{window.removeEventListener('p2:profile-updated',update);window.removeEventListener('p2:auth-refreshed',refreshed);window.removeEventListener('p2:session-expired',expired)}},[]);useEffect(()=>{setClientAuth(auth)},[auth]);// Oturum tazeleme yenileme jetonunu döndürür: aynı jetonla ikinci bir istek
// sunucuda geçersizdir. StrictMode etkiyi iki kez çalıştırdığı için bu koruma
// olmadan ikinci çağrı başarısız olup kayıtlı oturumu siliyordu; ana pencere
// bellekteki oturumla çalışmaya devam ettiğinden bu ancak yeni bir pencere
// açıldığında "tekrar giriş yap" olarak ortaya çıkıyordu.
  const restoring=useRef(false)
  const chosenNow=useRef(false)
  async function restore(){if(restoring.current)return;restoring.current=true;setProblem(false);try{// Sunucu modunda gömülü API açılışta birkaç saniye geç kalkar; hemen pes etmemek için beklenir.
let up=false
if(readInstallMode()==='host'&&window.p2?.database.serverWait){const waited=await window.p2.database.serverWait();if(!waited.ok){
  // Kurulum artık geçersizse (veritabanı silinmiş, parola değişmiş) eski yapılandırmaya
  // tutunmanın anlamı yok: yapılandırma unutulur, uygulama ilk açılış gibi seçim ekranına döner.
  if(/does not exist|password authentication|role .* does not exist|Yapılandırması yok/i.test(waited.reason||'')){try{window.localStorage.removeItem('p2:mode')}catch{};await window.p2.database.reset?.();restoring.current=false;setLoading(false);setMode('');return}
  setProblemReason(waited.reason||'');restoring.current=false;setProblem(true);setLoading(false);return}}
for(let attempt=0;attempt<3;attempt++){try{await api('/health');up=true;break}catch{await new Promise(r=>setTimeout(r,400))}}
if(!up){setProblemReason('');restoring.current=false;setProblem(true);setLoading(false);return}
const state=await api<{installed:boolean}>('/setup/status');if(!state.installed){setSetup(true);setLoading(false);return}}catch(problem:any){restoring.current=false;if(problem?.unreachable){setProblem(true)}else setSetup(true);setLoading(false);return}const t=await window.p2?.session.load();if(t)try{const a=await api<Auth>('/auth/refresh',{method:'POST',body:JSON.stringify({refreshToken:t})});await window.p2?.session.save(a.refreshToken);setAuth(a)}catch(problem:any){if(problem?.code)await window.p2?.session.clear()}restoring.current=false;setLoading(false)}useEffect(()=>{if(!window.p2){restore();return}// Yalnızca açılışta: kayıtlı "host" modu ama bu bilgisayarda sunucu yapılandırması yoksa seçim ekranına dön. Kullanıcı seçimi yeni yaptıysa doğrudan kuruluma geç.
if(mode==='host'){const fresh=chosenNow.current;chosenNow.current=true;window.p2.database.status().then(ready=>{if(ready)restore();else if(fresh){setSetup(true);setLoading(false)}else{try{window.localStorage.removeItem('p2:mode')}catch{};setMode('')}}).catch(()=>restore());return}if(mode){restore();return}window.p2.database.status().then(ready=>{if(!ready)return;try{window.localStorage.setItem('p2:mode','host')}catch{};setMode('host')}).catch(()=>{})},[mode]);async function done(a:Auth,r:boolean){setSessionPersistence(r);if(r)await window.p2?.session.save(a.refreshToken);else await window.p2?.session.clear();setAuth(a)}async function logout(){closeRealtime();if(auth)await api('/auth/logout',{method:'POST',body:JSON.stringify({refreshToken:auth.refreshToken})}).catch(()=>{});await window.p2?.session.clear();setSessionPersistence(true);setAuth(null)}// Güncelleme şeridi her ekranın üstünde durur: kurulum, giriş, çalışma alanı.
  // Güncelleme penceresi ve kısa bildirimler her ekranın üstünde durur.
  const banner=<>{window.p2?.updates?<UpdateBanner apiBase={API}/>:null}<Toasts/></>
  if(window.p2&&!mode)return <>{banner}<ModePicker done={()=>{chosenNow.current=true;setMode(readInstallMode())}}/></>;if(loading)return <>{banner}<main className="splash"><span className="logo">P2</span><p>Yükleniyor…</p></main></>;if(problem)return <ServerProblem mode={mode} reason={problemReason} retry={()=>{setProblem(false);setLoading(true);void restore()}} reset={()=>{try{window.localStorage.removeItem('p2:mode')}catch{};void window.p2?.database.reset?.();setProblem(false);setLoading(true);setMode('')}} changeAddress={()=>{try{window.localStorage.removeItem('p2:mode');window.localStorage.removeItem('p2:apiUrl')}catch{};window.location.reload()}}/>;if(setup)return <Setup complete={a=>{setSessionPersistence(true);setAuth(a);setSetup(false)}} back={()=>{try{window.localStorage.removeItem('p2:mode')}catch{};void window.p2?.database.reset?.();setSetup(false);setLoading(true);setMode('')}}/>;return <>{banner}{auth?<OrganizationGate auth={auth} logout={logout}/>:<AuthScreen done={done}/>}</>}
