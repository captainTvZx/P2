import type { QualityId } from './screenShare'
import { defaultAppearance, type Appearance } from './theme'
import { useSyncExternalStore } from 'react'

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
}

const KEY='p2:settings'
export const defaultSettings:Settings={
  taskView:'panel',
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
    return {...loaded,inputVolume:Math.min(100,loaded.inputVolume),outputVolume:Math.min(100,loaded.outputVolume),ringtoneVolume:Math.min(100,loaded.ringtoneVolume)}
  }catch{return defaultSettings}
}

let current=load()
const listeners=new Set<()=>void>()

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
