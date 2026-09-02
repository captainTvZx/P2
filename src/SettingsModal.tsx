import { useEffect, useRef, useState } from 'react'
import { avatarHue, userInitials } from './initials'
import { canUseMedia, mediaBlockReason, mediaDevices } from './media'
import { playMessageSound } from './notifications'
import { presenceClass, statusLabel } from './ProfilePanel'
import { applyOutputDevice, audioConstraints, defaultSettings, updateSettings, useSettings, videoConstraints } from './settings'
import { useEscape } from './useEscape'

type Auth={user:{id:string;username:string;displayName:string};accessToken:string}
type Section='media'|'voice'|'notifications'|'app'|'account'|'shortcuts'
const sections:Array<{id:Section;label:string;hint:string}>=[
  {id:'media',label:'Ses ve Görüntü',hint:'Mikrofon, hoparlör, kamera'},
  {id:'voice',label:'Konuşma Modu',hint:'Bas-konuş, bas-sustur'},
  {id:'notifications',label:'Bildirimler',hint:'Ses ve masaüstü bildirimi'},
  {id:'app',label:'Uygulama',hint:'Pencere ve kapatma davranışı'},
  {id:'account',label:'Hesap',hint:'Profil ve durum'},
  {id:'shortcuts',label:'Kısayollar',hint:'Klavye kısayolları'},
]
const presenceOptions=[{value:'ONLINE',label:'Çevrimiçi'},{value:'AWAY',label:'Uzakta'},{value:'BUSY',label:'Meşgul'},{value:'INVISIBLE',label:'Görünmez'}]
const shortcuts=[
  {keys:'Ctrl + ,',what:'Ayarları aç'},
  {keys:'Esc',what:'Son açılan şeyi kapat (menü, panel, düzenleme, alıntı)'},
  {keys:'Enter',what:'Mesajı gönder · öneri listesi açıksa seçimi uygula'},
  {keys:'Shift + Enter',what:'Alt satıra geç'},
  {keys:'Ctrl + B / Ctrl + I',what:'Kalın / italik'},
  {keys:'↑ ↓',what:'@, / ve :emoji önerilerinde gezin'},
]

function Slider({label,value,onChange,suffix='%'}:{label:string;value:number;onChange:(value:number)=>void;suffix?:string}){
  return <label className="settings-slider"><span>{label}<b>{value}{suffix}</b></span><input type="range" min={0} max={100} value={value} onChange={e=>onChange(Number(e.target.value))}/></label>
}
function Toggle({label,hint,checked,onChange}:{label:string;hint?:string;checked:boolean;onChange:(value:boolean)=>void}){
  return <button type="button" className={checked?'settings-toggle on':'settings-toggle'} onClick={()=>onChange(!checked)} role="switch" aria-checked={checked}>
    <span><b>{label}</b>{hint&&<small>{hint}</small>}</span><i/>
  </button>
}

