import { useEffect, useState } from 'react'
import { updateSettings, useSettings } from '../../settings'
import { bridge, MOCK, mockPerf } from '../overlayBridge'
import { usePing } from '../usePing'
import type { OverlayPerf, OverlayToolProps } from '../overlayTypes'

// Performans penceresi: FPS (DLL Present sayacı, host 1 Hz) + ping (aramada WebRTC RTT, değilse sunucu /health).
// Köşedeki küçük HUD buradan açılıp kapatılır. ≤1 Hz güncellenir (her çizim tüm overlay yüzeyinin paint'idir).
export default function PerfWindow(props:OverlayToolProps){
  const settings=useSettings()
  const[perf,setPerf]=useState<OverlayPerf|null>(null)
  const[history,setHistory]=useState<number[]>([])
  const healthPing=usePing(!MOCK)
  useEffect(()=>{props.setTitle?.('Performans')},[])
  useEffect(()=>{
    let last=0
    const off=bridge.onPerf(next=>{const now=Date.now();if(now-last<900)return;last=now;setPerf(next);setHistory(list=>[...list.slice(-59),next.fps])})
    let timer=0
    if(MOCK){let tick=0;timer=window.setInterval(()=>{tick++;mockPerf({fps:138+((tick*7)%9),ping:31+((tick*3)%6)})},1000)}
    return()=>{off();if(timer)window.clearInterval(timer)}
  },[])
  const fps=perf&&Number.isFinite(perf.fps)?Math.round(perf.fps):null
  const ping=perf&&perf.ping!=null&&Number.isFinite(perf.ping)?Math.round(perf.ping):healthPing
  const avg=history.length?Math.round(history.reduce((sum,value)=>sum+value,0)/history.length):null
  const min=history.length?Math.min(...history):null
  if(props.pinned&&props.minimized)return <div className="overlay-perf-pill"><b>{fps??'—'}</b><small>FPS</small><i/><b>{ping??'—'}</b><small>ms</small></div>
  return <div className="overlay-perf">
    <div className="overlay-perf-grid">
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">FPS</span><b className={fps!=null&&fps<30?'low':''}>{fps??'—'}</b><small>{avg!=null?`ort. ${avg} · en düşük ${min}`:'oyun kare çizince başlar'}</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">Ping</span><b className={ping!=null&&ping>150?'low':''}>{ping??'—'}<small> ms</small></b><small>{perf?.ping!=null?'sesli arama (WebRTC)':'P2 sunucusu'}</small></div>
    </div>
    <div className="overlay-perf-grid small">
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">CPU</span><b className={perf?.cpu!=null&&perf.cpu>90?'low':''}>{perf?.cpu??'—'}<small> %</small></b><small>tüm çekirdekler</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">RAM</span><b>{perf?.ram?Math.round(perf.ram.used/1024*10)/10:'—'}<small> GB</small></b><small>{perf?.ram?`${Math.round(perf.ram.total/1024)} GB toplam · %${Math.round(100*perf.ram.used/perf.ram.total)}`:'—'}</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">GPU</span><b className={perf?.gpu&&perf.gpu.temp>85?'low':''}>{perf?.gpu?perf.gpu.util:'—'}<small> %</small></b><small>{perf?.gpu?`${perf.gpu.temp}°C · ${Math.round(perf.gpu.memUsed/1024*10)/10}/${Math.round(perf.gpu.memTotal/1024)} GB bellek`:'NVIDIA sürücüsü (nvidia-smi) bulunamadı'}</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">Paket kaybı</span><b className={perf?.loss!=null&&perf.loss>5?'low':''}>{perf?.loss!=null?Math.round(perf.loss*10)/10:'—'}<small> %</small></b><small>{perf?.loss!=null?'sesli arama':'aramada ölçülür'}</small></div>
    </div>
    <div className="overlay-perf-bars" aria-hidden="true">{history.map((value,index)=><i key={index} style={{height:`${Math.max(4,Math.min(100,value/2))}%`}}/>)}</div>
    {!props.pinned&&<div className="overlay-perf-actions">
      <label className="p2-row"><span>Köşede küçük gösterge (HUD)</span><button type="button" role="switch" aria-checked={settings.overlayHud} className={`p2-toggle${settings.overlayHud?' on':''}`} onClick={()=>updateSettings({overlayHud:!settings.overlayHud})}><i/></button></label>
      <label className="p2-row"><span>Animasyonları azalt</span><button type="button" role="switch" aria-checked={settings.overlayReduceMotion} className={`p2-toggle${settings.overlayReduceMotion?' on':''}`} onClick={()=>updateSettings({overlayReduceMotion:!settings.overlayReduceMotion})}><i/></button></label>
      <small className="overlay-perf-note">FPS oyunun gerçek Present sayısıdır (overlay DLL'i sayar). Overlay yüzeyi oyunun her karesine binmez; yalnız arayüz değişince yeniden çizilir.</small>
    </div>}
  </div>
}
