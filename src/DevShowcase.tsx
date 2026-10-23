import { useMemo, useState } from 'react'
import { AuthScreen, ModePicker, OrganizationOnboarding, ServerProblem, Setup, type Auth } from './App'
import SettingsModal from './SettingsModal'
import ProfilePanel, { type ProfileMember } from './ProfilePanel'
import ConfirmDialog from './ConfirmDialog'
import ScreenShareModal from './ScreenShareModal'
import GroupCreator from './GroupCreator'
import MessageRichText from './MessageRichText'
import AttachmentViewer from './AttachmentViewer'
import { Board } from './BoardPage'
import { AttachmentQueue, DropZone, useFileDrop } from './attachments'
import UpdateBanner from './UpdateBanner'
import Modal from './ui/Modal'
import Menu, { MenuDivider, MenuItem, MenuLabel } from './ui/Menu'
import Avatar from './ui/Avatar'
import Logo from './ui/Logo'
import { pushToast } from './Toasts'
import { ChannelSidebar } from './Workspace'
import { ComposerIcon } from './ComposerIcons'
import { avatarHue, userInitials } from './initials'
import OverlayApp from './OverlayApp'
import OverlayToasts from './overlay/Toasts'
import Hud from './overlay/Hud'
import { mockDismiss, mockToast } from './overlay/overlayBridge'
import { updateSettings, useSettings } from './settings'
import type { OverlayToastCorner } from './overlay/overlayTypes'
import TasksWindow from './overlay/tools/TasksWindow'
import ProjectNotesPanel from './ProjectNotesPanel'
import { sortNotes, type ProjectNotesApi } from './hooks/useProjectNotes'
import type { ProjectNote } from './api/notes'
import { IconNotes, IconTasks } from './overlay/icons'

type DemoId='ui-kit'|'shell'|'loading'|'update-check'|'update-download'|'update-ready'|'mode'|'login'|'organization'|'setup'|'server-host'|'server-client'|'settings'|'profile'|'confirm'|'share'|'group'|'messages'|'image'|'board'|'dropzone'|'overlay-wm'|'overlay-toasts'|'overlay-notes'|'overlay-tasks'
type DemoItem={id:DemoId;label:string;group:string;hint:string}

