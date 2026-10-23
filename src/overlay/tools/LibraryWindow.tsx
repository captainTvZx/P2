import { useEffect, useState, type CSSProperties } from 'react'
import { channelLabel, formatDuration, useLibraryStore } from '../../library'
import { overlaySessionInfo, useOverlayAuth } from '../../overlaySession'
import { updateSettings, useSettings } from '../../settings'
import { bridge } from '../overlayBridge'
import type { OverlayToastCorner, OverlayToolProps } from '../overlayTypes'
import './library-tool.css'

// Kütüphane hızlı menüsü (overlay): oyunu kapat / klasörü aç / ekran görüntüsü al + overlay ayarları.
// Ayarlar `updateSettings` ile paylaşılan localStorage'a yazılır; ana pencere 'storage' olayıyla aynı anda görür.
// Pencere sabitlenemez (toolMeta pinnable:false); yine de sabitli gelirse hiçbir şey çizilmez.

const CORNERS:{value:OverlayToastCorner;label:string}[]=[
  {value:'top-right',label:'Sağ üst'},
  {value:'top-left',label:'Sol üst'},
  {value:'bottom-right',label:'Sağ alt'},
  {value:'bottom-left',label:'Sol alt'},
]

function Slider({label,value,min,max,step=1,unit='%',onChange}:{label:string;value:number;min:number;max:number;step?:number;unit?:string;onChange(value:number):void}){
  const percent=Math.round(((value-min)/(max-min))*100)
  return <label className="lt-row slider">
    <span><b>{label}</b><em>{value}{unit}</em></span>
    <input type="range" className="p2-slider" min={min} max={max} step={step} value={value} style={{'--value':`${percent}%`} as CSSProperties} aria-label={label} onChange={event=>onChange(Number(event.target.value))}/>
  </label>
}

function Toggle({label,hint,checked,onChange}:{label:string;hint?:string;checked:boolean;onChange(value:boolean):void}){
  return <div className="lt-row toggle" role="switch" aria-checked={checked} tabIndex={0} onClick={()=>onChange(!checked)} onKeyDown={event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();onChange(!checked)}}}>
    <span><b>{label}</b>{hint&&<small>{hint}</small>}</span>
    <span className="p2-toggle" aria-hidden="true" style={{display:'block'}}><i/></span>
  </div>
}

