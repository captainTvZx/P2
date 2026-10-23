// Oyun içi overlay kısayolları — saf modül (DOM'a modül düzeyinde dokunmaz, vitest ile test edilir).
// Kanonik biçim: `[Ctrl+][Alt+][Shift+]<Tuş>` (örn. 'Shift+2', 'Ctrl+Alt+F12', 'Space'); '' = kapalı.
// Tuş adları KeyboardEvent.code'dan türetilir ve Windows sanal tuş koduna (VK) çevrilir;
// native tarafın (p2overlay_ipc.h) beklediği `NativeHotkey` buradan üretilir.
import { HOTKEY_ID, type NativeHotkey, type OverlayHotkeys } from './overlay/overlayTypes'

export const MOD_SHIFT=1
export const MOD_CTRL=2
export const MOD_ALT=4
export const MOD_PASS_THROUGH=0x80 // bit7: tuş oyuna da iletilsin (bas-konuş)

export const HOTKEY_LABELS:Record<keyof OverlayHotkeys,string>={
  panel:'Paneli aç/kapat',
  screenshot:'Ekran görüntüsü',
  mute:'Mikrofonu sustur',
  deafen:'Sesi kapat (sağırlaştır)',
  pushToTalk:'Bas-konuş',
}

// ---- VK tablosu ----
const VK:Record<string,number>={
  Space:0x20,Tab:0x09,Enter:0x0D,Backspace:0x08,
  ArrowLeft:0x25,ArrowUp:0x26,ArrowRight:0x27,ArrowDown:0x28,
  Insert:0x2D,Delete:0x2E,Home:0x24,End:0x23,PageUp:0x21,PageDown:0x22,
  NumpadMultiply:0x6A,NumpadAdd:0x6B,NumpadSeparator:0x6C,NumpadSubtract:0x6D,NumpadDecimal:0x6E,NumpadDivide:0x6F,
  Semicolon:0xBA,Equal:0xBB,Comma:0xBC,Minus:0xBD,Period:0xBE,Slash:0xBF,Backquote:0xC0,
  BracketLeft:0xDB,Backslash:0xDC,BracketRight:0xDD,Quote:0xDE,
  Pause:0x13,CapsLock:0x14,ScrollLock:0x91,
}
for(let i=0;i<10;i++)VK[String(i)]=0x30+i
for(let i=0;i<26;i++)VK[String.fromCharCode(65+i)]=0x41+i
for(let i=1;i<=24;i++)VK['F'+i]=0x70+(i-1)
for(let i=0;i<10;i++)VK['Numpad'+i]=0x60+i

// Türkçe/okunur çip etiketleri (kanonik ad → ekranda görünen).
const KEY_LABELS:Record<string,string>={
  Space:'Boşluk',Enter:'Enter',Backspace:'⌫',Tab:'Tab',
  ArrowLeft:'←',ArrowRight:'→',ArrowUp:'↑',ArrowDown:'↓',
  Insert:'Insert',Delete:'Delete',Home:'Home',End:'End',PageUp:'Page Up',PageDown:'Page Down',
  Semicolon:';',Equal:'=',Comma:',',Minus:'-',Period:'.',Slash:'/',Backquote:'`',
  BracketLeft:'[',BracketRight:']',Backslash:'\\',Quote:"'",
  NumpadMultiply:'Num *',NumpadAdd:'Num +',NumpadSubtract:'Num -',NumpadDecimal:'Num .',NumpadDivide:'Num /',NumpadSeparator:'Num ,',
  Pause:'Pause',CapsLock:'Caps Lock',ScrollLock:'Scroll Lock',
}

// Hiçbir koşulda kısayol olamayacak tuşlar (Esc arayüzü kapatır, PrintScreen/Win sisteme ait).
const FORBIDDEN=new Set(['Escape','PrintScreen','MetaLeft','MetaRight','Meta','OSLeft','OSRight','ContextMenu'])
const MODIFIER_CODES=new Set(['ShiftLeft','ShiftRight','ControlLeft','ControlRight','AltLeft','AltRight','MetaLeft','MetaRight','OSLeft','OSRight'])

/** KeyboardEvent.code'u kanonik tuş adına çevirir (Digit2→'2', KeyM→'M'); bilinmeyen/yasak → null. */
export function keyNameFromCode(code:string):string|null{
  if(!code||FORBIDDEN.has(code)||MODIFIER_CODES.has(code))return null
  if(/^Digit[0-9]$/.test(code))return code.slice(5)
  if(/^Key[A-Z]$/.test(code))return code.slice(3)
  return code in VK?code:null
}

export function isValidKeyName(name:string):boolean{return name in VK}