const items:DemoItem[]=[
  {id:'ui-kit',label:'UI parçaları (P2 Mavi)',group:'Tasarım',hint:'Düğme, alan, kart, pencere, menü, bildirim'},
  {id:'shell',label:'Kabuk: ray + kenar çubuğu',group:'Tasarım',hint:'Proje rayı, kanal listesi, kişiler'},
  {id:'loading',label:'Açılış / loading',group:'Başlangıç',hint:'Uygulama yüklenirken'},
  {id:'update-check',label:'Güncelleme denetimi',group:'Başlangıç',hint:'Açılışta sürüm kontrolü'},
  {id:'update-download',label:'Güncelleme indirme',group:'Başlangıç',hint:'Açılışta ilerleme çubuğu'},
  {id:'update-ready',label:'Güncelleme şeridi',group:'Başlangıç',hint:'Çalışırken bulunan sürüm'},
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
  {id:'dropzone',label:'Dosya bırakma',group:'Sohbet',hint:'Sürükle bırak ve ek önizlemesi'},
  {id:'image',label:'Görsel görüntüleyici',group:'Dosyalar',hint:'Yakınlaştırma ve sürükleme'},
  {id:'board',label:'Görev panosu',group:'Araçlar',hint:'Sprint ve kanban sütunları'},
  {id:'overlay-wm',label:'Oyun içi overlay',group:'Overlay',hint:'Araç çubuğu, pencereler, sabitleme (sahte köprü)'},
  {id:'overlay-toasts',label:'Overlay bildirimleri',group:'Overlay',hint:'Köşe, kuyruk, yapışkan arama kartı'},
  {id:'overlay-notes',label:'Overlay: Notlar penceresi',group:'Overlay',hint:'Kompakt not editörü (sahte veri)'},
  {id:'overlay-tasks',label:'Overlay: Görevler penceresi',group:'Overlay',hint:'Bana atananlar, pano, satır içi düzenleme (sahte veri)'},
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

const demoComments={t12:[
  {id:'c1',taskId:'t12',authorId:'demo-1',content:'Sürükleme bitince **sıra** hemen kaydedilsin.',createdAt:'2026-09-26T09:12:00.000Z',updatedAt:'2026-09-26T09:12:00.000Z',reactions:[{emoji:'👍',count:2,reacted:true},{emoji:'🔥',count:1,reacted:false}]},
  {id:'c2',taskId:'t12',authorId:'demo-user',content:'Tamam, bu akşam bakıyorum.',createdAt:'2026-09-26T10:02:00.000Z',updatedAt:'2026-09-26T10:02:00.000Z'},
]}
const demoBoards=[
  {id:'b1',name:'Oyun takımı',position:0,projectId:'p1'},
  {id:'b2',name:'Sanat',position:1,projectId:'p1'},
  {id:'b3',name:'Backend',position:2,projectId:'p1'},
]
const demoSprints=[
  {id:'s7',name:'Sprint 7',goal:'Kanban panosunu bitir',status:'ACTIVE' as const,startsAt:'2026-09-22T09:00:00.000Z',endsAt:'2026-10-03T17:00:00.000Z'},
  {id:'s6',name:'Sprint 6',goal:'Sesli sohbet düzeltmeleri',status:'DONE' as const,startsAt:'2026-09-08T09:00:00.000Z',endsAt:'2026-09-19T17:00:00.000Z'},
]
const demoTasks=[
  {number:12,title:'Sürükle bırak sonrası sıra sunucuya yazılsın',status:'TODO',priority:'HIGH',points:3,assigneeId:'demo-1',dueAt:'2026-09-29T17:00:00.000Z'},
  {number:13,title:'Sprint bitince görevler otomatik raporlansın',status:'TODO',priority:'LOW',points:5,assigneeId:null,dueAt:null},
  {number:9,title:'Görev kartına yorum alanı',status:'IN_PROGRESS',priority:'MEDIUM',points:2,assigneeId:'demo-2',dueAt:'2026-09-27T17:00:00.000Z'},
  {number:7,title:'Bildirimlerde görev atamaları',status:'REVIEW',priority:'URGENT',points:8,assigneeId:'demo-user',dueAt:'2026-09-24T17:00:00.000Z'},
  {number:4,title:'Pano yetkileri (görev oluştur / panoyu yönet)',status:'DONE',priority:'MEDIUM',points:1,assigneeId:'demo-3',dueAt:null},
  {number:15,title:'Backlog: ses ayarları ekranı',status:'TODO',priority:'LOW',points:2,assigneeId:null,dueAt:null,backlog:true},
  {number:16,title:'Backlog: bildirim sesleri',status:'TODO',priority:'MEDIUM',points:3,assigneeId:'demo-user',dueAt:null,backlog:true},
].map((task,index)=>({...task,id:`t${task.number}`,description:null,position:index,createdById:'demo-user',sprintId:(task as any).backlog?null:'s7'})) as any

// Sürükle bırak: gerçek useFileDrop/DropZone bileşenleriyle, yüklemesiz.
function DropDemo(){
  const[files,setFiles]=useState<{name:string;type:string;size:number;url:string}[]>([
    {name:'ekran-goruntusu.png',type:'image/png',size:184320,url:'data:image/svg+xml;charset=utf-8,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="120" height="120"><rect width="120" height="120" fill="#3b5ea8"/><circle cx="60" cy="52" r="26" fill="#9dc0ff"/></svg>')},
    {name:'yapilacaklar.pdf',type:'application/pdf',size:822344,url:'#'},
    {name:'notlar.txt',type:'text/plain',size:2048,url:'#'},
  ])
  const drop=useFileDrop(items=>setFiles(current=>[...current,...items.map(file=>({name:file.name,type:file.type||'application/octet-stream',size:file.size,url:URL.createObjectURL(file)}))]))
  const state={files,busy:false,error:'',remove:(index:number)=>setFiles(current=>current.filter((_,i)=>i!==index))} as any
  return <main className="dev-message-demo">
    <header><div><p className="eyebrow">DOSYA PAYLAŞIMI</p><h1>Sohbete sürükle bırak</h1></div><small>Gerçek DropZone bileşeni</small></header>
    <div className="chat-layout dev-drop-demo" {...drop.handlers}>
      <DropZone active={drop.dragging} label="#genel kanalına yüklenecek"/>
      <p className="dev-drop-hint">Bir dosyayı buraya sürükle: sohbetin tamamı bırakma alanıdır, yazma kutusuna denk getirmek gerekmez.</p>
      <AttachmentQueue state={state}/>
    </div>
  </main>
}

