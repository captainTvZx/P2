import { useEffect, useState } from 'react'
import { formatHotkeyKeys, hotkeyFromEvent } from '../hotkeys'

// Tek kısayol satırı: etiket + kbd çipleri + "Tuşu değiştir". Dinlerken ilk tuş vuruşu yakalanır;
// Esc vazgeçer, Backspace (allowEmpty ise) temizler. Yalnız değiştirici basılırsa dinleme sürer.
export default function HotkeyInput({value,onChange,label,allowEmpty,conflict}:{value:string;onChange:(next:string)=>void;label:string;allowEmpty?:boolean;conflict?:string}){
  const[listening,setListening]=useState(false)
  useEffect(()=>{
    if(!listening)return
    const capture=(event:globalThis.KeyboardEvent)=>{
      event.preventDefault()
      event.stopPropagation()
      if(event.code==='Escape'||event.key==='Escape'){setListening(false);return}
      if(event.code==='Backspace'||event.key==='Backspace'){if(allowEmpty)onChange('');setListening(false);return}
      const next=hotkeyFromEvent(event)
      if(!next)return // yalnız değiştirici ya da yasak tuş: beklemeye devam
      onChange(next)
      setListening(false)
    }
    const cancel=()=>setListening(false)
    window.addEventListener('keydown',capture,true)
    window.addEventListener('blur',cancel)
    return()=>{window.removeEventListener('keydown',capture,true);window.removeEventListener('blur',cancel)}
  },[listening,allowEmpty,onChange])
  const keys=formatHotkeyKeys(value)
  return <div className={`hotkey-input${listening?' listening':''}${conflict?' conflict':''}`}>
    <span className="hotkey-label">{label}</span>
    <span className="hotkey-value">
      {listening
        ?<em>Bir tuşa bas… (Esc vazgeç{allowEmpty?', Backspace temizle':''})</em>
        :keys.length
          ?<span className="p2-keys">{keys.map((key,index)=><span key={index}>{index>0&&<i>+</i>}<kbd>{key}</kbd></span>)}</span>
          :<em className="hotkey-off">Kapalı</em>}
    </span>
    <button type="button" className="p2-btn secondary sm" aria-pressed={listening} onClick={()=>setListening(value=>!value)}>{listening?'Bekleniyor':'Tuşu değiştir'}</button>
    {conflict&&!listening&&<small className="hotkey-conflict">Bu tuş zaten '{conflict}' için kullanılıyor</small>}
  </div>
}