function MediaSection(){
  const settings=useSettings()
  const[devices,setDevices]=useState<MediaDeviceInfo[]>([])
  const[permission,setPermission]=useState<'unknown'|'granted'|'denied'>('unknown')
  const[level,setLevel]=useState(0)
  const[testing,setTesting]=useState(false)
  const[cameraOn,setCameraOn]=useState(false)
  const[error,setError]=useState('')
  const micStream=useRef<MediaStream|null>(null)
  const camStream=useRef<MediaStream|null>(null)
  const preview=useRef<HTMLVideoElement>(null)
  const audioContext=useRef<AudioContext|null>(null)
  const frame=useRef<number|undefined>(undefined)

  async function listDevices(){
    const devices=mediaDevices()
    if(!devices)return
    try{
      const list=await devices.enumerateDevices()
      setDevices(list)
      if(list.some(device=>device.kind==='audioinput'&&device.label))setPermission('granted')
    }catch{setError('Cihaz listesi alınamadı.')}
  }
  useEffect(()=>{
    const devices=mediaDevices()
    if(!devices)return
    void listDevices()
    devices.addEventListener?.('devicechange',listDevices)
    return()=>devices.removeEventListener?.('devicechange',listDevices)
  },[])

  async function askPermission(){
    setError('')
    if(!canUseMedia()){setError(mediaBlockReason());return}
    try{
      const stream=await mediaDevices()!.getUserMedia({audio:true})
      stream.getTracks().forEach(track=>track.stop())
      setPermission('granted')
      await listDevices()
    }catch{
      setPermission('denied')
      setError('Mikrofon izni verilmedi. Windows > Gizlilik ve güvenlik > Mikrofon ayarlarından masaüstü uygulamalarına izin verildiğinden emin ol.')
    }
  }

  function stopMic(){
    if(frame.current!==undefined)cancelAnimationFrame(frame.current)
    frame.current=undefined
    micStream.current?.getTracks().forEach(track=>track.stop())
    micStream.current=null
    void audioContext.current?.close().catch(()=>{})
    audioContext.current=null
    setLevel(0)
  }
  async function startMic(){
    setError('')
    if(!canUseMedia()){setError(mediaBlockReason());return}
    try{
      const stream=await mediaDevices()!.getUserMedia({audio:audioConstraints(settings)})
      micStream.current=stream
      setPermission('granted')
      const context=new AudioContext()
      audioContext.current=context
      const analyser=context.createAnalyser()
      analyser.fftSize=1024
      context.createMediaStreamSource(stream).connect(analyser)
      const data=new Uint8Array(analyser.frequencyBinCount)
      const tick=()=>{
        analyser.getByteTimeDomainData(data)
        let peak=0
        for(const value of data)peak=Math.max(peak,Math.abs(value-128)/128)
        setLevel(Math.min(100,Math.round(peak*140*(settings.inputVolume/100))))
        frame.current=requestAnimationFrame(tick)
      }
      tick()
    }catch{setError('Mikrofon açılamadı. Cihaz seçimini ve izinleri kontrol et.')}
  }
  useEffect(()=>{if(!testing){stopMic();return}void startMic();return stopMic},[testing,settings.inputDeviceId,settings.echoCancellation,settings.noiseSuppression,settings.autoGainControl])

  useEffect(()=>{
    if(!cameraOn||!canUseMedia()){camStream.current?.getTracks().forEach(track=>track.stop());camStream.current=null;if(preview.current)preview.current.srcObject=null;return}
    let live=true
    mediaDevices()!.getUserMedia({video:videoConstraints(settings)}).then(stream=>{
      if(!live){stream.getTracks().forEach(track=>track.stop());return}
      camStream.current=stream
      if(preview.current){preview.current.srcObject=stream;void preview.current.play().catch(()=>{})}
    }).catch(()=>setError('Kamera açılamadı.'))
    return()=>{live=false;camStream.current?.getTracks().forEach(track=>track.stop());camStream.current=null}
  },[cameraOn,settings.cameraId])

  const inputs=devices.filter(device=>device.kind==='audioinput')
  const outputs=devices.filter(device=>device.kind==='audiooutput')
  const cameras=devices.filter(device=>device.kind==='videoinput')
  const named=(device:MediaDeviceInfo,index:number)=>device.label||`Cihaz ${index+1}`

  return <>
    {!canUseMedia()&&<div className="settings-notice warning">
      <b>Bu bağlantıda mikrofon ve kamera kullanılamıyor</b>
      <p>{mediaBlockReason()}</p>
    </div>}
    {canUseMedia()&&permission!=='granted'&&<div className="settings-notice">
      <b>Cihaz adlarını görmek için izin gerekiyor</b>
      <p>Tarayıcı motoru, izin verilmeden mikrofon ve kamera adlarını gizler.</p>
      <button type="button" onClick={askPermission}>Mikrofona izin ver</button>
    </div>}
    <section className="settings-block">
      <h4>Giriş cihazı</h4>
      <select value={settings.inputDeviceId} onChange={e=>updateSettings({inputDeviceId:e.target.value})}>
        <option value="default">Sistem varsayılanı</option>
        {inputs.map((device,index)=><option key={device.deviceId} value={device.deviceId}>{named(device,index)}</option>)}
      </select>
      <Slider label="Giriş ses seviyesi" value={settings.inputVolume} onChange={inputVolume=>updateSettings({inputVolume})}/>
      <div className="mic-test">
        <button type="button" className={testing?'active':undefined} disabled={!canUseMedia()} onClick={()=>setTesting(value=>!value)}>{testing?'Testi durdur':'Mikrofonu test et'}</button>
        <div className="mic-meter"><i style={{width:`${level}%`}}/></div>
      </div>
      <p className="settings-hint">Konuş; çubuk hareket ediyorsa mikrofon çalışıyor demektir.</p>
    </section>
    <section className="settings-block">
      <h4>Çıkış cihazı</h4>
      <select value={settings.outputDeviceId} onChange={e=>updateSettings({outputDeviceId:e.target.value})}>
        <option value="default">Sistem varsayılanı</option>
        {outputs.map((device,index)=><option key={device.deviceId} value={device.deviceId}>{named(device,index)}</option>)}
      </select>
      <Slider label="Çıkış ses seviyesi" value={settings.outputVolume} onChange={outputVolume=>updateSettings({outputVolume})}/>
    </section>
    <section className="settings-block">
      <h4>Kamera</h4>
      <select value={settings.cameraId} onChange={e=>updateSettings({cameraId:e.target.value})}>
        <option value="default">Sistem varsayılanı</option>
        {cameras.map((device,index)=><option key={device.deviceId} value={device.deviceId}>{named(device,index)}</option>)}
      </select>
      <div className="camera-preview">
        <video ref={preview} muted playsInline/>
        {!cameraOn&&<span>Önizleme kapalı</span>}
      </div>
      <button type="button" className="settings-button" disabled={!canUseMedia()} onClick={()=>setCameraOn(value=>!value)}>{cameraOn?'Önizlemeyi kapat':'Kamerayı test et'}</button>
    </section>
    <section className="settings-block">
      <h4>Ses işleme</h4>
      <Toggle label="Yankı engelleme" hint="Hoparlörden geleni mikrofondan ayıklar" checked={settings.echoCancellation} onChange={echoCancellation=>updateSettings({echoCancellation})}/>
      <Toggle label="Gürültü bastırma" hint="Fan, klavye gibi arka plan seslerini azaltır" checked={settings.noiseSuppression} onChange={noiseSuppression=>updateSettings({noiseSuppression})}/>
      <Toggle label="Otomatik kazanç" hint="Ses seviyeni otomatik dengeler" checked={settings.autoGainControl} onChange={autoGainControl=>updateSettings({autoGainControl})}/>
    </section>
    <section className="settings-block">
      <h4>Bağlantı</h4>
      <label className="settings-field"><span>STUN sunucusu</span><input value={settings.stunUrl} onChange={e=>updateSettings({stunUrl:e.target.value})} placeholder={defaultSettings.stunUrl}/></label>
      <p className="settings-hint">Aynı ağdaki aramalar için gerekmez; farklı ağlardan bağlanırken kullanılır.</p>
      <label className="settings-field"><span>TURN sunucusu</span><input value={settings.turnUrl} onChange={e=>updateSettings({turnUrl:e.target.value})} placeholder="turn:sunucu:3478"/></label>
      <label className="settings-field"><span>TURN kullanıcı adı</span><input value={settings.turnUsername} onChange={e=>updateSettings({turnUsername:e.target.value})} placeholder="isteğe bağlı"/></label>
      <label className="settings-field"><span>TURN parolası</span><input type="password" value={settings.turnPassword} onChange={e=>updateSettings({turnPassword:e.target.value})} placeholder="isteğe bağlı"/></label>
      <p className="settings-hint">Katı güvenlik duvarı ya da mobil veri arkasındaki kişilerle ses akmıyorsa gerekir; sesi bir röle üzerinden taşır. Aramadaki herkesin aynı bilgileri girmesi gerekir.</p>
      <button type="button" className="settings-button" onClick={()=>updateSettings({turnUrl:'turn:openrelay.metered.ca:80',turnUsername:'openrelayproject',turnPassword:'openrelayproject'})}>Ücretsiz test sunucusunu doldur</button>
      <Toggle label="Yalnızca TURN üzerinden bağlan" hint="TURN sunucusunun gerçekten çalışıp çalışmadığını denemek için; doğrudan bağlantı denenmez." checked={settings.forceRelay} onChange={forceRelay=>updateSettings({forceRelay})}/>
      <p className="settings-hint">Test sunucusu herkese açıktır ve sesin üçüncü bir sunucudan geçmesi anlamına gelir; kalıcı kullanım için kendi TURN sunucunu gir.</p>
    </section>
    {error&&<p className="settings-error">{error}</p>}
  </>
}

