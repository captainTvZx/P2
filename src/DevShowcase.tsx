import { useMemo, useState } from 'react'
import { AuthScreen, ModePicker, OrganizationOnboarding, ServerProblem, Setup, type Auth } from './App'
import SettingsModal from './SettingsModal'
import ProfilePanel, { type ProfileMember } from './ProfilePanel'
import ConfirmDialog from './ConfirmDialog'
import ScreenShareModal from './ScreenShareModal'
import GroupCreator from './GroupCreator'
import MessageRichText from './MessageRichText'
import AttachmentViewer from './AttachmentViewer'
import GitPage from './GitPage'

type DemoId='loading'|'mode'|'login'|'organization'|'setup'|'server-host'|'server-client'|'settings'|'profile'|'confirm'|'share'|'group'|'messages'|'image'|'git'
type DemoItem={id:DemoId;label:string;group:string;hint:string}

const items:DemoItem[]=[
  {id:'loading',label:'Açılış / loading',group:'Başlangıç',hint:'Uygulama yüklenirken'},
  {id:'mode',label:'Kurulum türü',group:'Başlangıç',hint:'Sunucu oluştur veya bağlan'},
  {id:'login',label:'Giriş ve kayıt',group:'Hesap',hint:'Aynı ekrandan kayıt moduna geç'},
  {id:'organization',label:'Kuruluş başlangıcı',group:'Hesap',hint:'Oluşturma ve katılma adımları'},
  {id:'setup',label:'Sunucu kurulumu',group:'Kurulum',hint:'PostgreSQL, yönetici ve servis'},
  {id:'server-host',label:'Yerel sunucu hatası',group:'Hata durumları',hint:'Host yanıt vermiyor'},
  {id:'server-client',label:'Uzak sunucu hatası',group:'Hata durumları',hint:'Bağlantı adresine ulaşılamıyor'},
  {id:'settings',label:'Ayarlar',group:'Pencereler',hint:'Tüm ayar sekmeleri gerçek'},
  {id:'profile',label:'Profil paneli',group:'Pencereler',hint:'Üye profili ve durum'},
  {id:'confirm',label:'Onay penceresi',group:'Pencereler',hint:'Tehlikeli işlem örneği'},
  {id:'share',label:'Ekran paylaşımı',group:'Arama',hint:'Kaynak ve kalite seçimi'},
  {id:'group',label:'Grup oluşturma',group:'Sohbet',hint:'Kişi seçimi ve arama'},
  {id:'messages',label:'Mesaj içerikleri',group:'Sohbet',hint:'Kod, link ve önizleme örnekleri'},
  {id:'image',label:'Görsel görüntüleyici',group:'Dosyalar',hint:'Yakınlaştırma ve sürükleme'},
  {id:'git',label:'Source Control',group:'Araçlar',hint:'Gerçek masaüstü Git başlangıcı'},
]

const mockAuth:Auth={
  user:{id:'demo-user',email:'hasan@example.com',username:'captainTvZ',displayName:'Hasan Özoğlu',status:'ONLINE',title:'Oyun geliştiricisi',twoFactorEnabled:false},
  accessToken:'ui-demo-token',refreshToken:'ui-demo-refresh',
}
const mockMember:ProfileMember={...mockAuth.user,status:'ONLINE',role:'OWNER'}
const people=[
  {id:'demo-1',username:'cem',displayName:'Cem Akkaya',status:'ONLINE',role:'MEMBER',avatarUrl:null},
  {id:'demo-2',username:'asli',displayName:'Aslı Demir',status:'AWAY',role:'ADMIN',avatarUrl:null},
  {id:'demo-3',username:'mert',displayName:'Mert Kaya',status:'BUSY',role:'MEMBER',avatarUrl:null},
]

