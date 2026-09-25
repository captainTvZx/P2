import { useEffect, useState } from 'react'

// Yeni sürüm şeridi. Sessizce indirilir; hazır olunca üstte küçük bir şerit
// çıkar. "Sonra" denirse kapanır ve kurulum uygulama kapanırken yapılır.
export default function UpdateBanner({apiBase}:{apiBase:string}){
  const[state,setState]=useState<UpdateState|null>(null)
  const[hidden,setHidden]=useState(false)
  const[installing,setInstalling]=useState(false)
  useEffect(()=>{
    const bridge=window.p2?.updates
    if(!bridge)return
    void bridge.state().then(setState)
    const stop=bridge.onState(value=>{setState(value);if(value.status==='downloading'||value.status==='ready')setHidden(false)})
    void bridge.configure(apiBase)
    return stop
  },[apiBase])
  if(!state||hidden)return null
  if(state.status!=='downloading'&&state.status!=='ready')return null
  const ready=state.status==='ready'
  return <div className={ready?'update-banner ready':'update-banner'} role="status">
    {ready?<>
      <b>Yeni sürüm hazır{state.version?` · ${state.version}`:''}</b>
      <span>Yeniden başlatınca kurulur; istersen uygulamayı kapattığında da kurulur.</span>
      <button type="button" className="update-primary" disabled={installing} onClick={async()=>{setInstalling(true);await window.p2?.updates?.install()}}>{installing?'Kuruluyor…':'Yeniden başlat'}</button>
      <button type="button" className="update-later" onClick={()=>setHidden(true)}>Sonra</button>
    </>:<>
      <b>Yeni sürüm indiriliyor{state.version?` · ${state.version}`:''}</b>
      <span className="update-track"><i style={{width:`${Math.max(4,state.percent)}%`}}/></span>
      <small>%{state.percent}</small>
    </>}
  </div>
}
