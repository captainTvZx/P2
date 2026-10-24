import { useEffect } from 'react'
import { updateSettings, useSettings } from '../../settings'
import { MOCK } from '../overlayBridge'
import { PERF_HISTORY, perfAverage, perfMin, startMockPerf, stopMockPerf, usePerf } from '../perfStore'
import { usePingStats } from '../usePing'
import type { OverlayHudPosition, OverlayToolProps } from '../overlayTypes'

// Performans penceresi: FPS (DLL Present sayacı, host 1 Hz) + ping (aramada WebRTC RTT, değilse sunucu /health),
// oyun sürecinin CPU/RAM'i (host eklentisi), sistem CPU/RAM, GPU (nvidia-smi), paket kaybı (aramada WebRTC; değilse
// P2 sunucusu erişim kaybı). Geçmiş perfStore'da tutulur: pencere kapanıp açılınca grafik sıfırlanmaz.
// Köşedeki küçük HUD buradan açılıp kapatılır, yeri ve ayrıntı düzeyi seçilir. ≤1 Hz güncellenir.
const POSITIONS:{value:OverlayHudPosition;label:string}[]=[{value:'top-left',label:'Sol üst'},{value:'top-center',label:'Üst orta'},{value:'top-right',label:'Sağ üst'}]
const gb=(mb:number)=>Math.round(mb/1024*10)/10

export default function PerfWindow(props:OverlayToolProps){
  const settings=useSettings()
  const{perf,history}=usePerf()
  const net=usePingStats(!MOCK)
  const ghost=!props.visible
  useEffect(()=>{props.setTitle?.('Performans')},[])
  useEffect(()=>{if(!MOCK)return;startMockPerf();return stopMockPerf},[])
  const fps=perf&&Number.isFinite(perf.fps)?Math.round(perf.fps):null
  const ping=perf&&perf.ping!=null&&Number.isFinite(perf.ping)?Math.round(perf.ping):net.ping
  const avg=perfAverage(history),min=perfMin(history)
  const loss=perf?.loss!=null?Math.round(perf.loss*10)/10:net.loss
  const gameCpu=perf?.game?.cpu??null,gameRam=perf?.game?.ramMb??null
  const peak=Math.max(60,...history)
  if(props.pinned&&props.minimized)return <div className="overlay-perf-pill"><b>{fps??'—'}</b><small>FPS</small><i/><b>{ping??'—'}</b><small>ms</small>{gameCpu!=null&&<><i/><b>{gameCpu}</b><small>CPU</small></>}</div>
  return <div className="overlay-perf">
    <div className="overlay-perf-grid">
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">FPS</span><b className={fps!=null&&fps<30?'low':''}>{fps??'—'}</b><small>{avg!=null?`ort. ${avg} · en düşük ${min}`:'oyun kare çizince başlar'}</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">Ping</span><b className={ping!=null&&ping>150?'low':''}>{ping??'—'}<small> ms</small></b><small>{perf?.ping!=null?'sesli arama (WebRTC)':'P2 sunucusu'}</small></div>
    </div>
    <div className="overlay-perf-grid small">
      <div className="p2-card overlay-perf-stat" title="Oyun sürecinin CPU kullanımı (tüm çekirdeklere göre)"><span className="p2-kicker">Oyun CPU</span><b className={gameCpu!=null&&gameCpu>90?'low':''}>{gameCpu??'—'}<small> %</small></b><small>{perf?.cpu!=null?`sistem %${perf.cpu}`:'ölçüm bekleniyor'}</small></div>
      <div className="p2-card overlay-perf-stat" title="Oyun sürecinin çalışma belleği"><span className="p2-kicker">Oyun RAM</span><b>{gameRam!=null?(gameRam>=1024?gb(gameRam):gameRam):'—'}<small> {gameRam!=null&&gameRam<1024?'MB':'GB'}</small></b><small>{perf?.ram?`sistem ${gb(perf.ram.used)}/${Math.round(perf.ram.total/1024)} GB`:'—'}</small></div>
      <div className="p2-card overlay-perf-stat"><span className="p2-kicker">GPU</span><b className={perf?.gpu&&perf.gpu.temp>85?'low':''}>{perf?.gpu?perf.gpu.util:'—'}<small> %</small></b><small>{perf?.gpu?`${perf.gpu.temp}°C · ${gb(perf.gpu.memUsed)}/${Math.round(perf.gpu.memTotal/1024)} GB`:'NVIDIA sürücüsü (nvidia-smi) bulunamadı'}</small></div>
      <div className="p2-card overlay-perf-stat" title={perf?.loss!=null?'Sesli aramada WebRTC paket kaybı':'P2 sunucusuna yoklama kaybı (oyunun kendi trafiği ölçülemez)'}><span className="p2-kicker">Paket kaybı</span><b className={loss!=null&&loss>5?'low':''}>{loss??'—'}<small> %</small></b><small>{perf?.loss!=null?'sesli arama':net.samples>=3?'P2 sunucusu':'ölçülüyor…'}</small></div>
    </div>
    <div className="overlay-perf-chart-wrap" aria-hidden="true" title={`Son ${history.length} sn FPS`}>
      <svg className="overlay-perf-chart" viewBox={`0 0 ${PERF_HISTORY} 40`} preserveAspectRatio="none">
        {history.length>1&&(()=>{const offset=PERF_HISTORY-history.length;const points=history.map((value,index)=>`${offset+index},${(40-Math.round(value/peak*36)).toFixed(1)}`);return <><path d={`M${offset},40 L${points.join(' L')} L${PERF_HISTORY-1},40 Z`}/><polyline points={points.join(' ')}/></>})()}
      </svg>
      <span className="overlay-perf-chart-label">FPS · son {history.length||0} sn</span>
      {history.length>0&&<span className="overlay-perf-chart-peak">tepe {peak}</span>}
    </div>
    {!ghost&&<div className="overlay-perf-actions">
      <label className="p2-row"><span>Köşede küçük gösterge (HUD)</span><button type="button" role="switch" aria-checked={settings.overlayHud} className={`p2-toggle${settings.overlayHud?' on':''}`} onClick={()=>updateSettings({overlayHud:!settings.overlayHud})}><i/></button></label>
      <label className="p2-row"><span>Gösterge yeri</span><select className="p2-select" value={settings.overlayHudPosition||'top-left'} aria-label="Gösterge yeri" onChange={event=>updateSettings({overlayHudPosition:event.target.value as OverlayHudPosition})}>{POSITIONS.map(item=><option key={item.value} value={item.value}>{item.label}</option>)}</select></label>
      <label className="p2-row"><span>Ayrıntılı gösterge (CPU · RAM · GPU · kayıp)</span><button type="button" role="switch" aria-checked={settings.overlayHudDetails} className={`p2-toggle${settings.overlayHudDetails?' on':''}`} onClick={()=>updateSettings({overlayHudDetails:!settings.overlayHudDetails})}><i/></button></label>
      <label className="p2-row"><span>Animasyonları azalt</span><button type="button" role="switch" aria-checked={settings.overlayReduceMotion} className={`p2-toggle${settings.overlayReduceMotion?' on':''}`} onClick={()=>updateSettings({overlayReduceMotion:!settings.overlayReduceMotion})}><i/></button></label>
      <small className="overlay-perf-note">FPS oyunun gerçek Present sayısıdır (overlay DLL'i sayar). CPU/RAM yalnız oyun sürecinindir; paket kaybı aramada WebRTC'den, değilse P2 sunucusuna yoklamalardan gelir.</small>
    </div>}
  </div>
}
