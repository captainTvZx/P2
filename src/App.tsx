import { FormEvent, useEffect, useState } from 'react'
import Workspace from './Workspace'
import { API_BASE } from './apiBase'
import { closeRealtime } from './realtime'
import { userInitials } from './initials'
type User={id:string;email:string;username:string;displayName:string;status:string;title?:string|null;avatarUrl?:string|null;twoFactorEnabled?:boolean}; type Auth={user:User;accessToken:string;refreshToken:string}; type Project={id:string;name:string}; type Friend={id:string;username:string;displayName:string;status:string}
export type Organization={id:string;name:string;slug:string;role:'OWNER'|'ADMIN'|'MEMBER';inviteCode?:string}
const API=API_BASE
async function api<T>(path:string,init?:RequestInit,token?:string):Promise<T>{let r:Response;try{r=await fetch(API+path,{...init,headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})}})}catch{throw{message:'P2 sunucusuna ulaşılamıyor. PostgreSQL ve backend durumunu kontrol edin.'}}const data=await r.json().catch(()=>({}));if(!r.ok)throw data;return data}

function AuthScreen({done}:{done:(a:Auth,r:boolean)=>void}){const[register,setRegister]=useState(false),[busy,setBusy]=useState(false),[remember,setRemember]=useState(true),[error,setError]=useState(''),[twoFactor,setTwoFactor]=useState(false);async function submit(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const v=Object.fromEntries(new FormData(e.currentTarget));try{const a=await api<Auth>(register?'/auth/register':'/auth/login',{method:'POST',body:JSON.stringify(register?v:{identifier:v.identifier,password:v.password,rememberMe:remember,...(v.code?{code:String(v.code).replace(/\D/g,'')}:{})})});done(a,remember)}catch(x:any){if(x?.code==='TWO_FACTOR_REQUIRED'){setTwoFactor(true);setError('')}else setError(x.message||'Bir hata oluştu.')}finally{setBusy(false)}}return <main className="auth-page"><section className="auth-copy"><span className="logo">P2</span><p className="eyebrow">BUILD · SHARE · PLAY</p><h1>Oyunların<br/>buluşma noktası.</h1><p>Ekibini topla, sürümlerini paylaş ve birlikte test et.</p></section><section className="auth-card"><h2>{register?'Hesap oluştur':'Tekrar hoş geldin'}</h2><p>{register?'Geliştirmeye başlamak için aramıza katıl.':'P2 hesabınla devam et.'}</p><form onSubmit={submit}>{register&&<><label>E-posta<input name="email" type="email" required/></label><label>Kullanıcı adı<input name="username" minLength={3} maxLength={24} pattern="[a-zA-Z0-9_.]+" title="Boşluk kullanma; harf, rakam, nokta ve alt çizgi serbest. Büyük/küçük harf yazdığın gibi korunur." autoCapitalize="off" autoCorrect="off" spellCheck={false} required/></label><label>Görünen ad<input name="displayName" minLength={2} required/></label></>}{!register&&<label>E-posta veya kullanıcı adı<input name="identifier" required autoFocus/></label>}<label>Parola<input name="password" type="password" minLength={8} pattern="(?=.*\p{Ll})(?=.*\p{Lu})(?=.*\d).{8,}" title="En az 8 karakter; en az bir büyük harf, bir küçük harf ve bir rakam içermeli." required/></label>{register&&<label>Parola tekrarı<input name="passwordConfirm" type="password" minLength={8} required/></label>}{twoFactor&&!register&&<label>Doğrulama kodu<input name="code" inputMode="numeric" autoComplete="one-time-code" maxLength={6} placeholder="123456" autoFocus required/><small className="auth-note">Doğrulayıcı uygulamandaki 6 haneli kodu gir.</small></label>}{!register&&<label className="check"><input type="checkbox" checked={remember} onChange={e=>setRemember(e.target.checked)}/> Beni hatırla</label>}{error&&<div className="error">{error}</div>}<button disabled={busy}>{busy?'Bağlanıyor…':register?'Hesap oluştur':twoFactor?'Doğrula ve gir':'Giriş yap'} <span>→</span></button></form><p className="switch">{register?'Zaten hesabın var mı?':'P2’de yeni misin?'} <button onClick={()=>{setRegister(!register);setError('');setTwoFactor(false)}}>{register?'Giriş yap':'Hesap oluştur'}</button></p></section></main>}