function DemoBackdrop(){return <div className="dev-demo-backdrop"><aside><span className="dev-demo-project">M</span><i/><i/><i/></aside><header><b>MineTastic</b><span>genel</span></header><nav><b>PROJE KANALLARI</b><span># genel</span><span>🔊 ekip odası</span><b>KİŞİLER — 3</b><span>● Cem Akkaya</span><span>● Aslı Demir</span></nav><main><h2>UI showroom</h2><p>Bu arka plan, açılır pencerenin gerçek uygulama üzerinde nasıl göründüğünü gösterir.</p></main></div>}

function MessageDemo(){return <main className="dev-message-demo"><header><div><p className="eyebrow">MESAJ BİLEŞENLERİ</p><h1>Mesaj ve dosya önizlemeleri</h1></div><small>Gerçek MessageRichText bileşeni</small></header><section><article><span className="dev-avatar">HÖ</span><div><header><b>Hasan Özoğlu</b><small>şimdi</small></header><MessageRichText content={'Google görsel bağlantıları artık otomatik deneniyor.\n\n```tsx\nconst greeting = "Merhaba P2"\nconsole.log(greeting)\n```\n\nhttps://www.youtube.com/watch?v=dQw4w9WgXcQ'}/></div></article><article><span className="dev-avatar alt">CA</span><div><header><b>Cem Akkaya</b><small>2 dakika önce</small></header><MessageRichText content={'**Kalın metin**, *italik metin*, @captainTvZ ve normal bağlantı:\nhttps://example.com'}/></div></article></section></main>}

function DemoScreen({id}:{id:DemoId}){
  const noop=()=>{}
  if(id==='loading')return <main className="splash"><span className="logo">P2</span><p>Çalışma alanın hazırlanıyor…</p></main>
  if(id==='mode')return <ModePicker done={noop} demo/>
  if(id==='login')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><AuthScreen done={noop}/></div>
  if(id==='organization')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><OrganizationOnboarding auth={mockAuth} complete={noop}/></div>
  if(id==='setup')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><Setup complete={noop} back={noop}/></div>
  if(id==='server-host')return <ServerProblem mode="host" reason={'PostgreSQL bağlantısı kurulamadı: connection refused'} retry={noop} reset={noop} changeAddress={noop}/>
  if(id==='server-client')return <ServerProblem mode="client" reason={'P2 sunucusuna ulaşılamıyor.'} retry={noop} reset={noop} changeAddress={noop}/>
  if(id==='messages')return <MessageDemo/>
  if(id==='git')return <GitPage/>
  if(id==='settings')return <div onClickCapture={event=>{const button=(event.target as HTMLElement).closest('button');if(button&&/Servisi kaydet|Şimdilik başlat|Uygula ve yeniden yükle|Çıkış yap/.test(button.textContent||'')){event.preventDefault();event.stopPropagation()}}}><DemoBackdrop/><SettingsModal auth={mockAuth} preference="ONLINE" onPresence={noop} onLogout={noop} onClose={noop}/></div>
  if(id==='profile')return <div className="dev-panel-demo"><DemoBackdrop/><ProfilePanel user={mockAuth.user} member={mockMember} onClose={noop} onMention={noop} onDirect={noop}/></div>
  if(id==='confirm')return <><DemoBackdrop/><ConfirmDialog title="Projeyi sil?" body="Bu işlem projeyi ve kanallarını geri dönüşüm süresine taşır." confirmLabel="Projeyi sil" danger onConfirm={noop} onCancel={noop}/></>
  if(id==='share')return <><DemoBackdrop/><ScreenShareModal onClose={noop} onStart={noop}/></>
  if(id==='group')return <><DemoBackdrop/><GroupCreator people={people} busy={false} error="" onClose={noop} onCreate={noop}/></>
  return <><DemoBackdrop/><AttachmentViewer file={{name:'p2-ui-demo.svg',type:'image/svg+xml',size:418,url:'data:image/svg+xml;charset=utf-8,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="700"><defs><linearGradient id="g"><stop stop-color="#263f8f"/><stop offset="1" stop-color="#11131b"/></linearGradient></defs><rect width="1200" height="700" fill="url(#g)"/><circle cx="600" cy="350" r="180" fill="#5476df" opacity=".35"/><text x="600" y="380" text-anchor="middle" fill="white" font-family="Segoe UI" font-size="110" font-weight="800">P2 UI DEMO</text></svg>')}} onClose={noop}/></>
}

export default function DevShowcase(){
  const initial=(new URLSearchParams(window.location.search).get('ui-demo')||'mode') as DemoId
  const[active,setActive]=useState<DemoId>(items.some(item=>item.id===initial)?initial:'mode')
  const[open,setOpen]=useState(true)
  const groups=useMemo(()=>[...new Set(items.map(item=>item.group))],[])
  function choose(id:DemoId){setActive(id);const url=new URL(window.location.href);url.searchParams.set('ui-demo',id);window.history.replaceState({},'',url)}
  function normal(){const url=new URL(window.location.href);url.searchParams.delete('ui-demo');window.location.href=url.toString()}
  return <div className="dev-showcase">
    <DemoScreen key={active} id={active}/>
    <button type="button" className="dev-showcase-toggle" onClick={()=>setOpen(value=>!value)}><span>◆</span>{open?'Kataloğu gizle':'UI DEMO'}</button>
    {open&&<aside className="dev-showcase-menu"><header><div><b>P2 UI SHOWROOM</b><small>Yalnızca geliştirme modunda</small></div><button onClick={()=>setOpen(false)}>×</button></header><div className="dev-showcase-list">{groups.map(group=><section key={group}><h4>{group}</h4>{items.filter(item=>item.group===group).map(item=><button type="button" key={item.id} className={active===item.id?'active':undefined} onClick={()=>choose(item.id)}><b>{item.label}</b><small>{item.hint}</small></button>)}</section>)}</div><footer><button type="button" onClick={normal}>Normal uygulamaya dön</button><small>Adres: <code>?ui-demo</code></small></footer></aside>}
  </div>
}
