// Oyun içi overlay — ana pencere, overlay penceresi ve Electron ana süreci arasındaki ORTAK SÖZLEŞME.
// Buradaki tipler üç tarafta da kullanılır: değiştirirken electron/overlayHost.cjs, electron/overlay-ui-preload.cjs,
// electron/preload.cjs ve native/overlay/shared/p2overlay_ipc.h ile uyumu koru.

// ---- Araçlar (sol araç çubuğu) ----
export type OverlayToolId='voice'|'team'|'chat'|'notes'|'tasks'|'capture'|'summary'|'browser'|'perf'|'library'

// Her araç penceresinin içeriği bu prop'larla çizilir (pencere yöneticisi verir, araç bileşeni alır).
export type OverlayToolProps={
  projectId:string
  visible:boolean          // panel açık mı (kapalıyken yalnız sabitli pencereler "hayalet" olarak çizilir, girdi yok)
  pinned:boolean           // bu pencere sabitli mi (panel açıkken tam arayüz; kapalıyken salt-okunur hayalet)
  minimized:boolean
  instance?:string         // araç örneği (ör. 'tasks:<boardId>' penceresinde boardId); yoksa temel pencere
  focus():void
  requestClose():void
  setTitle?(title:string):void
}

// ---- Kısayollar (DLL tablosu kimlikleri; p2overlay_ipc.h ile aynı) ----
export const HOTKEY_ID={panel:1,screenshot:2,mute:3,pushToTalk:4,deafen:5} as const
export type NativeHotkey={id:number;vk:number;mods:number;passThrough?:boolean} // mods: bit0 shift, bit1 ctrl, bit2 alt
export type OverlayHotkeys={panel:string;screenshot:string;mute:string;deafen:string;pushToTalk:string} // 'Shift+2', 'F12', '' = kapalı

// ---- Ayar tipleri ----
export type OverlayToastCorner='top-right'|'top-left'|'bottom-right'|'bottom-left'
export type OverlayHudPosition='top-left'|'top-center'|'top-right'
export type OverlayNotificationSettings={messages:boolean;calls:boolean;play:boolean;tasks:boolean;releases:boolean;downloads:boolean}

// ---- Bildirim kartı ----
export type OverlayToastKind='info'|'success'|'error'|'message'|'mention'|'call'|'screenshot'|'play'|'play-stop'|'task'|'release'|'download'
export type OverlayToastAction={id:string;label:string;primary?:boolean}
export type OverlayToast={
  id:string|number
  kind?:OverlayToastKind
  title:string
  text?:string
  keys?:string[]
  duration?:number          // ms (2000–20000); sticky ise yok sayılır
  sticky?:boolean           // süresiz; tag ile kapatılır
  tag?:string               // aynı tag'li eski kart yenisiyle değişir / dismiss(tag)
  avatar?:{userId:string;name:string;url?:string|null}
  actions?:OverlayToastAction[]
  payload?:unknown
}
export type OverlayToastActionDetail={toastId:string|number;tag?:string;actionId:string;payload?:unknown}

// ---- Host ↔ overlay penceresi ----
export type OverlayDrawState={toasts:number;pinned:boolean;hud:boolean}
// cpu/ram: tüm sistem; game: yalnız oyun süreci (host eklentisi GetProcessTimes/WorkingSet ile ölçer); loss: aramadaysa WebRTC.
export type OverlayPerf={fps:number;ping:number|null;cpu?:number|null;ram?:{used:number;total:number};gpu?:{util:number;temp:number;memUsed:number;memTotal:number}|null;loss?:number|null;game?:{cpu:number|null;ramMb:number|null}|null}
export type OverlayHotkeyEvent={id:number;down:boolean}
export type OverlayScreenshot={id:number;path:string;width:number;height:number;projectId:string;fallback:boolean;jpegDataUrl?:string}
export type OverlaySessionInfo={pid:number;projectId:string;projectName:string}
export type OverlayEvent={type:string;[key:string]:unknown}

// ---- Ana pencere ↔ overlay (ana süreç üzerinden) ----
export type OverlayPushTopic='auth'|'event'|'voice'|'snapshot'|'open'|'notify'
export type OverlayPush={topic:OverlayPushTopic;payload:any}
export type OverlaySendTopic='auth-request'|'emit'|'call'|'toast-action'|'open-in-app'
export type OverlaySend={topic:OverlaySendTopic;payload:any;pid?:number}

