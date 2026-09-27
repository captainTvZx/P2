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
import GitPage from './GitPage'
import UpdateBanner from './UpdateBanner'

type DemoId='loading'|'update-check'|'update-download'|'update-ready'|'mode'|'login'|'organization'|'setup'|'server-host'|'server-client'|'settings'|'profile'|'confirm'|'share'|'group'|'messages'|'image'|'git'|'board'|'dropzone'
type DemoItem={id:DemoId;label:string;group:string;hint:string}

const items:DemoItem[]=[
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

function DemoScreen({id}:{id:DemoId}){
  const noop=()=>{}
  const still=()=>Promise.resolve()
  if(id==='loading')return <main className="splash"><span className="logo">P2</span><p>Çalışma alanın hazırlanıyor…</p></main>
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