// P2 Mavi ortak parçalarının canlı kataloğu: ekran geçişlerinde referans alınır.
function UiKitDemo(){
  const[modal,setModal]=useState(false)
  const[menu,setMenu]=useState<{x:number;y:number;up:number}|null>(null)
  const[on,setOn]=useState(true)
  const[value,setValue]=useState(60)
  const[tab,setTab]=useState('genel')
  const toast=(tone:'ok'|'error'|'info')=>pushToast({tone,title:tone==='ok'?'Sürüm yayınlandı':tone==='error'?'Yükleme başarısız':'Yeni mesaj',text:tone==='ok'?'MineTastic 1.4 canlıya alındı':tone==='error'?'Sunucuya ulaşılamadı, yeniden dene':'Cem Akkaya #genel kanalına yazdı'})
  return <main className="dev-kit">
    <header className="dev-kit-head"><Logo size="lg" pop/><div><span className="p2-kicker">P2 Mavi</span><h1>UI parçaları</h1><p>Token'lar, düğmeler, alanlar, kartlar, pencere, menü, bildirim ve hareket.</p></div></header>
    <section className="dev-kit-row">
      <h4 className="p2-kicker">Düğmeler</h4>
      <div className="dev-kit-line">
        <button className="p2-btn primary">Birincil</button><button className="p2-btn secondary">İkincil</button><button className="p2-btn ghost">Sessiz</button><button className="p2-btn danger">Sil</button>
        <button className="p2-btn icon secondary" aria-label="Ayarlar">⚙</button><button className="p2-btn primary sm">Küçük</button><button className="p2-btn secondary lg">Büyük</button><button className="p2-btn secondary pill">Hap</button><button className="p2-btn primary is-busy">Kaydediliyor</button><button className="p2-btn primary" disabled>Pasif</button>
      </div>
    </section>
    <section className="dev-kit-row">
      <h4 className="p2-kicker">Alanlar</h4>
      <div className="dev-kit-grid">
        <label className="p2-field"><span>Proje adı</span><input className="p2-input" defaultValue="MineTastic"/></label>
        <label className="p2-field"><span>Kategori</span><select className="p2-select" defaultValue="stable"><option value="stable">Kararlı</option><option value="beta">Beta</option></select></label>
        <label className="p2-field"><span>Hatalı alan</span><input className="p2-input invalid" defaultValue="çok kısa"/><small>En az 8 karakter</small></label>
        <label className="p2-field"><span>Notlar</span><textarea className="p2-textarea" defaultValue="Sürüm notları buraya."/></label>
      </div>
      <div className="dev-kit-line">
        <button type="button" role="switch" aria-checked={on} className="p2-toggle" onClick={()=>setOn(v=>!v)}><i/></button>
        <input type="range" className="p2-slider" min={0} max={100} value={value} style={{'--value':`${value}%`} as any} onChange={e=>setValue(Number(e.target.value))}/>
        <div className="p2-tabs segment">{['genel','ekip','sürümler'].map(id=><button key={id} className={`p2-tab${tab===id?' active':''}`} onClick={()=>setTab(id)}>{id}</button>)}</div>
        <div className="p2-tabs line">{['genel','ekip','sürümler'].map(id=><button key={id} className={`p2-tab${tab===id?' active':''}`} onClick={()=>setTab(id)}>{id}</button>)}</div>
      </div>
    </section>
    <section className="dev-kit-row">
      <h4 className="p2-kicker">Rozet, hap, avatar, kısayol</h4>
      <div className="dev-kit-line">
        <span className="p2-badge">3</span><span className="p2-badge soft">Beta</span><span className="p2-badge danger">!</span><span className="p2-badge success">Canlı</span><span className="p2-badge warning">İnceleme</span><span className="p2-badge muted">Taslak</span>
        <span className="p2-pill">Tümü</span><span className="p2-pill active">Benim</span>
        <Avatar id="demo-1" name="Cem Akkaya" size="sm" status="online"/><Avatar id="demo-2" name="Aslı Demir" size="md" status="away"/><Avatar id="demo-3" name="Mert Kaya" size="lg" ring status="busy"/><Avatar id="demo-user" name="Hasan Özoğlu" size="xl" round/>
        <span className="p2-keys"><kbd>Shift</kbd><i>+</i><kbd>2</kbd></span><kbd className="p2-kbd">Esc</kbd>
      </div>
    </section>
    <section className="dev-kit-row">
      <h4 className="p2-kicker">Kart, satır, ilerleme</h4>
      <div className="dev-kit-grid">
        <div className="p2-card pad"><span className="p2-kicker">Kart</span><b>Düz kart</b><p>Cam zemin, ince kenarlık.</p></div>
        <div className="p2-card pad glow"><span className="p2-kicker">Kart · glow</span><b>Vurgulu kart</b><p>Güçlü kenarlık ve mavi parıltı.</p></div>
        <div className="p2-card pad interactive"><span className="p2-kicker">Kart · interactive</span><b>Tıklanabilir</b><p>Hover'da hafifçe yükselir.</p></div>
        <div className="p2-card pad anim-list"><button className="p2-row active"># genel</button><button className="p2-row"># duyurular</button><button className="p2-row">🔊 ekip odası</button><div className="p2-divider"/><div className="p2-progress"><i style={{'--value':'62%'} as any}/></div><div className="p2-progress thin indeterminate"><i/></div></div>
      </div>
    </section>
    <section className="dev-kit-row">
      <h4 className="p2-kicker">Pencere, menü, bildirim</h4>
      <div className="dev-kit-line">
        <button className="p2-btn primary" onClick={()=>setModal(true)}>Pencere aç</button>
        <button className="p2-btn secondary" onClick={e=>{const r=(e.currentTarget as HTMLElement).getBoundingClientRect();setMenu({x:r.left,y:r.bottom+6,up:r.top-6})}}>Menü aç</button>
        <button className="p2-btn secondary" onClick={()=>toast('info')}>Bildirim</button><button className="p2-btn secondary" onClick={()=>toast('ok')}>Başarılı</button><button className="p2-btn secondary" onClick={()=>toast('error')}>Hata</button>
        <button className="p2-btn secondary" onClick={()=>pushToast({kind:'play',user:{id:'demo-1',displayName:'Cem Akkaya'},title:'Cem Akkaya oyunda',text:'MineTastic çalıştırdı'})}>Oyunda kartı</button>
      </div>
      <div className="p2-empty"><Logo size="sm"/><b>Boş durum</b><small>Henüz bir şey yok. Yeni bir şey ekleyerek başla.</small></div>
    </section>
    {modal&&<Modal title="Projeyi sil?" subtitle="Bu işlem projeyi ve kanallarını geri dönüşüm süresine taşır." size="sm" onClose={()=>setModal(false)} footer={<><button className="p2-btn secondary" onClick={()=>setModal(false)}>Vazgeç</button><button className="p2-btn danger" onClick={()=>setModal(false)}>Projeyi sil</button></>}>
      <label className="p2-field"><span>Onaylamak için proje adını yaz</span><input className="p2-input" placeholder="MineTastic"/></label>
    </Modal>}
    {menu&&<Menu anchor={menu} onClose={()=>setMenu(null)}>
      <MenuLabel>Proje</MenuLabel>
      <MenuItem icon="✎" label="Proje ayarları…" kbd="Ctrl+," onSelect={()=>setMenu(null)}/>
      <MenuItem icon="★" label="Yıldızla" onSelect={()=>setMenu(null)}/>
      <MenuItem icon="⧉" label="Yeni pencerede aç" hint="popout" onSelect={()=>setMenu(null)}/>
      <MenuDivider/>
      <MenuItem icon="🗑" label="Projeyi sil" danger onSelect={()=>setMenu(null)}/>
    </Menu>}
  </main>
}

