import type { ButtonHTMLAttributes, ReactNode } from 'react'

// Düğme: .p2-btn sınıfının React sarmalayıcısı; yalnız "loading" durumu için var, düz sınıf da kullanılabilir.
export default function Button({variant='secondary',size,icon,loading,className='',children,disabled,type='button',...rest}:ButtonHTMLAttributes<HTMLButtonElement>&{
  variant?:'primary'|'secondary'|'ghost'|'danger'
  size?:'sm'|'lg'
  icon?:ReactNode
  loading?:boolean
}){
  return <button type={type} className={`p2-btn ${variant}${size?' '+size:''}${loading?' is-busy':''}${className?' '+className:''}`} disabled={disabled||loading} {...rest}>
    {icon}{children}
  </button>
}
