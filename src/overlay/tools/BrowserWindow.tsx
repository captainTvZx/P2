import { useEffect, useRef, useState } from 'react'
import { getSettings, updateSettings, useSettings } from '../../settings'
import { bridge } from '../overlayBridge'
import { ComposerIcon } from '../../ComposerIcons'
import type { OverlayToolProps } from '../overlayTypes'

// Tarayıcı penceresi. Electron'un <webview> etiketi ekran dışı (OSR) pencerede yalnız küçük bir şerit
// çizip kaymıyor (prototip: scratchpad/webview-preview.cjs), bu yüzden sayfa bir <iframe> içinde açılır.
// Gömülmeye izin vermeyen siteler (X-Frame-Options) için "Dış tarayıcıda aç" düğmesi vardır.
// YouTube izleme sayfası, hafif gömülü oynatıcıya çevrilir (ambient mod / WebGPU efektleri yok; OSR'de çökmeye yol açıyordu).
function youtubeEmbed(value:string){
  const match=/^(?:https?:\/\/)?(?:www\.|m\.)?(?:youtube\.com\/(?:watch\?(?:.*&)?v=|shorts\/|live\/)|youtu\.be\/)([\w-]{6,})/i.exec(value)
  return match?`https://www.youtube-nocookie.com/embed/${match[1]}?rel=0`:''
}
function normalizeUrl(raw:string){
  const value=raw.trim()
  if(!value)return ''
  const embed=youtubeEmbed(value)
  if(embed)return embed
  if(/^[a-z]+:\/\//i.test(value))return value
  if(/^[\w.-]+\.[a-z]{2,}(\/.*)?$/i.test(value))return `https://${value}`
  return `https://www.google.com/search?q=${encodeURIComponent(value)}`
}

export default function BrowserWindow(props:OverlayToolProps){
  const settings=useSettings()
  const[url,setUrl]=useState(()=>settings.overlayBrowserHome||'https://www.google.com')
  const[input,setInput]=useState(url)
  const[history,setHistory]=useState<string[]>([url])
  const[index,setIndex]=useState(0)
  const[loading,setLoading]=useState(true)
  const[slow,setSlow]=useState(false)
  const frame=useRef<HTMLIFrameElement>(null)
  useEffect(()=>{props.setTitle?.('Tarayıcı')},[])
  // Sayfadaki yeni pencere istekleri (target=_blank) ana süreçten buraya yönlendirilir
  useEffect(()=>bridge.onEvent(event=>{if(event.type==='open-url'&&typeof event.url==='string')go(event.url)}),[history,index])
  // Yükleme 6 sn sürerse site büyük olasılıkla gömülmeyi engelliyor: ipucu göster.
  useEffect(()=>{setLoading(true);setSlow(false);const timer=window.setTimeout(()=>setSlow(true),6000);return()=>window.clearTimeout(timer)},[url])
  const go=(next:string,push=true)=>{
    const target=normalizeUrl(next)
    if(!target)return
    setUrl(target);setInput(target)
    if(push){const trimmed=history.slice(0,index+1);setHistory([...trimmed,target]);setIndex(trimmed.length)}
  }
  const back=()=>{if(index>0){const next=index-1;setIndex(next);setUrl(history[next]);setInput(history[next])}}
  const forward=()=>{if(index<history.length-1){const next=index+1;setIndex(next);setUrl(history[next]);setInput(history[next])}}
  const[reloadKey,setReloadKey]=useState(0)
  if(props.pinned&&props.minimized)return null
  return <div className="overlay-browser">
    {props.visible&&<form className="overlay-browser-bar" onSubmit={event=>{event.preventDefault();go(input)}}>
      <button type="button" className="p2-btn sm icon ghost" title="Geri" aria-label="Geri" disabled={index<=0} onClick={back}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M15 5l-7 7 7 7"/></svg></button>
      <button type="button" className="p2-btn sm icon ghost" title="İleri" aria-label="İleri" disabled={index>=history.length-1} onClick={forward}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M9 5l7 7-7 7"/></svg></button>
      <button type="button" className="p2-btn sm icon ghost" title="Yenile" aria-label="Yenile" onClick={()=>{setLoading(true);setReloadKey(value=>value+1)}}><ComposerIcon name="refresh"/></button>
      <button type="button" className="p2-btn sm icon ghost" title="Başlangıç sayfası" aria-label="Başlangıç sayfası" onClick={()=>go(settings.overlayBrowserHome||'https://www.google.com')}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round"><path d="M4 11.5 12 4.5l8 7"/><path d="M6.5 10v9.5h11V10"/><path d="M10 19.5v-5h4v5"/></svg></button>
      <input className="p2-input" value={input} onChange={event=>setInput(event.target.value)} onFocus={event=>event.target.select()} placeholder="Adres ya da arama" spellCheck={false}/>
      <button type="button" className="p2-btn sm icon ghost" title="Bu sayfayı başlangıç sayfası yap" aria-label="Bu sayfayı başlangıç sayfası yap" onClick={()=>{updateSettings({overlayBrowserHome:url})}}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round"><path d="M7 4.5h10a1 1 0 0 1 1 1V20l-6-3.5L6 20V5.5a1 1 0 0 1 1-1z"/></svg></button>
    </form>}
    <div className="overlay-browser-frame">
      {/* Video siteleri PresentationRequest/otomatik oynatma ister; sandbox'ta allow-presentation olmazsa sayfa hata fırlatıp OSR'de
          yüzeyi çökertebiliyor. Otomatik oynatma kapalı (allow listesinde yok): video yalnız kullanıcı tıklayınca başlar. */}
      <iframe ref={frame} key={`${url}#${reloadKey}`} src={url} title="Tarayıcı" sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-presentation" allow="fullscreen; picture-in-picture; clipboard-write" referrerPolicy="no-referrer-when-downgrade" onLoad={()=>{setLoading(false);setSlow(false)}}/>
      {loading&&slow&&<div className="overlay-browser-hint">
        <b>Sayfa gömülmeyi engelliyor olabilir</b>
        <small>Bazı siteler iframe içinde açılmaz. Başlangıç sayfasını kendi wiki/doküman adresin yapabilirsin.</small>
      </div>}
    </div>
  </div>
}
export function overlayBrowserHome(){return getSettings().overlayBrowserHome||'https://www.google.com'}