// Kabuk demosu: statik ray + gerçek ChannelSidebar (sahte veriyle, ağ isteği yok).
function ShellDemo(){
  const noop=()=>{}
  const projects=[{id:'p1',name:'MineTastic'},{id:'p2',name:'Tow Truck'},{id:'p3',name:'Aurora'},{id:'p4',name:'Backend'}]
  const categories=[
    {id:'c1',name:'Genel',position:0,channels:[{id:'ch1',name:'genel',type:'TEXT' as const,position:0},{id:'ch2',name:'duyurular',type:'ANNOUNCEMENT' as const,position:1},{id:'ch3',name:'ekip odası',type:'VOICE' as const,position:2}]},
    {id:'c2',name:'Geliştirme',position:1,channels:[{id:'ch4',name:'build-notları',type:'TEXT' as const,position:0},{id:'ch5',name:'fikirler',type:'FORUM' as const,position:1}]},
  ]
  const sidebarPeople=people.map(p=>({...p,role:p.role as any}))
  const groups=[{id:'g1',name:'Sanat ekibi',members:sidebarPeople.slice(0,2),unread:2}] as any
  const perms={can:()=>true,canIn:()=>true} as any
  return <main className="project-blank project-stage" style={{gridTemplateColumns:'72px 252px minmax(0,1fr)'}} data-project-title="MineTastic">
    <aside className="project-rail" aria-label="Projeler">
      <div className="rail-projects">{projects.map((project,index)=><button key={project.id} className={index===0?'rail-project active':'rail-project'} title={project.name}><span className={`rail-avatar tone-${index%6}`}>{project.name.slice(0,2).toUpperCase()}</span>{index===2&&<em className="rail-unread">3</em>}</button>)}</div>
      <div className="rail-actions">
        <button className="rail-action activity" aria-label="Etkinlik"><ComposerIcon name="bell"/><em>5</em></button>
        <button className="rail-action board" aria-label="Görevler"><ComposerIcon name="board"/></button>
        <button className="rail-action documents" aria-label="Dokümanlar"><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M7 3.5h7l4 4V20H7z"/><path d="M14 3.5V8h4M10 12h5M10 15.5h5"/></svg></button>
        <button className="rail-action library" aria-label="Kütüphane"><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M12 3v10M8 9l4 4 4-4M5 20h14M5 20v-4M19 20v-4"/></svg><em>1</em></button>
        <button className="rail-action" aria-label="Yeni proje">＋</button>
        <span className="rail-separator"/>
        <button className={`rail-action rail-profile ${avatarHue(mockAuth.user.id)}`} aria-label="Profilim">{userInitials(mockAuth.user.displayName)}<i className="presence online"/></button>
      </div>
    </aside>
    <ChannelSidebar auth={mockAuth as any} organization={{id:'o1',name:'Red Axe Games',slug:'redaxe',role:'OWNER',inviteCode:'RAX-2026'} as any} project={{id:'p1',name:'MineTastic',slug:'minetastic',ownerId:'demo-user',defaultChannelId:'ch1'} as any}
      categories={categories as any} selected={categories[0].channels[0] as any} onSelect={noop} onDefaultChannel={noop} onChannelCreated={noop} onChannelDeleted={noop} onCategoryCreated={noop} onCategoryDeleted={noop}
      people={sidebarPeople as any} selectedPerson={null} unread={{'demo-2':4}} channelUnread={{ch4:2}} onSelectPerson={noop} onViewProfile={noop} onCallPerson={noop} onResize={noop} typing={new Set(['demo-1'])}
      voiceRooms={{}} voiceLevels={{}}
      prefs={{'CHANNEL:ch2':{starred:true,notify:'ALL'}}} onStar={noop} onHidden={noop} onOpenAdmin={noop} perms={perms} activities={{'demo-1':{kind:'playing',projectName:'MineTastic'}}}
      groups={groups} selectedGroup={null} onSelectGroup={noop} onCreateGroup={noop} onLeaveGroup={noop}/>
    <section className="channel-content-empty"><div className="p2-empty"><Logo size="lg"/><b>Kabuk demosu</b><small>Sağ tık menülerini, kategori katlamayı ve arama kutusunu dene.</small></div></section>
  </main>
}

