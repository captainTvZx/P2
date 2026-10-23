import { useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { useEscape } from '../useEscape'

// Tek pencere kalıbı: arka plan katmanı + elastik giren kart. Esc ve arka plana tık kapatır;
// kapanış önce çıkış animasyonunu oynatır (.leaving), sonra onClose çağrılır.
type Size='sm'|'md'|'lg'|'xl'
export default function Modal({onClose,size='md',kicker,title,subtitle,footer,className='',layerClassName='',dismissOnBackdrop=true,closeLabel='Kapat',children}:{
  onClose:()=>void
  size?:Size
  kicker?:ReactNode
  title?:ReactNode
  subtitle?:ReactNode
  footer?:ReactNode
  className?:string
  layerClassName?:string
  dismissOnBackdrop?:boolean
  closeLabel?:string
  children:ReactNode
}){
  const [leaving,setLeaving]=useState(false)
  const close=()=>{
    if(leaving)return
    if(window.matchMedia?.('(prefers-reduced-motion: reduce)').matches){onClose();return}
    setLeaving(true)
    window.setTimeout(onClose,140)
  }
  useEscape(true,close)
  return createPortal(
    <div className={`p2-modal-layer${leaving?' leaving':''}${layerClassName?' '+layerClassName:''}`} onMouseDown={event=>{if(dismissOnBackdrop&&event.target===event.currentTarget)close()}}>
      <section role="dialog" aria-modal="true" className={`p2-modal ${size}${leaving?' leaving':''}${className?' '+className:''}`}>
        {(title||subtitle||kicker)&&<header className="p2-modal-head">
          <div>{kicker&&<span className="p2-kicker">{kicker}</span>}{title&&<b>{title}</b>}{subtitle&&<small>{subtitle}</small>}</div>
          <button type="button" className="p2-btn icon ghost sm" aria-label={closeLabel} onClick={close}>×</button>
        </header>}
        <div className="p2-modal-body">{children}</div>
        {footer&&<footer className="p2-modal-foot">{footer}</footer>}
      </section>
    </div>,
    document.body,
  )
}