const voiceModes=[
  {value:'open' as const,label:'Normal (açık mikrofon)',hint:'Mikrofonun sürekli açık; sustur düğmesiyle kapatırsın.'},
  {value:'ptt' as const,label:'Bas-konuş',hint:'Yalnızca tuşa basılı tuttuğun sürece sesin gider.'},
  {value:'ptm' as const,label:'Bas-sustur',hint:'Mikrofon açık; tuşa bastığın sürece susarsın.'},
]
function VoiceSection(){
  const settings=useSettings()
  const[listening,setListening]=useState(false)
  useEffect(()=>{
    if(!listening)return
    const capture=(event:globalThis.KeyboardEvent)=>{
      event.preventDefault()
      if(event.key!=='Escape')updateSettings({voiceKey:event.code||event.key})
      setListening(false)
    }
    window.addEventListener('keydown',capture,true)
    return()=>window.removeEventListener('keydown',capture,true)
  },[listening])
  const keyLabel=settings.voiceKey==='Space'?'Boşluk':settings.voiceKey.replace(/^Key|^Digit/,'')
  return <>
    <section className="settings-block">
      <h4>Konuşma modu</h4>
      <div className="settings-choices">{voiceModes.map(mode=>
        <button type="button" key={mode.value} className={settings.voiceMode===mode.value?'active':undefined} onClick={()=>updateSettings({voiceMode:mode.value})}>
          <b>{mode.label}</b><small>{mode.hint}</small>
        </button>)}
      </div>
    </section>
    {settings.voiceMode!=='open'&&<section className="settings-block">
      <h4>Tuş ataması</h4>
      <div className="key-bind">
        <kbd>{listening?'Bir tuşa bas…':keyLabel}</kbd>
        <button type="button" className="settings-button" onClick={()=>setListening(true)}>{listening?'Bekleniyor':'Tuşu değiştir'}</button>
      </div>
      <p className="settings-hint">Yazı yazarken bu tuş metin girişini bozmaz; yalnızca sohbet dışında dinlenir. Vazgeçmek için Esc.</p>
    </section>}
  </>
}

