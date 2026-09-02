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


// Sesli oda ve yayın işaretleri: Discord'unkiler gibi kısa, yumuşak ve filtreli iki-üç nota.
type Cue='join'|'leave'|'streamStart'|'streamStop'|'viewerJoin'|'viewerLeave'
const CUES:Record<Cue,{notes:[number,number][];gain:number}>={
  join:{notes:[[523.25,0],[783.99,0.085]],gain:1},            // do -> sol (yükselen)
  leave:{notes:[[783.99,0],[523.25,0.085]],gain:.95},         // sol -> do (alçalan)
  streamStart:{notes:[[659.25,0],[830.61,0.075],[1046.5,0.15]],gain:1.05},
  streamStop:{notes:[[1046.5,0],[830.61,0.075],[659.25,0.15]],gain:.95},
  viewerJoin:{notes:[[987.77,0],[1318.51,0.07]],gain:.8},
  viewerLeave:{notes:[[1318.51,0],[987.77,0.07]],gain:.75},
}

export function playCue(kind:Cue){
  try{
    const settings=getSettings()
    if(!settings.notificationSound)return
    const ctx=context()
    if(!ctx)return
    const cue=CUES[kind]
    const now=ctx.currentTime
    const master=ctx.createGain()
    const soften=ctx.createBiquadFilter()
    soften.type='lowpass'
    soften.frequency.setValueAtTime(2600,now)
    master.gain.setValueAtTime(Math.max(0.02,0.15*cue.gain*(settings.ringtoneVolume/100)),now)
    master.connect(soften);soften.connect(ctx.destination)
    for(const[frequency,start]of cue.notes){
      // İki katman: sinüs taban + hafif detune üçgen, marimba benzeri yumuşak bir tını verir.
      for(const[type,detune,level] of [['sine',0,0.9],['triangle',7,0.28]] as Array<[OscillatorType,number,number]>){
        const tone=ctx.createOscillator(),gain=ctx.createGain()
        tone.type=type
        tone.frequency.setValueAtTime(frequency,now+start)
        tone.detune.setValueAtTime(detune,now+start)
        gain.gain.setValueAtTime(0.0001,now+start)
        gain.gain.exponentialRampToValueAtTime(level,now+start+0.014)
        gain.gain.exponentialRampToValueAtTime(0.0001,now+start+0.26)
        tone.connect(gain);gain.connect(master)
        tone.start(now+start);tone.stop(now+start+0.3)
      }
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
