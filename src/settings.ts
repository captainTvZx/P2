import type { QualityId } from './screenShare'
import { defaultAppearance, normalizeAppearance, type Appearance } from './theme'
import { useSyncExternalStore } from 'react'
import type { OverlayHotkeys, OverlayHudPosition, OverlayNotificationSettings, OverlayToastCorner } from './overlay/overlayTypes'

// Cihaza özgü kişisel ayarlar. Cihaz kimlikleri makineye bağlı olduğu için
// sunucuda değil, localStorage'da tutulur.
export type Settings={
  inputDeviceId:string
  outputDeviceId:string
  cameraId:string
  inputVolume:number
  outputVolume:number
  echoCancellation:boolean
  noiseSuppression:boolean
  autoGainControl:boolean
  notificationSound:boolean
  desktopNotifications:boolean
  ringtoneVolume:number
  stunUrl:string
  turnUrl:string
  turnUsername:string
  turnPassword:string
  forceRelay:boolean
  shareQuality:QualityId
  shareFps:number
  shareAudio:boolean
  appearance:Appearance
  penInk:string
  penWidth:number
  voiceMode:'open'|'ptt'|'ptm'
  voiceBitrate:number      // kbps; Opus hedef bit hızı (32–128)
  voiceKey:string
  closeAction:'ask'|'tray'|'minimize'|'quit'
  language:string          // boş: sistem dilinden seçilir
  timeZone:string
  autoTimeZone:boolean
  hour24:boolean
  weekStart:'auto'|'monday'|'sunday'
  workDays:number[]
  workStart:string
  workEnd:string
  outsideHoursStatus:'none'|'AWAY'|'BUSY'
  taskView:'panel'|'center'   // görev ayrıntısı sağda mı ortada mı açılsın
  libraryAutoDownload:boolean  // yeni sürüm yayınlanınca kurulu oyunu kendiliğinden indir
  libraryBandwidthMbps:number  // 0: sınırsız
  shareActivity:boolean        // ne yaptığımı (oyunda, indiriyor…) ekiple paylaş
  minimizeOnLaunch:boolean     // oyun açılınca P2'yi simge durumuna küçült
  announcePlay:boolean         // oyunu çalıştırınca ekibe "X çalıştırdı" bildirimi gitsin
  playToasts:boolean           // ekipten biri oyun çalıştırınca köşede bildirim göster
  libraryOverlay:boolean       // oyun içi P2 overlay (Shift+2) enjekte edilsin
  // ---- Oyun içi overlay ----
  overlayHotkeys:OverlayHotkeys            // 'Shift+2' biçimi; boş = kapalı (bkz. src/hotkeys.ts)
  overlayToastCorner:OverlayToastCorner    // oyun içi bildirim köşesi
  overlayOpacity:number                    // sabitlenmiş pencerelerin varsayılan saydamlığı (0.35–1)
  overlayScale:number                      // overlay arayüz ölçeği (0.8–1.5)
  overlayReduceMotion:boolean              // overlay'de animasyonları azalt
  overlayHud:boolean                       // FPS + ping göstergesi
  overlayHudPosition:OverlayHudPosition    // göstergenin yeri: sol üst / üst orta / sağ üst
  overlayHudDetails:boolean                // göstergede CPU / RAM / GPU / paket kaybı da yazsın
  overlayScreenshotDir:string              // '' = Resimler\P2
  overlayBrowserHome:string                // overlay tarayıcısının açılış sayfası
  overlayNotifications:OverlayNotificationSettings
}

const KEY='p2:settings'
export const defaultSettings:Settings={
  taskView:'panel',
  libraryAutoDownload:true,
  libraryBandwidthMbps:0,
  shareActivity:true,
  minimizeOnLaunch:true,
  announcePlay:true,
  playToasts:true,
  libraryOverlay:true,
  overlayHotkeys:{panel:'Shift+2',screenshot:'F12',mute:'Shift+M',deafen:'',pushToTalk:''},
  overlayToastCorner:'top-right',
  overlayOpacity:0.92,
  overlayScale:1,
  overlayReduceMotion:false,
  overlayHud:false,
  overlayHudPosition:'top-left',
  overlayHudDetails:true,
  overlayScreenshotDir:'',
  overlayBrowserHome:'https://www.google.com',
  overlayNotifications:{messages:true,calls:true,play:true,tasks:true,releases:true,downloads:true},
  inputDeviceId:'default',
  outputDeviceId:'default',
  cameraId:'default',
  inputVolume:100,
  outputVolume:100,
  echoCancellation:true,
  noiseSuppression:true,
  autoGainControl:true,
  notificationSound:true,
  desktopNotifications:true,
  ringtoneVolume:70,
  stunUrl:'stun:stun.l.google.com:19302',
  turnUrl:'',
  turnUsername:'',
  turnPassword:'',
  forceRelay:false,
  shareQuality:'1080p',
  shareFps:30,
  shareAudio:true,
  appearance:defaultAppearance,
  penInk:'',
  penWidth:4,
  voiceMode:'open',
  voiceBitrate:96,
  voiceKey:'Space',
  closeAction:'ask',
  language:'',
  timeZone:'',
  autoTimeZone:true,
  hour24:true,
  weekStart:'auto',
  workDays:[1,2,3,4,5],
  workStart:'09:00',
  workEnd:'18:00',
  outsideHoursStatus:'AWAY',
}