function Launcher({auth,logout}:{auth:Auth;logout:()=>void}){const[projects,setProjects]=useState<Project[]>([]),[friends,setFriends]=useState<Friend[]>([]);useEffect(()=>{Promise.all([api<Project[]>('/projects',{},auth.accessToken),api<Friend[]>('/friends',{},auth.accessToken)]).then(([p,f])=>{setProjects(p);setFriends(f)}).catch(()=>{})},[auth.accessToken]);return <main className="launcher"><aside className="sidebar"><div className="side-brand"><span className="logo">P2</span><b>LIBRARY</b></div><p className="section-title">PROJELERİM</p><nav>{projects.map(p=><button key={p.id}>◈ {p.name}</button>)}{!projects.length&&<div className="empty-small">Henüz proje yok.</div>}</nav><button className="new-project">＋ Yeni proje</button></aside><section className="workspace"><header><div><p className="eyebrow">P2 WORKSPACE</p><h2>Hoş geldin, {auth.user.displayName}</h2></div><div className="profile"><span>{userInitials(auth.user.displayName)}</span><div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div><button onClick={logout}>Çıkış</button></div></header><div className="welcome"><span className="orb">P2</span><h1>İlk projen için hazırsın.</h1><p>Build’lerini ekibinle paylaşacağın ve oyunlarını test edeceğin alan burada.</p><button>＋ Proje oluştur</button></div></section><aside className="friends"><header><b>EKİP</b><span>{friends.filter(f=>f.status==='ONLINE').length} çevrimiçi</span></header>{friends.map(f=><div className="friend" key={f.id}><i className={f.status.toLowerCase()}/><span><b>{f.displayName}</b><small>{f.status==='ONLINE'?'Çevrimiçi':'Çevrimdışı'}</small></span></div>)}{!friends.length&&<div className="friends-empty">Arkadaşların burada görünecek.</div>}<button className="friends-add">＋ Arkadaş ekle</button></aside></main>}

function OrganizationOnboarding({auth,complete}:{auth:Auth;complete:(organization:Organization)=>void}){
  const[mode,setMode]=useState<'choose'|'create'|'join'|'created'>('choose'),[error,setError]=useState(''),[busy,setBusy]=useState(false),[created,setCreated]=useState<Organization|null>(null)
  const slugify=(value:string)=>value.toLocaleLowerCase('tr-TR').normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/ı/g,'i').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'').slice(0,40)
  async function create(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const name=String(new FormData(e.currentTarget).get('name')||'').trim();try{const organization=await api<Organization>('/organizations',{method:'POST',body:JSON.stringify({name,slug:slugify(name)})},auth.accessToken);setCreated(organization);setMode('created')}catch(x:any){setError(x.message||'Kuruluş oluşturulamadı.')}finally{setBusy(false)}}
  async function join(e:FormEvent<HTMLFormElement>){e.preventDefault();setBusy(true);setError('');const code=String(new FormData(e.currentTarget).get('code')||'');try{complete(await api<Organization>('/organizations/join',{method:'POST',body:JSON.stringify({code})},auth.accessToken))}catch(x:any){setError(x.message||'Kuruluşa katılınamadı.')}finally{setBusy(false)}}
  if(mode==='created'&&created)return <main className="org-page"><section className="org-dialog success"><span className="success-mark">✓</span><p className="eyebrow">KURULUŞ HAZIR</p><h1>{created.name}</h1><p>Ekip arkadaşların aşağıdaki kodla kuruluşuna katılabilir.</p><div className="invite-code"><span>{created.inviteCode}</span><button onClick={()=>navigator.clipboard.writeText(created.inviteCode||'')}>Kopyala</button></div><button className="primary" onClick={()=>complete(created)}>Çalışma alanına geç <span>→</span></button></section></main>
  return <main className="org-page"><section className="org-shell"><header><p className="eyebrow">P2 ORGANIZATIONS</p><h1>{mode==='choose'?'Ekibinle çalışmaya başla.':mode==='create'?'Yeni bir kuruluş oluştur.':'Davet kodunla katıl.'}</h1><p>{mode==='choose'?'Projeler, build’ler ve ekip üyeleri bir kuruluş altında birlikte yaşar.':mode==='create'?'Kuruluşunun sahibi olacaksın ve ekibini davet edebileceksin.':'Ekip yöneticinin paylaştığı davet kodunu gir.'}</p></header>{mode==='choose'?<div className="org-options"><button onClick={()=>setMode('join')}><span className="option-icon">↗</span><b>Bir kuruluşa katıl</b><small>Mevcut ekibinden aldığın davet koduyla çalışma alanına gir.</small><i>Devam et →</i></button><button className="featured" onClick={()=>setMode('create')}><em>ÖNERİLEN</em><span className="option-icon">＋</span><b>Kuruluş oluştur</b><small>Yeni bir ekip kur, projelerini yönet ve üyelerini davet et.</small><i>Oluştur →</i></button></div>:<form className="org-form" onSubmit={mode==='create'?create:join}>{mode==='create'?<label>Kuruluş adı<input name="name" minLength={2} maxLength={60} placeholder="Örn. Northstar Games" autoFocus required/><small>Kuruluş adresi addan otomatik oluşturulur.</small></label>:<label>Davet kodu<input name="code" minLength={6} maxLength={20} placeholder="P2-XXXXXXXX" autoFocus required/></label>}{error&&<div className="error">{error}</div>}<div className="form-actions"><button type="button" onClick={()=>{setMode('choose');setError('')}}>← Geri</button><button className="primary" disabled={busy}>{busy?'İşleniyor…':mode==='create'?'Kuruluşu oluştur':'Kuruluşa katıl'} <span>→</span></button></div></form>}</section></main>
}

