import { useEffect, useState } from 'react'
import { FRAME_RATES, QUALITIES, type QualityId, type ShareOptions } from './screenShare'
import { getSettings, updateSettings } from './settings'
import { useEscape } from './useEscape'

type Source={id:string;name:string;kind:'screen'|'window';thumbnail:string;icon:string}

// Yayın başlamadan önce kaynak, çözünürlük, kare hızı ve bilgisayar sesi seçilir.
export default function ScreenShareModal({onClose,onStart}:{onClose:()=>void;onStart:(options:ShareOptions)=>void}){
  const picker=Boolean(window.p2?.screen)
  const remembered=getSettings()
  const[sources,setSources]=useState<Source[]>([])
  const[loading,setLoading]=useState(picker)
  const[selected,setSelected]=useState('')
  const[kind,setKind]=useState<'screen'|'window'>('screen')
  const[level,setLevel]=useState<QualityId>(remembered.shareQuality)
  const[fps,setFps]=useState(remembered.shareFps)
  const[audio,setAudio]=useState(remembered.shareAudio)
  useEscape(true,onClose)

  useEffect(()=>{
    if(!picker)return
    let alive=true
    window.p2!.screen!.sources().then(items=>{
      if(!alive)return
      setSources(items)
      setSelected(current=>current||items.find(item=>item.kind==='screen')?.id||items[0]?.id||'')
      setLoading(false)
    }).catch(()=>{if(alive)setLoading(false)})
    return()=>{alive=false}
  },[picker])

  const visible=sources.filter(source=>source.kind===kind)
  function start(){
    updateSettings({shareQuality:level,shareFps:fps,shareAudio:audio})
    onStart({sourceId:selected,quality:level,fps,audio})
  }

  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onClose()}}>
    <section className="share-modal">
      <header>
        <div><b>Ekran paylaş</b><small>{picker?'Paylaşmak istediğin ekranı ya da pencereyi seç':'Tarayıcı kendi seçim penceresini gösterecek'}</small></div>
        <button type="button" onClick={onClose} aria-label="Kapat">×</button>
      </header>

      {picker&&<>
        <div className="share-tabs">
          <button type="button" className={kind==='screen'?'active':undefined} onClick={()=>setKind('screen')}>Ekranlar</button>
          <button type="button" className={kind==='window'?'active':undefined} onClick={()=>setKind('window')}>Pencereler</button>
        </div>
        <div className="share-sources">
          {loading&&<p className="share-empty">Kaynaklar okunuyor…</p>}
          {!loading&&!visible.length&&<p className="share-empty">{kind==='screen'?'Ekran bulunamadı.':'Paylaşılabilir pencere yok.'}</p>}
          {visible.map(source=><button type="button" key={source.id} className={selected===source.id?'share-source active':'share-source'} onClick={()=>setSelected(source.id)} title={source.name}>
            {source.thumbnail?<img src={source.thumbnail} alt=""/>:<span className="share-noshot"/>}
            <span className="share-source-name">{source.icon&&<img className="share-icon" src={source.icon} alt=""/>}{source.name}</span>
          </button>)}
        </div>
      </>}

      <div className="share-options">
        <label>Çözünürlük<select value={level} onChange={event=>setLevel(event.target.value as QualityId)}>
          {QUALITIES.map(item=><option key={item.id} value={item.id}>{item.label}</option>)}
        </select></label>
        <label>Kare hızı<select value={fps} onChange={event=>setFps(Number(event.target.value))}>
          {FRAME_RATES.map(rate=><option key={rate} value={rate}>{rate} FPS</option>)}
        </select></label>
        <label className="share-check"><input type="checkbox" checked={audio} onChange={event=>setAudio(event.target.checked)}/>Bilgisayar sesini paylaş</label>
      </div>
      <p className="share-hint">Yüksek çözünürlük ve kare hızı daha fazla bant genişliği ister. İzleyenler kendi tarafında kaliteyi düşürebilir.{audio&&' Bilgisayar sesi doğrudan hoparlör çıkışından alınır; aramadaki kişilerin sesi yayına karışmaz.'}</p>

      <footer>
        <button type="button" className="share-cancel" onClick={onClose}>Vazgeç</button>
        <button type="button" className="share-start" onClick={start} disabled={picker&&!selected}>Paylaşımı başlat</button>
      </footer>
    </section>
  </div>
}
