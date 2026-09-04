import { useEffect, useRef, useState } from 'react'
import { avatarHue, userInitials } from './initials'
import { canUseMedia, mediaBlockReason, mediaDevices } from './media'
import { playMessageSound } from './notifications'
import { presenceClass, statusLabel } from './ProfilePanel'
import { applyOutputDevice, audioConstraints, updateSettings, useSettings, videoConstraints } from './settings'
import { useEscape } from './useEscape'
import AppearanceSettings from './AppearanceSettings'
import { API_BASE } from './apiBase'
import QRCode from 'qrcode'
import { LANGUAGES, formatDateTime, formatTime, systemTimeZone, timeZones, useT, zoneLabel } from './i18n'

type Auth={user:{id:string;username:string;displayName:string;title?:string|null;avatarUrl?:string|null;twoFactorEnabled?:boolean};accessToken:string}

async function api<T>(path:string,token:string,init?:RequestInit):Promise<T>{const response=await fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`}});const data=await response.json().catch(()=>({}));if(!response.ok)throw data;return data}
type Section='media'|'appearance'|'language'|'notifications'|'app'|'account'|'availability'|'shortcuts'
const sections:Array<{id:Section;label:string;hint:string}>=[
  {id:'media',label:'Ses ve Görüntü',hint:'Mikrofon, hoparlör, kamera'},
  {id:'appearance',label:'Görünüm',hint:'Tema, renk modu ve yazı tipi'},
  {id:'language',label:'Dil ve Bölge',hint:'Dil, saat dilimi ve biçim'},
  {id:'notifications',label:'Bildirimler',hint:'Ses ve masaüstü bildirimi'},
  {id:'app',label:'Uygulama',hint:'Pencere ve kapatma davranışı'},
  {id:'account',label:'Hesap ve Güvenlik',hint:'Profil, durum ve 2FA'},
  {id:'availability',label:'Uygunluk',hint:'Çalışma saatleri ve otomatik durum'},
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
    <VoiceSection/>
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

function LanguageSection(){
  const settings=useSettings(),now=new Date()
  return <>
    <section className="settings-block">
      <h4>Dil</h4>
      <p className="settings-hint">Menüler, ayarlar ve bildirim metinleri seçtiğin dilde gösterilir. Eksik çeviriler İngilizceye düşer.</p>
      <div className="language-grid">{LANGUAGES.map(language=><button type="button" key={language.id} className={(settings.language||'tr')===language.id?'active':''} onClick={()=>updateSettings({language:language.id})}><b>{language.native}</b>{language.native!==language.english&&<small>{language.english}</small>}</button>)}</div>
    </section>
    <section className="settings-block">
      <h4>Saat dilimi ve biçim</h4>
      <Toggle label="Saat dilimini otomatik belirle" hint={`Bilgisayarının saat dilimi kullanılır: ${systemTimeZone()}`} checked={settings.autoTimeZone} onChange={autoTimeZone=>updateSettings({autoTimeZone})}/>
      {!settings.autoTimeZone&&<label className="settings-field"><span>Saat dilimi</span><select value={settings.timeZone||systemTimeZone()} onChange={event=>updateSettings({timeZone:event.target.value})}>{timeZones().map(zone=><option key={zone} value={zone}>{zoneLabel(zone)}</option>)}</select></label>}
      <Toggle label="24 saatlik gösterim" hint="Kapalıysa saatler ÖÖ/ÖS biçiminde gösterilir" checked={settings.hour24} onChange={hour24=>updateSettings({hour24})}/>
      <label className="settings-field"><span>Haftanın ilk günü</span><select value={settings.weekStart} onChange={event=>updateSettings({weekStart:event.target.value as typeof settings.weekStart})}><option value="auto">Bölgeye göre</option><option value="monday">Pazartesi</option><option value="sunday">Pazar</option></select></label>
      <div className="region-preview"><div><small>Mesaj saati</small><b>{formatTime(now)}</b></div><div><small>Tam tarih</small><b>{formatDateTime(now)}</b></div></div>
    </section>
  </>
}

function AvailabilitySection(){
  const settings=useSettings()
  const days=[['Pz',0],['Pt',1],['Sa',2],['Ça',3],['Pe',4],['Cu',5],['Ct',6]] as const
  function day(value:number){const next=settings.workDays.includes(value)?settings.workDays.filter(item=>item!==value):[...settings.workDays,value].sort();updateSettings({workDays:next})}
  return <>
    <section className="settings-block">
      <h4>Çalışma günleri</h4>
      <p className="settings-hint">Ekip arkadaşların sana ne zaman ulaşabileceklerini bilir.</p>
      <div className="work-days">{days.map(([label,value])=><button type="button" key={value} className={settings.workDays.includes(value)?'active':''} onClick={()=>day(value)}>{label}</button>)}</div>
      <div className="work-hours"><label className="settings-field"><span>Başlangıç</span><input type="time" value={settings.workStart} onChange={event=>updateSettings({workStart:event.target.value})}/></label><span>—</span><label className="settings-field"><span>Bitiş</span><input type="time" value={settings.workEnd} onChange={event=>updateSettings({workEnd:event.target.value})}/></label></div>
    </section>
    <section className="settings-block">
      <h4>Çalışma saatleri dışında</h4>
      <div className="settings-choices">{[{value:'none',label:'Durumu değiştirme',hint:'Seçtiğin durum korunur.'},{value:'AWAY',label:'Uzakta göster',hint:'Çalışma saatleri dışında otomatik olarak uzakta görün.'},{value:'BUSY',label:'Meşgul göster',hint:'Bildirimleri azaltmak istediğini ekibine göster.'}].map(option=><button type="button" key={option.value} className={settings.outsideHoursStatus===option.value?'active':''} onClick={()=>updateSettings({outsideHoursStatus:option.value as typeof settings.outsideHoursStatus})}><b>{option.label}</b><small>{option.hint}</small></button>)}</div>
    </section>
  </>
}

function AvatarCropper({file,onCancel,onDone}:{file:File;onCancel:()=>void;onDone:(value:string)=>void}){
  const canvas=useRef<HTMLCanvasElement>(null),[image,setImage]=useState<HTMLImageElement|null>(null),[zoom,setZoom]=useState(1),[x,setX]=useState(50),[y,setY]=useState(50)
  useEffect(()=>{const url=URL.createObjectURL(file),next=new Image();next.onload=()=>setImage(next);next.src=url;return()=>URL.revokeObjectURL(url)},[file])
  function draw(target:HTMLCanvasElement,size:number){if(!image)return;target.width=size;target.height=size;const context=target.getContext('2d');if(!context)return;context.clearRect(0,0,size,size);const scale=Math.max(size/image.width,size/image.height)*zoom,w=image.width*scale,h=image.height*scale,dx=(size-w)*(x/100),dy=(size-h)*(y/100);context.drawImage(image,dx,dy,w,h)}
  useEffect(()=>{if(canvas.current)draw(canvas.current,320)},[image,zoom,x,y])
  function use(){const output=document.createElement('canvas');draw(output,256);onDone(output.toDataURL('image/webp',.82))}
  return <div className="avatar-crop-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onCancel()}}><section className="avatar-crop-dialog"><header><div><b>Profil fotoğrafını ayarla</b><small>Kullanılacak alanı seç</small></div><button type="button" onClick={onCancel}>×</button></header><canvas ref={canvas}/><label><span>Yakınlaştır</span><input type="range" min="1" max="4" step=".05" value={zoom} onChange={event=>setZoom(Number(event.target.value))}/></label><label><span>Yatay konum</span><input type="range" min="0" max="100" value={x} onChange={event=>setX(Number(event.target.value))}/></label><label><span>Dikey konum</span><input type="range" min="0" max="100" value={y} onChange={event=>setY(Number(event.target.value))}/></label><footer><button type="button" className="settings-button" onClick={onCancel}>Vazgeç</button><button type="button" className="settings-button primary" disabled={!image} onClick={use}>Bu alanı kullan</button></footer></section></div>
}

function ProfileEditor({auth}:{auth:Auth}){
  const[name,setName]=useState(auth.user.displayName),[title,setTitle]=useState(auth.user.title||''),[avatar,setAvatar]=useState(auth.user.avatarUrl||''),[cropFile,setCropFile]=useState<File|null>(null),[busy,setBusy]=useState(false),[notice,setNotice]=useState('')
  function choose(file?:File){if(!file)return;if(!file.type.startsWith('image/')){setNotice('Geçerli bir görsel seç.');return}setCropFile(file)}
  async function save(){setBusy(true);setNotice('');try{const user=await api<Auth['user']>('/auth/profile',auth.accessToken,{method:'PATCH',body:JSON.stringify({displayName:name,title,avatarUrl:avatar})});window.dispatchEvent(new CustomEvent('p2:profile-updated',{detail:user}));setNotice('Profil kaydedildi.')}catch(x:any){setNotice(x?.message||'Profil kaydedilemedi.')}finally{setBusy(false)}}
  return <section className="settings-block profile-settings-block"><h4>Profil</h4><div className="profile-editor"><label className="profile-photo">{avatar?<img src={avatar} alt="Profil"/>:<span className={`settings-avatar ${avatarHue(auth.user.id)}`}>{userInitials(name)}</span>}<input type="file" accept="image/*" hidden onChange={event=>{choose(event.target.files?.[0]);event.target.value=''}}/><small>Fotoğrafı değiştir</small></label><div><label className="settings-field"><span>Görünen ad</span><input minLength={2} maxLength={40} value={name} onChange={event=>setName(event.target.value)}/></label><label className="settings-field"><span>Unvan</span><input maxLength={60} value={title} onChange={event=>setTitle(event.target.value)} placeholder="Senior Game Developer"/></label></div></div><div className="settings-row"><button type="button" className="settings-button primary" disabled={busy||name.trim().length<2} onClick={save}>{busy?'Kaydediliyor…':'Profili kaydet'}</button>{avatar&&<button type="button" className="settings-button" onClick={()=>setAvatar('')}>Fotoğrafı kaldır</button>}</div>{notice&&<p className="settings-hint">{notice}</p>}{cropFile&&<AvatarCropper file={cropFile} onCancel={()=>setCropFile(null)} onDone={value=>{setAvatar(value);setCropFile(null);setNotice('Fotoğraf hazır. Kaydetmeyi unutma.')}}/>}</section>
}

// Gizli anahtar doğrulayıcıya elle girilecekse dörderli okunması kolay olur.
function grouped(secret:string){return secret.replace(/(.{4})/g,'$1 ').trim()}

// İki adımlı doğrulama: kurulum karekodu üretir, ilk kod onaylanınca açılır.
function TwoFactorBlock({auth}:{auth:Auth}){
  const[enabled,setEnabled]=useState(!!auth.user.twoFactorEnabled)
  const[setup,setSetup]=useState<{secret:string;otpauth:string}|null>(null)
  const[qr,setQr]=useState('')
  const[code,setCode]=useState('')
  const[busy,setBusy]=useState(false)
  const[error,setError]=useState('')
  const[notice,setNotice]=useState('')
  const[disabling,setDisabling]=useState(false)

  useEffect(()=>{
    if(!setup){setQr('');return}
    let alive=true
    // Karekod çevrimdışı üretilir; hiçbir dış servise anahtar gönderilmez.
    QRCode.toDataURL(setup.otpauth,{margin:1,width:320,color:{dark:'#101317',light:'#ffffff'}}).then(url=>{if(alive)setQr(url)}).catch(()=>{})
    return()=>{alive=false}
  },[setup])

  async function begin(){
    setBusy(true);setError('');setNotice('')
    try{setSetup(await api<{secret:string;otpauth:string}>('/auth/2fa/setup',auth.accessToken,{method:'POST'}))}
    catch(x:any){setError(x?.message||'Kurulum başlatılamadı.')}
    finally{setBusy(false)}
  }
  async function confirm(){
    setBusy(true);setError('')
    try{
      await api('/auth/2fa/enable',auth.accessToken,{method:'POST',body:JSON.stringify({code:code.replace(/\D/g,'')})})
      setEnabled(true);setSetup(null);setCode('');setNotice('İki adımlı doğrulama açıldı.')
    }catch(x:any){setError(x?.message||'Kod doğrulanamadı.')}
    finally{setBusy(false)}
  }
  async function turnOff(){
    setBusy(true);setError('')
    try{
      await api('/auth/2fa/disable',auth.accessToken,{method:'POST',body:JSON.stringify({code:code.replace(/\D/g,'')})})
      setEnabled(false);setDisabling(false);setCode('');setNotice('İki adımlı doğrulama kapatıldı.')
    }catch(x:any){setError(x?.message||'Kod doğrulanamadı.')}
    finally{setBusy(false)}
  }

  return <section className="settings-block">
    <h4>İki adımlı doğrulama</h4>
    <div className={enabled?'twofa-state on':'twofa-state'}>
      <i/><div><b>{enabled?'Açık':'Kapalı'}</b><small>{enabled?'Girişte parolanın yanında doğrulayıcı kodun da istenir.':'Hesabını yalnızca parola koruyor.'}</small></div>
    </div>

    {!enabled&&!setup&&<>
      <p className="settings-hint">Google Authenticator, Microsoft Authenticator, Authy ya da 1Password gibi bir doğrulayıcı uygulamayla çalışır. Kodlar cihazında üretilir, internete ihtiyaç duymaz.</p>
      <button type="button" className="settings-button primary" disabled={busy} onClick={begin}>{busy?'Hazırlanıyor…':'İki adımlı doğrulamayı aç'}</button>
    </>}

    {!enabled&&setup&&<div className="twofa-setup">
      <ol className="twofa-steps">
        <li><b>1.</b> Doğrulayıcı uygulamanı aç ve karekodu okut.</li>
        <li><b>2.</b> Okutamıyorsan anahtarı elle gir.</li>
        <li><b>3.</b> Uygulamadaki 6 haneli kodu aşağıya yaz.</li>
      </ol>
      <div className="twofa-code-area">
        <span className="twofa-qr">{qr?<img src={qr} alt="Karekod"/>:<i>Karekod üretiliyor…</i>}</span>
        <div className="twofa-secret">
          <span>Kurulum anahtarı</span>
          <code>{grouped(setup.secret)}</code>
          <button type="button" className="settings-button" onClick={()=>{void navigator.clipboard?.writeText(setup.secret);setNotice('Anahtar kopyalandı.')}}>Anahtarı kopyala</button>
        </div>
      </div>
      <label className="settings-field"><span>Doğrulama kodu</span>
        <input className="twofa-input" value={code} inputMode="numeric" maxLength={6} placeholder="123456" onChange={event=>setCode(event.target.value.replace(/\D/g,'').slice(0,6))}/>
      </label>
      <div className="settings-row">
        <button type="button" className="settings-button primary" disabled={busy||code.length!==6} onClick={confirm}>{busy?'Doğrulanıyor…':'Doğrula ve aç'}</button>
        <button type="button" className="settings-button" onClick={()=>{setSetup(null);setCode('');setError('')}}>Vazgeç</button>
      </div>
    </div>}

    {enabled&&!disabling&&<button type="button" className="settings-button danger" onClick={()=>{setDisabling(true);setNotice('')}}>İki adımlı doğrulamayı kapat</button>}
    {enabled&&disabling&&<div className="twofa-setup">
      <p className="settings-hint">Kapatmak için doğrulayıcı uygulamandaki güncel kodu gir.</p>
      <label className="settings-field"><span>Doğrulama kodu</span>
        <input className="twofa-input" value={code} inputMode="numeric" maxLength={6} placeholder="123456" onChange={event=>setCode(event.target.value.replace(/\D/g,'').slice(0,6))}/>
      </label>
      <div className="settings-row">
        <button type="button" className="settings-button danger" disabled={busy||code.length!==6} onClick={turnOff}>{busy?'Kapatılıyor…':'Kapat'}</button>
        <button type="button" className="settings-button" onClick={()=>{setDisabling(false);setCode('');setError('')}}>Vazgeç</button>
      </div>
    </div>}

    {error&&<p className="settings-warn">{error}</p>}
    {notice&&!error&&<p className="settings-hint">{notice}</p>}
  </section>
}

function AccountSection({auth,preference,onPresence,onLogout}:{auth:Auth;preference:string;onPresence:(value:string)=>void;onLogout:()=>void}){
  return <>
    <ProfileEditor auth={auth}/>
    <section className="settings-block">
      <h4>Durum</h4>
      <div className="settings-status">{presenceOptions.map(option=>
        <button type="button" key={option.value} className={preference===option.value?'active':undefined} onClick={()=>onPresence(option.value)}>
          <i className={presenceClass(option.value)}/>{option.label}
        </button>)}
      </div>
      <p className="settings-hint">Şu anki görünürlüğün: {preference==='INVISIBLE'?'Görünmez (herkese çevrimdışı görünürsün)':statusLabel(preference)}. Uygulamayı arka plana alırsan 3 dakika sonra otomatik olarak "Uzakta" olursun.</p>
    </section>
    <TwoFactorBlock auth={auth}/>
    <section className="settings-block">
      <h4>Oturum</h4>
      <button type="button" className="settings-button danger" onClick={onLogout}>Çıkış yap</button>
    </section>
  </>
}

export default function SettingsModal({auth,preference,onPresence,onLogout,onClose}:{auth:Auth;preference:string;onPresence:(value:string)=>void;onLogout:()=>void;onClose:()=>void}){
  const[section,setSection]=useState<Section>('media')
  const t=useT()
  useEscape(true,onClose)
  return <div className="settings-layer" onMouseDown={e=>{if(e.target===e.currentTarget)onClose()}}>
    <section className="settings-modal" role="dialog" aria-label="Ayarlar">
      <nav>
        <header><b>{t('set.title')}</b><small>{t('set.version',{version:window.p2?.version||'0.2.1'})}</small></header>
        {sections.map(item=><button type="button" key={item.id} className={section===item.id?'active':undefined} onClick={()=>setSection(item.id)}>
          <b>{t(`sec.${item.id}`)}</b><small>{t(`sec.${item.id}.hint`)}</small>
        </button>)}
      </nav>
      <div className="settings-body">
        <header>
          <h3>{t(`sec.${section}`)}</h3>
          <button type="button" onClick={onClose} aria-label="Kapat" title="Kapat (Esc)">×</button>
        </header>
        <div className="settings-scroll">
          {section==='media'&&<MediaSection/>}
          {section==='appearance'&&<AppearanceSettings/>}
          {section==='language'&&<LanguageSection/>}
          {section==='app'&&<AppSection/>}
          {section==='notifications'&&<NotificationSection/>}
          {section==='account'&&<AccountSection auth={auth} preference={preference} onPresence={onPresence} onLogout={onLogout}/>}
          {section==='availability'&&<AvailabilitySection/>}
          {section==='shortcuts'&&<section className="settings-block"><h4>Klavye kısayolları</h4><div className="shortcut-list">{shortcuts.map(item=><div key={item.keys}><kbd>{item.keys}</kbd><span>{item.what}</span></div>)}</div></section>}
        </div>
      </div>
    </section>
  </div>
}
