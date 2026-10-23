import { expect, test } from 'vitest'
import { HOTKEY_ID, type OverlayHotkeys } from './overlay/overlayTypes'
import { MOD_ALT, MOD_CTRL, MOD_PASS_THROUGH, MOD_SHIFT, canonicalHotkey, findConflicts, formatHotkeyKeys, hotkeyFromEvent, parseHotkey, toNativeHotkeys } from './hotkeys'

const base:OverlayHotkeys={panel:'Shift+2',screenshot:'F12',mute:'Shift+M',deafen:'',pushToTalk:''}

test('kanonik dizeler VK ve değiştirici bitlerine çevrilir', () => {
  expect(parseHotkey('Shift+2')).toEqual({vk:0x32,mods:MOD_SHIFT})
  expect(parseHotkey('F12')).toEqual({vk:0x7B,mods:0})
  expect(parseHotkey('Ctrl+Alt+Shift+M')).toEqual({vk:0x4D,mods:MOD_CTRL|MOD_ALT|MOD_SHIFT})
  expect(parseHotkey('Space')).toEqual({vk:0x20,mods:0})
  expect(parseHotkey('Numpad5')).toEqual({vk:0x65,mods:0})
  expect(parseHotkey('BracketLeft')).toEqual({vk:0xDB,mods:0})
  expect(parseHotkey('ArrowDown')).toEqual({vk:0x28,mods:0})
  expect(parseHotkey('F24')).toEqual({vk:0x87,mods:0})
})

test('değiştirici sırası ve harf büyüklüğü esnek, tekrar eden değiştirici geçersiz', () => {
  expect(parseHotkey('shift+ctrl+m')).toEqual({vk:0x4D,mods:MOD_CTRL|MOD_SHIFT})
  expect(canonicalHotkey('shift+ctrl+m')).toBe('Ctrl+Shift+M')
  expect(canonicalHotkey('Alt+Ctrl+f5')).toBe('Ctrl+Alt+F5')
  expect(parseHotkey('Shift+Shift+2')).toBeNull()
  expect(parseHotkey('Ctrl+Shift')).toBeNull()
  expect(parseHotkey('Shift+2+M')).toBeNull()
})

test('boş ve yasak tuşlar geçersiz', () => {
  expect(parseHotkey('')).toBeNull()
  expect(parseHotkey('Escape')).toBeNull()
  expect(parseHotkey('PrintScreen')).toBeNull()
  expect(parseHotkey('MetaLeft')).toBeNull()
  expect(parseHotkey('Ctrl+Escape')).toBeNull()
  expect(parseHotkey('Bilinmeyen')).toBeNull()
})

test('çip etiketleri Türkçe ve okunur', () => {
  expect(formatHotkeyKeys('Shift+2')).toEqual(['Shift','2'])
  expect(formatHotkeyKeys('Ctrl+Alt+Space')).toEqual(['Ctrl','Alt','Boşluk'])
  expect(formatHotkeyKeys('ArrowLeft')).toEqual(['←'])
  expect(formatHotkeyKeys('Backspace')).toEqual(['⌫'])
  expect(formatHotkeyKeys('Enter')).toEqual(['Enter'])
  expect(formatHotkeyKeys('Comma')).toEqual([','])
  expect(formatHotkeyKeys('')).toEqual([])
})

test('parse → format → parse gidiş dönüşü korunur', () => {
  for(const value of ['Shift+2','F12','Ctrl+Alt+Shift+Numpad0','Space','Ctrl+Period','Alt+PageUp']){
    const parsed=parseHotkey(value)!
    expect(parsed).not.toBeNull()
    expect(parseHotkey(canonicalHotkey(value))).toEqual(parsed)
    expect(canonicalHotkey(canonicalHotkey(value))).toBe(canonicalHotkey(value))
  }
})

