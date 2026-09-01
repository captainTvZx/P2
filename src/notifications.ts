import { getSettings } from './settings'

// Bireysel mesaj bildirimleri: kısa bir ses + Windows bildirimi.
let audio:AudioContext|null=null
function context(){
  if(!audio){const Ctor=window.AudioContext||(window as any).webkitAudioContext;if(!Ctor)return null;audio=new Ctor()}
  if(audio.state==='suspended')void audio.resume()
  return audio
}

// Chromium ilk sesi bir kullanıcı hareketinden sonra çalmaya izin veriyor; ilk
// tıklama/tuşta ses motorunu hazırlayıp bildirimlerin sessiz kalmasını önlüyoruz.
if(typeof window!=='undefined'){
  const warm=()=>{context();window.removeEventListener('pointerdown',warm);window.removeEventListener('keydown',warm)}
  window.addEventListener('pointerdown',warm);window.addEventListener('keydown',warm)
}

// Dosya taşımamak için ses WebAudio ile üretiliyor: iki notalı yumuşak bir "ding".
// `force` yalnızca ayarlardaki "sesi dene" butonu için: kapalıyken bile çalar.
export function playMessageSound(force=false){
  try{
    const settings=getSettings()
    if(!force&&!settings.notificationSound)return
    const ctx=context()
    if(!ctx)return
    const level=Math.max(0.02,0.16*(settings.ringtoneVolume/100))
    const now=ctx.currentTime
    const master=ctx.createGain()
    master.gain.setValueAtTime(0.0001,now)
    master.gain.exponentialRampToValueAtTime(level,now+0.012)
    master.gain.exponentialRampToValueAtTime(0.0001,now+0.42)
    master.connect(ctx.destination)
    for(const[frequency,start,length]of[[880,0,0.18],[1318.5,0.09,0.30]] as Array<[number,number,number]>){
      const tone=ctx.createOscillator(),gain=ctx.createGain()
      tone.type='sine'
      tone.frequency.setValueAtTime(frequency,now+start)
      gain.gain.setValueAtTime(0.0001,now+start)
      gain.gain.exponentialRampToValueAtTime(0.9,now+start+0.02)
      gain.gain.exponentialRampToValueAtTime(0.0001,now+start+length)
      tone.connect(gain);gain.connect(master)
      tone.start(now+start);tone.stop(now+start+length+0.02)
    }
  }catch{}
}

export type NotificationTarget={userId?:string;projectId?:string;channelId?:string}
export function showMessageNotification(title:string,body:string,data:NotificationTarget){
  if(!getSettings().desktopNotifications)return
  const preview=body.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,'$1').replace(/\*\*|\*/g,'').trim()||'Yeni mesaj'
  const bridge=window.p2?.notifications
  if(bridge){void bridge.show({title,body:preview,data}).catch(()=>{});return}
  try{
    if(typeof Notification==='undefined')return
    if(Notification.permission==='granted')new Notification(title,{body:preview})
    else if(Notification.permission!=='denied')void Notification.requestPermission().then(permission=>{if(permission==='granted')new Notification(title,{body:preview})})
  }catch{}
}