function load():Settings{
  try{
    const raw=window.localStorage.getItem(KEY)
    const loaded=raw?{...defaultSettings,...JSON.parse(raw)}:defaultSettings
    // Eski tema alanları (mode/themeId/custom) burada atılır; bir sonraki kayıtta yazılmaz.
    // İç içe overlay nesneleri eksik anahtar bırakmasın diye varsayılanla birleştirilir.
    return {
      ...loaded,
      appearance:normalizeAppearance(loaded.appearance),
      inputVolume:Math.min(100,loaded.inputVolume),outputVolume:Math.min(100,loaded.outputVolume),ringtoneVolume:Math.min(100,loaded.ringtoneVolume),
      overlayHotkeys:{...defaultSettings.overlayHotkeys,...(loaded.overlayHotkeys||{})},
      overlayNotifications:{...defaultSettings.overlayNotifications,...(loaded.overlayNotifications||{})},
    }
  }catch{return defaultSettings}
}

let current=load()
const listeners=new Set<()=>void>()

// Overlay penceresi ana pencereyle aynı localStorage'ı paylaşır ama kendi bellek kopyasını tutar:
// ana penceredeki bir değişiklik (kısayol, köşe, ölçek) 'storage' olayıyla buraya yansır.
export const OVERLAY_MODE=(()=>{try{return new URLSearchParams(window.location.search).get('overlay')==='1'}catch{return false}})()
if(OVERLAY_MODE&&typeof window!=='undefined'){
  window.addEventListener('storage',event=>{
    if(event.key!==KEY)return
    current=load()
    listeners.forEach(listener=>listener())
  })
}

export function getSettings(){return current}
export function updateSettings(patch:Partial<Settings>){
  if(patch.inputVolume!==undefined)patch.inputVolume=Math.max(0,Math.min(100,patch.inputVolume))
  if(patch.outputVolume!==undefined)patch.outputVolume=Math.max(0,Math.min(100,patch.outputVolume))
  if(patch.ringtoneVolume!==undefined)patch.ringtoneVolume=Math.max(0,Math.min(100,patch.ringtoneVolume))
  current={...current,...patch}
  try{window.localStorage.setItem(KEY,JSON.stringify(current))}catch{}
  listeners.forEach(listener=>listener())
}
export function resetSettings(){updateSettings(defaultSettings)}

function subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener)}}
export const subscribeSettings=subscribe
export function useSettings(){return useSyncExternalStore(subscribe,getSettings,getSettings)}

// Mikrofon akışını ayarlardaki cihaz ve işleme tercihleriyle açar.
export function audioConstraints(settings:Settings=current):MediaTrackConstraints{
  return {
    ...(settings.inputDeviceId&&settings.inputDeviceId!=='default'?{deviceId:{exact:settings.inputDeviceId}}:{}),
    echoCancellation:settings.echoCancellation,
    noiseSuppression:settings.noiseSuppression,
    autoGainControl:settings.autoGainControl,
    // Opus 48 kHz tek kanalla en iyi sonucu verir; ideal istek cihaz desteklemiyorsa sessizce düşer.
    channelCount:{ideal:1},
    sampleRate:{ideal:48000},
  }
}
export function videoConstraints(settings:Settings=current):MediaTrackConstraints{
  return {
    ...(settings.cameraId&&settings.cameraId!=='default'?{deviceId:{exact:settings.cameraId}}:{}),
    width:{ideal:1280},height:{ideal:720},frameRate:{ideal:30},
  }
}
// Çıkış cihazı seçimi (Chromium setSinkId); desteklenmiyorsa sessizce geçilir.
export function applyOutputDevice(element:HTMLMediaElement,settings:Settings=current){
  const sink=(element as HTMLMediaElement&{setSinkId?:(id:string)=>Promise<void>}).setSinkId
  if(!sink||!settings.outputDeviceId||settings.outputDeviceId==='default')return
  sink.call(element,settings.outputDeviceId).catch(()=>{})
}