function keyEvent(code:string,mods:Partial<{ctrlKey:boolean;altKey:boolean;shiftKey:boolean;metaKey:boolean}>={}){
  return {code,key:code,ctrlKey:false,altKey:false,shiftKey:false,metaKey:false,...mods}
}
test('klavye olayından kanonik dize', () => {
  expect(hotkeyFromEvent(keyEvent('Digit2',{shiftKey:true}))).toBe('Shift+2')
  expect(hotkeyFromEvent(keyEvent('KeyM',{ctrlKey:true,shiftKey:true}))).toBe('Ctrl+Shift+M')
  expect(hotkeyFromEvent(keyEvent('F12'))).toBe('F12')
  expect(hotkeyFromEvent(keyEvent('Space',{altKey:true}))).toBe('Alt+Space')
  expect(hotkeyFromEvent(keyEvent('Numpad5'))).toBe('Numpad5')
  expect(hotkeyFromEvent(keyEvent('Minus'))).toBe('Minus')
  expect(hotkeyFromEvent(keyEvent('ArrowUp'))).toBe('ArrowUp')
  expect(hotkeyFromEvent(keyEvent('ScrollLock'))).toBe('ScrollLock')
})

test('yalnız değiştirici veya yasak tuş olayları null', () => {
  expect(hotkeyFromEvent(keyEvent('ShiftLeft',{shiftKey:true}))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('ControlLeft',{ctrlKey:true}))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('AltRight',{altKey:true}))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('Escape'))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('PrintScreen'))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('MetaLeft',{metaKey:true}))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('KeyM',{metaKey:true}))).toBeNull()
  expect(hotkeyFromEvent(keyEvent('IntlBackslash'))).toBeNull()
})

test('native tablo: kimlikler, bitler ve bas-konuş geçişi', () => {
  const list=toNativeHotkeys({...base,deafen:'Ctrl+D',pushToTalk:'Alt+Space'})
  expect(list.map(item=>item.id).sort()).toEqual([HOTKEY_ID.panel,HOTKEY_ID.screenshot,HOTKEY_ID.mute,HOTKEY_ID.pushToTalk,HOTKEY_ID.deafen].sort())
  const panel=list.find(item=>item.id===HOTKEY_ID.panel)!
  expect(panel).toEqual({id:1,vk:0x32,mods:MOD_SHIFT})
  expect(panel.passThrough).toBeUndefined()
  const ptt=list.find(item=>item.id===HOTKEY_ID.pushToTalk)!
  expect(ptt.vk).toBe(0x20)
  expect(ptt.mods&MOD_PASS_THROUGH).toBe(MOD_PASS_THROUGH)
  expect(ptt.mods&7).toBe(MOD_ALT)
  expect(ptt.passThrough).toBe(true)
  const deafen=list.find(item=>item.id===HOTKEY_ID.deafen)!
  expect(deafen.mods).toBe(MOD_CTRL)
  expect(deafen.vk).toBe(0x44)
})

test('boş ve geçersiz kısayollar native tabloya girmez', () => {
  const list=toNativeHotkeys({...base,mute:'Escape',deafen:'',pushToTalk:''})
  expect(list.map(item=>item.id)).toEqual([HOTKEY_ID.panel,HOTKEY_ID.screenshot])
  expect(toNativeHotkeys({panel:'',screenshot:'',mute:'',deafen:'',pushToTalk:''})).toEqual([])
})

test('çakışmalar her iki tarafta işaretlenir, boşlar çakışmaz', () => {
  expect(findConflicts(base)).toEqual({})
  expect(findConflicts({...base,mute:'Shift+2'})).toEqual({mute:'panel',panel:'mute'})
  expect(findConflicts({...base,mute:'shift+2'})).toEqual({mute:'panel',panel:'mute'})
  expect(findConflicts({...base,deafen:'',pushToTalk:''})).toEqual({})
  // Aynı tuş, farklı değiştirici → çakışma değil
  expect(findConflicts({...base,mute:'Ctrl+2'})).toEqual({})
  const triple=findConflicts({...base,mute:'F12',deafen:'F12'})
  expect(triple.mute).toBe('screenshot')
  expect(triple.deafen).toBe('screenshot')
  expect(triple.screenshot).toBe('mute')
})