// Overlay bildirim demosu: sahte köprü (mockToast) üzerinden her tür kart; köşe ve HUD ayarı gerçek ayarları değiştirir.
const CORNERS:{id:OverlayToastCorner;label:string}[]=[{id:'top-right',label:'Sağ üst'},{id:'top-left',label:'Sol üst'},{id:'bottom-right',label:'Sağ alt'},{id:'bottom-left',label:'Sol alt'}]
let demoToastSeq=0
function OverlayToastsDemo(){
  const settings=useSettings()


  return <main className="dev-kit" style={{padding:'28px 32px'}}>
    <div className="ow-demo-stage" aria-hidden="true" style={{zIndex:0}}/>
    <section className="dev-kit-row" style={{position:'relative',zIndex:1}}>
      <h4 className="p2-kicker">Oyun içi bildirimler · köşe: {CORNERS.find(c=>c.id===settings.overlayToastCorner)?.label}</h4>
      <div className="dev-kit-line">
        {CORNERS.map(corner=><button key={corner.id} className={`p2-btn sm${settings.overlayToastCorner===corner.id?' primary':' secondary'}`} onClick={()=>updateSettings({overlayToastCorner:corner.id})}>{corner.label}</button>)}
        <button className={`p2-btn sm${settings.overlayHud?' primary':' secondary'}`} onClick={()=>updateSettings({overlayHud:!settings.overlayHud})}>FPS/ping HUD</button>
        <button className={`p2-btn sm${settings.overlayReduceMotion?' primary':' secondary'}`} onClick={()=>updateSettings({overlayReduceMotion:!settings.overlayReduceMotion})}>Hareketi azalt</button>
      </div>
      <div className="dev-kit-line">
        <button className="p2-btn secondary" onClick={()=>mockToast({title:'P2 Overlay hazır',text:'Aç/kapat:',keys:['Shift','2']})}>Başlangıç (keys)</button>
        <button className="p2-btn secondary" onClick={()=>{const n=++demoToastSeq;mockToast({kind:'message',title:'Cem Akkaya · #genel',text:`Build ${n} hazır, test edebilir misin?`,avatar:{userId:'demo-1',name:'Cem Akkaya'},actions:[{id:'reply',label:'Yanıtla',primary:true},{id:'open',label:'Aç'}]})}}>Mesaj</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'mention',title:'Aslı Demir seni andı',text:'@captainTvZ shader hatası sende mi?',avatar:{userId:'demo-2',name:'Aslı Demir'}})}>Anma</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'call',sticky:true,tag:'call:demo',title:'Mert Kaya arıyor',text:'Sesli arama',avatar:{userId:'demo-3',name:'Mert Kaya'},actions:[{id:'accept',label:'Kabul et',primary:true},{id:'decline',label:'Reddet'}]})}>Arama (yapışkan)</button>
        <button className="p2-btn ghost" onClick={()=>mockDismiss('call:demo')}>Aramayı kapat (tag)</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'play',title:'Cem Akkaya oyunda',text:'MineTastic çalıştırdı',avatar:{userId:'demo-1',name:'Cem Akkaya'}})}>Oyunda</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'task',title:'Sana görev atandı',text:'Kanban sürükle-bırak hatası · Yüksek',actions:[{id:'open',label:'Görevi aç'}]})}>Görev</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'release',title:'Yeni sürüm: 0.9.6',text:'Sürüm notları yayınlandı',actions:[{id:'update',label:'Güncelle',primary:true}]})}>Sürüm</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'download',tag:'dl:demo',title:'İndirme tamamlandı',text:'MineTastic 0.9.6 · 1,2 GB'})}>İndirme</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'screenshot',title:'Ekran görüntüsü alındı',text:'Resimler\\P2\\MineTastic',actions:[{id:'share',label:'Paylaş',primary:true},{id:'folder',label:'Klasörü aç'}]})}>Ekran görüntüsü</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'success',title:'Not kaydedildi',duration:3000})}>Başarılı</button>
        <button className="p2-btn secondary" onClick={()=>mockToast({kind:'error',title:'Mesaj gönderilemedi',text:'Sunucuya ulaşılamıyor'})}>Hata</button>
        <button className="p2-btn primary" onClick={()=>{for(let i=1;i<=5;i++)mockToast({kind:'message',title:`Kuyruk testi ${i}`,text:'En çok 3 kart görünür, kalanı sırada bekler',avatar:{userId:`demo-${(i%3)+1}`,name:['Cem Akkaya','Aslı Demir','Mert Kaya'][i%3]}})}}>5 kart (kuyruk)</button>
      </div>
      <p className="overlay-placeholder">Kartlar masaüstüyle aynı <code>.p2-toast</code> bileşenidir; düğmeler yalnız panel açıkken tıklanır. Köşe ve HUD ayarı gerçek <code>settings</code> anahtarlarını değiştirir.</p>
    </section>
    <OverlayToasts interactive/>
    <Hud shifted={false}/>
  </main>
}