/** 'Ctrl+Shift+2' → {vk, mods}; boş veya geçersiz → null. Değiştirici sırası serbest, büyük/küçük harf duyarsız. */
export function parseHotkey(value:string):{vk:number;mods:number}|null{
  if(!value)return null
  const parts=value.split('+').map(part=>part.trim()).filter(Boolean)
  if(!parts.length)return null
  let mods=0
  let key:string|null=null
  for(const part of parts){
    const lower=part.toLowerCase()
    if(lower==='ctrl'||lower==='control'){if(mods&MOD_CTRL)return null;mods|=MOD_CTRL;continue}
    if(lower==='alt'){if(mods&MOD_ALT)return null;mods|=MOD_ALT;continue}
    if(lower==='shift'){if(mods&MOD_SHIFT)return null;mods|=MOD_SHIFT;continue}
    if(key!==null)return null
    key=normaliseKeyName(part)
    if(key===null)return null
  }
  if(key===null)return null
  return {vk:VK[key],mods}
}

// Tuş adını tabloya göre normalize eder: 'm'→'M', 'f12'→'F12', 'space'→'Space', 'pageup'→'PageUp'.
const KEY_BY_LOWER:Record<string,string>={}
for(const name of Object.keys(VK))KEY_BY_LOWER[name.toLowerCase()]=name
function normaliseKeyName(part:string):string|null{
  if(part in VK)return part
  const fromCode=keyNameFromCode(part)
  if(fromCode)return fromCode
  return KEY_BY_LOWER[part.toLowerCase()]||null
}

/** Kanonik dizeyi yeniden kurar: değiştiriciler Ctrl, Alt, Shift sırasıyla. Geçersizse ''. */
export function canonicalHotkey(value:string):string{
  const parsed=parseHotkey(value)
  if(!parsed)return ''
  const key=Object.keys(VK).find(name=>VK[name]===parsed.vk)!
  return buildHotkey(parsed.mods,key)
}

function buildHotkey(mods:number,key:string):string{
  const parts:string[]=[]
  if(mods&MOD_CTRL)parts.push('Ctrl')
  if(mods&MOD_ALT)parts.push('Alt')
  if(mods&MOD_SHIFT)parts.push('Shift')
  parts.push(key)
  return parts.join('+')
}

/** 'Shift+2' → ['Shift','2']; 'Space' → ['Boşluk']; '' → []. Çip olarak çizilir. */
export function formatHotkeyKeys(value:string):string[]{
  if(!value)return []
  const parsed=parseHotkey(value)
  if(!parsed)return value.split('+').map(part=>part.trim()).filter(Boolean)
  const key=Object.keys(VK).find(name=>VK[name]===parsed.vk)!
  const keys:string[]=[]
  if(parsed.mods&MOD_CTRL)keys.push('Ctrl')
  if(parsed.mods&MOD_ALT)keys.push('Alt')
  if(parsed.mods&MOD_SHIFT)keys.push('Shift')
  keys.push(KEY_LABELS[key]||key)
  return keys
}

/** Klavye olayından kanonik dize üretir; yalnız değiştirici basıldıysa veya tuş yasaksa null. */
export function hotkeyFromEvent(event:Pick<KeyboardEvent,'code'|'key'|'ctrlKey'|'altKey'|'shiftKey'|'metaKey'>):string|null{
  if(event.metaKey)return null
  const key=keyNameFromCode(event.code)
  if(!key)return null
  let mods=0
  if(event.ctrlKey)mods|=MOD_CTRL
  if(event.altKey)mods|=MOD_ALT
  if(event.shiftKey)mods|=MOD_SHIFT
  return buildHotkey(mods,key)
}

/** Ayarlardaki kısayolları DLL tablosuna çevirir; boş/geçersiz olanlar atlanır. Bas-konuş oyuna da geçer (bit7). */
export function toNativeHotkeys(hotkeys:OverlayHotkeys):NativeHotkey[]{
  const list:NativeHotkey[]=[]
  for(const name of Object.keys(HOTKEY_ID) as (keyof OverlayHotkeys)[]){
    const parsed=parseHotkey(hotkeys[name]||'')
    if(!parsed)continue
    const passThrough=name==='pushToTalk'
    list.push({id:HOTKEY_ID[name],vk:parsed.vk,mods:passThrough?parsed.mods|MOD_PASS_THROUGH:parsed.mods,...(passThrough?{passThrough:true}:{})})
  }
  return list
}

/** Aynı tuşu paylaşan kısayollar: her çakışan alan için ilk sahibini verir ({mute:'panel'} gibi). */
export function findConflicts(hotkeys:OverlayHotkeys):Partial<Record<keyof OverlayHotkeys,keyof OverlayHotkeys>>{
  const owners=new Map<string,keyof OverlayHotkeys>()
  const result:Partial<Record<keyof OverlayHotkeys,keyof OverlayHotkeys>>={}
  for(const name of Object.keys(HOTKEY_LABELS) as (keyof OverlayHotkeys)[]){
    const parsed=parseHotkey(hotkeys[name]||'')
    if(!parsed)continue
    const signature=`${parsed.mods&7}:${parsed.vk}`
    const owner=owners.get(signature)
    if(owner){result[name]=owner;result[owner]=result[owner]||name}
    else owners.set(signature,name)
  }
  return result
}
