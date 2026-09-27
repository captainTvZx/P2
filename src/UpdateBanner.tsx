import { useEffect, useState } from 'react'

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
    return stop
  },[apiBase,demoState])
  // Açılış turu: ayrı pencere yok, uygulamanın kendi açılış ekranı ilerlemeyi gösterir.
  if(state?.startup&&state.status!=='idle')return <main className="splash update-splash">
    <span className="logo">P2</span>
    <p>{state.text||'Güncellemeler denetleniyor…'}{state.status==='downloading'&&state.percent>0?` · %${state.percent}`:''}</p>
    <div className={state.status==='downloading'&&state.percent>0?'splash-track':'splash-track waiting'}><i style={state.status==='downloading'&&state.percent>0?{width:`${state.percent}%`}:undefined}/></div>
  </main>
  if(!state||hidden)return null
  if(state.status!=='downloading'&&state.status!=='ready')return null
  // Sürüm hazır: ekranın ortasında küçük bir onay penceresi. (Şerit denendi,
  // uzun metinle ekranı kaplıyordu ve kullanıcı için de onay hissi vermiyordu.)
  if(state.status==='ready')return <div className="update-layer" role="dialog" aria-modal="true" aria-label="Yeni sürüm hazır">
    <section className="update-modal">
      <span className="update-modal-mark">P2</span>
      <b>Yeni sürüm hazır{state.version?` · ${state.version}`:''}</b>
      <p>Kurulum sessizce yapılır ve uygulama kendiliğinden yeniden açılır. “Sonra” dersen güncelleme, uygulamayı kapattığında kurulur.</p>
      <footer>
        <button type="button" className="update-later" onClick={()=>setHidden(true)}>Sonra</button>
        <button type="button" className="update-primary" disabled={installing} onClick={async()=>{setInstalling(true);await window.p2?.updates?.install()}}>{installing?'Kuruluyor…':'Şimdi yeniden başlat'}</button>
      </footer>
    </section>
  </div>
  // İndirme sürerken yalnızca üstte ince bir şerit.
  return <div className="update-banner" role="status">
    <b>Yeni sürüm indiriliyor{state.version?` · ${state.version}`:''}</b>
    <span className="update-track"><i style={{width:`${Math.max(4,state.percent)}%`}}/></span>
    <small>%{state.percent}</small>
  </div>
}