// Overlay araç pencereleri: gerçek bileşen, pencere çerçevesini andıran sabit boyutlu kutu içinde, ağ yok.
function OverlayFrame({title,icon,width,height,children}:{title:string;icon:React.ReactNode;width:number;height:number;children:React.ReactNode}){
  return <main className="dev-kit" style={{display:'grid',placeItems:'center'}}>
    <div style={{width,height,display:'flex',flexDirection:'column',border:'1px solid var(--p2-border-strong)',borderRadius:'var(--p2-r-lg)',background:'var(--p2-bg-1)',boxShadow:'var(--p2-shadow-lg)',overflow:'hidden'}}>
      <div className="ow-head" style={{cursor:'default'}}><span className="ow-head-icon">{icon}</span><span className="ow-title">{title}</span></div>
      <div className="ow-body">{children}</div>
    </div>
  </main>
}
const demoAuthor={id:'demo-user',displayName:'Hasan Özoğlu'}
const demoNotes:ProjectNote[]=[
  {id:'n1',projectId:'p1',authorId:'demo-user',title:'Sprint 7 hatırlatmalar',body:'- Kanban sürükle-bırak sonrası **sıra** sunucuya yazılsın\n- Sesli odada *gürültü bastırma* testi\n- Build 42 notları: [sürüm notları](https://example.com)',visibility:'TEAM',pinned:true,version:3,createdAt:'2026-10-20T09:00:00.000Z',updatedAt:new Date(Date.now()-5*60000).toISOString(),author:demoAuthor},
  {id:'n2',projectId:'p1',authorId:'demo-user',title:'Shader fikirleri',body:'Su yüzeyi için ikinci bir normal map katmanı dene.',visibility:'PRIVATE',pinned:false,version:1,createdAt:'2026-10-21T09:00:00.000Z',updatedAt:new Date(Date.now()-3*3600000).toISOString(),author:demoAuthor},
  {id:'n3',projectId:'p1',authorId:'demo-2',title:'Sanat ekibi: palet',body:'Ana palet: #263f8f, #5476df, #9dc0ff. Karakter çizimlerinde kontrastı artır.',visibility:'TEAM',pinned:false,version:2,createdAt:'2026-10-18T09:00:00.000Z',updatedAt:'2026-10-19T12:00:00.000Z',author:{id:'demo-2',displayName:'Aslı Demir'}},
]
function OverlayNotesDemo(){
  const[notes,setNotes]=useState(demoNotes)
  const patch=(id:string,data:Partial<ProjectNote>)=>setNotes(list=>sortNotes(list.map(note=>note.id===id?{...note,...data,updatedAt:new Date().toISOString()}:note)))
  const api:ProjectNotesApi={notes,loading:false,error:'',savingIds:new Set<string>(),reload:async()=>{},flush:async()=>{},flushAll:()=>{},
    create:async init=>{const note:ProjectNote={id:`n${Date.now()}`,projectId:'p1',authorId:'demo-user',title:'',body:'',visibility:'PRIVATE',pinned:false,version:1,createdAt:new Date().toISOString(),updatedAt:new Date().toISOString(),author:demoAuthor,...init};setNotes(list=>sortNotes([...list,note]));return note},
    save:(id,data)=>patch(id,data),remove:async id=>{setNotes(list=>list.filter(note=>note.id!==id));return true},
    togglePin:id=>setNotes(list=>sortNotes(list.map(note=>note.id===id?{...note,pinned:!note.pinned}:note))),setVisibility:(id,visibility)=>patch(id,{visibility}),
    canEdit:(note,canManage)=>note.authorId==='demo-user'||(note.visibility==='TEAM'&&canManage)}
  return <OverlayFrame title="Notlar · 3" icon={<IconNotes size={16}/>} width={460} height={560}>
    <div className="overlay-notes"><ProjectNotesPanel api={api} token="" organizationId="o1" projectId="p1" me="demo-user" canManage compact/></div>
  </OverlayFrame>
}
function OverlayTasksDemo(){
  const noop=()=>{}
  const tasks=(demoTasks as any[]).map(task=>({...task,boardId:'b1',projectId:'p1',createdAt:'2026-10-20T09:00:00.000Z',updatedAt:'2026-10-22T09:00:00.000Z',dueAt:task.dueAt?new Date(Date.now()+(task.number%2?3:-2)*86400000).toISOString():null}))
  const demo={boards:demoBoards,tasks,me:'demo-user',people:[{id:'demo-user',displayName:'Hasan Özoğlu'},...people]}
  return <OverlayFrame title="Görevler · 2" icon={<IconTasks size={16}/>} width={780} height={600}>
    <TasksWindow projectId="p1" visible pinned={false} minimized={false} focus={noop} requestClose={noop} demo={demo}/>
  </OverlayFrame>
}

