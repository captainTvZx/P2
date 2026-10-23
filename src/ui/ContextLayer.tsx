import type { HTMLAttributes, ReactNode } from 'react'
import { createPortal } from 'react-dom'

// Sağ tık / ••• menülerinin tam ekran katmanı. body'ye portal'lanır: kenar çubuğu gibi animasyonlu ya da
// kaydırmalı bir atanın stacking context'i / containing block'u menüyü arkada bırakamaz.
export default function ContextLayer({children,className,...rest}:{children:ReactNode}&HTMLAttributes<HTMLDivElement>){
  if(typeof document==='undefined')return null
  return createPortal(<div className={className?`channel-context-layer ${className}`:'channel-context-layer'} {...rest}>{children}</div>,document.body)
}