export default function LibraryWindow(props:OverlayToolProps){
  const auth=useOverlayAuth()
  const session=overlaySessionInfo()
  const projectId=props.projectId||session?.projectId||auth.selectedProjectId||''
  const store=useLibraryStore()
  const settings=useSettings()
  const[confirmStop,setConfirmStop]=useState(false)
  const[busy,setBusy]=useState<''|'stop'|'folder'|'shot'>('')
  const[error,setError]=useState('')
  useEffect(()=>{props.setTitle?.('Ayarlar')},[])
  useEffect(()=>{setConfirmStop(false);setError('')},[projectId])

  if(props.pinned)return null
  const projects=auth.projects as {id:string;name:string}[]
  const projectName=projects.find(project=>project.id===projectId)?.name||session?.projectName||'Proje'
  const installed=store.projects[projectId]
  const running=store.running.includes(projectId)
  const library=window.p2?.library
  const canUse=Boolean(library&&projectId)

  async function stopGame(){
    if(!library)return
    setBusy('stop');setError('')
    try{
      const result=await library.stop({projectId})
      if(!result.ok)setError(result.error||'Oyun kapatılamadı.')
    }catch{setError('Oyun kapatılamadı.')}
    finally{setBusy('');setConfirmStop(false)}
  }
  async function openFolder(){
    if(!library)return
    setBusy('folder');setError('')
    try{const result=await library.openFolder({projectId,projectName});if(!result.ok)setError('Klasör açılamadı; oyun kurulu değil olabilir.')}
    catch{setError('Klasör açılamadı.')}
    finally{setBusy('')}
  }
  async function screenshot(){
    setBusy('shot');setError('')
    try{const result=await bridge.screenshot();if(!result.ok)setError(result.error||'Ekran görüntüsü alınamadı.')}
    catch{setError('Ekran görüntüsü alınamadı.')}
    finally{setBusy('')}
  }

  const opacity=Math.round(settings.overlayOpacity*100)
  const scale=Math.round(settings.overlayScale*100)

  return <div className="lt" aria-label="Kütüphane">
    <section className="lt-section">
      <h4>Oyun</h4>
      <div className="lt-game">
        <div className="lt-game-main">
          <div className="lt-game-title" title={projectName}>{projectName}</div>
          <div className="lt-game-meta">
            {installed?<><code>v{installed.version}</code><span>{channelLabel(installed.channel)}</span>{installed.playSeconds?<span>· {formatDuration(installed.playSeconds)}</span>:null}</>:<span>Kurulu değil</span>}
          </div>
        </div>
        <span className={`lt-state${running?' on':''}`}><i/>{running?'Çalışıyor':'Kapalı'}</span>
      </div>
      {confirmStop
        ?<div className="lt-confirm" role="alertdialog" aria-label="Oyunu kapat">
          <span>Oyun kapatılsın mı? Kaydedilmemiş ilerleme kaybolabilir.</span>
          <button type="button" className="p2-btn sm danger" disabled={busy==='stop'} onClick={()=>void stopGame()}>{busy==='stop'?'Kapatılıyor…':'Kapat'}</button>
          <button type="button" className="p2-btn sm ghost" onClick={()=>setConfirmStop(false)}>Vazgeç</button>
        </div>
        :<div className="lt-actions">
          <button type="button" className="p2-btn secondary" disabled={!canUse||!running||busy!==''} title={running?'Oyun sürecini sonlandır':'Oyun şu an çalışmıyor'} onClick={()=>setConfirmStop(true)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5v8"/><path d="M7 6.5a8 8 0 1 0 10 0"/></svg>Oyunu kapat
          </button>
          <button type="button" className="p2-btn secondary" disabled={!canUse||!installed||busy!==''} title="Kurulum klasörünü dosya yöneticisinde aç" onClick={()=>void openFolder()}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 6.5A1.5 1.5 0 0 1 5 5h4.5l2 2H19a1.5 1.5 0 0 1 1.5 1.5v9A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z"/></svg>Klasörü aç
          </button>
          <button type="button" className="p2-btn secondary" disabled={busy!==''} title="Oyunun ekran görüntüsünü al" onClick={()=>void screenshot()}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8.5A1.5 1.5 0 0 1 5.5 7H8l1.4-2h5.2L16 7h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="13" r="3.4"/></svg>{busy==='shot'?'Alınıyor…':'Ekran görüntüsü al'}
          </button>
        </div>}
      {error&&<div className="lt-error" role="alert">{error}</div>}
    </section>

    <section className="lt-section">
      <h4>Overlay</h4>
      <div className="lt-rows">
        <Slider label="Pencere saydamlığı" value={opacity} min={35} max={100} onChange={value=>updateSettings({overlayOpacity:value/100})}/>
        <label className="lt-row">
          <span><b>Bildirim köşesi</b><small>Oyun içi bildirim kartlarının yeri</small></span>
          <select className="p2-select" value={settings.overlayToastCorner} onChange={event=>updateSettings({overlayToastCorner:event.target.value as OverlayToastCorner})}>
            {CORNERS.map(corner=><option key={corner.value} value={corner.value}>{corner.label}</option>)}
          </select>
        </label>
        <Toggle label="FPS ve ping göstergesi" hint="Köşede kare hızı ve gecikme" checked={settings.overlayHud} onChange={overlayHud=>updateSettings({overlayHud})}/>
        <Toggle label="Animasyonları azalt" hint="Geçiş ve hareket efektlerini kapatır" checked={settings.overlayReduceMotion} onChange={overlayReduceMotion=>updateSettings({overlayReduceMotion})}/>
        <Slider label="Arayüz ölçeği" value={scale} min={80} max={150} step={5} onChange={value=>updateSettings({overlayScale:value/100})}/>
      </div>
      <div className="lt-foot">
        <button type="button" className="p2-btn sm ghost" onClick={()=>bridge.openInMain({kind:'settings',section:'library'})}>Tüm ayarlar</button>
      </div>
    </section>
  </div>
}
