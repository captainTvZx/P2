import { useSyncExternalStore } from 'react'
import { DICTIONARIES } from './locales'
import type { LangId } from './locales/types'
import { getSettings, subscribeSettings } from './settings'

// Uygulama dili: metinler Türkçe yazıldı, çeviriler locales.ts'te. Bir anahtarın
// karşılığı seçili dilde yoksa İngilizceye, o da yoksa Türkçeye düşülür; böylece
// yeni bir metin eklendiğinde arayüz hiçbir dilde boş kalmaz.
export type Language={id:LangId;native:string;english:string;locale:string}
export const LANGUAGES:Language[]=[
  {id:'tr',native:'Türkçe',english:'Turkish',locale:'tr-TR'},
  {id:'en',native:'English (US)',english:'English (US)',locale:'en-US'},
  {id:'en-GB',native:'English (UK)',english:'English (UK)',locale:'en-GB'},
  {id:'de',native:'Deutsch',english:'German',locale:'de-DE'},
  {id:'es',native:'Español (España)',english:'Spanish (Spain)',locale:'es-ES'},
  {id:'es-LA',native:'Español (Latinoamérica)',english:'Spanish (Latin America)',locale:'es-419'},
  {id:'fr',native:'Français',english:'French',locale:'fr-FR'},
  {id:'pt-BR',native:'Português (Brasil)',english:'Portuguese (Brazil)',locale:'pt-BR'},
  {id:'ja',native:'日本語',english:'Japanese',locale:'ja-JP'},
]

export function languageOf(id:string){return LANGUAGES.find(item=>item.id===id)||LANGUAGES[0]}

// Sistem dili uygulama dillerinden birine yakınsa onu seçer (ilk açılış için).
export function detectLanguage():LangId{
  try{
    for(const tag of navigator.languages||[navigator.language]){
      const exact=LANGUAGES.find(item=>item.locale.toLowerCase()===tag.toLowerCase()||item.id.toLowerCase()===tag.toLowerCase())
      if(exact)return exact.id
      const base=tag.split('-')[0].toLowerCase()
      const loose=LANGUAGES.find(item=>item.id.split('-')[0]===base)
      if(loose)return loose.id
    }
  }catch{}
  return 'tr'
}

export function translate(lang:string,key:string,vars?:Record<string,string|number>){
  const text=DICTIONARIES[lang as LangId]?.[key]??DICTIONARIES.en[key]??DICTIONARIES.tr[key]??key
  return vars?text.replace(/\{(\w+)\}/g,(match:string,name:string)=>String(vars[name]??match)):text
}

export type Translate=(key:string,vars?:Record<string,string|number>)=>string

let detected=''
const currentLanguage=()=>languageOf(getSettings().language||(detected||=detectLanguage())).id
export function useLanguage(){return useSyncExternalStore(subscribeSettings,currentLanguage,currentLanguage)}
export function useT():Translate{
  const lang=useLanguage()
  return (key,vars)=>translate(lang,key,vars)
}
// Bileşen dışından (bildirim metinleri gibi) çeviri gerektiğinde.
export function t(key:string,vars?:Record<string,string|number>){return translate(currentLanguage(),key,vars)}

// ---- bölge biçimlendirme ----
export function activeLocale(){return languageOf(currentLanguage()).locale}
export function activeTimeZone(){
  const settings=getSettings()
  if(settings.autoTimeZone||!settings.timeZone)return undefined
  return settings.timeZone
}
export function timeOptions():Intl.DateTimeFormatOptions{
  return {hour:'2-digit',minute:'2-digit',hour12:!getSettings().hour24,timeZone:activeTimeZone()}
}
export function formatTime(value:Date|string|number){
  return new Date(value).toLocaleTimeString(activeLocale(),timeOptions())
}
export function formatDate(value:Date|string|number,options:Intl.DateTimeFormatOptions={day:'numeric',month:'long'}){
  return new Date(value).toLocaleDateString(activeLocale(),{...options,timeZone:activeTimeZone()})
}
export function formatDateTime(value:Date|string|number){
  return new Date(value).toLocaleString(activeLocale(),{day:'numeric',month:'long',...timeOptions()})
}
// Ortamda desteklenen saat dilimleri; eski motorlarda kısa bir liste kullanılır.
export function timeZones(){
  const supported=(Intl as any).supportedValuesOf
  try{if(supported)return supported.call(Intl,'timeZone') as string[]}catch{}
  return ['Europe/Istanbul','Europe/London','Europe/Berlin','Europe/Madrid','America/New_York','America/Chicago','America/Los_Angeles','America/Sao_Paulo','Asia/Tokyo','Asia/Seoul','Asia/Shanghai','Asia/Taipei','Australia/Sydney','UTC']
}
export function systemTimeZone(){try{return Intl.DateTimeFormat().resolvedOptions().timeZone||'UTC'}catch{return 'UTC'}}
// "Europe/Istanbul (UTC+03:00)" gibi okunur etiket.
export function zoneLabel(zone:string){
  try{
    const parts=new Intl.DateTimeFormat('en-US',{timeZone:zone,timeZoneName:'longOffset'}).formatToParts(new Date())
    const offset=parts.find(part=>part.type==='timeZoneName')?.value||''
    return `${zone.replace(/_/g,' ')}${offset?` (${offset})`:''}`
  }catch{return zone}
}