// Tünel adresi değiştiğinde uygulamayı yeniden kurmak gerekmesin diye adres burada değiştirilebilir.
function ServerAddress(){
  const stored=(()=>{try{return window.localStorage.getItem('p2:apiUrl')||''}catch{return ''}})()
  const[value,setValue]=useState(stored)
  const current=stored||window.p2?.apiUrl||window.location.origin
  return <>
    <label className="settings-field"><span>Adres</span><input value={value} onChange={e=>setValue(e.target.value.trim())} placeholder={current}/></label>
    <p className="settings-hint">Şu an kullanılan: {current}. Sunucunun adresi değişirse buraya yaz ve uygula; uygulama yeniden yüklenir. Site adresi kullanıyorsan sonuna /api ekle (örnek: https://ornek.trycloudflare.com/api).</p>
    <div className="settings-row">
      <button type="button" className="settings-button" onClick={()=>{try{value?window.localStorage.setItem('p2:apiUrl',value):window.localStorage.removeItem('p2:apiUrl')}catch{};window.location.reload()}}>Uygula ve yeniden yükle</button>
      <button type="button" className="settings-button" onClick={()=>{try{window.localStorage.removeItem('p2:apiUrl');window.localStorage.removeItem('p2:mode')}catch{};setValue('');window.location.reload()}}>Kurulum türünü yeniden seç</button>
    </div>
  </>
}

function AppSection(){
  const settings=useSettings()
  return <>
    <section className="settings-block">
      <h4>Kapat düğmesine basınca</h4>
      <div className="settings-choices">
        {[{value:'tray' as const,label:'Sistem tepsisine küçült',hint:'Uygulama arka planda çalışmaya devam eder; aramaların ve bildirimlerin sürer.'},
          {value:'minimize' as const,label:'Simge durumuna küçült',hint:'Pencere görev çubuğunda kalır.'},
          {value:'quit' as const,label:'Uygulamadan çık',hint:'Aramalar sonlanır, bildirim gelmez.'},
          {value:'ask' as const,label:'Her seferinde sor',hint:'Kapatırken hangi davranışı istediğin sorulur.'}].map(option=>
          <button type="button" key={option.value} className={settings.closeAction===option.value?'active':undefined} onClick={()=>{updateSettings({closeAction:option.value});window.p2?.windowControls.setCloseAction?.(option.value)}}>
            <b>{option.label}</b><small>{option.hint}</small>
          </button>)}
      </div>
    </section>
    <section className="settings-block">
      <h4>Sunucu adresi</h4>
      <ServerAddress/>
    </section>
    <section className="settings-block">
      <h4>Sesli arama balonu</h4>
      <p className="settings-hint">Uygulama küçültüldüğünde veya tepsiye alındığında, aktif arama için masaüstünde küçük bir pencere görünür. Balonu kapatsan bile arama sürer; başlık çubuğundaki menüden geri açabilirsin.</p>
    </section>
  </>
}

function NotificationSection(){
  const settings=useSettings()
  const test=useRef<HTMLAudioElement|null>(null)
  useEffect(()=>{if(test.current)applyOutputDevice(test.current,settings)},[settings.outputDeviceId])
  return <>
    <section className="settings-block">
      <h4>Bildirimler</h4>
      <Toggle label="Bildirim sesi" hint="Mesaj geldiğinde kısa bir ses çalar" checked={settings.notificationSound} onChange={notificationSound=>updateSettings({notificationSound})}/>
      <Toggle label="Masaüstü bildirimi" hint="Windows bildirimi olarak gösterilir" checked={settings.desktopNotifications} onChange={desktopNotifications=>updateSettings({desktopNotifications})}/>
      <Slider label="Bildirim ve zil sesi seviyesi" value={settings.ringtoneVolume} onChange={ringtoneVolume=>updateSettings({ringtoneVolume})}/>
      <button type="button" className="settings-button" onClick={()=>playMessageSound(true)}>Sesi dene</button>
    </section>
    <p className="settings-hint">Kanal bazlı bildirim tercihini (tüm mesajlar / yalnızca etiketler / sessiz) kanal başlığındaki zil simgesinden ayarlayabilirsin.</p>
  </>
}

function AccountSection({auth,preference,onPresence,onLogout}:{auth:Auth;preference:string;onPresence:(value:string)=>void;onLogout:()=>void}){
  return <>
    <section className="settings-block">
      <h4>Profil</h4>
      <div className="settings-identity">
        <span className={`settings-avatar ${avatarHue(auth.user.id)}`}>{userInitials(auth.user.displayName)}</span>
        <div><b>{auth.user.displayName}</b><small>@{auth.user.username}</small></div>
      </div>
    </section>
    <section className="settings-block">
      <h4>Durum</h4>
      <div className="settings-status">{presenceOptions.map(option=>
        <button type="button" key={option.value} className={preference===option.value?'active':undefined} onClick={()=>onPresence(option.value)}>
          <i className={presenceClass(option.value)}/>{option.label}
        </button>)}
      </div>
      <p className="settings-hint">Şu anki görünürlüğün: {preference==='INVISIBLE'?'Görünmez (herkese çevrimdışı görünürsün)':statusLabel(preference)}. Uygulamayı arka plana alırsan 3 dakika sonra otomatik olarak "Uzakta" olursun.</p>
    </section>
    <section className="settings-block">
      <h4>Oturum</h4>
      <button type="button" className="settings-button danger" onClick={onLogout}>Çıkış yap</button>
    </section>
  </>
}

export default function SettingsModal({auth,preference,onPresence,onLogout,onClose}:{auth:Auth;preference:string;onPresence:(value:string)=>void;onLogout:()=>void;onClose:()=>void}){
  const[section,setSection]=useState<Section>('media')
  useEscape(true,onClose)
  return <div className="settings-layer" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}>
    <section className="settings-modal" role="dialog" aria-label="Ayarlar">
      <nav>
        <header><b>Ayarlar</b><small>P2 · sürüm {window.p2?.version||'0.2.1'}</small></header>
        {sections.map(item=><button type="button" key={item.id} className={section===item.id?'active':undefined} onClick={()=>setSection(item.id)}>
          <b>{item.label}</b><small>{item.hint}</small>
        </button>)}
      </nav>
      <div className="settings-body">
        <header>
          <h3>{sections.find(item=>item.id===section)?.label}</h3>
          <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button>
        </header>
        <div className="settings-scroll">
          {section==='media'&&<MediaSection/>}
          {section==='voice'&&<VoiceSection/>}
          {section==='app'&&<AppSection/>}
          {section==='notifications'&&<NotificationSection/>}
          {section==='account'&&<AccountSection auth={auth} preference={preference} onPresence={onPresence} onLogout={onLogout}/>}
          {section==='shortcuts'&&<section className="settings-block"><h4>Klavye kısayolları</h4><div className="shortcut-list">{shortcuts.map(item=><div key={item.keys}><kbd>{item.keys}</kbd><span>{item.what}</span></div>)}</div></section>}
        </div>
      </div>
    </section>
  </div>
}