function DemoScreen({id}:{id:DemoId}){
  const noop=()=>{}
  const still=()=>Promise.resolve()
  if(id==='ui-kit')return <UiKitDemo/>
  if(id==='shell')return <ShellDemo/>
  if(id==='overlay-wm')return <OverlayApp demo/>
  if(id==='overlay-toasts')return <OverlayToastsDemo/>
  if(id==='overlay-notes')return <OverlayNotesDemo/>
  if(id==='overlay-tasks')return <OverlayTasksDemo/>
  if(id==='loading')return <main className="splash"><Logo size="xl" pop/><p>Çalışma alanın hazırlanıyor…</p></main>
  // Güncelleme ekranları: gerçek bileşen, sahte durumla.
  if(id==='update-check')return <UpdateBanner demoState={{status:'checking',version:'',percent:0,error:'',startup:true,text:'Güncellemeler denetleniyor…'}}/>
  if(id==='update-download')return <UpdateBanner demoState={{status:'downloading',version:'0.4.8',percent:42,error:'',startup:true,text:'Güncelleme indiriliyor'}}/>
  if(id==='update-ready')return <><DemoBackdrop/><UpdateBanner demoState={{status:'ready',version:'0.4.8',percent:100,error:'',startup:false}}/></>
  if(id==='mode')return <ModePicker done={noop} demo/>
  if(id==='login')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><AuthScreen done={noop}/></div>
  if(id==='organization')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><OrganizationOnboarding auth={mockAuth} complete={noop}/></div>
  if(id==='setup')return <div onSubmitCapture={event=>{event.preventDefault();event.stopPropagation()}}><Setup complete={noop} back={noop}/></div>
  if(id==='server-host')return <ServerProblem mode="host" reason={'PostgreSQL bağlantısı kurulamadı: connection refused'} retry={noop} reset={noop} changeAddress={noop}/>
  if(id==='server-client')return <ServerProblem mode="client" reason={'P2 sunucusuna ulaşılamıyor.'} retry={noop} reset={noop} changeAddress={noop}/>
  if(id==='messages')return <MessageDemo/>
  if(id==='dropzone')return <DropDemo/>
  if(id==='board')return <Board token="demo" projectName="MineTastic" boards={demoBoards} boardId="b1" onSelectBoard={noop} onCreateBoard={still} onRenameBoard={still} onDeleteBoard={still} comments={demoComments} onLoadComments={noop} onComment={still} onCommentDelete={still} onCommentEdit={still} onCommentReact={still} tasks={demoTasks} sprints={demoSprints} members={[{id:'demo-user',username:'captainTvZ',displayName:'Hasan Özoğlu',avatarUrl:null},...people]} me="demo-user" canCreate canManage onCreate={still} onUpdate={still} onDelete={still} onMove={still} onSprint={still} onSprintDelete={still}/>
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
  const[open,setOpen]=useState(!new URLSearchParams(window.location.search).has('ui-menu'))
  const groups=useMemo(()=>[...new Set(items.map(item=>item.group))],[])
  function choose(id:DemoId){setActive(id);const url=new URL(window.location.href);url.searchParams.set('ui-demo',id);window.history.replaceState({},'',url)}
  function normal(){const url=new URL(window.location.href);url.searchParams.delete('ui-demo');window.location.href=url.toString()}
  return <div className="dev-showcase">
    <DemoScreen key={active} id={active}/>
    <button type="button" className="dev-showcase-toggle" onClick={()=>setOpen(value=>!value)}><span>◆</span>{open?'Kataloğu gizle':'UI DEMO'}</button>
    {open&&<aside className="dev-showcase-menu"><header><div><b>P2 UI SHOWROOM</b><small>Yalnızca geliştirme modunda</small></div><button onClick={()=>setOpen(false)}>×</button></header><div className="dev-showcase-list">{groups.map(group=><section key={group}><h4>{group}</h4>{items.filter(item=>item.group===group).map(item=><button type="button" key={item.id} className={active===item.id?'active':undefined} onClick={()=>choose(item.id)}><b>{item.label}</b><small>{item.hint}</small></button>)}</section>)}</div><footer><button type="button" onClick={normal}>Normal uygulamaya dön</button><small>Adres: <code>?ui-demo</code></small></footer></aside>}
  </div>
}
