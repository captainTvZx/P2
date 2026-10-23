import { useEffect, useState } from 'react'
import type React from 'react'
import Modal from './ui/Modal'
import Logo from './ui/Logo'

// Yeni sürüm şeridi. Sessizce indirilir; hazır olunca üstte küçük bir şerit
// çıkar. "Sonra" denirse kapanır ve kurulum uygulama kapanırken yapılır.
// demoState: UI showroom'da gerçek güncelleyici olmadan ekranı göstermek için.
export default function UpdateBanner({apiBase,demoState}:{apiBase?:string;demoState?:UpdateState}){
  const[live,setLive]=useState<UpdateState|null>(null)
  const state=demoState||live
  const setState=setLive
  const[hidden,setHidden]=useState(false)
  const[installing,setInstalling]=useState(false)
  useEffect(()=>{
    const bridge=window.p2?.updates
    if(!bridge||demoState||!apiBase)return
    void bridge.state().then(setState)
    const stop=bridge.onState(value=>{setState(value);if(value.status==='downloading'||value.status==='ready')setHidden(false)})
    void bridge.configure(apiBase)
    // Sunucu "yeni sürüm yayınlandı" dediği anda denetim yapılır; ayrıca
    // pencereye geri dönüldüğünde (en fazla dakikada bir) tazelenir.
    let lastCheck=0
    const check=()=>{const now=Date.now();if(now-lastCheck<60000)return;lastCheck=now;void bridge.check()}
    const announced=()=>{lastCheck=0;check()}
    const focused=()=>{if(document.visibilityState==='visible')check()}
    window.addEventListener('p2:update-published',announced)
    window.addEventListener('focus',focused)
    document.addEventListener('visibilitychange',focused)
    return()=>{
      stop?.()
      window.removeEventListener('p2:update-published',announced)
      window.removeEventListener('focus',focused)
      document.removeEventListener('visibilitychange',focused)
    }
  },[apiBase,demoState])
  // Açılış turu: ayrı pencere yok, uygulamanın kendi açılış ekranı ilerlemeyi gösterir.
  if(state?.startup&&state.status!=='idle')return <main className="splash update-splash">
    <Logo size="xl" pop/>
    <p>{state.text||'Güncellemeler denetleniyor…'}{state.status==='downloading'&&state.percent>0?` · %${state.percent}`:''}</p>
    <div className={state.status==='downloading'&&state.percent>0?'p2-progress splash-track':'p2-progress splash-track indeterminate'}><i style={state.status==='downloading'&&state.percent>0?{'--value':`${state.percent}%`} as React.CSSProperties:undefined}/></div>
  </main>
  if(!state||hidden)return null
  if(state.status!=='downloading'&&state.status!=='ready')return null
  // Sürüm hazır: ekranın ortasında küçük bir onay penceresi. (Şerit denendi,
  // uzun metinle ekranı kaplıyordu ve kullanıcı için de onay hissi vermiyordu.)
  if(state.status==='ready')return <Modal size="sm" className="centered update-modal" layerClassName="update-layer" dismissOnBackdrop={false} onClose={()=>setHidden(true)} kicker="GÜNCELLEME" title={<>Yeni sürüm hazır{state.version?` · ${state.version}`:''}</>} footer={<>
        <button type="button" className="p2-btn ghost" onClick={()=>setHidden(true)}>Sonra</button>
        <button type="button" className={installing?'p2-btn primary is-busy':'p2-btn primary'} disabled={installing} onClick={async()=>{setInstalling(true);await window.p2?.updates?.install()}}>{installing?'Kuruluyor…':'Şimdi yeniden başlat'}</button>
      </>}>
      <Logo size="lg" pop className="update-modal-mark"/>
      <p>Kurulum sessizce yapılır ve uygulama kendiliğinden yeniden açılır. “Sonra” dersen güncelleme, uygulamayı kapattığında kurulur.</p>
    </Modal>
  // İndirme sürerken yalnızca üstte ince bir şerit.
  return <div className="update-banner" role="status">
    <b>Yeni sürüm indiriliyor{state.version?` · ${state.version}`:''}</b>
    <span className="p2-progress update-track"><i style={{'--value':`${Math.max(4,state.percent)}%`} as React.CSSProperties}/></span>
    <small>%{state.percent}</small>
  </div>
}
