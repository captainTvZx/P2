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
  voiceMode:'open'|'ptt'|'ptm'
  voiceKey:string
  closeAction:'ask'|'tray'|'minimize'|'quit'
}

const KEY='p2:settings'
export const defaultSettings:Settings={
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
  voiceMode:'open',
  voiceKey:'Space',
  closeAction:'ask',
}

function load():Settings{
  try{
    const raw=window.localStorage.getItem(KEY)
    return raw?{...defaultSettings,...JSON.parse(raw)}:defaultSettings
  }catch{return defaultSettings}
}

let current=load()
const listeners=new Set<()=>void>()

export function getSettings(){return current}
export function updateSettings(patch:Partial<Settings>){
  current={...current,...patch}
  try{window.localStorage.setItem(KEY,JSON.stringify(current))}catch{}
  listeners.forEach(listener=>listener())
}
export function resetSettings(){updateSettings(defaultSettings)}

function subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener)}}
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