export type OverlayAuthPush={user:Record<string,unknown>;accessToken:string;organization:Record<string,unknown>|null;projects:Record<string,unknown>[];selectedProjectId:string|null}
export type OverlaySnapshot={
  organization:Record<string,unknown>|null
  projects:Record<string,unknown>[]
  selectedProjectId:string|null
  activities:Record<string,{kind:string;projectName:string;detail?:string;percent?:number}|null>
  voiceRooms:Record<string,{userId:string;displayName:string;avatarUrl?:string|null}[]>
  unread:Record<string,number>
  channelUnread:Record<string,number>
  people:Record<string,unknown>[]
  prefs:Record<string,{starred:boolean;notify:string;hidden?:boolean}>
  channels?:{id:string;name:string;type:string;projectId:string}[]   // seçili projenin kanalları (sesli oda adları için)
}
export type OverlayOpenPayload={window:OverlayToolId|string;target?:unknown}   // 'tasks' ya da 'tasks:<boardId>'
export type OverlayOpenInApp={kind:'person'|'channel'|'group'|'task'|'profile'|'settings'|'library'|'overview'|'url';id?:string;projectId?:string;boardId?:string;section?:string;url?:string}

// ---- Ses durumu (ana pencereden 150 ms kısılmış) ----
export type OverlayVoiceParticipant={userId:string;displayName:string;avatarUrl?:string|null;muted:boolean;deafened:boolean;screen:boolean;video:boolean;speaking:boolean;level:number;locallyMuted:boolean;volume:number;pending?:boolean}
export type OverlayVoiceState={
  active:boolean
  kind:'direct'|'channel'|'group'|null
  callId:string|null
  channelId:string|null
  label:string
  ringing:boolean
  connectedAt:number|null
  muted:boolean
  deafened:boolean
  micReady:boolean
  me:string
  participants:OverlayVoiceParticipant[]
  incoming:{callId:string;from:{userId:string;displayName:string};kind:'direct'|'channel'|'group';silent:boolean}|null
  voiceRooms:Record<string,{userId:string;displayName:string;avatarUrl?:string|null}[]>
}
export type OverlayCallAction=
  |{action:'mute'|'deafen'|'leave'|'accept'|'decline'|'silence'|'open'|'toggleScreen'|'toggleCamera'}
  |{action:'joinChannel';channelId:string}
  |{action:'joinGroup';groupId:string}
  |{action:'invite';userId:string}
  |{action:'startDirect';userId:string}
  |{action:'setUserVolume';userId:string;volume:number}
  |{action:'toggleLocalMute';userId:string}
export const EMPTY_VOICE_STATE:OverlayVoiceState={active:false,kind:null,callId:null,channelId:null,label:'',ringing:false,connectedAt:null,muted:false,deafened:false,micReady:false,me:'',participants:[],incoming:null,voiceRooms:{}}

// ---- window.p2overlay (overlay penceresi preload'u) ----
export interface P2OverlayBridge{
  session():OverlaySessionInfo|null
  onVisible(handler:(visible:boolean)=>void):()=>void
  setVisible(visible:boolean):void
  drawState(state:OverlayDrawState):void
  onFocus(handler:(focused:boolean)=>void):()=>void
  onPerf(handler:(perf:OverlayPerf)=>void):()=>void
  onHotkey(handler:(event:OverlayHotkeyEvent)=>void):()=>void
  onScreenshot(handler:(shot:OverlayScreenshot)=>void):()=>void
  onEvent(handler:(event:OverlayEvent)=>void):()=>void
  onToast(handler:(toast:OverlayToast)=>void):()=>void
  onToastDismiss(handler:(tag:string)=>void):()=>void
  toastAction(detail:OverlayToastActionDetail):void
  screenshot():Promise<{ok:boolean;error?:string}>
  setZoom(factor:number):void
  openInMain(detail:OverlayOpenInApp):void
  send(topic:OverlaySendTopic,payload:unknown):void
  onPush(handler:(push:OverlayPush)=>void):()=>void
  // Eski kanallar (çağrı özeti; baloncukla aynı yük): korunur
  onCallState(handler:(state:unknown)=>void):()=>void
  callAction(action:string):void
}

// ---- window.p2.gameOverlay (ana pencere preload'u) ----
export interface GameOverlayApi{
  push(topic:OverlayPushTopic,payload:unknown):void
  onSend(handler:(message:OverlaySend)=>void):()=>void
  setHotkeys(list:NativeHotkey[]):Promise<{ok:boolean}>
  callStats(stats:{rttMs:number|null;lossPct:number|null}):void
  toast(target:'all'|number|{projectId:string},toast:OverlayToast):void
  event(event:OverlayEvent):void
  onToastAction(handler:(detail:OverlayToastActionDetail&{pid?:number})=>void):()=>void
  onPtt(handler:(down:boolean)=>void):()=>void
  onReady(handler:(info:{projectId:string;pid:number;ready:boolean})=>void):()=>void
  pickScreenshotDir():Promise<string|null>
  setVisible(pid:number,visible:boolean):void
  configure(options:{screenshotDir?:string}):void
  onScreenshot(handler:(shot:OverlayScreenshot)=>void):()=>void
}