function OrganizationGate({auth,logout}:{auth:Auth;logout:()=>void}){const[loading,setLoading]=useState(true),[organization,setOrganization]=useState<Organization|null>(null);useEffect(()=>{api<Organization[]>('/organizations',{},auth.accessToken).then(items=>{if(items.length)setOrganization(items[0])}).finally(()=>setLoading(false))},[auth.accessToken]);if(loading)return <main className="splash"><span className="logo">P2</span><p>Kuruluşlar yükleniyor…</p></main>;return organization?<Workspace auth={auth} organization={organization} logout={logout}/>:<OrganizationOnboarding auth={auth} complete={setOrganization}/>}

// Masaüstü sürümü iki şekilde kurulabilir: sunucuyu bu bilgisayarda barındırmak ya da
// yalnızca istemci olarak başka birinin sunucusuna bağlanmak. Seçim ilk açılışta sorulur
// ve Ayarlar > Uygulama > Sunucu adresi bölümünden değiştirilebilir.
export function readInstallMode():'host'|'client'|''{
  try{return(window.localStorage.getItem('p2:mode')as'host'|'client'|null)||''}catch{return ''}
}
function ModePicker({done}:{done:()=>void}){
  const[mode,setMode]=useState<'choose'|'client'>('choose')
  const[address,setAddress]=useState(window.p2?.suggestedServer||'')
  const[busy,setBusy]=useState(false),[error,setError]=useState('')
  function chooseHost(){
    try{window.localStorage.removeItem('p2:apiUrl');window.localStorage.setItem('p2:mode','host')}catch{}
    done()
  }
  async function connect(e:FormEvent<HTMLFormElement>){
    e.preventDefault();setBusy(true);setError('')
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
    <header><span className="logo">P2</span><div><p className="eyebrow">YALNIZCA İSTEMCİ</p><h2>Sunucuya bağlan</h2></div></header>
    <form onSubmit={connect}>
      <label className="mode-field">Sunucu adresi<input value={address} onChange={e=>setAddress(e.target.value)} placeholder="https://ornek.trycloudflare.com" autoFocus/></label>
      <p className="mode-hint">Sunucuyu barındıran arkadaşının verdiği adresi yaz. Bu bilgisayara PostgreSQL kurulmaz.</p>
      {error&&<div className="error">{error}</div>}
      <div className="mode-actions">
        <button type="button" className="mode-back" onClick={()=>setMode('choose')} disabled={busy}>Geri</button>
        <button className="install-button" disabled={busy}>{busy?'Bağlanılıyor…':'Bağlan'} <span>→</span></button>
      </div>
    </form>
  </section></main>
  return <main className="setup-page"><section className="setup-shell mode-shell">
    <header><span className="logo">P2</span><div><p className="eyebrow">KURULUM TÜRÜ</p><h2>P2'yi nasıl kullanacaksın?</h2></div></header>
    <div className="mode-choices">
      <button type="button" onClick={chooseHost}>
        <b>Sunucu + istemci</b>
        <small>P2 sunucusunu bu bilgisayarda barındır. PostgreSQL bağlantısı istenir; ekibin sana bağlanır.</small>
      </button>
      <button type="button" onClick={()=>setMode('client')}>
        <b>Yalnızca istemci</b>
        <small>Başka birinin sunucusuna bağlan. Kurulum yok, sadece adres ve giriş.</small>
      </button>
    </div>
    <p className="mode-hint">Seçimini sonradan Ayarlar &gt; Uygulama &gt; Sunucu adresi bölümünden değiştirebilirsin.</p>
  </section></main>
}

function Setup({complete}:{complete:(auth:Auth)=>void}){
  const[error,setError]=useState(''),[busy,setBusy]=useState(false)
  async function submit(e:FormEvent<HTMLFormElement>){
    e.preventDefault();setBusy(true);setError('');const v=Object.fromEntries(new FormData(e.currentTarget))
    const result=await window.p2?.database.install({host:String(v.dbHost),port:Number(v.dbPort),username:String(v.dbUser),password:String(v.dbPassword),database:String(v.dbName),ssl:v.dbSsl==='on'})
    if(!result?.ok){setError(result?.message||'Veritabanı kurulamadı.');setBusy(false);return}
    let online=false;for(let i=0;i<30;i++){await new Promise(r=>setTimeout(r,500));try{await api('/health');online=true;break}catch{}}
    if(!online){setError('API başlatılamadı. PostgreSQL bağlantısını kontrol edin.');setBusy(false);return}
    try{const auth=await api<Auth>('/setup/admin',{method:'POST',body:JSON.stringify({email:v.email,username:v.username,displayName:v.displayName,password:v.password,passwordConfirm:v.passwordConfirm})});await window.p2?.session.save(auth.refreshToken);complete(auth)}catch(x:any){setError(x.message||'Admin hesabı oluşturulamadı.');setBusy(false)}
  }
  return <main className="setup-page"><section className="setup-shell"><header><span className="logo">P2</span><div><p className="eyebrow">SELF-HOSTED KURULUM</p><h2>P2 sunucunu hazırla</h2></div></header><form onSubmit={submit}><fieldset><legend><span>1</span> PostgreSQL bağlantısı</legend><div className="form-grid"><label>Sunucu<input name="dbHost" defaultValue="127.0.0.1" required/></label><label>Port<input name="dbPort" type="number" defaultValue="5432" required/></label><label>Yönetici kullanıcı<input name="dbUser" defaultValue="postgres" required/></label><label>Veritabanı adı<input name="dbName" defaultValue="p2" pattern="[a-z][a-z0-9_]*" required/></label><label className="wide">PostgreSQL parolası<input name="dbPassword" type="password" required/></label><label className="check wide"><input name="dbSsl" type="checkbox"/> SSL bağlantısı kullan</label></div></fieldset><fieldset><legend><span>2</span> İlk yönetici hesabı</legend><div className="form-grid"><label>E-posta<input name="email" type="email" required/></label><label>Kullanıcı adı<input name="username" minLength={3} maxLength={24} pattern="[a-zA-Z0-9_.]+" title="Boşluk kullanma; harf, rakam, nokta ve alt çizgi serbest. Büyük/küçük harf yazdığın gibi korunur." autoCapitalize="off" autoCorrect="off" spellCheck={false} required/></label><label className="wide">Görünen ad<input name="displayName" minLength={2} required/></label><label>Parola<input name="password" type="password" minLength={8} pattern="(?=.*\p{Ll})(?=.*\p{Lu})(?=.*\d).{8,}" title="En az 8 karakter; en az bir büyük harf, bir küçük harf ve bir rakam içermeli." required/></label><label>Parola tekrarı<input name="passwordConfirm" type="password" minLength={8} required/></label></div></fieldset>{error&&<div className="error">{error}</div>}<button className="install-button" disabled={busy}>{busy?'P2 kuruluyor…':'Kurulumu tamamla'} <span>→</span></button><small>Veritabanı parolası Windows güvenli kasasında şifrelenerek saklanır.</small></form></section></main>
}

export default function App(){const[auth,setAuth]=useState<Auth|null>(null),[loading,setLoading]=useState(true),[setup,setSetup]=useState(false),[mode,setMode]=useState(readInstallMode());useEffect(()=>{const update=(event:Event)=>setAuth(current=>current?{...current,user:{...current.user,...(event as CustomEvent<User>).detail}}:current);window.addEventListener('p2:profile-updated',update);return()=>window.removeEventListener('p2:profile-updated',update)},[]);async function restore(){try{await api('/health');const state=await api<{installed:boolean}>('/setup/status');if(!state.installed){setSetup(true);setLoading(false);return}}catch{setSetup(true);setLoading(false);return}const t=await window.p2?.session.load();if(t)try{const a=await api<Auth>('/auth/refresh',{method:'POST',body:JSON.stringify({refreshToken:t})});await window.p2?.session.save(a.refreshToken);setAuth(a)}catch{await window.p2?.session.clear()}setLoading(false)}useEffect(()=>{if(!window.p2||mode){restore();return}window.p2.database.status().then(ready=>{if(!ready)return;try{window.localStorage.setItem('p2:mode','host')}catch{};setMode('host')}).catch(()=>{})},[mode]);async function done(a:Auth,r:boolean){if(r)await window.p2?.session.save(a.refreshToken);setAuth(a)}async function logout(){closeRealtime();if(auth)await api('/auth/logout',{method:'POST',body:JSON.stringify({refreshToken:await window.p2?.session.load()})}).catch(()=>{});await window.p2?.session.clear();setAuth(null)}if(window.p2&&!mode)return <ModePicker done={()=>setMode(readInstallMode())}/>;if(loading)return <main className="splash"><span className="logo">P2</span><p>Yükleniyor…</p></main>;if(setup)return <Setup complete={a=>{setAuth(a);setSetup(false)}}/>;return auth?<OrganizationGate auth={auth} logout={logout}/>:<AuthScreen done={done}/>}
